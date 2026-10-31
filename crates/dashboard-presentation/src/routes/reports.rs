use dioxus::prelude::*;
use eez_domain::{FormCompletionPeriod, InteractionReportPeriod};

#[cfg(test)]
use super::api::interaction_report_is_valid;
#[cfg(feature = "browser")]
use super::api::{load_form_completion_report, load_interaction_report};
#[cfg(test)]
use super::api::{CampaignInteractionCountResponse, DailyInteractionReportDayResponse};
use super::api::{FormCompletionReportResponse, InteractionReportResponse};
use super::RouteView;

#[cfg(any(feature = "browser", test))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CompletionRequestVersion(u64);

#[cfg(any(feature = "browser", test))]
impl CompletionRequestVersion {
    fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    fn is_current(self, latest: Self) -> bool {
        self == latest
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct FormCompletionReportState {
    #[cfg(any(feature = "browser", test))]
    latest: CompletionRequestVersion,
    funnel_id: Option<String>,
    period_days: Option<u8>,
    response: Option<Result<FormCompletionReportResponse, ()>>,
}

impl FormCompletionReportState {
    fn response_for(
        &self,
        funnel_id: &str,
        period: FormCompletionPeriod,
    ) -> Option<Result<FormCompletionReportResponse, ()>> {
        if self.funnel_id.as_deref() == Some(funnel_id) && self.period_days == Some(period.days()) {
            self.response.clone()
        } else {
            None
        }
    }

    #[cfg(any(feature = "browser", test))]
    fn begin(
        &mut self,
        funnel_id: String,
        period: FormCompletionPeriod,
    ) -> CompletionRequestVersion {
        let request = self.latest.next();
        self.latest = request;
        self.funnel_id = Some(funnel_id);
        self.period_days = Some(period.days());
        self.response = None;
        request
    }

    #[cfg(any(feature = "browser", test))]
    fn complete(
        &mut self,
        request: CompletionRequestVersion,
        response: Result<FormCompletionReportResponse, ()>,
    ) {
        if request.is_current(self.latest) {
            self.response = Some(response);
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct InteractionReportState {
    #[cfg(any(feature = "browser", test))]
    latest: CompletionRequestVersion,
    funnel_id: Option<String>,
    period_days: Option<u8>,
    response: Option<Result<InteractionReportResponse, ()>>,
}

impl InteractionReportState {
    fn response_for(
        &self,
        funnel_id: &str,
        period: InteractionReportPeriod,
    ) -> Option<Result<InteractionReportResponse, ()>> {
        if self.funnel_id.as_deref() == Some(funnel_id) && self.period_days == Some(period.days()) {
            self.response.clone()
        } else {
            None
        }
    }

    #[cfg(any(feature = "browser", test))]
    fn begin(
        &mut self,
        funnel_id: String,
        period: InteractionReportPeriod,
    ) -> CompletionRequestVersion {
        let request = self.latest.next();
        self.latest = request;
        self.funnel_id = Some(funnel_id);
        self.period_days = Some(period.days());
        self.response = None;
        request
    }

    #[cfg(any(feature = "browser", test))]
    fn complete(
        &mut self,
        request: CompletionRequestVersion,
        response: Result<InteractionReportResponse, ()>,
    ) {
        if request.is_current(self.latest) {
            self.response = Some(response);
        }
    }
}

#[component]
pub(super) fn FunnelCompletions(id: String) -> Element {
    let mut selected_period = use_signal(|| FormCompletionPeriod::Days30);
    let report_state = use_signal(FormCompletionReportState::default);

    #[cfg(feature = "browser")]
    {
        let page_state = report_state;
        let period_state = selected_period;
        use_effect(use_reactive!(|(id)| {
            let funnel_id = id.clone();
            let period = period_state();
            spawn(async move {
                refresh_form_completion_report(page_state, funnel_id, period).await;
            });
        }));
    }

    let period = selected_period();
    let snapshot = report_state();
    let response = snapshot.response_for(&id, period);
    rsx! {
        div { class: "completion-report",
            section { class: "page-heading",
                h1 { "Form completions" }
                p { class: "page-description", "Successful form submissions still within the configured retention period. These counts do not represent a conversion rate." }
                p { class: "completion-funnel-id", "Funnel: " code { "{id}" } }
                Link { to: RouteView::FunnelDetail { id: id.clone() }, class: "completion-back-link", "Back to funnel" }
                Link { to: RouteView::CampaignLinksPage { id: id.clone() }, class: "completion-report-link", "Manage campaign links" }
            }
            fieldset { class: "completion-period-selector",
                legend { "UTC date range" }
                div { class: "completion-period-options",
                    for (option, label) in [
                        (FormCompletionPeriod::Days7, "7 days"),
                        (FormCompletionPeriod::Days30, "30 days"),
                        (FormCompletionPeriod::Days90, "90 days"),
                    ] {
                        button {
                            class: if period == option { "completion-period-option selected" } else { "completion-period-option" },
                            r#type: "button",
                            "aria-pressed": period == option,
                            onclick: move |_| selected_period.set(option),
                            "{label}"
                        }
                    }
                }
            }
            match response {
                None => rsx! {
                    section { class: "completion-loading", role: "status", "aria-busy": "true", "Loading completion counts…" }
                },
                Some(Err(())) => rsx! {
                    section { class: "completion-error", role: "alert",
                        h2 { "Completions could not be loaded." }
                        p { "Check the dashboard connection and try again." }
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            onclick: move |_| {
                                #[cfg(feature = "browser")]
                                {
                                    let funnel_id = id.clone();
                                    spawn(async move {
                                        refresh_form_completion_report(report_state, funnel_id, period).await;
                                    });
                                }
                            },
                            "Try again"
                        }
                    }
                },
                Some(Ok(report)) => {
                    let max_daily = report.days.iter().map(|day| day.completions).max().unwrap_or(0).max(1);
                    rsx! {
                        section { class: "completion-results", "aria-label": "Form completion report",
                            p { class: "completion-total",
                                strong { "{report.total_completions}" }
                                " successful submissions in the selected period"
                            }
                            p { class: "completion-retention-note", "Only submissions that have not expired are counted. The current UTC date is partial." }
                            div { class: "completion-table-wrap", role: "region", "aria-label": "Daily completion counts", tabindex: "0",
                                table { class: "completion-table",
                                    caption { "Daily completions in UTC" }
                                    thead { tr { th { scope: "col", "Date" } th { scope: "col", "Completions" } th { scope: "col", "Relative volume" } } }
                                    tbody {
                                        for day in report.days.iter() {
                                            {
                                                let width = (day.completions as f64 / max_daily as f64) * 100.0;
                                                rsx! {
                                                    tr {
                                                        th { scope: "row",
                                                            if day.is_partial_today { span { class: "completion-today-label", "Today (partial) · " } }
                                                            time { datetime: "{day.utc_day}", "{day.utc_day}" }
                                                        }
                                                        td { class: "completion-number", "{day.completions}" }
                                                        td { class: "completion-volume",
                                                            span { class: "completion-volume-track", "aria-hidden": "true",
                                                                span { class: "completion-volume-bar", style: "width: {width}%" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            section { class: "completion-form-section",
                                h2 { "By form" }
                                if report.forms.is_empty() {
                                    p { class: "completion-empty-forms", "No retained form submissions in this period." }
                                } else {
                                    div { class: "completion-table-wrap", role: "region", "aria-label": "Completion counts by form", tabindex: "0",
                                        table { class: "completion-table form-completion-table",
                                            caption { "Completions grouped by stable form ID" }
                                            thead { tr { th { scope: "col", "Form" } th { scope: "col", "Completions" } } }
                                            tbody {
                                                for form in report.forms.iter() {
                                                    tr {
                                                        th { scope: "row", code { "Form ID {form.form_id}" } }
                                                        td { class: "completion-number", "{form.completions}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            section { class: "completion-form-section completion-channel-section",
                                h2 { "By campaign channel" }
                                p { class: "completion-channel-note", "Categories come from utm_medium on the page containing the form. They are self-reported and do not verify a visitor’s identity or path." }
                                if report.channels.is_empty() {
                                    p { class: "completion-empty-forms", "No retained form submissions in this period." }
                                } else {
                                    div { class: "completion-table-wrap", role: "region", "aria-label": "Completion counts by campaign channel", tabindex: "0",
                                        table { class: "completion-table form-completion-table completion-channel-table",
                                            caption { "Successful form submissions grouped by page campaign category" }
                                            thead { tr { th { scope: "col", "Channel" } th { scope: "col", "Completions" } } }
                                            tbody {
                                                for channel in report.channels.iter() {
                                                    tr {
                                                        th { scope: "row", "{channel.label}" }
                                                        td { class: "completion-number", "{channel.completions}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn FunnelInteractionReport(id: String) -> Element {
    let mut selected_period = use_signal(|| InteractionReportPeriod::Days30);
    let report_state = use_signal(InteractionReportState::default);

    #[cfg(feature = "browser")]
    {
        let page_state = report_state;
        let period_state = selected_period;
        use_effect(use_reactive!(|(id)| {
            let funnel_id = id.clone();
            let period = period_state();
            spawn(async move {
                refresh_interaction_report(page_state, funnel_id, period).await;
            });
        }));
    }

    let period = selected_period();
    let snapshot = report_state();
    let response = snapshot.response_for(&id, period);
    rsx! {
        div { class: "interaction-report",
            section { class: "page-heading",
                h1 { "Interaction counts" }
                p { class: "page-description", "Counts of consented first-party page and form events that are still retained. Visitors may decline measurement; these counts do not identify people or measure conversions." }
                p { class: "completion-funnel-id", "Funnel: " code { "{id}" } }
                Link { to: RouteView::FunnelDetail { id: id.clone() }, class: "completion-back-link", "Back to funnel" }
            }
            fieldset { class: "completion-period-selector",
                legend { "UTC date range" }
                div { class: "completion-period-options",
                    for (option, label) in [
                        (InteractionReportPeriod::Days7, "7 days"),
                        (InteractionReportPeriod::Days30, "30 days"),
                        (InteractionReportPeriod::Days90, "90 days"),
                    ] {
                        button {
                            class: if period == option { "completion-period-option selected" } else { "completion-period-option" },
                            r#type: "button",
                            "aria-pressed": period == option,
                            onclick: move |_| selected_period.set(option),
                            "{label}"
                        }
                    }
                }
            }
            match response {
                None => rsx! {
                    section { class: "completion-loading", role: "status", "aria-busy": "true", "Loading interaction counts…" }
                },
                Some(Err(())) => rsx! {
                    section { class: "completion-error", role: "alert",
                        h2 { "Interaction counts could not be loaded." }
                        p { "Check the dashboard connection and try again." }
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            onclick: move |_| {
                                #[cfg(feature = "browser")]
                                {
                                    let funnel_id = id.clone();
                                    spawn(async move {
                                        refresh_interaction_report(report_state, funnel_id, period).await;
                                    });
                                }
                            },
                            "Try again"
                        }
                    }
                },
                Some(Ok(report)) => rsx! {
                    section { class: "completion-results", "aria-label": "Interaction count report",
                        p { class: "completion-total",
                            strong { "{report.total_events}" }
                            " retained interaction events in the selected period"
                        }
                        p { class: "completion-retention-note", "Dates use the server receipt time in UTC. Only events that have not expired are counted; the current UTC date is partial." }
                        div { class: "completion-table-wrap", role: "region", "aria-label": "Daily interaction counts", tabindex: "0",
                            table { class: "completion-table interaction-daily-table",
                                caption { "Consent based event counts by UTC date" }
                                thead { tr {
                                    th { scope: "col", "Date" }
                                    th { scope: "col", "Pages viewed" }
                                    th { scope: "col", "Forms viewed" }
                                    th { scope: "col", "Forms started" }
                                } }
                                tbody {
                                    for day in report.days.iter() {
                                        tr {
                                            th { scope: "row",
                                                if day.is_partial_today { span { class: "completion-today-label", "Today (partial) · " } }
                                                time { datetime: "{day.utc_day}", "{day.utc_day}" }
                                            }
                                            td { class: "completion-number", "{day.page_viewed}" }
                                            td { class: "completion-number", "{day.form_viewed}" }
                                            td { class: "completion-number", "{day.form_started}" }
                                        }
                                    }
                                }
                            }
                        }
                        section { class: "completion-form-section interaction-campaign-section",
                            h2 { "By campaign ID" }
                            p { class: "completion-channel-note", "Campaign IDs are exact labels configured by the owner. Unregistered visitor supplied values are discarded and remain unattributed." }
                            if report.campaigns.is_empty() {
                                p { class: "completion-empty-forms", "No retained interaction events in this period." }
                            } else {
                                div { class: "completion-table-wrap", role: "region", "aria-label": "Interaction counts by campaign", tabindex: "0",
                                    table { class: "completion-table interaction-campaign-table",
                                        caption { "Retained events grouped by campaign label" }
                                        thead { tr { th { scope: "col", "Campaign" } th { scope: "col", "Events" } } }
                                        tbody {
                                            for campaign in report.campaigns.iter() {
                                                tr {
                                                    th { scope: "row",
                                                        match campaign.campaign_id.as_deref() {
                                                            Some(campaign_id) => rsx! { code { "{campaign_id}" } },
                                                            None => rsx! { "Unattributed" },
                                                        }
                                                    }
                                                    td { class: "completion-number", "{campaign.events}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[cfg(feature = "browser")]
async fn refresh_form_completion_report(
    mut load_state: Signal<FormCompletionReportState>,
    funnel_id: String,
    period: FormCompletionPeriod,
) {
    let request_version = load_state.with_mut(|state| state.begin(funnel_id.clone(), period));
    let result = load_form_completion_report(&funnel_id, period).await;
    if load_state.try_peek().is_err() {
        return;
    }
    load_state.with_mut(|state| state.complete(request_version, result));
}

#[cfg(feature = "browser")]
async fn refresh_interaction_report(
    mut load_state: Signal<InteractionReportState>,
    funnel_id: String,
    period: InteractionReportPeriod,
) {
    let request_version = load_state.with_mut(|state| state.begin(funnel_id.clone(), period));
    let result = load_interaction_report(&funnel_id, period).await;
    if load_state.try_peek().is_err() {
        return;
    }
    load_state.with_mut(|state| state.complete(request_version, result));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_completion_responses_cannot_replace_a_new_funnel_or_period() {
        let mut state = FormCompletionReportState::default();
        let old_request = state.begin("funnel-old".to_owned(), FormCompletionPeriod::Days7);
        let current_request =
            state.begin("funnel-current".to_owned(), FormCompletionPeriod::Days90);
        state.complete(
            old_request,
            Ok(FormCompletionReportResponse {
                funnel_id: "funnel-old".to_owned(),
                period_days: 7,
                total_completions: 3,
                days: Vec::new(),
                forms: Vec::new(),
                channels: Vec::new(),
            }),
        );
        assert!(state.response.is_none());

        state.complete(
            current_request,
            Ok(FormCompletionReportResponse {
                funnel_id: "funnel-current".to_owned(),
                period_days: 90,
                total_completions: 0,
                days: Vec::new(),
                forms: Vec::new(),
                channels: Vec::new(),
            }),
        );
        assert_eq!(state.funnel_id.as_deref(), Some("funnel-current"));
        assert_eq!(state.period_days, Some(90));
        assert_eq!(
            state.response.as_ref().unwrap().as_ref().unwrap().funnel_id,
            "funnel-current"
        );
    }

    #[test]
    fn completion_results_are_hidden_until_the_current_funnel_and_period_match() {
        let state = FormCompletionReportState {
            funnel_id: Some("funnel-old".to_owned()),
            period_days: Some(7),
            response: Some(Ok(FormCompletionReportResponse {
                funnel_id: "funnel-old".to_owned(),
                period_days: 7,
                total_completions: 3,
                days: Vec::new(),
                forms: Vec::new(),
                channels: Vec::new(),
            })),
            ..FormCompletionReportState::default()
        };

        assert!(state
            .response_for("funnel-old", FormCompletionPeriod::Days7)
            .is_some());
        assert!(state
            .response_for("funnel-current", FormCompletionPeriod::Days7)
            .is_none());
        assert!(state
            .response_for("funnel-old", FormCompletionPeriod::Days30)
            .is_none());
    }

    fn interaction_report(funnel_id: &str, period_days: u8) -> InteractionReportResponse {
        let days = (1..=period_days)
            .map(|day| DailyInteractionReportDayResponse {
                utc_day: format!("2026-10-{day:02}"),
                page_viewed: if day == period_days { 1 } else { 0 },
                form_viewed: 0,
                form_started: 0,
                is_partial_today: day == period_days,
            })
            .collect();
        InteractionReportResponse {
            funnel_id: funnel_id.to_owned(),
            period_days,
            total_events: 1,
            days,
            campaigns: vec![CampaignInteractionCountResponse {
                campaign_id: None,
                events: 1,
            }],
        }
    }

    #[test]
    fn interaction_report_response_is_bounded_and_matches_aggregate_totals() {
        let report = interaction_report("funnel-a", 7);
        assert!(interaction_report_is_valid(
            &report,
            "funnel-a",
            InteractionReportPeriod::Days7
        ));
        assert!(!interaction_report_is_valid(
            &report,
            "another-funnel",
            InteractionReportPeriod::Days7
        ));

        let mut invalid = report.clone();
        invalid.campaigns[0].events = 2;
        assert!(!interaction_report_is_valid(
            &invalid,
            "funnel-a",
            InteractionReportPeriod::Days7
        ));
        let mut duplicate = report;
        duplicate.campaigns.push(CampaignInteractionCountResponse {
            campaign_id: None,
            events: 0,
        });
        assert!(!interaction_report_is_valid(
            &duplicate,
            "funnel-a",
            InteractionReportPeriod::Days7
        ));
    }

    #[test]
    fn stale_interaction_report_responses_cannot_replace_a_new_funnel_or_period() {
        let mut state = InteractionReportState::default();
        let old_request = state.begin("funnel-old".to_owned(), InteractionReportPeriod::Days7);
        let current_request =
            state.begin("funnel-current".to_owned(), InteractionReportPeriod::Days90);
        state.complete(old_request, Ok(interaction_report("funnel-old", 7)));
        assert!(state.response.is_none());

        state.complete(
            current_request,
            Ok(interaction_report("funnel-current", 90)),
        );
        assert_eq!(state.funnel_id.as_deref(), Some("funnel-current"));
        assert_eq!(state.period_days, Some(90));
        assert_eq!(
            state.response.as_ref().unwrap().as_ref().unwrap().funnel_id,
            "funnel-current"
        );
    }
}
