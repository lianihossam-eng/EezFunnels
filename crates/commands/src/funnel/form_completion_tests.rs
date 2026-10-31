use super::{build_form_completion_report, FormCompletionReportCommand, FunnelCommandError};
use eez_domain::FormCompletionPeriod;
use eez_storage::repository::FormCompletionCountRow;

const AS_OF: &str = "2026-10-17T12:00:00Z";

fn row(
    row_type: &str,
    utc_day: Option<&str>,
    page_id: Option<&str>,
    form_id: Option<&str>,
    revision_id: Option<&str>,
    submission_channel: Option<&str>,
    completions: i64,
) -> FormCompletionCountRow {
    FormCompletionCountRow {
        row_type: row_type.to_owned(),
        utc_day: utc_day.map(str::to_owned),
        page_id: page_id.map(str::to_owned),
        form_id: form_id.map(str::to_owned),
        revision_id: revision_id.map(str::to_owned),
        submission_channel: submission_channel.map(str::to_owned),
        as_of: AS_OF.to_owned(),
        completions,
    }
}

#[test]
fn report_builder_zero_fills_dates_groups_stable_references_and_omits_payload_fields() {
    let mut rows = (11..=17)
        .map(|day| {
            row(
                "day",
                Some(&format!("2026-10-{day:02}")),
                None,
                None,
                None,
                None,
                match day {
                    11 => 5,
                    17 => 1,
                    _ => 0,
                },
            )
        })
        .collect::<Vec<_>>();
    rows.extend([
        row("page", None, Some("page-a"), None, None, None, 6),
        row("form", None, None, Some("form-a"), None, None, 3),
        row("form", None, None, Some("form-b"), None, None, 3),
        row("revision", None, None, None, Some("revision-a"), None, 6),
        row("channel", None, None, None, None, Some("unattributed"), 4),
        row("channel", None, None, None, None, Some("search"), 2),
    ]);
    rows.reverse();

    let report =
        build_form_completion_report("funnel-a".to_owned(), FormCompletionPeriod::Days7, rows)
            .expect("valid aggregate rows produce a report");

    assert_eq!(report.funnel_id, "funnel-a");
    assert_eq!(report.period_days, 7);
    assert_eq!(report.as_of, AS_OF);
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
            .pages
            .iter()
            .map(|page| (page.page_id.as_str(), page.completions))
            .collect::<Vec<_>>(),
        [("page-a", 6)]
    );
    assert_eq!(
        report
            .forms
            .iter()
            .map(|form| (form.form_id.as_str(), form.completions))
            .collect::<Vec<_>>(),
        [("form-a", 3), ("form-b", 3)]
    );
    assert_eq!(
        report
            .revisions
            .iter()
            .map(|revision| (revision.revision_id.as_str(), revision.completions))
            .collect::<Vec<_>>(),
        [("revision-a", 6)]
    );
    assert_eq!(
        report
            .channels
            .iter()
            .map(|channel| (channel.channel, channel.label, channel.completions))
            .collect::<Vec<_>>(),
        [("unattributed", "Unattributed", 4), ("search", "Search", 2)]
    );
    let value = serde_json::to_value(report).expect("report serializes");
    assert!(value.get("field_values_json").is_none());
    assert!(value.get("request_consent_copy").is_none());
    assert!(value.to_string().contains("unattributed"));
    assert!(!value.to_string().contains("raw-utm"));
}

#[test]
fn report_builder_rejects_inconsistent_context_and_group_totals() {
    let days = (11..=17)
        .map(|day| {
            row(
                "day",
                Some(&format!("2026-10-{day:02}")),
                None,
                None,
                None,
                None,
                0,
            )
        })
        .collect::<Vec<_>>();
    let mut incomplete_groups = days.clone();
    incomplete_groups[6].completions = 1;
    assert!(matches!(
        build_form_completion_report(
            "funnel-a".to_owned(),
            FormCompletionPeriod::Days7,
            incomplete_groups,
        ),
        Err(FunnelCommandError::Internal)
    ));

    let mut rows = days;
    rows[0].as_of = "2026-10-17T12:01:00Z".to_owned();
    rows.push(row("page", None, Some("page-a"), None, None, None, 1));
    rows.push(row("form", None, None, Some("form-a"), None, None, 1));
    rows.push(row(
        "revision",
        None,
        None,
        None,
        Some("revision-a"),
        None,
        1,
    ));
    rows.push(row("channel", None, None, None, None, Some("search"), 1));
    assert!(matches!(
        build_form_completion_report("funnel-a".to_owned(), FormCompletionPeriod::Days7, rows),
        Err(FunnelCommandError::Internal)
    ));
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
