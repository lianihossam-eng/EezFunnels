#[cfg(feature = "browser")]
use eez_domain::service_brief::SERVICE_BRIEF_FORMAT_VERSION;
#[cfg(any(feature = "browser", test))]
use eez_domain::service_brief::{BriefVersion, ServiceBrief};
#[cfg(any(feature = "browser", test))]
use eez_domain::FormCompletionPeriod;
use eez_domain::FormSubmissionStatus;
#[cfg(any(feature = "browser", test))]
use eez_domain::InteractionReportPeriod;
use serde::Deserialize;
use serde_json::Value;
#[cfg(any(feature = "browser", test))]
use std::collections::BTreeSet;

#[cfg(feature = "browser")]
use super::idempotency::PreparedIdempotencyKey;
#[cfg(feature = "browser")]
use dioxus::prelude::*;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct FormSubmissionListResponse {
    pub(super) items: Vec<FormSubmissionListItem>,
    pub(super) next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct FormSubmissionListItem {
    pub(super) submission_id: String,
    pub(super) status: FormSubmissionStatus,
    pub(super) follow_up_version: i32,
    pub(super) next_action: Option<String>,
    pub(super) due_on: Option<String>,
    pub(super) close_reason: Option<String>,
    pub(super) funnel_name: String,
    pub(super) page_title: String,
    pub(super) form_purpose: String,
    pub(super) fields: Vec<FormSubmissionField>,
    pub(super) request_consent_acknowledged: bool,
    pub(super) request_consent_copy_version: String,
    pub(super) request_consent_copy: String,
    pub(super) request_consent_recorded_at_label: String,
    pub(super) marketing_opt_in: bool,
    pub(super) marketing_opt_in_copy_version: Option<String>,
    pub(super) marketing_opt_in_copy: Option<String>,
    pub(super) marketing_opt_in_recorded_at_label: Option<String>,
    pub(super) submitted_at_label: String,
    pub(super) submitted_at_datetime: String,
    pub(super) expires_at_label: String,
    pub(super) delivery: FormSubmissionDelivery,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct FormSubmissionDelivery {
    pub(super) status: String,
    pub(super) attempt_count: u8,
    pub(super) last_attempt_at_label: Option<String>,
    pub(super) next_attempt_at_label: Option<String>,
    pub(super) last_http_status: Option<u16>,
    pub(super) last_outcome_code: Option<String>,
    pub(super) can_retry: bool,
    pub(super) can_cancel: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct WebhookDestinationResponse {
    pub(super) enabled: bool,
    pub(super) destination_version_id: Option<String>,
    pub(super) destination_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct WebhookFormDeliverySnapshot {
    pub(super) funnel_id: String,
    pub(super) published_revision_id: Option<String>,
    pub(super) destination_version_id: Option<String>,
    pub(super) forms: Vec<WebhookFormDeliveryOption>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct WebhookFormDeliveryOption {
    pub(super) form_id: String,
    pub(super) purpose: String,
    pub(super) fields: Vec<WebhookFormFieldOption>,
    pub(super) enabled: bool,
    pub(super) expected_destination_version_id: Option<String>,
    pub(super) mapping_version_id: Option<String>,
    pub(super) selected_field_ids: Vec<String>,
    pub(super) mapping_compatible: bool,
    pub(super) needs_revalidation: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct WebhookFormFieldOption {
    pub(super) field_id: String,
    pub(super) label: String,
}

#[cfg(feature = "browser")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DeliveryControlFailure {
    Conflict,
    Uncertain,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct FormSubmissionField {
    pub(super) label: String,
    pub(super) value: Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct FormCompletionReportResponse {
    pub(super) funnel_id: String,
    pub(super) period_days: u8,
    pub(super) as_of: String,
    pub(super) report_context: ReportContextResponse,
    pub(super) total_completions: u64,
    pub(super) days: Vec<DailyFormCompletionCountResponse>,
    pub(super) pages: Vec<PageFormCompletionCountResponse>,
    pub(super) forms: Vec<FormCompletionCountResponse>,
    pub(super) revisions: Vec<RevisionFormCompletionCountResponse>,
    pub(super) channels: Vec<FormSubmissionChannelCountResponse>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct ServiceOutcomeReportResponse {
    pub(super) funnel_id: String,
    pub(super) period_days: u8,
    pub(super) as_of: String,
    pub(super) history_started_at: String,
    pub(super) report_context: ReportContextResponse,
    pub(super) retained_submissions: u64,
    pub(super) history_covered_submissions: u64,
    pub(super) history_uncovered_submissions: u64,
    pub(super) current_stock: ServiceOutcomeStatusCountsResponse,
    pub(super) recorded_outcomes: ServiceOutcomeCountsResponse,
    pub(super) first_contact: FirstContactDelaySummaryResponse,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(super) struct ServiceOutcomeStatusCountsResponse {
    pub(super) new: u64,
    pub(super) contacted: u64,
    pub(super) qualified: u64,
    pub(super) closed: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(super) struct ServiceOutcomeCountsResponse {
    pub(super) taken_in_charge: u64,
    pub(super) qualified: u64,
    pub(super) closed: u64,
    pub(super) closed_with_reason: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(super) struct FirstContactDelaySummaryResponse {
    pub(super) submission_count: u64,
    pub(super) average_delay_seconds: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum ReportCollectionState {
    Enabled,
    Disabled,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum HistoricalCoverage {
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(super) struct ReportContextResponse {
    pub(super) collection_state: ReportCollectionState,
    pub(super) effective_retention_days: Option<u16>,
    pub(super) historical_coverage: HistoricalCoverage,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct DailyFormCompletionCountResponse {
    pub(super) utc_day: String,
    pub(super) completions: u64,
    pub(super) is_partial_today: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct FormCompletionCountResponse {
    pub(super) form_id: String,
    pub(super) completions: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct PageFormCompletionCountResponse {
    pub(super) page_id: String,
    pub(super) completions: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct RevisionFormCompletionCountResponse {
    pub(super) revision_id: String,
    pub(super) completions: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct FormSubmissionChannelCountResponse {
    pub(super) channel: String,
    pub(super) label: String,
    pub(super) completions: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct InteractionReportResponse {
    pub(super) funnel_id: String,
    pub(super) period_days: u8,
    pub(super) as_of: String,
    pub(super) report_context: ReportContextResponse,
    pub(super) total_events: u64,
    pub(super) days: Vec<DailyInteractionReportDayResponse>,
    pub(super) pages: Vec<PageInteractionCountResponse>,
    pub(super) forms: Vec<FormInteractionCountResponse>,
    pub(super) revisions: Vec<RevisionInteractionCountResponse>,
    pub(super) campaigns: Vec<CampaignInteractionCountResponse>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct DailyInteractionReportDayResponse {
    pub(super) utc_day: String,
    pub(super) page_viewed: u64,
    pub(super) form_viewed: u64,
    pub(super) form_started: u64,
    pub(super) is_partial_today: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct CampaignInteractionCountResponse {
    pub(super) campaign_id: Option<String>,
    pub(super) events: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct PageInteractionCountResponse {
    pub(super) page_id: String,
    pub(super) events: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct FormInteractionCountResponse {
    pub(super) form_id: String,
    pub(super) events: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct RevisionInteractionCountResponse {
    pub(super) revision_id: String,
    pub(super) events: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct FunnelCard {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) slug: String,
    pub(super) published_revision_id: Option<String>,
    pub(super) created_at: String,
    pub(super) updated_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct CampaignRegistryResponse {
    pub(super) funnel_id: String,
    pub(super) campaign_ids: Vec<String>,
    pub(super) max_campaign_ids: usize,
    pub(super) published: bool,
    pub(super) campaign_links: Vec<CampaignLinkResponse>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct CampaignLinkResponse {
    pub(super) campaign_id: String,
    pub(super) url: Option<String>,
    pub(super) relative_path: Option<String>,
}

#[cfg(feature = "browser")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CampaignMutationFailure {
    Uncertain,
    Rejected,
}

#[cfg(any(feature = "browser", test))]
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(super) struct ServiceBriefApiResponse {
    pub(super) funnel_id: String,
    pub(super) version: BriefVersion,
    pub(super) format_version: u8,
    pub(super) brief: ServiceBrief,
    pub(super) updated_at: Option<String>,
}

#[cfg(any(feature = "browser", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ServiceBriefSaveFailure {
    Conflict,
    Uncertain,
    Rejected,
}

#[cfg(feature = "browser")]
#[derive(Deserialize)]
struct FunnelListResponse {
    items: Vec<FunnelCard>,
}

#[cfg(any(feature = "browser", test))]
#[derive(Deserialize)]
pub(super) struct BrowserResponse {
    pub(super) status: u16,
    pub(super) body: String,
}

#[cfg(any(feature = "browser", test))]
pub(super) fn interaction_report_is_valid(
    report: &InteractionReportResponse,
    funnel_id: &str,
    period: InteractionReportPeriod,
) -> bool {
    let daily_total = report.days.iter().try_fold(0_u64, |total, day| {
        total
            .checked_add(day.page_viewed)?
            .checked_add(day.form_viewed)?
            .checked_add(day.form_started)
    });
    let mut campaign_ids = BTreeSet::new();
    let campaign_total = report.campaigns.iter().try_fold(0_u64, |total, campaign| {
        if !campaign_ids.insert(campaign.campaign_id.as_deref()) {
            return None;
        }
        total.checked_add(campaign.events)
    });
    let page_total = unique_group_total(
        report
            .pages
            .iter()
            .map(|page| (page.page_id.as_str(), page.events)),
    );
    let form_total = unique_group_total(
        report
            .forms
            .iter()
            .map(|form| (form.form_id.as_str(), form.events)),
    );
    let revision_total = unique_group_total(
        report
            .revisions
            .iter()
            .map(|revision| (revision.revision_id.as_str(), revision.events)),
    );
    let dates_are_valid = report_dates_are_valid(
        report
            .days
            .iter()
            .map(|day| (&day.utc_day, day.is_partial_today)),
        &report.as_of,
        period.days(),
    );
    report.funnel_id == funnel_id
        && report.period_days == period.days()
        && dates_are_valid
        && report_context_is_valid(&report.report_context)
        && daily_total == Some(report.total_events)
        && campaign_total == Some(report.total_events)
        && page_total == Some(report.total_events)
        && form_total.is_some_and(|total| total <= report.total_events)
        && revision_total == Some(report.total_events)
}

#[cfg(any(feature = "browser", test))]
pub(super) fn form_completion_report_is_valid(
    report: &FormCompletionReportResponse,
    funnel_id: &str,
    period: FormCompletionPeriod,
) -> bool {
    let daily_total = report
        .days
        .iter()
        .try_fold(0_u64, |total, day| total.checked_add(day.completions));
    let page_total = unique_group_total(
        report
            .pages
            .iter()
            .map(|page| (page.page_id.as_str(), page.completions)),
    );
    let form_total = unique_group_total(
        report
            .forms
            .iter()
            .map(|form| (form.form_id.as_str(), form.completions)),
    );
    let revision_total = unique_group_total(
        report
            .revisions
            .iter()
            .map(|revision| (revision.revision_id.as_str(), revision.completions)),
    );
    let channel_total = report.channels.iter().try_fold(0_u64, |total, channel| {
        total.checked_add(channel.completions)
    });
    report.funnel_id == funnel_id
        && report.period_days == period.days()
        && report_dates_are_valid(
            report
                .days
                .iter()
                .map(|day| (&day.utc_day, day.is_partial_today)),
            &report.as_of,
            period.days(),
        )
        && report_context_is_valid(&report.report_context)
        && daily_total == Some(report.total_completions)
        && page_total == Some(report.total_completions)
        && form_total == Some(report.total_completions)
        && revision_total == Some(report.total_completions)
        && channel_total == Some(report.total_completions)
}

#[cfg(any(feature = "browser", test))]
pub(super) fn service_outcome_report_is_valid(
    report: &ServiceOutcomeReportResponse,
    funnel_id: &str,
    period: FormCompletionPeriod,
) -> bool {
    let stock_total = report
        .current_stock
        .new
        .checked_add(report.current_stock.contacted)
        .and_then(|total| total.checked_add(report.current_stock.qualified))
        .and_then(|total| total.checked_add(report.current_stock.closed));
    let outcomes_fit_covered_cohort = [
        report.recorded_outcomes.taken_in_charge,
        report.recorded_outcomes.qualified,
        report.recorded_outcomes.closed,
        report.recorded_outcomes.closed_with_reason,
        report.first_contact.submission_count,
    ]
    .into_iter()
    .all(|count| count <= report.history_covered_submissions);
    let history_boundary_is_valid = report_as_of_day(&report.history_started_at)
        .zip(report_as_of_day(&report.as_of))
        .is_some_and(|(history_day, as_of_day)| {
            report.history_started_at <= report.as_of && history_day <= as_of_day
        });

    report.funnel_id == funnel_id
        && report.period_days == period.days()
        && report_as_of_day(&report.as_of).is_some()
        && history_boundary_is_valid
        && report_context_is_valid(&report.report_context)
        && report.history_covered_submissions <= report.retained_submissions
        && report.history_uncovered_submissions
            == report.retained_submissions - report.history_covered_submissions
        && stock_total == Some(report.retained_submissions)
        && outcomes_fit_covered_cohort
        && report.recorded_outcomes.closed_with_reason <= report.recorded_outcomes.closed
        && report.first_contact.submission_count == report.recorded_outcomes.taken_in_charge
        && (report.first_contact.submission_count > 0
            || report.first_contact.average_delay_seconds == 0)
}

#[cfg(any(feature = "browser", test))]
fn unique_group_total<'a>(mut groups: impl Iterator<Item = (&'a str, u64)>) -> Option<u64> {
    let mut ids = BTreeSet::new();
    groups.try_fold(0_u64, |total, (id, count)| {
        if id.is_empty() || id.len() > 128 || !ids.insert(id) {
            return None;
        }
        total.checked_add(count)
    })
}

#[cfg(any(feature = "browser", test))]
fn report_context_is_valid(context: &ReportContextResponse) -> bool {
    context.historical_coverage == HistoricalCoverage::Unknown
        && match context.collection_state {
            ReportCollectionState::Enabled => context
                .effective_retention_days
                .is_some_and(|days| (1..=3650).contains(&days)),
            ReportCollectionState::Disabled => context
                .effective_retention_days
                .is_none_or(|days| (1..=3650).contains(&days)),
        }
}

#[cfg(any(feature = "browser", test))]
fn report_dates_are_valid<'a>(
    days: impl Iterator<Item = (&'a String, bool)>,
    as_of: &str,
    period_days: u8,
) -> bool {
    let days = days.collect::<Vec<_>>();
    let Some(as_of_day) = report_as_of_day(as_of) else {
        return false;
    };
    days.len() == usize::from(period_days)
        && days
            .last()
            .is_some_and(|(day, _)| day.as_str() == as_of_day)
        && days.iter().all(|(day, partial)| {
            report_day_is_valid(day) && *partial == (day.as_str() == as_of_day)
        })
        && !days.windows(2).any(|pair| pair[0].0 >= pair[1].0)
}

#[cfg(any(feature = "browser", test))]
fn report_as_of_day(as_of: &str) -> Option<&str> {
    let bytes = as_of.as_bytes();
    if !(bytes.len() == 20
        && bytes.get(10) == Some(&b'T')
        && bytes.get(13) == Some(&b':')
        && bytes.get(16) == Some(&b':')
        && bytes.get(19) == Some(&b'Z')
        && bytes[..10].iter().enumerate().all(|(index, byte)| {
            if index == 4 || index == 7 {
                *byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
        && bytes[11..19]
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 2 || index == 5 || byte.is_ascii_digit()))
    {
        return None;
    }
    let day = as_of.get(..10)?;
    if !report_day_is_valid(day)
        || decimal(&bytes[11..13])? > 23
        || decimal(&bytes[14..16])? > 59
        || decimal(&bytes[17..19])? > 59
    {
        return None;
    }
    Some(day)
}

#[cfg(any(feature = "browser", test))]
fn report_day_is_valid(day: &str) -> bool {
    let bytes = day.as_bytes();
    let shape_is_valid = bytes.len() == 10
        && bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && bytes.iter().enumerate().all(|(index, byte)| {
            if index == 4 || index == 7 {
                *byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        });
    if !shape_is_valid {
        return false;
    }
    let Some(year) = decimal(&bytes[..4]) else {
        return false;
    };
    let Some(month) = decimal(&bytes[5..7]) else {
        return false;
    };
    let Some(day) = decimal(&bytes[8..10]) else {
        return false;
    };
    let month_length = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => return false,
    };
    day > 0 && day <= month_length
}

#[cfg(any(feature = "browser", test))]
fn decimal(bytes: &[u8]) -> Option<u32> {
    bytes.iter().try_fold(0_u32, |value, byte| {
        if !byte.is_ascii_digit() {
            return None;
        }
        value.checked_mul(10)?.checked_add(u32::from(byte - b'0'))
    })
}

#[cfg(any(feature = "browser", test))]
pub(super) fn created_summary(body: &str, name: &str) -> String {
    let section_types = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|result| result.get("initial_revision").cloned())
        .and_then(|revision| revision.get("section_types").cloned())
        .and_then(|types| types.as_array().cloned())
        .map(|types| {
            types
                .iter()
                .filter_map(Value::as_str)
                .map(section_label)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|types| !types.is_empty())
        .unwrap_or_else(|| "starter sections".to_owned());
    format!("Created {name} with {section_types}.")
}

pub(super) fn section_label(section_type: &str) -> String {
    match section_type {
        "hero" => "Hero".to_owned(),
        "features" => "Features".to_owned(),
        "testimonials" => "Testimonials".to_owned(),
        "cta" => "CTA".to_owned(),
        "footer" => "Footer".to_owned(),
        _ => "Section".to_owned(),
    }
}

#[cfg(any(feature = "browser", test))]
pub(super) fn error_message(body: &str) -> String {
    let correlation_id = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| value.get("correlation_id").cloned())
        .and_then(|value| value.as_str().map(str::to_owned));
    match correlation_id {
        Some(id) => format!("The request could not be completed. Reference: {id}"),
        None => "The request could not be completed. Try again.".to_owned(),
    }
}

#[cfg(feature = "browser")]
const BROWSER_API_REQUEST: &str = r#"
const request = await dioxus.recv();
const headers = {"Accept": "application/json"};
if (request.body !== null) headers["Content-Type"] = "application/json";
if (request.idempotency_key) headers["Idempotency-Key"] = request.idempotency_key;
try {
    const response = await fetch(request.path, {method: request.method, headers, body: request.body ?? undefined});
    dioxus.send({status: response.status, body: await response.text()});
} catch (_) {
    dioxus.send({status: 0, body: ""});
}
"#;

#[cfg(feature = "browser")]
pub(super) async fn api_request(
    method: &str,
    path: &str,
    body: Option<String>,
    idempotency_key: Option<PreparedIdempotencyKey>,
) -> Result<BrowserResponse, ()> {
    let mut request = document::eval(BROWSER_API_REQUEST);
    request
        .send(serde_json::json!({
            "method": method,
            "path": path,
            "body": body,
            "idempotency_key": idempotency_key.map(|prepared_key| prepared_key.key),
        }))
        .map_err(|_| ())?;
    request.recv().await.map_err(|_| ())
}

#[cfg(feature = "browser")]
pub(super) async fn load_funnels() -> Result<Vec<FunnelCard>, ()> {
    let response = api_request("GET", "/api/funnels", None, None).await?;
    if response.status != 200 {
        return Err(());
    }
    serde_json::from_str::<FunnelListResponse>(&response.body)
        .map(|response| response.items)
        .map_err(|_| ())
}

#[cfg(feature = "browser")]
pub(super) async fn load_campaign_registry(
    funnel_id: &str,
) -> Result<CampaignRegistryResponse, ()> {
    let response = api_request(
        "GET",
        &format!("/api/funnels/{funnel_id}/campaigns"),
        None,
        None,
    )
    .await?;
    if response.status != 200 {
        return Err(());
    }
    let snapshot =
        serde_json::from_str::<CampaignRegistryResponse>(&response.body).map_err(|_| ())?;
    if !campaign_registry_response_is_valid(&snapshot, funnel_id) {
        return Err(());
    }
    Ok(snapshot)
}

#[cfg(feature = "browser")]
pub(super) async fn add_campaign_id(
    funnel_id: &str,
    campaign_id: &str,
) -> Result<CampaignRegistryResponse, CampaignMutationFailure> {
    let body = serde_json::json!({"campaign_id": campaign_id}).to_string();
    mutate_campaign_registry(
        "POST",
        &format!("/api/funnels/{funnel_id}/campaigns"),
        Some(body),
        funnel_id,
    )
    .await
}

#[cfg(feature = "browser")]
pub(super) async fn remove_campaign_id(
    funnel_id: &str,
    campaign_id: &str,
) -> Result<CampaignRegistryResponse, CampaignMutationFailure> {
    mutate_campaign_registry(
        "DELETE",
        &format!("/api/funnels/{funnel_id}/campaigns/{campaign_id}"),
        None,
        funnel_id,
    )
    .await
}

#[cfg(feature = "browser")]
async fn mutate_campaign_registry(
    method: &str,
    path: &str,
    body: Option<String>,
    funnel_id: &str,
) -> Result<CampaignRegistryResponse, CampaignMutationFailure> {
    let response = api_request(method, path, body, None)
        .await
        .map_err(|_| CampaignMutationFailure::Uncertain)?;
    if response.status == 0 || response.status >= 500 {
        return Err(CampaignMutationFailure::Uncertain);
    }
    if response.status != 200 {
        return Err(CampaignMutationFailure::Rejected);
    }
    let snapshot = serde_json::from_str::<CampaignRegistryResponse>(&response.body)
        .map_err(|_| CampaignMutationFailure::Uncertain)?;
    if !campaign_registry_response_is_valid(&snapshot, funnel_id) {
        return Err(CampaignMutationFailure::Uncertain);
    }
    Ok(snapshot)
}

#[cfg(any(feature = "browser", test))]
fn campaign_registry_response_is_valid(
    snapshot: &CampaignRegistryResponse,
    funnel_id: &str,
) -> bool {
    use eez_domain::interaction_event::{CampaignId, MAX_CAMPAIGN_IDS_PER_FUNNEL};

    let ids_are_valid = snapshot
        .campaign_ids
        .iter()
        .all(|campaign_id| CampaignId::parse(campaign_id.clone()).is_ok());
    let ids_are_unique_and_sorted = snapshot.campaign_ids.windows(2).all(|ids| ids[0] < ids[1]);
    snapshot.funnel_id == funnel_id
        && snapshot.max_campaign_ids == MAX_CAMPAIGN_IDS_PER_FUNNEL
        && snapshot.campaign_ids.len() <= snapshot.max_campaign_ids
        && snapshot.campaign_links.len() == snapshot.campaign_ids.len()
        && snapshot
            .campaign_links
            .iter()
            .zip(&snapshot.campaign_ids)
            .all(|(link, campaign_id)| {
                link.campaign_id == *campaign_id
                    && if snapshot.published {
                        link.relative_path
                            .as_deref()
                            .is_some_and(|path| valid_campaign_relative_path(path, campaign_id))
                            && link.url.as_deref().is_none_or(|url| {
                                link.relative_path
                                    .as_deref()
                                    .is_some_and(|path| valid_campaign_url(url, path))
                            })
                    } else {
                        link.url.is_none() && link.relative_path.is_none()
                    }
            })
        && ids_are_valid
        && ids_are_unique_and_sorted
}

#[cfg(any(feature = "browser", test))]
fn valid_campaign_relative_path(path: &str, campaign_id: &str) -> bool {
    let Some((route, query)) = path.split_once('?') else {
        return false;
    };
    route.starts_with("/f/")
        && route.len() > 3
        && route[3..]
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !route[3..].starts_with('-')
        && !route[3..].ends_with('-')
        && query == format!("campaign_id={campaign_id}")
}

#[cfg(any(feature = "browser", test))]
fn valid_campaign_url(url: &str, relative_path: &str) -> bool {
    let Some(authority_and_path) = url.strip_prefix("https://") else {
        return false;
    };
    let Some(path_offset) = authority_and_path.find('/') else {
        return false;
    };
    let (authority, path) = authority_and_path.split_at(path_offset);
    !authority.is_empty()
        && !authority.contains('@')
        && !authority.bytes().any(|byte| {
            byte.is_ascii_whitespace()
                || byte.is_ascii_control()
                || matches!(byte, b'?' | b'#' | b'\\')
        })
        && path == relative_path
}

#[cfg(feature = "browser")]
pub(super) async fn load_submission_page(
    cursor: Option<&str>,
    status: Option<FormSubmissionStatus>,
    funnel_id: Option<&str>,
) -> Result<FormSubmissionListResponse, ()> {
    let mut parameters = Vec::new();
    if let Some(status) = status {
        parameters.push(format!("status={}", status.as_str()));
    }
    if let Some(funnel_id) = funnel_id {
        parameters.push(format!("funnel_id={funnel_id}"));
    }
    if let Some(cursor) = cursor {
        parameters.push(format!("cursor={cursor}"));
    }
    let path = if parameters.is_empty() {
        "/api/submissions".to_owned()
    } else {
        format!("/api/submissions?{}", parameters.join("&"))
    };
    let response = api_request("GET", &path, None, None).await?;
    if response.status != 200 {
        return Err(());
    }
    serde_json::from_str::<FormSubmissionListResponse>(&response.body).map_err(|_| ())
}

#[cfg(feature = "browser")]
pub(super) async fn load_form_completion_report(
    funnel_id: &str,
    period: FormCompletionPeriod,
) -> Result<FormCompletionReportResponse, ()> {
    let path = format!(
        "/api/funnels/{funnel_id}/completions?period={}",
        period.days()
    );
    let response = api_request("GET", &path, None, None).await?;
    if response.status != 200 {
        return Err(());
    }
    let report =
        serde_json::from_str::<FormCompletionReportResponse>(&response.body).map_err(|_| ())?;
    if !form_completion_report_is_valid(&report, funnel_id, period) {
        return Err(());
    }
    Ok(report)
}

#[cfg(feature = "browser")]
pub(super) async fn load_service_outcome_report(
    funnel_id: &str,
    period: FormCompletionPeriod,
) -> Result<ServiceOutcomeReportResponse, ()> {
    let path = format!(
        "/api/funnels/{funnel_id}/service-outcomes?period={}",
        period.days()
    );
    let response = api_request("GET", &path, None, None).await?;
    if response.status != 200 {
        return Err(());
    }
    let report =
        serde_json::from_str::<ServiceOutcomeReportResponse>(&response.body).map_err(|_| ())?;
    if !service_outcome_report_is_valid(&report, funnel_id, period) {
        return Err(());
    }
    Ok(report)
}

#[cfg(feature = "browser")]
pub(super) async fn load_interaction_report(
    funnel_id: &str,
    period: InteractionReportPeriod,
) -> Result<InteractionReportResponse, ()> {
    let path = format!(
        "/api/funnels/{funnel_id}/interaction-report?period={}",
        period.days()
    );
    let response = api_request("GET", &path, None, None).await?;
    if response.status != 200 {
        return Err(());
    }
    let report =
        serde_json::from_str::<InteractionReportResponse>(&response.body).map_err(|_| ())?;
    if !interaction_report_is_valid(&report, funnel_id, period) {
        return Err(());
    }
    Ok(report)
}

#[cfg(feature = "browser")]
pub(super) async fn update_public_form_submission_follow_up(
    submission_id: &str,
    request_body: &str,
    idempotency_key: PreparedIdempotencyKey,
) -> Result<i32, FollowUpMutationFailure> {
    let response = api_request(
        "PUT",
        &format!("/api/submissions/{submission_id}/follow-up"),
        Some(request_body.to_owned()),
        Some(idempotency_key),
    )
    .await
    .map_err(|_| FollowUpMutationFailure::Uncertain)?;
    if response.status == 409 {
        if serde_json::from_str::<serde_json::Value>(&response.body)
            .ok()
            .and_then(|body| {
                body.get("code")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .as_deref()
            == Some("conflict/submission_follow_up_correction_target")
        {
            return Err(FollowUpMutationFailure::CorrectionTargetUnavailable);
        }
        return Err(FollowUpMutationFailure::Conflict);
    }
    if response.status == 0 || response.status >= 500 {
        return Err(FollowUpMutationFailure::Uncertain);
    }
    if response.status != 200 {
        return Err(FollowUpMutationFailure::Rejected);
    }
    let result = serde_json::from_str::<FollowUpMutationResponse>(&response.body)
        .map_err(|_| FollowUpMutationFailure::Uncertain)?;
    let expected_version = serde_json::from_str::<serde_json::Value>(request_body)
        .ok()
        .and_then(|body| {
            body.get("expected_version")
                .and_then(serde_json::Value::as_i64)
        })
        .and_then(|version| i32::try_from(version).ok());
    if result.submission_id == submission_id
        && expected_version
            .is_some_and(|version| (version..=version.saturating_add(1)).contains(&result.version))
    {
        Ok(result.version)
    } else {
        Err(FollowUpMutationFailure::Uncertain)
    }
}

#[cfg(feature = "browser")]
pub(super) async fn load_public_form_submission_follow_up(
    submission_id: &str,
) -> Result<FollowUpHistoryResponse, ()> {
    let response = api_request(
        "GET",
        &format!("/api/submissions/{submission_id}/follow-up"),
        None,
        None,
    )
    .await?;
    if response.status != 200 {
        return Err(());
    }
    serde_json::from_str::<FollowUpHistoryResponse>(&response.body).map_err(|_| ())
}

#[cfg(feature = "browser")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FollowUpMutationFailure {
    Conflict,
    CorrectionTargetUnavailable,
    Uncertain,
    Rejected,
}

#[cfg(feature = "browser")]
#[derive(Clone, Debug, Deserialize, PartialEq)]
struct FollowUpMutationResponse {
    submission_id: String,
    version: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct FollowUpHistoryResponse {
    pub(super) current: FollowUpSnapshotResponse,
    pub(super) events: Vec<FollowUpEventResponse>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct FollowUpSnapshotResponse {
    pub(super) submission_id: String,
    pub(super) version: i32,
    pub(super) status: FormSubmissionStatus,
    pub(super) next_action: Option<String>,
    pub(super) due_on: Option<String>,
    pub(super) close_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct FollowUpEventResponse {
    pub(super) version: i32,
    pub(super) status: FormSubmissionStatus,
    pub(super) next_action: Option<String>,
    pub(super) due_on: Option<String>,
    pub(super) close_reason: Option<String>,
    pub(super) recorded_at_label: String,
    pub(super) provenance: String,
    pub(super) supersedes_version: Option<i32>,
    pub(super) corrects_version: Option<i32>,
}

#[cfg(feature = "browser")]
pub(super) async fn delete_public_form_submission(submission_id: &str) -> Result<(), ()> {
    let response = api_request(
        "DELETE",
        &format!("/api/submissions/{submission_id}"),
        None,
        None,
    )
    .await?;
    if response.status == 204 {
        Ok(())
    } else {
        Err(())
    }
}

#[cfg(feature = "browser")]
pub(super) async fn load_webhook_destination() -> Result<WebhookDestinationResponse, ()> {
    let response = api_request("GET", "/api/webhook-delivery", None, None).await?;
    if response.status != 200 {
        return Err(());
    }
    serde_json::from_str::<WebhookDestinationResponse>(&response.body).map_err(|_| ())
}

#[cfg(feature = "browser")]
pub(super) async fn save_webhook_destination(
    expected_version_id: Option<&str>,
    expected_enabled: bool,
    destination_url: &str,
    enabled: bool,
    idempotency_key: PreparedIdempotencyKey,
) -> Result<WebhookDestinationResponse, DeliveryControlFailure> {
    let body = serde_json::json!({
        "expected_version_id": expected_version_id,
        "expected_enabled": expected_enabled,
        "destination_url": destination_url,
        "enabled": enabled,
    })
    .to_string();
    let response = api_request(
        "PUT",
        "/api/webhook-delivery",
        Some(body),
        Some(idempotency_key),
    )
    .await
    .map_err(|_| DeliveryControlFailure::Uncertain)?;
    if response.status == 409 {
        return Err(DeliveryControlFailure::Conflict);
    }
    if response.status == 0 || response.status >= 500 {
        return Err(DeliveryControlFailure::Uncertain);
    }
    if response.status != 200 {
        return Err(DeliveryControlFailure::Rejected);
    }
    let saved = serde_json::from_str::<WebhookDestinationResponse>(&response.body)
        .map_err(|_| DeliveryControlFailure::Uncertain)?;
    if saved.enabled != enabled || saved.destination_version_id.is_none() {
        return Err(DeliveryControlFailure::Uncertain);
    }
    Ok(saved)
}

#[cfg(feature = "browser")]
pub(super) async fn load_webhook_form_delivery(
    funnel_id: &str,
) -> Result<WebhookFormDeliverySnapshot, ()> {
    let response = api_request(
        "GET",
        &format!("/api/funnels/{funnel_id}/webhook-delivery"),
        None,
        None,
    )
    .await?;
    if response.status != 200 {
        return Err(());
    }
    let snapshot =
        serde_json::from_str::<WebhookFormDeliverySnapshot>(&response.body).map_err(|_| ())?;
    if snapshot.funnel_id != funnel_id {
        return Err(());
    }
    Ok(snapshot)
}

#[cfg(feature = "browser")]
pub(super) async fn save_webhook_form_delivery(
    funnel_id: &str,
    expected_published_revision_id: &str,
    form_id: &str,
    destination_version_id: &str,
    expected_mapping_version_id: Option<&str>,
    enabled: bool,
    selected_field_ids: &[String],
    idempotency_key: PreparedIdempotencyKey,
) -> Result<WebhookFormDeliverySnapshot, DeliveryControlFailure> {
    let path = format!("/api/funnels/{funnel_id}/webhook-delivery");
    let body = serde_json::json!({
        "form_id": form_id,
        "expected_published_revision_id": expected_published_revision_id,
        "destination_version_id": destination_version_id,
        "expected_mapping_version_id": expected_mapping_version_id,
        "enabled": enabled,
        "selected_field_ids": selected_field_ids,
    })
    .to_string();
    let response = api_request("PUT", &path, Some(body), Some(idempotency_key))
        .await
        .map_err(|_| DeliveryControlFailure::Uncertain)?;
    if response.status == 409 {
        return Err(DeliveryControlFailure::Conflict);
    }
    if response.status == 0 || response.status >= 500 {
        return Err(DeliveryControlFailure::Uncertain);
    }
    if response.status != 200 {
        return Err(DeliveryControlFailure::Rejected);
    }
    let snapshot = serde_json::from_str::<WebhookFormDeliverySnapshot>(&response.body)
        .map_err(|_| DeliveryControlFailure::Uncertain)?;
    let expected_fields = selected_field_ids.iter().collect::<BTreeSet<_>>();
    if snapshot.funnel_id != funnel_id
        || !snapshot.forms.iter().any(|form| {
            let returned_fields = form.selected_field_ids.iter().collect::<BTreeSet<_>>();
            form.form_id == form_id
                && form.mapping_version_id.as_deref().is_some()
                && returned_fields == expected_fields
                && form.enabled == enabled
        })
    {
        return Err(DeliveryControlFailure::Uncertain);
    }
    Ok(snapshot)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SubmissionDeliveryAction {
    Retry,
    Cancel,
}

#[cfg(feature = "browser")]
pub(super) async fn update_submission_delivery(
    submission_id: &str,
    action: SubmissionDeliveryAction,
    idempotency_key: PreparedIdempotencyKey,
) -> Result<(), DeliveryControlFailure> {
    let action = match action {
        SubmissionDeliveryAction::Retry => "retry",
        SubmissionDeliveryAction::Cancel => "cancel",
    };
    let response = api_request(
        "POST",
        &format!("/api/submissions/{submission_id}/delivery/{action}"),
        None,
        Some(idempotency_key),
    )
    .await
    .map_err(|_| DeliveryControlFailure::Uncertain)?;
    if response.status == 409 {
        return Err(DeliveryControlFailure::Conflict);
    }
    if response.status == 0 || response.status >= 500 {
        return Err(DeliveryControlFailure::Uncertain);
    }
    if response.status != 200 {
        return Err(DeliveryControlFailure::Rejected);
    }
    #[derive(Deserialize)]
    struct ActionResponse {
        submission_id: String,
        accepted: bool,
    }
    let result = serde_json::from_str::<ActionResponse>(&response.body)
        .map_err(|_| DeliveryControlFailure::Uncertain)?;
    if result.submission_id == submission_id && result.accepted {
        Ok(())
    } else {
        Err(DeliveryControlFailure::Uncertain)
    }
}

#[cfg(feature = "browser")]
pub(super) async fn load_service_brief(funnel_id: &str) -> Result<ServiceBriefApiResponse, ()> {
    let response = api_request(
        "GET",
        &format!("/api/funnels/{funnel_id}/service-brief"),
        None,
        None,
    )
    .await?;
    if response.status != 200 {
        return Err(());
    }
    let brief = serde_json::from_str::<ServiceBriefApiResponse>(&response.body).map_err(|_| ())?;
    if brief.funnel_id != funnel_id || brief.format_version != SERVICE_BRIEF_FORMAT_VERSION {
        return Err(());
    }
    Ok(brief)
}

#[cfg(feature = "browser")]
pub(super) async fn save_service_brief(
    funnel_id: &str,
    expected_version: BriefVersion,
    brief: &ServiceBrief,
) -> Result<ServiceBriefApiResponse, ServiceBriefSaveFailure> {
    let body = serde_json::json!({
        "expected_version": expected_version.value(),
        "format_version": SERVICE_BRIEF_FORMAT_VERSION,
        "brief": brief,
    })
    .to_string();
    let response = api_request(
        "PUT",
        &format!("/api/funnels/{funnel_id}/service-brief"),
        Some(body),
        None,
    )
    .await
    .map_err(|_| ServiceBriefSaveFailure::Uncertain)?;
    if response.status == 409 {
        return Err(ServiceBriefSaveFailure::Conflict);
    }
    if response.status == 0 || response.status >= 500 {
        return Err(ServiceBriefSaveFailure::Uncertain);
    }
    if response.status != 200 {
        return Err(ServiceBriefSaveFailure::Rejected);
    }
    let saved = serde_json::from_str::<ServiceBriefApiResponse>(&response.body)
        .map_err(|_| ServiceBriefSaveFailure::Uncertain)?;
    if saved.funnel_id != funnel_id
        || saved.format_version != SERVICE_BRIEF_FORMAT_VERSION
        || saved.version.value() < expected_version.value()
        || saved.brief != *brief
    {
        return Err(ServiceBriefSaveFailure::Uncertain);
    }
    Ok(saved)
}

#[cfg(test)]
mod campaign_registry_tests {
    use super::{
        campaign_registry_response_is_valid, CampaignLinkResponse, CampaignRegistryResponse,
    };
    use eez_domain::interaction_event::MAX_CAMPAIGN_IDS_PER_FUNNEL;

    fn response(campaign_ids: Vec<String>) -> CampaignRegistryResponse {
        CampaignRegistryResponse {
            funnel_id: "funnel-01".to_owned(),
            campaign_links: campaign_ids
                .iter()
                .map(|campaign_id| CampaignLinkResponse {
                    campaign_id: campaign_id.clone(),
                    url: Some(format!(
                        "https://funnels.example/f/lead-gen?campaign_id={campaign_id}"
                    )),
                    relative_path: Some(format!("/f/lead-gen?campaign_id={campaign_id}")),
                })
                .collect(),
            max_campaign_ids: MAX_CAMPAIGN_IDS_PER_FUNNEL,
            campaign_ids,
            published: true,
        }
    }

    #[test]
    fn campaign_snapshot_validation_accepts_exact_sorted_links_and_rejects_bad_state() {
        let valid = response(vec!["spring-launch".to_owned(), "winter-sale".to_owned()]);
        assert!(campaign_registry_response_is_valid(&valid, "funnel-01"));
        assert!(!campaign_registry_response_is_valid(
            &valid,
            "another-funnel"
        ));

        let mut duplicate = response(vec!["spring-launch".to_owned(), "spring-launch".to_owned()]);
        assert!(!campaign_registry_response_is_valid(
            &duplicate,
            "funnel-01"
        ));
        duplicate = response(vec!["invalid campaign".to_owned()]);
        assert!(!campaign_registry_response_is_valid(
            &duplicate,
            "funnel-01"
        ));
        duplicate.campaign_links.clear();
        assert!(!campaign_registry_response_is_valid(
            &duplicate,
            "funnel-01"
        ));
    }

    #[test]
    fn campaign_snapshot_validation_rejects_oversized_registry() {
        let too_many = (0..=MAX_CAMPAIGN_IDS_PER_FUNNEL)
            .map(|index| format!("campaign-{index:03}"))
            .collect();
        assert!(!campaign_registry_response_is_valid(
            &response(too_many),
            "funnel-01"
        ));
    }

    #[test]
    fn campaign_snapshot_validation_rejects_inconsistent_or_insecure_links() {
        let mut value = response(vec!["spring-launch".to_owned()]);
        value.campaign_links[0].url = Some("javascript:alert(1)".to_owned());
        assert!(!campaign_registry_response_is_valid(&value, "funnel-01"));

        let mut value = response(vec!["spring-launch".to_owned()]);
        value.campaign_links[0].url = Some(
            "https://funnels.example?redirect=/f/lead-gen?campaign_id=spring-launch".to_owned(),
        );
        assert!(!campaign_registry_response_is_valid(&value, "funnel-01"));

        let mut value = response(vec!["spring-launch".to_owned()]);
        value.campaign_links[0].relative_path =
            Some("/f/other?campaign_id=spring-launch".to_owned());
        assert!(!campaign_registry_response_is_valid(&value, "funnel-01"));

        let mut value = response(vec!["spring-launch".to_owned()]);
        value.campaign_links[0].url =
            Some("https://custom.example/f/lead-gen?campaign_id=spring-launch".to_owned());
        assert!(campaign_registry_response_is_valid(&value, "funnel-01"));

        let mut value = response(vec!["spring-launch".to_owned()]);
        value.published = false;
        value.campaign_links[0].url = None;
        value.campaign_links[0].relative_path = None;
        assert!(campaign_registry_response_is_valid(&value, "funnel-01"));

        value.campaign_links[0].relative_path =
            Some("/f/lead-gen?campaign_id=spring-launch".to_owned());
        assert!(!campaign_registry_response_is_valid(&value, "funnel-01"));
    }
}
