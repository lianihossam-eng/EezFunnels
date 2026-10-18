use super::{build_interaction_report, FunnelCommandError, InteractionReportReadCommand};
use eez_domain::InteractionReportPeriod;
use eez_storage::repository::PublicInteractionReportRow;

fn day_row(utc_day: &str, event_kind: &str, events: i64) -> PublicInteractionReportRow {
    PublicInteractionReportRow {
        row_type: "day_kind".to_owned(),
        utc_day: Some(utc_day.to_owned()),
        event_kind: Some(event_kind.to_owned()),
        campaign_id: None,
        events,
    }
}

#[test]
fn report_builder_sorts_zero_fills_and_returns_aggregate_counts_only() {
    let mut rows = Vec::new();
    for day in 11..=17 {
        let date = format!("2026-10-{day:02}");
        rows.push(day_row(&date, "page_viewed", if day == 17 { 1 } else { 0 }));
        rows.push(day_row(&date, "form_viewed", if day == 11 { 2 } else { 0 }));
        rows.push(day_row(
            &date,
            "form_started",
            if day == 14 { 1 } else { 0 },
        ));
    }
    rows.push(PublicInteractionReportRow {
        row_type: "campaign".to_owned(),
        utc_day: None,
        event_kind: None,
        campaign_id: None,
        events: 3,
    });
    rows.push(PublicInteractionReportRow {
        row_type: "campaign".to_owned(),
        utc_day: None,
        event_kind: None,
        campaign_id: Some("spring-launch".to_owned()),
        events: 1,
    });
    rows.reverse();

    let report =
        build_interaction_report("funnel-a".to_owned(), InteractionReportPeriod::Days7, rows)
            .expect("valid aggregate rows produce a report");

    assert_eq!(report.funnel_id, "funnel-a");
    assert_eq!(report.period_days, 7);
    assert_eq!(report.total_events, 4);
    assert_eq!(report.days.len(), 7);
    assert_eq!(report.days[0].utc_day, "2026-10-11");
    assert_eq!(report.days[0].form_viewed, 2);
    assert_eq!(report.days[1].page_viewed, 0);
    assert_eq!(report.days[6].utc_day, "2026-10-17");
    assert!(report.days[6].is_partial_today);
    assert!(report.days[..6].iter().all(|day| !day.is_partial_today));
    assert_eq!(report.campaigns.len(), 2);
    assert_eq!(report.campaigns[0].campaign_id, None);
    assert_eq!(report.campaigns[0].events, 3);
    assert_eq!(
        report.campaigns[1].campaign_id.as_deref(),
        Some("spring-launch")
    );
    assert_eq!(report.campaigns[1].events, 1);

    let value = serde_json::to_value(report).expect("report serializes");
    let encoded = value.to_string();
    assert!(encoded.contains("interaction.report.read"));
    assert!(!encoded.contains("unregistered"));
    for private_field in [
        "event_id",
        "replay_fingerprint",
        "received_at",
        "visitor_id",
        "session_id",
        "form_id",
        "email",
    ] {
        assert!(
            value.get(private_field).is_none(),
            "unexpected {private_field}"
        );
        assert!(
            !encoded.contains(private_field),
            "unexpected {private_field}"
        );
    }
}

#[test]
fn report_builder_rejects_incomplete_or_inconsistent_aggregates() {
    let rows = vec![day_row("2026-10-17", "page_viewed", 1)];
    assert!(matches!(
        build_interaction_report("funnel-a".to_owned(), InteractionReportPeriod::Days7, rows),
        Err(FunnelCommandError::Internal)
    ));
}

#[test]
fn interaction_report_command_rejects_invalid_periods_ids_and_extra_fields() {
    assert!(InteractionReportReadCommand::from_json(
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
            InteractionReportReadCommand::from_json(parameters),
            Err(FunnelCommandError::InvalidParameters | FunnelCommandError::MissingField)
        ));
    }
}
