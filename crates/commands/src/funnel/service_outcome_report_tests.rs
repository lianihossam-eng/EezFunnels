use super::{build_service_outcome_report, FunnelCommandError, ServiceOutcomeReportReadCommand};
use eez_domain::FormCompletionPeriod;
use eez_storage::repository::PublicServiceOutcomeReportRow;

fn summary() -> PublicServiceOutcomeReportRow {
    PublicServiceOutcomeReportRow {
        row_type: "summary".to_owned(),
        as_of: "2026-10-31T12:00:00Z".to_owned(),
        history_started_at: "2026-10-20T10:00:00Z".to_owned(),
        retained_submissions: 5,
        history_covered_submissions: 3,
        current_new: 1,
        current_contacted: 1,
        current_qualified: 2,
        current_closed: 1,
        submissions_taken_in_charge: 2,
        submissions_qualified: 1,
        submissions_closed: 1,
        submissions_closed_with_reason: 1,
        first_contact_count: 2,
        average_first_contact_delay_seconds: 86_400,
    }
}

#[test]
fn outcome_report_keeps_current_stock_separate_from_covered_recorded_outcomes() {
    let report = build_service_outcome_report(
        "funnel-a".to_owned(),
        FormCompletionPeriod::Days30,
        vec![summary()],
    )
    .expect("valid aggregate produces an outcome report");

    assert_eq!(report.funnel_id, "funnel-a");
    assert_eq!(report.period_days, 30);
    assert_eq!(report.as_of, "2026-10-31T12:00:00Z");
    assert_eq!(report.history_started_at, "2026-10-20T10:00:00Z");
    assert_eq!(report.retained_submissions, 5);
    assert_eq!(report.history_covered_submissions, 3);
    assert_eq!(report.history_uncovered_submissions, 2);
    assert_eq!(report.current_stock.new, 1);
    assert_eq!(report.current_stock.contacted, 1);
    assert_eq!(report.current_stock.qualified, 2);
    assert_eq!(report.current_stock.closed, 1);
    assert_eq!(report.recorded_outcomes.taken_in_charge, 2);
    assert_eq!(report.recorded_outcomes.qualified, 1);
    assert_eq!(report.recorded_outcomes.closed, 1);
    assert_eq!(report.recorded_outcomes.closed_with_reason, 1);
    assert_eq!(report.first_contact.submission_count, 2);
    assert_eq!(report.first_contact.average_delay_seconds, 86_400);

    let serialized = serde_json::to_string(&report).expect("report serializes");
    for private_field in ["submission_id", "field_values", "email", "close_reason"] {
        assert!(
            !serialized.contains(private_field),
            "unexpected {private_field}"
        );
    }
    assert!(serialized.contains("service.outcome.report.read"));
}

#[test]
fn outcome_report_rejects_unbalanced_counts_and_missing_summary() {
    let mut inconsistent = summary();
    inconsistent.current_closed = 0;
    assert!(matches!(
        build_service_outcome_report(
            "funnel-a".to_owned(),
            FormCompletionPeriod::Days30,
            vec![inconsistent]
        ),
        Err(FunnelCommandError::Internal)
    ));

    assert!(matches!(
        build_service_outcome_report(
            "funnel-a".to_owned(),
            FormCompletionPeriod::Days30,
            Vec::new()
        ),
        Err(FunnelCommandError::Internal)
    ));
}

#[test]
fn outcome_report_rejects_transition_counts_outside_the_history_covered_cohort() {
    let mut inconsistent = summary();
    inconsistent.submissions_qualified = 4;
    assert!(matches!(
        build_service_outcome_report(
            "funnel-a".to_owned(),
            FormCompletionPeriod::Days30,
            vec![inconsistent]
        ),
        Err(FunnelCommandError::Internal)
    ));

    let mut inconsistent = summary();
    inconsistent.first_contact_count = 1;
    assert!(matches!(
        build_service_outcome_report(
            "funnel-a".to_owned(),
            FormCompletionPeriod::Days30,
            vec![inconsistent]
        ),
        Err(FunnelCommandError::Internal)
    ));
}

#[test]
fn outcome_report_command_rejects_invalid_periods_ids_and_extra_fields() {
    assert!(ServiceOutcomeReportReadCommand::from_json(
        r#"{"funnel_id":"funnel-a","period_days":30}"#
    )
    .is_ok());
    for parameters in [
        r#"{"funnel_id":"funnel-a","period_days":14}"#,
        r#"{"funnel_id":"bad/id","period_days":30}"#,
        r#"{"funnel_id":"funnel-a","period_days":30,"extra":true}"#,
        r#"{"period_days":30}"#,
    ] {
        assert!(matches!(
            ServiceOutcomeReportReadCommand::from_json(parameters),
            Err(FunnelCommandError::InvalidParameters | FunnelCommandError::MissingField)
        ));
    }
}
