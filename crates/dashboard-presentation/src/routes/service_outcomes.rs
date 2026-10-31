use dioxus::prelude::*;
use eez_domain::FormCompletionPeriod;

#[cfg(feature = "browser")]
use super::api::load_service_outcome_report;
#[cfg(test)]
use super::api::service_outcome_report_is_valid;
use super::api::ServiceOutcomeReportResponse;
#[cfg(test)]
use super::api::{
    FirstContactDelaySummaryResponse, HistoricalCoverage, ReportCollectionState,
    ReportContextResponse, ServiceOutcomeCountsResponse, ServiceOutcomeStatusCountsResponse,
};
#[cfg(any(feature = "browser", test))]
use super::reports::ReportRequestVersion;
use super::RouteView;

#[derive(Clone, Debug, Default, PartialEq)]
struct ServiceOutcomeReportState {
    #[cfg(any(feature = "browser", test))]
    latest: ReportRequestVersion,
    funnel_id: Option<String>,
    period_days: Option<u8>,
    response: Option<Result<ServiceOutcomeReportResponse, ()>>,
}

impl ServiceOutcomeReportState {
    fn response_for(
        &self,
        funnel_id: &str,
        period: FormCompletionPeriod,
    ) -> Option<Result<ServiceOutcomeReportResponse, ()>> {
        if self.funnel_id.as_deref() == Some(funnel_id) && self.period_days == Some(period.days()) {
            self.response.clone()
        } else {
            None
        }
    }

    #[cfg(any(feature = "browser", test))]
    fn begin(&mut self, funnel_id: String, period: FormCompletionPeriod) -> ReportRequestVersion {
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
        request: ReportRequestVersion,
        response: Result<ServiceOutcomeReportResponse, ()>,
    ) {
        if request.is_current(self.latest) {
            self.response = Some(response);
        }
    }
}

#[component]
pub(super) fn FunnelServiceOutcomes(id: String) -> Element {
    let mut selected_period = use_signal(|| FormCompletionPeriod::Days30);
    let report_state = use_signal(ServiceOutcomeReportState::default);

    #[cfg(feature = "browser")]
    {
        let page_state = report_state;
        let period_state = selected_period;
        use_effect(use_reactive!(|(id)| {
            let funnel_id = id.clone();
            let period = period_state();
            spawn(async move {
                refresh_service_outcome_report(page_state, funnel_id, period).await;
            });
        }));
    }

    let period = selected_period();
    let response = report_state().response_for(&id, period);
    rsx! {
        div { class: "service-outcome-report",
            section { class: "page-heading",
                h1 { "Service outcomes" }
                p { class: "page-description", "Operational records for retained form submissions. Recorded follow-up is not proof of a sale or of contact outside this workspace." }
                p { class: "completion-funnel-id", "Funnel: " code { "{id}" } }
                Link { to: RouteView::FunnelDetail { id: id.clone() }, class: "completion-back-link", "Back to funnel" }
                Link { to: RouteView::Submissions {}, class: "completion-report-link", "Open submissions" }
            }
            fieldset { class: "completion-period-selector",
                legend { "Submission date range (UTC)" }
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
                    section { class: "completion-loading", role: "status", "aria-busy": "true", "Loading service outcome counts…" }
                },
                Some(Err(())) => rsx! {
                    section { class: "completion-error", role: "alert",
                        h2 { "Service outcomes could not be loaded." }
                        p { "Check the dashboard connection and try again." }
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            onclick: move |_| {
                                #[cfg(feature = "browser")]
                                {
                                    let funnel_id = id.clone();
                                    spawn(async move {
                                        refresh_service_outcome_report(report_state, funnel_id, period).await;
                                    });
                                }
                            },
                            "Try again"
                        }
                    }
                },
                Some(Ok(report)) => {
                    let taken_in_charge = recorded_share(
                        report.recorded_outcomes.taken_in_charge,
                        report.history_covered_submissions,
                    );
                    let qualified = recorded_share(
                        report.recorded_outcomes.qualified,
                        report.history_covered_submissions,
                    );
                    let closed = recorded_share(
                        report.recorded_outcomes.closed,
                        report.history_covered_submissions,
                    );
                    let closed_with_reason = recorded_share(
                        report.recorded_outcomes.closed_with_reason,
                        report.history_covered_submissions,
                    );
                    let first_contact_delay = format_first_contact_delay(
                        report.first_contact.average_delay_seconds,
                    );
                    rsx! {
                        section { class: "service-outcome-results", "aria-label": "Service outcome report",
                            p { class: "completion-total",
                                strong { "{report.retained_submissions}" }
                                " retained submissions received during this period"
                            }
                            dl { class: "service-outcome-context",
                                div {
                                    dt { "Calculated" }
                                    dd { time { datetime: "{report.as_of}", "{report.as_of} UTC" } }
                                }
                                div {
                                    dt { "Current retention setting" }
                                    dd {
                                        match report.report_context.effective_retention_days {
                                            Some(days) => rsx! { "{days} days" },
                                            None => rsx! { "Unavailable" },
                                        }
                                    }
                                }
                                div {
                                    dt { "Follow-up history available since" }
                                    dd { time { datetime: "{report.history_started_at}", "{report.history_started_at} UTC" } }
                                }
                            }
                            p { class: "service-outcome-coverage",
                                "{report.history_covered_submissions} of {report.retained_submissions} retained submissions are in the history-covered cohort. Earlier submissions remain in current stock; transitions before the history boundary are unknown and excluded."
                            }
                            if report.retained_submissions == 0 {
                                p { class: "completion-zero-note", "No submissions remain retained in this date range. Expired or erased submissions are excluded." }
                            }
                            section { class: "completion-form-section",
                                h2 { "Current retained stock" }
                                p { "These are the current saved statuses for every retained submission in the selected cohort, including records with incomplete transition history." }
                                div { class: "completion-table-wrap", role: "region", "aria-label": "Current submission status counts", tabindex: "0",
                                    table { class: "completion-table service-outcome-table",
                                        caption { "Current status counts" }
                                        thead { tr { th { scope: "col", "Current status" } th { scope: "col", "Retained submissions" } } }
                                        tbody {
                                            for (label, count) in [
                                                ("New", report.current_stock.new),
                                                ("Contacted", report.current_stock.contacted),
                                                ("Qualified", report.current_stock.qualified),
                                                ("Closed", report.current_stock.closed),
                                            ] {
                                                tr { th { scope: "row", "{label}" } td { class: "completion-number", "{count}" } }
                                            }
                                        }
                                    }
                                }
                            }
                            section { class: "completion-form-section",
                                h2 { "Recorded outcomes" }
                                p { "Each proportion uses the same history-covered, retained submission cohort. Rows can overlap when one submission has several recorded outcomes." }
                                if report.history_covered_submissions == 0 {
                                    p { class: "completion-empty-forms", "No retained submissions fall after the follow-up history boundary, so transition proportions are unavailable." }
                                } else {
                                    div { class: "completion-table-wrap", role: "region", "aria-label": "Recorded service outcome proportions", tabindex: "0",
                                        table { class: "completion-table service-outcome-table",
                                            caption { "Recorded outcomes among retained submissions with available history" }
                                            thead { tr { th { scope: "col", "Recorded outcome" } th { scope: "col", "Count of cohort" } } }
                                            tbody {
                                                tr { th { scope: "row", "Explicit contact recorded" } td { class: "completion-number", "{taken_in_charge}" } }
                                                tr { th { scope: "row", "Qualification recorded" } td { class: "completion-number", "{qualified}" } }
                                                tr { th { scope: "row", "Closed" } td { class: "completion-number", "{closed}" } }
                                                tr { th { scope: "row", "Closed with a reason" } td { class: "completion-number", "{closed_with_reason}" } }
                                            }
                                        }
                                    }
                                }
                                if report.first_contact.submission_count == 0 {
                                    p { class: "service-outcome-delay", "No explicit contacted event is recorded for this retained cohort." }
                                } else {
                                    p { class: "service-outcome-delay",
                                        "Average recorded delay from submission to the first explicit contacted event: {first_contact_delay}, across {report.first_contact.submission_count} retained submissions."
                                    }
                                }
                            }
                            p { class: "service-outcome-note", "Qualification does not imply a recorded contact. A closed status does not imply a sale or payment. Only submissions still retained at calculation time are included." }
                        }
                    }
                }
            }
        }
    }
}

fn recorded_share(count: u64, denominator: u64) -> String {
    if denominator == 0 {
        return "Unavailable (no history-covered submissions)".to_owned();
    }
    let percentage = (count as f64 / denominator as f64) * 100.0;
    format!("{count} / {denominator} ({percentage:.1}%)")
}

fn format_first_contact_delay(seconds: u64) -> String {
    if seconds >= 86_400 {
        format!("{:.1} days", seconds as f64 / 86_400.0)
    } else if seconds >= 3_600 {
        format!("{:.1} hours", seconds as f64 / 3_600.0)
    } else if seconds >= 60 {
        format!("{:.1} minutes", seconds as f64 / 60.0)
    } else {
        format!("{seconds} seconds")
    }
}

#[cfg(feature = "browser")]
async fn refresh_service_outcome_report(
    mut load_state: Signal<ServiceOutcomeReportState>,
    funnel_id: String,
    period: FormCompletionPeriod,
) {
    let request_version = load_state.with_mut(|state| state.begin(funnel_id.clone(), period));
    let result = load_service_outcome_report(&funnel_id, period).await;
    if load_state.try_peek().is_err() {
        return;
    }
    load_state.with_mut(|state| state.complete(request_version, result));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service_outcome_report(funnel_id: &str) -> ServiceOutcomeReportResponse {
        ServiceOutcomeReportResponse {
            funnel_id: funnel_id.to_owned(),
            period_days: 7,
            as_of: "2026-01-07T12:00:00Z".to_owned(),
            history_started_at: "2025-12-31T09:00:00Z".to_owned(),
            report_context: test_report_context(ReportCollectionState::Enabled),
            retained_submissions: 3,
            history_covered_submissions: 2,
            history_uncovered_submissions: 1,
            current_stock: ServiceOutcomeStatusCountsResponse {
                new: 1,
                contacted: 1,
                qualified: 0,
                closed: 1,
            },
            recorded_outcomes: ServiceOutcomeCountsResponse {
                taken_in_charge: 1,
                qualified: 1,
                closed: 1,
                closed_with_reason: 1,
            },
            first_contact: FirstContactDelaySummaryResponse {
                submission_count: 1,
                average_delay_seconds: 180,
            },
        }
    }

    fn test_report_context(collection_state: ReportCollectionState) -> ReportContextResponse {
        ReportContextResponse {
            collection_state,
            effective_retention_days: Some(30),
            historical_coverage: HistoricalCoverage::Unknown,
        }
    }

    #[test]
    fn service_outcome_response_requires_a_balanced_retained_cohort_and_explicit_history() {
        let report = service_outcome_report("funnel-a");
        assert!(service_outcome_report_is_valid(
            &report,
            "funnel-a",
            FormCompletionPeriod::Days7
        ));

        let mut invalid = report.clone();
        invalid.history_uncovered_submissions = 0;
        assert!(!service_outcome_report_is_valid(
            &invalid,
            "funnel-a",
            FormCompletionPeriod::Days7
        ));

        let mut invalid = report.clone();
        invalid.recorded_outcomes.taken_in_charge = 2;
        assert!(!service_outcome_report_is_valid(
            &invalid,
            "funnel-a",
            FormCompletionPeriod::Days7
        ));

        let mut invalid = report;
        invalid.history_started_at = "2026-01-08T00:00:00Z".to_owned();
        assert!(!service_outcome_report_is_valid(
            &invalid,
            "funnel-a",
            FormCompletionPeriod::Days7
        ));
    }

    #[test]
    fn stale_service_outcome_responses_cannot_replace_a_new_funnel_or_period() {
        let mut state = ServiceOutcomeReportState::default();
        let old_request = state.begin("funnel-old".to_owned(), FormCompletionPeriod::Days7);
        let current_request =
            state.begin("funnel-current".to_owned(), FormCompletionPeriod::Days90);
        state.complete(old_request, Ok(service_outcome_report("funnel-old")));
        assert!(state.response.is_none());

        let mut current = service_outcome_report("funnel-current");
        current.period_days = 90;
        state.complete(current_request, Ok(current));
        assert_eq!(state.funnel_id.as_deref(), Some("funnel-current"));
        assert_eq!(state.period_days, Some(90));
        assert!(state
            .response_for("funnel-current", FormCompletionPeriod::Days90)
            .is_some());
        assert!(state
            .response_for("funnel-current", FormCompletionPeriod::Days7)
            .is_none());
    }

    #[test]
    fn outcome_labels_make_the_common_denominator_and_delay_units_explicit() {
        assert_eq!(recorded_share(1, 3), "1 / 3 (33.3%)");
        assert_eq!(
            recorded_share(0, 0),
            "Unavailable (no history-covered submissions)"
        );
        assert_eq!(format_first_contact_delay(59), "59 seconds");
        assert_eq!(format_first_contact_delay(3_600), "1.0 hours");
        assert_eq!(format_first_contact_delay(86_400), "1.0 days");
    }
}
