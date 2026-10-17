use dioxus::prelude::*;

mod editor;
use editor::FunnelDetail;
use eez_domain::FunnelBlueprint;
use serde::Deserialize;
use serde_json::Value;
#[cfg(any(feature = "browser", test))]
use sha2::{Digest, Sha256};
#[cfg(any(feature = "browser", test))]
use std::future::Future;
#[cfg(feature = "browser")]
use uuid::Uuid;

#[cfg(feature = "browser")]
type IdempotencyKeyState = PreparedIdempotencyKey;
#[cfg(not(feature = "browser"))]
type IdempotencyKeyState = String;

const MAX_FUNNEL_NAME_BYTES: usize = 120;
const FUNNEL_NAME_LIMIT_MESSAGE: &str = "Funnel name must be 120 UTF-8 bytes or fewer.";

#[cfg(any(feature = "browser", test))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct FunnelLoadVersion(u64);

#[cfg(any(feature = "browser", test))]
impl FunnelLoadVersion {
    fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    fn is_current(self, latest: Self) -> bool {
        self == latest
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct FunnelListLoad {
    #[cfg(any(feature = "browser", test))]
    latest: FunnelLoadVersion,
    response: Option<Result<Vec<FunnelCard>, ()>>,
}

impl FunnelListLoad {
    #[cfg(any(feature = "browser", test))]
    fn begin(&mut self) -> FunnelLoadVersion {
        let request = self.latest.next();
        self.latest = request;
        self.response = None;
        request
    }

    #[cfg(any(feature = "browser", test))]
    fn complete(&mut self, request: FunnelLoadVersion, response: Result<Vec<FunnelCard>, ()>) {
        if request.is_current(self.latest) {
            self.response = Some(response);
        }
    }

    #[cfg(any(feature = "browser", test))]
    fn delete_succeeded(&mut self, funnel_id: &str) {
        self.latest = self.latest.next();
        if let Some(Ok(items)) = self.response.as_mut() {
            items.retain(|item| item.id != funnel_id);
        }
    }
}

#[derive(Clone, PartialEq, Routable)]
#[rustfmt::skip]
pub(crate) enum RouteView {
    #[layout(DashboardShell)]
    #[route("/")]
    Dashboard {},
    #[end_layout]
    #[layout(DashboardShell)]
    #[route("/funnels")]
    Funnels {},
    #[end_layout]
    #[layout(DashboardShell)]
    #[route("/funnels/:id")]
    FunnelDetail { id: String },
    #[end_layout]
    #[layout(DashboardShell)]
    #[route("/submissions")]
    Submissions {},
}

#[component]
fn DashboardShell() -> Element {
    let breadcrumb = match use_route::<RouteView>() {
        RouteView::Dashboard {} => "Workspace / Overview",
        RouteView::Funnels {} => "Workspace / Funnels",
        RouteView::FunnelDetail { .. } => "Workspace / Funnel editor",
        RouteView::Submissions {} => "Workspace / Submissions",
    };
    rsx! {
        div { class: "app-shell",
            aside { class: "sidebar",
                Link { to: RouteView::Dashboard {}, class: "brand",
                    span { class: "brand-mark", "E" }
                    span { "Eez Funnel" }
                }
                div { class: "sidebar-section",
                    p { class: "nav-label", "Workspace" }
                    nav { class: "primary-nav", "aria-label": "Main navigation",
                        Link { to: RouteView::Dashboard {}, class: "nav-link",
                            span { class: "nav-icon", "aria-hidden": "true",
                                svg { view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round",
                                    path { d: "m3 10 9-7 9 7 M5 9v11h14V9 M9 20v-6h6v6" }
                                }
                            }
                            span { "Dashboard" }
                        }
                        Link { to: RouteView::Funnels {}, class: "nav-link",
                            span { class: "nav-icon", "aria-hidden": "true",
                                svg { view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round",
                                    path { d: "M4 4h16v16H4z M8 8h8 M8 12h8 M8 16h5" }
                                }
                            }
                            span { "Funnels" }
                        }
                        Link { to: RouteView::Submissions {}, class: "nav-link",
                            span { class: "nav-icon", "aria-hidden": "true",
                                svg { view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.8", stroke_linecap: "round", stroke_linejoin: "round",
                                    path { d: "M4 5h16v14H4z M4 12h4l2 3h4l2-3h4" }
                                }
                            }
                            span { "Submissions" }
                        }
                    }
                }
            }
            div { class: "workspace",
                header { class: "topbar",
                    span { class: "breadcrumb", "{breadcrumb}" }
                    div { class: "owner-chip",
                        span { class: "owner-avatar", "EF" }
                        span { "Funnel workspace" }
                    }
                }
                main { class: "page-content",
                    Outlet::<RouteView> {}
                }
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct FormSubmissionListResponse {
    items: Vec<FormSubmissionListItem>,
    next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct FormSubmissionListItem {
    submission_id: String,
    funnel_name: String,
    page_title: String,
    form_purpose: String,
    fields: Vec<FormSubmissionField>,
    request_consent_acknowledged: bool,
    request_consent_copy_version: String,
    request_consent_copy: String,
    request_consent_recorded_at_label: String,
    marketing_opt_in: bool,
    marketing_opt_in_copy_version: Option<String>,
    marketing_opt_in_copy: Option<String>,
    marketing_opt_in_recorded_at_label: Option<String>,
    submitted_at_label: String,
    submitted_at_datetime: String,
    expires_at_label: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct FormSubmissionField {
    label: String,
    value: Value,
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
    response: Option<Result<FormSubmissionListResponse, ()>>,
    loading_more: bool,
    load_more_error: bool,
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
    fn complete(
        &mut self,
        request: SubmissionRequestVersion,
        response: Result<FormSubmissionListResponse, ()>,
    ) {
        if request.is_current(self.latest) {
            self.response = Some(response);
            self.loading_more = false;
            self.load_more_error = false;
        }
    }

    #[cfg(any(feature = "browser", test))]
    fn begin_more(&mut self) -> Option<(SubmissionRequestVersion, String)> {
        if self.loading_more {
            return None;
        }
        let cursor = self.response.as_ref()?.as_ref().ok()?.next_cursor.clone()?;
        let request = self.latest.next();
        self.latest = request;
        self.loading_more = true;
        Some((request, cursor))
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
                if let Some(Ok(current_page)) = self.response.as_mut() {
                    current_page.items.append(&mut next_page.items);
                    current_page.next_cursor = next_page.next_cursor;
                    self.load_more_error = false;
                }
            }
            Err(()) => self.load_more_error = true,
        }
    }
}

#[component]
fn Submissions() -> Element {
    let load_state = use_signal(SubmissionInboxState::default);

    #[cfg(feature = "browser")]
    {
        let page_state = load_state;
        use_effect(move || {
            spawn(async move {
                refresh_submission_list(page_state).await;
            });
        });
    }

    let snapshot = load_state();
    rsx! {
        section { class: "page-heading",
            h1 { "Submissions" }
            p { class: "page-description", "Review form responses alongside the consent and retention record captured at submission time." }
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
            Some(Ok(response)) if response.items.is_empty() => rsx! {
                section { class: "empty-state submission-empty", "aria-label": "No submissions yet",
                    h2 { "No form submissions yet" }
                    p { "Responses from published funnels will appear here while they remain within the configured retention period." }
                    Link { to: RouteView::Funnels {}, class: "secondary-button", "View funnels" }
                }
            },
            Some(Ok(response)) => rsx! {
                section { class: "submission-list", "aria-label": "Form submissions",
                    p { class: "submission-retention-note", "Only submissions within the configured retention period are shown." }
                    for item in response.items.iter() {
                        SubmissionEntry { key: "{item.submission_id}", item: item.clone() }
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
fn SubmissionEntry(item: FormSubmissionListItem) -> Element {
    rsx! {
        details { class: "submission-entry",
            summary {
                span { class: "submission-entry-purpose", "{item.form_purpose}" }
                span { class: "submission-entry-funnel", "{item.funnel_name}" }
                time { class: "submission-entry-time", datetime: "{item.submitted_at_datetime}", "{item.submitted_at_label}" }
                span { class: if item.marketing_opt_in { "submission-consent-state opted-in" } else { "submission-consent-state" },
                    if item.marketing_opt_in { "Marketing opt-in granted" } else { "Marketing opt-in not granted" }
                }
            }
            div { class: "submission-entry-content",
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

#[component]
fn Dashboard() -> Element {
    rsx! {
        section { class: "page-heading",
            p { class: "eyebrow", "Overview" }
            h1 { "Dashboard" }
            p { class: "page-description", "A clear view of your funnel workspace." }
        }
        section { class: "welcome-card", "aria-label": "Welcome to your dashboard",
            div {
                p { class: "eyebrow", "Your workspace" }
                h2 { "Build a smoother customer journey" }
                p { "Your dashboard is ready. Your funnels and workspace tools will appear here as you grow." }
            }
            div { class: "welcome-art", "aria-hidden": "true",
                div { class: "art-orbit",
                    div { class: "art-card",
                        div { class: "art-line" }
                        div { class: "art-line" }
                        div { class: "art-line" }
                    }
                }
            }
        }
        div { class: "section-heading",
            h2 { "Workspace snapshot" }
            span { "In development" }
        }
        section { class: "empty-state", "aria-label": "Workspace summary",
            "Workspace activity will appear here when its data is available."
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DialogCancelDisposition {
    KeepOpen,
    Dismiss,
}

fn dialog_cancel_disposition(mutation_pending: bool) -> DialogCancelDisposition {
    if mutation_pending {
        DialogCancelDisposition::KeepOpen
    } else {
        DialogCancelDisposition::Dismiss
    }
}

#[component]
fn Funnels() -> Element {
    let load_state = use_signal(FunnelListLoad::default);
    let mut create_open = use_signal(|| false);
    let mut funnel_name = use_signal(String::new);
    let mut blueprint = use_signal(|| FunnelBlueprint::Tripwire);
    let mut create_pending = use_signal(|| false);
    let mut create_error = use_signal(|| None::<String>);
    let mut create_idempotency_key = use_signal(|| None::<IdempotencyKeyState>);
    let mut create_success = use_signal(|| None::<String>);
    let mut delete_target = use_signal(|| None::<String>);
    let mut delete_pending = use_signal(|| false);
    let mut delete_error = use_signal(|| None::<String>);
    let mut delete_idempotency_key = use_signal(|| None::<IdempotencyKeyState>);
    let mut delete_idempotency_target = use_signal(|| None::<String>);

    #[cfg(feature = "browser")]
    {
        let page_state = load_state;
        use_effect(move || {
            spawn(async move {
                refresh_funnel_list(page_state).await;
            });
        });

        let create_modal_open = create_open;
        use_effect(move || {
            let open = create_modal_open();
            spawn(async move { set_modal_dialog("create-funnel-dialog", open).await });
        });

        let delete_modal_target = delete_target;
        use_effect(move || {
            let open = delete_modal_target().is_some();
            spawn(async move { set_modal_dialog("delete-funnel-dialog", open).await });
        });
    }

    rsx! {
        section { class: "page-heading",
            p { class: "eyebrow", "Workspace" }
            h1 { "Your funnels" }
            p { class: "page-description", "Review the funnels in your workspace." }
        }
        div { class: "section-heading funnel-list-heading",
            h2 { "Funnels" }
            button {
                class: "primary-button",
                r#type: "button",
                onclick: move |_| {
                    create_error.set(None);
                    create_success.set(None);
                    create_open.set(true);
                },
                "Create funnel"
            }
        }
        if let Some(message) = create_success() {
            p { class: "success-message", role: "status", "{message}" }
        }
        match load_state().response {
            None => rsx! {
                section { class: "empty-state", "aria-label": "Funnel list", role: "status",
                    "Loading your funnels…"
                }
            },
            Some(Err(())) => rsx! {
                section { class: "empty-state", "aria-label": "Funnel list", role: "status",
                    p { "Your funnels could not be loaded." }
                    button {
                        class: "secondary-button",
                        r#type: "button",
                        onclick: move |_| {
                            #[cfg(feature = "browser")]
                            spawn(async move {
                                refresh_funnel_list(load_state).await;
                            });
                        },
                        "Try again"
                    }
                }
            },
            Some(Ok(items)) if items.is_empty() => rsx! {
                section { class: "empty-state", "aria-label": "Funnel list",
                    h3 { "No funnels yet" }
                    p { "Create a funnel to start with a ready-to-edit page." }
                    button {
                        class: "secondary-button",
                        r#type: "button",
                        onclick: move |_| create_open.set(true),
                        "Create your first funnel"
                    }
                }
            },
            Some(Ok(items)) => rsx! {
                section { class: "funnel-list", "aria-label": "Funnel list",
                    for item in items {
                        article { class: "funnel-card",
                            div { class: "funnel-card-main",
                                h3 {
                                    Link {
                                        to: RouteView::FunnelDetail { id: item.id.clone() },
                                        class: "funnel-card-link",
                                        "{item.name}"
                                    }
                                }
                                p { class: "funnel-slug", "/{item.slug}" }
                            }
                            div { class: "funnel-card-meta",
                                span { class: if item.published_revision_id.is_some() { "status-badge published" } else { "status-badge draft" },
                                    if item.published_revision_id.is_some() { "Published" } else { "Draft" }
                                }
                                span { "Updated {item.updated_at}" }
                            }
                            button {
                                class: "text-button danger-button",
                                r#type: "button",
                                disabled: delete_pending(),
                                onclick: move |_| {
                                    if delete_idempotency_target().as_deref() != Some(item.id.as_str()) {
                                        delete_idempotency_key.set(None);
                                        delete_idempotency_target.set(Some(item.id.clone()));
                                    }
                                    delete_target.set(Some(item.id.clone()));
                                    delete_error.set(None);
                                },
                                "Delete"
                            }
                        }
                    }
                }
            },
        }

        dialog {
            id: "create-funnel-dialog",
            class: "funnel-dialog",
            "aria-modal": "true",
            "aria-labelledby": "create-funnel-title",
            "aria-describedby": "create-funnel-description",
            oncancel: move |event| match dialog_cancel_disposition(create_pending()) {
                DialogCancelDisposition::KeepOpen => event.prevent_default(),
                DialogCancelDisposition::Dismiss => create_open.set(false),
            },
            div { class: "dialog-heading",
                p { class: "eyebrow", "New workspace asset" }
                h2 { id: "create-funnel-title", "Create a funnel" }
                p { id: "create-funnel-description", "Choose a starting point. You can edit every section later." }
            }
            form {
                onsubmit: move |event| {
                    event.prevent_default();
                    if create_pending() || !can_create_funnel(&funnel_name()) { return; }
                    create_pending.set(true);
                    create_error.set(None);
                    #[cfg(feature = "browser")]
                    let request_name = funnel_name().trim().to_owned();
                    #[cfg(feature = "browser")]
                    let request_blueprint = blueprint();
                    #[cfg(feature = "browser")]
                    let request_body = serde_json::json!({
                        "name": request_name.clone(),
                        "blueprint": request_blueprint.id(),
                    }).to_string();
                    #[cfg(feature = "browser")]
                    let prepared_key = match prepare_idempotency_key(
                        "POST",
                        "/api/funnels/v2",
                        Some(&request_body),
                        create_idempotency_key(),
                    ) {
                        Ok(prepared_key) => prepared_key,
                        Err(()) => {
                            create_error.set(Some("The browser could not prepare a retry-safe request.".to_owned()));
                            create_pending.set(false);
                            return;
                        }
                    };
                    #[cfg(feature = "browser")]
                    create_idempotency_key.set(Some(prepared_key.clone()));
                    #[cfg(feature = "browser")]
                    spawn(async move {
                        let response = api_request(
                            "POST",
                            "/api/funnels/v2",
                            Some(request_body),
                            Some(prepared_key.clone()),
                        );
                        let retry_key = prepared_key.clone();
                        finish_create_response(
                            response,
                            request_name,
                            move |message| {
                                let _ = clear_idempotency_storage(&prepared_key);
                                create_success.set(Some(message));
                                create_open.set(false);
                                funnel_name.set(String::new());
                                blueprint.set(FunnelBlueprint::Tripwire);
                                create_idempotency_key.set(None);
                            },
                            move |message| {
                                create_idempotency_key.set(Some(retry_key));
                                create_error.set(Some(message));
                            },
                            move || create_pending.set(false),
                            refresh_funnel_list(load_state),
                        ).await;
                    });
                    #[cfg(not(feature = "browser"))]
                    create_pending.set(false);
                },
                CreateFields {
                    name: funnel_name(),
                    blueprint: blueprint(),
                    pending: create_pending(),
                    on_name_change: move |value| {
                        if create_pending() { return; }
                        funnel_name.set(value);
                        create_idempotency_key.set(None);
                    },
                    on_blueprint_change: move |value| {
                        if create_pending() { return; }
                        blueprint.set(value);
                        create_idempotency_key.set(None);
                    },
                }
                if let Some(message) = create_error() {
                    p { class: "error-message", role: "alert", "{message}" }
                }
                div { class: "dialog-actions",
                    button {
                        class: "secondary-button",
                        r#type: "button",
                        disabled: create_pending(),
                        onclick: move |_| {
                            create_open.set(false);
                        },
                        "Cancel"
                    }
                    button {
                        class: "primary-button",
                        r#type: "submit",
                        disabled: create_pending() || !can_create_funnel(&funnel_name()),
                        if create_pending() { "Creating…" } else { "Create funnel" }
                    }
                }
            }
        }

        if let Some(target_id) = delete_target() {
            if let Some(target) = load_state()
                .response
                .and_then(|result| result.ok())
                .and_then(|items| items.into_iter().find(|item| item.id == target_id))
            {
                dialog {
                    id: "delete-funnel-dialog",
                    class: "funnel-dialog confirmation-dialog",
                    "aria-modal": "true",
                    "aria-labelledby": "delete-funnel-title",
                    "aria-describedby": "delete-funnel-description",
                    oncancel: move |event| match dialog_cancel_disposition(delete_pending()) {
                        DialogCancelDisposition::KeepOpen => event.prevent_default(),
                        DialogCancelDisposition::Dismiss => {
                            delete_target.set(None);
                            delete_error.set(None);
                        }
                    },
                    h2 { id: "delete-funnel-title", "Delete {target.name}?" }
                    p { id: "delete-funnel-description", "This removes the funnel and its saved revisions." }
                    if let Some(message) = delete_error() {
                        p { class: "error-message", role: "alert", "{message}" }
                    }
                    div { class: "dialog-actions",
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            disabled: delete_pending(),
                            onclick: move |_| {
                                delete_target.set(None);
                                delete_error.set(None);
                            },
                            "Cancel"
                        }
                        button {
                            class: "primary-button danger-button",
                            r#type: "button",
                            disabled: delete_pending(),
                            onclick: move |_| {
                                if delete_pending() { return; }
                                delete_pending.set(true);
                                delete_error.set(None);
                                #[cfg(feature = "browser")]
                                let request_path = format!("/api/funnels/{}", target_id);
                                #[cfg(feature = "browser")]
                                let response_funnel_id = target_id.clone();
                                #[cfg(feature = "browser")]
                                let prepared_key = match prepare_idempotency_key(
                                    "DELETE",
                                    &request_path,
                                    None,
                                    delete_idempotency_key(),
                                ) {
                                    Ok(prepared_key) => prepared_key,
                                    Err(()) => {
                                        delete_error.set(Some("The browser could not prepare a retry-safe request.".to_owned()));
                                        delete_pending.set(false);
                                        return;
                                    }
                                };
                                #[cfg(feature = "browser")]
                                delete_idempotency_key.set(Some(prepared_key.clone()));
                                #[cfg(feature = "browser")]
                                spawn(async move {
                                    match api_request(
                                        "DELETE",
                                        &request_path,
                                        None,
                                        Some(prepared_key.clone()),
                                    ).await {
                                        Ok(response) if response.status == 200 => {
                                            let _ = clear_idempotency_storage(&prepared_key);
                                            let mut current_list = load_state;
                                            current_list.with_mut(|state| {
                                                state.delete_succeeded(&response_funnel_id);
                                            });
                                            let refreshed_list = load_state;
                                            spawn(async move { refresh_funnel_list(refreshed_list).await; });
                                            delete_target.set(None);
                                            delete_error.set(None);
                                            delete_idempotency_key.set(None);
                                            delete_idempotency_target.set(None);
                                        }
                                        Ok(response) => {
                                            delete_idempotency_key.set(Some(prepared_key.clone()));
                                            delete_error.set(Some(error_message(&response.body)));
                                        }
                                        Err(()) => delete_error.set(Some("The funnel could not be deleted. Try again.".to_owned())),
                                    }
                                    delete_pending.set(false);
                                });
                                #[cfg(not(feature = "browser"))]
                                delete_pending.set(false);
                            },
                            if delete_pending() { "Deleting…" } else { "Delete funnel" }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct FunnelCard {
    id: String,
    name: String,
    slug: String,
    published_revision_id: Option<String>,
    created_at: String,
    updated_at: String,
}

#[cfg(feature = "browser")]
#[derive(Deserialize)]
struct FunnelListResponse {
    items: Vec<FunnelCard>,
}

#[cfg(any(feature = "browser", test))]
#[derive(Deserialize)]
struct BrowserResponse {
    status: u16,
    body: String,
}

#[cfg(feature = "browser")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedIdempotencyKey {
    key: String,
    slot: String,
}

#[cfg(any(feature = "browser", test))]
fn idempotency_storage_slot(method: &str, path: &str, body: Option<&str>) -> String {
    let body = body
        .map(|body| {
            serde_json::from_str::<serde_json::Value>(body)
                .unwrap_or_else(|_| serde_json::Value::String(body.to_owned()))
        })
        .unwrap_or(serde_json::Value::Null);
    let canonical_request = serde_json::json!([method, path, body]).to_string();
    let digest = Sha256::digest(canonical_request.as_bytes());
    let digest_hex = digest
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
            output
        });
    format!("eezfunnel:idempotency:v1:{method}:{path}:{digest_hex}")
}

#[cfg(feature = "browser")]
fn prepare_idempotency_key(
    method: &str,
    path: &str,
    body: Option<&str>,
    retry_key: Option<PreparedIdempotencyKey>,
) -> Result<PreparedIdempotencyKey, ()> {
    let slot = idempotency_storage_slot(method, path, body);
    let storage = web_sys::window()
        .ok_or(())?
        .session_storage()
        .map_err(|_| ())?
        .ok_or(())?;
    let stored_key = storage
        .get_item(&slot)
        .map_err(|_| ())?
        .and_then(|stored| Uuid::parse_str(&stored).ok().map(|key| key.to_string()));
    let retry_key = retry_key
        .filter(|retry_key| retry_key.slot == slot)
        .and_then(|retry_key| {
            Uuid::parse_str(&retry_key.key)
                .ok()
                .map(|key| key.to_string())
        });
    let key = retry_key
        .or(stored_key)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    storage.set_item(&slot, &key).map_err(|_| ())?;
    Ok(PreparedIdempotencyKey { key, slot })
}

#[cfg(feature = "browser")]
fn clear_idempotency_storage(prepared_key: &PreparedIdempotencyKey) -> Result<(), ()> {
    let storage = web_sys::window()
        .ok_or(())?
        .session_storage()
        .map_err(|_| ())?
        .ok_or(())?;
    if storage
        .get_item(&prepared_key.slot)
        .map_err(|_| ())?
        .as_deref()
        == Some(prepared_key.key.as_str())
    {
        storage.remove_item(&prepared_key.slot).map_err(|_| ())?;
    }
    Ok(())
}

#[cfg(any(feature = "browser", test))]
fn created_summary(body: &str, name: &str) -> String {
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

fn section_label(section_type: &str) -> String {
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
fn error_message(body: &str) -> String {
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
async fn api_request(
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
const BROWSER_MODAL_DIALOG: &str = r#"
const {id, open} = await dioxus.recv();
const dialog = document.getElementById(id);
if (dialog instanceof HTMLDialogElement) {
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
}
dioxus.send(true);
"#;

#[cfg(feature = "browser")]
async fn set_modal_dialog(id: &'static str, open: bool) {
    let mut request = document::eval(BROWSER_MODAL_DIALOG);
    if request
        .send(serde_json::json!({ "id": id, "open": open }))
        .is_ok()
    {
        let _ = request.recv::<bool>().await;
    }
}

#[cfg(feature = "browser")]
async fn load_funnels() -> Result<Vec<FunnelCard>, ()> {
    let response = api_request("GET", "/api/funnels", None, None).await?;
    if response.status != 200 {
        return Err(());
    }
    serde_json::from_str::<FunnelListResponse>(&response.body)
        .map(|response| response.items)
        .map_err(|_| ())
}

#[cfg(feature = "browser")]
async fn load_submission_page(cursor: Option<&str>) -> Result<FormSubmissionListResponse, ()> {
    let path = match cursor {
        Some(cursor) => format!("/api/submissions?cursor={cursor}"),
        None => "/api/submissions".to_owned(),
    };
    let response = api_request("GET", &path, None, None).await?;
    if response.status != 200 {
        return Err(());
    }
    serde_json::from_str::<FormSubmissionListResponse>(&response.body).map_err(|_| ())
}

#[cfg(feature = "browser")]
async fn refresh_submission_list(mut load_state: Signal<SubmissionInboxState>) {
    let request_version = load_state.with_mut(SubmissionInboxState::begin);
    let result = load_submission_page(None).await;
    if load_state.try_peek().is_err() {
        return;
    }
    load_state.with_mut(|state| state.complete(request_version, result));
}

#[cfg(feature = "browser")]
async fn load_more_submissions(mut load_state: Signal<SubmissionInboxState>) {
    let Some((request_version, cursor)) = load_state.with_mut(SubmissionInboxState::begin_more)
    else {
        return;
    };
    let result = load_submission_page(Some(&cursor)).await;
    if load_state.try_peek().is_err() {
        return;
    }
    load_state.with_mut(|state| state.complete_more(request_version, result));
}

#[cfg(feature = "browser")]
async fn refresh_funnel_list(mut load_state: Signal<FunnelListLoad>) {
    let request_version = load_state.with_mut(FunnelListLoad::begin);
    let result = load_funnels().await;
    if load_state.try_peek().is_err() {
        return;
    }
    load_state.with_mut(|state| state.complete(request_version, result));
}

#[cfg(any(feature = "browser", test))]
async fn finish_create_response<Q, S, E, P, R>(
    response: Q,
    request_name: String,
    finish_success: S,
    finish_error: E,
    mut clear_pending: P,
    refresh: R,
) where
    Q: Future<Output = Result<BrowserResponse, ()>>,
    S: FnOnce(String),
    E: FnOnce(String),
    P: FnMut(),
    R: Future<Output = ()>,
{
    match response.await {
        Ok(response) if response.status == 201 => {
            finish_success(created_summary(&response.body, &request_name));
            clear_pending();
            refresh.await;
        }
        Ok(response) => {
            finish_error(error_message(&response.body));
            clear_pending();
        }
        Err(()) => {
            finish_error("The funnel could not be created. Try again.".to_owned());
            clear_pending();
        }
    }
}

fn can_create_funnel(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty() && trimmed.len() <= MAX_FUNNEL_NAME_BYTES
}

#[component]
fn CreateFields(
    name: String,
    blueprint: FunnelBlueprint,
    pending: bool,
    on_name_change: EventHandler<String>,
    on_blueprint_change: EventHandler<FunnelBlueprint>,
) -> Element {
    rsx! {
        label { "Funnel name"
            input {
                r#type: "text",
                name: "name",
                required: true,
                maxlength: "120",
                disabled: pending,
                value: "{name}",
                oninput: move |event| on_name_change.call(event.value()),
            }
        }
        if !name.trim().is_empty() && name.trim().len() > MAX_FUNNEL_NAME_BYTES {
            p { class: "error-message", role: "alert", "{FUNNEL_NAME_LIMIT_MESSAGE}" }
        }
        fieldset { class: "blueprint-options",
            legend { "Choose a blueprint" }
            for item in FunnelBlueprint::ALL {
                label { class: "blueprint-option",
                    "data-blueprint": item.id(),
                    input {
                        r#type: "radio",
                        name: "blueprint",
                        value: item.id(),
                        checked: blueprint == item,
                        disabled: pending,
                        onchange: move |_| on_blueprint_change.call(item),
                    }
                    span { class: "blueprint-copy",
                        strong { "{item.label()}" }
                        span { "{item.description()}" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::core::VirtualDom;

    #[test]
    fn create_fields_are_disabled_while_a_request_is_pending() {
        let mut virtual_dom = VirtualDom::new(|| {
            rsx! {
                CreateFields {
                    name: "Offer".to_owned(),
                    blueprint: FunnelBlueprint::Tripwire,
                    pending: true,
                    on_name_change: move |_| {},
                    on_blueprint_change: move |_| {},
                }
            }
        });
        virtual_dom.rebuild_in_place();
        let html = dioxus::ssr::render(&virtual_dom);

        assert_eq!(html.matches("<input").count(), 6, "{html}");
        assert_eq!(html.matches("disabled").count(), 6, "{html}");
    }

    #[test]
    fn create_name_limit_matches_the_command_utf8_byte_limit() {
        assert!(can_create_funnel(&"x".repeat(120)));
        assert!(!can_create_funnel(&"x".repeat(121)));
        assert!(can_create_funnel(&"😀".repeat(30)));
        assert!(!can_create_funnel(&"😀".repeat(31)));
    }

    #[test]
    fn create_fields_explain_when_multibyte_name_exceeds_command_byte_limit() {
        let mut virtual_dom = VirtualDom::new(|| {
            rsx! {
                CreateFields {
                    name: "😀".repeat(31),
                    blueprint: FunnelBlueprint::Tripwire,
                    pending: false,
                    on_name_change: move |_| {},
                    on_blueprint_change: move |_| {},
                }
            }
        });
        virtual_dom.rebuild_in_place();
        let html = dioxus::ssr::render(&virtual_dom);

        assert!(html.contains("120 UTF-8 bytes or fewer"), "{html}");
    }

    #[test]
    fn stale_funnel_responses_do_not_replace_the_newer_list_state() {
        let mut list_state = FunnelListLoad::default();
        let initial_load = list_state.begin();
        let create_refresh = list_state.begin();
        let created_funnel = FunnelCard {
            id: "created".to_owned(),
            name: "Created funnel".to_owned(),
            slug: "created-funnel".to_owned(),
            published_revision_id: None,
            created_at: "2026-10-14T00:00:00Z".to_owned(),
            updated_at: "2026-10-14T00:00:00Z".to_owned(),
        };

        list_state.complete(create_refresh, Ok(vec![created_funnel.clone()]));
        list_state.complete(initial_load, Ok(Vec::new()));
        list_state.complete(initial_load, Err(()));

        assert_eq!(list_state.response, Some(Ok(vec![created_funnel])));
    }

    #[test]
    fn a_list_response_started_before_delete_cannot_restore_the_deleted_funnel() {
        let mut list_state = FunnelListLoad::default();
        let pre_delete_refresh = list_state.begin();
        let deleted_funnel = FunnelCard {
            id: "deleted".to_owned(),
            name: "Deleted funnel".to_owned(),
            slug: "deleted-funnel".to_owned(),
            published_revision_id: None,
            created_at: "2026-10-14T00:00:00Z".to_owned(),
            updated_at: "2026-10-14T00:00:00Z".to_owned(),
        };
        let retained_funnel = FunnelCard {
            id: "retained".to_owned(),
            name: "Retained funnel".to_owned(),
            slug: "retained-funnel".to_owned(),
            published_revision_id: None,
            created_at: "2026-10-14T00:00:00Z".to_owned(),
            updated_at: "2026-10-14T00:00:00Z".to_owned(),
        };
        let stale_snapshot = vec![deleted_funnel.clone(), retained_funnel.clone()];

        list_state.response = Some(Ok(stale_snapshot.clone()));
        list_state.delete_succeeded(&deleted_funnel.id);
        list_state.complete(pre_delete_refresh, Ok(stale_snapshot));

        assert_eq!(list_state.response, Some(Ok(vec![retained_funnel])));
    }

    #[test]
    fn successful_create_closes_and_enables_form_before_unresolved_list_refresh() {
        use std::cell::Cell;
        use std::future::Future;
        use std::pin::Pin;
        use std::rc::Rc;
        use std::sync::Arc;
        use std::task::{Context, Poll, Wake, Waker};

        struct NoopWake;
        impl Wake for NoopWake {
            fn wake(self: Arc<Self>) {}
        }

        struct ManualRefresh(Rc<Cell<bool>>);
        impl Future for ManualRefresh {
            type Output = ();

            fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
                if self.0.get() {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            }
        }

        fn poll_once<F: Future>(mut future: Pin<&mut F>) -> Poll<F::Output> {
            let waker = Waker::from(Arc::new(NoopWake));
            let mut context = Context::from_waker(&waker);
            future.as_mut().poll(&mut context)
        }

        let dialog_open = Rc::new(Cell::new(true));
        let mutation_pending = Rc::new(Cell::new(true));
        let refresh_ready = Rc::new(Cell::new(false));
        let finished_dialog = Rc::clone(&dialog_open);
        let finished_pending = Rc::clone(&mutation_pending);
        let response = async {
            Ok(BrowserResponse {
                status: 201,
                body: "{}".to_owned(),
            })
        };
        let refresh = finish_create_response(
            response,
            "Offer".to_owned(),
            move |message| {
                assert_eq!(message, "Created Offer with starter sections.");
                finished_dialog.set(false);
            },
            |_| panic!("a successful creation must not take the error path"),
            move || finished_pending.set(false),
            ManualRefresh(Rc::clone(&refresh_ready)),
        );
        let mut refresh = std::pin::pin!(refresh);

        assert!(poll_once(refresh.as_mut()).is_pending());
        assert!(
            !dialog_open.get(),
            "success closes the create dialog immediately"
        );
        assert!(
            !mutation_pending.get(),
            "list refresh must not keep the create mutation pending"
        );

        dialog_open.set(true);
        mutation_pending.set(true);
        refresh_ready.set(true);
        assert!(poll_once(refresh.as_mut()).is_ready());
        assert!(dialog_open.get(), "a second create can open during refresh");
        assert!(
            mutation_pending.get(),
            "the first refresh completion must not clear the second create's pending state"
        );
    }

    #[test]
    fn unsuccessful_create_responses_clear_pending_without_refreshing() {
        use std::cell::Cell;
        use std::future::Future;
        use std::pin::Pin;
        use std::rc::Rc;
        use std::sync::Arc;
        use std::task::{Context, Poll, Wake, Waker};

        struct NoopWake;
        impl Wake for NoopWake {
            fn wake(self: Arc<Self>) {}
        }

        struct Refresh(Rc<Cell<bool>>);
        impl Future for Refresh {
            type Output = ();

            fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
                self.0.set(true);
                Poll::Ready(())
            }
        }

        let responses = [
            Ok(BrowserResponse {
                status: 400,
                body: "{}".to_owned(),
            }),
            Err(()),
        ];
        for response in responses {
            let mutation_pending = Rc::new(Cell::new(true));
            let error_reported = Rc::new(Cell::new(false));
            let refresh_polled = Rc::new(Cell::new(false));
            let finished_pending = Rc::clone(&mutation_pending);
            let reported_error = Rc::clone(&error_reported);
            let response = async move { response };
            let future = finish_create_response(
                response,
                "Offer".to_owned(),
                |_| panic!("an unsuccessful creation must not take the success path"),
                move |message| reported_error.set(!message.is_empty()),
                move || finished_pending.set(false),
                Refresh(Rc::clone(&refresh_polled)),
            );
            let mut future = std::pin::pin!(future);
            let waker = Waker::from(Arc::new(NoopWake));
            let mut context = Context::from_waker(&waker);

            assert!(future.as_mut().poll(&mut context).is_ready());
            assert!(error_reported.get());
            assert!(!mutation_pending.get());
            assert!(!refresh_polled.get());
        }
    }

    #[test]
    fn native_dialog_cancel_stays_open_until_pending_mutation_finishes() {
        assert_eq!(
            dialog_cancel_disposition(true),
            DialogCancelDisposition::KeepOpen
        );
        assert_eq!(
            dialog_cancel_disposition(false),
            DialogCancelDisposition::Dismiss
        );
    }

    #[test]
    fn idempotency_storage_slot_uses_canonical_payload_digest_without_exposing_fields() {
        let body = r#"{"name":"Offer","blueprint":"tripwire"}"#;
        let original = idempotency_storage_slot("POST", "/api/funnels", Some(body));
        assert_eq!(
            original,
            idempotency_storage_slot(
                "POST",
                "/api/funnels",
                Some(r#"{ "blueprint": "tripwire", "name": "Offer" }"#),
            )
        );
        assert!(!original.contains("Offer"));
        assert!(!original.contains("tripwire"));
        assert!(!original.contains(body));
        assert_ne!(
            original,
            idempotency_storage_slot(
                "POST",
                "/api/funnels",
                Some(r#"{"name":"Offer 2","blueprint":"tripwire"}"#),
            )
        );
        assert_ne!(
            original,
            idempotency_storage_slot("DELETE", "/api/funnels/funnel-1", None)
        );
    }

    fn submission_item(submission_id: &str) -> FormSubmissionListItem {
        FormSubmissionListItem {
            submission_id: submission_id.to_owned(),
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

        let (older_load, cursor) = state.begin_more().expect("next page can load");
        assert_eq!(cursor, "cursor-one");
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
        let (failed_load, _) = state.begin_more().expect("next page can load");
        state.complete_more(failed_load, Err(()));

        assert!(state.load_more_error);
        assert_eq!(
            state.response.as_ref().unwrap().as_ref().unwrap().items[0].submission_id,
            "newest"
        );
        let (retry, cursor) = state.begin_more().expect("failed page can be retried");
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
}
