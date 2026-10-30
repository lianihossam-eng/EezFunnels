use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};

use eez_domain::{
    FormCompletionPeriod, FormSubmissionChannel, FormSubmissionStatus, InteractionReportPeriod,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use worker::d1::D1Type;
use worker::{D1Database, Error};

const SUPPORTED_SCHEMA_VERSION: i32 = 2;
const MAX_FUNNEL_BYTES: usize = 1_048_576;
const MAX_FUNNELS: i32 = 100;
const MAX_REVISIONS_RETAINED: i32 = 100;
const PUBLIC_FORM_SUBMISSION_CLEANUP_BATCH_SIZE: i32 = 10_000;
const PUBLIC_FORM_SUBMISSION_CLEANUP_MAX_BATCHES: u8 = 20;
const PUBLIC_INTERACTION_EVENT_CLEANUP_BATCH_SIZE: i32 = 10_000;
const PUBLIC_INTERACTION_EVENT_CLEANUP_MAX_BATCHES: u8 = 20;
const PUBLIC_INTERACTION_REPORT_QUERY: &str = r#"WITH RECURSIVE clock(now) AS (SELECT unixepoch()), funnel(id) AS (SELECT id FROM funnels WHERE id = ?), bounds(now, start_at, end_at) AS (SELECT clock.now, unixepoch(date(clock.now, 'unixepoch', printf('-%d days', ?))), unixepoch(date(clock.now, 'unixepoch', '+1 day')) FROM clock), days(utc_day, offset) AS (SELECT date(bounds.start_at, 'unixepoch'), 0 FROM bounds UNION ALL SELECT date(days.utc_day, '+1 day'), days.offset + 1 FROM days WHERE days.offset + 1 < ?), event_kinds(event_kind) AS (VALUES ('page_viewed'), ('form_viewed'), ('form_started')), retained AS MATERIALIZED (SELECT events.received_at, events.event_kind, events.campaign_id FROM public_interaction_events AS events JOIN funnel ON funnel.id = events.funnel_id CROSS JOIN bounds WHERE events.received_at >= bounds.start_at AND events.received_at < bounds.end_at AND events.expires_at > bounds.now), daily AS (SELECT strftime('%Y-%m-%d', received_at, 'unixepoch') AS utc_day, event_kind, COUNT(*) AS events FROM retained GROUP BY strftime('%Y-%m-%d', received_at, 'unixepoch'), event_kind) SELECT 'day_kind' AS row_type, days.utc_day, event_kinds.event_kind, NULL AS campaign_id, COALESCE(daily.events, 0) AS events FROM days CROSS JOIN event_kinds CROSS JOIN funnel LEFT JOIN daily ON daily.utc_day = days.utc_day AND daily.event_kind = event_kinds.event_kind UNION ALL SELECT 'campaign' AS row_type, NULL AS utc_day, NULL AS event_kind, retained.campaign_id, COUNT(*) AS events FROM retained GROUP BY retained.campaign_id ORDER BY row_type ASC, utc_day ASC, event_kind ASC, campaign_id ASC"#;
pub const PUBLIC_FORM_SUBMISSION_PAGE_SIZE: usize = 25;

pub type RepositoryResult<T> = std::result::Result<T, RepositoryError>;

#[derive(Debug)]
pub enum RepositoryError {
    Database(Error),
    UnsupportedSchemaVersion(i64),
    InvalidJson,
    PayloadTooLarge,
    InvalidField(&'static str),
    FunnelNotFound,
    RevisionNotFound,
    FunnelLimitReached,
    RevisionLimitReached,
    StaleEditorRevision,
    StalePublishedRevision,
    EditorRevisionLimitReached,
    StageCapacityReached,
    ActiveStageExists,
    StageIdGenerationFailed,
    StageDigestMismatch,
    StageUnavailable,
    StageControlUnavailable,
    StageStorageUnavailable,
}

impl Display for RepositoryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "D1 repository operation failed: {error}"),
            Self::UnsupportedSchemaVersion(version) => {
                write!(
                    formatter,
                    "validation/schema_version_unsupported: {version}"
                )
            }
            Self::InvalidJson => formatter.write_str("validation/json_invalid"),
            Self::PayloadTooLarge => formatter.write_str("validation/payload_too_large"),
            Self::InvalidField(field) => write!(formatter, "validation/field_invalid: {field}"),
            Self::FunnelNotFound => formatter.write_str("not_found/funnel"),
            Self::RevisionNotFound => formatter.write_str("not_found/revision"),
            Self::FunnelLimitReached => formatter.write_str("conflict/funnel_limit_reached"),
            Self::RevisionLimitReached => formatter.write_str("validation/revision_limit_reached"),
            Self::StaleEditorRevision => formatter.write_str("conflict/edit_stale_revision"),
            Self::StalePublishedRevision => formatter.write_str("conflict/publish_stale_revision"),
            Self::EditorRevisionLimitReached => {
                formatter.write_str("conflict/revision_limit_reached")
            }
            Self::StageCapacityReached => formatter.write_str("rate_limit/exceeded"),
            Self::ActiveStageExists => formatter.write_str("conflict/v2_stage_active"),
            Self::StageIdGenerationFailed => formatter.write_str("internal/unexpected"),
            Self::StageDigestMismatch => formatter.write_str("validation/invalid_json"),
            Self::StageUnavailable => formatter.write_str("conflict/v2_stage_active"),
            Self::StageControlUnavailable | Self::StageStorageUnavailable => {
                formatter.write_str("external/unavailable")
            }
        }
    }
}

impl RepositoryError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Database(_) => "external/unavailable",
            Self::UnsupportedSchemaVersion(_) => "validation/schema_version_unsupported",
            Self::InvalidJson => "validation/invalid_json",
            Self::PayloadTooLarge => "validation/invalid_json",
            Self::InvalidField(_) => "validation/invalid_json",
            Self::FunnelNotFound | Self::RevisionNotFound => "not_found/funnel",
            Self::FunnelLimitReached => "conflict/funnel_limit_reached",
            Self::RevisionLimitReached => "validation/invalid_json",
            Self::StaleEditorRevision => "conflict/edit_stale_revision",
            Self::StalePublishedRevision => "conflict/publish_stale_revision",
            Self::EditorRevisionLimitReached => "conflict/revision_limit_reached",
            Self::StageCapacityReached => "rate_limit/exceeded",
            Self::ActiveStageExists | Self::StageUnavailable => "conflict/v2_stage_active",
            Self::StageIdGenerationFailed => "internal/unexpected",
            Self::StageDigestMismatch => "validation/invalid_json",
            Self::StageControlUnavailable | Self::StageStorageUnavailable => "external/unavailable",
        }
    }
}

impl From<Error> for RepositoryError {
    fn from(error: Error) -> Self {
        Self::Database(error)
    }
}

impl From<RepositoryError> for Error {
    fn from(error: RepositoryError) -> Self {
        Self::RustError(error.to_string())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct FunnelRecord {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub published_revision_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct RevisionRecord {
    pub id: String,
    pub funnel_id: String,
    pub schema_version: i64,
    pub data_json: String,
    pub render_manifest_json: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PublishedRevisionRecord {
    pub slug: String,
    pub revision: RevisionRecord,
}

#[derive(Deserialize)]
struct PublishedRevisionJoin {
    slug: String,
    id: String,
    funnel_id: String,
    schema_version: i64,
    data_json: String,
    render_manifest_json: String,
    created_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct RevisionSummary {
    pub id: String,
    pub funnel_id: String,
    pub schema_version: i64,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct EditorRevisionRecord {
    pub funnel_id: String,
    pub name: String,
    pub slug: String,
    pub updated_at: String,
    pub head_revision_id: String,
    pub published_revision_id: Option<String>,
    pub revision: RevisionRecord,
}

#[derive(Deserialize)]
struct EditorRevisionJoin {
    funnel_id: String,
    name: String,
    slug: String,
    updated_at: String,
    head_revision_id: String,
    published_revision_id: Option<String>,
    revision_id: String,
    revision_funnel_id: String,
    schema_version: i64,
    data_json: String,
    render_manifest_json: String,
    revision_created_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ThemeRecord {
    pub id: String,
    pub name: String,
    pub tokens_json: String,
    pub is_default: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewFunnel {
    pub id: String,
    pub name: String,
    pub slug: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewRevision {
    pub id: String,
    pub funnel_id: String,
    pub schema_version: i64,
    pub data_json: String,
    pub render_manifest_json: String,
}

pub struct EditorSaveMutation<'a> {
    pub command_id: &'a str,
    pub funnel_id: &'a str,
    pub expected_revision_id: &'a str,
    pub revision: &'a NewRevision,
    pub idempotency_key: &'a str,
    pub payload_json: &'a str,
    pub result_json: &'a str,
}

pub struct PublicationMutation<'a> {
    pub funnel_id: &'a str,
    pub published_revision_id: Option<&'a str>,
    pub expected_published_revision_id: Option<&'a str>,
    pub command_id: &'a str,
    pub idempotency_key: &'a str,
    pub payload_json: &'a str,
    pub result_json: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewTheme {
    pub id: String,
    pub name: String,
    pub tokens_json: String,
    pub is_default: bool,
}

pub struct NewPublicFormSubmission<'a> {
    pub submission_id: &'a str,
    pub funnel_id: &'a str,
    pub funnel_slug: &'a str,
    pub revision_id: &'a str,
    pub page_id: &'a str,
    pub form_id: &'a str,
    pub submission_channel: FormSubmissionChannel,
    pub submission_key: &'a str,
    pub field_values_json: &'a str,
    pub request_consent_copy_version: &'a str,
    pub request_consent_copy: &'a str,
    pub marketing_opt_in: bool,
    pub marketing_opt_in_copy_version: Option<&'a str>,
    pub marketing_opt_in_copy: Option<&'a str>,
    pub retention_seconds: i32,
    pub limit_per_minute: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublicFormSubmissionResult {
    Stored { submission_id: String },
    Duplicate { submission_id: String },
    KeyReuseMismatch,
    ErasedReplay,
    RateLimited,
    NotCurrent,
}

pub struct NewPublicInteractionEvent<'a> {
    pub event_id: &'a str,
    pub funnel_id: &'a str,
    pub funnel_slug: &'a str,
    pub revision_id: &'a str,
    pub page_id: &'a str,
    pub event_kind: &'a str,
    pub form_id: Option<&'a str>,
    pub campaign_id: Option<&'a str>,
    pub replay_fingerprint: &'a str,
    pub retention_days: u8,
    pub limit_per_minute: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicInteractionEventResult {
    Stored,
    Duplicate,
    KeyReuseMismatch,
    RateLimited,
    NotCurrent,
}

#[derive(Deserialize)]
struct PublicFormSubmissionState {
    canonical_submission_id: Option<String>,
    is_current: i64,
    is_duplicate: i64,
    is_key_reuse: i64,
    is_rate_limited: i64,
    is_erased_replay: i64,
}

#[derive(Deserialize)]
struct PublicFormSubmissionCanonicalState {
    canonical_submission_id: Option<String>,
}

#[derive(Deserialize)]
struct PublicFormSubmissionId {
    submission_id: String,
}

#[derive(Deserialize)]
struct PublicInteractionEventState {
    is_current: i64,
    is_duplicate: i64,
    is_key_reuse: i64,
    is_rate_limited: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicFormSubmissionCursor {
    pub submitted_at: i64,
    pub submission_id: String,
    pub status: Option<FormSubmissionStatus>,
    pub funnel_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PublicFormSubmissionRecord {
    pub submission_id: String,
    pub triage_status: FormSubmissionStatus,
    pub funnel_id: String,
    pub funnel_name: String,
    pub funnel_slug: String,
    pub revision_id: String,
    pub revision_schema_version: i64,
    pub page_id: String,
    pub form_id: String,
    pub field_values_json: String,
    pub request_consent_acknowledged: i64,
    pub request_consent_copy_version: String,
    pub request_consent_copy: String,
    pub request_consent_recorded_at: i64,
    pub request_consent_recorded_at_label: String,
    pub marketing_opt_in: i64,
    pub marketing_opt_in_copy_version: Option<String>,
    pub marketing_opt_in_copy: Option<String>,
    pub marketing_opt_in_recorded_at: Option<i64>,
    pub marketing_opt_in_recorded_at_label: Option<String>,
    pub submitted_at: i64,
    pub submitted_at_label: String,
    pub submitted_at_datetime: String,
    pub expires_at: i64,
    pub expires_at_label: String,
    pub delivery_state: Option<String>,
    pub delivery_retry_scheduled: i64,
    pub delivery_retry_window_open: i64,
    pub delivery_attempt_count: Option<i64>,
    pub delivery_last_attempt_at_label: Option<String>,
    pub delivery_next_attempt_at_label: Option<String>,
    pub delivery_last_http_status: Option<i64>,
    pub delivery_last_outcome_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PublicFormSubmissionRevision {
    pub id: String,
    pub funnel_id: String,
    pub schema_version: i64,
    pub data_json: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct PublicFormSubmissionPage {
    pub items: Vec<PublicFormSubmissionRecord>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct FormCompletionCountRow {
    pub row_type: String,
    pub utc_day: Option<String>,
    pub form_id: Option<String>,
    pub submission_channel: Option<String>,
    pub completions: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PublicInteractionReportRow {
    pub row_type: String,
    pub utc_day: Option<String>,
    pub event_kind: Option<String>,
    pub campaign_id: Option<String>,
    pub events: i64,
}

pub enum MutationResult {
    Stored(String),
    PayloadMismatch,
}

#[derive(Deserialize)]
struct CommandReceipt {
    command_id: String,
    payload_json: String,
    result_json: String,
}

pub struct Repository<'a> {
    database: &'a D1Database,
}

impl<'a> Repository<'a> {
    pub fn new(database: &'a D1Database) -> Self {
        Self { database }
    }

    pub async fn classify_active_receipt(
        &self,
        idempotency_key: &str,
        command_id: &str,
        payload_json: Option<&str>,
    ) -> RepositoryResult<Option<MutationResult>> {
        validate_required_field(command_id, "command_id")?;
        validate_idempotency_key(idempotency_key)?;

        Ok(self
            .active_receipt(idempotency_key)
            .await?
            .map(|receipt| match payload_json {
                Some(payload_json) => classify_receipt(receipt, command_id, payload_json),
                None => MutationResult::PayloadMismatch,
            }))
    }

    pub async fn create_funnel(&self, input: &NewFunnel) -> RepositoryResult<FunnelRecord> {
        validate_required_field(&input.id, "id")?;
        validate_required_field(&input.name, "name")?;
        validate_required_field(&input.slug, "slug")?;

        let id = D1Type::Text(&input.id);
        let name = D1Type::Text(&input.name);
        let slug = D1Type::Text(&input.slug);
        self.database
            .prepare("INSERT INTO funnels (id, name, slug) VALUES (?, ?, ?)")
            .bind_refs([&id, &name, &slug])?
            .run()
            .await?;
        self.get_funnel(&input.id)
            .await?
            .ok_or_else(|| Error::RustError("created funnel could not be read".to_owned()).into())
    }

    pub async fn create_funnel_with_revision(
        &self,
        funnel: &NewFunnel,
        revision: &NewRevision,
        command_id: &str,
        idempotency_key: &str,
        payload_json: &str,
        result_json: &str,
    ) -> RepositoryResult<MutationResult> {
        validate_legacy_revision_schema_version(revision.schema_version)?;
        self.create_funnel_with_revision_inner(
            funnel,
            revision,
            command_id,
            idempotency_key,
            payload_json,
            result_json,
        )
        .await
    }

    pub async fn create_funnel_with_v2_revision(
        &self,
        funnel: &NewFunnel,
        revision: &NewRevision,
        command_id: &str,
        idempotency_key: &str,
        payload_json: &str,
        result_json: &str,
    ) -> RepositoryResult<MutationResult> {
        if revision.schema_version != 2 || command_id != "funnel.create.v2" {
            return Err(RepositoryError::InvalidField("v2_initial_revision"));
        }
        self.create_funnel_with_revision_inner(
            funnel,
            revision,
            command_id,
            idempotency_key,
            payload_json,
            result_json,
        )
        .await
    }

    async fn create_funnel_with_revision_inner(
        &self,
        funnel: &NewFunnel,
        revision: &NewRevision,
        command_id: &str,
        idempotency_key: &str,
        payload_json: &str,
        result_json: &str,
    ) -> RepositoryResult<MutationResult> {
        validate_required_field(&funnel.id, "id")?;
        validate_required_field(&funnel.name, "name")?;
        validate_required_field(&funnel.slug, "slug")?;
        validate_required_field(&revision.id, "revision_id")?;
        validate_required_field(command_id, "command_id")?;
        validate_idempotency_key(idempotency_key)?;
        if revision.funnel_id != funnel.id {
            return Err(RepositoryError::InvalidField("funnel_id"));
        }
        validate_payload_size(&revision.data_json)?;
        self.validate_json(&revision.data_json).await?;
        self.validate_json(&revision.render_manifest_json).await?;
        self.validate_json(payload_json).await?;
        self.validate_json(result_json).await?;

        if let Some(receipt) = self.active_receipt(idempotency_key).await? {
            return Ok(classify_receipt(receipt, command_id, payload_json));
        }

        let key = D1Type::Text(idempotency_key);
        let command = D1Type::Text(command_id);
        let payload = D1Type::Text(payload_json);
        let result = D1Type::Text(result_json);
        let delete_expired_receipts = self.database.prepare(
            "DELETE FROM command_receipts WHERE expires_at <= CAST(strftime('%s', 'now') AS INTEGER)",
        );
        let max_funnels = D1Type::Integer(MAX_FUNNELS);
        let insert_receipt = self
            .database
            .prepare(
                "INSERT INTO command_receipts (idempotency_key, command_id, payload_json, result_json, expires_at) SELECT ?, ?, ?, ?, CAST(strftime('%s', 'now') AS INTEGER) + 86400 WHERE (SELECT COUNT(*) FROM funnels) < ? ON CONFLICT(idempotency_key) DO UPDATE SET command_id = excluded.command_id, payload_json = excluded.payload_json, result_json = excluded.result_json, expires_at = excluded.expires_at WHERE command_receipts.expires_at <= CAST(strftime('%s', 'now') AS INTEGER)",
            )
            .bind_refs([&key, &command, &payload, &result, &max_funnels])?;

        let funnel_id = D1Type::Text(&funnel.id);
        let funnel_name = D1Type::Text(&funnel.name);
        let funnel_slug = D1Type::Text(&funnel.slug);
        let insert_funnel = self
            .database
            .prepare(
                "INSERT INTO funnels (id, name, slug) SELECT ?, ?, ? WHERE EXISTS (SELECT 1 FROM command_receipts WHERE idempotency_key = ? AND command_id = ? AND payload_json = ? AND result_json = ? AND expires_at > CAST(strftime('%s', 'now') AS INTEGER)) AND (SELECT COUNT(*) FROM funnels) < ?",
            )
            .bind_refs([
                &funnel_id,
                &funnel_name,
                &funnel_slug,
                &key,
                &command,
                &payload,
                &result,
                &max_funnels,
            ])?;

        let revision_id = D1Type::Text(&revision.id);
        let revision_funnel_id = D1Type::Text(&revision.funnel_id);
        let schema_version = D1Type::Integer(
            i32::try_from(revision.schema_version)
                .map_err(|_| RepositoryError::InvalidField("schema_version"))?,
        );
        let data_json = D1Type::Text(&revision.data_json);
        let manifest_json = D1Type::Text(&revision.render_manifest_json);
        let insert_revision = self
            .database
            .prepare(
                "INSERT INTO revisions (id, funnel_id, schema_version, data_json, render_manifest_json) SELECT ?, ?, ?, ?, ? WHERE EXISTS (SELECT 1 FROM funnels WHERE id = ?) AND EXISTS (SELECT 1 FROM command_receipts WHERE idempotency_key = ? AND command_id = ? AND payload_json = ? AND result_json = ? AND expires_at > CAST(strftime('%s', 'now') AS INTEGER))",
            )
            .bind_refs([
                &revision_id,
                &revision_funnel_id,
                &schema_version,
                &data_json,
                &manifest_json,
                &revision_funnel_id,
                &key,
                &command,
                &payload,
                &result,
            ])?;

        let insert_editor_head = self
            .database
            .prepare(
                "INSERT INTO funnel_editor_heads (funnel_id, revision_id) SELECT ?, ? WHERE EXISTS (SELECT 1 FROM revisions WHERE id = ? AND funnel_id = ?) AND EXISTS (SELECT 1 FROM command_receipts WHERE idempotency_key = ? AND command_id = ? AND payload_json = ? AND result_json = ? AND expires_at > CAST(strftime('%s', 'now') AS INTEGER))",
            )
            .bind_refs([
                &revision_funnel_id,
                &revision_id,
                &revision_id,
                &revision_funnel_id,
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
                insert_funnel,
                insert_revision,
                insert_editor_head,
            ])
            .await
        {
            if let Some(receipt) = self.active_receipt(idempotency_key).await? {
                return Ok(classify_receipt(receipt, command_id, payload_json));
            }
            if self.funnel_count().await? >= i64::from(MAX_FUNNELS) {
                return Err(RepositoryError::FunnelLimitReached);
            }
            return Err(error.into());
        }

        match self.active_receipt(idempotency_key).await? {
            Some(receipt) => Ok(classify_receipt(receipt, command_id, payload_json)),
            None if self.funnel_count().await? >= i64::from(MAX_FUNNELS) => {
                Err(RepositoryError::FunnelLimitReached)
            }
            None => Err(Error::RustError("command receipt was not committed".to_owned()).into()),
        }
    }

    pub async fn delete_funnel_idempotently(
        &self,
        funnel_id: &str,
        command_id: &str,
        idempotency_key: &str,
        payload_json: &str,
        result_json: &str,
    ) -> RepositoryResult<MutationResult> {
        validate_required_field(funnel_id, "funnel_id")?;
        validate_required_field(command_id, "command_id")?;
        validate_idempotency_key(idempotency_key)?;
        self.validate_json(payload_json).await?;
        self.validate_json(result_json).await?;

        if let Some(receipt) = self.active_receipt(idempotency_key).await? {
            return Ok(classify_receipt(receipt, command_id, payload_json));
        }

        let key = D1Type::Text(idempotency_key);
        let command = D1Type::Text(command_id);
        let payload = D1Type::Text(payload_json);
        let result = D1Type::Text(result_json);
        let funnel_id_value = D1Type::Text(funnel_id);
        let delete_expired_receipts = self.database.prepare(
            "DELETE FROM command_receipts WHERE expires_at <= CAST(strftime('%s', 'now') AS INTEGER)",
        );
        let insert_receipt = self
            .database
            .prepare(
                "INSERT INTO command_receipts (idempotency_key, command_id, payload_json, result_json, expires_at) SELECT ?, ?, ?, ?, CAST(strftime('%s', 'now') AS INTEGER) + 86400 WHERE EXISTS (SELECT 1 FROM funnels WHERE id = ?) ON CONFLICT(idempotency_key) DO UPDATE SET command_id = excluded.command_id, payload_json = excluded.payload_json, result_json = excluded.result_json, expires_at = excluded.expires_at WHERE command_receipts.expires_at <= CAST(strftime('%s', 'now') AS INTEGER)",
            )
            .bind_refs([&key, &command, &payload, &result, &funnel_id_value])?;
        let delete_funnel = self
            .database
            .prepare(
                "DELETE FROM funnels WHERE id = ? AND EXISTS (SELECT 1 FROM command_receipts WHERE idempotency_key = ? AND command_id = ? AND payload_json = ? AND result_json = ? AND expires_at > CAST(strftime('%s', 'now') AS INTEGER))",
            )
            .bind_refs([&funnel_id_value, &key, &command, &payload, &result])?;

        if let Err(error) = self
            .database
            .batch(vec![delete_expired_receipts, insert_receipt, delete_funnel])
            .await
        {
            if let Some(receipt) = self.active_receipt(idempotency_key).await? {
                return Ok(classify_receipt(receipt, command_id, payload_json));
            }
            return Err(error.into());
        }

        match self.active_receipt(idempotency_key).await? {
            Some(receipt) => Ok(classify_receipt(receipt, command_id, payload_json)),
            None => Err(RepositoryError::FunnelNotFound),
        }
    }

    pub async fn save_editor_revision(
        &self,
        mutation: &EditorSaveMutation<'_>,
    ) -> RepositoryResult<MutationResult> {
        validate_legacy_revision_schema_version(mutation.revision.schema_version)?;
        self.save_editor_revision_from_v1_head(mutation).await
    }

    pub async fn save_v1_conversion_revision(
        &self,
        mutation: &EditorSaveMutation<'_>,
    ) -> RepositoryResult<MutationResult> {
        if mutation.revision.schema_version != 2
            || mutation.command_id != "funnel.document.conversion.commit.v2"
        {
            return Err(RepositoryError::InvalidField("v1_conversion_revision"));
        }
        self.save_editor_revision_from_v1_head(mutation).await
    }

    async fn save_editor_revision_from_v1_head(
        &self,
        mutation: &EditorSaveMutation<'_>,
    ) -> RepositoryResult<MutationResult> {
        let command_id = mutation.command_id;
        let funnel_id = mutation.funnel_id;
        let expected_revision_id = mutation.expected_revision_id;
        let revision = mutation.revision;
        let idempotency_key = mutation.idempotency_key;
        let payload_json = mutation.payload_json;
        let result_json = mutation.result_json;
        validate_required_field(funnel_id, "funnel_id")?;
        validate_required_field(expected_revision_id, "expected_revision_id")?;
        validate_required_field(&revision.id, "revision_id")?;
        validate_required_field(command_id, "command_id")?;
        validate_idempotency_key(idempotency_key)?;
        if revision.funnel_id != funnel_id {
            return Err(RepositoryError::InvalidField("funnel_id"));
        }
        validate_payload_size(&revision.data_json)?;
        self.validate_json(&revision.data_json).await?;
        self.validate_json(&revision.render_manifest_json).await?;
        self.validate_json(payload_json).await?;
        self.validate_json(result_json).await?;

        if let Some(receipt) = self.active_receipt(idempotency_key).await? {
            return Ok(classify_receipt(receipt, command_id, payload_json));
        }

        let funnel_id_value = D1Type::Text(funnel_id);
        let expected_id_value = D1Type::Text(expected_revision_id);
        let revision_id_value = D1Type::Text(&revision.id);
        let revision_funnel_id = D1Type::Text(&revision.funnel_id);
        let schema_version = D1Type::Integer(
            i32::try_from(revision.schema_version)
                .map_err(|_| RepositoryError::InvalidField("schema_version"))?,
        );
        let data_json = D1Type::Text(&revision.data_json);
        let manifest_json = D1Type::Text(&revision.render_manifest_json);
        let retained_limit = D1Type::Integer(MAX_REVISIONS_RETAINED);
        let base_schema_version = D1Type::Integer(1);
        let key = D1Type::Text(idempotency_key);
        let command = D1Type::Text(command_id);
        let payload = D1Type::Text(payload_json);
        let result = D1Type::Text(result_json);
        let delete_expired_receipts = self.database.prepare(
            "DELETE FROM command_receipts WHERE expires_at <= CAST(strftime('%s', 'now') AS INTEGER)",
        );
        let insert_receipt = self
            .database
            .prepare(
                "INSERT INTO command_receipts (idempotency_key, command_id, payload_json, result_json, expires_at) SELECT ?, ?, ?, ?, CAST(strftime('%s', 'now') AS INTEGER) + 86400 WHERE EXISTS (SELECT 1 FROM funnel_editor_heads AS head JOIN revisions AS base ON base.id = head.revision_id AND base.funnel_id = head.funnel_id WHERE head.funnel_id = ? AND head.revision_id = ? AND base.schema_version = ?) AND (SELECT COUNT(*) FROM revisions WHERE funnel_id = ?) < ? ON CONFLICT(idempotency_key) DO UPDATE SET command_id = excluded.command_id, payload_json = excluded.payload_json, result_json = excluded.result_json, expires_at = excluded.expires_at WHERE command_receipts.expires_at <= CAST(strftime('%s', 'now') AS INTEGER)",
            )
            .bind_refs([
                &key,
                &command,
                &payload,
                &result,
                &funnel_id_value,
                &expected_id_value,
                &base_schema_version,
                &funnel_id_value,
                &retained_limit,
            ])?;

        let owns_receipt = "EXISTS (SELECT 1 FROM command_receipts WHERE idempotency_key = ? AND command_id = ? AND payload_json = ? AND result_json = ? AND expires_at > CAST(strftime('%s', 'now') AS INTEGER))";
        let insert_revision = self
            .database
            .prepare(format!(
                "INSERT INTO revisions (id, funnel_id, schema_version, data_json, render_manifest_json) SELECT ?, ?, ?, ?, ? WHERE EXISTS (SELECT 1 FROM funnel_editor_heads AS head JOIN revisions AS base ON base.id = head.revision_id AND base.funnel_id = head.funnel_id WHERE head.funnel_id = ? AND head.revision_id = ? AND base.schema_version = ?) AND (SELECT COUNT(*) FROM revisions WHERE funnel_id = ?) < ? AND {owns_receipt}"
            ))
            .bind_refs([
                &revision_id_value,
                &revision_funnel_id,
                &schema_version,
                &data_json,
                &manifest_json,
                &funnel_id_value,
                &expected_id_value,
                &base_schema_version,
                &funnel_id_value,
                &retained_limit,
                &key,
                &command,
                &payload,
                &result,
            ])?;

        let new_head_revision = D1Type::Text(&revision.id);
        let update_head = self
            .database
            .prepare(format!(
                "UPDATE funnel_editor_heads SET revision_id = ? WHERE funnel_id = ? AND revision_id = ? AND EXISTS (SELECT 1 FROM revisions WHERE id = ? AND funnel_id = ?) AND {owns_receipt}"
            ))
            .bind_refs([
                &new_head_revision,
                &funnel_id_value,
                &expected_id_value,
                &revision_id_value,
                &funnel_id_value,
                &key,
                &command,
                &payload,
                &result,
            ])?;
        let update_funnel = self
            .database
            .prepare(format!(
                "UPDATE funnels SET updated_at = CURRENT_TIMESTAMP WHERE id = ? AND EXISTS (SELECT 1 FROM funnel_editor_heads WHERE funnel_id = ? AND revision_id = ?) AND {owns_receipt}"
            ))
            .bind_refs([
                &funnel_id_value,
                &funnel_id_value,
                &new_head_revision,
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
            ])
            .await
        {
            if let Some(receipt) = self.active_receipt(idempotency_key).await? {
                return Ok(classify_receipt(receipt, command_id, payload_json));
            }
            return self
                .classify_editor_save_failure(funnel_id, expected_revision_id, error)
                .await;
        }

        if let Some(receipt) = self.active_receipt(idempotency_key).await? {
            return Ok(classify_receipt(receipt, command_id, payload_json));
        }
        self.classify_editor_save_failure(
            funnel_id,
            expected_revision_id,
            Error::RustError("editor save did not advance the head".to_owned()),
        )
        .await
    }

    async fn classify_editor_save_failure(
        &self,
        funnel_id: &str,
        expected_revision_id: &str,
        failure: Error,
    ) -> RepositoryResult<MutationResult> {
        if self.get_funnel(funnel_id).await?.is_none() {
            return Err(RepositoryError::FunnelNotFound);
        }
        match self.get_editor_head(funnel_id).await? {
            Some(head) if head != expected_revision_id => {
                return Err(RepositoryError::StaleEditorRevision)
            }
            None => return Err(RepositoryError::FunnelNotFound),
            Some(_) => {}
        }
        if self.revision_count(funnel_id).await? >= i64::from(MAX_REVISIONS_RETAINED) {
            return Err(RepositoryError::EditorRevisionLimitReached);
        }
        Err(failure.into())
    }

    async fn get_editor_head(&self, funnel_id: &str) -> RepositoryResult<Option<String>> {
        let funnel_id = D1Type::Text(funnel_id);
        self.database
            .prepare("SELECT revision_id FROM funnel_editor_heads WHERE funnel_id = ?")
            .bind_refs([&funnel_id])?
            .first::<String>(Some("revision_id"))
            .await
            .map_err(Into::into)
    }

    async fn revision_count(&self, funnel_id: &str) -> RepositoryResult<i64> {
        let funnel_id = D1Type::Text(funnel_id);
        self.database
            .prepare("SELECT COUNT(*) AS revision_count FROM revisions WHERE funnel_id = ?")
            .bind_refs([&funnel_id])?
            .first::<i64>(Some("revision_count"))
            .await?
            .ok_or_else(|| {
                Error::RustError("funnel revision count could not be read".to_owned()).into()
            })
    }

    async fn active_receipt(
        &self,
        idempotency_key: &str,
    ) -> RepositoryResult<Option<CommandReceipt>> {
        let key = D1Type::Text(idempotency_key);
        self.database
            .prepare(
                "SELECT command_id, payload_json, result_json FROM command_receipts WHERE idempotency_key = ? AND expires_at > CAST(strftime('%s', 'now') AS INTEGER)",
            )
            .bind_refs([&key])?
            .first::<CommandReceipt>(None)
            .await
            .map_err(Into::into)
    }

    pub async fn get_funnel(&self, id: &str) -> RepositoryResult<Option<FunnelRecord>> {
        let id = D1Type::Text(id);
        self.database
            .prepare(
                "SELECT id, name, slug, published_revision_id, created_at, updated_at FROM funnels WHERE id = ?",
            )
            .bind_refs([&id])?
            .first::<FunnelRecord>(None)
            .await
            .map_err(Into::into)
    }

    pub async fn get_editor_revision(
        &self,
        funnel_id: &str,
    ) -> RepositoryResult<Option<EditorRevisionRecord>> {
        let funnel_id = D1Type::Text(funnel_id);
        let row = self
            .database
            .prepare(
                "SELECT funnels.id AS funnel_id, funnels.name, funnels.slug, funnels.updated_at, funnel_editor_heads.revision_id AS head_revision_id, funnels.published_revision_id, revisions.id AS revision_id, revisions.funnel_id AS revision_funnel_id, revisions.schema_version, revisions.data_json, revisions.render_manifest_json, revisions.created_at AS revision_created_at FROM funnels JOIN funnel_editor_heads ON funnel_editor_heads.funnel_id = funnels.id JOIN revisions ON revisions.id = funnel_editor_heads.revision_id AND revisions.funnel_id = funnels.id WHERE funnels.id = ?",
            )
            .bind_refs([&funnel_id])?
            .first::<EditorRevisionJoin>(None)
            .await?;
        row.map(|row| {
            validate_schema_version(row.schema_version)?;
            Ok(EditorRevisionRecord {
                funnel_id: row.funnel_id,
                name: row.name,
                slug: row.slug,
                updated_at: row.updated_at,
                head_revision_id: row.head_revision_id,
                published_revision_id: row.published_revision_id,
                revision: RevisionRecord {
                    id: row.revision_id,
                    funnel_id: row.revision_funnel_id,
                    schema_version: row.schema_version,
                    data_json: row.data_json,
                    render_manifest_json: row.render_manifest_json,
                    created_at: row.revision_created_at,
                },
            })
        })
        .transpose()
    }

    pub async fn get_owned_revision(
        &self,
        funnel_id: &str,
        revision_id: &str,
    ) -> RepositoryResult<Option<RevisionRecord>> {
        let funnel_id = D1Type::Text(funnel_id);
        let revision_id = D1Type::Text(revision_id);
        let revision = self
            .database
            .prepare(
                "SELECT id, funnel_id, schema_version, data_json, render_manifest_json, created_at FROM revisions WHERE funnel_id = ? AND id = ?",
            )
            .bind_refs([&funnel_id, &revision_id])?
            .first::<RevisionRecord>(None)
            .await?;
        if let Some(revision) = revision {
            validate_schema_version(revision.schema_version)?;
            Ok(Some(revision))
        } else {
            Ok(None)
        }
    }

    pub async fn list_funnels(&self) -> RepositoryResult<Vec<FunnelRecord>> {
        let limit = D1Type::Integer(MAX_FUNNELS);
        let result = self
            .database
            .prepare(
                "SELECT id, name, slug, published_revision_id, created_at, updated_at FROM funnels ORDER BY created_at, id LIMIT ?",
            )
            .bind_refs([&limit])?
            .all()
            .await?;
        result.results::<FunnelRecord>().map_err(Into::into)
    }

    pub async fn list_published_revisions(&self) -> RepositoryResult<Vec<PublishedRevisionRecord>> {
        let limit = D1Type::Integer(MAX_FUNNELS);
        let rows = self
            .database
            .prepare(
                "SELECT funnels.slug, revisions.id, revisions.funnel_id, revisions.schema_version, revisions.data_json, revisions.render_manifest_json, revisions.created_at FROM funnels JOIN revisions ON revisions.id = funnels.published_revision_id AND revisions.funnel_id = funnels.id WHERE funnels.published_revision_id IS NOT NULL ORDER BY funnels.slug LIMIT ?",
            )
            .bind_refs([&limit])?
            .all()
            .await?
            .results::<PublishedRevisionJoin>()?;
        rows.into_iter()
            .map(|row| {
                validate_schema_version(row.schema_version)?;
                Ok(PublishedRevisionRecord {
                    slug: row.slug,
                    revision: RevisionRecord {
                        id: row.id,
                        funnel_id: row.funnel_id,
                        schema_version: row.schema_version,
                        data_json: row.data_json,
                        render_manifest_json: row.render_manifest_json,
                        created_at: row.created_at,
                    },
                })
            })
            .collect()
    }

    async fn funnel_count(&self) -> RepositoryResult<i64> {
        self.database
            .prepare("SELECT COUNT(*) AS funnel_count FROM funnels")
            .first::<i64>(Some("funnel_count"))
            .await?
            .ok_or_else(|| {
                Error::RustError("workspace funnel count could not be read".to_owned()).into()
            })
    }

    pub async fn update_funnel(
        &self,
        id: &str,
        name: &str,
        slug: &str,
    ) -> RepositoryResult<Option<FunnelRecord>> {
        validate_required_field(name, "name")?;
        validate_required_field(slug, "slug")?;
        let id = D1Type::Text(id);
        let name = D1Type::Text(name);
        let slug = D1Type::Text(slug);
        let result = self
            .database
            .prepare(
                "UPDATE funnels SET name = ?, slug = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ? RETURNING id, name, slug, published_revision_id, created_at, updated_at",
            )
            .bind_refs([&name, &slug, &id])?
            .first::<FunnelRecord>(None)
            .await?;
        Ok(result)
    }

    pub async fn delete_funnel(&self, id: &str) -> RepositoryResult<bool> {
        let id = D1Type::Text(id);
        let deleted = self
            .database
            .prepare("DELETE FROM funnels WHERE id = ? RETURNING id")
            .bind_refs([&id])?
            .first::<String>(Some("id"))
            .await?;
        Ok(deleted.is_some())
    }

    pub async fn create_revision(&self, input: &NewRevision) -> RepositoryResult<RevisionRecord> {
        validate_required_field(&input.id, "id")?;
        validate_required_field(&input.funnel_id, "funnel_id")?;
        if self.get_funnel(&input.funnel_id).await?.is_none() {
            return Err(RepositoryError::FunnelNotFound);
        }
        validate_legacy_revision_schema_version(input.schema_version)?;
        validate_payload_size(&input.data_json)?;
        self.validate_json(&input.data_json).await?;
        self.validate_json(&input.render_manifest_json).await?;

        let id = D1Type::Text(&input.id);
        let funnel_id = D1Type::Text(&input.funnel_id);
        let schema_version = D1Type::Integer(1);
        let data_json = D1Type::Text(&input.data_json);
        let render_manifest_json = D1Type::Text(&input.render_manifest_json);
        let result = self
            .database
            .prepare(
                "INSERT INTO revisions (id, funnel_id, schema_version, data_json, render_manifest_json) SELECT ?, ?, ?, ?, ? WHERE (SELECT COUNT(*) FROM revisions WHERE funnel_id = ?) < ? RETURNING id, funnel_id, schema_version, data_json, render_manifest_json, created_at",
            )
            .bind_refs([
                &id,
                &funnel_id,
                &schema_version,
                &data_json,
                &render_manifest_json,
                &funnel_id,
                &D1Type::Integer(MAX_REVISIONS_RETAINED),
            ])?
            .first::<RevisionRecord>(None)
            .await?;
        let revision = result.ok_or(RepositoryError::RevisionLimitReached)?;
        validate_schema_version(revision.schema_version)?;
        Ok(revision)
    }

    pub async fn get_revision(&self, id: &str) -> RepositoryResult<Option<RevisionRecord>> {
        let id = D1Type::Text(id);
        let revision = self
            .database
            .prepare(
                "SELECT id, funnel_id, schema_version, data_json, render_manifest_json, created_at FROM revisions WHERE id = ?",
            )
            .bind_refs([&id])?
            .first::<RevisionRecord>(None)
            .await?;
        if let Some(revision) = revision {
            validate_schema_version(revision.schema_version)?;
            Ok(Some(revision))
        } else {
            Ok(None)
        }
    }

    pub async fn list_revisions(&self, funnel_id: &str) -> RepositoryResult<Vec<RevisionRecord>> {
        let funnel_id = D1Type::Text(funnel_id);
        let result = self
            .database
            .prepare(
                "SELECT id, funnel_id, schema_version, data_json, render_manifest_json, created_at FROM revisions WHERE funnel_id = ? ORDER BY created_at, id",
            )
            .bind_refs([&funnel_id])?
            .all()
            .await?;
        let revisions = result.results::<RevisionRecord>()?;
        for revision in &revisions {
            validate_schema_version(revision.schema_version)?;
        }
        Ok(revisions)
    }

    pub async fn list_revision_summaries(
        &self,
        funnel_id: &str,
    ) -> RepositoryResult<Vec<RevisionSummary>> {
        let funnel_id = D1Type::Text(funnel_id);
        let limit = D1Type::Integer(MAX_REVISIONS_RETAINED);
        let result = self
            .database
            .prepare(
                "SELECT id, funnel_id, schema_version, created_at FROM revisions WHERE funnel_id = ? ORDER BY created_at DESC, id DESC LIMIT ?",
            )
            .bind_refs([&funnel_id, &limit])?
            .all()
            .await?;
        let revisions = result.results::<RevisionSummary>()?;
        for revision in &revisions {
            validate_schema_version(revision.schema_version)?;
        }
        Ok(revisions)
    }

    pub async fn delete_revision(&self, id: &str) -> RepositoryResult<bool> {
        let id = D1Type::Text(id);
        let deleted = self
            .database
            .prepare(
                "DELETE FROM revisions WHERE id = ? AND NOT EXISTS (SELECT 1 FROM funnels WHERE published_revision_id = ?) AND NOT EXISTS (SELECT 1 FROM funnel_editor_heads WHERE revision_id = ?) RETURNING id",
            )
            .bind_refs([&id, &id, &id])?
            .first::<String>(Some("id"))
            .await?;
        Ok(deleted.is_some())
    }

    pub async fn publish_revision(
        &self,
        funnel_id: &str,
        revision_id: &str,
        expected_published_revision_id: Option<&str>,
    ) -> RepositoryResult<bool> {
        let funnel_id = D1Type::Text(funnel_id);
        let revision_id = D1Type::Text(revision_id);
        let expected = expected_published_revision_id
            .map(D1Type::Text)
            .unwrap_or(D1Type::Null);
        let supported_schema_version = D1Type::Integer(SUPPORTED_SCHEMA_VERSION);
        let published = self
            .database
            .prepare(
                "UPDATE funnels SET published_revision_id = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ? AND published_revision_id IS ? AND published_revision_id IS NOT ? AND EXISTS (SELECT 1 FROM revisions WHERE id = ? AND funnel_id = ? AND schema_version <= ?) RETURNING id",
            )
            .bind_refs([
                &revision_id,
                &funnel_id,
                &expected,
                &revision_id,
                &revision_id,
                &funnel_id,
                &supported_schema_version,
            ])?
            .first::<String>(Some("id"))
            .await?;
        Ok(published.is_some())
    }

    pub async fn set_published_revision_idempotently(
        &self,
        mutation: &PublicationMutation<'_>,
    ) -> RepositoryResult<MutationResult> {
        validate_required_field(mutation.funnel_id, "funnel_id")?;
        validate_required_field(mutation.command_id, "command_id")?;
        validate_idempotency_key(mutation.idempotency_key)?;
        if let Some(revision_id) = mutation.published_revision_id {
            validate_required_field(revision_id, "revision_id")?;
        }
        self.validate_json(mutation.payload_json).await?;
        self.validate_json(mutation.result_json).await?;

        let funnel = D1Type::Text(mutation.funnel_id);
        let target = mutation
            .published_revision_id
            .map(D1Type::Text)
            .unwrap_or(D1Type::Null);
        let expected = mutation
            .expected_published_revision_id
            .map(D1Type::Text)
            .unwrap_or(D1Type::Null);
        let target_schema = D1Type::Integer(SUPPORTED_SCHEMA_VERSION);
        let key = D1Type::Text(mutation.idempotency_key);
        let command = D1Type::Text(mutation.command_id);
        let payload = D1Type::Text(mutation.payload_json);
        let result = D1Type::Text(mutation.result_json);
        let delete_expired_receipts = self.database.prepare(
            "DELETE FROM command_receipts WHERE expires_at <= CAST(strftime('%s', 'now') AS INTEGER)",
        );
        let insert_receipt = self
            .database
            .prepare(
                "INSERT INTO command_receipts (idempotency_key, command_id, payload_json, result_json, expires_at) SELECT ?, ?, ?, ?, CAST(strftime('%s', 'now') AS INTEGER) + 86400 WHERE EXISTS (SELECT 1 FROM funnels WHERE id = ? AND published_revision_id IS ? AND published_revision_id IS NOT ?) AND (? IS NULL OR EXISTS (SELECT 1 FROM revisions WHERE id = ? AND funnel_id = ? AND schema_version <= ?)) ON CONFLICT(idempotency_key) DO UPDATE SET command_id = excluded.command_id, payload_json = excluded.payload_json, result_json = excluded.result_json, expires_at = excluded.expires_at WHERE command_receipts.expires_at <= CAST(strftime('%s', 'now') AS INTEGER)",
            )
            .bind_refs([
                &key,
                &command,
                &payload,
                &result,
                &funnel,
                &expected,
                &target,
                &target,
                &target,
                &funnel,
                &target_schema,
            ])?;
        let update_funnel = self
            .database
            .prepare(
                "UPDATE funnels SET published_revision_id = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ? AND published_revision_id IS ? AND published_revision_id IS NOT ? AND (? IS NULL OR EXISTS (SELECT 1 FROM revisions WHERE id = ? AND funnel_id = ? AND schema_version <= ?)) AND EXISTS (SELECT 1 FROM command_receipts WHERE idempotency_key = ? AND command_id = ? AND payload_json = ? AND result_json = ? AND expires_at > CAST(strftime('%s', 'now') AS INTEGER)) AND changes() = 1",
            )
            .bind_refs([
                &target,
                &funnel,
                &expected,
                &target,
                &target,
                &target,
                &funnel,
                &target_schema,
                &key,
                &command,
                &payload,
                &result,
            ])?;

        if let Err(error) = self
            .database
            .batch(vec![delete_expired_receipts, insert_receipt, update_funnel])
            .await
        {
            if let Some(receipt) = self.active_receipt(mutation.idempotency_key).await? {
                return Ok(classify_receipt(
                    receipt,
                    mutation.command_id,
                    mutation.payload_json,
                ));
            }
            self.classify_publication_failure(
                mutation.funnel_id,
                mutation.published_revision_id,
                mutation.expected_published_revision_id,
            )
            .await?;
            return Err(error.into());
        }

        match self.active_receipt(mutation.idempotency_key).await? {
            Some(receipt) => Ok(classify_receipt(
                receipt,
                mutation.command_id,
                mutation.payload_json,
            )),
            None => {
                self.classify_publication_failure(
                    mutation.funnel_id,
                    mutation.published_revision_id,
                    mutation.expected_published_revision_id,
                )
                .await?;
                Err(Error::RustError("publication receipt was not committed".to_owned()).into())
            }
        }
    }

    async fn classify_publication_failure(
        &self,
        funnel_id: &str,
        published_revision_id: Option<&str>,
        expected_published_revision_id: Option<&str>,
    ) -> RepositoryResult<()> {
        let funnel = self
            .get_funnel(funnel_id)
            .await?
            .ok_or(RepositoryError::FunnelNotFound)?;
        if let Some(revision_id) = published_revision_id {
            if self
                .get_owned_revision(funnel_id, revision_id)
                .await?
                .is_none()
            {
                return Err(RepositoryError::RevisionNotFound);
            }
        }
        if funnel.published_revision_id.as_deref() != expected_published_revision_id
            || funnel.published_revision_id.as_deref() == published_revision_id
        {
            return Err(RepositoryError::StalePublishedRevision);
        }
        Ok(())
    }

    pub async fn store_public_form_submission(
        &self,
        input: NewPublicFormSubmission<'_>,
    ) -> RepositoryResult<PublicFormSubmissionResult> {
        for (value, field) in [
            (input.submission_id, "submission_id"),
            (input.funnel_id, "funnel_id"),
            (input.funnel_slug, "funnel_slug"),
            (input.revision_id, "revision_id"),
            (input.page_id, "page_id"),
            (input.form_id, "form_id"),
            (input.submission_key, "submission_key"),
            (
                input.request_consent_copy_version,
                "request_consent_copy_version",
            ),
            (input.request_consent_copy, "request_consent_copy"),
        ] {
            validate_required_field(value, field)?;
        }
        validate_idempotency_key(input.submission_id)?;
        validate_idempotency_key(input.submission_key)?;
        if input.field_values_json.len() > crate::MAX_PUBLIC_FORM_SUBMISSION_BYTES {
            return Err(RepositoryError::PayloadTooLarge);
        }
        let field_values: serde_json::Value = serde_json::from_str(input.field_values_json)
            .map_err(|_| RepositoryError::InvalidJson)?;
        if !field_values.is_object() {
            return Err(RepositoryError::InvalidJson);
        }
        let mut fingerprint_payload = serde_json::json!({
            "funnel_id": input.funnel_id,
            "funnel_slug": input.funnel_slug,
            "revision_id": input.revision_id,
            "page_id": input.page_id,
            "form_id": input.form_id,
            "field_values": field_values,
            "request_consent_acknowledged": true,
            "request_consent_copy_version": input.request_consent_copy_version,
            "request_consent_copy": input.request_consent_copy,
            "marketing_opt_in": input.marketing_opt_in,
            "marketing_opt_in_copy_version": input.marketing_opt_in_copy_version,
            "marketing_opt_in_copy": input.marketing_opt_in_copy,
        });
        if input.submission_channel != FormSubmissionChannel::Unattributed {
            fingerprint_payload
                .as_object_mut()
                .ok_or(RepositoryError::InvalidJson)?
                .insert(
                    "submission_channel".to_owned(),
                    serde_json::Value::String(input.submission_channel.as_str().to_owned()),
                );
        }
        let fingerprint_payload = serde_json_canonicalizer::to_vec(&fingerprint_payload)
            .map_err(|_| RepositoryError::InvalidJson)?;
        let mut fingerprint_hasher = Sha256::new();
        fingerprint_hasher.update(b"eezfunnel-public-form-submission-v1\0");
        fingerprint_hasher.update(fingerprint_payload);
        let digest = fingerprint_hasher.finalize();
        let mut request_fingerprint = String::with_capacity(digest.len() * 2);
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(request_fingerprint, "{byte:02x}");
        }
        if input.request_consent_copy_version.len() > 64
            || input.request_consent_copy.len() > 1024
            || !(86_400..=315_360_000).contains(&input.retention_seconds)
            || !(1..=100).contains(&input.limit_per_minute)
        {
            return Err(RepositoryError::InvalidField("public_form_submission"));
        }
        if input.marketing_opt_in_copy_version.is_some() != input.marketing_opt_in_copy.is_some()
            || (input.marketing_opt_in && input.marketing_opt_in_copy_version.is_none())
            || input
                .marketing_opt_in_copy_version
                .is_some_and(|value| value.is_empty() || value.len() > 64)
            || input
                .marketing_opt_in_copy
                .is_some_and(|value| value.is_empty() || value.len() > 1024)
        {
            return Err(RepositoryError::InvalidField("marketing_opt_in"));
        }

        let submission_id = D1Type::Text(input.submission_id);
        let funnel_id = D1Type::Text(input.funnel_id);
        let funnel_slug = D1Type::Text(input.funnel_slug);
        let revision_id = D1Type::Text(input.revision_id);
        let page_id = D1Type::Text(input.page_id);
        let form_id = D1Type::Text(input.form_id);
        let submission_channel = D1Type::Text(input.submission_channel.as_str());
        let submission_key = D1Type::Text(input.submission_key);
        let erased_replay_digest = crate::submission_lifecycle::replay_digest(
            input.funnel_id,
            input.form_id,
            input.submission_key,
        )
        .map_err(|_| RepositoryError::InvalidField("submission_key"))?;
        let erased_replay_digest = D1Type::Text(&erased_replay_digest);
        let request_fingerprint = D1Type::Text(&request_fingerprint);
        let field_values_json = D1Type::Text(input.field_values_json);
        let request_consent_copy_version = D1Type::Text(input.request_consent_copy_version);
        let request_consent_copy = D1Type::Text(input.request_consent_copy);
        let marketing_opt_in = D1Type::Boolean(input.marketing_opt_in);
        let marketing_opt_in_copy_version = input
            .marketing_opt_in_copy_version
            .map(D1Type::Text)
            .unwrap_or(D1Type::Null);
        let marketing_opt_in_copy = input
            .marketing_opt_in_copy
            .map(D1Type::Text)
            .unwrap_or(D1Type::Null);
        let retention_seconds = D1Type::Integer(input.retention_seconds);
        let limit_per_minute = D1Type::Integer(input.limit_per_minute);

        let canonical_lookup = self
            .database
            .prepare(
                "SELECT (SELECT submission_id FROM public_form_submissions WHERE funnel_id = ? AND form_id = ? AND submission_key = ?) AS canonical_submission_id",
            )
            .bind_refs([&funnel_id, &form_id, &submission_key])?;
        let insert_submission = self
            .database
            .prepare(
                "INSERT INTO public_form_submissions (submission_id, funnel_id, revision_id, page_id, form_id, submission_channel, submission_key, request_fingerprint, field_values_json, request_consent_acknowledged, request_consent_copy_version, request_consent_copy, request_consent_recorded_at, marketing_opt_in, marketing_opt_in_copy_version, marketing_opt_in_copy, marketing_opt_in_recorded_at, submitted_at, expires_at) SELECT ?, funnels.id, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, unixepoch(), ?, ?, ?, CASE WHEN ? IS NOT NULL THEN unixepoch() ELSE NULL END, unixepoch(), unixepoch() + ? FROM funnels JOIN revisions ON revisions.id = funnels.published_revision_id AND revisions.funnel_id = funnels.id WHERE funnels.id = ? AND funnels.slug = ? AND revisions.id = ? AND revisions.schema_version = 2 AND (SELECT COUNT(*) FROM public_form_submissions WHERE funnel_id = funnels.id AND submitted_at >= unixepoch() - 60) < ? AND NOT EXISTS (SELECT 1 FROM public_submission_erasure_markers WHERE replay_digest = ? AND expires_at > unixepoch()) ON CONFLICT(funnel_id, form_id, submission_key) DO NOTHING RETURNING submission_id",
            )
            .bind_refs([
                &submission_id,
                &revision_id,
                &page_id,
                &form_id,
                &submission_channel,
                &submission_key,
                &request_fingerprint,
                &field_values_json,
                &request_consent_copy_version,
                &request_consent_copy,
                &marketing_opt_in,
                &marketing_opt_in_copy_version,
                &marketing_opt_in_copy,
                &marketing_opt_in_copy_version,
                &retention_seconds,
                &funnel_id,
                &funnel_slug,
                &revision_id,
                &limit_per_minute,
                &erased_replay_digest,
            ])?;
        let insert_outbox = self
            .database
            .prepare(
                "INSERT INTO public_submission_delivery_outbox (task_id, source_submission_id, funnel_id, revision_id, page_id, form_id, destination_version_id, mapping_version_id, state, created_at) SELECT source.submission_id, source.submission_id, source.funnel_id, source.revision_id, source.page_id, source.form_id, installation.destination_version_id, settings.mapping_version_id, 'pending', source.submitted_at FROM public_form_submissions AS source JOIN webhook_installation_delivery AS installation ON installation.installation_id = 1 AND installation.enabled = 1 JOIN public_form_delivery_settings AS settings ON settings.funnel_id = source.funnel_id AND settings.form_id = source.form_id AND settings.enabled = 1 AND settings.expected_destination_version_id = installation.destination_version_id JOIN public_form_delivery_mapping_versions AS mapping ON mapping.mapping_version_id = settings.mapping_version_id AND mapping.funnel_id = source.funnel_id AND mapping.form_id = source.form_id AND mapping.destination_version_id = installation.destination_version_id WHERE source.submission_id = ? AND changes() = 1 AND mapping.selected_field_contract_json IS NOT NULL AND NOT EXISTS (SELECT 1 FROM json_each(mapping.selected_fields_json) AS selected WHERE NOT EXISTS (SELECT 1 FROM json_each(mapping.selected_field_contract_json) AS contract WHERE json_extract(contract.value, '$.id') = selected.value)) AND NOT EXISTS (SELECT 1 FROM json_each(mapping.selected_field_contract_json) AS contract WHERE NOT EXISTS (SELECT 1 FROM json_each(mapping.selected_fields_json) AS selected WHERE selected.value = json_extract(contract.value, '$.id'))) AND NOT EXISTS (SELECT 1 FROM json_each(mapping.selected_field_contract_json) AS contract WHERE NOT EXISTS (SELECT 1 FROM revisions AS source_revision, json_each(source_revision.data_json, '$.forms') AS saved_form, json_each(saved_form.value, '$.fields') AS saved_field WHERE source_revision.id = source.revision_id AND source_revision.funnel_id = source.funnel_id AND json_extract(saved_form.value, '$.id') = source.form_id AND json_extract(saved_field.value, '$.id') = json_extract(contract.value, '$.id') AND json(saved_field.value) = json(contract.value)))",
            )
            .bind_refs([&submission_id])?;
        let results = self
            .database
            .batch(vec![canonical_lookup, insert_submission, insert_outbox])
            .await?;
        let canonical_state = results
            .first()
            .ok_or(RepositoryError::InvalidField("public_form_submission"))?
            .results::<PublicFormSubmissionCanonicalState>()?
            .into_iter()
            .next()
            .ok_or(RepositoryError::InvalidField("public_form_submission"))?;
        let inserted = results
            .get(1)
            .ok_or(RepositoryError::InvalidField("public_form_submission"))?
            .results::<PublicFormSubmissionId>()?
            .into_iter()
            .next();
        if let Some(inserted) = inserted {
            return Ok(PublicFormSubmissionResult::Stored {
                submission_id: inserted.submission_id,
            });
        }

        let state = self
            .database
            .prepare(
                "SELECT (SELECT submission_id FROM public_form_submissions WHERE funnel_id = ? AND form_id = ? AND submission_key = ?) AS canonical_submission_id, EXISTS(SELECT 1 FROM funnels WHERE id = ? AND slug = ? AND published_revision_id = ?) AS is_current, EXISTS(SELECT 1 FROM public_form_submissions WHERE funnel_id = ? AND form_id = ? AND submission_key = ? AND request_fingerprint = ?) AS is_duplicate, EXISTS(SELECT 1 FROM public_form_submissions WHERE funnel_id = ? AND form_id = ? AND submission_key = ?) AS is_key_reuse, (SELECT COUNT(*) FROM public_form_submissions WHERE funnel_id = ? AND submitted_at >= unixepoch() - 60) >= ? AS is_rate_limited, EXISTS(SELECT 1 FROM public_submission_erasure_markers WHERE replay_digest = ? AND expires_at > unixepoch()) AS is_erased_replay",
            )
            .bind_refs([
                &funnel_id,
                &form_id,
                &submission_key,
                &funnel_id,
                &funnel_slug,
                &revision_id,
                &funnel_id,
                &form_id,
                &submission_key,
                &request_fingerprint,
                &funnel_id,
                &form_id,
                &submission_key,
                &funnel_id,
                &limit_per_minute,
                &erased_replay_digest,
            ])?
            .first::<PublicFormSubmissionState>(None)
            .await?
            .ok_or(RepositoryError::InvalidField("public_form_submission"))?;

        if state.is_current != 1 {
            return Ok(PublicFormSubmissionResult::NotCurrent);
        }
        if state.is_erased_replay == 1 {
            return Ok(PublicFormSubmissionResult::ErasedReplay);
        }
        if state.is_duplicate == 1 {
            let canonical_submission_id = state
                .canonical_submission_id
                .or(canonical_state.canonical_submission_id)
                .ok_or(RepositoryError::InvalidField("public_form_submission"))?;
            return Ok(PublicFormSubmissionResult::Duplicate {
                submission_id: canonical_submission_id,
            });
        }
        if state.is_key_reuse == 1 {
            return Ok(PublicFormSubmissionResult::KeyReuseMismatch);
        }
        if state.is_rate_limited == 1 {
            return Ok(PublicFormSubmissionResult::RateLimited);
        }
        Err(RepositoryError::InvalidField("public_form_submission"))
    }

    pub async fn cleanup_expired_public_form_submissions(&self) -> RepositoryResult<()> {
        for _ in 0..PUBLIC_FORM_SUBMISSION_CLEANUP_MAX_BATCHES {
            let batch_size = D1Type::Integer(PUBLIC_FORM_SUBMISSION_CLEANUP_BATCH_SIZE);
            let deleted = self
                .database
                .prepare(
                    "DELETE FROM public_form_submissions WHERE submission_id IN (SELECT source.submission_id FROM public_form_submissions AS source WHERE source.expires_at <= unixepoch() AND NOT EXISTS (SELECT 1 FROM public_submission_erasure_intents AS intent WHERE intent.submission_id = source.submission_id) ORDER BY source.expires_at LIMIT ?)",
                )
                .bind_refs([&batch_size])?
                .run()
                .await?
                .meta()?
                .and_then(|meta| meta.changes)
                .unwrap_or(PUBLIC_FORM_SUBMISSION_CLEANUP_BATCH_SIZE as usize);
            if deleted < PUBLIC_FORM_SUBMISSION_CLEANUP_BATCH_SIZE as usize {
                break;
            }
        }
        Ok(())
    }

    pub async fn store_public_interaction_event(
        &self,
        input: NewPublicInteractionEvent<'_>,
    ) -> RepositoryResult<PublicInteractionEventResult> {
        for (value, field) in [
            (input.event_id, "event_id"),
            (input.funnel_id, "funnel_id"),
            (input.funnel_slug, "funnel_slug"),
            (input.revision_id, "revision_id"),
            (input.page_id, "page_id"),
            (input.event_kind, "event_kind"),
            (input.replay_fingerprint, "replay_fingerprint"),
        ] {
            validate_required_field(value, field)?;
        }
        let event_id = uuid::Uuid::parse_str(input.event_id)
            .map_err(|_| RepositoryError::InvalidField("event_id"))?;
        if event_id.get_version_num() != 4
            || event_id.hyphenated().to_string() != input.event_id
            || !valid_interaction_reference(input.funnel_id)
            || !valid_interaction_reference(input.revision_id)
            || !valid_interaction_reference(input.page_id)
            || input
                .form_id
                .is_some_and(|value| !valid_interaction_reference(value))
            || input
                .campaign_id
                .is_some_and(|value| !valid_interaction_reference(value))
            || !matches!(
                input.event_kind,
                "page_viewed" | "form_viewed" | "form_started"
            )
            || (input.event_kind == "page_viewed") != input.form_id.is_none()
            || input.replay_fingerprint.len() != 64
            || !input
                .replay_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || !(1..=90).contains(&input.retention_days)
            || !(1..=10_000).contains(&input.limit_per_minute)
        {
            return Err(RepositoryError::InvalidField("public_interaction_event"));
        }

        let event_id = D1Type::Text(input.event_id);
        let funnel_id = D1Type::Text(input.funnel_id);
        let funnel_slug = D1Type::Text(input.funnel_slug);
        let revision_id = D1Type::Text(input.revision_id);
        let page_id = D1Type::Text(input.page_id);
        let event_kind = D1Type::Text(input.event_kind);
        let form_id = input.form_id.map(D1Type::Text).unwrap_or(D1Type::Null);
        let campaign_id = input.campaign_id.map(D1Type::Text).unwrap_or(D1Type::Null);
        let replay_fingerprint = D1Type::Text(input.replay_fingerprint);
        let retention_seconds = D1Type::Integer(i32::from(input.retention_days) * 86_400);
        let limit_per_minute = D1Type::Integer(input.limit_per_minute);

        let inserted = self
            .database
            .prepare(
                "INSERT INTO public_interaction_events (event_id, funnel_id, revision_id, page_id, event_kind, form_id, campaign_id, replay_fingerprint, received_at, expires_at) SELECT ?, funnels.id, ?, ?, ?, ?, ?, ?, unixepoch(), unixepoch() + ? FROM funnels JOIN revisions ON revisions.id = funnels.published_revision_id AND revisions.funnel_id = funnels.id WHERE funnels.id = ? AND funnels.slug = ? AND funnels.published_revision_id = ? AND revisions.schema_version IN (1, 2) AND (SELECT COUNT(*) FROM public_interaction_events WHERE funnel_id = funnels.id AND received_at >= unixepoch() - 60) < ? ON CONFLICT(event_id) DO NOTHING RETURNING event_id",
            )
            .bind_refs([
                &event_id,
                &revision_id,
                &page_id,
                &event_kind,
                &form_id,
                &campaign_id,
                &replay_fingerprint,
                &retention_seconds,
                &funnel_id,
                &funnel_slug,
                &revision_id,
                &limit_per_minute,
            ])?
            .first::<String>(Some("event_id"))
            .await?;
        if inserted.is_some() {
            return Ok(PublicInteractionEventResult::Stored);
        }

        let state = self
            .database
            .prepare(
                "SELECT EXISTS(SELECT 1 FROM funnels JOIN revisions ON revisions.id = funnels.published_revision_id AND revisions.funnel_id = funnels.id WHERE funnels.id = ? AND funnels.slug = ? AND funnels.published_revision_id = ? AND revisions.schema_version IN (1, 2)) AS is_current, EXISTS(SELECT 1 FROM public_interaction_events WHERE event_id = ? AND replay_fingerprint = ?) AS is_duplicate, EXISTS(SELECT 1 FROM public_interaction_events WHERE event_id = ?) AS is_key_reuse, (SELECT COUNT(*) FROM public_interaction_events WHERE funnel_id = ? AND received_at >= unixepoch() - 60) >= ? AS is_rate_limited",
            )
            .bind_refs([
                &funnel_id,
                &funnel_slug,
                &revision_id,
                &event_id,
                &replay_fingerprint,
                &event_id,
                &funnel_id,
                &limit_per_minute,
            ])?
            .first::<PublicInteractionEventState>(None)
            .await?
            .ok_or(RepositoryError::InvalidField("public_interaction_event"))?;

        if state.is_current != 1 {
            return Ok(PublicInteractionEventResult::NotCurrent);
        }
        if state.is_duplicate == 1 {
            return Ok(PublicInteractionEventResult::Duplicate);
        }
        if state.is_key_reuse == 1 {
            return Ok(PublicInteractionEventResult::KeyReuseMismatch);
        }
        if state.is_rate_limited == 1 {
            return Ok(PublicInteractionEventResult::RateLimited);
        }
        Err(RepositoryError::InvalidField("public_interaction_event"))
    }

    pub async fn cleanup_expired_public_interaction_events(&self) -> RepositoryResult<()> {
        for _ in 0..PUBLIC_INTERACTION_EVENT_CLEANUP_MAX_BATCHES {
            let batch_size = D1Type::Integer(PUBLIC_INTERACTION_EVENT_CLEANUP_BATCH_SIZE);
            let deleted = self
                .database
                .prepare(
                    "DELETE FROM public_interaction_events WHERE event_id IN (SELECT event_id FROM public_interaction_events WHERE expires_at <= unixepoch() ORDER BY expires_at LIMIT ?)",
                )
                .bind_refs([&batch_size])?
                .run()
                .await?
                .meta()?
                .and_then(|meta| meta.changes)
                .unwrap_or(PUBLIC_INTERACTION_EVENT_CLEANUP_BATCH_SIZE as usize);
            if deleted < PUBLIC_INTERACTION_EVENT_CLEANUP_BATCH_SIZE as usize {
                break;
            }
        }
        Ok(())
    }

    pub async fn list_public_form_submissions(
        &self,
        cursor: Option<&PublicFormSubmissionCursor>,
        status: Option<FormSubmissionStatus>,
        funnel_id: Option<&str>,
    ) -> RepositoryResult<PublicFormSubmissionPage> {
        if cursor.is_some_and(|cursor| {
            cursor.status != status || cursor.funnel_id.as_deref() != funnel_id
        }) {
            return Err(RepositoryError::InvalidField("submission_cursor"));
        }
        if funnel_id.is_some_and(|funnel_id| {
            uuid::Uuid::parse_str(funnel_id)
                .map(|parsed| parsed.to_string() != funnel_id)
                .unwrap_or(true)
        }) {
            return Err(RepositoryError::InvalidField("funnel_id"));
        }
        if let Some(cursor) = cursor {
            if cursor.submitted_at <= 0
                || cursor.submitted_at > 9_007_199_254_740_991
                || uuid::Uuid::parse_str(&cursor.submission_id)
                    .map(|submission_id| submission_id.to_string() != cursor.submission_id)
                    .unwrap_or(true)
            {
                return Err(RepositoryError::InvalidField("submission_cursor"));
            }
        }

        let cursor_timestamp = cursor
            .map(|cursor| D1Type::Real(cursor.submitted_at as f64))
            .unwrap_or(D1Type::Null);
        let cursor_timestamp_tiebreak = cursor
            .map(|cursor| D1Type::Real(cursor.submitted_at as f64))
            .unwrap_or(D1Type::Null);
        let cursor_id = cursor
            .map(|cursor| D1Type::Text(&cursor.submission_id))
            .unwrap_or(D1Type::Null);
        let cursor_timestamp_guard = cursor
            .map(|cursor| D1Type::Real(cursor.submitted_at as f64))
            .unwrap_or(D1Type::Null);
        let status_predicate = if status.is_some() {
            " AND public_form_submissions.triage_status = ?"
        } else {
            ""
        };
        let funnel_predicate = if funnel_id.is_some() {
            " AND public_form_submissions.funnel_id = ?"
        } else {
            ""
        };
        let limit = D1Type::Integer((PUBLIC_FORM_SUBMISSION_PAGE_SIZE + 1) as i32);
        let query = format!(
            "SELECT public_form_submissions.submission_id, public_form_submissions.triage_status, public_form_submissions.funnel_id, funnels.name AS funnel_name, funnels.slug AS funnel_slug, public_form_submissions.revision_id, revisions.schema_version AS revision_schema_version, public_form_submissions.page_id, public_form_submissions.form_id, public_form_submissions.field_values_json, public_form_submissions.request_consent_acknowledged, public_form_submissions.request_consent_copy_version, public_form_submissions.request_consent_copy, public_form_submissions.request_consent_recorded_at, strftime('%Y-%m-%d %H:%M UTC', public_form_submissions.request_consent_recorded_at, 'unixepoch') AS request_consent_recorded_at_label, public_form_submissions.marketing_opt_in, public_form_submissions.marketing_opt_in_copy_version, public_form_submissions.marketing_opt_in_copy, public_form_submissions.marketing_opt_in_recorded_at, CASE WHEN public_form_submissions.marketing_opt_in_recorded_at IS NOT NULL THEN strftime('%Y-%m-%d %H:%M UTC', public_form_submissions.marketing_opt_in_recorded_at, 'unixepoch') ELSE NULL END AS marketing_opt_in_recorded_at_label, public_form_submissions.submitted_at, strftime('%Y-%m-%d %H:%M UTC', public_form_submissions.submitted_at, 'unixepoch') AS submitted_at_label, strftime('%Y-%m-%dT%H:%M:%SZ', public_form_submissions.submitted_at, 'unixepoch') AS submitted_at_datetime, public_form_submissions.expires_at, strftime('%Y-%m-%d %H:%M UTC', public_form_submissions.expires_at, 'unixepoch') AS expires_at_label, delivery_state.status AS delivery_state, CASE WHEN delivery_state.status = 'pending' AND delivery_state.attempt_count > 0 AND delivery_state.next_attempt_at > unixepoch() THEN 1 ELSE 0 END AS delivery_retry_scheduled, CASE WHEN delivery.created_at IS NOT NULL AND delivery.created_at + 86400 > unixepoch() THEN 1 ELSE 0 END AS delivery_retry_window_open, delivery_state.attempt_count AS delivery_attempt_count, CASE WHEN delivery_state.last_attempt_at IS NOT NULL THEN strftime('%Y-%m-%d %H:%M UTC', delivery_state.last_attempt_at, 'unixepoch') ELSE NULL END AS delivery_last_attempt_at_label, CASE WHEN delivery_state.status = 'pending' AND delivery_state.next_attempt_at > unixepoch() THEN strftime('%Y-%m-%d %H:%M UTC', delivery_state.next_attempt_at, 'unixepoch') ELSE NULL END AS delivery_next_attempt_at_label, delivery_state.last_http_status AS delivery_last_http_status, delivery_state.last_outcome_code AS delivery_last_outcome_code FROM public_form_submissions JOIN funnels ON funnels.id = public_form_submissions.funnel_id JOIN revisions ON revisions.id = public_form_submissions.revision_id AND revisions.funnel_id = funnels.id LEFT JOIN public_submission_delivery_outbox AS delivery ON delivery.source_submission_id = public_form_submissions.submission_id LEFT JOIN public_submission_delivery_state AS delivery_state ON delivery_state.task_id = delivery.task_id WHERE public_form_submissions.expires_at > unixepoch() AND revisions.schema_version = 2 AND NOT EXISTS (SELECT 1 FROM public_submission_erasure_intents AS intent WHERE intent.submission_id = public_form_submissions.submission_id){funnel_predicate}{status_predicate} AND (? IS NULL OR public_form_submissions.submitted_at < ? OR (public_form_submissions.submitted_at = ? AND public_form_submissions.submission_id < ?)) ORDER BY public_form_submissions.submitted_at DESC, public_form_submissions.submission_id DESC LIMIT ?"
        );
        let mut bindings = Vec::with_capacity(7);
        if let Some(funnel_id) = funnel_id {
            bindings.push(D1Type::Text(funnel_id));
        }
        if let Some(status) = status {
            bindings.push(D1Type::Text(status.as_str()));
        }
        bindings.extend([
            cursor_timestamp,
            cursor_timestamp_tiebreak,
            cursor_timestamp_guard,
            cursor_id,
            limit,
        ]);
        let binding_refs = bindings.iter().collect::<Vec<_>>();
        let mut items = self
            .database
            .prepare(&query)
            .bind_refs(binding_refs)?
            .all()
            .await?
            .results::<PublicFormSubmissionRecord>()?;
        let has_more = items.len() > PUBLIC_FORM_SUBMISSION_PAGE_SIZE;
        items.truncate(PUBLIC_FORM_SUBMISSION_PAGE_SIZE);
        Ok(PublicFormSubmissionPage { items, has_more })
    }

    pub async fn set_public_form_submission_status(
        &self,
        submission_id: &str,
        status: FormSubmissionStatus,
    ) -> RepositoryResult<bool> {
        if uuid::Uuid::parse_str(submission_id)
            .map(|parsed| parsed.to_string() != submission_id)
            .unwrap_or(true)
        {
            return Err(RepositoryError::InvalidField("submission_id"));
        }

        let submission_id = D1Type::Text(submission_id);
        let status = D1Type::Text(status.as_str());
        let updated = self
            .database
            .prepare(
                "UPDATE public_form_submissions SET triage_status = ? WHERE submission_id = ? AND expires_at > unixepoch() AND NOT EXISTS (SELECT 1 FROM public_submission_erasure_intents WHERE submission_id = public_form_submissions.submission_id) RETURNING submission_id",
            )
            .bind_refs([&status, &submission_id])?
            .first::<String>(Some("submission_id"))
            .await?;
        Ok(updated.is_some())
    }

    pub async fn form_completion_count_rows(
        &self,
        funnel_id: &str,
        period: FormCompletionPeriod,
    ) -> RepositoryResult<Vec<FormCompletionCountRow>> {
        let funnel_id = D1Type::Text(funnel_id);
        let start_offset = D1Type::Integer(i32::from(period.days() - 1));
        let period_days = D1Type::Integer(i32::from(period.days()));
        let rows = self
            .database
            .prepare(
                "WITH RECURSIVE clock(now) AS (SELECT unixepoch()), funnel(id) AS (SELECT id FROM funnels WHERE id = ?), bounds(now, start_at, end_at) AS (SELECT clock.now, unixepoch(date(clock.now, 'unixepoch', printf('-%d days', ?))), unixepoch(date(clock.now, 'unixepoch', '+1 day')) FROM clock), days(utc_day, offset) AS (SELECT date(bounds.start_at, 'unixepoch'), 0 FROM bounds UNION ALL SELECT date(days.utc_day, '+1 day'), days.offset + 1 FROM days WHERE days.offset + 1 < ?), retained AS MATERIALIZED (SELECT submissions.submitted_at, submissions.form_id, submissions.submission_channel FROM public_form_submissions AS submissions JOIN funnel ON funnel.id = submissions.funnel_id CROSS JOIN bounds WHERE submissions.submitted_at >= bounds.start_at AND submissions.submitted_at < bounds.end_at AND submissions.expires_at > bounds.now AND NOT EXISTS (SELECT 1 FROM public_submission_erasure_intents AS intent WHERE intent.submission_id = submissions.submission_id)), daily AS (SELECT strftime('%Y-%m-%d', submitted_at, 'unixepoch') AS utc_day, COUNT(*) AS completions FROM retained GROUP BY strftime('%Y-%m-%d', submitted_at, 'unixepoch')), forms AS (SELECT form_id, COUNT(*) AS completions FROM retained GROUP BY form_id), channels AS (SELECT submission_channel, COUNT(*) AS completions FROM retained GROUP BY submission_channel) SELECT 'day' AS row_type, days.utc_day, NULL AS form_id, NULL AS submission_channel, COALESCE(daily.completions, 0) AS completions FROM days CROSS JOIN funnel LEFT JOIN daily ON daily.utc_day = days.utc_day UNION ALL SELECT 'form' AS row_type, NULL AS utc_day, forms.form_id, NULL AS submission_channel, forms.completions FROM forms UNION ALL SELECT 'channel' AS row_type, NULL AS utc_day, NULL AS form_id, channels.submission_channel, channels.completions FROM channels ORDER BY row_type ASC, utc_day ASC, form_id ASC, submission_channel ASC",
            )
            .bind_refs([
                &funnel_id,
                &start_offset,
                &period_days,
            ])?
            .all()
            .await?
            .results::<FormCompletionCountRow>()?;
        Ok(rows)
    }

    pub async fn public_interaction_report_rows(
        &self,
        funnel_id: &str,
        period: InteractionReportPeriod,
    ) -> RepositoryResult<Vec<PublicInteractionReportRow>> {
        validate_required_field(funnel_id, "funnel_id")?;
        let funnel_id = D1Type::Text(funnel_id);
        let start_offset = D1Type::Integer(i32::from(period.days() - 1));
        let period_days = D1Type::Integer(i32::from(period.days()));
        let rows = self
            .database
            .prepare(PUBLIC_INTERACTION_REPORT_QUERY)
            .bind_refs([&funnel_id, &start_offset, &period_days])?
            .all()
            .await?
            .results::<PublicInteractionReportRow>()?;
        Ok(rows)
    }

    pub async fn list_public_form_submission_revisions(
        &self,
        revision_ids: &[String],
    ) -> RepositoryResult<Vec<PublicFormSubmissionRevision>> {
        let revision_ids = revision_ids.iter().cloned().collect::<BTreeSet<_>>();
        if revision_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = std::iter::repeat("?")
            .take(revision_ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let query = format!(
            "SELECT id, funnel_id, schema_version, data_json FROM revisions WHERE schema_version = 2 AND id IN ({placeholders})"
        );
        let bindings = revision_ids
            .iter()
            .map(|revision_id| D1Type::Text(revision_id.as_str()))
            .collect::<Vec<_>>();
        let binding_refs = bindings.iter().collect::<Vec<_>>();
        let revisions = self
            .database
            .prepare(&query)
            .bind_refs(binding_refs)?
            .all()
            .await?
            .results::<PublicFormSubmissionRevision>()?;
        if revisions.len() != revision_ids.len() {
            return Err(RepositoryError::InvalidField("submission_revision"));
        }
        Ok(revisions)
    }

    pub async fn get_published_revision_by_slug(
        &self,
        slug: &str,
    ) -> RepositoryResult<Option<RevisionRecord>> {
        validate_required_field(slug, "slug")?;
        let slug = D1Type::Text(slug);
        let revision = self
            .database
            .prepare(
                "SELECT revisions.id, revisions.funnel_id, revisions.schema_version, revisions.data_json, revisions.render_manifest_json, revisions.created_at FROM funnels JOIN revisions ON revisions.id = funnels.published_revision_id AND revisions.funnel_id = funnels.id WHERE funnels.slug = ? AND funnels.published_revision_id IS NOT NULL",
            )
            .bind_refs([&slug])?
            .first::<RevisionRecord>(None)
            .await?;
        if let Some(revision) = revision {
            validate_schema_version(revision.schema_version)?;
            Ok(Some(revision))
        } else {
            Ok(None)
        }
    }

    pub async fn create_theme(&self, input: &NewTheme) -> RepositoryResult<ThemeRecord> {
        validate_required_field(&input.id, "id")?;
        validate_required_field(&input.name, "name")?;
        self.validate_json(&input.tokens_json).await?;
        let id = D1Type::Text(&input.id);
        let name = D1Type::Text(&input.name);
        let tokens_json = D1Type::Text(&input.tokens_json);
        let is_default = D1Type::Boolean(input.is_default);
        self.database
            .prepare("INSERT INTO themes (id, name, tokens_json, is_default) VALUES (?, ?, ?, ?)")
            .bind_refs([&id, &name, &tokens_json, &is_default])?
            .run()
            .await?;
        self.get_theme(&input.id)
            .await?
            .ok_or_else(|| Error::RustError("created theme could not be read".to_owned()).into())
    }

    pub async fn get_theme(&self, id: &str) -> RepositoryResult<Option<ThemeRecord>> {
        let id = D1Type::Text(id);
        self.database
            .prepare(
                "SELECT id, name, tokens_json, is_default, created_at, updated_at FROM themes WHERE id = ?",
            )
            .bind_refs([&id])?
            .first::<ThemeRecord>(None)
            .await
            .map_err(Into::into)
    }

    pub async fn list_themes(&self) -> RepositoryResult<Vec<ThemeRecord>> {
        let result = self
            .database
            .prepare(
                "SELECT id, name, tokens_json, is_default, created_at, updated_at FROM themes ORDER BY name, id",
            )
            .all()
            .await?;
        result.results::<ThemeRecord>().map_err(Into::into)
    }

    pub async fn update_theme(
        &self,
        id: &str,
        name: &str,
        tokens_json: &str,
        is_default: bool,
    ) -> RepositoryResult<Option<ThemeRecord>> {
        validate_required_field(name, "name")?;
        self.validate_json(tokens_json).await?;
        let id = D1Type::Text(id);
        let name = D1Type::Text(name);
        let tokens_json = D1Type::Text(tokens_json);
        let is_default = D1Type::Boolean(is_default);
        let result = self
            .database
            .prepare(
                "UPDATE themes SET name = ?, tokens_json = ?, is_default = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ? RETURNING id, name, tokens_json, is_default, created_at, updated_at",
            )
            .bind_refs([&name, &tokens_json, &is_default, &id])?
            .first::<ThemeRecord>(None)
            .await?;
        Ok(result)
    }

    pub async fn delete_theme(&self, id: &str) -> RepositoryResult<bool> {
        let id = D1Type::Text(id);
        let deleted = self
            .database
            .prepare("DELETE FROM themes WHERE id = ? RETURNING id")
            .bind_refs([&id])?
            .first::<String>(Some("id"))
            .await?;
        Ok(deleted.is_some())
    }

    async fn validate_json(&self, value: &str) -> RepositoryResult<()> {
        let value = D1Type::Text(value);
        let is_valid = self
            .database
            .prepare("SELECT json_valid(?) AS is_valid")
            .bind_refs([&value])?
            .first::<i64>(Some("is_valid"))
            .await?
            .unwrap_or(0);
        if is_valid == 1 {
            Ok(())
        } else {
            Err(RepositoryError::InvalidJson)
        }
    }
}

pub fn validate_schema_version(version: i64) -> RepositoryResult<()> {
    if matches!(version, 1 | 2) {
        Ok(())
    } else {
        Err(RepositoryError::UnsupportedSchemaVersion(version))
    }
}

fn validate_legacy_revision_schema_version(version: i64) -> RepositoryResult<()> {
    if version == 1 {
        Ok(())
    } else {
        Err(RepositoryError::UnsupportedSchemaVersion(version))
    }
}

fn classify_receipt(
    receipt: CommandReceipt,
    command_id: &str,
    payload_json: &str,
) -> MutationResult {
    if receipt.command_id == command_id && receipt.payload_json == payload_json {
        MutationResult::Stored(receipt.result_json)
    } else {
        MutationResult::PayloadMismatch
    }
}

fn validate_idempotency_key(value: &str) -> RepositoryResult<()> {
    if !(1..=128).contains(&value.len())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        Err(RepositoryError::InvalidField("idempotency_key"))
    } else {
        Ok(())
    }
}

fn validate_required_field(value: &str, field: &'static str) -> RepositoryResult<()> {
    if value.trim().is_empty() {
        Err(RepositoryError::InvalidField(field))
    } else {
        Ok(())
    }
}

fn valid_interaction_reference(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn validate_payload_size(value: &str) -> RepositoryResult<()> {
    if value.len() > MAX_FUNNEL_BYTES {
        Err(RepositoryError::PayloadTooLarge)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        validate_legacy_revision_schema_version, validate_payload_size, validate_required_field,
        validate_schema_version, RepositoryError,
    };

    #[test]
    fn schema_versions_one_and_two_are_supported_for_storage_dispatch() {
        assert!(validate_schema_version(1).is_ok());
        assert!(validate_schema_version(2).is_ok());
    }

    #[test]
    fn legacy_revision_creation_rejects_v2_instead_of_relabeling_it_as_v1() {
        assert!(validate_legacy_revision_schema_version(1).is_ok());
        assert!(matches!(
            validate_legacy_revision_schema_version(2),
            Err(RepositoryError::UnsupportedSchemaVersion(2))
        ));
    }

    #[test]
    fn unsupported_schema_versions_are_rejected() {
        assert!(matches!(
            validate_schema_version(0),
            Err(RepositoryError::UnsupportedSchemaVersion(0))
        ));
        assert!(matches!(
            validate_schema_version(3),
            Err(RepositoryError::UnsupportedSchemaVersion(3))
        ));
        assert!(matches!(
            validate_schema_version(i64::MAX),
            Err(RepositoryError::UnsupportedSchemaVersion(i64::MAX))
        ));
    }

    #[test]
    fn payload_limit_counts_utf8_bytes() {
        let payload = "😀".repeat(262_144);
        assert!(validate_payload_size(&payload).is_ok());
        let payload_with_one_more_byte = format!("{payload}x");
        assert!(matches!(
            validate_payload_size(&payload_with_one_more_byte),
            Err(RepositoryError::PayloadTooLarge)
        ));
    }

    #[test]
    fn required_fields_reject_empty_and_whitespace_values() {
        assert!(validate_required_field("id", "id").is_ok());
        assert!(matches!(
            validate_required_field("  ", "id"),
            Err(RepositoryError::InvalidField("id"))
        ));
    }
}
