use dioxus::prelude::*;
use eez_domain::FunnelBlueprint;
use serde::Deserialize;
#[cfg(feature = "browser")]
use serde_json::Value;

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

#[component]
fn Funnels() -> Element {
    let mut load_state = use_signal(|| None::<Vec<FunnelCard>>);
    let mut load_failed = use_signal(|| false);
    let mut create_open = use_signal(|| false);
    let mut funnel_name = use_signal(String::new);
    let mut blueprint = use_signal(|| FunnelBlueprint::Tripwire);
    let mut create_pending = use_signal(|| false);
    let mut create_error = use_signal(|| None::<String>);
    let mut create_idempotency_key = use_signal(|| None::<String>);
    let mut create_success = use_signal(|| None::<String>);
    let mut delete_target = use_signal(|| None::<String>);
    let mut delete_pending = use_signal(|| false);
    let mut delete_error = use_signal(|| None::<String>);
    let mut delete_idempotency_key = use_signal(|| None::<String>);
    let mut delete_idempotency_target = use_signal(|| None::<String>);

    #[cfg(feature = "browser")]
    {
        let mut page_state = load_state;
        use_effect(move || {
            spawn(async move {
                match load_funnels().await {
                    Ok(items) => page_state.set(Some(items)),
                    Err(()) => load_failed.set(true),
                }
            });
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
        match (load_state(), load_failed()) {
            (None, false) => rsx! {
                section { class: "empty-state", "aria-label": "Funnel list", role: "status",
                    "Loading your funnels…"
                }
            },
            (None, true) => rsx! {
                section { class: "empty-state", "aria-label": "Funnel list", role: "status",
                    p { "Your funnels could not be loaded." }
                    button {
                        class: "secondary-button",
                        r#type: "button",
                        onclick: move |_| {
                            load_state.set(None);
                            load_failed.set(false);
                            #[cfg(feature = "browser")]
                            spawn(async move {
                                match load_funnels().await {
                                    Ok(items) => load_state.set(Some(items)),
                                    Err(()) => load_failed.set(true),
                                }
                            });
                        },
                        "Try again"
                    }
                }
            },
            (Some(items), _) if items.is_empty() => rsx! {
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
            (Some(items), _) => rsx! {
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
            class: "funnel-dialog",
            open: create_open(),
            "aria-labelledby": "create-funnel-title",
            "aria-describedby": "create-funnel-description",
            div { class: "dialog-heading",
                p { class: "eyebrow", "New workspace asset" }
                h2 { id: "create-funnel-title", "Create a funnel" }
                p { id: "create-funnel-description", "Choose a starting point. You can edit every section later." }
            }
            form {
                onsubmit: move |event| {
                    event.prevent_default();
                    if create_pending() || funnel_name().trim().is_empty() { return; }
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
                    let retry_key = create_idempotency_key();
                    #[cfg(feature = "browser")]
                    spawn(async move {
                        match api_request("POST", "/api/funnels", Some(request_body), retry_key).await {
                            Ok(response) if response.status == 201 => {
                                let message = created_summary(&response.body, &request_name);
                                create_success.set(Some(message));
                                create_open.set(false);
                                funnel_name.set(String::new());
                                blueprint.set(FunnelBlueprint::Tripwire);
                                create_idempotency_key.set(None);
                                load_state.set(None);
                                load_failed.set(false);
                                match load_funnels().await {
                                    Ok(items) => load_state.set(Some(items)),
                                    Err(()) => load_failed.set(true),
                                }
                            }
                            Ok(response) => {
                                create_idempotency_key.set(response.idempotency_key);
                                create_error.set(Some(error_message(&response.body)));
                            }
                            Err(()) => create_error.set(Some("The funnel could not be created. Try again.".to_owned())),
                        }
                        create_pending.set(false);
                    });
                    #[cfg(not(feature = "browser"))]
                    create_pending.set(false);
                },
                label { "Funnel name"
                    input {
                        r#type: "text",
                        name: "name",
                        required: true,
                        maxlength: "120",
                        value: "{funnel_name}",
                        oninput: move |event| {
                            funnel_name.set(event.value());
                            create_idempotency_key.set(None);
                        },
                    }
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
                                checked: blueprint() == item,
                                onchange: move |_| {
                                    blueprint.set(item);
                                    create_idempotency_key.set(None);
                                },
                            }
                            span { class: "blueprint-copy",
                                strong { "{item.label()}" }
                                span { "{item.description()}" }
                            }
                        }
                    }
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
                        disabled: create_pending() || funnel_name().trim().is_empty(),
                        if create_pending() { "Creating…" } else { "Create funnel" }
                    }
                }
            }
        }

        if let Some(target_id) = delete_target() {
            if let Some(target) = load_state()
                .and_then(|items| items.into_iter().find(|item| item.id == target_id))
            {
                dialog {
                    class: "funnel-dialog confirmation-dialog",
                    open: true,
                    "aria-labelledby": "delete-funnel-title",
                    "aria-describedby": "delete-funnel-description",
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
                                let retry_key = delete_idempotency_key();
                                #[cfg(feature = "browser")]
                                spawn(async move {
                                    match api_request("DELETE", &request_path, None, retry_key).await {
                                        Ok(response) if response.status == 200 => {
                                            let current = load_state();
                                            if let Some(mut items) = current {
                                                items.retain(|item| item.id != response_funnel_id);
                                                load_state.set(Some(items));
                                            }
                                            delete_target.set(None);
                                            delete_error.set(None);
                                            delete_idempotency_key.set(None);
                                            delete_idempotency_target.set(None);
                                        }
                                        Ok(response) => {
                                            delete_idempotency_key.set(response.idempotency_key);
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
    idempotency_key: Option<String>,
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
let key = request.idempotency_key;
if ((request.method === "POST" || request.method === "DELETE") && !key) {
    key = globalThis.crypto?.randomUUID?.() ?? `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}
const headers = {"Accept": "application/json"};
if (request.body !== null) headers["Content-Type"] = "application/json";
if (key) headers["Idempotency-Key"] = key;
try {
    const response = await fetch(request.path, {method: request.method, headers, body: request.body ?? undefined});
    dioxus.send({status: response.status, body: await response.text(), idempotency_key: key ?? null});
} catch (_) {
    dioxus.send({status: 0, body: "", idempotency_key: key ?? null});
}
"#;

#[cfg(feature = "browser")]
async fn api_request(
    method: &str,
    path: &str,
    body: Option<String>,
    idempotency_key: Option<String>,
) -> Result<BrowserResponse, ()> {
    let mut request = document::eval(BROWSER_API_REQUEST);
    request
        .send(serde_json::json!({
            "method": method,
            "path": path,
            "body": body,
            "idempotency_key": idempotency_key,
        }))
        .map_err(|_| ())?;
    request.recv().await.map_err(|_| ())
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
