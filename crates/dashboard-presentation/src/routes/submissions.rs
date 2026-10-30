use dioxus::prelude::*;
use eez_domain::FormSubmissionStatus;
use serde_json::Value;

use super::api::SubmissionDeliveryAction;
#[cfg(feature = "browser")]
use super::api::{
    delete_public_form_submission, load_submission_page, update_public_form_submission_status,
};
#[cfg(feature = "browser")]
use super::api::{update_submission_delivery, DeliveryControlFailure};
use super::api::{FormSubmissionListItem, FormSubmissionListResponse};
use super::delivery::{webhook_status_copy, WebhookDeliveryPanel};
#[cfg(feature = "browser")]
use super::funnels::refresh_funnel_list;
use super::funnels::FunnelListLoad;
use super::idempotency::IdempotencyKeyState;
#[cfg(feature = "browser")]
use super::idempotency::{clear_idempotency_storage, prepare_idempotency_key};
use super::RouteView;

#[cfg(any(feature = "browser", test))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SubmissionRequestVersion(u64);

#[cfg(any(feature = "browser", test))]
impl SubmissionRequestVersion {
    fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    fn is_current(self, latest: Self) -> bool {
        self == latest
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct SubmissionInboxState {
    #[cfg(any(feature = "browser", test))]
    latest: SubmissionRequestVersion,
    status: Option<FormSubmissionStatus>,
    funnel_id: Option<String>,
    response: Option<Result<FormSubmissionListResponse, ()>>,
    loading_more: bool,
    load_more_error: bool,
    delete_pending: Option<String>,
    delete_error: bool,
    delete_error_id: Option<String>,
}

impl SubmissionInboxState {
    #[cfg(any(feature = "browser", test))]
    fn begin(&mut self) -> SubmissionRequestVersion {
        let request = self.latest.next();
        self.latest = request;
        self.response = None;
        self.loading_more = false;
        self.load_more_error = false;
        request
    }

    #[cfg(any(feature = "browser", test))]
    fn set_status(&mut self, status: Option<FormSubmissionStatus>) -> bool {
        if self.status == status {
            return false;
        }
        self.latest = self.latest.next();
        self.status = status;
        self.response = None;
        self.loading_more = false;
        self.load_more_error = false;
        true
    }

    #[cfg(any(feature = "browser", test))]
    fn set_funnel_id(&mut self, funnel_id: Option<String>) -> bool {
        if self.funnel_id == funnel_id {
            return false;
        }
        self.latest = self.latest.next();
        self.funnel_id = funnel_id;
        self.response = None;
        self.loading_more = false;
        self.load_more_error = false;
        true
    }

    #[cfg(any(feature = "browser", test))]
    fn complete(
        &mut self,
        request: SubmissionRequestVersion,
        mut response: Result<FormSubmissionListResponse, ()>,
    ) {
        if request.is_current(self.latest) {
            if let Ok(response) = response.as_mut() {
                response.items.retain(|item| {
                    self.delete_pending.as_deref() != Some(item.submission_id.as_str())
                        && self.delete_error_id.as_deref() != Some(item.submission_id.as_str())
                });
            }
            self.response = Some(response);
            self.loading_more = false;
            self.load_more_error = false;
        }
    }

    #[cfg(any(feature = "browser", test))]
    fn begin_more(
        &mut self,
    ) -> Option<(
        SubmissionRequestVersion,
        String,
        Option<FormSubmissionStatus>,
        Option<String>,
    )> {
        if self.loading_more {
            return None;
        }
        let cursor = self.response.as_ref()?.as_ref().ok()?.next_cursor.clone()?;
        let request = self.latest.next();
        self.latest = request;
        self.loading_more = true;
        Some((request, cursor, self.status, self.funnel_id.clone()))
    }

    #[cfg(any(feature = "browser", test))]
    fn complete_more(
        &mut self,
        request: SubmissionRequestVersion,
        response: Result<FormSubmissionListResponse, ()>,
    ) {
        if !request.is_current(self.latest) {
            return;
        }
        self.loading_more = false;
        match response {
            Ok(mut next_page) => {
                next_page.items.retain(|item| {
                    self.delete_pending.as_deref() != Some(item.submission_id.as_str())
                        && self.delete_error_id.as_deref() != Some(item.submission_id.as_str())
                });
                if let Some(Ok(current_page)) = self.response.as_mut() {
                    current_page.items.append(&mut next_page.items);
                    current_page.next_cursor = next_page.next_cursor;
                    self.load_more_error = false;
                }
            }
            Err(()) => self.load_more_error = true,
        }
    }

    #[cfg(any(feature = "browser", test))]
    fn begin_delete(&mut self, submission_id: &str) -> bool {
        if self.delete_pending.is_some() || self.delete_error {
            return false;
        }
        self.latest = self.latest.next();
        if let Some(Ok(response)) = self.response.as_mut() {
            response
                .items
                .retain(|item| item.submission_id != submission_id);
        }
        self.loading_more = false;
        self.load_more_error = false;
        self.delete_pending = Some(submission_id.to_owned());
        self.delete_error = false;
        self.delete_error_id = None;
        true
    }

    #[cfg(any(feature = "browser", test))]
    fn finish_delete(&mut self) {
        self.delete_pending = None;
        self.delete_error = false;
        self.delete_error_id = None;
    }

    #[cfg(any(feature = "browser", test))]
    fn fail_delete(&mut self, submission_id: &str) {
        self.latest = self.latest.next();
        self.loading_more = false;
        self.load_more_error = false;
        self.delete_pending = None;
        self.delete_error = true;
        self.delete_error_id = Some(submission_id.to_owned());
    }

    #[cfg(any(feature = "browser", test))]
    fn clear_delete_error(&mut self) {
        self.delete_error = false;
        self.delete_error_id = None;
    }
}

#[component]
pub(super) fn Submissions() -> Element {
    let load_state = use_signal(SubmissionInboxState::default);
    let funnel_list_state = use_signal(FunnelListLoad::default);
    #[cfg(feature = "browser")]
    let mut delete_state = load_state;
    let on_delete = use_callback(move |submission_id: String| {
        #[cfg(feature = "browser")]
        {
            if !delete_state.with_mut(|state| state.begin_delete(&submission_id)) {
                return;
            }
            let mut request_state = delete_state;
            spawn(async move {
                let deleted = delete_public_form_submission(&submission_id).await.is_ok();
                if request_state.try_peek().is_err() {
                    return;
                }
                if deleted {
                    request_state.with_mut(SubmissionInboxState::finish_delete);
                    refresh_submission_list(request_state).await;
                } else {
                    request_state.with_mut(|state| state.fail_delete(&submission_id));
                }
            });
        }
        #[cfg(not(feature = "browser"))]
        let _ = submission_id;
    });

    #[cfg(feature = "browser")]
    {
        let page_state = load_state;
        use_effect(move || {
            spawn(async move {
                refresh_submission_list(page_state).await;
            });
        });
        let list_state = funnel_list_state;
        use_effect(move || {
            spawn(async move {
                refresh_funnel_list(list_state).await;
            });
        });
    }

    let snapshot = load_state();
    let funnel_snapshot = funnel_list_state();
    rsx! {
        section { class: "page-heading",
            h1 { "Submissions" }
            p { class: "page-description", "Review form responses alongside the consent and retention record captured at submission time." }
        }
        WebhookDeliveryPanel {}
        div { class: "submission-toolbar",
            div { class: "submission-filter-group",
                label { r#for: "submission-status-filter", "Filter submissions by status" }
                select {
                    id: "submission-status-filter",
                    "aria-label": "Filter submissions by status",
                    value: snapshot.status.map(FormSubmissionStatus::as_str).unwrap_or("all"),
                    onchange: move |event| {
                        let selected = match event.value().as_str() {
                            "all" => Some(None),
                            "new" => Some(Some(FormSubmissionStatus::New)),
                            "contacted" => Some(Some(FormSubmissionStatus::Contacted)),
                            "qualified" => Some(Some(FormSubmissionStatus::Qualified)),
                            "closed" => Some(Some(FormSubmissionStatus::Closed)),
                            _ => None,
                        };
                        #[cfg(feature = "browser")]
                        if let Some(selected) = selected {
                            let mut load_state = load_state;
                            load_state.with_mut(|state| state.set_status(selected));
                            spawn(async move { refresh_submission_list(load_state).await; });
                        }
                        #[cfg(not(feature = "browser"))]
                        let _ = selected;
                    },
                    option { value: "all", "All statuses" }
                    for status in FormSubmissionStatus::ALL {
                        option { value: status.as_str(), "{status.label()}" }
                    }
                }
            }
            div { class: "submission-filter-group",
                label { r#for: "submission-funnel-filter", "Filter submissions by funnel" }
                select {
                    id: "submission-funnel-filter",
                    "aria-label": "Filter submissions by funnel",
                    value: snapshot.funnel_id.as_deref().unwrap_or("all"),
                    disabled: !matches!(funnel_snapshot.response.as_ref(), Some(Ok(_))),
                    onchange: move |event| {
                        let selected = match event.value().as_str() {
                            "all" => Some(None),
                            value if uuid::Uuid::parse_str(value).is_ok() => Some(Some(value.to_owned())),
                            _ => None,
                        };
                        #[cfg(feature = "browser")]
                        if let Some(selected) = selected {
                            let mut load_state = load_state;
                            load_state.with_mut(|state| state.set_funnel_id(selected));
                            spawn(async move { refresh_submission_list(load_state).await; });
                        }
                        #[cfg(not(feature = "browser"))]
                        let _ = selected;
                    },
                    option { value: "all", "All funnels" }
                    match funnel_snapshot.response.as_ref() {
                        Some(Ok(funnels)) => rsx! {
                            for funnel in funnels.iter() {
                                option { value: "{funnel.id}", "{funnel.name} — /{funnel.slug}" }
                            }
                        },
                        Some(Err(())) => rsx! { option { value: "unavailable", "Funnel filters unavailable" } },
                        None => rsx! { option { value: "loading", "Loading funnels…" } },
                    }
                }
            }
        }
        if matches!(funnel_snapshot.response.as_ref(), Some(Err(()))) {
            div { class: "submission-filter-error", role: "status",
                p { "Funnel filters could not be loaded. The inbox is showing submissions from all funnels." }
                button {
                    class: "secondary-button",
                    r#type: "button",
                    onclick: move |_| {
                        #[cfg(feature = "browser")]
                        spawn(async move { refresh_funnel_list(funnel_list_state).await; });
                    },
                    "Retry funnel filters"
                }
            }
        }
        if snapshot.delete_pending.is_some() {
            p { class: "submission-delete-feedback", role: "status", "The submission was removed from this view. Finishing the deletion…" }
        }
        if snapshot.delete_error {
            div { class: "submission-delete-error", role: "alert",
                p { "The server could not confirm the deletion. The submission was removed from this view; reload to check the stored result." }
                button {
                    class: "secondary-button",
                    r#type: "button",
                    onclick: move |_| {
                        #[cfg(feature = "browser")]
                        {
                                let mut load_state = load_state;
                            load_state.with_mut(SubmissionInboxState::clear_delete_error);
                            spawn(async move { refresh_submission_list(load_state).await; });
                        }
                    },
                    "Reload inbox"
                }
            }
        }
        match snapshot.response {
            None => rsx! {
                section { class: "submission-loading", "aria-label": "Loading submissions", role: "status", "aria-busy": "true",
                    span { class: "submission-loading-line" }
                    span { class: "submission-loading-line" }
                    span { class: "submission-loading-line" }
                }
            },
            Some(Err(())) => rsx! {
                section { class: "empty-state submission-empty", "aria-label": "Submissions unavailable", role: "status",
                    h2 { "Submissions could not be loaded." }
                    p { "Check the dashboard connection and try again." }
                    button {
                        class: "secondary-button",
                        r#type: "button",
                        onclick: move |_| {
                            #[cfg(feature = "browser")]
                            spawn(async move { refresh_submission_list(load_state).await; });
                        },
                        "Try again"
                    }
                }
            },
            Some(Ok(response)) if response.items.is_empty() && (snapshot.status.is_some() || snapshot.funnel_id.is_some()) => rsx! {
                section { class: "empty-state submission-empty", "aria-label": "No matching submissions",
                    if snapshot.status.is_some() && snapshot.funnel_id.is_some() {
                        h2 { "No matching submissions" }
                        p { "No retained submissions currently match the selected funnel and status." }
                    } else if snapshot.funnel_id.is_some() {
                        h2 { "No submissions for this funnel" }
                        p { "No retained submissions currently match this funnel." }
                    } else if let Some(status) = snapshot.status {
                        h2 { "No {status.label().to_lowercase()} submissions" }
                        p { "No retained submissions currently match this status." }
                    }
                }
            },
            Some(Ok(response)) if response.items.is_empty() => rsx! {
                section { class: "empty-state submission-empty", "aria-label": "No form submissions",
                    h2 { "No form submissions yet" }
                    p { "Responses from published funnels will appear here while they remain within the configured retention period." }
                    Link { to: RouteView::Funnels {}, class: "secondary-button", "View funnels" }
                }
            },
            Some(Ok(response)) => rsx! {
                section { class: "submission-list", "aria-label": "Form submissions",
                    p { class: "submission-retention-note", "Only submissions within the configured retention period are shown." }
                    for item in response.items.iter() {
                        SubmissionEntry { key: "{item.submission_id}", item: item.clone(), load_state, delete_pending: snapshot.delete_pending.is_some() || snapshot.delete_error, on_delete }
                    }
                    if snapshot.load_more_error {
                        p { class: "editor-save-error", role: "alert", "Older submissions could not be loaded. Your current results are still available." }
                    }
                    if response.next_cursor.is_some() {
                        button {
                            class: "secondary-button submission-more-button",
                            r#type: "button",
                            disabled: snapshot.loading_more,
                            onclick: move |_| {
                                #[cfg(feature = "browser")]
                                spawn(async move { load_more_submissions(load_state).await; });
                            },
                            if snapshot.loading_more { "Loading older submissions…" } else if snapshot.load_more_error { "Retry loading older submissions" } else { "Load older submissions" }
                        }
                    } else {
                        p { class: "submission-list-end", "All retained submissions are shown." }
                    }
                }
            },
        }
    }
}

#[component]
fn SubmissionEntry(
    item: FormSubmissionListItem,
    load_state: Signal<SubmissionInboxState>,
    delete_pending: bool,
    on_delete: EventHandler<String>,
) -> Element {
    #[allow(unused_mut)]
    let mut current_status = use_signal(|| item.status);
    let mut status_pending = use_signal(|| false);
    let mut status_error = use_signal(|| false);
    let delivery_action_pending = use_signal(|| false);
    let delivery_action_error = use_signal(|| None::<String>);
    let delivery_action_key = use_signal(|| None::<IdempotencyKeyState>);
    let status_control_id = format!("submission-status-{}", item.submission_id);
    let submission_id = item.submission_id.clone();
    let delete_submission_id = submission_id.clone();
    let retry_submission_id = submission_id.clone();
    let cancel_submission_id = submission_id.clone();
    let status_accessible_name = format!(
        "Follow-up status for {} submitted {}",
        item.form_purpose, item.submitted_at_label
    );
    rsx! {
        details { class: "submission-entry",
            summary {
                span { class: "submission-entry-purpose", "{item.form_purpose}" }
                span { class: "submission-entry-funnel", "{item.funnel_name}" }
                time { class: "submission-entry-time", datetime: "{item.submitted_at_datetime}", "{item.submitted_at_label}" }
                span { class: if item.marketing_opt_in { "submission-consent-state opted-in" } else { "submission-consent-state" },
                    if item.marketing_opt_in { "Marketing opt-in granted" } else { "Marketing opt-in not granted" }
                }
                span { class: "submission-status-badge", "{current_status().label()}" }
            }
            div { class: "submission-entry-content",
                div { class: "submission-entry-actions",
                    a {
                        class: "secondary-button",
                        href: "/api/submissions/{submission_id}/export",
                        "Export JSON"
                    }
                    details { class: "submission-delete-confirmation",
                        summary { "Delete submission" }
                        p { "This permanently removes this response after its recovery record is saved. The action cannot be undone." }
                        button {
                            class: "danger-button",
                            r#type: "button",
                            disabled: delete_pending,
                            onclick: move |_| {
                                on_delete.call(delete_submission_id.clone());
                            },
                            "Delete permanently"
                        }
                    }
                }
                div { class: "submission-status-control",
                    label { r#for: "{status_control_id}", "Follow-up status" }
                    select {
                        id: "{status_control_id}",
                        "aria-label": "{status_accessible_name}",
                        value: current_status().as_str(),
                        disabled: status_pending(),
                        onchange: move |event| {
                            let Some(next_status) = FormSubmissionStatus::from_wire_value(&event.value()) else {
                                return;
                            };
                            if status_pending() || next_status == current_status() {
                                return;
                            }
                            status_pending.set(true);
                            status_error.set(false);
                            let submission_id = submission_id.clone();
                            #[cfg(feature = "browser")]
                            spawn(async move {
                                if update_public_form_submission_status(&submission_id, next_status).await.is_ok() {
                                    current_status.set(next_status);
                                    refresh_submission_list(load_state).await;
                                } else {
                                    status_error.set(true);
                                }
                                status_pending.set(false);
                            });
                            #[cfg(not(feature = "browser"))]
                            {
                                let _ = (submission_id, next_status);
                                status_pending.set(false);
                            }
                        },
                        option { value: "new", "New" }
                        option { value: "contacted", "Contacted" }
                        option { value: "qualified", "Qualified" }
                        option { value: "closed", "Closed" }
                    }
                    if status_pending() {
                        span { class: "submission-status-feedback", role: "status", "Saving status…" }
                    } else if status_error() {
                        span { class: "submission-status-error", role: "alert", "Status could not be saved. Choose a status to retry." }
                    }
                }
                section { class: "submission-webhook-delivery", "aria-label": "Webhook delivery status",
                    h2 { class: "submission-section-title", "Webhook delivery" }
                    p { class: "submission-delivery-status", "{webhook_status_copy(&item.delivery.status).label}" }
                    p { class: "submission-delivery-description", "{webhook_status_copy(&item.delivery.status).description}" }
                    if item.delivery.attempt_count > 0 {
                        dl { class: "submission-delivery-details",
                            div { dt { "Attempts" } dd { "{item.delivery.attempt_count} of 5" } }
                            if let Some(last_attempt) = item.delivery.last_attempt_at_label.as_ref() {
                                div { dt { "Last attempt" } dd { "{last_attempt}" } }
                            }
                            if let Some(next_attempt) = item.delivery.next_attempt_at_label.as_ref() {
                                div { dt { "Next attempt" } dd { "{next_attempt}" } }
                            }
                            if let Some(status) = item.delivery.last_http_status {
                                div { dt { "Receiver response" } dd { "HTTP {status}" } }
                            }
                            if let Some(outcome) = item.delivery.last_outcome_code.as_deref() {
                                div { dt { "Latest result" } dd { "{delivery_outcome_label(outcome)}" } }
                            }
                        }
                    }
                    if let Some(message) = delivery_action_error() {
                        p { class: "submission-delivery-error", role: "alert", "{message}" }
                    }
                    if delivery_action_pending() {
                        p { class: "submission-delivery-feedback", role: "status", "Updating delivery…" }
                    }
                    div { class: "submission-delivery-actions",
                        if item.delivery.can_retry {
                            button {
                                class: "secondary-button",
                                r#type: "button",
                                disabled: !cfg!(feature = "browser") || delivery_action_pending(),
                                onclick: move |_| start_delivery_action(
                                    retry_submission_id.clone(),
                                    SubmissionDeliveryAction::Retry,
                                    delivery_action_pending,
                                    delivery_action_error,
                                    delivery_action_key,
                                    load_state,
                                ),
                                "Retry delivery"
                            }
                        }
                        if item.delivery.can_cancel {
                            button {
                                class: "secondary-button",
                                r#type: "button",
                                disabled: !cfg!(feature = "browser") || delivery_action_pending(),
                                onclick: move |_| start_delivery_action(
                                    cancel_submission_id.clone(),
                                    SubmissionDeliveryAction::Cancel,
                                    delivery_action_pending,
                                    delivery_action_error,
                                    delivery_action_key,
                                    load_state,
                                ),
                                "Cancel future attempts"
                            }
                        }
                    }
                }
                dl { class: "submission-context-list",
                    div { dt { "Page" } dd { "{item.page_title}" } }
                    div { dt { "Form" } dd { "{item.form_purpose}" } }
                }
                h2 { class: "submission-section-title", "Response" }
                if item.fields.is_empty() {
                    p { class: "submission-empty-fields", "This response contains no field values." }
                } else {
                    dl { class: "submission-field-list",
                        for field in item.fields.iter() {
                            div { dt { "{field.label}" } dd { "{submission_value_text(&field.value)}" } }
                        }
                    }
                }
                h2 { class: "submission-section-title", "Consent record" }
                dl { class: "submission-consent-list",
                    div { dt { "Request consent" } dd {
                        if item.request_consent_acknowledged { "Acknowledged" } else { "Not acknowledged" }
                    } }
                    div { dt { "Copy version" } dd { "{item.request_consent_copy_version}" } }
                    div { dt { "Recorded wording" } dd { "{item.request_consent_copy}" } }
                    div { dt { "Recorded at" } dd { "{item.request_consent_recorded_at_label}" } }
                    if let Some(version) = item.marketing_opt_in_copy_version.as_ref() {
                        div { dt { "Marketing copy version" } dd { "{version}" } }
                    }
                    if let Some(copy) = item.marketing_opt_in_copy.as_ref() {
                        div { dt { "Marketing wording" } dd { "{copy}" } }
                    }
                    if let Some(recorded_at) = item.marketing_opt_in_recorded_at_label.as_ref() {
                        div { dt { "Marketing choice recorded at" } dd { "{recorded_at}" } }
                    }
                    div { dt { "Retention" } dd { "Available until {item.expires_at_label}" } }
                }
            }
        }
    }
}

fn submission_value_text(value: &Value) -> String {
    match value {
        Value::Bool(value) => if *value { "Yes" } else { "No" }.to_owned(),
        Value::String(value) => value.clone(),
        _ => String::new(),
    }
}

fn delivery_outcome_label(code: &str) -> &'static str {
    match code {
        "receiver_accepted" => "Receiver accepted the event",
        "transport_retryable" => {
            "Temporary receiver or network issue; another attempt is scheduled"
        }
        "redirect_refused" => "Receiver redirects are not followed",
        "receiver_rejected" => "Receiver rejected the event",
        "invalid_response" => "Receiver returned an invalid response",
        "invalid_destination" => "Destination is no longer valid",
        "incompatible_mapping" => "Form fields need revalidation",
        "invalid_submission" => "Submission did not match its saved revision",
        "signing_failed" => "Worker signing configuration is unavailable",
        "retry_exhausted" => "Delivery attempt limit reached",
        "delivery_window_elapsed" => "Delivery time window elapsed",
        "cancelled_by_owner" => "Cancelled by the owner",
        _ => "Delivery result recorded",
    }
}

#[cfg(feature = "browser")]
async fn refresh_submission_list(mut load_state: Signal<SubmissionInboxState>) {
    let (request_version, status, funnel_id) = load_state.with_mut(|state| {
        let request_version = state.begin();
        (request_version, state.status, state.funnel_id.clone())
    });
    let result = load_submission_page(None, status, funnel_id.as_deref()).await;
    if load_state.try_peek().is_err() {
        return;
    }
    load_state.with_mut(|state| state.complete(request_version, result));
}

#[cfg(feature = "browser")]
async fn load_more_submissions(mut load_state: Signal<SubmissionInboxState>) {
    let Some((request_version, cursor, status, funnel_id)) =
        load_state.with_mut(SubmissionInboxState::begin_more)
    else {
        return;
    };
    let result = load_submission_page(Some(&cursor), status, funnel_id.as_deref()).await;
    if load_state.try_peek().is_err() {
        return;
    }
    load_state.with_mut(|state| state.complete_more(request_version, result));
}

#[cfg(feature = "browser")]
fn start_delivery_action(
    submission_id: String,
    action: SubmissionDeliveryAction,
    mut pending: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut idempotency_key: Signal<Option<IdempotencyKeyState>>,
    load_state: Signal<SubmissionInboxState>,
) {
    if pending() {
        return;
    }
    let action_path = match action {
        SubmissionDeliveryAction::Retry => "retry",
        SubmissionDeliveryAction::Cancel => "cancel",
    };
    let path = format!("/api/submissions/{submission_id}/delivery/{action_path}");
    let key = match prepare_idempotency_key("POST", &path, None, idempotency_key()) {
        Ok(key) => key,
        Err(()) => {
            error.set(Some(
                "This browser could not prepare a retry-safe request.".to_owned(),
            ));
            return;
        }
    };
    idempotency_key.set(Some(key.clone()));
    pending.set(true);
    error.set(None);
    spawn(async move {
        match update_submission_delivery(&submission_id, action, key.clone()).await {
            Ok(()) => {
                let _ = clear_idempotency_storage(&key);
                idempotency_key.set(None);
                refresh_submission_list(load_state).await;
            }
            Err(DeliveryControlFailure::Conflict) => {
                error.set(Some("Delivery state changed before this action completed. The inbox has been refreshed.".to_owned()));
                refresh_submission_list(load_state).await;
            }
            Err(DeliveryControlFailure::Uncertain) => {
                error.set(Some(
                    "The server may have applied this action. Retry it to safely confirm."
                        .to_owned(),
                ));
            }
            Err(DeliveryControlFailure::Rejected) => {
                error.set(Some(
                    "This delivery action is no longer available. The inbox has been refreshed."
                        .to_owned(),
                ));
                refresh_submission_list(load_state).await;
            }
        }
        pending.set(false);
    });
}

#[cfg(not(feature = "browser"))]
fn start_delivery_action(
    _submission_id: String,
    _action: SubmissionDeliveryAction,
    _pending: Signal<bool>,
    _error: Signal<Option<String>>,
    _idempotency_key: Signal<Option<IdempotencyKeyState>>,
    _load_state: Signal<SubmissionInboxState>,
) {
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_outcome_labels_match_persisted_worker_codes() {
        assert_eq!(
            delivery_outcome_label("redirect_refused"),
            "Receiver redirects are not followed"
        );
        assert_eq!(
            delivery_outcome_label("transport_retryable"),
            "Temporary receiver or network issue; another attempt is scheduled"
        );
        assert_eq!(
            delivery_outcome_label("receiver_rejected"),
            "Receiver rejected the event"
        );
        assert_eq!(
            delivery_outcome_label("invalid_response"),
            "Receiver returned an invalid response"
        );
    }

    fn submission_item(submission_id: &str) -> FormSubmissionListItem {
        FormSubmissionListItem {
            submission_id: submission_id.to_owned(),
            status: FormSubmissionStatus::New,
            funnel_name: "Lead capture".to_owned(),
            page_title: "Home".to_owned(),
            form_purpose: "Contact".to_owned(),
            fields: Vec::new(),
            request_consent_acknowledged: true,
            request_consent_copy_version: "request-v1".to_owned(),
            request_consent_copy: "I agree to be contacted.".to_owned(),
            request_consent_recorded_at_label: "2026-10-17 12:00 UTC".to_owned(),
            marketing_opt_in: false,
            marketing_opt_in_copy_version: None,
            marketing_opt_in_copy: None,
            marketing_opt_in_recorded_at_label: None,
            submitted_at_label: "2026-10-17 12:00 UTC".to_owned(),
            submitted_at_datetime: "2026-10-17T12:00:00Z".to_owned(),
            expires_at_label: "2027-01-15 12:00 UTC".to_owned(),
            delivery: super::super::api::FormSubmissionDelivery {
                status: "inbox_only".to_owned(),
                attempt_count: 0,
                last_attempt_at_label: None,
                next_attempt_at_label: None,
                last_http_status: None,
                last_outcome_code: None,
                can_retry: false,
                can_cancel: false,
            },
        }
    }

    fn submission_page(ids: &[&str], next_cursor: Option<&str>) -> FormSubmissionListResponse {
        FormSubmissionListResponse {
            items: ids.iter().map(|id| submission_item(id)).collect(),
            next_cursor: next_cursor.map(str::to_owned),
        }
    }

    #[test]
    fn submission_pages_append_and_advance_the_cursor() {
        let mut state = SubmissionInboxState::default();
        let initial_load = state.begin();
        state.complete(
            initial_load,
            Ok(submission_page(&["newest"], Some("cursor-one"))),
        );

        let (older_load, cursor, status, funnel_id) =
            state.begin_more().expect("next page can load");
        assert_eq!(cursor, "cursor-one");
        assert_eq!(status, None);
        assert_eq!(funnel_id, None);
        state.complete_more(
            older_load,
            Ok(submission_page(&["older"], Some("cursor-two"))),
        );

        let page = state.response.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(
            page.items
                .iter()
                .map(|item| item.submission_id.as_str())
                .collect::<Vec<_>>(),
            ["newest", "older"]
        );
        assert_eq!(page.next_cursor.as_deref(), Some("cursor-two"));
    }

    #[test]
    fn a_failed_older_page_can_be_retried_without_dropping_current_results() {
        let mut state = SubmissionInboxState::default();
        let initial_load = state.begin();
        state.complete(
            initial_load,
            Ok(submission_page(&["newest"], Some("cursor-one"))),
        );
        let (failed_load, _, _, _) = state.begin_more().expect("next page can load");
        state.complete_more(failed_load, Err(()));

        assert!(state.load_more_error);
        assert_eq!(
            state.response.as_ref().unwrap().as_ref().unwrap().items[0].submission_id,
            "newest"
        );
        let (retry, cursor, _, _) = state.begin_more().expect("failed page can be retried");
        assert_eq!(cursor, "cursor-one");
        state.complete_more(retry, Ok(submission_page(&["older"], None)));
        assert!(!state.load_more_error);
        assert_eq!(
            state
                .response
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .items
                .len(),
            2
        );
    }

    #[test]
    fn changing_submission_status_filter_clears_pages_and_ignores_stale_loads() {
        let mut state = SubmissionInboxState::default();
        let unfiltered = state.begin();
        state.complete(
            unfiltered,
            Ok(submission_page(&["newest"], Some("cursor-one"))),
        );
        let (older, _, _, _) = state.begin_more().expect("older page starts");

        assert!(state.set_status(Some(FormSubmissionStatus::Contacted)));
        assert_eq!(state.status, Some(FormSubmissionStatus::Contacted));
        assert!(state.response.is_none());
        assert!(!state.loading_more);
        assert!(!state.load_more_error);

        state.complete_more(older, Ok(submission_page(&["stale"], None)));
        assert!(state.response.is_none());
        assert!(state.set_status(None));
        assert!(!state.set_status(None));
    }

    #[test]
    fn changing_submission_funnel_filter_clears_pages_and_keeps_filters_on_pagination() {
        let funnel_id = "11111111-1111-4111-8111-111111111111".to_owned();
        let mut state = SubmissionInboxState::default();
        assert!(state.set_status(Some(FormSubmissionStatus::Qualified)));
        assert!(state.set_funnel_id(Some(funnel_id.clone())));
        let first_page = state.begin();
        state.complete(
            first_page,
            Ok(submission_page(&["matching"], Some("cursor-one"))),
        );

        let (older, cursor, status, selected_funnel) =
            state.begin_more().expect("older page starts");
        assert_eq!(cursor, "cursor-one");
        assert_eq!(status, Some(FormSubmissionStatus::Qualified));
        assert_eq!(selected_funnel.as_deref(), Some(funnel_id.as_str()));

        assert!(state.set_funnel_id(None));
        assert!(state.response.is_none());
        assert!(!state.loading_more);
        state.complete_more(older, Ok(submission_page(&["stale"], None)));
        assert!(state.response.is_none());
    }

    #[test]
    fn deleting_a_submission_hides_its_cached_copy_and_invalidates_in_flight_pages() {
        let mut state = SubmissionInboxState::default();
        let initial_load = state.begin();
        state.complete(
            initial_load,
            Ok(submission_page(&["target", "other"], Some("older-cursor"))),
        );
        let (stale_page, _, _, _) = state.begin_more().expect("older page starts");

        assert!(state.begin_delete("target"));
        assert!(!state.begin_delete("other"));

        let refreshed = state.begin();
        state.complete(
            refreshed,
            Ok(submission_page(&["target", "other"], Some("newer-cursor"))),
        );

        let visible_ids = &state.response.as_ref().unwrap().as_ref().unwrap().items;
        assert_eq!(visible_ids.len(), 1);
        assert_eq!(visible_ids[0].submission_id, "other");
        assert_eq!(state.delete_pending.as_deref(), Some("target"));
        state.complete_more(stale_page, Ok(submission_page(&["target"], None)));
        let visible_ids = &state.response.as_ref().unwrap().as_ref().unwrap().items;
        assert_eq!(visible_ids.len(), 1);
        assert_eq!(visible_ids[0].submission_id, "other");

        let delayed_list = state.begin();
        state.fail_delete("target");
        state.complete(
            delayed_list,
            Ok(submission_page(&["target", "other"], Some("late-cursor"))),
        );
        assert!(state.delete_error);
        assert!(state.delete_pending.is_none());
        assert!(state.response.is_none());

        let background_refresh = state.begin();
        state.complete(
            background_refresh,
            Ok(submission_page(&["target", "other"], None)),
        );
        let visible_ids = &state.response.as_ref().unwrap().as_ref().unwrap().items;
        assert_eq!(visible_ids.len(), 1);
        assert_eq!(visible_ids[0].submission_id, "other");
        assert!(state.delete_error);

        state.clear_delete_error();
        assert!(state.delete_error_id.is_none());
        let explicit_reload = state.begin();
        state.complete(
            explicit_reload,
            Ok(submission_page(&["target", "other"], None)),
        );
        assert!(!state.delete_error);
        assert_eq!(
            state
                .response
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .items
                .len(),
            2
        );
    }

    #[test]
    fn completing_a_deletion_clears_pending_and_error_state() {
        let mut state = SubmissionInboxState {
            delete_pending: Some("target".to_owned()),
            delete_error: true,
            delete_error_id: Some("target".to_owned()),
            ..SubmissionInboxState::default()
        };

        state.finish_delete();

        assert!(state.delete_pending.is_none());
        assert!(!state.delete_error);
        assert!(state.delete_error_id.is_none());
    }
}
