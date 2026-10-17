use super::{build_form_completion_report, FormCompletionReportCommand, FunnelCommandError};
use eez_domain::FormCompletionPeriod;
use eez_storage::repository::FormCompletionCountRow;

#[test]
fn report_builder_zero_fills_dates_and_sorts_forms_without_payload_fields() {
    let mut rows = vec![
        FormCompletionCountRow {
            utc_day: "2026-10-11".to_owned(),
            form_id: Some("form-b".to_owned()),
            completions: 3,
        },
        FormCompletionCountRow {
            utc_day: "2026-10-11".to_owned(),
            form_id: Some("form-a".to_owned()),
            completions: 2,
        },
        FormCompletionCountRow {
            utc_day: "2026-10-12".to_owned(),
            form_id: None,
            completions: 0,
        },
        FormCompletionCountRow {
            utc_day: "2026-10-13".to_owned(),
            form_id: None,
            completions: 0,
        },
        FormCompletionCountRow {
            utc_day: "2026-10-14".to_owned(),
            form_id: None,
            completions: 0,
        },
        FormCompletionCountRow {
            utc_day: "2026-10-15".to_owned(),
            form_id: None,
            completions: 0,
        },
        FormCompletionCountRow {
            utc_day: "2026-10-16".to_owned(),
            form_id: None,
            completions: 0,
        },
        FormCompletionCountRow {
            utc_day: "2026-10-17".to_owned(),
            form_id: Some("form-a".to_owned()),
            completions: 1,
        },
    ];
    rows.reverse();

    let report =
        build_form_completion_report("funnel-a".to_owned(), FormCompletionPeriod::Days7, rows)
            .expect("valid aggregate rows produce a report");

    assert_eq!(report.funnel_id, "funnel-a");
    assert_eq!(report.period_days, 7);
    assert_eq!(report.total_completions, 6);
    assert_eq!(report.days.len(), 7);
    assert_eq!(report.days[0].utc_day, "2026-10-11");
    assert_eq!(report.days[0].completions, 5);
    assert!(!report.days[0].is_partial_today);
    assert_eq!(report.days[1].completions, 0);
    assert_eq!(report.days[6].utc_day, "2026-10-17");
    assert!(report.days[6].is_partial_today);
    assert_eq!(report.days[6].completions, 1);
    assert_eq!(
        report
            .forms
            .iter()
            .map(|form| (form.form_id.as_str(), form.completions))
            .collect::<Vec<_>>(),
        [("form-a", 3), ("form-b", 3)]
    );
    let value = serde_json::to_value(report).expect("report serializes");
    assert!(value.get("field_values_json").is_none());
    assert!(value.get("request_consent_copy").is_none());
}

#[test]
fn completion_report_command_rejects_open_periods_and_invalid_funnel_ids() {
    assert!(
        FormCompletionReportCommand::from_json(r#"{"funnel_id":"funnel-a","period_days":30}"#)
            .is_ok()
    );
    for parameters in [
        r#"{"funnel_id":"funnel-a","period_days":14}"#,
        r#"{"funnel_id":"bad/id","period_days":30}"#,
        r#"{"funnel_id":"funnel-a","period_days":30,"extra":true}"#,
        r#"{"period_days":30}"#,
    ] {
        assert!(matches!(
            FormCompletionReportCommand::from_json(parameters),
            Err(FunnelCommandError::InvalidParameters | FunnelCommandError::MissingField)
        ));
    }
}
