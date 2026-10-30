use std::future::Future;
use std::pin::Pin;

use eez_domain::webhook_delivery::{
    classify_webhook_status, prepare_webhook_payload, sign_webhook_delivery,
    ValidatedWebhookDestination, WebhookHttpOutcome, WebhookPreparationError, WebhookSigningKey,
    WebhookSubmissionSnapshot, WebhookTerminalReason,
};
use eez_storage::repository::RepositoryError;
use eez_storage::submission_delivery::{
    ClaimedSubmissionDelivery, DeliveryAttemptResult, SubmissionDeliveryRepository,
};
use eez_storage::submission_lifecycle::SubmissionLifecycleRepository;

const MAX_DELIVERIES_PER_RUN: usize = 4;

pub trait WebhookTransport {
    fn post<'a>(
        &'a self,
        request: eez_domain::webhook_delivery::SignedWebhookDelivery,
    ) -> Pin<Box<dyn Future<Output = WebhookTransportResult> + 'a>>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebhookTransportResult {
    HttpStatus(u16),
    NetworkFailure,
    InvalidRequest,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DispatchSummary {
    pub attempted: u8,
    pub accepted: u8,
    pub retryable: u8,
    pub terminal: u8,
    pub paused: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchError {
    StorageUnavailable,
}

impl From<RepositoryError> for DispatchError {
    fn from(_: RepositoryError) -> Self {
        Self::StorageUnavailable
    }
}

pub async fn dispatch_due<T: WebhookTransport>(
    lifecycle: &SubmissionLifecycleRepository<'_>,
    deliveries: &SubmissionDeliveryRepository<'_>,
    signing_key: &WebhookSigningKey,
    transport: &T,
) -> Result<DispatchSummary, DispatchError> {
    let mut summary = DispatchSummary::default();
    for _ in 0..MAX_DELIVERIES_PER_RUN {
        if lifecycle.require_recovery_ready().await.is_err() {
            summary.paused = true;
            break;
        }

        let lease_token = uuid::Uuid::new_v4().to_string();
        let Some(delivery) = deliveries.claim_next(&lease_token).await? else {
            break;
        };

        let destination = match ValidatedWebhookDestination::parse(&delivery.destination_url) {
            Ok(destination) => destination,
            Err(_) => {
                if deliveries
                    .fail_before_attempt(&delivery, WebhookTerminalReason::InvalidDestination)
                    .await?
                {
                    summary.terminal = summary.terminal.saturating_add(1);
                }
                continue;
            }
        };

        let prepared = match prepare_delivery_payload(destination, &delivery) {
            Ok(prepared) => prepared,
            Err(reason) => {
                if deliveries.fail_before_attempt(&delivery, reason).await? {
                    summary.terminal = summary.terminal.saturating_add(1);
                }
                continue;
            }
        };

        if lifecycle.require_recovery_ready().await.is_err() {
            deliveries.release_without_attempt(&delivery).await?;
            summary.paused = true;
            break;
        }

        let Some(attempt) = deliveries.start_attempt(&delivery).await? else {
            continue;
        };
        let request = match sign_webhook_delivery(prepared, attempt.timestamp, signing_key) {
            Ok(request) => request,
            Err(_) => {
                deliveries
                    .finish_attempt(
                        &delivery,
                        attempt,
                        DeliveryAttemptResult::Terminal {
                            reason: WebhookTerminalReason::SigningFailed,
                            http_status: None,
                        },
                    )
                    .await?;
                summary.attempted = summary.attempted.saturating_add(1);
                summary.terminal = summary.terminal.saturating_add(1);
                continue;
            }
        };

        if lifecycle.require_recovery_ready().await.is_err() {
            deliveries
                .abandon_unstarted_attempt(&delivery, attempt)
                .await?;
            summary.paused = true;
            break;
        }
        if !deliveries.attempt_is_live(&delivery, attempt).await? {
            deliveries
                .abandon_unstarted_attempt(&delivery, attempt)
                .await?;
            continue;
        }

        let result = match transport.post(request).await {
            WebhookTransportResult::NetworkFailure => {
                summary.retryable = summary.retryable.saturating_add(1);
                DeliveryAttemptResult::Retryable { http_status: None }
            }
            WebhookTransportResult::InvalidRequest => {
                summary.terminal = summary.terminal.saturating_add(1);
                DeliveryAttemptResult::Terminal {
                    reason: WebhookTerminalReason::InvalidResponse,
                    http_status: None,
                }
            }
            WebhookTransportResult::HttpStatus(status) => match classify_webhook_status(status) {
                WebhookHttpOutcome::Accepted { status } => {
                    summary.accepted = summary.accepted.saturating_add(1);
                    DeliveryAttemptResult::ReceiverAccepted {
                        http_status: status,
                    }
                }
                WebhookHttpOutcome::Retryable { status } => {
                    summary.retryable = summary.retryable.saturating_add(1);
                    DeliveryAttemptResult::Retryable {
                        http_status: Some(status),
                    }
                }
                WebhookHttpOutcome::Terminal { status, reason } => {
                    summary.terminal = summary.terminal.saturating_add(1);
                    DeliveryAttemptResult::Terminal {
                        reason,
                        http_status: persistable_http_status(status),
                    }
                }
            },
        };
        summary.attempted = summary.attempted.saturating_add(1);
        deliveries
            .finish_attempt(&delivery, attempt, result)
            .await?;
    }
    Ok(summary)
}

fn prepare_delivery_payload(
    destination: ValidatedWebhookDestination,
    delivery: &ClaimedSubmissionDelivery,
) -> Result<eez_domain::webhook_delivery::PreparedWebhookDelivery, WebhookTerminalReason> {
    prepare_webhook_payload(
        destination,
        WebhookSubmissionSnapshot {
            delivery_id: &delivery.task_id,
            funnel_id: &delivery.funnel_id,
            revision_id: &delivery.revision_id,
            page_id: &delivery.page_id,
            form_id: &delivery.form_id,
            submitted_at: delivery.submitted_at,
            document_json: &delivery.document_json,
            selected_fields_json: &delivery.selected_fields_json,
            field_values_json: &delivery.field_values_json,
        },
    )
    .map_err(preparation_reason)
}

fn preparation_reason(error: WebhookPreparationError) -> WebhookTerminalReason {
    match error {
        WebhookPreparationError::InvalidDestination => WebhookTerminalReason::InvalidDestination,
        WebhookPreparationError::IncompatibleMapping => WebhookTerminalReason::IncompatibleMapping,
        WebhookPreparationError::InvalidSigningKey
        | WebhookPreparationError::InvalidTimestamp
        | WebhookPreparationError::SigningFailed => WebhookTerminalReason::SigningFailed,
        WebhookPreparationError::InvalidDeliveryId
        | WebhookPreparationError::InvalidSubmission
        | WebhookPreparationError::EncodingFailure => WebhookTerminalReason::InvalidSubmission,
    }
}

fn persistable_http_status(status: u16) -> Option<u16> {
    (100..=599).contains(&status).then_some(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eez_domain::webhook_delivery::{verify_webhook_signature, SignedWebhookDelivery};
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    struct LocalReferenceReceiver {
        key: WebhookSigningKey,
        accepted: RefCell<BTreeMap<String, Vec<u8>>>,
        accepted_effects: RefCell<u8>,
    }

    impl LocalReferenceReceiver {
        fn new() -> Self {
            Self {
                key: WebhookSigningKey::parse("PUB-009 fictional webhook signing secret 32 bytes")
                    .expect("fake receiver key is long enough"),
                accepted: RefCell::new(BTreeMap::new()),
                accepted_effects: RefCell::new(0),
            }
        }

        fn accept(&self, request: &SignedWebhookDelivery, now: u64) -> u16 {
            if request.timestamp().abs_diff(now) > 300
                || !verify_webhook_signature(
                    &self.key,
                    request.timestamp(),
                    request.delivery_id(),
                    request.body(),
                    &format!("v1={}", request.signature()),
                )
            {
                return 401;
            }
            let Ok(payload) = serde_json::from_slice::<serde_json::Value>(request.body()) else {
                return 400;
            };
            if payload
                .get("delivery_id")
                .and_then(serde_json::Value::as_str)
                != Some(request.delivery_id())
            {
                return 400;
            }
            match self
                .accepted
                .borrow_mut()
                .entry(request.delivery_id().to_owned())
            {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(request.body().to_vec());
                    *self.accepted_effects.borrow_mut() += 1;
                    202
                }
                std::collections::btree_map::Entry::Occupied(entry) => {
                    if entry.get().as_slice() == request.body() {
                        202
                    } else {
                        409
                    }
                }
            }
        }
    }

    #[test]
    fn local_receiver_verifies_signature_and_deduplicates_uncertain_retries() {
        let receiver = LocalReferenceReceiver::new();
        let key = WebhookSigningKey::parse("PUB-009 fictional webhook signing secret 32 bytes")
            .expect("test key is long enough");
        let document = eez_domain::document_v2::DocumentV2::starter(
            "funnel-public",
            eez_domain::document_v2::BlueprintId::LeadGen,
            eez_domain::ThemePreset::Minimal,
        )
        .expect("lead generation starter");
        let prepared = prepare_webhook_payload(
            ValidatedWebhookDestination::parse("https://hooks.public-site.dev/receive")
                .expect("valid test destination"),
            WebhookSubmissionSnapshot {
                delivery_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                funnel_id: "funnel-public",
                revision_id: "revision-accepted",
                page_id: "page-home",
                form_id: "form-service-inquiry",
                submitted_at: 1_800_000_000,
                document_json: std::str::from_utf8(document.canonical_bytes())
                    .expect("document json"),
                selected_fields_json: r#"["field-email"]"#,
                field_values_json: r#"{"email":"person@example.test"}"#,
            },
        )
        .expect("mapping belongs to exact source revision");
        let first =
            sign_webhook_delivery(prepared, 1_800_000_030, &key).expect("first request is signed");
        let _response_lost_after_receiver_acceptance = receiver.accept(&first, 1_800_000_030);
        assert_eq!(*receiver.accepted_effects.borrow(), 1);

        let repeated = prepare_webhook_payload(
            ValidatedWebhookDestination::parse("https://hooks.public-site.dev/receive")
                .expect("valid test destination"),
            WebhookSubmissionSnapshot {
                delivery_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                funnel_id: "funnel-public",
                revision_id: "revision-accepted",
                page_id: "page-home",
                form_id: "form-service-inquiry",
                submitted_at: 1_800_000_000,
                document_json: std::str::from_utf8(document.canonical_bytes())
                    .expect("document json"),
                selected_fields_json: r#"["field-email"]"#,
                field_values_json: r#"{"email":"person@example.test"}"#,
            },
        )
        .expect("retry is reconstructed from the immutable source");
        let retry = sign_webhook_delivery(repeated, 1_800_000_090, &key)
            .expect("retry is signed with current attempt time");
        assert_eq!(first.body(), retry.body());
        assert_eq!(first.delivery_id(), retry.delivery_id());
        assert_eq!(receiver.accept(&retry, 1_800_000_090), 202);
        assert_eq!(*receiver.accepted_effects.borrow(), 1);

        let conflicting = sign_webhook_delivery(
            prepare_webhook_payload(
                ValidatedWebhookDestination::parse("https://hooks.public-site.dev/receive")
                    .expect("valid test destination"),
                WebhookSubmissionSnapshot {
                    delivery_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                    funnel_id: "funnel-public",
                    revision_id: "revision-accepted",
                    page_id: "page-home",
                    form_id: "form-service-inquiry",
                    submitted_at: 1_800_000_000,
                    document_json: std::str::from_utf8(document.canonical_bytes())
                        .expect("document json"),
                    selected_fields_json: r#"["field-email"]"#,
                    field_values_json: r#"{"email":"other@example.test"}"#,
                },
            )
            .expect("source can be serialized with its original id"),
            1_800_000_120,
            &key,
        )
        .expect("conflicting request is still properly signed");
        assert_eq!(receiver.accept(&conflicting, 1_800_000_120), 409);
        assert_eq!(receiver.accept(&retry, 1_800_000_120), 202);
        assert_eq!(*receiver.accepted_effects.borrow(), 1);
    }

    #[test]
    fn invalid_transport_statuses_are_not_persisted_as_http_responses() {
        assert_eq!(persistable_http_status(0), None);
        assert_eq!(persistable_http_status(99), None);
        assert_eq!(persistable_http_status(600), None);
        assert_eq!(persistable_http_status(599), Some(599));
    }
}
