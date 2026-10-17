use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use worker::d1::D1Type;
use worker::{Bucket, Conditional, D1Database, Error};

use crate::repository::{MutationResult, NewRevision, RepositoryError, RepositoryResult};

pub const V2_DOCUMENT_STAGE_MAX_BYTES: usize = 1_048_576;
const V2_DOCUMENT_STAGE_SLOT_LIMIT: i32 = 10;
const V2_DOCUMENT_STAGE_ATTEMPT_LIMIT: i32 = 10;
const V2_DOCUMENT_STAGE_TTL_SECONDS: i64 = 15 * 60;
const V2_DOCUMENT_STAGE_ATTEMPT_TTL_SECONDS: i64 = 2 * 60;
const V2_DOCUMENT_STAGE_ATTEMPT_GRACE_SECONDS: i64 = 2 * 60;
const V2_DOCUMENT_STAGE_TOMBSTONE_SECONDS: i64 = 24 * 60 * 60;
const R2_SEQUENCE_KEY: &str = "private/stage-control/sequence-v1";
const R2_STAGE_PREFIX: &str = "private/stage/";
const R2_SEQUENCE_RETRIES: usize = 16;
const R2_SWEEP_PAGE_SIZE: u32 = 100;
const MAX_REVISIONS_RETAINED: i32 = 100;
const MAX_V2_REVISION_ROW_BYTES: usize = 1_100_000;
const STAGE_MINT_COMMAND_ID: &str = "funnel.document.stage.v2";
const STAGE_CONTROL_FORMAT_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StageCapacity {
    pub orphan_byte_budget: u64,
    pub orphan_key_budget: u64,
}

impl StageCapacity {
    pub fn from_env(env: &worker::Env) -> Option<Self> {
        let orphan_byte_budget = env
            .var("V2_STAGE_ORPHAN_BYTE_BUDGET")
            .ok()?
            .to_string()
            .parse::<u64>()
            .ok()?;
        let orphan_key_budget = env
            .var("V2_STAGE_ORPHAN_KEY_BUDGET")
            .ok()?
            .to_string()
            .parse::<u64>()
            .ok()?;
        (orphan_byte_budget > 0 && orphan_key_budget > 0).then_some(Self {
            orphan_byte_budget,
            orphan_key_budget,
        })
    }
}

pub struct StageMintMutation<'a> {
    pub funnel_id: &'a str,
    pub base_revision_id: &'a str,
    pub expected_length: usize,
    pub expected_sha256: &'a str,
    pub command_id: &'a str,
    pub idempotency_key: &'a str,
    pub payload_json: &'a str,
}

pub struct StageFinalizeMutation<'a> {
    pub funnel_id: &'a str,
    pub base_revision_id: &'a str,
    pub stage_id: &'a str,
    pub expected_length: usize,
    pub expected_sha256: &'a str,
    pub command_id: &'a str,
    pub idempotency_key: &'a str,
    pub payload_json: &'a str,
    pub result_json: &'a str,
    pub revision: &'a NewRevision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StageCleanupProgress {
    pub listed: usize,
    pub deleted: usize,
    pub pass_completed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct StageCapacitySnapshot {
    pub orphan_byte_budget: u64,
    pub orphan_bytes_held: u64,
    pub orphan_key_budget: u64,
    pub orphan_keys_held: u64,
    pub worst_case_orphan_bytes: u64,
    pub worst_case_orphan_keys: u64,
}

pub struct DocumentStageRepository<'a> {
    database: &'a D1Database,
    bucket: &'a Bucket,
    capacity: Option<StageCapacity>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct StageControl {
    format_version: u8,
    sequence: String,
    orphan_byte_budget: u64,
    // Retained in the stored v1 control shape; live reservations are tracked in D1 now.
    #[serde(rename = "orphan_bytes_spent")]
    _legacy_orphan_bytes_spent: u64,
    orphan_key_budget: u64,
    #[serde(rename = "orphan_keys_spent")]
    _legacy_orphan_keys_spent: u64,
}

impl StageControl {
    fn initial(capacity: StageCapacity) -> Self {
        Self {
            format_version: STAGE_CONTROL_FORMAT_VERSION,
            sequence: "00000000000000000000".to_owned(),
            orphan_byte_budget: capacity.orphan_byte_budget,
            _legacy_orphan_bytes_spent: 0,
            orphan_key_budget: capacity.orphan_key_budget,
            _legacy_orphan_keys_spent: 0,
        }
    }

    fn validate(&self) -> bool {
        self.format_version == STAGE_CONTROL_FORMAT_VERSION
            && is_sequence(&self.sequence)
            && self.orphan_byte_budget > 0
            && self.orphan_key_budget > 0
    }

    fn increase_capacity(&mut self, capacity: StageCapacity) -> RepositoryResult<bool> {
        if capacity.orphan_byte_budget < self.orphan_byte_budget
            || capacity.orphan_key_budget < self.orphan_key_budget
        {
            return Err(RepositoryError::StageControlUnavailable);
        }
        let changed = capacity.orphan_byte_budget != self.orphan_byte_budget
            || capacity.orphan_key_budget != self.orphan_key_budget;
        self.orphan_byte_budget = capacity.orphan_byte_budget;
        self.orphan_key_budget = capacity.orphan_key_budget;
        Ok(changed)
    }

    fn allocate_sequence(&mut self) -> RepositoryResult<String> {
        let sequence =
            increment_sequence(&self.sequence).ok_or(RepositoryError::StageControlUnavailable)?;
        self.sequence = sequence.clone();
        Ok(sequence)
    }
}

#[derive(Deserialize)]
struct StageReceipt {
    command_id: String,
    payload_json: String,
    result_json: String,
}

#[derive(Deserialize)]
struct StageAdmissionState {
    funnel_exists: i64,
    base_matches: i64,
    active_stage_exists: i64,
    held_slots: i64,
}

#[derive(Deserialize)]
struct StageAttemptUsage {
    held_bytes: i64,
    held_keys: i64,
}

#[derive(Deserialize)]
struct StageUploadRecord {
    stage_id_hash: String,
    funnel_id: String,
    base_revision_id: String,
    purpose: String,
    document_type: String,
    document_version: i64,
    schema_version: i64,
    manifest_version: i64,
    maximum_bytes: i64,
    expected_length: i64,
    expected_sha256: String,
    expires_at: i64,
    state: String,
    object_key: Option<String>,
    uploaded_length: Option<i64>,
    uploaded_sha256: Option<String>,
}

#[derive(Deserialize)]
struct StageAttemptRecord {
    state: String,
    object_key: Option<String>,
}

#[derive(Deserialize)]
struct SweepStateRecord {
    high_water_sequence: Option<String>,
    cursor: Option<String>,
}

#[derive(Deserialize)]
struct TerminalStageRecord {
    stage_id_hash: String,
    object_key: Option<String>,
}

#[derive(Deserialize)]
struct AbandonedAttemptRecord {
    attempt_id: String,
    object_key: Option<String>,
}

impl<'a> DocumentStageRepository<'a> {
    pub fn new(
        database: &'a D1Database,
        bucket: &'a Bucket,
        capacity: Option<StageCapacity>,
    ) -> Self {
        Self {
            database,
            bucket,
            capacity,
        }
    }

    pub async fn mint(&self, mutation: &StageMintMutation<'_>) -> RepositoryResult<MutationResult> {
        validate_mint(mutation)?;
        if let Some(receipt) = self.active_receipt(mutation.idempotency_key).await? {
            return Ok(classify_receipt(receipt, mutation));
        }
        self.ensure_sequence_control().await?;

        let now = self
            .database
            .prepare("SELECT CAST(strftime('%s', 'now') AS INTEGER) AS now_seconds")
            .first::<i64>(Some("now_seconds"))
            .await?
            .ok_or_else(|| Error::RustError("database time could not be read".to_owned()))?;
        let expires_at = now
            .checked_add(V2_DOCUMENT_STAGE_TTL_SECONDS)
            .ok_or(RepositoryError::InvalidField("expires_at"))?;
        let token = random_stage_id()?;
        let stage_id_hash = sha256_hex(token.as_bytes());
        let result_json = serde_json::to_string(&serde_json::json!({
            "command_id": mutation.command_id,
            "stage_id": token,
            "expires_at": expires_at,
        }))
        .map_err(|_| RepositoryError::InvalidJson)?;

        let funnel_id = D1Type::Text(mutation.funnel_id);
        let base_revision_id = D1Type::Text(mutation.base_revision_id);
        let expected_length = D1Type::Integer(
            i32::try_from(mutation.expected_length)
                .map_err(|_| RepositoryError::InvalidField("expected_length"))?,
        );
        let expected_sha256 = D1Type::Text(mutation.expected_sha256);
        let stage_id_hash = D1Type::Text(&stage_id_hash);
        let command_id = D1Type::Text(mutation.command_id);
        let idempotency_key = D1Type::Text(mutation.idempotency_key);
        let payload_json = D1Type::Text(mutation.payload_json);
        let result_json_value = D1Type::Text(&result_json);
        let expires_at_value = D1Type::Real(expires_at as f64);
        let stage_slots = D1Type::Integer(V2_DOCUMENT_STAGE_SLOT_LIMIT);
        let document_version = D1Type::Integer(2);
        let max_bytes = D1Type::Integer(V2_DOCUMENT_STAGE_MAX_BYTES as i32);

        let delete_expired_receipts = self.database.prepare(
            "DELETE FROM command_receipts WHERE expires_at <= CAST(strftime('%s', 'now') AS INTEGER)",
        );
        let now_value = D1Type::Real(now as f64);
        let expire_stages = self.database.prepare(
            "UPDATE v2_document_stages SET state = 'expired' WHERE state IN ('issued', 'uploaded') AND (expires_at <= ? OR NOT EXISTS (SELECT 1 FROM funnel_editor_heads WHERE funnel_id = v2_document_stages.funnel_id AND revision_id = v2_document_stages.base_revision_id))",
        ).bind_refs([&now_value])?;
        let insert_receipt = self
            .database
            .prepare(
                "INSERT INTO command_receipts (idempotency_key, command_id, payload_json, result_json, expires_at) SELECT ?, ?, ?, ?, CAST(strftime('%s', 'now') AS INTEGER) + 86400 WHERE EXISTS (SELECT 1 FROM funnel_editor_heads WHERE funnel_id = ? AND revision_id = ?) AND (SELECT COUNT(*) FROM v2_document_stages WHERE slot_held = 1) < ? AND NOT EXISTS (SELECT 1 FROM v2_document_stages WHERE funnel_id = ? AND state IN ('issued', 'uploaded')) ON CONFLICT(idempotency_key) DO UPDATE SET command_id = excluded.command_id, payload_json = excluded.payload_json, result_json = excluded.result_json, expires_at = excluded.expires_at WHERE command_receipts.expires_at <= CAST(strftime('%s', 'now') AS INTEGER)",
            )
            .bind_refs([
                &idempotency_key,
                &command_id,
                &payload_json,
                &result_json_value,
                &funnel_id,
                &base_revision_id,
                &stage_slots,
                &funnel_id,
            ])?;
        let insert_stage = self
            .database
            .prepare(
                "INSERT INTO v2_document_stages (stage_id_hash, funnel_id, base_revision_id, purpose, document_type, document_version, schema_version, manifest_version, maximum_bytes, expected_length, expected_sha256, expires_at, state) SELECT ?, ?, ?, 'full_snapshot', 'eezfunnel.document', ?, ?, ?, ?, ?, ?, ?, 'issued' WHERE EXISTS (SELECT 1 FROM command_receipts WHERE idempotency_key = ? AND command_id = ? AND payload_json = ? AND result_json = ? AND expires_at > CAST(strftime('%s', 'now') AS INTEGER)) AND EXISTS (SELECT 1 FROM funnel_editor_heads WHERE funnel_id = ? AND revision_id = ?) AND (SELECT COUNT(*) FROM v2_document_stages WHERE slot_held = 1) < ? AND NOT EXISTS (SELECT 1 FROM v2_document_stages WHERE funnel_id = ? AND state IN ('issued', 'uploaded'))",
            )
            .bind_refs([
                &stage_id_hash,
                &funnel_id,
                &base_revision_id,
                &document_version,
                &document_version,
                &document_version,
                &max_bytes,
                &expected_length,
                &expected_sha256,
                &expires_at_value,
                &idempotency_key,
                &command_id,
                &payload_json,
                &result_json_value,
                &funnel_id,
                &base_revision_id,
                &stage_slots,
                &funnel_id,
            ])?;

        if let Err(error) = self
            .database
            .batch(vec![
                delete_expired_receipts,
                expire_stages,
                insert_receipt,
                insert_stage,
            ])
            .await
        {
            if let Some(receipt) = self.active_receipt(mutation.idempotency_key).await? {
                return Ok(classify_receipt(receipt, mutation));
            }
            self.classify_mint_failure(mutation).await?;
            return Err(error.into());
        }

        if let Some(receipt) = self.active_receipt(mutation.idempotency_key).await? {
            return Ok(classify_receipt(receipt, mutation));
        }
        self.classify_mint_failure(mutation).await?;
        Err(Error::RustError("document stage was not minted".to_owned()).into())
    }

    pub async fn upload(
        &self,
        funnel_id: &str,
        stage_id: &str,
        bytes: &[u8],
    ) -> RepositoryResult<()> {
        if !is_stage_id(stage_id) {
            return Err(RepositoryError::FunnelNotFound);
        }
        if bytes.is_empty() || bytes.len() > V2_DOCUMENT_STAGE_MAX_BYTES {
            return Err(RepositoryError::InvalidField("upload_length"));
        }
        let stage_id_hash = sha256_hex(stage_id.as_bytes());
        let stage = self.find_upload_stage(&stage_id_hash).await?;
        let Some(stage) = stage.filter(|stage| stage.funnel_id == funnel_id) else {
            return Err(RepositoryError::FunnelNotFound);
        };
        let digest = sha256_hex(bytes);
        if stage.expected_length != bytes.len() as i64 || stage.expected_sha256 != digest {
            return Err(RepositoryError::StageDigestMismatch);
        }
        let now = self.database_now().await?;
        if is_live_uploaded_retry(&stage, now, bytes.len(), &digest) {
            return Ok(());
        }
        if stage.state == "uploaded" {
            return Err(RepositoryError::StageUnavailable);
        }
        if stage.state != "issued" || stage.expires_at <= now {
            return Err(RepositoryError::StageUnavailable);
        }
        self.ensure_sequence_control().await?;

        let attempt_id = random_stage_id().map_err(|_| RepositoryError::StageIdGenerationFailed)?;
        self.reserve_attempt(&stage, &attempt_id).await?;
        let sequence = match self.allocate_sequence().await {
            Ok(sequence) => sequence,
            Err(error) => {
                self.release_unwritten_attempt(&attempt_id).await?;
                return Err(error);
            }
        };
        let object_key = stage_object_key(&sequence, &attempt_id)
            .ok_or(RepositoryError::StageControlUnavailable)?;
        if let Err(error) = self
            .record_attempt_key(&stage, &attempt_id, &sequence, &object_key)
            .await
        {
            self.release_unwritten_attempt(&attempt_id).await?;
            return Err(error);
        }

        match self
            .bucket
            .put(object_key.clone(), bytes.to_vec())
            .execute()
            .await
        {
            Ok(Some(_)) => {}
            Ok(None) => {
                self.abandon_attempt(&attempt_id).await?;
                return Err(RepositoryError::StageStorageUnavailable);
            }
            Err(_) => {
                self.abandon_attempt(&attempt_id).await?;
                return Err(RepositoryError::StageStorageUnavailable);
            }
        }

        if self
            .complete_attempt(&stage, &attempt_id, &object_key, bytes.len(), &digest)
            .await?
        {
            return Ok(());
        }
        self.abandon_attempt(&attempt_id).await?;
        let current = self.find_upload_stage(&stage.stage_id_hash).await?;
        let now = self.database_now().await?;
        match current {
            Some(current) if is_live_uploaded_retry(&current, now, bytes.len(), &digest) => Ok(()),
            _ => Err(RepositoryError::StageUnavailable),
        }
    }

    pub async fn capacity_snapshot(&self) -> RepositoryResult<StageCapacitySnapshot> {
        self.ensure_sequence_control().await?;
        let capacity = self
            .capacity
            .ok_or(RepositoryError::StageControlUnavailable)?;
        let usage = self.attempt_usage().await?;
        Ok(StageCapacitySnapshot {
            orphan_byte_budget: capacity.orphan_byte_budget,
            orphan_bytes_held: u64::try_from(usage.held_bytes)
                .map_err(|_| RepositoryError::StageControlUnavailable)?,
            orphan_key_budget: capacity.orphan_key_budget,
            orphan_keys_held: u64::try_from(usage.held_keys)
                .map_err(|_| RepositoryError::StageControlUnavailable)?,
            worst_case_orphan_bytes: capacity.orphan_byte_budget,
            worst_case_orphan_keys: capacity.orphan_key_budget,
        })
    }

    pub async fn ensure_sequence_control(&self) -> RepositoryResult<()> {
        let capacity = self
            .capacity
            .ok_or(RepositoryError::StageControlUnavailable)?;
        let initial = StageControl::initial(capacity);
        let initial_json =
            serde_json::to_vec(&initial).map_err(|_| RepositoryError::InvalidJson)?;
        for _ in 0..R2_SEQUENCE_RETRIES {
            match self.read_sequence_control().await {
                Ok((mut current, etag)) => {
                    if !current.increase_capacity(capacity)? {
                        return Ok(());
                    }
                    let encoded =
                        serde_json::to_vec(&current).map_err(|_| RepositoryError::InvalidJson)?;
                    match conditional_r2_put(
                        self.bucket,
                        R2_SEQUENCE_KEY,
                        &encoded,
                        r2_if_match(&etag),
                    )
                    .await
                    {
                        Ok(true) => return Ok(()),
                        Ok(false) | Err(_) => continue,
                    }
                }
                Err(_) => match conditional_r2_put(
                    self.bucket,
                    R2_SEQUENCE_KEY,
                    &initial_json,
                    r2_if_absent(),
                )
                .await
                {
                    Ok(true) => return Ok(()),
                    Ok(false) | Err(_) => continue,
                },
            }
        }
        Err(RepositoryError::StageControlUnavailable)
    }

    pub async fn cleanup_one_page(&self) -> RepositoryResult<StageCleanupProgress> {
        let now = self.database_now().await?;
        let grace_deadline = now.saturating_sub(V2_DOCUMENT_STAGE_ATTEMPT_GRACE_SECONDS);
        let tombstone_deadline = now
            .checked_add(V2_DOCUMENT_STAGE_TOMBSTONE_SECONDS)
            .ok_or(RepositoryError::InvalidField("tombstone_expires_at"))?;
        let now_value = D1Type::Real(now as f64);
        let grace_value = D1Type::Real(grace_deadline as f64);
        let tombstone_value = D1Type::Real(tombstone_deadline as f64);
        self.database
            .batch(vec![
                self.database.prepare(
                    "UPDATE v2_document_stages SET state = 'expired' WHERE state IN ('issued', 'uploaded') AND (expires_at <= ? OR NOT EXISTS (SELECT 1 FROM funnel_editor_heads WHERE funnel_id = v2_document_stages.funnel_id AND revision_id = v2_document_stages.base_revision_id))",
                ).bind_refs([&now_value])?,
                self.database.prepare(
                    "UPDATE v2_document_stage_attempts SET state = 'abandoned', tombstone_expires_at = ?, quota_held = 1 WHERE state = 'receiving' AND object_key IS NOT NULL AND deadline_at <= ? AND quota_held = 1",
                ).bind_refs([&tombstone_value, &grace_value])?,
                self.database.prepare(
                    "DELETE FROM v2_document_stage_attempts WHERE state = 'receiving' AND object_key IS NULL AND deadline_at <= ? AND quota_held = 1",
                ).bind_refs([&grace_value])?,
            ])
            .await?;

        self.cleanup_terminal_stages().await?;
        self.cleanup_abandoned_attempts(now).await?;
        self.retire_terminal_attempts().await?;

        let current_state = self.read_sweep_state().await?;
        let start_new_pass = current_state.high_water_sequence.is_none();
        let (high_water, cursor) = match current_state.high_water_sequence {
            Some(high_water) if is_sequence(&high_water) => (high_water, current_state.cursor),
            Some(_) => return Err(RepositoryError::StageControlUnavailable),
            None => {
                self.ensure_sequence_control().await?;
                (self.read_sequence_control().await?.0.sequence, None)
            }
        };
        if start_new_pass {
            self.write_sweep_state(Some(&high_water), None).await?;
        }

        let mut list = self
            .bucket
            .list()
            .prefix(R2_STAGE_PREFIX)
            .limit(R2_SWEEP_PAGE_SIZE);
        if let Some(cursor) = cursor.as_deref() {
            list = list.cursor(cursor);
        }
        let page = list
            .execute()
            .await
            .map_err(|_| RepositoryError::StageStorageUnavailable)?;
        let truncated = page.truncated();
        let next_cursor = page.cursor();
        let mut progress = StageCleanupProgress {
            listed: 0,
            deleted: 0,
            pass_completed: false,
        };
        let cutoff_seconds = now.saturating_sub(V2_DOCUMENT_STAGE_TOMBSTONE_SECONDS);
        let objects = page.objects();
        for object in objects {
            progress.listed += 1;
            let key = object.key();
            let Some(sequence) = stage_sequence_from_key(&key) else {
                continue;
            };
            if sequence.as_str() > high_water.as_str() {
                progress.pass_completed = true;
                self.write_sweep_state(None, None).await?;
                return Ok(progress);
            }
            let uploaded_seconds = object.uploaded().as_millis() / 1_000;
            if uploaded_seconds > cutoff_seconds.max(0) as u64 {
                continue;
            }
            if self.is_live_stage_reference(&key).await? {
                continue;
            }
            self.bucket
                .delete(key)
                .await
                .map_err(|_| RepositoryError::StageStorageUnavailable)?;
            progress.deleted += 1;
        }

        if truncated {
            let next_cursor = next_cursor.ok_or(RepositoryError::StageStorageUnavailable)?;
            self.write_sweep_state(Some(&high_water), Some(&next_cursor))
                .await?;
        } else {
            progress.pass_completed = true;
            self.write_sweep_state(None, None).await?;
        }
        Ok(progress)
    }

    async fn cleanup_terminal_stages(&self) -> RepositoryResult<()> {
        let limit = D1Type::Integer(V2_DOCUMENT_STAGE_SLOT_LIMIT);
        let result = self
            .database
            .prepare(
                "SELECT stage_id_hash, object_key FROM v2_document_stages WHERE slot_held = 1 AND state IN ('expired', 'consumed') ORDER BY created_at, stage_id_hash LIMIT ?",
            )
            .bind_refs([&limit])?
            .all()
            .await?;
        let stages = result.results::<TerminalStageRecord>()?;
        for stage in stages {
            if let Some(object_key) = stage.object_key.as_deref() {
                self.bucket
                    .delete(object_key.to_owned())
                    .await
                    .map_err(|_| RepositoryError::StageStorageUnavailable)?;
            }
            let stage_hash = D1Type::Text(&stage.stage_id_hash);
            let object_key = stage
                .object_key
                .as_deref()
                .map(D1Type::Text)
                .unwrap_or(D1Type::Null);
            self.database
                .prepare(
                    "UPDATE v2_document_stages SET state = 'expired', object_key = NULL, uploaded_length = NULL, uploaded_sha256 = NULL, slot_held = 0 WHERE stage_id_hash = ? AND slot_held = 1 AND state IN ('expired', 'consumed') AND object_key IS ?",
                )
                .bind_refs([&stage_hash, &object_key])?
                .run()
                .await?;
        }
        Ok(())
    }

    pub async fn read_for_finalize(
        &self,
        funnel_id: &str,
        base_revision_id: &str,
        stage_id: &str,
        expected_length: usize,
        expected_sha256: &str,
    ) -> RepositoryResult<Vec<u8>> {
        if !is_stage_id(stage_id) || !is_lower_hex_digest(expected_sha256) {
            return Err(RepositoryError::StageUnavailable);
        }
        if expected_length == 0 || expected_length > V2_DOCUMENT_STAGE_MAX_BYTES {
            return Err(RepositoryError::InvalidField("expected_length"));
        }
        let stage_hash = sha256_hex(stage_id.as_bytes());
        let stage = self
            .find_upload_stage(&stage_hash)
            .await?
            .filter(|stage| stage.funnel_id == funnel_id)
            .ok_or(RepositoryError::FunnelNotFound)?;
        let now = self.database_now().await?;
        if stage.state != "uploaded"
            || stage.expires_at <= now
            || stage.base_revision_id != base_revision_id
            || stage.expected_length != expected_length as i64
            || stage.expected_sha256 != expected_sha256
            || stage.purpose != "full_snapshot"
            || stage.document_type != "eezfunnel.document"
            || stage.document_version != 2
            || stage.schema_version != 2
            || stage.manifest_version != 2
            || stage.maximum_bytes != V2_DOCUMENT_STAGE_MAX_BYTES as i64
        {
            return Err(RepositoryError::StageUnavailable);
        }
        let Some(current_head) = self.editor_head(funnel_id).await? else {
            return Err(RepositoryError::FunnelNotFound);
        };
        if current_head != base_revision_id {
            self.expire_stage(&stage.stage_id_hash).await?;
            return Err(RepositoryError::StaleEditorRevision);
        }
        let object_key = stage
            .object_key
            .as_deref()
            .ok_or(RepositoryError::StageUnavailable)?;
        let head = self
            .bucket
            .head(object_key.to_owned())
            .await
            .map_err(|_| RepositoryError::StageStorageUnavailable)?
            .ok_or(RepositoryError::StageUnavailable)?;
        if head.size() != expected_length as u64 {
            return Err(RepositoryError::StageDigestMismatch);
        }
        let object = self
            .bucket
            .get(object_key.to_owned())
            .execute()
            .await
            .map_err(|_| RepositoryError::StageStorageUnavailable)?
            .ok_or(RepositoryError::StageUnavailable)?;
        if object.size() != expected_length as u64 {
            return Err(RepositoryError::StageDigestMismatch);
        }
        let bytes = object
            .body()
            .ok_or(RepositoryError::StageUnavailable)?
            .bytes()
            .await
            .map_err(|_| RepositoryError::StageStorageUnavailable)?;
        if bytes.len() != expected_length || sha256_hex(&bytes) != expected_sha256 {
            return Err(RepositoryError::StageDigestMismatch);
        }
        Ok(bytes)
    }

    pub async fn save_staged_revision(
        &self,
        mutation: &StageFinalizeMutation<'_>,
    ) -> RepositoryResult<MutationResult> {
        validate_finalize(mutation)?;
        if let Some(receipt) = self.active_receipt(mutation.idempotency_key).await? {
            return Ok(classify_finalize_receipt(receipt, mutation));
        }

        let stage_id_hash = sha256_hex(mutation.stage_id.as_bytes());
        let revision = mutation.revision;
        let funnel_id = D1Type::Text(mutation.funnel_id);
        let base_revision_id = D1Type::Text(mutation.base_revision_id);
        let stage_hash = D1Type::Text(&stage_id_hash);
        let stage_length = D1Type::Integer(
            i32::try_from(mutation.expected_length)
                .map_err(|_| RepositoryError::InvalidField("expected_length"))?,
        );
        let stage_digest = D1Type::Text(mutation.expected_sha256);
        let revision_id = D1Type::Text(&revision.id);
        let revision_funnel_id = D1Type::Text(&revision.funnel_id);
        let schema_version = D1Type::Integer(2);
        let data_json = D1Type::Text(&revision.data_json);
        let manifest_json = D1Type::Text(&revision.render_manifest_json);
        let key = D1Type::Text(mutation.idempotency_key);
        let command = D1Type::Text(mutation.command_id);
        let payload = D1Type::Text(mutation.payload_json);
        let result = D1Type::Text(mutation.result_json);
        let revision_limit = D1Type::Integer(MAX_REVISIONS_RETAINED);
        let delete_expired_receipts = self.database.prepare(
            "DELETE FROM command_receipts WHERE expires_at <= CAST(strftime('%s', 'now') AS INTEGER)",
        );
        let insert_receipt = self
            .database
            .prepare(
                "INSERT INTO command_receipts (idempotency_key, command_id, payload_json, result_json, expires_at) SELECT ?, ?, ?, ?, CAST(strftime('%s', 'now') AS INTEGER) + 86400 WHERE EXISTS (SELECT 1 FROM funnel_editor_heads WHERE funnel_id = ? AND revision_id = ?) AND (SELECT COUNT(*) FROM revisions WHERE funnel_id = ?) < ? AND EXISTS (SELECT 1 FROM v2_document_stages WHERE stage_id_hash = ? AND funnel_id = ? AND base_revision_id = ? AND purpose = 'full_snapshot' AND document_type = 'eezfunnel.document' AND document_version = 2 AND schema_version = 2 AND manifest_version = 2 AND maximum_bytes = 1048576 AND expected_length = ? AND expected_sha256 = ? AND state = 'uploaded' AND uploaded_length = ? AND uploaded_sha256 = ? AND expires_at > CAST(strftime('%s', 'now') AS INTEGER) AND slot_held = 1)",
            )
            .bind_refs([
                &key,
                &command,
                &payload,
                &result,
                &funnel_id,
                &base_revision_id,
                &funnel_id,
                &revision_limit,
                &stage_hash,
                &funnel_id,
                &base_revision_id,
                &stage_length,
                &stage_digest,
                &stage_length,
                &stage_digest,
            ])?;
        let receipt_matches = "EXISTS (SELECT 1 FROM command_receipts WHERE idempotency_key = ? AND command_id = ? AND payload_json = ? AND result_json = ? AND expires_at > CAST(strftime('%s', 'now') AS INTEGER))";
        let insert_revision = self
            .database
            .prepare(format!(
                "INSERT INTO revisions (id, funnel_id, schema_version, data_json, render_manifest_json) SELECT ?, ?, ?, ?, ? WHERE EXISTS (SELECT 1 FROM funnel_editor_heads WHERE funnel_id = ? AND revision_id = ?) AND (SELECT COUNT(*) FROM revisions WHERE funnel_id = ?) < ? AND EXISTS (SELECT 1 FROM v2_document_stages WHERE stage_id_hash = ? AND funnel_id = ? AND base_revision_id = ? AND expected_length = ? AND expected_sha256 = ? AND state = 'uploaded' AND uploaded_length = ? AND uploaded_sha256 = ? AND expires_at > CAST(strftime('%s', 'now') AS INTEGER) AND slot_held = 1) AND {receipt_matches}"
            ))
            .bind_refs([
                &revision_id,
                &revision_funnel_id,
                &schema_version,
                &data_json,
                &manifest_json,
                &funnel_id,
                &base_revision_id,
                &funnel_id,
                &revision_limit,
                &stage_hash,
                &funnel_id,
                &base_revision_id,
                &stage_length,
                &stage_digest,
                &stage_length,
                &stage_digest,
                &key,
                &command,
                &payload,
                &result,
            ])?;
        let update_head = self
            .database
            .prepare(format!(
                "UPDATE funnel_editor_heads SET revision_id = ? WHERE funnel_id = ? AND revision_id = ? AND EXISTS (SELECT 1 FROM revisions WHERE id = ? AND funnel_id = ?) AND EXISTS (SELECT 1 FROM v2_document_stages WHERE stage_id_hash = ? AND state = 'uploaded' AND expires_at > CAST(strftime('%s', 'now') AS INTEGER)) AND {receipt_matches}"
            ))
            .bind_refs([
                &revision_id,
                &funnel_id,
                &base_revision_id,
                &revision_id,
                &funnel_id,
                &stage_hash,
                &key,
                &command,
                &payload,
                &result,
            ])?;
        let update_funnel = self
            .database
            .prepare(format!(
                "UPDATE funnels SET updated_at = CURRENT_TIMESTAMP WHERE id = ? AND EXISTS (SELECT 1 FROM funnel_editor_heads WHERE funnel_id = ? AND revision_id = ?) AND {receipt_matches}"
            ))
            .bind_refs([
                &funnel_id,
                &funnel_id,
                &revision_id,
                &key,
                &command,
                &payload,
                &result,
            ])?;
        let consume_stage = self
            .database
            .prepare(format!(
                "UPDATE v2_document_stages SET state = 'consumed' WHERE stage_id_hash = ? AND funnel_id = ? AND base_revision_id = ? AND state = 'uploaded' AND expires_at > CAST(strftime('%s', 'now') AS INTEGER) AND expected_length = ? AND expected_sha256 = ? AND uploaded_length = ? AND uploaded_sha256 = ? AND EXISTS (SELECT 1 FROM funnel_editor_heads WHERE funnel_id = ? AND revision_id = ?) AND {receipt_matches}"
            ))
            .bind_refs([
                &stage_hash,
                &funnel_id,
                &base_revision_id,
                &stage_length,
                &stage_digest,
                &stage_length,
                &stage_digest,
                &funnel_id,
                &revision_id,
                &key,
                &command,
                &payload,
                &result,
            ])?;

        if let Err(error) = self
            .database
            .batch(vec![
                delete_expired_receipts,
                insert_receipt,
                insert_revision,
                update_head,
                update_funnel,
                consume_stage,
            ])
            .await
        {
            if let Some(receipt) = self.active_receipt(mutation.idempotency_key).await? {
                return Ok(classify_finalize_receipt(receipt, mutation));
            }
            self.classify_finalize_failure(mutation, error).await?;
        }
        if let Some(receipt) = self.active_receipt(mutation.idempotency_key).await? {
            return Ok(classify_finalize_receipt(receipt, mutation));
        }
        self.classify_finalize_failure(
            mutation,
            Error::RustError("V2 finalization did not commit".to_owned()),
        )
        .await?;
        Err(RepositoryError::StageUnavailable)
    }

    async fn retire_terminal_attempts(&self) -> RepositoryResult<()> {
        self.database
            .batch(vec![
                self.database.prepare(
                    "DELETE FROM v2_document_stage_attempts WHERE state = 'complete' AND EXISTS (SELECT 1 FROM v2_document_stages WHERE stage_id_hash = v2_document_stage_attempts.stage_id_hash AND slot_held = 0)",
                ),
                self.database.prepare(
                    "DELETE FROM v2_document_stages WHERE slot_held = 0 AND NOT EXISTS (SELECT 1 FROM v2_document_stage_attempts WHERE stage_id_hash = v2_document_stages.stage_id_hash)",
                ),
            ])
            .await?;
        Ok(())
    }

    async fn cleanup_abandoned_attempts(&self, now: i64) -> RepositoryResult<()> {
        let now_value = D1Type::Real(now as f64);
        let limit = D1Type::Integer(V2_DOCUMENT_STAGE_ATTEMPT_LIMIT);
        let attempts = self
            .database
            .prepare(
                "SELECT attempt_id, object_key FROM v2_document_stage_attempts WHERE state = 'abandoned' AND tombstone_expires_at <= ? ORDER BY tombstone_expires_at, attempt_id LIMIT ?",
            )
            .bind_refs([&now_value, &limit])?
            .all()
            .await?
            .results::<AbandonedAttemptRecord>()?;
        for attempt in attempts {
            if let Some(object_key) = attempt.object_key.as_deref() {
                if self.is_live_stage_reference(object_key).await? {
                    continue;
                }
                if self
                    .bucket
                    .head(object_key.to_owned())
                    .await
                    .map_err(|_| RepositoryError::StageStorageUnavailable)?
                    .is_some()
                {
                    self.bucket
                        .delete(object_key.to_owned())
                        .await
                        .map_err(|_| RepositoryError::StageStorageUnavailable)?;
                    if self
                        .bucket
                        .head(object_key.to_owned())
                        .await
                        .map_err(|_| RepositoryError::StageStorageUnavailable)?
                        .is_some()
                    {
                        return Err(RepositoryError::StageStorageUnavailable);
                    }
                }
            }
            let attempt_id = D1Type::Text(&attempt.attempt_id);
            let object_key = attempt
                .object_key
                .as_deref()
                .map(D1Type::Text)
                .unwrap_or(D1Type::Null);
            self.database
                .prepare(
                    "DELETE FROM v2_document_stage_attempts WHERE attempt_id = ? AND state = 'abandoned' AND tombstone_expires_at <= ? AND object_key IS ?",
                )
                .bind_refs([&attempt_id, &now_value, &object_key])?
                .run()
                .await?;
        }
        Ok(())
    }

    async fn classify_finalize_failure(
        &self,
        mutation: &StageFinalizeMutation<'_>,
        failure: Error,
    ) -> RepositoryResult<()> {
        #[derive(Deserialize)]
        struct Existence {
            exists: i64,
        }
        let funnel_id = D1Type::Text(mutation.funnel_id);
        let exists = self
            .database
            .prepare("SELECT EXISTS(SELECT 1 FROM funnels WHERE id = ?) AS exists")
            .bind_refs([&funnel_id])?
            .first::<Existence>(None)
            .await?
            .ok_or_else(|| Error::RustError("funnel state could not be read".to_owned()))?;
        if exists.exists == 0 {
            return Err(RepositoryError::FunnelNotFound);
        }
        match self.editor_head(mutation.funnel_id).await? {
            Some(head) if head != mutation.base_revision_id => {
                let stage_hash = sha256_hex(mutation.stage_id.as_bytes());
                self.expire_stage(&stage_hash).await?;
                return Err(RepositoryError::StaleEditorRevision);
            }
            None => return Err(RepositoryError::FunnelNotFound),
            Some(_) => {}
        }
        let funnel_id = D1Type::Text(mutation.funnel_id);
        let revisions = self
            .database
            .prepare("SELECT COUNT(*) AS count FROM revisions WHERE funnel_id = ?")
            .bind_refs([&funnel_id])?
            .first::<i64>(Some("count"))
            .await?
            .ok_or_else(|| Error::RustError("revision count could not be read".to_owned()))?;
        if revisions >= i64::from(MAX_REVISIONS_RETAINED) {
            return Err(RepositoryError::EditorRevisionLimitReached);
        }
        let stage_hash = sha256_hex(mutation.stage_id.as_bytes());
        let now = self.database_now().await?;
        let stage = self.find_upload_stage(&stage_hash).await?;
        if !stage.is_some_and(|stage| {
            stage.funnel_id == mutation.funnel_id
                && stage.base_revision_id == mutation.base_revision_id
                && stage.state == "uploaded"
                && stage.expires_at > now
                && stage.expected_length == mutation.expected_length as i64
                && stage.expected_sha256 == mutation.expected_sha256
        }) {
            return Err(RepositoryError::StageUnavailable);
        }
        Err(failure.into())
    }

    async fn editor_head(&self, funnel_id: &str) -> RepositoryResult<Option<String>> {
        let funnel_id = D1Type::Text(funnel_id);
        self.database
            .prepare("SELECT revision_id FROM funnel_editor_heads WHERE funnel_id = ?")
            .bind_refs([&funnel_id])?
            .first::<String>(Some("revision_id"))
            .await
            .map_err(Into::into)
    }

    async fn expire_stage(&self, stage_id_hash: &str) -> RepositoryResult<()> {
        let stage_hash = D1Type::Text(stage_id_hash);
        self.database
            .prepare(
                "UPDATE v2_document_stages SET state = 'expired' WHERE stage_id_hash = ? AND state IN ('issued', 'uploaded')",
            )
            .bind_refs([&stage_hash])?
            .run()
            .await?;
        Ok(())
    }

    async fn is_live_stage_reference(&self, object_key: &str) -> RepositoryResult<bool> {
        #[derive(Deserialize)]
        struct LiveReference {
            live: i64,
        }
        let stage_key = D1Type::Text(object_key);
        let attempt_key = D1Type::Text(object_key);
        self.database
            .prepare(
                "SELECT EXISTS(SELECT 1 FROM v2_document_stages WHERE object_key = ? AND state = 'uploaded' AND slot_held = 1) OR EXISTS(SELECT 1 FROM v2_document_stage_attempts WHERE object_key = ? AND state = 'receiving' AND quota_held = 1) AS live",
            )
            .bind_refs([&stage_key, &attempt_key])?
            .first::<LiveReference>(None)
            .await?
            .map(|reference| reference.live != 0)
            .ok_or_else(|| Error::RustError("stage reference state could not be read".to_owned()).into())
    }

    async fn read_sweep_state(&self) -> RepositoryResult<SweepStateRecord> {
        self.database
            .prepare(
                "SELECT high_water_sequence, cursor FROM v2_document_stage_sweep_state WHERE singleton_id = 1",
            )
            .first::<SweepStateRecord>(None)
            .await?
            .ok_or_else(|| Error::RustError("stage sweep state could not be read".to_owned()).into())
    }

    async fn write_sweep_state(
        &self,
        high_water_sequence: Option<&str>,
        cursor: Option<&str>,
    ) -> RepositoryResult<()> {
        let high_water = high_water_sequence
            .map(D1Type::Text)
            .unwrap_or(D1Type::Null);
        let cursor = cursor.map(D1Type::Text).unwrap_or(D1Type::Null);
        self.database
            .prepare(
                "UPDATE v2_document_stage_sweep_state SET high_water_sequence = ?, cursor = ?, updated_at = CAST(strftime('%s', 'now') AS INTEGER) WHERE singleton_id = 1",
            )
            .bind_refs([&high_water, &cursor])?
            .run()
            .await?;
        Ok(())
    }

    async fn find_upload_stage(
        &self,
        stage_id_hash: &str,
    ) -> RepositoryResult<Option<StageUploadRecord>> {
        let stage_hash = D1Type::Text(stage_id_hash);
        self.database
            .prepare(
                "SELECT stage_id_hash, funnel_id, base_revision_id, purpose, document_type, document_version, schema_version, manifest_version, maximum_bytes, expected_length, expected_sha256, expires_at, state, object_key, uploaded_length, uploaded_sha256 FROM v2_document_stages WHERE stage_id_hash = ?",
            )
            .bind_refs([&stage_hash])?
            .first::<StageUploadRecord>(None)
            .await
            .map_err(Into::into)
    }

    async fn reserve_attempt(
        &self,
        stage: &StageUploadRecord,
        attempt_id: &str,
    ) -> RepositoryResult<()> {
        let capacity = self
            .capacity
            .ok_or(RepositoryError::StageControlUnavailable)?;
        let now = self.database_now().await?;
        let deadline = now
            .checked_add(V2_DOCUMENT_STAGE_ATTEMPT_TTL_SECONDS)
            .ok_or(RepositoryError::InvalidField("attempt_deadline"))?;
        let now_value = D1Type::Real(now as f64);
        let deadline_value = D1Type::Real(deadline as f64);
        let attempt_id_value = D1Type::Text(attempt_id);
        let stage_hash = D1Type::Text(&stage.stage_id_hash);
        let expected_length = D1Type::Integer(
            i32::try_from(stage.expected_length)
                .map_err(|_| RepositoryError::InvalidField("expected_length"))?,
        );
        let expected_digest = D1Type::Text(&stage.expected_sha256);
        let attempt_limit = D1Type::Integer(V2_DOCUMENT_STAGE_ATTEMPT_LIMIT);
        let byte_budget = D1Type::Real(capacity.orphan_byte_budget as f64);
        let key_budget = D1Type::Real(capacity.orphan_key_budget as f64);
        self.database
            .prepare(
                "INSERT INTO v2_document_stage_attempts (attempt_id, stage_id_hash, state, expected_length, expected_sha256, deadline_at) SELECT ?, ?, 'receiving', ?, ?, ? WHERE EXISTS (SELECT 1 FROM v2_document_stages AS stage WHERE stage_id_hash = ? AND state = 'issued' AND expires_at > ? AND EXISTS (SELECT 1 FROM funnel_editor_heads WHERE funnel_id = stage.funnel_id AND revision_id = stage.base_revision_id)) AND (SELECT COUNT(*) FROM v2_document_stage_attempts WHERE quota_held = 1) < ? AND (SELECT COALESCE(SUM(expected_length), 0) FROM v2_document_stage_attempts WHERE quota_held = 1) + ? <= ? AND (SELECT COUNT(*) FROM v2_document_stage_attempts WHERE quota_held = 1) + 1 <= ?",
            )
            .bind_refs([
                &attempt_id_value,
                &stage_hash,
                &expected_length,
                &expected_digest,
                &deadline_value,
                &stage_hash,
                &now_value,
                &attempt_limit,
                &expected_length,
                &byte_budget,
                &key_budget,
            ])?
            .run()
            .await?;
        let attempt = self.find_attempt(attempt_id).await?;
        if attempt.is_some() {
            return Ok(());
        }
        let state = self.read_admission_state(stage).await?;
        if state.funnel_exists == 0 {
            return Err(RepositoryError::FunnelNotFound);
        }
        if state.base_matches == 0 || state.active_stage_exists == 0 {
            return Err(RepositoryError::StageUnavailable);
        }
        if stage.expires_at <= self.database_now().await? {
            return Err(RepositoryError::StageUnavailable);
        }
        if self.attempt_slots_held().await? >= i64::from(V2_DOCUMENT_STAGE_ATTEMPT_LIMIT) {
            return Err(RepositoryError::StageCapacityReached);
        }
        let usage = self.attempt_usage().await?;
        if !stage_quota_fits(
            usage.held_bytes,
            usage.held_keys,
            stage.expected_length as u64,
            capacity,
        ) {
            return Err(RepositoryError::StageCapacityReached);
        }
        Err(RepositoryError::StageUnavailable)
    }

    async fn read_admission_state(
        &self,
        stage: &StageUploadRecord,
    ) -> RepositoryResult<StageAdmissionState> {
        let funnel_id = D1Type::Text(&stage.funnel_id);
        let stage_hash = D1Type::Text(&stage.stage_id_hash);
        self.database
            .prepare(
                "SELECT EXISTS(SELECT 1 FROM funnels WHERE id = ?) AS funnel_exists, EXISTS(SELECT 1 FROM funnel_editor_heads WHERE funnel_id = ? AND revision_id = (SELECT base_revision_id FROM v2_document_stages WHERE stage_id_hash = ?)) AS base_matches, EXISTS(SELECT 1 FROM v2_document_stages WHERE stage_id_hash = ? AND state = 'issued') AS active_stage_exists, (SELECT COUNT(*) FROM v2_document_stages WHERE slot_held = 1) AS held_slots",
            )
            .bind_refs([&funnel_id, &funnel_id, &stage_hash, &stage_hash])?
            .first::<StageAdmissionState>(None)
            .await
            .map_err(RepositoryError::from)?
            .ok_or_else(|| Error::RustError("stage admission could not be read".to_owned()).into())
    }

    async fn attempt_slots_held(&self) -> RepositoryResult<i64> {
        Ok(self.attempt_usage().await?.held_keys)
    }

    async fn attempt_usage(&self) -> RepositoryResult<StageAttemptUsage> {
        self.database
            .prepare(
                "SELECT COALESCE(SUM(expected_length), 0) AS held_bytes, COUNT(*) AS held_keys FROM v2_document_stage_attempts WHERE quota_held = 1",
            )
            .first::<StageAttemptUsage>(None)
            .await?
            .ok_or_else(|| {
                Error::RustError("stage attempt quota could not be read".to_owned()).into()
            })
    }

    async fn find_attempt(&self, attempt_id: &str) -> RepositoryResult<Option<StageAttemptRecord>> {
        let attempt_id = D1Type::Text(attempt_id);
        self.database
            .prepare(
                "SELECT state, object_key FROM v2_document_stage_attempts WHERE attempt_id = ?",
            )
            .bind_refs([&attempt_id])?
            .first::<StageAttemptRecord>(None)
            .await
            .map_err(Into::into)
    }

    async fn record_attempt_key(
        &self,
        stage: &StageUploadRecord,
        attempt_id: &str,
        sequence: &str,
        object_key: &str,
    ) -> RepositoryResult<()> {
        let now = self.database_now().await?;
        let now_value = D1Type::Real(now as f64);
        let attempt_id_value = D1Type::Text(attempt_id);
        let sequence_value = D1Type::Text(sequence);
        let object_key_value = D1Type::Text(object_key);
        let stage_hash = D1Type::Text(&stage.stage_id_hash);
        self.database
            .prepare(
                "UPDATE v2_document_stage_attempts SET sequence = ?, object_key = ? WHERE attempt_id = ? AND state = 'receiving' AND sequence IS NULL AND deadline_at > ? AND EXISTS (SELECT 1 FROM v2_document_stages AS stage WHERE stage_id_hash = ? AND state = 'issued' AND expires_at > ? AND EXISTS (SELECT 1 FROM funnel_editor_heads WHERE funnel_id = stage.funnel_id AND revision_id = stage.base_revision_id))",
            )
            .bind_refs([
                &sequence_value,
                &object_key_value,
                &attempt_id_value,
                &now_value,
                &stage_hash,
                &now_value,
            ])?
            .run()
            .await?;
        match self.find_attempt(attempt_id).await? {
            Some(record)
                if record.state == "receiving"
                    && record.object_key.as_deref() == Some(object_key) =>
            {
                Ok(())
            }
            _ => Err(RepositoryError::StageUnavailable),
        }
    }

    async fn complete_attempt(
        &self,
        stage: &StageUploadRecord,
        attempt_id: &str,
        object_key: &str,
        uploaded_length: usize,
        uploaded_sha256: &str,
    ) -> RepositoryResult<bool> {
        let now = self.database_now().await?;
        let now_value = D1Type::Real(now as f64);
        let attempt_id_value = D1Type::Text(attempt_id);
        let object_key_value = D1Type::Text(object_key);
        let length_value = D1Type::Integer(
            i32::try_from(uploaded_length)
                .map_err(|_| RepositoryError::InvalidField("uploaded_length"))?,
        );
        let digest_value = D1Type::Text(uploaded_sha256);
        let stage_hash = D1Type::Text(&stage.stage_id_hash);
        let update_attempt = self
            .database
            .prepare(
                "UPDATE v2_document_stage_attempts SET state = 'complete' WHERE attempt_id = ? AND state = 'receiving' AND deadline_at > ? AND object_key = ? AND EXISTS (SELECT 1 FROM v2_document_stages AS stage WHERE stage_id_hash = ? AND state = 'issued' AND expires_at > ? AND EXISTS (SELECT 1 FROM funnel_editor_heads WHERE funnel_id = stage.funnel_id AND revision_id = stage.base_revision_id))",
            )
            .bind_refs([
                &attempt_id_value,
                &now_value,
                &object_key_value,
                &stage_hash,
                &now_value,
            ])?;
        let update_stage = self
            .database
            .prepare(
                "UPDATE v2_document_stages SET state = 'uploaded', object_key = ?, uploaded_length = ?, uploaded_sha256 = ? WHERE stage_id_hash = ? AND state = 'issued' AND expires_at > ? AND expected_length = ? AND expected_sha256 = ? AND EXISTS (SELECT 1 FROM v2_document_stage_attempts WHERE attempt_id = ? AND state = 'complete' AND object_key = ?) AND EXISTS (SELECT 1 FROM funnel_editor_heads WHERE funnel_id = v2_document_stages.funnel_id AND revision_id = v2_document_stages.base_revision_id)",
            )
            .bind_refs([
                &object_key_value,
                &length_value,
                &digest_value,
                &stage_hash,
                &now_value,
                &length_value,
                &digest_value,
                &attempt_id_value,
                &object_key_value,
            ])?;
        self.database
            .batch(vec![update_attempt, update_stage])
            .await?;
        let stage_after = self.find_upload_stage(&stage.stage_id_hash).await?;
        Ok(stage_after.is_some_and(|record| {
            record.state == "uploaded" && record.object_key.as_deref() == Some(object_key)
        }))
    }

    async fn abandon_attempt(&self, attempt_id: &str) -> RepositoryResult<()> {
        let now = self.database_now().await?;
        let tombstone = now
            .checked_add(V2_DOCUMENT_STAGE_TOMBSTONE_SECONDS)
            .ok_or(RepositoryError::InvalidField("tombstone_expires_at"))?;
        let tombstone_value = D1Type::Real(tombstone as f64);
        let attempt_id_value = D1Type::Text(attempt_id);
        self.database
            .prepare(
                "UPDATE v2_document_stage_attempts SET state = 'abandoned', tombstone_expires_at = ?, quota_held = 1 WHERE attempt_id = ? AND state IN ('receiving', 'complete') AND object_key IS NOT NULL",
            )
            .bind_refs([&tombstone_value, &attempt_id_value])?
            .run()
            .await?;
        Ok(())
    }

    async fn release_unwritten_attempt(&self, attempt_id: &str) -> RepositoryResult<()> {
        let attempt_id = D1Type::Text(attempt_id);
        self.database
            .prepare(
                "DELETE FROM v2_document_stage_attempts WHERE attempt_id = ? AND state = 'receiving' AND object_key IS NULL",
            )
            .bind_refs([&attempt_id])?
            .run()
            .await?;
        Ok(())
    }

    async fn database_now(&self) -> RepositoryResult<i64> {
        self.database
            .prepare("SELECT CAST(strftime('%s', 'now') AS INTEGER) AS now_seconds")
            .first::<i64>(Some("now_seconds"))
            .await?
            .ok_or_else(|| Error::RustError("database time could not be read".to_owned()).into())
    }

    async fn allocate_sequence(&self) -> RepositoryResult<String> {
        self.capacity
            .ok_or(RepositoryError::StageControlUnavailable)?;
        for _ in 0..R2_SEQUENCE_RETRIES {
            let (mut current, etag) = self.read_sequence_control().await?;
            let next = current.allocate_sequence()?;
            let encoded = serde_json::to_vec(&current).map_err(|_| RepositoryError::InvalidJson)?;
            if conditional_r2_put(self.bucket, R2_SEQUENCE_KEY, &encoded, r2_if_match(&etag))
                .await
                .map_err(|_| RepositoryError::StageControlUnavailable)?
            {
                return Ok(next);
            }
        }
        Err(RepositoryError::StageControlUnavailable)
    }

    async fn read_sequence_control(&self) -> RepositoryResult<(StageControl, String)> {
        let object = self
            .bucket
            .get(R2_SEQUENCE_KEY)
            .execute()
            .await
            .map_err(|_| RepositoryError::StageControlUnavailable)?
            .ok_or(RepositoryError::StageControlUnavailable)?;
        let etag = object.http_etag();
        if etag.is_empty() {
            return Err(RepositoryError::StageControlUnavailable);
        }
        let body = object
            .body()
            .ok_or(RepositoryError::StageControlUnavailable)?
            .text()
            .await
            .map_err(|_| RepositoryError::StageControlUnavailable)?;
        let control = serde_json::from_str::<StageControl>(&body)
            .map_err(|_| RepositoryError::StageControlUnavailable)?;
        if !control.validate() {
            return Err(RepositoryError::StageControlUnavailable);
        }
        Ok((control, etag))
    }

    async fn active_receipt(
        &self,
        idempotency_key: &str,
    ) -> RepositoryResult<Option<StageReceipt>> {
        let key = D1Type::Text(idempotency_key);
        self.database
            .prepare(
                "SELECT command_id, payload_json, result_json FROM command_receipts WHERE idempotency_key = ? AND expires_at > CAST(strftime('%s', 'now') AS INTEGER)",
            )
            .bind_refs([&key])?
            .first::<StageReceipt>(None)
            .await
            .map_err(Into::into)
    }

    async fn classify_mint_failure(
        &self,
        mutation: &StageMintMutation<'_>,
    ) -> RepositoryResult<()> {
        let funnel_id = D1Type::Text(mutation.funnel_id);
        let base_revision_id = D1Type::Text(mutation.base_revision_id);
        let state = self
            .database
            .prepare(
                "SELECT EXISTS(SELECT 1 FROM funnels WHERE id = ?) AS funnel_exists, EXISTS(SELECT 1 FROM funnel_editor_heads WHERE funnel_id = ? AND revision_id = ?) AS base_matches, EXISTS(SELECT 1 FROM v2_document_stages WHERE funnel_id = ? AND state IN ('issued', 'uploaded')) AS active_stage_exists, (SELECT COUNT(*) FROM v2_document_stages WHERE slot_held = 1) AS held_slots",
            )
            .bind_refs([
                &funnel_id,
                &funnel_id,
                &base_revision_id,
                &funnel_id,
            ])?
            .first::<StageAdmissionState>(None)
            .await?
            .ok_or_else(|| Error::RustError("document stage admission could not be read".to_owned()))?;
        if state.funnel_exists == 0 {
            return Err(RepositoryError::FunnelNotFound);
        }
        if state.base_matches == 0 {
            return Err(RepositoryError::StaleEditorRevision);
        }
        if state.active_stage_exists != 0 {
            return Err(RepositoryError::ActiveStageExists);
        }
        if state.held_slots >= i64::from(V2_DOCUMENT_STAGE_SLOT_LIMIT) {
            return Err(RepositoryError::StageCapacityReached);
        }
        Ok(())
    }
}

fn validate_mint(mutation: &StageMintMutation<'_>) -> RepositoryResult<()> {
    if mutation.expected_length == 0 || mutation.expected_length > V2_DOCUMENT_STAGE_MAX_BYTES {
        return Err(RepositoryError::InvalidField("expected_length"));
    }
    if !is_lower_hex_digest(mutation.expected_sha256) {
        return Err(RepositoryError::InvalidField("expected_sha256"));
    }
    if mutation.command_id != STAGE_MINT_COMMAND_ID {
        return Err(RepositoryError::InvalidField("command_id"));
    }
    Ok(())
}

fn validate_finalize(mutation: &StageFinalizeMutation<'_>) -> RepositoryResult<()> {
    if mutation.revision.data_json.len() != mutation.expected_length
        || sha256_hex(mutation.revision.data_json.as_bytes()) != mutation.expected_sha256
    {
        return Err(RepositoryError::StageDigestMismatch);
    }
    if mutation.command_id != "funnel.revision.save.v2"
        || !is_stage_id(mutation.stage_id)
        || !is_lower_hex_digest(mutation.expected_sha256)
        || mutation.expected_length == 0
        || mutation.expected_length > V2_DOCUMENT_STAGE_MAX_BYTES
        || mutation.revision.funnel_id != mutation.funnel_id
        || mutation.revision.schema_version != 2
        || mutation.revision.data_json.len() > eez_domain::document_v2::MAX_PERSISTED_DOCUMENT_BYTES
        || mutation.revision.render_manifest_json.len() > 16_384
        || mutation.revision.data_json.len()
            + mutation.revision.render_manifest_json.len()
            + mutation.revision.id.len()
            + mutation.revision.funnel_id.len()
            + 32
            > MAX_V2_REVISION_ROW_BYTES
        || mutation.payload_json.len() > 16_384
        || mutation.result_json.len() > 16_384
        || !valid_idempotency_key(mutation.idempotency_key)
        || serde_json::from_str::<Value>(mutation.payload_json).is_err()
        || serde_json::from_str::<Value>(mutation.result_json).is_err()
    {
        return Err(RepositoryError::InvalidField("v2_finalize"));
    }
    Ok(())
}

fn is_live_uploaded_retry(
    stage: &StageUploadRecord,
    now: i64,
    length: usize,
    digest: &str,
) -> bool {
    stage.state == "uploaded"
        && stage.expires_at > now
        && stage.uploaded_length == Some(length as i64)
        && stage.uploaded_sha256.as_deref() == Some(digest)
}

fn stage_quota_fits(
    held_bytes: i64,
    held_keys: i64,
    expected_bytes: u64,
    capacity: StageCapacity,
) -> bool {
    let (Ok(held_bytes), Ok(held_keys)) = (u64::try_from(held_bytes), u64::try_from(held_keys))
    else {
        return false;
    };
    held_bytes
        .checked_add(expected_bytes)
        .is_some_and(|total| total <= capacity.orphan_byte_budget)
        && held_keys
            .checked_add(1)
            .is_some_and(|total| total <= capacity.orphan_key_budget)
}

fn valid_idempotency_key(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn classify_receipt(receipt: StageReceipt, mutation: &StageMintMutation<'_>) -> MutationResult {
    if receipt.command_id == mutation.command_id && receipt.payload_json == mutation.payload_json {
        MutationResult::Stored(receipt.result_json)
    } else {
        MutationResult::PayloadMismatch
    }
}

fn classify_finalize_receipt(
    receipt: StageReceipt,
    mutation: &StageFinalizeMutation<'_>,
) -> MutationResult {
    if receipt.command_id == mutation.command_id && receipt.payload_json == mutation.payload_json {
        MutationResult::Stored(receipt.result_json)
    } else {
        MutationResult::PayloadMismatch
    }
}

fn random_stage_id() -> RepositoryResult<String> {
    use worker::wasm_bindgen::{JsCast, JsValue};

    let global = worker::js_sys::global();
    let crypto = worker::js_sys::Reflect::get(&global, &JsValue::from_str("crypto"))
        .map_err(|_| RepositoryError::StageIdGenerationFailed)?;
    let get_random_values =
        worker::js_sys::Reflect::get(&crypto, &JsValue::from_str("getRandomValues"))
            .map_err(|_| RepositoryError::StageIdGenerationFailed)?;
    let get_random_values = get_random_values
        .dyn_into::<worker::js_sys::Function>()
        .map_err(|_| RepositoryError::StageIdGenerationFailed)?;
    let bytes = worker::js_sys::Uint8Array::new_with_length(32);
    get_random_values
        .call1(&crypto, bytes.as_ref())
        .map_err(|_| RepositoryError::StageIdGenerationFailed)?;
    let mut raw = [0_u8; 32];
    bytes.copy_to(&mut raw);
    Ok(encode_hex(&raw))
}

fn sha256_hex(bytes: &[u8]) -> String {
    encode_hex(&Sha256::digest(bytes))
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn is_lower_hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_stage_id(value: &str) -> bool {
    is_lower_hex_digest(value)
}

fn is_sequence(value: &str) -> bool {
    value.len() == 20 && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn increment_sequence(value: &str) -> Option<String> {
    if !is_sequence(value) {
        return None;
    }
    let mut digits = value.as_bytes().to_vec();
    for digit in digits.iter_mut().rev() {
        if *digit < b'9' {
            *digit += 1;
            return String::from_utf8(digits).ok();
        }
        *digit = b'0';
    }
    None
}

fn stage_object_key(sequence: &str, attempt_id: &str) -> Option<String> {
    (is_sequence(sequence) && is_stage_id(attempt_id))
        .then(|| format!("{R2_STAGE_PREFIX}{sequence}/{attempt_id}"))
}

fn stage_sequence_from_key(key: &str) -> Option<String> {
    let suffix = key.strip_prefix(R2_STAGE_PREFIX)?;
    let (sequence, attempt_id) = suffix.split_once('/')?;
    (is_sequence(sequence) && is_stage_id(attempt_id) && !attempt_id.contains('/'))
        .then(|| sequence.to_owned())
}

async fn conditional_r2_put(
    bucket: &Bucket,
    key: &str,
    bytes: &[u8],
    condition: Conditional,
) -> worker::Result<bool> {
    bucket
        .put(key.to_owned(), bytes.to_vec())
        .only_if(condition)
        .execute()
        .await
        .map(|result| result.is_some())
}

fn r2_if_match(etag: &str) -> Conditional {
    Conditional {
        etag_matches: Some(etag.to_owned()),
        ..Conditional::default()
    }
}

fn r2_if_absent() -> Conditional {
    Conditional {
        etag_does_not_match: Some("*".to_owned()),
        ..Conditional::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        encode_hex, increment_sequence, is_live_uploaded_retry, is_lower_hex_digest, is_sequence,
        r2_if_absent, r2_if_match, sha256_hex, stage_object_key, stage_quota_fits,
        stage_sequence_from_key, validate_finalize, StageCapacity, StageControl,
        StageFinalizeMutation, StageUploadRecord,
    };
    use crate::repository::{NewRevision, RepositoryError};

    fn uploaded_stage(
        expires_at: i64,
        uploaded_length: i64,
        uploaded_sha256: &str,
    ) -> StageUploadRecord {
        StageUploadRecord {
            stage_id_hash: "a".repeat(64),
            funnel_id: "funnel-a".to_owned(),
            base_revision_id: "revision-a".to_owned(),
            purpose: "full_snapshot".to_owned(),
            document_type: "eezfunnel.document".to_owned(),
            document_version: 2,
            schema_version: 2,
            manifest_version: 2,
            maximum_bytes: super::V2_DOCUMENT_STAGE_MAX_BYTES as i64,
            expected_length: uploaded_length,
            expected_sha256: uploaded_sha256.to_owned(),
            expires_at,
            state: "uploaded".to_owned(),
            object_key: Some("private/stage/00000000000000000001/attempt".to_owned()),
            uploaded_length: Some(uploaded_length),
            uploaded_sha256: Some(uploaded_sha256.to_owned()),
        }
    }

    #[test]
    fn encodes_stage_tokens_and_hashes_as_lowercase_hex() {
        assert_eq!(encode_hex(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(is_lower_hex_digest(&"a".repeat(64)));
        assert!(!is_lower_hex_digest(&"A".repeat(64)));
        assert!(!is_lower_hex_digest(&"a".repeat(63)));
    }

    #[test]
    fn sequences_are_fixed_width_and_never_roll_back_or_wrap() {
        assert!(is_sequence("00000000000000000000"));
        assert_eq!(
            increment_sequence("00000000000000000000").as_deref(),
            Some("00000000000000000001")
        );
        assert_eq!(
            increment_sequence("00000000000000000099").as_deref(),
            Some("00000000000000000100")
        );
        assert_eq!(increment_sequence("99999999999999999999"), None);
        assert_eq!(increment_sequence("1"), None);
        assert!(!is_sequence("0000000000000000000x"));
    }

    #[test]
    fn stage_object_keys_are_bound_to_the_fixed_width_sequence_and_unique_attempt() {
        let attempt_id = "ab".repeat(32);
        assert_eq!(
            stage_object_key("00000000000000000007", &attempt_id).as_deref(),
            Some(format!("private/stage/00000000000000000007/{attempt_id}").as_str())
        );
        assert_eq!(stage_object_key("7", &attempt_id), None);
        assert_eq!(stage_object_key("00000000000000000007", "bad"), None);
    }

    #[test]
    fn stage_sweep_parses_only_generated_keys_and_compares_fixed_width_watermarks() {
        let attempt_id = "ab".repeat(32);
        let key = format!("private/stage/00000000000000000007/{attempt_id}");
        assert_eq!(
            stage_sequence_from_key(&key).as_deref(),
            Some("00000000000000000007")
        );
        assert_eq!(
            stage_sequence_from_key(&format!("private/stage/0000000000000000007/{attempt_id}")),
            None
        );
        assert_eq!(
            stage_sequence_from_key("private/stage-control/sequence-v1"),
            None
        );
        assert!("00000000000000000008" > "00000000000000000007");
    }

    #[test]
    fn stage_quota_tracks_only_live_attempt_reservations() {
        let capacity = StageCapacity {
            orphan_byte_budget: 4,
            orphan_key_budget: 2,
        };
        assert!(stage_quota_fits(0, 0, 4, capacity));
        assert!(!stage_quota_fits(0, 0, 5, capacity));
        assert!(!stage_quota_fits(4, 0, 1, capacity));
        assert!(!stage_quota_fits(0, 2, 1, capacity));
        assert!(stage_quota_fits(0, 0, 1, capacity));
    }

    #[test]
    fn operator_capacity_changes_only_increase_and_control_state_is_validated() {
        let initial = StageCapacity {
            orphan_byte_budget: 8,
            orphan_key_budget: 3,
        };
        let increased = StageCapacity {
            orphan_byte_budget: 16,
            orphan_key_budget: 5,
        };
        let mut control = StageControl::initial(initial);
        assert!(control.validate());
        assert!(control.increase_capacity(increased).unwrap());
        assert_eq!(control.orphan_byte_budget, 16);
        assert_eq!(control.orphan_key_budget, 5);
        assert!(control.validate());
        assert!(matches!(
            control.increase_capacity(initial),
            Err(RepositoryError::StageControlUnavailable)
        ));
        control.sequence = "not-a-sequence".to_owned();
        assert!(!control.validate());
    }

    #[test]
    fn uploaded_stage_retries_require_matching_content_before_expiry() {
        let digest = sha256_hex(b"document");
        let live = uploaded_stage(11, 8, &digest);
        let expired = uploaded_stage(10, 8, &digest);

        assert!(is_live_uploaded_retry(&live, 10, 8, &digest));
        assert!(!is_live_uploaded_retry(&expired, 10, 8, &digest));
        assert!(!is_live_uploaded_retry(&live, 10, 7, &digest));
        assert!(!is_live_uploaded_retry(&live, 10, 8, &"b".repeat(64)));
    }

    #[test]
    fn sequence_writes_use_r2_conditional_etag_fields() {
        let matches = r2_if_match("etag-1");
        assert_eq!(matches.etag_matches.as_deref(), Some("etag-1"));
        assert_eq!(matches.etag_does_not_match, None);

        let absent = r2_if_absent();
        assert_eq!(absent.etag_matches, None);
        assert_eq!(absent.etag_does_not_match.as_deref(), Some("*"));
    }

    #[test]
    fn staged_revision_bytes_must_match_the_uploaded_digest() {
        let data_json = r#"{"format_version":2}"#;
        let expected_sha256 = sha256_hex(data_json.as_bytes());
        let revision = NewRevision {
            id: "revision-v2".to_owned(),
            funnel_id: "funnel-a".to_owned(),
            schema_version: 2,
            data_json: data_json.to_owned(),
            render_manifest_json: "{}".to_owned(),
        };
        let stage_id = "a".repeat(64);
        let mutation = StageFinalizeMutation {
            funnel_id: "funnel-a",
            base_revision_id: "revision-a",
            stage_id: &stage_id,
            expected_length: data_json.len(),
            expected_sha256: &expected_sha256,
            command_id: "funnel.revision.save.v2",
            idempotency_key: "finalize-v2",
            payload_json: "{}",
            result_json: "{}",
            revision: &revision,
        };

        assert!(validate_finalize(&mutation).is_ok());

        let wrong_digest = "b".repeat(64);
        let mismatched = StageFinalizeMutation {
            expected_sha256: &wrong_digest,
            ..mutation
        };
        assert!(matches!(
            validate_finalize(&mismatched),
            Err(RepositoryError::StageDigestMismatch)
        ));
    }
}
