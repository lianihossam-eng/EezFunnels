use dioxus::prelude::*;

mod editor;
use editor::FunnelDetail;
use eez_domain::{media::MediaAssetDescriptor, FunnelBlueprint};
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
const DELETE_FUNNEL_DESCRIPTION: &str = "This removes the funnel, its saved revisions, and all saved form submissions and consent evidence. This cannot be undone.";

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
    #[route("/media")]
    MediaLibrary {},
    #[end_layout]
    #[layout(DashboardShell)]
    #[route("/funnels/:id")]
    FunnelDetail { id: String },
    #[route("/funnels/:id/leads")]
    LeadInbox { id: String },
    #[route("/funnels/:id/leads/:submission_id")]
    LeadDetail { id: String, submission_id: String },
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
                        Link { to: RouteView::MediaLibrary {}, class: "nav-link",
                            span { class: "nav-icon", "▧" }
                            span { "Media library" }
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

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct DashboardMediaItem {
    descriptor: MediaAssetDescriptor,
    display_name: String,
    archived: bool,
    created_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct DashboardMediaPage {
    items: Vec<DashboardMediaItem>,
    next_cursor: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
struct DashboardMediaCapacity {
    enabled: bool,
    byte_budget: u64,
    bytes_spent: u64,
    byte_remaining: u64,
    key_budget: u64,
    keys_spent: u64,
    key_remaining: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
struct DashboardMediaCapacityResponse {
    capacity: DashboardMediaCapacity,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct MediaLibraryState {
    items: Vec<DashboardMediaItem>,
    capacity: Option<DashboardMediaCapacity>,
    archived: bool,
    next_cursor: Option<String>,
    loading: bool,
    load_error: bool,
    pending: bool,
    status: Option<String>,
    latest_request: u64,
}

impl MediaLibraryState {
    #[cfg(any(feature = "browser", test))]
    fn begin_load(&mut self, archived: bool, append: bool) -> (u64, Option<String>) {
        self.latest_request = self.latest_request.wrapping_add(1);
        self.archived = archived;
        self.loading = true;
        self.load_error = false;
        let cursor = append.then(|| self.next_cursor.clone()).flatten();
        if !append {
            self.items.clear();
            self.next_cursor = None;
        }
        (self.latest_request, cursor)
    }

    #[cfg(any(feature = "browser", test))]
    fn finish_load(
        &mut self,
        request: u64,
        response: Result<DashboardMediaPage, ()>,
        append: bool,
    ) {
        if request != self.latest_request {
            return;
        }
        self.loading = false;
        match response {
            Ok(page) => {
                if append {
                    self.items.extend(page.items);
                } else {
                    self.items = page.items;
                }
                self.next_cursor = page.next_cursor;
                self.load_error = false;
            }
            Err(()) => self.load_error = true,
        }
    }
}

#[component]
fn MediaLibrary() -> Element {
    let state = use_signal(MediaLibraryState::default);

    #[cfg(feature = "browser")]
    {
        let initial_state = state;
        use_effect(move || {
            spawn(async move { refresh_media_library(initial_state, false, false).await });
        });
    }

    let view = state();
    rsx! {
        section { class: "page-heading",
            p { class: "eyebrow", "Workspace" }
            h1 { "Media library" }
            p { class: "page-description", "Manage the images used by your funnel pages." }
        }
        section { class: "media-library-toolbar", "aria-label": "Upload and filter media",
            label { class: "media-filter-control", r#for: "media-archive-filter",
                "Show"
                select {
                    id: "media-archive-filter",
                    value: if view.archived { "archived" } else { "active" },
                    disabled: view.loading || view.pending,
                    onchange: move |_event| {
                        #[cfg(feature = "browser")]
                        {
                            let archived = _event.value() == "archived";
                            spawn(async move { refresh_media_library(state, archived, false).await });
                        }
                    },
                    option { value: "active", "Available images" }
                    option { value: "archived", "Archived images" }
                }
            }
            label { class: "media-upload-control", r#for: "media-upload",
                "Choose a JPEG or PNG image"
                input {
                    id: "media-upload",
                    r#type: "file",
                    accept: "image/jpeg,image/png",
                    disabled: view.pending,
                }
            }
            button {
                class: "primary-button",
                r#type: "button",
                disabled: view.pending,
                onclick: move |_| {
                    #[cfg(feature = "browser")]
                    spawn(async move { upload_selected_media(state).await });
                },
                if view.pending { "Uploading…" } else { "Upload image" }
            }
        }
        p { class: "media-library-help", "Images are normalized to remove embedded metadata. SVG and animated images are not accepted." }
        if let Some(status) = view.status.as_ref() {
            p { class: "success-message", role: "status", "{status}" }
        }
        if view.load_error {
            p { class: "error-message", role: "alert", "The media library could not be loaded. Try again." }
        }
        if let Some(capacity) = view.capacity {
            if capacity.enabled {
                section { class: "media-capacity", "aria-label": "Media storage capacity",
                    div {
                        strong { "Media storage" }
                        span { "{media_size_label(capacity.bytes_spent)} used of {media_size_label(capacity.byte_budget)}" }
                    }
                    progress {
                        value: "{capacity.bytes_spent}",
                        max: "{capacity.byte_budget}",
                        "aria-label": "Media storage used"
                    }
                    p { "{capacity.keys_spent} of {capacity.key_budget} image slots used ({capacity.key_remaining} remain); {media_size_label(capacity.byte_remaining)} remain." }
                }
            } else {
                p { class: "media-library-help", role: "status", "Uploads are disabled until positive media R2 budgets are configured." }
            }
        }
        if view.loading && view.items.is_empty() {
            p { class: "media-library-empty", role: "status", "Loading images…" }
        } else if view.items.is_empty() && !view.load_error {
            p { class: "media-library-empty", "No images here yet. Upload a JPEG or PNG to use it in a funnel." }
        } else {
            section { class: "media-library-grid", "aria-label": "Media assets",
                for item in view.items.iter() {
                    {
                        let asset_id = item.descriptor.asset_id().as_str().to_owned();
                        let archived = item.archived;
                        let display_name = item.display_name.clone();
                        let width = item.descriptor.width();
                        let height = item.descriptor.height();
                        let byte_length = item.descriptor.byte_length();
                        let format = item.descriptor.format().mime_type().to_owned();
                        let image_url = format!("/api/media/{asset_id}/content");
                        #[cfg(feature = "browser")]
                        let operation_id = asset_id.clone();
                        rsx! {
                            article { class: "media-library-card",
                                img { src: "{image_url}", alt: "", loading: "lazy", width: "{width}", height: "{height}" }
                                div { class: "media-library-card-copy",
                                    h2 { "{display_name}" }
                                    p { "{width} × {height} · {format} · {byte_length} bytes" }
                                    code { "{asset_id}" }
                                }
                                button {
                                    class: "secondary-button",
                                    r#type: "button",
                                    disabled: view.pending || view.loading,
                                    onclick: move |_| {
                                        #[cfg(feature = "browser")]
                                        {
                                            let operation_id = operation_id.clone();
                                            spawn(async move { archive_media_item(state, operation_id, !archived).await });
                                        }
                                    },
                                    if archived { "Restore" } else { "Archive" }
                                }
                            }
                        }
                    }
                }
            }
            if view.next_cursor.is_some() {
                button {
                    class: "secondary-button media-load-more",
                    r#type: "button",
                    disabled: view.loading || view.pending,
                    onclick: move |_| {
                        #[cfg(feature = "browser")]
                        spawn(async move {
                            let archived = state.with(|state| state.archived);
                            refresh_media_library(state, archived, true).await;
                        });
                    },
                    if view.loading { "Loading…" } else { "Load more" }
                }
            }
        }
    }
}

#[cfg(feature = "browser")]
async fn refresh_media_library(mut state: Signal<MediaLibraryState>, archived: bool, append: bool) {
    let (request, cursor) = state.with_mut(|state| state.begin_load(archived, append));
    let path = match cursor {
        Some(cursor) => format!("/api/media?archived={archived}&cursor={cursor}"),
        None => format!("/api/media?archived={archived}"),
    };
    let response = api_request("GET", &path, None, None).await;
    let result = match response {
        Ok(response) if response.status == 200 => {
            serde_json::from_str::<DashboardMediaPage>(&response.body).map_err(|_| ())
        }
        _ => Err(()),
    };
    let capacity = api_request("GET", "/api/media/capacity", None, None)
        .await
        .ok()
        .filter(|response| response.status == 200)
        .and_then(|response| {
            serde_json::from_str::<DashboardMediaCapacityResponse>(&response.body)
                .ok()
                .map(|response| response.capacity)
        });
    if state.try_peek().is_ok() {
        state.with_mut(|state| {
            if request == state.latest_request {
                state.capacity = capacity;
            }
            state.finish_load(request, result, append);
        });
    }
}

fn media_size_label(bytes: u64) -> String {
    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} bytes")
    }
}

#[cfg(feature = "browser")]
const BROWSER_UPLOAD_MEDIA: &str = r#"
const input = document.getElementById("media-upload");
if (!(input instanceof HTMLInputElement) || !input.files || input.files.length === 0) {
    dioxus.send({status: 0, body: ""});
} else {
    const file = input.files[0];
    try {
        const response = await fetch("/api/media", {
            method: "POST",
            headers: {
                "Accept": "application/json",
                "Content-Type": "application/octet-stream",
                "X-File-Name": encodeURIComponent(file.name)
            },
            body: file
        });
        dioxus.send({status: response.status, body: await response.text()});
        if (response.ok) input.value = "";
    } catch (_) {
        dioxus.send({status: 0, body: ""});
    }
}
"#;

#[cfg(feature = "browser")]
async fn upload_selected_media(mut state: Signal<MediaLibraryState>) {
    state.with_mut(|state| {
        state.pending = true;
        state.status = None;
        state.load_error = false;
    });
    let mut request = document::eval(BROWSER_UPLOAD_MEDIA);
    let result = request
        .recv::<BrowserResponse>()
        .await
        .map_err(|_| ())
        .and_then(|response| (response.status == 201).then_some(response).ok_or(()));
    match result {
        Ok(_) => {
            state.with_mut(|state| {
                state.pending = false;
                state.status = Some("Image uploaded and ready to use.".to_owned());
            });
            refresh_media_library(state, false, false).await;
        }
        Err(()) => {
            state.with_mut(|state| {
                state.pending = false;
                state.status = Some(
                    "The upload failed. Choose a JPEG or PNG under 2 MB and try again.".to_owned(),
                );
            });
        }
    }
}

#[cfg(feature = "browser")]
async fn archive_media_item(
    mut state: Signal<MediaLibraryState>,
    asset_id: String,
    archived: bool,
) {
    state.with_mut(|state| {
        state.pending = true;
        state.status = None;
    });
    let path = format!("/api/media/{asset_id}/archive");
    let body = serde_json::json!({ "archived": archived }).to_string();
    let result = api_request("POST", &path, Some(body), None).await;
    match result {
        Ok(response) if response.status == 200 => {
            state.with_mut(|state| {
                state.pending = false;
                state.status = Some(if archived {
                    "Image archived. Published pages that use it keep working.".to_owned()
                } else {
                    "Image restored to the available library.".to_owned()
                });
            });
            let filter = state.with(|state| state.archived);
            refresh_media_library(state, filter, false).await;
        }
        _ => state.with_mut(|state| {
            state.pending = false;
            state.status = Some(error_message(""));
        }),
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
                                Link {
                                    to: RouteView::LeadInbox { id: item.id.clone() },
                                    class: "funnel-leads-link",
                                    "View leads"
                                }
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
                    p { id: "delete-funnel-description", "{DELETE_FUNNEL_DESCRIPTION}" }
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

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct LeadSummary {
    submission_id: String,
    submitted_at: i64,
    expires_at: i64,
    page_title: String,
    form_purpose: String,
    marketing_opt_in: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct LeadPage {
    leads: Vec<LeadSummary>,
    next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct LeadAnswer {
    name: String,
    label: String,
    kind: String,
    value: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct LeadConsentEvidence {
    acknowledged: bool,
    copy_version: String,
    copy: String,
    recorded_at: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct LeadMarketingConsentEvidence {
    copy_version: String,
    copy: String,
    recorded_at: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct LeadDetailRecord {
    submission_id: String,
    submitted_at: i64,
    expires_at: i64,
    page_title: String,
    form_purpose: String,
    answers: Vec<LeadAnswer>,
    request_consent: LeadConsentEvidence,
    marketing_opt_in: bool,
    marketing_consent: Option<LeadMarketingConsentEvidence>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LeadInboxState {
    funnel_id: String,
    funnel_name: Option<String>,
    leads: Vec<LeadSummary>,
    next_cursor: Option<String>,
    loading: bool,
    load_error: bool,
    latest_route: u64,
    latest_request: u64,
    announcement: String,
}

impl Default for LeadInboxState {
    fn default() -> Self {
        Self {
            funnel_id: String::new(),
            funnel_name: None,
            leads: Vec::new(),
            next_cursor: None,
            loading: true,
            load_error: false,
            latest_route: 0,
            latest_request: 0,
            announcement: String::new(),
        }
    }
}

impl LeadInboxState {
    #[cfg(any(feature = "browser", test))]
    fn begin_route(&mut self, funnel_id: String) -> u64 {
        self.funnel_id = funnel_id;
        self.funnel_name = None;
        self.leads.clear();
        self.next_cursor = None;
        self.loading = true;
        self.load_error = false;
        self.announcement.clear();
        self.latest_route = self.latest_route.wrapping_add(1);
        self.latest_request = self.latest_request.wrapping_add(1);
        self.latest_route
    }

    #[cfg(any(feature = "browser", test))]
    fn begin_page(&mut self, append: bool) -> Option<(u64, Option<String>)> {
        if append && self.next_cursor.is_none() {
            return None;
        }
        self.latest_request = self.latest_request.wrapping_add(1);
        self.loading = true;
        self.load_error = false;
        if !append {
            self.leads.clear();
            self.next_cursor = None;
        }
        Some((
            self.latest_request,
            append.then(|| self.next_cursor.clone()).flatten(),
        ))
    }

    #[cfg(any(feature = "browser", test))]
    fn finish_page(&mut self, request: u64, result: Result<LeadPage, ()>, append: bool) {
        if request != self.latest_request {
            return;
        }
        self.loading = false;
        match result {
            Ok(page) => {
                let count = page.leads.len();
                if append {
                    self.leads.extend(page.leads);
                    self.announcement = format!("Loaded {count} more leads.");
                } else {
                    self.leads = page.leads;
                    self.announcement = format!("Loaded {count} leads.");
                }
                self.next_cursor = page.next_cursor;
                self.load_error = false;
            }
            Err(()) => self.load_error = true,
        }
    }

    #[cfg(any(feature = "browser", test))]
    fn finish_funnel_name(&mut self, route: u64, funnel_name: Option<String>) {
        if route == self.latest_route {
            self.funnel_name = funnel_name;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LeadDetailState {
    funnel_id: String,
    submission_id: String,
    detail: Option<LeadDetailRecord>,
    loading: bool,
    load_error: bool,
    latest_request: u64,
}

impl Default for LeadDetailState {
    fn default() -> Self {
        Self {
            funnel_id: String::new(),
            submission_id: String::new(),
            detail: None,
            loading: true,
            load_error: false,
            latest_request: 0,
        }
    }
}

impl LeadDetailState {
    #[cfg(any(feature = "browser", test))]
    fn begin_request(&mut self, funnel_id: String, submission_id: String) -> u64 {
        self.funnel_id = funnel_id;
        self.submission_id = submission_id;
        self.detail = None;
        self.loading = true;
        self.load_error = false;
        self.latest_request = self.latest_request.wrapping_add(1);
        self.latest_request
    }

    #[cfg(any(feature = "browser", test))]
    fn finish_request(&mut self, request: u64, result: Result<LeadDetailRecord, ()>) {
        if request != self.latest_request {
            return;
        }
        self.loading = false;
        match result {
            Ok(detail) => {
                self.detail = Some(detail);
                self.load_error = false;
            }
            Err(()) => self.load_error = true,
        }
    }
}

#[component]
fn LeadInbox(id: String) -> Element {
    #[allow(unused_mut)]
    let mut state = use_signal(LeadInboxState::default);

    #[cfg(feature = "browser")]
    use_effect(use_reactive!(|(id)| {
        let requested_id = id.clone();
        let title_id = requested_id.clone();
        let route = state.with_mut(|state| state.begin_route(requested_id.clone()));
        spawn(async move { refresh_lead_inbox(state, requested_id, false, false).await });
        spawn(async move { refresh_lead_funnel_name(state, title_id, route).await });
    }));

    let view = state();
    let title = view
        .funnel_name
        .as_ref()
        .map(|name| format!("{name} leads"))
        .unwrap_or_else(|| "Funnel leads".to_owned());
    #[cfg(feature = "browser")]
    let retry_append = !view.leads.is_empty() && view.next_cursor.is_some();

    rsx! {
        section { class: "page-heading lead-page-heading",
            p { class: "eyebrow", "Funnels / Leads" }
            h1 { "{title}" }
            p { class: "page-description", "Review submissions collected through this funnel’s published forms." }
            div { class: "lead-page-actions",
                Link {
                    to: RouteView::FunnelDetail { id: id.clone() },
                    class: "lead-navigation-link",
                    "Back to funnel editor"
                }
            }
        }
        p { id: "lead-live-status", class: "lead-live-status", role: "status", "aria-live": "polite", tabindex: -1, "{view.announcement}" }
        if view.loading && view.leads.is_empty() {
            section { class: "empty-state", role: "status", "aria-label": "Lead submissions",
                "Loading leads…"
            }
        } else if view.load_error && view.leads.is_empty() {
            section { class: "empty-state lead-load-error", role: "alert", "aria-label": "Lead submissions error",
                p { "The leads for this funnel could not be loaded." }
                button {
                    class: "secondary-button",
                    r#type: "button",
                    onclick: move |_| {
                        #[cfg(feature = "browser")]
                        {
                            let funnel_id = id.clone();
                            spawn(async move {
                                refresh_lead_inbox(state, funnel_id, false, false).await
                            });
                        }
                    },
                    "Try again"
                }
            }
        } else if view.leads.is_empty() {
            section { class: "empty-state", "aria-label": "Lead submissions",
                h2 { "No leads yet" }
                p { "Submissions will appear here after a visitor completes a form on a published funnel page." }
            }
        } else {
            if view.load_error {
                p { class: "error-message", role: "alert", "The next page could not be loaded. You can retry without losing this list." }
            }
            div { class: "lead-table-wrap",
                table { class: "lead-table",
                    caption { "Recent form submissions" }
                    thead {
                        tr {
                            th { scope: "col", "Submitted" }
                            th { scope: "col", "Page" }
                            th { scope: "col", "Form purpose" }
                            th { scope: "col", "Marketing" }
                            th { scope: "col", "Retained until" }
                        }
                    }
                    tbody {
                        for lead in view.leads {
                            tr { key: "{lead.submission_id}",
                                td {
                                    Link {
                                        to: RouteView::LeadDetail {
                                            id: id.clone(),
                                            submission_id: lead.submission_id.clone(),
                                        },
                                        class: "lead-row-link",
                                        "aria-label": lead_submission_label(lead.submitted_at),
                                        LeadTime { timestamp: lead.submitted_at }
                                    }
                                }
                                td { "{lead.page_title}" }
                                td { "{lead.form_purpose}" }
                                td {
                                    span { class: if lead.marketing_opt_in { "lead-consent-badge opted-in" } else { "lead-consent-badge" },
                                        if lead.marketing_opt_in { "Opted in" } else { "No opt-in" }
                                    }
                                }
                                td { LeadTime { timestamp: lead.expires_at } }
                            }
                        }
                    }
                }
            }
            div { class: "lead-load-more",
                button {
                    id: "lead-load-more-button",
                    class: "secondary-button",
                    r#type: "button",
                    disabled: view.loading || (!view.load_error && view.next_cursor.is_none()),
                    onclick: move |_| {
                        #[cfg(feature = "browser")]
                        {
                            let funnel_id = id.clone();
                            spawn(async move {
                                let restore_focus = retry_append && lead_load_more_has_focus().await;
                                refresh_lead_inbox(
                                    state,
                                    funnel_id,
                                    retry_append,
                                    restore_focus,
                                )
                                .await
                            });
                        }
                    },
                    if view.loading { "Loading more leads…" }
                    else if view.load_error { "Retry loading leads" }
                    else if view.next_cursor.is_some() { "Load more leads" }
                    else { "All recent leads loaded" }
                }
            }
            if view.next_cursor.is_none() && !view.loading {
                p { id: "lead-end-status", class: "lead-end-status", role: "status", tabindex: -1,
                    "You’ve reached the end of recent leads."
                }
            }
        }
    }
}

#[component]
fn LeadDetail(id: String, submission_id: String) -> Element {
    #[allow(unused_mut)]
    let mut state = use_signal(LeadDetailState::default);

    #[cfg(feature = "browser")]
    use_effect(use_reactive!(|id, submission_id| {
        let funnel_id = id.clone();
        let requested_submission_id = submission_id.clone();
        let request = state.with_mut(|state| {
            state.begin_request(funnel_id.clone(), requested_submission_id.clone())
        });
        spawn(async move {
            refresh_lead_detail(state, funnel_id, requested_submission_id, request).await;
        });
    }));

    let view = state();
    let editor_id = id.clone();
    let inbox_id = id.clone();

    rsx! {
        section { class: "page-heading lead-page-heading",
            p { class: "eyebrow", "Funnels / Leads / Submission" }
            h1 { "Submission details" }
            div { class: "lead-page-actions",
                Link {
                    to: RouteView::LeadInbox { id: inbox_id },
                    class: "lead-navigation-link",
                    "Back to leads"
                }
                Link {
                    to: RouteView::FunnelDetail { id: editor_id },
                    class: "lead-navigation-link",
                    "Back to funnel editor"
                }
            }
        }
        if view.loading {
            section { class: "empty-state", role: "status", "aria-label": "Lead details",
                "Loading submission details…"
            }
        } else if view.load_error {
            section { class: "empty-state lead-load-error", role: "alert", "aria-label": "Lead details error",
                p { "This submission could not be loaded. It may have expired or may no longer be available." }
                button {
                    class: "secondary-button",
                    r#type: "button",
                    onclick: move |_| {
                        #[cfg(feature = "browser")]
                        {
                            let funnel_id = id.clone();
                            let submission_id = submission_id.clone();
                            let request = state.with_mut(|state| {
                                state.begin_request(funnel_id.clone(), submission_id.clone())
                            });
                            spawn(async move {
                                refresh_lead_detail(state, funnel_id, submission_id, request).await;
                            });
                        }
                    },
                    "Try again"
                }
            }
        } else if let Some(detail) = view.detail {
            section { class: "lead-detail-panel", "aria-label": "Lead submission details",
                div { class: "lead-detail-context",
                    p { class: "eyebrow", "{detail.form_purpose}" }
                    h2 { "{detail.page_title}" }
                }
                dl { class: "lead-detail-meta",
                    div {
                        dt { "Submitted" }
                        dd { LeadTime { timestamp: detail.submitted_at } }
                    }
                    div {
                        dt { "Retained until" }
                        dd { LeadTime { timestamp: detail.expires_at } }
                    }
                }
                section { class: "lead-answer-section", "aria-labelledby": "lead-answers-heading",
                    h3 { id: "lead-answers-heading", "Answers" }
                    dl { class: "lead-answer-list",
                        for answer in detail.answers {
                            div { key: "{answer.name}", class: "lead-answer-row",
                                dt { "{answer.label}" }
                                dd { LeadAnswerText { answer: answer.clone() } }
                            }
                        }
                    }
                }
                section { class: "lead-answer-section", "aria-labelledby": "lead-consent-heading",
                    h3 { id: "lead-consent-heading", "Consent evidence" }
                    dl { class: "lead-answer-list",
                        div { class: "lead-answer-row",
                            dt { "Request consent" }
                            dd { if detail.request_consent.acknowledged { "Acknowledged" } else { "Not acknowledged" } }
                        }
                        div { class: "lead-answer-row",
                            dt { "Request consent copy" }
                            dd { "{detail.request_consent.copy}" }
                        }
                        div { class: "lead-answer-row",
                            dt { "Request consent version" }
                            dd { "{detail.request_consent.copy_version}" }
                        }
                        div { class: "lead-answer-row",
                            dt { "Request consent recorded" }
                            dd { LeadTime { timestamp: detail.request_consent.recorded_at } }
                        }
                        div { class: "lead-answer-row",
                            dt { "Marketing opt-in" }
                            dd { if detail.marketing_opt_in { "Opted in" } else { "Not opted in" } }
                        }
                        if let Some(consent) = detail.marketing_consent {
                            div { class: "lead-answer-row",
                                dt { "Marketing consent copy" }
                                dd { "{consent.copy}" }
                            }
                            div { class: "lead-answer-row",
                                dt { "Marketing consent version" }
                                dd { "{consent.copy_version}" }
                            }
                            div { class: "lead-answer-row",
                                dt { "Marketing consent recorded" }
                                dd { LeadTime { timestamp: consent.recorded_at } }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn LeadTime(timestamp: i64) -> Element {
    match format_lead_timestamp(timestamp) {
        Some(value) => rsx! { time { datetime: "{value}", "{value}" } },
        None => rsx! { span { "Date unavailable" } },
    }
}

#[component]
fn LeadAnswerText(answer: LeadAnswer) -> Element {
    let display = lead_answer_display(&answer);
    rsx! { span { class: "lead-answer-value", "{display}" } }
}

fn lead_submission_label(timestamp: i64) -> String {
    let timestamp =
        format_lead_timestamp(timestamp).unwrap_or_else(|| "at an unknown time".to_owned());
    format!("View submission received {timestamp}")
}

fn format_lead_timestamp(timestamp: i64) -> Option<String> {
    if timestamp < 0 {
        return None;
    }
    let days = timestamp / 86_400;
    let seconds = timestamp % 86_400;
    let shifted_days = days.checked_add(719_468)?;
    let era = shifted_days / 146_097;
    let day_of_era = shifted_days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_piece = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_piece + 2) / 5 + 1;
    let month = month_piece + if month_piece < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    if !(1..=9_999).contains(&year) {
        return None;
    }
    let hour = seconds / 3_600;
    let minute = seconds % 3_600 / 60;
    let second = seconds % 60;
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
    ))
}

fn lead_answer_display(answer: &LeadAnswer) -> String {
    match answer.value.as_ref() {
        None => "Not provided".to_owned(),
        Some(Value::Bool(value)) if answer.kind == "checkbox" => {
            (if *value { "Checked" } else { "Not checked" }).to_owned()
        }
        Some(Value::String(value)) if value.is_empty() => "Empty response".to_owned(),
        Some(Value::String(value)) => value.clone(),
        _ => "Response unavailable".to_owned(),
    }
}

#[cfg(feature = "browser")]
async fn refresh_lead_inbox(
    mut state: Signal<LeadInboxState>,
    funnel_id: String,
    append: bool,
    restore_focus: bool,
) {
    let Some((request, cursor)) = state.with_mut(|state| state.begin_page(append)) else {
        return;
    };
    let path = match cursor {
        Some(cursor) => format!("/api/funnels/{funnel_id}/leads?cursor={cursor}"),
        None => format!("/api/funnels/{funnel_id}/leads"),
    };
    let result = match api_request("GET", &path, None, None).await {
        Ok(response) if response.status == 200 => {
            serde_json::from_str::<LeadPage>(&response.body).map_err(|_| ())
        }
        _ => Err(()),
    };
    let completed = result.is_ok();
    state.with_mut(|state| state.finish_page(request, result, append));
    let current_request = state.with(|state| state.latest_request == request);
    if append && completed && current_request && restore_focus {
        restore_lead_inbox_focus().await;
    }
}

#[cfg(feature = "browser")]
async fn lead_load_more_has_focus() -> bool {
    let mut request = document::eval(
        r#"
const button = document.getElementById("lead-load-more-button");
dioxus.send(document.activeElement === button);
"#,
    );
    if request.send(()).is_err() {
        return false;
    }
    let result: Result<bool, _> = request.recv().await;
    result.unwrap_or(false)
}

#[cfg(feature = "browser")]
async fn restore_lead_inbox_focus() {
    let mut request = document::eval(
        r#"
await new Promise((resolve) => requestAnimationFrame(() => resolve()));
const button = document.getElementById("lead-load-more-button");
if (button && !button.disabled) button.focus();
else document.getElementById("lead-end-status")?.focus();
dioxus.send(true);
"#,
    );
    let _ = request.send(());
    let result: Result<bool, _> = request.recv().await;
    let _ = result;
}

#[cfg(feature = "browser")]
async fn refresh_lead_funnel_name(
    mut state: Signal<LeadInboxState>,
    funnel_id: String,
    route: u64,
) {
    let response = api_request("GET", "/api/funnels", None, None).await;
    let funnel_name = response
        .ok()
        .filter(|response| response.status == 200)
        .and_then(|response| serde_json::from_str::<FunnelListResponse>(&response.body).ok())
        .and_then(|response| {
            response
                .items
                .into_iter()
                .find(|funnel| funnel.id == funnel_id)
                .map(|funnel| funnel.name)
        });
    state.with_mut(|state| state.finish_funnel_name(route, funnel_name));
}

#[cfg(feature = "browser")]
async fn refresh_lead_detail(
    mut state: Signal<LeadDetailState>,
    funnel_id: String,
    submission_id: String,
    request: u64,
) {
    let path = format!("/api/funnels/{funnel_id}/leads/{submission_id}");
    let result = match api_request("GET", &path, None, None).await {
        Ok(response) if response.status == 200 => {
            serde_json::from_str::<LeadDetailRecord>(&response.body).map_err(|_| ())
        }
        _ => Err(()),
    };
    state.with_mut(|state| state.finish_request(request, result));
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

    #[test]
    fn stale_media_list_response_cannot_replace_a_newer_archive_filter() {
        let mut state = MediaLibraryState::default();
        let (older_request, _) = state.begin_load(false, false);
        let (newer_request, _) = state.begin_load(true, false);

        state.finish_load(older_request, Err(()), false);
        assert!(state.loading);
        assert!(!state.load_error);
        assert!(state.archived);

        state.finish_load(newer_request, Err(()), false);
        assert!(!state.loading);
        assert!(state.load_error);
        assert!(state.archived);
    }

    #[test]
    fn lead_inbox_paging_keeps_order_and_ignores_stale_responses() {
        let mut state = LeadInboxState::default();
        state.begin_route("funnel-a".to_owned());
        let (first_request, _) = state.begin_page(false).expect("first page request");
        state.finish_page(
            first_request,
            Ok(LeadPage {
                leads: vec![lead_summary("lead-1"), lead_summary("lead-2")],
                next_cursor: Some("cursor-2".to_owned()),
            }),
            false,
        );

        let (second_request, cursor) = state.begin_page(true).expect("next page request");
        assert_eq!(cursor.as_deref(), Some("cursor-2"));
        state.finish_page(
            first_request,
            Ok(LeadPage {
                leads: vec![lead_summary("stale")],
                next_cursor: None,
            }),
            true,
        );
        assert_eq!(state.leads.len(), 2);
        assert!(state.loading);

        state.finish_page(
            second_request,
            Ok(LeadPage {
                leads: vec![lead_summary("lead-3")],
                next_cursor: None,
            }),
            true,
        );
        assert_eq!(
            state
                .leads
                .iter()
                .map(|lead| lead.submission_id.as_str())
                .collect::<Vec<_>>(),
            ["lead-1", "lead-2", "lead-3"]
        );
        assert_eq!(state.announcement, "Loaded 1 more leads.");
        assert_eq!(state.next_cursor, None);
    }

    #[test]
    fn failed_load_more_keeps_existing_leads_and_retry_cursor() {
        let mut state = LeadInboxState::default();
        state.begin_route("funnel-a".to_owned());
        let (first_request, _) = state.begin_page(false).expect("first page request");
        state.finish_page(
            first_request,
            Ok(LeadPage {
                leads: vec![lead_summary("lead-1")],
                next_cursor: Some("cursor-1".to_owned()),
            }),
            false,
        );
        let (append_request, _) = state.begin_page(true).expect("next page request");
        state.finish_page(append_request, Err(()), true);

        assert_eq!(state.leads.len(), 1);
        assert_eq!(state.next_cursor.as_deref(), Some("cursor-1"));
        assert!(state.load_error);
        assert!(!state.loading);
    }

    #[test]
    fn funnel_name_response_cannot_replace_a_newer_lead_route() {
        let mut state = LeadInboxState::default();
        let older = state.begin_route("funnel-a".to_owned());
        let newer = state.begin_route("funnel-b".to_owned());

        state.finish_funnel_name(older, Some("Old funnel".to_owned()));
        assert_eq!(state.funnel_name, None);

        state.finish_funnel_name(newer, Some("New funnel".to_owned()));
        assert_eq!(state.funnel_name.as_deref(), Some("New funnel"));
    }

    #[test]
    fn lead_detail_ignores_a_response_for_an_older_route() {
        let mut state = LeadDetailState::default();
        let older = state.begin_request("funnel-a".to_owned(), "lead-a".to_owned());
        let newer = state.begin_request("funnel-b".to_owned(), "lead-b".to_owned());

        state.finish_request(older, Err(()));
        assert!(state.loading);
        assert!(!state.load_error);
        assert_eq!(state.funnel_id, "funnel-b");

        state.finish_request(newer, Err(()));
        assert!(!state.loading);
        assert!(state.load_error);
    }

    #[test]
    fn lead_timestamps_are_rendered_as_utc_and_values_keep_missing_empty_and_false_distinct() {
        assert_eq!(
            format_lead_timestamp(1_000_000_000).as_deref(),
            Some("2001-09-09T01:46:40Z")
        );
        assert!(format_lead_timestamp(i64::MAX).is_none());

        let answer = |value| LeadAnswer {
            name: "choice".to_owned(),
            label: "Choice".to_owned(),
            kind: "checkbox".to_owned(),
            value,
        };
        assert_eq!(lead_answer_display(&answer(None)), "Not provided");
        assert_eq!(
            lead_answer_display(&answer(Some(Value::Bool(false)))),
            "Not checked"
        );
        assert_eq!(
            lead_answer_display(&answer(Some(Value::String(String::new())))),
            "Empty response"
        );
    }

    #[test]
    fn deleting_a_funnel_warns_that_saved_leads_and_consent_evidence_are_removed() {
        assert!(DELETE_FUNNEL_DESCRIPTION.contains("saved form submissions"));
        assert!(DELETE_FUNNEL_DESCRIPTION.contains("consent evidence"));
        assert!(DELETE_FUNNEL_DESCRIPTION.contains("cannot be undone"));
    }

    #[test]
    fn lead_answer_values_render_as_escaped_text() {
        let mut virtual_dom = VirtualDom::new_with_props(
            LeadAnswerText,
            LeadAnswerTextProps {
                answer: LeadAnswer {
                    name: "message".to_owned(),
                    label: "Message".to_owned(),
                    kind: "text".to_owned(),
                    value: Some(Value::String("<img src=x onerror=alert(1)>".to_owned())),
                },
            },
        );
        virtual_dom.rebuild_in_place();
        let html = dioxus::ssr::render(&virtual_dom);

        assert!(
            html.contains("&#60;img src=x onerror=alert(1)&#62;"),
            "{html}"
        );
        assert!(!html.contains("<img"), "{html}");
    }

    fn lead_summary(submission_id: &str) -> LeadSummary {
        LeadSummary {
            submission_id: submission_id.to_owned(),
            submitted_at: 1_000_000_000,
            expires_at: 1_000_000_100,
            page_title: "Home".to_owned(),
            form_purpose: "Contact request".to_owned(),
            marketing_opt_in: false,
        }
    }
}
