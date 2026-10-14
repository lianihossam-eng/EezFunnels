use dioxus::prelude::*;
use eez_domain::FunnelBlueprint;
use serde::Deserialize;
#[cfg(feature = "browser")]
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
}

#[component]
fn DashboardShell() -> Element {
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
                            span { class: "nav-icon", "⌂" }
                            span { "Dashboard" }
                        }
                        Link { to: RouteView::Funnels {}, class: "nav-link",
                            span { class: "nav-icon", "▤" }
                            span { "Funnels" }
                        }
                    }
                }
            }
            div { class: "workspace",
                header { class: "topbar",
                    span { class: "breadcrumb", "Workspace / Overview" }
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
                                h3 { "{item.name}" }
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
                        "/api/funnels",
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
                        match api_request(
                            "POST",
                            "/api/funnels",
                            Some(request_body),
                            Some(prepared_key.clone()),
                        ).await {
                            Ok(response) if response.status == 201 => {
                                let message = created_summary(&response.body, &request_name);
                                let _ = clear_idempotency_storage(&prepared_key);
                                finish_create_before_refresh(
                                    move || {
                                        create_success.set(Some(message));
                                        create_open.set(false);
                                        funnel_name.set(String::new());
                                        blueprint.set(FunnelBlueprint::Tripwire);
                                        create_idempotency_key.set(None);
                                        create_pending.set(false);
                                    },
                                    refresh_funnel_list(load_state),
                                )
                                .await;
                            }
                            Ok(response) => {
                                create_idempotency_key.set(Some(prepared_key.clone()));
                                create_error.set(Some(error_message(&response.body)));
                                create_pending.set(false);
                            }
                            Err(()) => {
                                create_error.set(Some("The funnel could not be created. Try again.".to_owned()));
                                create_pending.set(false);
                            }
                        }
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
                                                if let Some(Ok(items)) = state.response.as_mut() {
                                                    items.retain(|item| item.id != response_funnel_id);
                                                }
                                            });
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

#[cfg(feature = "browser")]
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
    web_sys::window()
        .ok_or(())?
        .session_storage()
        .map_err(|_| ())?
        .ok_or(())?
        .remove_item(&prepared_key.slot)
        .map_err(|_| ())
}

#[cfg(feature = "browser")]
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

#[cfg(feature = "browser")]
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

#[cfg(feature = "browser")]
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
async fn refresh_funnel_list(mut load_state: Signal<FunnelListLoad>) {
    let request_version = load_state.with_mut(FunnelListLoad::begin);
    let result = load_funnels().await;
    if load_state.try_peek().is_err() {
        return;
    }
    load_state.with_mut(|state| state.complete(request_version, result));
}

#[cfg(any(feature = "browser", test))]
async fn finish_create_before_refresh<F, R>(finish_create: F, refresh: R)
where
    F: FnOnce(),
    R: Future<Output = ()>,
{
    finish_create();
    refresh.await;
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
        let refresh = finish_create_before_refresh(
            move || {
                finished_dialog.set(false);
                finished_pending.set(false);
            },
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
}

#[component]
fn FunnelDetail(id: String) -> Element {
    rsx! {
        section { class: "page-heading",
            p { class: "eyebrow", "Funnels / Overview" }
            h1 { "Funnel overview" }
            p { class: "page-description", "This funnel is ready to open in the workspace." }
        }
        section { class: "detail-panel", "aria-label": "Funnel details",
            h2 { "Funnel details" }
            p { "This page provides the route-specific starting point for the dashboard." }
            code { class: "detail-id", "ID: {id}" }
        }
    }
}
