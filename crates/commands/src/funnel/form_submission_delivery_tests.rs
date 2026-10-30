use super::form_submission_delivery;
use eez_domain::FormSubmissionStatus;
use eez_storage::repository::PublicFormSubmissionRecord;

fn record(
    delivery_state: Option<&str>,
    attempts: Option<i64>,
    retry_scheduled: i64,
    outcome: Option<&str>,
) -> PublicFormSubmissionRecord {
    PublicFormSubmissionRecord {
        submission_id: "11111111-1111-4111-8111-111111111111".to_owned(),
        triage_status: FormSubmissionStatus::New,
        follow_up_version: 0,
        next_action: None,
        due_on: None,
        close_reason: None,
        funnel_id: "22222222-2222-4222-8222-222222222222".to_owned(),
        funnel_name: "Funnel".to_owned(),
        funnel_slug: "funnel".to_owned(),
        revision_id: "33333333-3333-4333-8333-333333333333".to_owned(),
        revision_schema_version: 2,
        page_id: "page-home".to_owned(),
        form_id: "form-contact".to_owned(),
        field_values_json: "{}".to_owned(),
        request_consent_acknowledged: 1,
        request_consent_copy_version: "request-v1".to_owned(),
        request_consent_copy: "I agree to be contacted.".to_owned(),
        request_consent_recorded_at: 1_800_000_000,
        request_consent_recorded_at_label: "2026-12-15 00:00 UTC".to_owned(),
        marketing_opt_in: 0,
        marketing_opt_in_copy_version: None,
        marketing_opt_in_copy: None,
        marketing_opt_in_recorded_at: None,
        marketing_opt_in_recorded_at_label: None,
        submitted_at: 1_800_000_000,
        submitted_at_label: "2026-12-15 00:00 UTC".to_owned(),
        submitted_at_datetime: "2026-12-15T00:00:00Z".to_owned(),
        expires_at: 1_807_776_000,
        expires_at_label: "2027-01-13 00:00 UTC".to_owned(),
        delivery_state: delivery_state.map(str::to_owned),
        delivery_retry_scheduled: retry_scheduled,
        delivery_retry_window_open: 1,
        delivery_attempt_count: attempts,
        delivery_last_attempt_at_label: attempts.map(|_| "2026-12-15 00:01 UTC".to_owned()),
        delivery_next_attempt_at_label: (retry_scheduled == 1)
            .then(|| "2026-12-15 00:06 UTC".to_owned()),
        delivery_last_http_status: (outcome == Some("transport_retryable")).then_some(503),
        delivery_last_outcome_code: outcome.map(str::to_owned),
    }
}

#[test]
fn expired_delivery_window_hides_retry_for_pending_and_failed_tasks() {
    let mut pending = record(Some("pending"), Some(1), 1, Some("transport_retryable"));
    pending.delivery_retry_window_open = 0;
    let pending = form_submission_delivery(&pending).expect("valid pending delivery");
    let pending = serde_json::to_value(pending).expect("delivery state serializes");
    assert_eq!(pending["can_retry"], false);

    let mut failed = record(Some("failed"), Some(1), 0, Some("receiver_rejected"));
    failed.delivery_retry_window_open = 0;
    let failed = form_submission_delivery(&failed).expect("valid failed delivery");
    let failed = serde_json::to_value(failed).expect("delivery state serializes");
    assert_eq!(failed["can_retry"], false);
}

#[test]
fn inbox_only_submission_has_no_delivery_actions() {
    let delivery = form_submission_delivery(&record(None, None, 0, None)).expect("valid inbox row");
    let response = serde_json::to_value(delivery).expect("delivery state serializes");

    assert_eq!(response["status"], "inbox_only");
    assert_eq!(response["attempt_count"], 0);
    assert_eq!(response["can_retry"], false);
    assert_eq!(response["can_cancel"], false);
}

#[test]
fn retry_wait_and_failed_delivery_expose_only_safe_actions() {
    let waiting = form_submission_delivery(&record(
        Some("pending"),
        Some(1),
        1,
        Some("transport_retryable"),
    ))
    .expect("valid scheduled retry");
    let waiting = serde_json::to_value(waiting).expect("delivery state serializes");
    assert_eq!(waiting["status"], "retry_scheduled");
    assert_eq!(waiting["next_attempt_at_label"], "2026-12-15 00:06 UTC");
    assert_eq!(waiting["last_http_status"], 503);
    assert_eq!(waiting["can_retry"], true);
    assert_eq!(waiting["can_cancel"], true);

    let failed = form_submission_delivery(&record(
        Some("failed"),
        Some(1),
        0,
        Some("receiver_rejected"),
    ))
    .expect("valid terminal response");
    let failed = serde_json::to_value(failed).expect("delivery state serializes");
    assert_eq!(failed["status"], "failed");
    assert_eq!(failed["can_retry"], true);
    assert_eq!(failed["can_cancel"], false);
}

#[test]
fn incompatible_mapping_and_cancelled_delivery_cannot_be_retried() {
    let incompatible = form_submission_delivery(&record(
        Some("failed"),
        Some(1),
        0,
        Some("incompatible_mapping"),
    ))
    .expect("valid incompatible mapping outcome");
    let incompatible = serde_json::to_value(incompatible).expect("delivery state serializes");
    assert_eq!(incompatible["status"], "failed");
    assert_eq!(incompatible["can_retry"], false);

    let cancelled = form_submission_delivery(&record(
        Some("failed"),
        Some(0),
        0,
        Some("cancelled_by_owner"),
    ))
    .expect("valid cancellation outcome");
    let cancelled = serde_json::to_value(cancelled).expect("delivery state serializes");
    assert_eq!(cancelled["status"], "cancelled");
    assert_eq!(cancelled["can_retry"], false);
}
