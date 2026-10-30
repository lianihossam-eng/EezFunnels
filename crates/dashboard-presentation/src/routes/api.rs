#[cfg(feature = "browser")]
use eez_domain::service_brief::SERVICE_BRIEF_FORMAT_VERSION;
#[cfg(any(feature = "browser", test))]
use eez_domain::service_brief::{BriefVersion, ServiceBrief};
#[cfg(feature = "browser")]
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
    pub(super) total_completions: u64,
    pub(super) days: Vec<DailyFormCompletionCountResponse>,
    pub(super) forms: Vec<FormCompletionCountResponse>,
    pub(super) channels: Vec<FormSubmissionChannelCountResponse>,
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
pub(super) struct FormSubmissionChannelCountResponse {
    pub(super) channel: String,
    pub(super) label: String,
    pub(super) completions: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(super) struct InteractionReportResponse {
    pub(super) funnel_id: String,
    pub(super) period_days: u8,
    pub(super) total_events: u64,
    pub(super) days: Vec<DailyInteractionReportDayResponse>,
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
pub(super) struct FunnelCard {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) slug: String,
    pub(super) published_revision_id: Option<String>,
    pub(super) created_at: String,
    pub(super) updated_at: String,
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
    let dates_are_valid = report.days.len() == usize::from(period.days())
        && !report.days.is_empty()
        && !report
            .days
            .iter()
            .enumerate()
            .any(|(index, day)| day.is_partial_today != (index + 1 == report.days.len()))
        && !report
            .days
            .windows(2)
            .any(|days| days[0].utc_day >= days[1].utc_day);
    report.funnel_id == funnel_id
        && report.period_days == period.days()
        && dates_are_valid
        && daily_total == Some(report.total_events)
        && campaign_total == Some(report.total_events)
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
    let daily_total = report
        .days
        .iter()
        .try_fold(0_u64, |total, day| total.checked_add(day.completions));
    let form_total = report
        .forms
        .iter()
        .try_fold(0_u64, |total, form| total.checked_add(form.completions));
    let dates_are_valid = report.days.len() == usize::from(period.days())
        && !report.days.is_empty()
        && !report
            .days
            .iter()
            .enumerate()
            .any(|(index, day)| day.is_partial_today != (index + 1 == report.days.len()))
        && !report
            .days
            .windows(2)
            .any(|days| days[0].utc_day >= days[1].utc_day);
    if report.funnel_id != funnel_id
        || report.period_days != period.days()
        || !dates_are_valid
        || daily_total != Some(report.total_completions)
        || form_total != Some(report.total_completions)
    {
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
pub(super) async fn update_public_form_submission_status(
    submission_id: &str,
    status: FormSubmissionStatus,
) -> Result<(), ()> {
    let response = api_request(
        "POST",
        &format!("/api/submissions/{submission_id}/status"),
        Some(serde_json::json!({ "status": status }).to_string()),
        None,
    )
    .await?;
    if response.status != 200 {
        return Err(());
    }
    let result = serde_json::from_str::<FormSubmissionStatusMutationResponse>(&response.body)
        .map_err(|_| ())?;
    if result.submission_id == submission_id && result.status == status {
        Ok(())
    } else {
        Err(())
    }
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

#[cfg(feature = "browser")]
#[derive(Deserialize)]
struct FormSubmissionStatusMutationResponse {
    submission_id: String,
    status: FormSubmissionStatus,
}
