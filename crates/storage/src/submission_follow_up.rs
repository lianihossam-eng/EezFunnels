use std::fmt::{Display, Formatter};

use eez_domain::{FormSubmissionStatus, SubmissionFollowUp};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use worker::d1::D1Type;
use worker::{D1Database, Error};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubmissionFollowUpSnapshot {
    pub submission_id: String,
    pub version: i32,
    pub status: FormSubmissionStatus,
    pub next_action: Option<String>,
    pub due_on: Option<String>,
    pub close_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionFollowUpProvenance {
    OwnerFollowUp,
    LegacyStatus,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubmissionFollowUpEvent {
    pub version: i32,
    pub status: FormSubmissionStatus,
    pub next_action: Option<String>,
    pub due_on: Option<String>,
    pub close_reason: Option<String>,
    pub recorded_at: i64,
    pub recorded_at_label: String,
    pub provenance: SubmissionFollowUpProvenance,
    pub supersedes_version: Option<i32>,
    pub corrects_version: Option<i32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubmissionFollowUpHistory {
    pub current: SubmissionFollowUpSnapshot,
    pub events: Vec<SubmissionFollowUpEvent>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubmissionFollowUpMutationReceipt {
    pub submission_id: String,
    pub version: i32,
    pub replayed: bool,
}

#[derive(Debug)]
pub enum SubmissionFollowUpStorageError {
    Database(Error),
    NotFound,
    StaleVersion,
    CorrectionTargetUnavailable,
    IdempotencyPayloadMismatch,
    VersionExhausted,
    InvalidRecord,
}

impl SubmissionFollowUpStorageError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Database(_) | Self::InvalidRecord => "external/unavailable",
            Self::NotFound => "not_found/submission",
            Self::StaleVersion => "conflict/submission_follow_up_stale",
            Self::CorrectionTargetUnavailable => "conflict/submission_follow_up_correction_target",
            Self::IdempotencyPayloadMismatch => "conflict/idempotency_payload_mismatch",
            Self::VersionExhausted => "conflict/submission_follow_up_stale",
        }
    }
}

impl Display for SubmissionFollowUpStorageError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for SubmissionFollowUpStorageError {}

impl From<Error> for SubmissionFollowUpStorageError {
    fn from(error: Error) -> Self {
        Self::Database(error)
    }
}

#[derive(Deserialize)]
struct FollowUpReadRow {
    submission_id: String,
    follow_up_version: Option<i32>,
    current_status: Option<FormSubmissionStatus>,
    current_next_action: Option<String>,
    current_due_on: Option<String>,
    current_close_reason: Option<String>,
    event_version: Option<i32>,
    event_status: Option<FormSubmissionStatus>,
    event_next_action: Option<String>,
    event_due_on: Option<String>,
    event_close_reason: Option<String>,
    event_recorded_at: Option<i64>,
    event_recorded_at_label: Option<String>,
    event_provenance: Option<SubmissionFollowUpProvenance>,
    event_supersedes_version: Option<i32>,
    event_corrects_version: Option<i32>,
}

#[derive(Deserialize)]
struct FollowUpReceiptRow {
    submission_id: String,
    request_digest: String,
    result_version: i32,
}

#[derive(Deserialize)]
struct SubmissionVisibilityRow {
    present: i64,
}

pub struct SubmissionFollowUpRepository<'a> {
    database: &'a D1Database,
}

impl<'a> SubmissionFollowUpRepository<'a> {
    pub fn new(database: &'a D1Database) -> Self {
        Self { database }
    }

    pub async fn read(
        &self,
        submission_id: &str,
    ) -> Result<SubmissionFollowUpHistory, SubmissionFollowUpStorageError> {
        if !canonical_submission_id(submission_id) {
            return Err(SubmissionFollowUpStorageError::InvalidRecord);
        }
        let id = D1Type::Text(submission_id);
        let rows = self
            .database
            .prepare(
                "SELECT source.submission_id, source.follow_up_version, source.triage_status AS current_status, source.next_action AS current_next_action, source.due_on AS current_due_on, source.close_reason AS current_close_reason, event.version AS event_version, event.status AS event_status, event.next_action AS event_next_action, event.due_on AS event_due_on, event.close_reason AS event_close_reason, event.recorded_at AS event_recorded_at, strftime('%Y-%m-%d %H:%M UTC', event.recorded_at, 'unixepoch') AS event_recorded_at_label, event.provenance AS event_provenance, event.supersedes_version AS event_supersedes_version, event.corrects_version AS event_corrects_version FROM public_form_submissions AS source LEFT JOIN public_submission_follow_up_events AS event ON event.submission_id = source.submission_id WHERE source.submission_id = ? AND source.expires_at > unixepoch() AND NOT EXISTS (SELECT 1 FROM public_submission_erasure_intents AS intent WHERE intent.submission_id = source.submission_id) ORDER BY event.version ASC",
            )
            .bind_refs([&id])?
            .all()
            .await?
            .results::<FollowUpReadRow>()?;
        let mut rows = rows.into_iter();
        let Some(first) = rows.next() else {
            return Err(SubmissionFollowUpStorageError::NotFound);
        };
        let current = SubmissionFollowUpSnapshot {
            submission_id: first.submission_id.clone(),
            version: first
                .follow_up_version
                .ok_or(SubmissionFollowUpStorageError::InvalidRecord)?,
            status: first
                .current_status
                .ok_or(SubmissionFollowUpStorageError::InvalidRecord)?,
            next_action: first.current_next_action.clone(),
            due_on: first.current_due_on.clone(),
            close_reason: first.current_close_reason.clone(),
        };
        let mut events = Vec::new();
        append_event(&mut events, first)?;
        for row in rows {
            if row.submission_id != current.submission_id
                || row.follow_up_version != Some(current.version)
                || row.current_status != Some(current.status)
                || row.current_next_action != current.next_action
                || row.current_due_on != current.due_on
                || row.current_close_reason != current.close_reason
            {
                return Err(SubmissionFollowUpStorageError::InvalidRecord);
            }
            append_event(&mut events, row)?;
        }
        if events.len() != usize::try_from(current.version).unwrap_or(usize::MAX) {
            return Err(SubmissionFollowUpStorageError::InvalidRecord);
        }
        Ok(SubmissionFollowUpHistory { current, events })
    }

    pub async fn update_owner_follow_up(
        &self,
        submission_id: &str,
        expected_version: i32,
        idempotency_key: &str,
        follow_up: &SubmissionFollowUp,
    ) -> Result<SubmissionFollowUpMutationReceipt, SubmissionFollowUpStorageError> {
        if !canonical_submission_id(submission_id)
            || expected_version < 0
            || expected_version == i32::MAX
            || !valid_idempotency_key(idempotency_key)
        {
            return Err(SubmissionFollowUpStorageError::InvalidRecord);
        }
        let (status, next_action, due_on, close_reason, corrects_version) =
            follow_up.clone().into_parts();
        let digest = request_digest(
            submission_id,
            expected_version,
            status,
            next_action.as_deref(),
            due_on.as_deref(),
            close_reason.as_deref(),
            corrects_version,
        );
        let id = D1Type::Text(submission_id);
        let key = D1Type::Text(idempotency_key);
        let digest_value = D1Type::Text(&digest);
        let expected = D1Type::Integer(expected_version);
        let status = D1Type::Text(status.as_str());
        let next_action = next_action
            .as_deref()
            .map(D1Type::Text)
            .unwrap_or(D1Type::Null);
        let due_on = due_on.as_deref().map(D1Type::Text).unwrap_or(D1Type::Null);
        let close_reason = close_reason
            .as_deref()
            .map(D1Type::Text)
            .unwrap_or(D1Type::Null);
        let corrects_version = corrects_version
            .map(D1Type::Integer)
            .unwrap_or(D1Type::Null);

        let expire_receipt = self
            .database
            .prepare(
                "DELETE FROM public_submission_follow_up_receipts WHERE idempotency_key = ? AND expires_at <= unixepoch()",
            )
            .bind_refs([&key])?;
        let insert_receipt = self
            .database
            .prepare(
                "INSERT INTO public_submission_follow_up_receipts (submission_id, idempotency_key, request_digest, result_version, expires_at) SELECT source.submission_id, ?, ?, source.follow_up_version + CASE WHEN ? IS NOT NULL OR source.triage_status <> ? OR source.next_action IS NOT ? OR source.due_on IS NOT ? OR source.close_reason IS NOT ? THEN 1 ELSE 0 END, source.expires_at FROM public_form_submissions AS source WHERE source.submission_id = ? AND source.follow_up_version = ? AND source.expires_at > unixepoch() AND NOT EXISTS (SELECT 1 FROM public_submission_erasure_intents AS intent WHERE intent.submission_id = source.submission_id) AND (? IS NULL OR EXISTS (SELECT 1 FROM public_submission_follow_up_events AS target WHERE target.submission_id = source.submission_id AND target.version = ? AND target.corrects_version IS NULL AND NOT EXISTS (SELECT 1 FROM public_submission_follow_up_events AS correction WHERE correction.submission_id = target.submission_id AND correction.corrects_version = target.version))) ON CONFLICT(idempotency_key) DO NOTHING RETURNING submission_id, request_digest, result_version",
            )
            .bind_refs([
                &key,
                &digest_value,
                &corrects_version,
                &status,
                &next_action,
                &due_on,
                &close_reason,
                &id,
                &expected,
                &corrects_version,
                &corrects_version,
            ])?;
        let insert_event = self
            .database
            .prepare(
                "INSERT INTO public_submission_follow_up_events (submission_id, version, status, next_action, due_on, close_reason, recorded_at, provenance, supersedes_version, corrects_version) SELECT source.submission_id, source.follow_up_version + 1, ?, ?, ?, ?, unixepoch(), 'owner_follow_up', CASE WHEN source.follow_up_version = 0 THEN NULL ELSE source.follow_up_version END, ? FROM public_form_submissions AS source JOIN public_submission_follow_up_receipts AS receipt ON receipt.submission_id = source.submission_id AND receipt.idempotency_key = ? AND receipt.request_digest = ? AND receipt.result_version = source.follow_up_version + 1 WHERE source.submission_id = ? AND source.follow_up_version = ? AND source.expires_at > unixepoch() AND NOT EXISTS (SELECT 1 FROM public_submission_erasure_intents AS intent WHERE intent.submission_id = source.submission_id) AND (? IS NULL OR EXISTS (SELECT 1 FROM public_submission_follow_up_events AS target WHERE target.submission_id = source.submission_id AND target.version = ? AND target.corrects_version IS NULL AND NOT EXISTS (SELECT 1 FROM public_submission_follow_up_events AS correction WHERE correction.submission_id = target.submission_id AND correction.corrects_version = target.version))) AND (? IS NOT NULL OR source.triage_status <> ? OR source.next_action IS NOT ? OR source.due_on IS NOT ? OR source.close_reason IS NOT ?)",
            )
            .bind_refs([
                &status,
                &next_action,
                &due_on,
                &close_reason,
                &corrects_version,
                &key,
                &digest_value,
                &id,
                &expected,
                &corrects_version,
                &corrects_version,
                &corrects_version,
                &status,
                &next_action,
                &due_on,
                &close_reason,
            ])?;
        let update_snapshot = self
            .database
            .prepare(
                "UPDATE public_form_submissions SET triage_status = ?, next_action = ?, due_on = ?, close_reason = ?, follow_up_version = follow_up_version + 1 WHERE submission_id = ? AND follow_up_version = ? AND EXISTS (SELECT 1 FROM public_submission_follow_up_events AS event WHERE event.submission_id = public_form_submissions.submission_id AND event.version = public_form_submissions.follow_up_version + 1)",
            )
            .bind_refs([&status, &next_action, &due_on, &close_reason, &id, &expected])?;
        let results = self
            .database
            .batch(vec![
                expire_receipt,
                insert_receipt,
                insert_event,
                update_snapshot,
            ])
            .await?;
        let inserted_receipt = results
            .get(1)
            .ok_or(SubmissionFollowUpStorageError::InvalidRecord)?
            .results::<FollowUpReceiptRow>()?
            .len()
            == 1;

        let receipt = self
            .database
            .prepare(
                "SELECT receipt.submission_id, receipt.request_digest, receipt.result_version FROM public_submission_follow_up_receipts AS receipt WHERE receipt.idempotency_key = ? AND receipt.expires_at > unixepoch()",
            )
            .bind_refs([&key])?
            .first::<FollowUpReceiptRow>(None)
            .await?;
        if let Some(receipt) = receipt {
            if receipt.submission_id != submission_id || receipt.request_digest != digest {
                return Err(SubmissionFollowUpStorageError::IdempotencyPayloadMismatch);
            }
            let visible = self
                .database
                .prepare(
                    "SELECT EXISTS(SELECT 1 FROM public_form_submissions AS source WHERE source.submission_id = ? AND source.expires_at > unixepoch() AND NOT EXISTS (SELECT 1 FROM public_submission_erasure_intents AS intent WHERE intent.submission_id = source.submission_id)) AS present",
                )
                .bind_refs([&id])?
                .first::<SubmissionVisibilityRow>(None)
                .await?
                .ok_or(SubmissionFollowUpStorageError::InvalidRecord)?;
            if visible.present == 0 {
                return Err(SubmissionFollowUpStorageError::NotFound);
            }
            return Ok(SubmissionFollowUpMutationReceipt {
                submission_id: submission_id.to_owned(),
                version: receipt.result_version,
                replayed: !inserted_receipt,
            });
        }

        let visible = self
            .database
            .prepare(
                "SELECT EXISTS(SELECT 1 FROM public_form_submissions AS source WHERE source.submission_id = ? AND source.expires_at > unixepoch() AND NOT EXISTS (SELECT 1 FROM public_submission_erasure_intents AS intent WHERE intent.submission_id = source.submission_id)) AS present",
            )
            .bind_refs([&id])?
            .first::<SubmissionVisibilityRow>(None)
            .await?
            .ok_or(SubmissionFollowUpStorageError::InvalidRecord)?;
        if visible.present == 0 {
            return Err(SubmissionFollowUpStorageError::NotFound);
        }
        if let D1Type::Integer(target_version) = corrects_version {
            let unavailable = self
                .database
                .prepare(
                    "SELECT EXISTS(SELECT 1 FROM public_form_submissions AS source WHERE source.submission_id = ? AND source.follow_up_version = ? AND NOT EXISTS (SELECT 1 FROM public_submission_follow_up_events AS target WHERE target.submission_id = source.submission_id AND target.version = ? AND target.corrects_version IS NULL AND NOT EXISTS (SELECT 1 FROM public_submission_follow_up_events AS correction WHERE correction.submission_id = target.submission_id AND correction.corrects_version = target.version)) AND source.expires_at > unixepoch() AND NOT EXISTS (SELECT 1 FROM public_submission_erasure_intents AS intent WHERE intent.submission_id = source.submission_id)) AS present",
                )
                .bind_refs([&id, &expected, &D1Type::Integer(target_version)])?
                .first::<SubmissionVisibilityRow>(None)
                .await?
                .ok_or(SubmissionFollowUpStorageError::InvalidRecord)?;
            if unavailable.present != 0 {
                return Err(SubmissionFollowUpStorageError::CorrectionTargetUnavailable);
            }
        }
        Err(SubmissionFollowUpStorageError::StaleVersion)
    }
}

fn append_event(
    events: &mut Vec<SubmissionFollowUpEvent>,
    row: FollowUpReadRow,
) -> Result<(), SubmissionFollowUpStorageError> {
    let Some(version) = row.event_version else {
        if row.event_status.is_some()
            || row.event_next_action.is_some()
            || row.event_due_on.is_some()
            || row.event_close_reason.is_some()
            || row.event_recorded_at.is_some()
            || row.event_recorded_at_label.is_some()
            || row.event_provenance.is_some()
            || row.event_supersedes_version.is_some()
            || row.event_corrects_version.is_some()
        {
            return Err(SubmissionFollowUpStorageError::InvalidRecord);
        }
        return Ok(());
    };
    let expected_supersedes = (version > 1).then_some(version - 1);
    if version != i32::try_from(events.len() + 1).unwrap_or(i32::MAX)
        || row.event_supersedes_version != expected_supersedes
    {
        return Err(SubmissionFollowUpStorageError::InvalidRecord);
    }
    if let Some(target_version) = row.event_corrects_version {
        let Some(target) = usize::try_from(target_version - 1)
            .ok()
            .and_then(|index| events.get(index))
        else {
            return Err(SubmissionFollowUpStorageError::InvalidRecord);
        };
        if target.corrects_version.is_some()
            || events
                .iter()
                .any(|event| event.corrects_version == Some(target_version))
        {
            return Err(SubmissionFollowUpStorageError::InvalidRecord);
        }
    }
    events.push(SubmissionFollowUpEvent {
        version,
        status: row
            .event_status
            .ok_or(SubmissionFollowUpStorageError::InvalidRecord)?,
        next_action: row.event_next_action,
        due_on: row.event_due_on,
        close_reason: row.event_close_reason,
        recorded_at: row
            .event_recorded_at
            .ok_or(SubmissionFollowUpStorageError::InvalidRecord)?,
        recorded_at_label: row
            .event_recorded_at_label
            .ok_or(SubmissionFollowUpStorageError::InvalidRecord)?,
        provenance: row
            .event_provenance
            .ok_or(SubmissionFollowUpStorageError::InvalidRecord)?,
        supersedes_version: row.event_supersedes_version,
        corrects_version: row.event_corrects_version,
    });
    Ok(())
}

pub(crate) fn request_digest(
    submission_id: &str,
    expected_version: i32,
    status: FormSubmissionStatus,
    next_action: Option<&str>,
    due_on: Option<&str>,
    close_reason: Option<&str>,
    corrects_version: Option<i32>,
) -> String {
    let canonical = serde_json::json!([
        "eezfunnel.submission_follow_up.v1",
        submission_id,
        expected_version,
        status.as_str(),
        next_action,
        due_on,
        close_reason,
        corrects_version,
    ])
    .to_string();
    Sha256::digest(canonical.as_bytes()).iter().fold(
        String::with_capacity(64),
        |mut output, byte| {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
            output
        },
    )
}

fn canonical_submission_id(value: &str) -> bool {
    uuid::Uuid::parse_str(value)
        .map(|id| id.hyphenated().to_string() == value)
        .unwrap_or(false)
}

fn valid_idempotency_key(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}
