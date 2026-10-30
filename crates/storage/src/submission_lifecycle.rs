use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use worker::d1::D1Type;
use worker::{Bucket, Conditional, D1Database, Error};

use crate::repository::RepositoryError;

const JOURNAL_PREFIX: &str = "private/submission-erasure/v1/";
const CONTROL_KEY: &str = "private/submission-erasure-control/v1/recovery";
const JOURNAL_PAGE_SIZE: u32 = 20;
const JOURNAL_MAX_BYTES: usize = 2_048;
const PENDING_PAGE_SIZE: i32 = 16;
const MARKER_CLEANUP_PAGE_SIZE: i32 = 1_000;
const UPSERT_REPLAY_MARKER_SQL: &str = "INSERT INTO public_submission_erasure_markers (replay_digest, expires_at) SELECT ?, ? WHERE ? > unixepoch() ON CONFLICT(replay_digest) DO UPDATE SET expires_at = MAX(public_submission_erasure_markers.expires_at, excluded.expires_at)";
const UPSERT_REPLAY_MARKER_FROM_INTENT_SQL: &str = "INSERT INTO public_submission_erasure_markers (replay_digest, expires_at) SELECT replay_digest, source_expires_at FROM public_submission_erasure_intents WHERE submission_id = ? AND replay_digest = ? AND source_expires_at = ? AND source_expires_at > unixepoch() ON CONFLICT(replay_digest) DO UPDATE SET expires_at = MAX(public_submission_erasure_markers.expires_at, excluded.expires_at)";

pub type LifecycleResult<T> = std::result::Result<T, LifecycleError>;

#[derive(Debug)]
pub enum LifecycleError {
    Database(Error),
    Unavailable,
    Conflict,
    InvalidInput,
}

impl From<Error> for LifecycleError {
    fn from(error: Error) -> Self {
        Self::Database(error)
    }
}

impl From<RepositoryError> for LifecycleError {
    fn from(error: RepositoryError) -> Self {
        match error {
            RepositoryError::Database(error) => Self::Database(error),
            _ => Self::Unavailable,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubmissionExportRecord {
    pub submission_id: String,
    pub funnel_id: String,
    pub revision_id: String,
    pub page_id: String,
    pub form_id: String,
    pub submission_channel: String,
    pub triage_status: String,
    pub field_values_json: String,
    pub request_consent_acknowledged: i64,
    pub request_consent_copy_version: String,
    pub request_consent_copy: String,
    pub request_consent_recorded_at: i64,
    pub marketing_opt_in: i64,
    pub marketing_opt_in_copy_version: Option<String>,
    pub marketing_opt_in_copy: Option<String>,
    pub marketing_opt_in_recorded_at: Option<i64>,
    pub submitted_at: i64,
    pub expires_at: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EraseResult {
    Erased,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryPhase {
    Ready,
    Preparing,
    AwaitingRestore,
    DrainingRestoredIntents,
    Reconciling,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RecoveryStatus {
    pub generation: String,
    pub phase: RecoveryPhase,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MaintenanceProgress {
    pub attempted: u32,
    pub completed: u32,
    pub failed: u32,
    pub pending: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JournalCleanupProgress {
    pub listed: u32,
    pub deleted: u32,
    pub pass_completed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ErasureJournalV1 {
    version: u8,
    submission_id: String,
    replay_digest: String,
    source_expires_at: i64,
    deleted_at: i64,
    retain_until: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ErasureJournalFileV1 {
    version: u8,
    submission_id: String,
    replay_digest: String,
    source_expires_at: i64,
    deleted_at: i64,
    retain_until: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct RecoveryControlV1 {
    version: u8,
    generation: String,
    phase: RecoveryPhase,
    reconcile_cursor: Option<String>,
    cleanup_cursor: Option<String>,
}

struct LoadedControl {
    control: RecoveryControlV1,
    etag: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ErasureIntent {
    submission_id: String,
    replay_digest: String,
    source_expires_at: i64,
    deleted_at: i64,
    retain_until: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct ErasureSource {
    funnel_id: String,
    form_id: String,
    submission_key: String,
    expires_at: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct RestoreState {
    funnel_id: Option<String>,
    form_id: Option<String>,
    submission_key: Option<String>,
    expires_at: Option<i64>,
    intent_digest: Option<String>,
    intent_source_expires_at: Option<i64>,
}

pub struct SubmissionLifecycleRepository<'a> {
    database: &'a D1Database,
    bucket: &'a Bucket,
}

impl<'a> SubmissionLifecycleRepository<'a> {
    pub fn new(database: &'a D1Database, bucket: &'a Bucket) -> Self {
        Self { database, bucket }
    }

    pub async fn recovery_status(&self) -> LifecycleResult<RecoveryStatus> {
        let control = self.read_control().await?;
        Ok(RecoveryStatus {
            generation: control.control.generation,
            phase: control.control.phase,
        })
    }

    pub async fn require_recovery_ready(&self) -> LifecycleResult<()> {
        if self.read_control().await?.control.phase == RecoveryPhase::Ready {
            Ok(())
        } else {
            Err(LifecycleError::Unavailable)
        }
    }

    pub async fn retained_export(
        &self,
        submission_id: &str,
    ) -> LifecycleResult<Option<SubmissionExportRecord>> {
        self.require_recovery_ready().await?;
        validate_submission_id(submission_id)?;
        let id = D1Type::Text(submission_id);
        self.database
            .prepare(
                "SELECT source.submission_id, source.funnel_id, source.revision_id, source.page_id, source.form_id, source.submission_channel, source.triage_status, source.field_values_json, source.request_consent_acknowledged, source.request_consent_copy_version, source.request_consent_copy, source.request_consent_recorded_at, source.marketing_opt_in, source.marketing_opt_in_copy_version, source.marketing_opt_in_copy, source.marketing_opt_in_recorded_at, source.submitted_at, source.expires_at FROM public_form_submissions AS source WHERE source.submission_id = ? AND source.expires_at > unixepoch() AND NOT EXISTS (SELECT 1 FROM public_submission_erasure_intents AS intent WHERE intent.submission_id = source.submission_id)",
            )
            .bind_refs([&id])?
            .first::<SubmissionExportRecord>(None)
            .await
            .map_err(Into::into)
    }

    pub async fn begin_erasure(
        &self,
        submission_id: &str,
        recovery_horizon_days: u16,
    ) -> LifecycleResult<Option<ErasureJournalV1>> {
        validate_submission_id(submission_id)?;
        if !(1..=3_650).contains(&recovery_horizon_days) {
            return Err(LifecycleError::InvalidInput);
        }
        self.require_recovery_ready().await?;

        if let Some(existing) = self.load_intent(submission_id).await? {
            self.suppress_delivery_for_intent(&existing).await?;
            self.ensure_replay_marker(&existing).await?;
            return Ok(Some(existing.into()));
        }

        let id = D1Type::Text(submission_id);
        let source = self
            .database
            .prepare(
                "SELECT funnel_id, form_id, submission_key, expires_at FROM public_form_submissions WHERE submission_id = ? AND expires_at > unixepoch()",
            )
            .bind_refs([&id])?
            .first::<ErasureSource>(None)
            .await?;
        let Some(source) = source else {
            let key = journal_key(submission_id)?;
            if let Some(journal) = self.read_journal_key_if_exists(&key).await? {
                self.ensure_replay_marker_from_journal(&journal).await?;
                return Ok(Some(journal));
            }
            return Ok(None);
        };
        let digest = replay_digest(&source.funnel_id, &source.form_id, &source.submission_key)?;
        let digest_value = D1Type::Text(&digest);
        let horizon_seconds = D1Type::Integer(
            i32::from(recovery_horizon_days)
                .checked_mul(86_400)
                .ok_or(LifecycleError::InvalidInput)?,
        );
        let expires_at = D1Type::Real(source.expires_at as f64);

        self.database
            .batch(vec![
                self.database
                    .prepare(
                        "INSERT OR IGNORE INTO public_submission_erasure_intents (submission_id, replay_digest, source_expires_at, deleted_at, retain_until) VALUES (?, ?, ?, unixepoch(), MAX(?, unixepoch() + ?))",
                    )
                    .bind_refs([
                        &id,
                        &digest_value,
                        &expires_at,
                        &expires_at,
                        &horizon_seconds,
                    ])?,
                self.database
                    .prepare(UPSERT_REPLAY_MARKER_FROM_INTENT_SQL)
                    .bind_refs([&id, &digest_value, &expires_at])?,
                self.database
                    .prepare(
                        "DELETE FROM public_submission_delivery_outbox WHERE source_submission_id = ? AND EXISTS (SELECT 1 FROM public_submission_erasure_intents WHERE submission_id = ? AND replay_digest = ? AND source_expires_at = ?)",
                    )
                    .bind_refs([&id, &id, &digest_value, &expires_at])?,
            ])
            .await?;

        let intent = self.load_intent(submission_id).await?;
        if let Some(intent) = intent {
            if intent.replay_digest != digest || intent.source_expires_at != source.expires_at {
                return Err(LifecycleError::Conflict);
            }
            self.ensure_replay_marker(&intent).await?;
            return Ok(Some(intent.into()));
        }
        Err(LifecycleError::Unavailable)
    }

    pub async fn maintain_pending(&self, budget: u32) -> LifecycleResult<MaintenanceProgress> {
        let limit = budget.min(PENDING_PAGE_SIZE as u32) as i32;
        let mut progress = MaintenanceProgress::default();
        if limit == 0 {
            progress.pending = self.pending_count().await?;
            return Ok(progress);
        }
        let limit_value = D1Type::Integer(limit);
        let intents = self
            .database
            .prepare(
                "SELECT submission_id, replay_digest, source_expires_at, deleted_at, retain_until FROM public_submission_erasure_intents ORDER BY last_attempt_at, deleted_at, submission_id LIMIT ?",
            )
            .bind_refs([&limit_value])?
            .all()
            .await?
            .results::<ErasureIntent>()?;

        for intent in intents {
            progress.attempted += 1;
            let id = D1Type::Text(&intent.submission_id);
            self.database
                .prepare(
                    "UPDATE public_submission_erasure_intents SET last_attempt_at = MAX(CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER), last_attempt_at + 1) WHERE submission_id = ? AND replay_digest = ?",
                )
                .bind_refs([&id, &D1Type::Text(&intent.replay_digest)])?
                .run()
                .await?;
            let journal = ErasureJournalV1::from(intent);
            match self.ensure_journal(&journal).await {
                Ok(()) => match self.ensure_replay_marker_from_journal(&journal).await {
                    Ok(()) => match self.finish_erasure(&journal).await {
                        Ok(()) => progress.completed += 1,
                        Err(_) => progress.failed += 1,
                    },
                    Err(_) => progress.failed += 1,
                },
                Err(_) => progress.failed += 1,
            }
        }
        progress.pending = self.pending_count().await?;
        Ok(progress)
    }

    pub async fn begin_recovery(&self) -> LifecycleResult<RecoveryStatus> {
        for _ in 0..8 {
            let loaded = self.read_control().await?;
            match loaded.control.phase {
                RecoveryPhase::Ready => {
                    let mut next = loaded.control;
                    next.phase = RecoveryPhase::Preparing;
                    next.generation = uuid::Uuid::new_v4().to_string();
                    next.reconcile_cursor = None;
                    if self.write_control(&loaded.etag, &next).await? {
                        return Ok(status_from_control(next));
                    }
                }
                RecoveryPhase::Preparing
                | RecoveryPhase::AwaitingRestore
                | RecoveryPhase::DrainingRestoredIntents
                | RecoveryPhase::Reconciling => return Ok(status_from_control(loaded.control)),
            }
        }
        Err(LifecycleError::Unavailable)
    }

    pub async fn prepare_recovery_step(
        &self,
        generation: &str,
    ) -> LifecycleResult<(RecoveryStatus, MaintenanceProgress)> {
        validate_generation(generation)?;
        let loaded = self.read_control().await?;
        if loaded.control.generation != generation
            || loaded.control.phase != RecoveryPhase::Preparing
        {
            return Err(LifecycleError::Conflict);
        }
        let progress = self.maintain_pending(PENDING_PAGE_SIZE as u32).await?;
        if progress.pending > 0 {
            return Ok((status_from_control(loaded.control), progress));
        }
        let mut next = loaded.control;
        next.phase = RecoveryPhase::AwaitingRestore;
        next.reconcile_cursor = None;
        if !self.write_control(&loaded.etag, &next).await? {
            return Err(LifecycleError::Unavailable);
        }
        Ok((status_from_control(next), progress))
    }

    pub async fn reconcile_after_restore(
        &self,
        generation: &str,
    ) -> LifecycleResult<(RecoveryStatus, u32)> {
        validate_generation(generation)?;
        let mut loaded = self.read_control().await?;
        if loaded.control.generation != generation {
            return Err(LifecycleError::Conflict);
        }
        if loaded.control.phase == RecoveryPhase::Ready {
            return Ok((status_from_control(loaded.control), 0));
        }
        if loaded.control.phase == RecoveryPhase::AwaitingRestore {
            let mut next = loaded.control;
            next.phase = RecoveryPhase::DrainingRestoredIntents;
            next.reconcile_cursor = None;
            if !self.write_control(&loaded.etag, &next).await? {
                return Err(LifecycleError::Unavailable);
            }
            loaded = self.read_control().await?;
            if loaded.control.generation != generation {
                return Err(LifecycleError::Conflict);
            }
        }
        if loaded.control.phase == RecoveryPhase::Ready {
            return Ok((status_from_control(loaded.control), 0));
        }
        if !matches!(
            loaded.control.phase,
            RecoveryPhase::DrainingRestoredIntents | RecoveryPhase::Reconciling
        ) {
            return Err(LifecycleError::Conflict);
        }
        if loaded.control.phase == RecoveryPhase::DrainingRestoredIntents {
            let pending = self.maintain_pending(PENDING_PAGE_SIZE as u32).await?;
            if pending.pending > 0 {
                return Ok((status_from_control(loaded.control), pending.attempted));
            }
            let mut next = loaded.control;
            next.phase = RecoveryPhase::Reconciling;
            next.reconcile_cursor = None;
            if !self.write_control(&loaded.etag, &next).await? {
                return Err(LifecycleError::Unavailable);
            }
            loaded = self.read_control().await?;
            if loaded.control.generation != generation {
                return Err(LifecycleError::Conflict);
            }
        }
        if loaded.control.phase == RecoveryPhase::Ready {
            return Ok((status_from_control(loaded.control), 0));
        }
        self.reconcile_page(loaded, generation).await
    }

    pub async fn cleanup_expired_markers(&self) -> LifecycleResult<u32> {
        self.require_recovery_ready().await?;
        let limit = D1Type::Integer(MARKER_CLEANUP_PAGE_SIZE);
        let result = self
            .database
            .prepare(
                "DELETE FROM public_submission_erasure_markers WHERE replay_digest IN (SELECT replay_digest FROM public_submission_erasure_markers WHERE expires_at <= unixepoch() ORDER BY expires_at, replay_digest LIMIT ?)",
            )
            .bind_refs([&limit])?
            .run()
            .await?;
        Ok(result.meta()?.and_then(|meta| meta.changes).unwrap_or(0) as u32)
    }

    pub async fn cleanup_journal_page(&self) -> LifecycleResult<JournalCleanupProgress> {
        let loaded = self.read_control().await?;
        if loaded.control.phase != RecoveryPhase::Ready {
            return Ok(JournalCleanupProgress::default());
        }
        let mut list = self
            .bucket
            .list()
            .prefix(JOURNAL_PREFIX)
            .limit(JOURNAL_PAGE_SIZE);
        if let Some(cursor) = loaded.control.cleanup_cursor.as_deref() {
            list = list.cursor(cursor);
        }
        let page = list
            .execute()
            .await
            .map_err(|_| LifecycleError::Unavailable)?;
        let truncated = page.truncated();
        let next_cursor = page.cursor();
        let mut progress = JournalCleanupProgress {
            listed: 0,
            deleted: 0,
            pass_completed: !truncated,
        };
        let now = self.database_now().await?;
        for object in page.objects() {
            progress.listed += 1;
            let key = object.key();
            let journal = self.read_journal_key(&key).await?;
            if journal.retain_until <= now {
                self.bucket
                    .delete(key)
                    .await
                    .map_err(|_| LifecycleError::Unavailable)?;
                progress.deleted += 1;
            }
        }
        if truncated
            && (next_cursor.is_none()
                || next_cursor.as_deref() == loaded.control.cleanup_cursor.as_deref())
        {
            return Err(LifecycleError::Unavailable);
        }
        let mut next = loaded.control;
        next.cleanup_cursor = if truncated { next_cursor } else { None };
        if !self.write_control(&loaded.etag, &next).await? {
            return Ok(JournalCleanupProgress::default());
        }
        Ok(progress)
    }

    async fn reconcile_page(
        &self,
        loaded: LoadedControl,
        generation: &str,
    ) -> LifecycleResult<(RecoveryStatus, u32)> {
        require_reconcile_control(&loaded.control, generation)?;
        let mut list = self
            .bucket
            .list()
            .prefix(JOURNAL_PREFIX)
            .limit(JOURNAL_PAGE_SIZE);
        if let Some(cursor) = loaded.control.reconcile_cursor.as_deref() {
            list = list.cursor(cursor);
        }
        let page = list
            .execute()
            .await
            .map_err(|_| LifecycleError::Unavailable)?;
        let truncated = page.truncated();
        let next_cursor = page.cursor();
        if truncated
            && (next_cursor.is_none()
                || next_cursor.as_deref() == loaded.control.reconcile_cursor.as_deref())
        {
            return Err(LifecycleError::Unavailable);
        }
        let mut applied = 0;
        for object in page.objects() {
            let key = object.key();
            let journal = self.read_journal_key(&key).await?;
            self.apply_restored_journal(&journal).await?;
            applied += 1;
        }
        if truncated {
            let mut next = loaded.control;
            next.reconcile_cursor = next_cursor;
            if !self.write_control(&loaded.etag, &next).await? {
                return Err(LifecycleError::Unavailable);
            }
            return Ok((status_from_control(next), applied));
        }
        if self.pending_count().await? > 0 {
            return Err(LifecycleError::Unavailable);
        }
        let mut next = loaded.control;
        next.phase = RecoveryPhase::Ready;
        next.reconcile_cursor = None;
        if !self.write_control(&loaded.etag, &next).await? {
            return Err(LifecycleError::Unavailable);
        }
        Ok((status_from_control(next), applied))
    }

    async fn apply_restored_journal(&self, journal: &ErasureJournalV1) -> LifecycleResult<()> {
        validate_journal(journal)?;
        let id = D1Type::Text(&journal.submission_id);
        let digest = D1Type::Text(&journal.replay_digest);
        let state = self
            .database
            .prepare(
                "SELECT source.funnel_id, source.form_id, source.submission_key, source.expires_at, intent.replay_digest AS intent_digest, intent.source_expires_at AS intent_source_expires_at FROM (SELECT ? AS submission_id) AS target LEFT JOIN public_form_submissions AS source ON source.submission_id = target.submission_id LEFT JOIN public_submission_erasure_intents AS intent ON intent.submission_id = target.submission_id",
            )
            .bind_refs([&id])?
            .first::<RestoreState>(None)
            .await?
            .ok_or(LifecycleError::Unavailable)?;

        if let Some(expires_at) = state.expires_at {
            let source_digest = replay_digest(
                state.funnel_id.as_deref().ok_or(LifecycleError::Conflict)?,
                state.form_id.as_deref().ok_or(LifecycleError::Conflict)?,
                state
                    .submission_key
                    .as_deref()
                    .ok_or(LifecycleError::Conflict)?,
            )?;
            if source_digest != journal.replay_digest || expires_at != journal.source_expires_at {
                return Err(LifecycleError::Conflict);
            }
        }
        match (state.intent_digest, state.intent_source_expires_at) {
            (Some(digest), Some(expiry))
                if digest == journal.replay_digest && expiry == journal.source_expires_at => {}
            (None, None) => {}
            _ => return Err(LifecycleError::Conflict),
        }
        let source_expiry = D1Type::Real(journal.source_expires_at as f64);
        self.database
            .batch(vec![
                self.database
                    .prepare(UPSERT_REPLAY_MARKER_SQL)
                    .bind_refs([&digest, &source_expiry, &source_expiry])?,
                self.database
                    .prepare("DELETE FROM public_form_submissions WHERE submission_id = ?")
                    .bind_refs([&id])?,
                self.database
                    .prepare(
                        "DELETE FROM public_submission_erasure_intents WHERE submission_id = ? AND replay_digest = ? AND source_expires_at = ?",
                    )
                    .bind_refs([&id, &digest, &source_expiry])?,
            ])
            .await?;
        Ok(())
    }

    pub async fn finish_erasure(&self, journal: &ErasureJournalV1) -> LifecycleResult<()> {
        validate_journal(journal)?;
        let source = self.load_source(&journal.submission_id).await?;
        if let Some(source) = source {
            let digest = replay_digest(&source.funnel_id, &source.form_id, &source.submission_key)?;
            if digest != journal.replay_digest || source.expires_at != journal.source_expires_at {
                return Err(LifecycleError::Conflict);
            }
        }
        let intent = self.load_intent(&journal.submission_id).await?;
        let Some(intent) = intent else {
            return Ok(());
        };
        if !intent_matches_journal_identity(&intent, journal) {
            return Err(LifecycleError::Conflict);
        }
        let id = D1Type::Text(&journal.submission_id);
        let digest = D1Type::Text(&journal.replay_digest);
        self.database
            .batch(vec![
                self.database
                    .prepare("DELETE FROM public_form_submissions WHERE submission_id = ?")
                    .bind_refs([&id])?,
                self.database
                    .prepare(
                        "DELETE FROM public_submission_erasure_intents WHERE submission_id = ? AND replay_digest = ? AND source_expires_at = ?",
                    )
                    .bind_refs([&id, &digest, &D1Type::Real(journal.source_expires_at as f64)])?,
            ])
            .await?;
        Ok(())
    }

    pub async fn ensure_journal(&self, journal: &ErasureJournalV1) -> LifecycleResult<()> {
        validate_journal(journal)?;
        let key = journal_key(&journal.submission_id)?;
        let bytes = serde_json::to_vec(journal).map_err(|_| LifecycleError::Unavailable)?;
        match self
            .bucket
            .put(key.clone(), bytes)
            .only_if(r2_if_absent())
            .execute()
            .await
        {
            Ok(Some(_)) => Ok(()),
            Ok(None) | Err(_) => {
                let existing = self.read_journal_key(&key).await?;
                if same_erasure_identity(&existing, journal) {
                    Ok(())
                } else {
                    Err(LifecycleError::Conflict)
                }
            }
        }
    }

    async fn read_journal_key(&self, key: &str) -> LifecycleResult<ErasureJournalV1> {
        self.read_journal_key_if_exists(key)
            .await?
            .ok_or(LifecycleError::Unavailable)
    }

    async fn read_journal_key_if_exists(
        &self,
        key: &str,
    ) -> LifecycleResult<Option<ErasureJournalV1>> {
        submission_id_from_journal_key(key).ok_or(LifecycleError::Unavailable)?;
        let Some(object) = self
            .bucket
            .get(key)
            .execute()
            .await
            .map_err(|_| LifecycleError::Unavailable)?
        else {
            return Ok(None);
        };
        if object.size() > JOURNAL_MAX_BYTES as u64 {
            return Err(LifecycleError::Unavailable);
        }
        let bytes = object
            .body()
            .ok_or(LifecycleError::Unavailable)?
            .bytes()
            .await
            .map_err(|_| LifecycleError::Unavailable)?;
        parse_journal_file(key, &bytes).map(Some)
    }

    async fn load_source(&self, submission_id: &str) -> LifecycleResult<Option<ErasureSource>> {
        let id = D1Type::Text(submission_id);
        self.database
            .prepare(
                "SELECT funnel_id, form_id, submission_key, expires_at FROM public_form_submissions WHERE submission_id = ?",
            )
            .bind_refs([&id])?
            .first::<ErasureSource>(None)
            .await
            .map_err(Into::into)
    }

    async fn load_intent(&self, submission_id: &str) -> LifecycleResult<Option<ErasureIntent>> {
        let id = D1Type::Text(submission_id);
        self.database
            .prepare(
                "SELECT submission_id, replay_digest, source_expires_at, deleted_at, retain_until FROM public_submission_erasure_intents WHERE submission_id = ?",
            )
            .bind_refs([&id])?
            .first::<ErasureIntent>(None)
            .await
            .map_err(Into::into)
    }

    async fn suppress_delivery_for_intent(&self, intent: &ErasureIntent) -> LifecycleResult<()> {
        let id = D1Type::Text(&intent.submission_id);
        let digest = D1Type::Text(&intent.replay_digest);
        let source_expires_at = D1Type::Real(intent.source_expires_at as f64);
        self.database
            .prepare(
                "DELETE FROM public_submission_delivery_outbox WHERE source_submission_id = ? AND EXISTS (SELECT 1 FROM public_submission_erasure_intents WHERE submission_id = ? AND replay_digest = ? AND source_expires_at = ?)",
            )
            .bind_refs([&id, &id, &digest, &source_expires_at])?
            .run()
            .await?;
        Ok(())
    }

    async fn marker_expiry(&self, digest: &str) -> LifecycleResult<Option<i64>> {
        let digest = D1Type::Text(digest);
        self.database
            .prepare(
                "SELECT expires_at FROM public_submission_erasure_markers WHERE replay_digest = ?",
            )
            .bind_refs([&digest])?
            .first::<i64>(Some("expires_at"))
            .await
            .map_err(Into::into)
    }

    async fn ensure_replay_marker(&self, intent: &ErasureIntent) -> LifecycleResult<()> {
        self.ensure_replay_marker_from_journal(&ErasureJournalV1::from(intent.clone()))
            .await
    }

    async fn ensure_replay_marker_from_journal(
        &self,
        journal: &ErasureJournalV1,
    ) -> LifecycleResult<()> {
        if journal.source_expires_at <= self.database_now().await? {
            return Ok(());
        }
        let digest = D1Type::Text(&journal.replay_digest);
        let expiry = D1Type::Real(journal.source_expires_at as f64);
        self.database
            .prepare(UPSERT_REPLAY_MARKER_SQL)
            .bind_refs([&digest, &expiry, &expiry])?
            .run()
            .await?;
        if self
            .marker_expiry(&journal.replay_digest)
            .await?
            .is_none_or(|stored_expiry| stored_expiry < journal.source_expires_at)
        {
            return Err(LifecycleError::Conflict);
        }
        Ok(())
    }

    async fn pending_count(&self) -> LifecycleResult<u64> {
        self.database
            .prepare("SELECT COUNT(*) AS count FROM public_submission_erasure_intents")
            .first::<u64>(Some("count"))
            .await
            .map_err(Into::into)
            .map(|count| count.unwrap_or(0))
    }

    async fn database_now(&self) -> LifecycleResult<i64> {
        self.database
            .prepare("SELECT CAST(strftime('%s', 'now') AS INTEGER) AS now_seconds")
            .first::<i64>(Some("now_seconds"))
            .await
            .map_err(Into::into)
            .and_then(|now| now.ok_or(LifecycleError::Unavailable))
    }

    async fn read_control(&self) -> LifecycleResult<LoadedControl> {
        if let Some(control) = self.load_control().await? {
            return Ok(control);
        }
        if self.has_erasure_history().await? {
            return Err(LifecycleError::Unavailable);
        }
        let initial = RecoveryControlV1 {
            version: 1,
            generation: uuid::Uuid::new_v4().to_string(),
            phase: RecoveryPhase::Ready,
            reconcile_cursor: None,
            cleanup_cursor: None,
        };
        let encoded = serde_json::to_vec(&initial).map_err(|_| LifecycleError::Unavailable)?;
        let _ = self
            .bucket
            .put(CONTROL_KEY.to_owned(), encoded)
            .only_if(r2_if_absent())
            .execute()
            .await;
        self.load_control()
            .await?
            .ok_or(LifecycleError::Unavailable)
    }

    async fn load_control(&self) -> LifecycleResult<Option<LoadedControl>> {
        let Some(object) = self
            .bucket
            .get(CONTROL_KEY)
            .execute()
            .await
            .map_err(|_| LifecycleError::Unavailable)?
        else {
            return Ok(None);
        };
        if object.size() > JOURNAL_MAX_BYTES as u64 {
            return Err(LifecycleError::Unavailable);
        }
        let etag = object.etag();
        if etag.is_empty() {
            return Err(LifecycleError::Unavailable);
        }
        let bytes = object
            .body()
            .ok_or(LifecycleError::Unavailable)?
            .bytes()
            .await
            .map_err(|_| LifecycleError::Unavailable)?;
        let control = parse_recovery_control(&bytes)?;
        Ok(Some(LoadedControl { control, etag }))
    }

    async fn write_control(
        &self,
        etag: &str,
        control: &RecoveryControlV1,
    ) -> LifecycleResult<bool> {
        let encoded = serde_json::to_vec(control).map_err(|_| LifecycleError::Unavailable)?;
        self.bucket
            .put(CONTROL_KEY.to_owned(), encoded)
            .only_if(r2_if_match(etag))
            .execute()
            .await
            .map(|result| result.is_some())
            .map_err(|_| LifecycleError::Unavailable)
    }

    async fn has_erasure_history(&self) -> LifecycleResult<bool> {
        let count = self
            .database
            .prepare(
                "SELECT (SELECT COUNT(*) FROM public_submission_erasure_intents) + (SELECT COUNT(*) FROM public_submission_erasure_markers) AS count",
            )
            .first::<u64>(Some("count"))
            .await?;
        if count.unwrap_or(1) > 0 {
            return Ok(true);
        }
        let page = self
            .bucket
            .list()
            .prefix(JOURNAL_PREFIX)
            .limit(1)
            .execute()
            .await
            .map_err(|_| LifecycleError::Unavailable)?;
        Ok(!page.objects().is_empty() || page.truncated())
    }
}

impl From<ErasureIntent> for ErasureJournalV1 {
    fn from(intent: ErasureIntent) -> Self {
        Self {
            version: 1,
            submission_id: intent.submission_id,
            replay_digest: intent.replay_digest,
            source_expires_at: intent.source_expires_at,
            deleted_at: intent.deleted_at,
            retain_until: intent.retain_until,
        }
    }
}

fn same_erasure_identity(left: &ErasureJournalV1, right: &ErasureJournalV1) -> bool {
    left.submission_id == right.submission_id
        && left.replay_digest == right.replay_digest
        && left.source_expires_at == right.source_expires_at
}

fn intent_matches_journal_identity(intent: &ErasureIntent, journal: &ErasureJournalV1) -> bool {
    intent.submission_id == journal.submission_id
        && intent.replay_digest == journal.replay_digest
        && intent.source_expires_at == journal.source_expires_at
}

fn status_from_control(control: RecoveryControlV1) -> RecoveryStatus {
    RecoveryStatus {
        generation: control.generation,
        phase: control.phase,
    }
}

fn require_reconcile_control(control: &RecoveryControlV1, generation: &str) -> LifecycleResult<()> {
    if control.generation == generation && control.phase == RecoveryPhase::Reconciling {
        Ok(())
    } else {
        Err(LifecycleError::Conflict)
    }
}

fn parse_journal_file(key: &str, bytes: &[u8]) -> LifecycleResult<ErasureJournalV1> {
    if bytes.len() > JOURNAL_MAX_BYTES {
        return Err(LifecycleError::Unavailable);
    }
    let key_id = submission_id_from_journal_key(key).ok_or(LifecycleError::Unavailable)?;
    let file = serde_json::from_slice::<ErasureJournalFileV1>(bytes)
        .map_err(|_| LifecycleError::Unavailable)?;
    let journal = ErasureJournalV1 {
        version: file.version,
        submission_id: file.submission_id,
        replay_digest: file.replay_digest,
        source_expires_at: file.source_expires_at,
        deleted_at: file.deleted_at,
        retain_until: file.retain_until,
    };
    validate_journal(&journal)?;
    if journal.submission_id != key_id {
        return Err(LifecycleError::Conflict);
    }
    Ok(journal)
}

fn validate_journal(journal: &ErasureJournalV1) -> LifecycleResult<()> {
    validate_submission_id(&journal.submission_id)?;
    if journal.version != 1
        || !is_lower_hex_digest(&journal.replay_digest)
        || journal.source_expires_at <= 0
        || journal.deleted_at <= 0
        || journal.retain_until < journal.source_expires_at
        || journal.retain_until < journal.deleted_at
    {
        return Err(LifecycleError::Unavailable);
    }
    Ok(())
}

fn parse_recovery_control(bytes: &[u8]) -> LifecycleResult<RecoveryControlV1> {
    if bytes.len() > JOURNAL_MAX_BYTES {
        return Err(LifecycleError::Unavailable);
    }
    let control = serde_json::from_slice::<RecoveryControlV1>(bytes)
        .map_err(|_| LifecycleError::Unavailable)?;
    if control.version != 1
        || validate_generation(&control.generation).is_err()
        || (control.phase != RecoveryPhase::Reconciling && control.reconcile_cursor.is_some())
        || control
            .reconcile_cursor
            .as_ref()
            .is_some_and(|cursor| cursor.is_empty())
        || control
            .cleanup_cursor
            .as_ref()
            .is_some_and(|cursor| cursor.is_empty())
    {
        return Err(LifecycleError::Unavailable);
    }
    Ok(control)
}

pub(crate) fn replay_digest(
    funnel_id: &str,
    form_id: &str,
    submission_key: &str,
) -> LifecycleResult<String> {
    let mut hasher = Sha256::new();
    hasher.update(b"eezfunnel-submission-erasure-v1\0");
    for value in [funnel_id, form_id, submission_key] {
        let bytes = value.as_bytes();
        let length = u32::try_from(bytes.len()).map_err(|_| LifecycleError::InvalidInput)?;
        hasher.update(length.to_be_bytes());
        hasher.update(bytes);
    }
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(encoded, "{byte:02x}").map_err(|_| LifecycleError::Unavailable)?;
    }
    Ok(encoded)
}

fn validate_submission_id(value: &str) -> LifecycleResult<()> {
    let id = uuid::Uuid::parse_str(value).map_err(|_| LifecycleError::InvalidInput)?;
    if id.hyphenated().to_string() != value {
        return Err(LifecycleError::InvalidInput);
    }
    Ok(())
}

fn validate_generation(value: &str) -> LifecycleResult<()> {
    validate_submission_id(value)
}

fn is_lower_hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn journal_key(submission_id: &str) -> LifecycleResult<String> {
    validate_submission_id(submission_id)?;
    Ok(format!("{JOURNAL_PREFIX}{submission_id}.json"))
}

fn submission_id_from_journal_key(key: &str) -> Option<String> {
    let suffix = key.strip_prefix(JOURNAL_PREFIX)?.strip_suffix(".json")?;
    validate_submission_id(suffix)
        .ok()
        .map(|_| suffix.to_owned())
}

fn r2_if_absent() -> Conditional {
    Conditional {
        etag_does_not_match: Some("*".to_owned()),
        ..Conditional::default()
    }
}

fn r2_if_match(etag: &str) -> Conditional {
    Conditional {
        etag_matches: Some(etag.to_owned()),
        ..Conditional::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        intent_matches_journal_identity, is_lower_hex_digest, journal_key, parse_journal_file,
        parse_recovery_control, replay_digest, require_reconcile_control, same_erasure_identity,
        submission_id_from_journal_key, validate_generation, ErasureIntent, ErasureJournalV1,
        LifecycleError, RecoveryControlV1, RecoveryPhase,
    };

    #[test]
    fn replay_identity_is_unambiguous_and_scoped_to_funnel_form_and_key() {
        assert_ne!(
            replay_digest("ab", "c", "d").unwrap(),
            replay_digest("a", "bc", "d").unwrap()
        );
        assert_ne!(
            replay_digest("funnel-a", "form-a", "key-a").unwrap(),
            replay_digest("funnel-b", "form-a", "key-a").unwrap()
        );
        assert_ne!(
            replay_digest("funnel-a", "form-a", "key-a").unwrap(),
            replay_digest("funnel-a", "form-b", "key-a").unwrap()
        );
        assert_ne!(
            replay_digest("funnel-a", "form-a", "key-a").unwrap(),
            replay_digest("funnel-a", "form-a", "key-b").unwrap()
        );
    }

    #[test]
    fn journal_object_names_require_canonical_submission_ids() {
        let id = "11111111-1111-4111-8111-111111111111";
        let key = journal_key(id).unwrap();
        assert_eq!(submission_id_from_journal_key(&key).as_deref(), Some(id));
        assert!(
            submission_id_from_journal_key("private/submission-erasure/v1/not-a-uuid.json")
                .is_none()
        );
        assert!(validate_generation("not-a-uuid").is_err());
    }

    #[test]
    fn erasure_digests_are_lowercase_sha256_hex() {
        assert!(is_lower_hex_digest(&"a".repeat(64)));
        assert!(!is_lower_hex_digest(&"A".repeat(64)));
        assert!(!is_lower_hex_digest(&"a".repeat(63)));
        assert!(!is_lower_hex_digest(&format!("{}g", "a".repeat(63))));
    }

    #[test]
    fn concurrent_erasure_retries_match_by_source_identity_and_keep_the_first_journal() {
        let first = ErasureJournalV1 {
            version: 1,
            submission_id: "11111111-1111-4111-8111-111111111111".to_owned(),
            replay_digest: "a".repeat(64),
            source_expires_at: 200,
            deleted_at: 100,
            retain_until: 2_592_100,
        };
        let delayed = ErasureJournalV1 {
            deleted_at: 101,
            retain_until: 3_456_101,
            ..first.clone()
        };
        let intent = ErasureIntent {
            submission_id: delayed.submission_id.clone(),
            replay_digest: delayed.replay_digest.clone(),
            source_expires_at: delayed.source_expires_at,
            deleted_at: delayed.deleted_at,
            retain_until: delayed.retain_until,
        };

        assert!(same_erasure_identity(&first, &delayed));
        assert!(intent_matches_journal_identity(&intent, &first));

        let wrong_digest = ErasureJournalV1 {
            replay_digest: "b".repeat(64),
            ..delayed.clone()
        };
        let wrong_expiry = ErasureJournalV1 {
            source_expires_at: 201,
            ..delayed
        };
        assert!(!same_erasure_identity(&first, &wrong_digest));
        assert!(!same_erasure_identity(&first, &wrong_expiry));
    }

    #[test]
    fn journal_parser_rejects_unknown_fields_duplicate_keys_and_identity_mismatch() {
        let id = "11111111-1111-4111-8111-111111111111";
        let key = journal_key(id).unwrap();
        let valid = format!(
            r#"{{"version":1,"submission_id":"{id}","replay_digest":"{}","source_expires_at":200,"deleted_at":100,"retain_until":2592100}}"#,
            "a".repeat(64)
        );
        assert!(parse_journal_file(&key, valid.as_bytes()).is_ok());
        assert!(parse_journal_file(&key, br#"{"version":1,"extra":true}"#).is_err());
        assert!(parse_journal_file(
            &key,
            format!(r#"{{"version":1,"version":1,"submission_id":"{id}"}}"#).as_bytes()
        )
        .is_err());
        let mismatch = valid.replace(id, "22222222-2222-4222-8222-222222222222");
        assert!(matches!(
            parse_journal_file(&key, mismatch.as_bytes()),
            Err(LifecycleError::Conflict)
        ));
        assert!(parse_journal_file(&key, &vec![b' '; super::JOURNAL_MAX_BYTES + 1]).is_err());
    }

    #[test]
    fn recovery_control_parser_rejects_noncanonical_state() {
        let generation = "11111111-1111-4111-8111-111111111111";
        let ready = format!(
            r#"{{"version":1,"generation":"{generation}","phase":"ready","reconcile_cursor":null,"cleanup_cursor":null}}"#
        );
        assert!(parse_recovery_control(ready.as_bytes()).is_ok());
        assert!(parse_recovery_control(
            format!(
                r#"{{"version":1,"generation":"{generation}","phase":"ready","reconcile_cursor":"skip","cleanup_cursor":null}}"#
            )
            .as_bytes()
        )
        .is_err());
        assert!(parse_recovery_control(
            format!(
                r#"{{"version":1,"generation":"{generation}","phase":"ready","reconcile_cursor":null,"cleanup_cursor":null,"open":true}}"#
            )
            .as_bytes()
        )
        .is_err());
    }

    #[test]
    fn reconciliation_requires_the_current_generation_and_reconciling_phase() {
        let generation = "11111111-1111-4111-8111-111111111111";
        let control = RecoveryControlV1 {
            version: 1,
            generation: generation.to_owned(),
            phase: RecoveryPhase::Reconciling,
            reconcile_cursor: None,
            cleanup_cursor: None,
        };
        assert!(require_reconcile_control(&control, generation).is_ok());
        assert!(matches!(
            require_reconcile_control(&control, "22222222-2222-4222-8222-222222222222"),
            Err(LifecycleError::Conflict)
        ));
        let awaiting_restore = RecoveryControlV1 {
            phase: RecoveryPhase::AwaitingRestore,
            ..control
        };
        assert!(matches!(
            require_reconcile_control(&awaiting_restore, generation),
            Err(LifecycleError::Conflict)
        ));
    }

    #[test]
    fn bounded_r2_parsers_do_not_panic_on_deterministic_fuzz_inputs() {
        const CASES: usize = 10_000;
        const TOKENS: &[u8] = b"{}[],:\"\\/0123456789.-+eEtruefalsn \t\r\n";
        let id = "11111111-1111-4111-8111-111111111111";
        let key = journal_key(id).unwrap();
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        for case_number in 0..CASES {
            let length = (next_fuzz_value(&mut state) % 512) as usize;
            let input = match case_number % 3 {
                0 => (0..length)
                    .map(|_| TOKENS[(next_fuzz_value(&mut state) as usize) % TOKENS.len()])
                    .collect::<Vec<_>>(),
                1 => format!(
                    "{}null{}",
                    "[".repeat(length % 128),
                    "]".repeat(length % 128)
                )
                .into_bytes(),
                _ => format!(
                    r#"{{"case":{case_number},"value":"{}"}}"#,
                    "é".repeat(length % 64)
                )
                .into_bytes(),
            };
            let result = std::panic::catch_unwind(|| {
                let _ = parse_journal_file(&key, &input);
                let _ = parse_recovery_control(&input);
            });
            assert!(result.is_ok(), "R2 parser panicked for case {case_number}");
        }
    }

    fn next_fuzz_value(state: &mut u64) -> u64 {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        *state
    }
}
