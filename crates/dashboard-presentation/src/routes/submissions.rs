use dioxus::prelude::*;
use eez_domain::FormSubmissionStatus;
use serde_json::Value;
use std::collections::BTreeMap;

use super::api::SubmissionDeliveryAction;
#[cfg(feature = "browser")]
use super::api::{
    delete_public_form_submission, load_public_form_submission_follow_up, load_submission_page,
    update_public_form_submission_follow_up, FollowUpMutationFailure,
};
#[cfg(feature = "browser")]
use super::api::{update_submission_delivery, DeliveryControlFailure};
use super::api::{FollowUpHistoryResponse, FormSubmissionListItem, FormSubmissionListResponse};
use super::delivery::{webhook_status_copy, WebhookDeliveryPanel};
#[cfg(feature = "browser")]
use super::funnels::refresh_funnel_list;
use super::funnels::FunnelListLoad;
use super::idempotency::IdempotencyKeyState;
#[cfg(feature = "browser")]
use super::idempotency::{clear_idempotency_storage, prepare_idempotency_key};
use super::RouteView;

#[derive(Clone, Debug, PartialEq)]
struct FollowUpSaveOperation {
    expected_version: i32,
    status: FormSubmissionStatus,
    next_action: Option<String>,
    due_on: Option<String>,
    close_reason: Option<String>,
    corrects_version: Option<i32>,
    request_body: String,
    key: IdempotencyKeyState,
}

#[derive(Clone, Debug, PartialEq)]
struct FollowUpDraft {
    submission_id: String,
    form_purpose: String,
    funnel_name: String,
    submitted_at_label: String,
    submitted_at_datetime: String,
    version: i32,
    status: FormSubmissionStatus,
    next_action: String,
    due_on: String,
    close_reason: String,
    saved_status: FormSubmissionStatus,
    saved_next_action: String,
    saved_due_on: String,
    saved_close_reason: String,
    corrects_version: Option<i32>,
    pending: bool,
    error: Option<String>,
    key: Option<IdempotencyKeyState>,
    retry: Option<FollowUpSaveOperation>,
    history: Option<FollowUpHistoryResponse>,
    history_visible: bool,
    history_pending: bool,
    history_request_version: u64,
    history_error: bool,
    needs_review: bool,
}

impl FollowUpDraft {
    fn from_item(item: &FormSubmissionListItem) -> Self {
        let next_action = item.next_action.clone().unwrap_or_default();
        let due_on = item.due_on.clone().unwrap_or_default();
        let close_reason = item.close_reason.clone().unwrap_or_default();
        Self {
            submission_id: item.submission_id.clone(),
            form_purpose: item.form_purpose.clone(),
            funnel_name: item.funnel_name.clone(),
            submitted_at_label: item.submitted_at_label.clone(),
            submitted_at_datetime: item.submitted_at_datetime.clone(),
            version: item.follow_up_version,
            status: item.status,
            next_action: next_action.clone(),
            due_on: due_on.clone(),
            close_reason: close_reason.clone(),
            saved_status: item.status,
            saved_next_action: next_action,
            saved_due_on: due_on,
            saved_close_reason: close_reason,
            corrects_version: None,
            pending: false,
            error: None,
            key: None,
            retry: None,
            history: None,
            history_visible: false,
            history_pending: false,
            history_request_version: 0,
            history_error: false,
            needs_review: false,
        }
    }

    fn is_dirty(&self) -> bool {
        self.status != self.saved_status
            || self.next_action != self.saved_next_action
            || self.due_on != self.saved_due_on
            || self.close_reason != self.saved_close_reason
            || self.corrects_version.is_some()
    }

    fn needs_workspace(&self) -> bool {
        self.is_dirty()
            || self.pending
            || self.retry.is_some()
            || self.needs_review
            || self.history_visible
            || self.history_pending
    }
}

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
    follow_up_drafts: BTreeMap<String, FollowUpDraft>,
    follow_up_history_reloads: Vec<FollowUpDraft>,
    follow_up_history_sequence: u64,
}

impl SubmissionInboxState {
    fn ensure_follow_up_draft(&mut self, initial: &FollowUpDraft) -> &mut FollowUpDraft {
        self.follow_up_drafts
            .entry(initial.submission_id.clone())
            .or_insert_with(|| initial.clone())
    }

    fn forget_follow_up_draft(&mut self, submission_id: &str) {
        self.follow_up_drafts.remove(submission_id);
    }

    #[cfg(any(feature = "browser", test))]
    fn begin_follow_up_history(&mut self, initial: &FollowUpDraft) -> Option<u64> {
        self.follow_up_history_sequence = self.follow_up_history_sequence.wrapping_add(1).max(1);
        let request_version = self.follow_up_history_sequence;
        let draft = self.follow_up_drafts.get_mut(&initial.submission_id)?;
        draft.history_request_version = request_version;
        draft.history_pending = true;
        draft.history_error = false;
        draft.history = None;
        Some(request_version)
    }

    #[cfg(any(feature = "browser", test))]
    fn reconcile_follow_up_drafts(&mut self, items: &[FormSubmissionListItem]) {
        for item in items {
            let reload_history = match self.follow_up_drafts.get_mut(&item.submission_id) {
                Some(draft)
                    if !draft.is_dirty()
                        && !draft.pending
                        && draft.retry.is_none()
                        && !draft.needs_review =>
                {
                    draft.form_purpose = item.form_purpose.clone();
                    draft.funnel_name = item.funnel_name.clone();
                    draft.submitted_at_label = item.submitted_at_label.clone();
                    draft.submitted_at_datetime = item.submitted_at_datetime.clone();
                    if item.follow_up_version < draft.version {
                        None
                    } else {
                        let changed = draft.version != item.follow_up_version
                            || draft.status != item.status
                            || draft.next_action != item.next_action.as_deref().unwrap_or_default()
                            || draft.due_on != item.due_on.as_deref().unwrap_or_default()
                            || draft.close_reason
                                != item.close_reason.as_deref().unwrap_or_default();
                        if changed {
                            let history_visible = draft.history_visible;
                            let mut refreshed = FollowUpDraft::from_item(item);
                            refreshed.history_visible = history_visible;
                            *draft = refreshed;
                            history_visible.then(|| draft.clone())
                        } else {
                            None
                        }
                    }
                }
                _ => None,
            };
            if let Some(draft) = reload_history {
                self.follow_up_history_reloads.push(draft);
            }
        }
    }

    #[cfg(any(feature = "browser", test))]
    fn take_follow_up_history_reloads(&mut self) -> Vec<FollowUpDraft> {
        std::mem::take(&mut self.follow_up_history_reloads)
    }

    #[cfg(any(feature = "browser", test))]
    fn begin(&mut self) -> SubmissionRequestVersion {
        let request = self.latest.next();
        self.latest = request;
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
                self.reconcile_follow_up_drafts(&response.items);
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
                self.reconcile_follow_up_drafts(&next_page.items);
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
        self.forget_follow_up_draft(submission_id);
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
        self.response = None;
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
    let on_follow_up_save = use_callback(move |initial: FollowUpDraft| {
        start_follow_up_save(initial, load_state);
    });
    let on_follow_up_history = use_callback(move |initial: FollowUpDraft| {
        start_follow_up_history(initial, load_state);
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
    let visible_submission_ids = snapshot
        .response
        .as_ref()
        .and_then(|response| response.as_ref().ok())
        .map(|response| {
            response
                .items
                .iter()
                .map(|item| item.submission_id.as_str())
                .collect::<std::collections::BTreeSet<_>>()
        })
        .unwrap_or_default();
    let pinned_follow_up_drafts = snapshot
        .follow_up_drafts
        .values()
        .filter(|draft| {
            draft.needs_workspace()
                && !visible_submission_ids.contains(draft.submission_id.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
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
        if !pinned_follow_up_drafts.is_empty() {
            section { class: "submission-draft-workspace", "aria-label": "Follow-up workspace",
                h2 { class: "submission-section-title", "Follow-up workspace" }
                p { class: "submission-follow-up-help", "Unsaved drafts, uncertain retries, and opened histories stay available while filters or pages change." }
                for draft in pinned_follow_up_drafts.iter() {
                    article { class: "submission-pinned-draft", key: "{draft.submission_id}",
                        p { class: "submission-pinned-draft-context", "{draft.form_purpose} — {draft.funnel_name} — received {draft.submitted_at_label}" }
                        FollowUpEditor {
                            initial: draft.clone(),
                            load_state,
                            on_save: on_follow_up_save,
                            on_load_history: on_follow_up_history,
                            pinned: true,
                        }
                    }
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
                        SubmissionEntry {
                            key: "{item.submission_id}",
                            item: item.clone(),
                            load_state,
                            delete_pending: snapshot.delete_pending.is_some() || snapshot.delete_error,
                            on_delete,
                            on_follow_up_save,
                            on_follow_up_history,
                        }
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
    on_follow_up_save: EventHandler<FollowUpDraft>,
    on_follow_up_history: EventHandler<FollowUpDraft>,
) -> Element {
    let delivery_action_pending = use_signal(|| false);
    let delivery_action_error = use_signal(|| None::<String>);
    let delivery_action_key = use_signal(|| None::<IdempotencyKeyState>);
    let initial_follow_up = FollowUpDraft::from_item(&item);
    let follow_up_draft = load_state.with(|state| {
        state
            .follow_up_drafts
            .get(&item.submission_id)
            .cloned()
            .unwrap_or_else(|| initial_follow_up.clone())
    });
    let submission_id = item.submission_id.clone();
    let delete_submission_id = submission_id.clone();
    let retry_submission_id = submission_id.clone();
    let cancel_submission_id = submission_id.clone();
    rsx! {
        details { class: "submission-entry",
            summary {
                span { class: "submission-entry-purpose", "{item.form_purpose}" }
                span { class: "submission-entry-funnel", "{item.funnel_name}" }
                time { class: "submission-entry-time", datetime: "{item.submitted_at_datetime}", "{item.submitted_at_label}" }
                span { class: if item.marketing_opt_in { "submission-consent-state opted-in" } else { "submission-consent-state" },
                    if item.marketing_opt_in { "Marketing opt-in granted" } else { "Marketing opt-in not granted" }
                }
                span { class: "submission-status-badge", "{follow_up_draft.status.label()}" }
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
                            disabled: delete_pending || follow_up_draft.pending || follow_up_draft.retry.is_some(),
                            onclick: move |_| {
                                on_delete.call(delete_submission_id.clone());
                            },
                            "Delete permanently"
                        }
                    }
                }
                FollowUpEditor {
                    initial: initial_follow_up,
                    load_state,
                    on_save: on_follow_up_save,
                    on_load_history: on_follow_up_history,
                    pinned: false,
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

#[component]
fn FollowUpEditor(
    initial: FollowUpDraft,
    load_state: Signal<SubmissionInboxState>,
    on_save: EventHandler<FollowUpDraft>,
    on_load_history: EventHandler<FollowUpDraft>,
    pinned: bool,
) -> Element {
    let draft = load_state.with(|state| {
        state
            .follow_up_drafts
            .get(&initial.submission_id)
            .cloned()
            .unwrap_or_else(|| initial.clone())
    });
    let status_control_id = format!("submission-status-{}", initial.submission_id);
    let action_control_id = format!("submission-next-action-{}", initial.submission_id);
    let due_control_id = format!("submission-due-on-{}", initial.submission_id);
    let close_control_id = format!("submission-close-reason-{}", initial.submission_id);
    let status_initial = initial.clone();
    let action_initial = initial.clone();
    let due_initial = initial.clone();
    let close_initial = initial.clone();
    let history_initial = initial.clone();
    let save_initial = initial.clone();
    let correction_submission_id = initial.submission_id.clone();
    let review_submission_id = initial.submission_id.clone();
    let discard_submission_id = initial.submission_id.clone();
    let status_accessible_name = format!(
        "Follow-up status for {} submitted {}",
        initial.form_purpose, initial.submitted_at_label
    );
    rsx! {
        section { class: "submission-follow-up", "aria-label": "Submission follow-up",
            h2 { class: "submission-section-title", "Follow-up" }
            div { class: "submission-follow-up-fields",
                div { class: "submission-follow-up-field",
                    label { r#for: "{status_control_id}", "Status" }
                    select {
                        id: "{status_control_id}",
                        "aria-label": "{status_accessible_name}",
                        value: draft.status.as_str(),
                        disabled: draft.pending || draft.retry.is_some(),
                        onchange: move |event| {
                            let Some(status) = FormSubmissionStatus::from_wire_value(&event.value()) else { return; };
                            load_state.with_mut(|state| {
                                let draft = state.ensure_follow_up_draft(&status_initial);
                                draft.status = status;
                                if status != FormSubmissionStatus::Closed { draft.close_reason.clear(); }
                                draft.error = None;
                            });
                        },
                        option { value: "new", "New" }
                        option { value: "contacted", "Contacted" }
                        option { value: "qualified", "Qualified" }
                        option { value: "closed", "Closed" }
                    }
                }
                div { class: "submission-follow-up-field",
                    label { r#for: "{action_control_id}", "Next action" }
                    textarea {
                        id: "{action_control_id}",
                        rows: "2",
                        maxlength: "512",
                        value: "{draft.next_action}",
                        disabled: draft.pending || draft.retry.is_some(),
                        oninput: move |event| {
                            let value = event.value();
                            load_state.with_mut(|state| {
                                let draft = state.ensure_follow_up_draft(&action_initial);
                                draft.next_action = value;
                                draft.error = None;
                            });
                        },
                    }
                }
                div { class: "submission-follow-up-field",
                    label { r#for: "{due_control_id}", "Due date (optional)" }
                    input {
                        id: "{due_control_id}",
                        r#type: "date",
                        value: "{draft.due_on}",
                        disabled: draft.pending || draft.retry.is_some(),
                        oninput: move |event| {
                            let value = event.value();
                            load_state.with_mut(|state| {
                                let draft = state.ensure_follow_up_draft(&due_initial);
                                draft.due_on = value;
                                draft.error = None;
                            });
                        },
                    }
                }
                if draft.status == FormSubmissionStatus::Closed {
                    div { class: "submission-follow-up-field",
                        label { r#for: "{close_control_id}", "Reason for closing (optional)" }
                        textarea {
                            id: "{close_control_id}",
                            rows: "2",
                            maxlength: "512",
                            value: "{draft.close_reason}",
                            disabled: draft.pending || draft.retry.is_some(),
                            oninput: move |event| {
                                let value = event.value();
                                load_state.with_mut(|state| {
                                    let draft = state.ensure_follow_up_draft(&close_initial);
                                    draft.close_reason = value;
                                    draft.error = None;
                                });
                            },
                        }
                        p { class: "submission-follow-up-help", "Closed records a follow-up outcome. It does not mean a sale or payment." }
                    }
                }
            }
            if let Some(target) = draft.corrects_version {
                p { class: "submission-follow-up-help", role: "status",
                    "This records a correction to version {target}. It will be recorded now; the original date and history stay unchanged."
                    button {
                        class: "text-button",
                        r#type: "button",
                        disabled: draft.pending || draft.retry.is_some(),
                        onclick: move |_| load_state.with_mut(|state| {
                            if let Some(draft) = state.follow_up_drafts.get_mut(&correction_submission_id) {
                                draft.corrects_version = None;
                            }
                        }),
                        "Cancel correction"
                    }
                }
            }
            if draft.needs_review {
                p { class: "submission-status-error", role: "alert", "The saved version changed. Review the current values and history before saving this draft." }
                button {
                    class: "text-button",
                    r#type: "button",
                    disabled: draft.pending || draft.retry.is_some(),
                    onclick: move |_| load_state.with_mut(|state| {
                        if let Some(draft) = state.follow_up_drafts.get_mut(&review_submission_id) {
                            draft.needs_review = false;
                            draft.error = None;
                        }
                    }),
                    "I reviewed the latest history"
                }
            }
            button {
                class: "secondary-button",
                r#type: "button",
                disabled: draft.pending || draft.needs_review,
                onclick: move |_| on_save.call(save_initial.clone()),
                if draft.retry.is_some() { "Retry save" } else { "Save follow-up" }
            }
            if let Some(message) = draft.error.as_deref() {
                p { class: "submission-status-error", role: "alert", "{message}" }
            }
            if pinned {
                button {
                    class: "text-button",
                    r#type: "button",
                    disabled: draft.pending || draft.history_pending,
                    onclick: move |_| {
                        #[cfg(feature = "browser")]
                        if let Some(key) = load_state.with(|state| state.follow_up_drafts.get(&discard_submission_id).and_then(|draft| draft.key.clone())) {
                            let _ = clear_idempotency_storage(&key);
                        }
                        load_state.with_mut(|state| state.forget_follow_up_draft(&discard_submission_id));
                    },
                    if draft.is_dirty() || draft.retry.is_some() || draft.needs_review { "Discard draft" } else { "Close history" }
                }
            }
            div { class: "submission-follow-up-history",
                button {
                    class: "text-button",
                    r#type: "button",
                    disabled: draft.history_pending || draft.pending,
                    onclick: move |_| {
                        let should_load = load_state.with_mut(|state| {
                            let draft = state.ensure_follow_up_draft(&history_initial);
                            draft.history_visible = !draft.history_visible;
                            if draft.history_visible && !draft.history_pending {
                                draft.history_pending = true;
                                draft.history_error = false;
                                draft.history = None;
                                true
                            } else { false }
                        });
                        if should_load { on_load_history.call(history_initial.clone()); }
                    },
                    if draft.history_pending { "Loading history…" } else if draft.history_visible { "Hide history" } else { "Show follow-up history" }
                }
                if draft.history_visible {
                    if draft.history_error {
                        p { class: "submission-status-error", role: "alert", "The follow-up history could not be loaded. Try again." }
                    }
                    if let Some(history) = draft.history.as_ref() {
                        p { class: "submission-follow-up-help", "The dates below show when the inbox record was updated. They do not establish when contact happened." }
                        if history.events.is_empty() {
                            p { class: "submission-follow-up-help", "No follow-up history is available for this older submission." }
                        } else {
                            ol { class: "submission-follow-up-timeline",
                                for event in history.events.iter().rev() {
                                    FollowUpHistoryEvent {
                                        event: event.clone(),
                                        can_correct: event.corrects_version.is_none()
                                            && !history.events.iter().any(|correction| correction.corrects_version == Some(event.version)),
                                        disabled: draft.pending || draft.retry.is_some(),
                                        initial: initial.clone(),
                                        load_state,
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
fn FollowUpHistoryEvent(
    event: super::api::FollowUpEventResponse,
    can_correct: bool,
    disabled: bool,
    initial: FollowUpDraft,
    load_state: Signal<SubmissionInboxState>,
) -> Element {
    let target_version = event.version;
    rsx! {
        li {
            strong { "Version {event.version}: {event.status.label()}" }
            span { class: "submission-follow-up-event-time", "{event.recorded_at_label}" }
            if let Some(previous) = event.supersedes_version { p { "Updates version {previous}." } }
            if let Some(target) = event.corrects_version {
                p { "Corrects version {target}. This correction was recorded at the time shown and does not change the original date." }
            }
            if let Some(action) = event.next_action.as_deref() { p { "Next action: {action}" } }
            if let Some(date) = event.due_on.as_deref() { p { "Due: {date}" } }
            if let Some(reason) = event.close_reason.as_deref() { p { "Close reason: {reason}" } }
            p { class: "submission-follow-up-help", if event.provenance == "legacy_status" { "Recorded through the existing status interface." } else { "Recorded through the owner follow-up interface." } }
            if can_correct {
                button {
                    class: "text-button",
                    r#type: "button",
                    disabled,
                    onclick: move |_| load_state.with_mut(|state| {
                        let draft = state.ensure_follow_up_draft(&initial);
                        draft.corrects_version = Some(target_version);
                        draft.needs_review = false;
                        draft.error = None;
                    }),
                    "Correct this entry"
                }
            }
        }
    }
}

#[cfg(feature = "browser")]
fn start_follow_up_save(initial: FollowUpDraft, mut load_state: Signal<SubmissionInboxState>) {
    let operation = load_state.with_mut(|state| {
        let draft = state.ensure_follow_up_draft(&initial);
        if draft.pending {
            return None;
        }
        let operation = if let Some(retry) = draft.retry.clone() {
            retry
        } else {
            let next_action =
                (!draft.next_action.trim().is_empty()).then(|| draft.next_action.trim().to_owned());
            let due_on = (!draft.due_on.trim().is_empty()).then(|| draft.due_on.trim().to_owned());
            let close_reason = if draft.status == FormSubmissionStatus::Closed {
                (!draft.close_reason.trim().is_empty())
                    .then(|| draft.close_reason.trim().to_owned())
            } else {
                None
            };
            let request_body = serde_json::json!({
                "expected_version": draft.version,
                "status": draft.status,
                "next_action": next_action,
                "due_on": due_on,
                "close_reason": close_reason,
                "corrects_version": draft.corrects_version,
            })
            .to_string();
            let path = format!("/api/submissions/{}/follow-up", draft.submission_id);
            let Ok(key) =
                prepare_idempotency_key("PUT", &path, Some(&request_body), draft.key.clone())
            else {
                draft.error =
                    Some("This browser could not prepare a retry-safe update.".to_owned());
                return None;
            };
            FollowUpSaveOperation {
                expected_version: draft.version,
                status: draft.status,
                next_action,
                due_on,
                close_reason,
                corrects_version: draft.corrects_version,
                request_body,
                key,
            }
        };
        draft.key = Some(operation.key.clone());
        draft.retry = Some(operation.clone());
        draft.pending = true;
        draft.error = None;
        Some(operation)
    });
    let Some(operation) = operation else {
        return;
    };
    let submission_id = initial.submission_id.clone();
    spawn(async move {
        match update_public_form_submission_follow_up(
            &submission_id,
            &operation.request_body,
            operation.key.clone(),
        )
        .await
        {
            Ok(saved_version) => {
                let _ = clear_idempotency_storage(&operation.key);
                let updated_draft = load_state.with_mut(|state| {
                    let Some(draft) = state.follow_up_drafts.get_mut(&submission_id) else {
                        return false;
                    };
                    draft.version = saved_version;
                    draft.status = operation.status;
                    draft.next_action = operation.next_action.clone().unwrap_or_default();
                    draft.due_on = operation.due_on.clone().unwrap_or_default();
                    draft.close_reason = operation.close_reason.clone().unwrap_or_default();
                    draft.saved_status = draft.status;
                    draft.saved_next_action = draft.next_action.clone();
                    draft.saved_due_on = draft.due_on.clone();
                    draft.saved_close_reason = draft.close_reason.clone();
                    draft.corrects_version = None;
                    draft.pending = false;
                    draft.error = None;
                    draft.key = None;
                    draft.retry = None;
                    draft.needs_review = false;
                    true
                });
                if !updated_draft {
                    return;
                }
                let reload_history = load_state.with(|state| {
                    state
                        .follow_up_drafts
                        .get(&submission_id)
                        .is_some_and(|draft| draft.history_visible)
                });
                if reload_history {
                    reload_follow_up_history(&initial, load_state).await;
                }
                refresh_submission_list(load_state).await;
            }
            Err(FollowUpMutationFailure::Conflict) => {
                let _ = clear_idempotency_storage(&operation.key);
                load_state.with_mut(|state| {
                    if let Some(draft) = state.follow_up_drafts.get_mut(&submission_id) {
                        draft.pending = false;
                        draft.key = None;
                        draft.retry = None;
                        draft.needs_review = true;
                        draft.error = Some("This follow-up changed elsewhere. Your draft is preserved; review the latest history before saving again.".to_owned());
                    }
                });
                reload_follow_up_history(&initial, load_state).await;
                refresh_submission_list(load_state).await;
            }
            Err(FollowUpMutationFailure::CorrectionTargetUnavailable) => {
                let _ = clear_idempotency_storage(&operation.key);
                load_state.with_mut(|state| {
                    if let Some(draft) = state.follow_up_drafts.get_mut(&submission_id) {
                        draft.pending = false;
                        draft.key = None;
                        draft.retry = None;
                        draft.needs_review = true;
                        draft.error = Some("The selected correction target is no longer eligible. Your fields and selected target are preserved; choose another entry after reviewing the history.".to_owned());
                    }
                });
                reload_follow_up_history(&initial, load_state).await;
                refresh_submission_list(load_state).await;
            }
            Err(FollowUpMutationFailure::Uncertain) => {
                load_state.with_mut(|state| {
                    if let Some(draft) = state.follow_up_drafts.get_mut(&submission_id) {
                        draft.pending = false;
                        draft.error = Some("The server may have saved this update. Retry save to safely confirm the same request.".to_owned());
                    }
                });
            }
            Err(FollowUpMutationFailure::Rejected) => {
                let _ = clear_idempotency_storage(&operation.key);
                load_state.with_mut(|state| {
                    if let Some(draft) = state.follow_up_drafts.get_mut(&submission_id) {
                        draft.pending = false;
                        draft.key = None;
                        draft.retry = None;
                        draft.error = Some(
                            "This follow-up could not be saved. Review the fields and try again."
                                .to_owned(),
                        );
                    }
                });
            }
        }
    });
}

#[cfg(not(feature = "browser"))]
fn start_follow_up_save(_initial: FollowUpDraft, _load_state: Signal<SubmissionInboxState>) {}

#[cfg(feature = "browser")]
fn start_follow_up_history(initial: FollowUpDraft, mut load_state: Signal<SubmissionInboxState>) {
    let Some(request_version) =
        load_state.with_mut(|state| state.begin_follow_up_history(&initial))
    else {
        return;
    };
    spawn(async move {
        load_follow_up_history(&initial, request_version, load_state).await;
    });
}

#[cfg(not(feature = "browser"))]
fn start_follow_up_history(_initial: FollowUpDraft, _load_state: Signal<SubmissionInboxState>) {}

#[cfg(feature = "browser")]
async fn load_follow_up_history(
    initial: &FollowUpDraft,
    request_version: u64,
    mut load_state: Signal<SubmissionInboxState>,
) {
    let result = load_public_form_submission_follow_up(&initial.submission_id).await;
    load_state.with_mut(|state| {
        let Some(draft) = state.follow_up_drafts.get_mut(&initial.submission_id) else {
            return;
        };
        if draft.history_request_version != request_version {
            return;
        }
        draft.history_pending = false;
        match result {
            Ok(history) if history.current.version >= draft.version => {
                let current = history.current.clone();
                if draft.retry.is_some() {
                    draft.history = Some(history);
                    draft.history_error = false;
                } else if !draft.is_dirty() {
                    draft.version = current.version;
                    draft.status = current.status;
                    draft.next_action = current.next_action.clone().unwrap_or_default();
                    draft.due_on = current.due_on.clone().unwrap_or_default();
                    draft.close_reason = current.close_reason.clone().unwrap_or_default();
                    draft.saved_status = draft.status;
                    draft.saved_next_action = draft.next_action.clone();
                    draft.saved_due_on = draft.due_on.clone();
                    draft.saved_close_reason = draft.close_reason.clone();
                    draft.needs_review = false;
                    draft.history = Some(history);
                    draft.history_error = false;
                } else {
                    if current.version > draft.version {
                        draft.version = current.version;
                        draft.saved_status = current.status;
                        draft.saved_next_action = current.next_action.clone().unwrap_or_default();
                        draft.saved_due_on = current.due_on.clone().unwrap_or_default();
                        draft.saved_close_reason = current.close_reason.clone().unwrap_or_default();
                        draft.needs_review = true;
                        draft.error = Some("A newer saved version exists. Your unsaved draft is preserved; review the history before saving again.".to_owned());
                    }
                    draft.history = Some(history);
                    draft.history_error = false;
                }
            }
            Ok(_) | Err(()) => draft.history_error = true,
        }
    });
}

#[cfg(feature = "browser")]
async fn reload_follow_up_history(
    initial: &FollowUpDraft,
    mut load_state: Signal<SubmissionInboxState>,
) {
    let Some(request_version) = load_state.with_mut(|state| state.begin_follow_up_history(initial))
    else {
        return;
    };
    load_follow_up_history(initial, request_version, load_state).await;
}

#[cfg(feature = "browser")]
fn start_pending_follow_up_history_reloads(mut load_state: Signal<SubmissionInboxState>) {
    let pending = load_state.with_mut(SubmissionInboxState::take_follow_up_history_reloads);
    for initial in pending {
        let request_state = load_state;
        spawn(async move {
            reload_follow_up_history(&initial, request_state).await;
        });
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
    start_pending_follow_up_history_reloads(load_state);
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
    start_pending_follow_up_history_reloads(load_state);
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
            follow_up_version: 0,
            next_action: None,
            due_on: None,
            close_reason: None,
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
    fn list_refresh_reconciles_clean_follow_up_and_preserves_dirty_drafts() {
        let clean = submission_item("clean");
        let dirty = submission_item("dirty");
        let mut state = SubmissionInboxState::default();
        state.follow_up_drafts.insert(
            clean.submission_id.clone(),
            FollowUpDraft::from_item(&clean),
        );
        let mut dirty_draft = FollowUpDraft::from_item(&dirty);
        dirty_draft.next_action = "Keep the owner's unsaved note".to_owned();
        state
            .follow_up_drafts
            .insert(dirty.submission_id.clone(), dirty_draft);

        let mut refreshed_clean = clean;
        refreshed_clean.follow_up_version = 3;
        refreshed_clean.status = FormSubmissionStatus::Contacted;
        refreshed_clean.next_action = Some("Call the prospect".to_owned());
        let mut refreshed_dirty = dirty;
        refreshed_dirty.follow_up_version = 4;
        refreshed_dirty.status = FormSubmissionStatus::Qualified;
        refreshed_dirty.next_action = Some("A different saved note".to_owned());
        state.reconcile_follow_up_drafts(&[refreshed_clean, refreshed_dirty]);

        let clean = &state.follow_up_drafts["clean"];
        assert_eq!(clean.version, 3);
        assert_eq!(clean.status, FormSubmissionStatus::Contacted);
        assert_eq!(clean.next_action, "Call the prospect");
        assert!(!clean.is_dirty());

        let dirty = &state.follow_up_drafts["dirty"];
        assert_eq!(dirty.version, 0);
        assert_eq!(dirty.status, FormSubmissionStatus::New);
        assert_eq!(dirty.next_action, "Keep the owner's unsaved note");
        assert!(dirty.is_dirty());
    }

    #[test]
    fn stale_list_snapshots_cannot_regress_an_open_follow_up_history() {
        let item = submission_item("open-history");
        let mut draft = FollowUpDraft::from_item(&item);
        draft.version = 2;
        draft.status = FormSubmissionStatus::Contacted;
        draft.saved_status = FormSubmissionStatus::Contacted;
        draft.next_action = "Current action".to_owned();
        draft.saved_next_action = "Current action".to_owned();
        draft.history_visible = true;
        draft.history_pending = true;
        draft.history_request_version = 1;
        let mut state = SubmissionInboxState {
            follow_up_history_sequence: 1,
            ..SubmissionInboxState::default()
        };
        state
            .follow_up_drafts
            .insert(item.submission_id.clone(), draft);

        let stale_item = submission_item("open-history");
        state.reconcile_follow_up_drafts(&[stale_item]);
        let current = &state.follow_up_drafts["open-history"];
        assert_eq!(current.version, 2);
        assert_eq!(current.status, FormSubmissionStatus::Contacted);
        assert_eq!(current.next_action, "Current action");
        assert!(current.history_visible);
        assert!(current.history_pending);
        assert!(state.follow_up_history_reloads.is_empty());

        let mut newer_item = submission_item("open-history");
        newer_item.follow_up_version = 3;
        newer_item.status = FormSubmissionStatus::Qualified;
        newer_item.next_action = Some("Updated action".to_owned());
        state.reconcile_follow_up_drafts(&[newer_item]);

        let current = &state.follow_up_drafts["open-history"];
        assert_eq!(current.version, 3);
        assert_eq!(current.status, FormSubmissionStatus::Qualified);
        assert_eq!(current.next_action, "Updated action");
        assert!(current.history_visible);
        assert!(!current.history_pending);
        assert!(current.history.is_none());
        assert_eq!(state.follow_up_history_reloads.len(), 1);
        let reload = state.take_follow_up_history_reloads().pop().unwrap();
        let request_version = state.begin_follow_up_history(&reload).unwrap();
        let current = &state.follow_up_drafts["open-history"];
        assert!(current.history_pending);
        assert_eq!(current.history_request_version, request_version);
        assert_ne!(current.history_request_version, 1);
    }

    #[test]
    fn background_refresh_keeps_current_entries_until_the_latest_result_arrives() {
        let mut state = SubmissionInboxState::default();
        let initial_load = state.begin();
        state.complete(
            initial_load,
            Ok(submission_page(&["current"], Some("cursor-one"))),
        );

        let stale_refresh = state.begin();
        assert_eq!(
            state.response.as_ref().unwrap().as_ref().unwrap().items[0].submission_id,
            "current"
        );
        let latest_refresh = state.begin();
        state.complete(stale_refresh, Err(()));
        assert_eq!(
            state.response.as_ref().unwrap().as_ref().unwrap().items[0].submission_id,
            "current"
        );

        state.complete(latest_refresh, Ok(submission_page(&["updated"], None)));
        assert_eq!(
            state.response.as_ref().unwrap().as_ref().unwrap().items[0].submission_id,
            "updated"
        );
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
