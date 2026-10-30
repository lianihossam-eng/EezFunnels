use eez_domain::webhook_delivery::{retry_delay_seconds, WebhookTerminalReason};
use serde::Deserialize;
use worker::d1::D1Type;
use worker::D1Database;

use crate::repository::RepositoryResult;

const FINISH_ATTEMPT_QUERY: &str = include_str!("../sql/public_submission_delivery_finish.sql");
const HAS_DUE_TASKS_QUERY: &str = include_str!("../sql/public_submission_delivery_due.sql");
const TERMINALIZE_EXHAUSTED_AND_EXPIRED_QUERY: &str =
    include_str!("../sql/public_submission_delivery_terminalize.sql");

pub struct SubmissionDeliveryRepository<'a> {
    database: &'a D1Database,
}

pub struct ClaimedSubmissionDelivery {
    pub task_id: String,
    pub lease_token: String,
    pub funnel_id: String,
    pub revision_id: String,
    pub page_id: String,
    pub form_id: String,
    pub destination_url: String,
    pub selected_fields_json: String,
    pub field_values_json: String,
    pub submitted_at: i64,
    pub document_json: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StartedDeliveryAttempt {
    pub attempt_number: u8,
    pub timestamp: u64,
    previous_attempt_at: Option<i64>,
    previous_next_attempt_at: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryAttemptResult {
    ReceiverAccepted {
        http_status: u16,
    },
    Retryable {
        http_status: Option<u16>,
    },
    Terminal {
        reason: WebhookTerminalReason,
        http_status: Option<u16>,
    },
}

#[derive(Deserialize)]
struct DeliveryTaskId {
    task_id: String,
}

#[derive(Deserialize)]
struct DeliverySnapshot {
    task_id: String,
    lease_token: String,
    funnel_id: String,
    revision_id: String,
    page_id: String,
    form_id: String,
    destination_url: String,
    selected_fields_json: String,
    field_values_json: String,
    submitted_at: i64,
    document_json: String,
}

#[derive(Deserialize)]
struct StartedAttemptRecord {
    attempt_count: i64,
    last_attempt_at: i64,
}

#[derive(Deserialize)]
struct DeliveryAttemptState {
    attempt_count: i64,
    last_attempt_at: Option<i64>,
    next_attempt_at: i64,
}

impl<'a> SubmissionDeliveryRepository<'a> {
    pub fn new(database: &'a D1Database) -> Self {
        Self { database }
    }

    pub async fn has_due_tasks(&self) -> RepositoryResult<bool> {
        let task = self
            .database
            .prepare(HAS_DUE_TASKS_QUERY)
            .first::<DeliveryTaskId>(None)
            .await?;
        Ok(task.is_some())
    }

    pub async fn has_dispatchable_tasks(&self) -> RepositoryResult<bool> {
        let task = self
            .database
            .prepare(
                "SELECT state.task_id FROM public_submission_delivery_state AS state JOIN retained_visible_public_submission_delivery AS visible ON visible.task_id = state.task_id WHERE ((state.status = 'pending' AND state.next_attempt_at <= unixepoch()) OR (state.status = 'leased' AND state.lease_expires_at <= unixepoch() AND state.next_attempt_at <= unixepoch())) AND state.attempt_count < 5 AND visible.created_at + 86400 > unixepoch() ORDER BY state.next_attempt_at, visible.created_at, visible.task_id LIMIT 1",
            )
            .first::<DeliveryTaskId>(None)
            .await?;
        Ok(task.is_some())
    }

    pub async fn claim_next(
        &self,
        lease_token: &str,
    ) -> RepositoryResult<Option<ClaimedSubmissionDelivery>> {
        self.terminalize_exhausted_and_expired().await?;

        let lease_token_value = lease_token.to_owned();
        let lease_token = D1Type::Text(&lease_token_value);
        let task = self
            .database
            .prepare(
                "UPDATE public_submission_delivery_state SET status = 'leased', lease_token = ?, lease_expires_at = unixepoch() + 30, updated_at = unixepoch() WHERE task_id = (SELECT visible.task_id FROM retained_visible_public_submission_delivery AS visible JOIN public_submission_delivery_state AS state ON state.task_id = visible.task_id WHERE ((state.status = 'pending' AND state.next_attempt_at <= unixepoch()) OR (state.status = 'leased' AND state.lease_expires_at <= unixepoch() AND state.next_attempt_at <= unixepoch())) AND state.attempt_count < 5 AND visible.created_at + 86400 > unixepoch() ORDER BY state.next_attempt_at, visible.created_at, visible.task_id LIMIT 1) AND ((public_submission_delivery_state.status = 'pending' AND public_submission_delivery_state.next_attempt_at <= unixepoch()) OR (public_submission_delivery_state.status = 'leased' AND public_submission_delivery_state.lease_expires_at <= unixepoch() AND public_submission_delivery_state.next_attempt_at <= unixepoch())) AND public_submission_delivery_state.attempt_count < 5 RETURNING task_id",
            )
            .bind_refs([&lease_token])?
            .first::<DeliveryTaskId>(None)
            .await?;
        let Some(task) = task else {
            return Ok(None);
        };

        let task_id = D1Type::Text(&task.task_id);
        let lease_token = D1Type::Text(&lease_token_value);
        let snapshot = self
            .database
            .prepare(
                "SELECT visible.task_id, state.lease_token, visible.funnel_id, visible.revision_id, visible.page_id, visible.form_id, destination.destination_url, mapping.selected_fields_json, source.field_values_json, source.submitted_at, revision.data_json AS document_json FROM retained_visible_public_submission_delivery AS visible JOIN public_submission_delivery_state AS state ON state.task_id = visible.task_id AND state.status = 'leased' JOIN public_form_submissions AS source ON source.submission_id = visible.source_submission_id JOIN revisions AS revision ON revision.id = visible.revision_id AND revision.funnel_id = visible.funnel_id JOIN webhook_destination_versions AS destination ON destination.destination_version_id = visible.destination_version_id JOIN public_form_delivery_mapping_versions AS mapping ON mapping.mapping_version_id = visible.mapping_version_id AND mapping.funnel_id = visible.funnel_id AND mapping.form_id = visible.form_id AND mapping.destination_version_id = visible.destination_version_id WHERE visible.task_id = ? AND state.lease_token = ? AND state.lease_expires_at > unixepoch()",
            )
            .bind_refs([&task_id, &lease_token])?
            .first::<DeliverySnapshot>(None)
            .await?;

        let Some(snapshot) = snapshot else {
            let outcome = D1Type::Text("invalid_submission");
            let task_id = D1Type::Text(&task.task_id);
            let lease_token = D1Type::Text(&lease_token_value);
            self.database
                .prepare(
                    "UPDATE public_submission_delivery_state SET status = 'failed', last_outcome_code = ?, completed_at = unixepoch(), lease_token = NULL, lease_expires_at = NULL, updated_at = unixepoch() WHERE task_id = ? AND status = 'leased' AND lease_token = ? AND lease_expires_at > unixepoch() AND EXISTS (SELECT 1 FROM retained_visible_public_submission_delivery AS visible WHERE visible.task_id = public_submission_delivery_state.task_id AND visible.created_at + 86400 > unixepoch())",
                )
                .bind_refs([&outcome, &task_id, &lease_token])?
                .run()
                .await?;
            return Ok(None);
        };

        Ok(Some(ClaimedSubmissionDelivery {
            task_id: snapshot.task_id,
            lease_token: snapshot.lease_token,
            funnel_id: snapshot.funnel_id,
            revision_id: snapshot.revision_id,
            page_id: snapshot.page_id,
            form_id: snapshot.form_id,
            destination_url: snapshot.destination_url,
            selected_fields_json: snapshot.selected_fields_json,
            field_values_json: snapshot.field_values_json,
            submitted_at: snapshot.submitted_at,
            document_json: snapshot.document_json,
        }))
    }

    pub async fn start_attempt(
        &self,
        delivery: &ClaimedSubmissionDelivery,
    ) -> RepositoryResult<Option<StartedDeliveryAttempt>> {
        let task_id = D1Type::Text(&delivery.task_id);
        let lease_token = D1Type::Text(&delivery.lease_token);
        let previous = self
            .database
            .prepare(
                "SELECT state.attempt_count, state.last_attempt_at, state.next_attempt_at FROM public_submission_delivery_state AS state JOIN retained_visible_public_submission_delivery AS visible ON visible.task_id = state.task_id WHERE state.task_id = ? AND state.status = 'leased' AND state.lease_token = ? AND state.lease_expires_at > unixepoch() AND state.attempt_count < 5 AND visible.created_at + 86400 > unixepoch()",
            )
            .bind_refs([&task_id, &lease_token])?
            .first::<DeliveryAttemptState>(None)
            .await?;
        let Some(previous) = previous else {
            return Ok(None);
        };
        let previous_attempt_count =
            D1Type::Integer(i32::try_from(previous.attempt_count).map_err(|_| {
                worker::Error::RustError("invalid delivery attempt count".to_owned())
            })?);
        let previous_last_attempt_at = previous
            .last_attempt_at
            .map(|timestamp| D1Type::Real(timestamp as f64))
            .unwrap_or(D1Type::Null);
        let previous_next_attempt_at = D1Type::Real(previous.next_attempt_at as f64);
        let attempt = self
            .database
            .prepare(
                "UPDATE public_submission_delivery_state SET attempt_count = attempt_count + 1, last_attempt_at = unixepoch(), next_attempt_at = unixepoch() + CASE attempt_count WHEN 0 THEN 60 WHEN 1 THEN 300 WHEN 2 THEN 1800 WHEN 3 THEN 7200 ELSE 86400 END, updated_at = unixepoch() WHERE task_id = ? AND status = 'leased' AND lease_token = ? AND lease_expires_at > unixepoch() AND attempt_count = ? AND next_attempt_at = ? AND last_attempt_at IS ? AND attempt_count < 5 AND EXISTS (SELECT 1 FROM retained_visible_public_submission_delivery AS visible WHERE visible.task_id = public_submission_delivery_state.task_id AND visible.created_at + 86400 > unixepoch()) RETURNING attempt_count, last_attempt_at",
            )
            .bind_refs([
                &task_id,
                &lease_token,
                &previous_attempt_count,
                &previous_next_attempt_at,
                &previous_last_attempt_at,
            ])?
            .first::<StartedAttemptRecord>(None)
            .await?;
        Ok(attempt.and_then(|attempt| {
            Some(StartedDeliveryAttempt {
                attempt_number: u8::try_from(attempt.attempt_count).ok()?,
                timestamp: u64::try_from(attempt.last_attempt_at).ok()?,
                previous_attempt_at: previous.last_attempt_at,
                previous_next_attempt_at: previous.next_attempt_at,
            })
        }))
    }

    pub async fn attempt_is_live(
        &self,
        delivery: &ClaimedSubmissionDelivery,
        attempt: StartedDeliveryAttempt,
    ) -> RepositoryResult<bool> {
        let task_id = D1Type::Text(&delivery.task_id);
        let lease_token = D1Type::Text(&delivery.lease_token);
        let attempt_count = D1Type::Integer(i32::from(attempt.attempt_number));
        let timestamp = D1Type::Real(attempt.timestamp as f64);
        let live = self
            .database
            .prepare(
                "SELECT state.task_id FROM public_submission_delivery_state AS state JOIN retained_visible_public_submission_delivery AS visible ON visible.task_id = state.task_id WHERE state.task_id = ? AND state.status = 'leased' AND state.lease_token = ? AND state.attempt_count = ? AND state.last_attempt_at = ? AND state.lease_expires_at > unixepoch() AND visible.created_at + 86400 > unixepoch()",
            )
            .bind_refs([&task_id, &lease_token, &attempt_count, &timestamp])?
            .first::<DeliveryTaskId>(None)
            .await?;
        Ok(live.is_some())
    }

    pub async fn abandon_unstarted_attempt(
        &self,
        delivery: &ClaimedSubmissionDelivery,
        attempt: StartedDeliveryAttempt,
    ) -> RepositoryResult<bool> {
        let task_id = D1Type::Text(&delivery.task_id);
        let lease_token = D1Type::Text(&delivery.lease_token);
        let attempt_count = D1Type::Integer(i32::from(attempt.attempt_number));
        let timestamp = D1Type::Real(attempt.timestamp as f64);
        let previous_attempt_at = attempt
            .previous_attempt_at
            .map(|timestamp| D1Type::Real(timestamp as f64))
            .unwrap_or(D1Type::Null);
        let previous_next_attempt_at = D1Type::Real(attempt.previous_next_attempt_at as f64);
        let updated = self
            .database
            .prepare(
                "UPDATE public_submission_delivery_state SET status = CASE WHEN EXISTS (SELECT 1 FROM retained_visible_public_submission_delivery AS visible WHERE visible.task_id = public_submission_delivery_state.task_id AND visible.created_at + 86400 > unixepoch()) THEN 'pending' ELSE 'failed' END, attempt_count = attempt_count - 1, last_attempt_at = ?, next_attempt_at = ?, last_outcome_code = CASE WHEN EXISTS (SELECT 1 FROM retained_visible_public_submission_delivery AS visible WHERE visible.task_id = public_submission_delivery_state.task_id AND visible.created_at + 86400 > unixepoch()) THEN last_outcome_code ELSE 'delivery_window_elapsed' END, lease_token = NULL, lease_expires_at = NULL, completed_at = CASE WHEN EXISTS (SELECT 1 FROM retained_visible_public_submission_delivery AS visible WHERE visible.task_id = public_submission_delivery_state.task_id AND visible.created_at + 86400 > unixepoch()) THEN NULL ELSE unixepoch() END, updated_at = unixepoch() WHERE task_id = ? AND status = 'leased' AND lease_token = ? AND attempt_count = ? AND last_attempt_at = ? RETURNING task_id",
            )
            .bind_refs([
                &previous_attempt_at,
                &previous_next_attempt_at,
                &task_id,
                &lease_token,
                &attempt_count,
                &timestamp,
            ])?
            .first::<DeliveryTaskId>(None)
            .await?;
        Ok(updated.is_some())
    }

    pub async fn finish_attempt(
        &self,
        delivery: &ClaimedSubmissionDelivery,
        attempt: StartedDeliveryAttempt,
        result: DeliveryAttemptResult,
    ) -> RepositoryResult<bool> {
        let (outcome_code, http_status, retry_delay, transport_kind) = match result {
            DeliveryAttemptResult::ReceiverAccepted { http_status } => {
                ("receiver_accepted", Some(http_status), 0, "accepted")
            }
            DeliveryAttemptResult::Retryable { http_status } => {
                ("transport_retryable", http_status, 0, "retryable")
            }
            DeliveryAttemptResult::Terminal {
                reason,
                http_status,
            } => (reason.as_str(), http_status, 0, "terminal"),
        };
        let retry_delay = if matches!(result, DeliveryAttemptResult::Retryable { .. }) {
            retry_delay_seconds(attempt.attempt_number).unwrap_or(0)
        } else {
            retry_delay
        };
        let task_id = D1Type::Text(&delivery.task_id);
        let lease_token = D1Type::Text(&delivery.lease_token);
        let outcome_code = D1Type::Text(outcome_code);
        let http_status = http_status
            .map(|status| D1Type::Integer(i32::from(status)))
            .unwrap_or(D1Type::Null);
        let transport_kind = D1Type::Text(transport_kind);
        let retry_delay = D1Type::Integer(i32::try_from(retry_delay).unwrap_or(0));
        let attempt_count = D1Type::Integer(i32::from(attempt.attempt_number));
        let last_attempt_at = D1Type::Real(attempt.timestamp as f64);
        let updated = self
            .database
            .prepare(FINISH_ATTEMPT_QUERY)
            .bind_refs([
                &transport_kind,
                &transport_kind,
                &retry_delay,
                &retry_delay,
                &transport_kind,
                &retry_delay,
                &retry_delay,
                &retry_delay,
                &transport_kind,
                &transport_kind,
                &retry_delay,
                &retry_delay,
                &outcome_code,
                &http_status,
                &transport_kind,
                &transport_kind,
                &retry_delay,
                &retry_delay,
                &task_id,
                &lease_token,
                &attempt_count,
                &last_attempt_at,
            ])?
            .first::<DeliveryTaskId>(None)
            .await?;
        Ok(updated.is_some())
    }

    pub async fn fail_before_attempt(
        &self,
        delivery: &ClaimedSubmissionDelivery,
        reason: WebhookTerminalReason,
    ) -> RepositoryResult<bool> {
        self.finish_without_attempt(delivery, reason.as_str()).await
    }

    pub async fn release_without_attempt(
        &self,
        delivery: &ClaimedSubmissionDelivery,
    ) -> RepositoryResult<bool> {
        let task_id = D1Type::Text(&delivery.task_id);
        let lease_token = D1Type::Text(&delivery.lease_token);
        let released = self
            .database
            .prepare(
                "UPDATE public_submission_delivery_state SET status = 'pending', next_attempt_at = MIN(next_attempt_at, unixepoch()), lease_token = NULL, lease_expires_at = NULL, updated_at = unixepoch() WHERE task_id = ? AND status = 'leased' AND lease_token = ? RETURNING task_id",
            )
            .bind_refs([&task_id, &lease_token])?
            .first::<DeliveryTaskId>(None)
            .await?;
        Ok(released.is_some())
    }

    async fn finish_without_attempt(
        &self,
        delivery: &ClaimedSubmissionDelivery,
        outcome_code: &str,
    ) -> RepositoryResult<bool> {
        let task_id = D1Type::Text(&delivery.task_id);
        let lease_token = D1Type::Text(&delivery.lease_token);
        let outcome_code = D1Type::Text(outcome_code);
        let updated = self
            .database
            .prepare(
                "UPDATE public_submission_delivery_state SET status = 'failed', last_outcome_code = ?, completed_at = unixepoch(), lease_token = NULL, lease_expires_at = NULL, updated_at = unixepoch() WHERE task_id = ? AND status = 'leased' AND lease_token = ? AND lease_expires_at > unixepoch() AND EXISTS (SELECT 1 FROM retained_visible_public_submission_delivery AS visible WHERE visible.task_id = public_submission_delivery_state.task_id AND visible.created_at + 86400 > unixepoch()) RETURNING task_id",
            )
            .bind_refs([&outcome_code, &task_id, &lease_token])?
            .first::<DeliveryTaskId>(None)
            .await?;
        Ok(updated.is_some())
    }

    pub async fn terminalize_exhausted_and_expired(&self) -> RepositoryResult<()> {
        self.database
            .prepare(TERMINALIZE_EXHAUSTED_AND_EXPIRED_QUERY)
            .run()
            .await?;
        Ok(())
    }
}
