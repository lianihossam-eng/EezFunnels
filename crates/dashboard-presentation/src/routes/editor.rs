use super::section_label;
#[cfg(feature = "browser")]
use super::{
    api_request, clear_idempotency_storage, error_message, prepare_idempotency_key,
    BrowserResponse, PreparedIdempotencyKey,
};
use dioxus::prelude::*;
use dioxus_dnd::prelude::{SortEvent, SortableList};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct EditorSection {
    id: String,
    #[serde(rename = "type")]
    section_type: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct EditorFunnel {
    id: String,
    name: String,
    slug: String,
    updated_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct EditorReadResponse {
    funnel: EditorFunnel,
    base_revision_id: String,
    sections: Vec<EditorSection>,
    preview_html: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EditorDocument {
    funnel: EditorFunnel,
    base_revision_id: String,
    sections: Vec<EditorSection>,
    saved_order: Vec<String>,
    order: Vec<String>,
    preview_html: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EditorPreviewCapture {
    generation: u64,
    funnel_id: String,
    base_revision_id: String,
    section_ids: Vec<String>,
}

#[cfg(any(feature = "browser", test))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct EditorSaveCapture {
    route_epoch: u64,
    generation: u64,
    funnel_id: String,
    base_revision_id: String,
    section_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct EditorState {
    funnel_id: String,
    #[cfg(any(feature = "browser", test))]
    route_epoch: u64,
    loading: bool,
    load_error: bool,
    document: Option<EditorDocument>,
    generation: u64,
    preview_error: bool,
    keyboard_section: Option<String>,
    keyboard_original_order: Option<Vec<String>>,
    announcement: String,
    saving: bool,
    retryable: bool,
    stale: bool,
    save_error: Option<String>,
}

impl EditorState {
    fn for_route(funnel_id: String) -> Self {
        Self {
            funnel_id,
            loading: true,
            #[cfg(any(feature = "browser", test))]
            route_epoch: 0,
            ..Self::default()
        }
    }

    #[cfg(any(feature = "browser", test))]
    fn begin_route(&mut self, funnel_id: String) -> u64 {
        let route_epoch = self.route_epoch.wrapping_add(1);
        *self = Self::for_route(funnel_id);
        self.route_epoch = route_epoch;
        route_epoch
    }

    #[cfg(any(feature = "browser", test))]
    fn finish_load(
        &mut self,
        route_epoch: u64,
        funnel_id: &str,
        response: Result<EditorReadResponse, ()>,
    ) {
        if self.route_epoch != route_epoch || self.funnel_id != funnel_id {
            return;
        }
        self.loading = false;
        match response {
            Ok(response) if response.funnel.id == funnel_id => {
                let order = response
                    .sections
                    .iter()
                    .map(|section| section.id.clone())
                    .collect::<Vec<_>>();
                self.document = Some(EditorDocument {
                    funnel: response.funnel,
                    base_revision_id: response.base_revision_id,
                    sections: response.sections,
                    saved_order: order.clone(),
                    order,
                    preview_html: response.preview_html,
                });
                self.load_error = false;
            }
            _ => self.load_error = true,
        }
    }

    fn move_section(&mut self, from: usize, to: usize) -> Option<EditorPreviewCapture> {
        if self.saving || self.retryable || self.stale {
            return None;
        }
        let document = self.document.as_mut()?;
        if from >= document.order.len() || to >= document.order.len() || from == to {
            return None;
        }
        let item = document.order.remove(from);
        document.order.insert(to, item);
        Some(self.begin_preview())
    }

    fn move_section_by_id(
        &mut self,
        section_id: &str,
        delta: isize,
    ) -> Option<EditorPreviewCapture> {
        let document = self.document.as_ref()?;
        let from = document.order.iter().position(|id| id == section_id)?;
        let to = from.checked_add_signed(delta)?;
        self.move_section(from, to)
    }

    fn begin_preview(&mut self) -> EditorPreviewCapture {
        self.generation = self.generation.wrapping_add(1);
        self.preview_error = false;
        let document = self.document.as_ref();
        EditorPreviewCapture {
            generation: self.generation,
            funnel_id: self.funnel_id.clone(),
            base_revision_id: document
                .map(|document| document.base_revision_id.clone())
                .unwrap_or_default(),
            section_ids: document
                .map(|document| document.order.clone())
                .unwrap_or_default(),
        }
    }

    #[cfg(any(feature = "browser", test))]
    fn finish_preview(&mut self, capture: &EditorPreviewCapture, response: Result<String, ()>) {
        if self.funnel_id != capture.funnel_id || self.generation != capture.generation {
            return;
        }
        let Some(document) = self.document.as_mut() else {
            return;
        };
        if document.base_revision_id != capture.base_revision_id
            || document.order != capture.section_ids
        {
            return;
        }
        match response {
            Ok(html) => {
                document.preview_html = html;
                self.preview_error = false;
            }
            Err(()) => self.preview_error = true,
        }
    }

    fn start_keyboard_drag(&mut self, section_id: &str) -> bool {
        let Some(document) = self.document.as_ref() else {
            return false;
        };
        if self.keyboard_section.is_some()
            || !document.order.iter().any(|id| id == section_id)
            || self.saving
            || self.retryable
            || self.stale
        {
            return false;
        }
        self.keyboard_original_order = Some(document.order.clone());
        self.keyboard_section = Some(section_id.to_owned());
        self.announcement = format!(
            "Picked up {} at position {} of {}. Use the arrow keys to move, Space to drop, or Escape to cancel.",
            self.section_name(section_id),
            document.order.iter().position(|id| id == section_id).unwrap_or(0) + 1,
            document.order.len()
        );
        true
    }

    fn finish_keyboard_drag(&mut self) {
        if let Some(section_id) = self.keyboard_section.take() {
            if let Some(document) = self.document.as_ref() {
                let position = document
                    .order
                    .iter()
                    .position(|id| id == &section_id)
                    .unwrap_or(0)
                    + 1;
                self.announcement = format!(
                    "Dropped {} at position {} of {}.",
                    self.section_name(&section_id),
                    position,
                    document.order.len()
                );
            }
        }
        self.keyboard_original_order = None;
    }

    fn cancel_keyboard_drag(&mut self) -> Option<EditorPreviewCapture> {
        if self.saving || self.retryable || self.stale {
            return None;
        }
        let original = self.keyboard_original_order.take()?;
        self.keyboard_section = None;
        let changed = self
            .document
            .as_ref()
            .is_some_and(|document| document.order != original);
        if let Some(document) = self.document.as_mut() {
            document.order = original;
        }
        self.announcement = "Reorder canceled. The original order was restored.".to_owned();
        changed.then(|| self.begin_preview())
    }

    fn can_save(&self) -> bool {
        !self.saving
            && !self.stale
            && self.keyboard_section.is_none()
            && self.keyboard_original_order.is_none()
            && (self.retryable
                || self
                    .document
                    .as_ref()
                    .is_some_and(|document| document.order != document.saved_order))
    }

    #[cfg(any(feature = "browser", test))]
    fn begin_save(&mut self) -> Option<EditorSaveCapture> {
        if !self.can_save() {
            return None;
        }
        let document = self.document.as_ref()?;
        let capture = EditorSaveCapture {
            route_epoch: self.route_epoch,
            generation: self.generation,
            funnel_id: self.funnel_id.clone(),
            base_revision_id: document.base_revision_id.clone(),
            section_ids: document.order.clone(),
        };
        self.saving = true;
        self.save_error = None;
        Some(capture)
    }

    #[cfg(any(feature = "browser", test))]
    fn is_current_save(&self, capture: &EditorSaveCapture) -> bool {
        self.route_epoch == capture.route_epoch
            && self.generation == capture.generation
            && self.funnel_id == capture.funnel_id
            && self.saving
            && self.document.as_ref().is_some_and(|document| {
                document.base_revision_id == capture.base_revision_id
                    && document.order == capture.section_ids
            })
    }

    #[cfg(any(feature = "browser", test))]
    fn finish_save(
        &mut self,
        capture: &EditorSaveCapture,
        revision_id: String,
    ) -> Option<EditorPreviewCapture> {
        if !self.is_current_save(capture) {
            return None;
        }
        let document = self.document.as_mut()?;
        document.base_revision_id = revision_id;
        document.saved_order = document.order.clone();
        self.saving = false;
        self.retryable = false;
        self.stale = false;
        self.save_error = None;
        Some(self.begin_preview())
    }

    #[cfg(any(feature = "browser", test))]
    fn fail_save(&mut self, capture: &EditorSaveCapture, stale: bool, message: String) -> bool {
        if !self.is_current_save(capture) {
            return false;
        }
        self.saving = false;
        self.stale = stale;
        self.retryable = !stale;
        self.save_error = Some(message);
        true
    }

    fn section_name(&self, section_id: &str) -> String {
        self.document
            .as_ref()
            .and_then(|document| {
                document
                    .sections
                    .iter()
                    .find(|section| section.id == section_id)
            })
            .map(|section| section_label(&section.section_type))
            .unwrap_or_else(|| "Section".to_owned())
    }
}

#[cfg(feature = "browser")]
#[derive(Deserialize)]
struct EditorPreviewResponse {
    preview_html: String,
}

#[cfg(feature = "browser")]
#[derive(Deserialize)]
struct EditorSaveResponse {
    revision_id: String,
}

#[cfg(feature = "browser")]
#[derive(Deserialize)]
struct ApiErrorBody {
    code: String,
}

#[cfg(feature = "browser")]
async fn load_editor(funnel_id: &str) -> Result<EditorReadResponse, ()> {
    let path = format!("/api/funnels/{funnel_id}");
    let response = api_request("GET", &path, None, None).await?;
    if response.status != 200 {
        return Err(());
    }
    serde_json::from_str::<EditorReadResponse>(&response.body).map_err(|_| ())
}

#[cfg(feature = "browser")]
async fn fetch_editor_preview(capture: &EditorPreviewCapture) -> Result<String, ()> {
    let path = format!("/api/funnels/{}/preview", capture.funnel_id);
    let body = serde_json::json!({
        "base_revision_id": capture.base_revision_id,
        "section_ids": capture.section_ids,
    })
    .to_string();
    let response = api_request("POST", &path, Some(body), None).await?;
    if response.status != 200 {
        return Err(());
    }
    serde_json::from_str::<EditorPreviewResponse>(&response.body)
        .map(|response| response.preview_html)
        .map_err(|_| ())
}

#[cfg(feature = "browser")]
async fn update_editor_preview(mut state: Signal<EditorState>, capture: EditorPreviewCapture) {
    let response = fetch_editor_preview(&capture).await;
    if state.try_peek().is_ok() {
        state.with_mut(|state| state.finish_preview(&capture, response));
    }
}

#[cfg(feature = "browser")]
fn schedule_editor_preview(state: Signal<EditorState>, capture: Option<EditorPreviewCapture>) {
    if let Some(capture) = capture {
        spawn(async move { update_editor_preview(state, capture).await });
    }
}

#[cfg(feature = "browser")]
async fn focus_editor_section(section_id: String) {
    let mut request = document::eval(
        r#"
const sectionId = await dioxus.recv();
requestAnimationFrame(() => {
    const row = document.querySelector(`[data-editor-section="${CSS.escape(sectionId)}"]`);
    if (row instanceof HTMLElement) row.focus();
});
dioxus.send(true);
"#,
    );
    if request.send(section_id).is_ok() {
        let _ = request.recv::<bool>().await;
    }
}

#[component]
pub(super) fn FunnelDetail(id: String) -> Element {
    #[cfg(feature = "browser")]
    let mut state = use_signal(|| EditorState::for_route(id.clone()));
    #[cfg(not(feature = "browser"))]
    let state = use_signal(|| EditorState::for_route(id.clone()));
    #[cfg(feature = "browser")]
    let retry_route_id = id.clone();
    #[cfg(feature = "browser")]
    let reload_route_id = id.clone();
    #[cfg(feature = "browser")]
    let mut save_key = use_signal(|| None::<PreparedIdempotencyKey>);
    #[cfg(feature = "browser")]
    let mut save_body = use_signal(|| None::<String>);

    #[cfg(feature = "browser")]
    use_effect(use_reactive!(|(id)| {
        let requested_id = id.clone();
        if let Some(key) = save_key.peek().as_ref() {
            let _ = clear_idempotency_storage(key);
        }
        save_key.set(None);
        save_body.set(None);
        let route_epoch = state.with_mut(|state| state.begin_route(requested_id.clone()));
        spawn(async move { load_editor_state(state, requested_id, route_epoch).await });
    }));

    let view = state();
    let title = view
        .document
        .as_ref()
        .map(|document| document.funnel.name.clone())
        .unwrap_or_else(|| "Funnel editor".to_owned());

    rsx! {
        section { class: "page-heading",
            p { class: "eyebrow", "Funnels / Editor" }
            h1 { "{title}" }
            p { class: "page-description", "Reorder sections and preview the funnel before saving a new revision." }
            code { class: "detail-id", "Funnel ID: {id}" }
        }
        if view.loading {
            section { class: "empty-state editor-loading", role: "status", "aria-label": "Funnel editor",
                "Loading the current funnel revision…"
            }
        }
        if view.load_error {
            section { class: "empty-state editor-load-error", role: "alert", "aria-label": "Funnel editor error",
                p { "The funnel editor could not be loaded." }
                button {
                    class: "secondary-button",
                    r#type: "button",
                    onclick: move |_| {
                        #[cfg(feature = "browser")]
                        {
                            let route_epoch = state.with_mut(|state| state.begin_route(retry_route_id.clone()));
                            let requested_id = retry_route_id.clone();
                            spawn(async move { load_editor_state(state, requested_id, route_epoch).await });
                        }
                    },
                    "Try again"
                }
            }
        }
        if let Some(document) = view.document.clone() {
            section { class: "editor-toolbar", "aria-label": "Editor actions",
                div {
                    p { class: "eyebrow", "Draft revision" }
                    code { "{document.base_revision_id}" }
                    if document.order != document.saved_order {
                        span { class: "editor-unsaved-badge", "Unsaved changes" }
                    } else {
                        span { class: "editor-saved-badge", "All changes saved" }
                    }
                }
                div { class: "editor-actions",
                    if view.stale {
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            onclick: move |_| {
                                #[cfg(feature = "browser")]
                                {
                                    if let Some(key) = save_key() { let _ = clear_idempotency_storage(&key); }
                                    save_key.set(None);
                                    save_body.set(None);
                                    let route_epoch = state.with_mut(|state| state.begin_route(reload_route_id.clone()));
                                    let requested_id = reload_route_id.clone();
                                    spawn(async move { load_editor_state(state, requested_id, route_epoch).await });
                                }
                            },
                            "Reload current revision"
                        }
                    }
                    button {
                        class: "primary-button",
                        r#type: "button",
                        disabled: !view.can_save(),
                        onclick: move |_| {
                            #[cfg(feature = "browser")]
                            spawn(async move {
                                save_editor_revision(state, save_key, save_body).await;
                            });
                        },
                        if view.retryable { "Retry save" } else if view.saving { "Saving…" } else { "Save revision" }
                    }
                }
            }
            if let Some(error) = view.save_error.clone() {
                p { class: "error-message editor-save-error", role: "alert", "{error}" }
            }
            if view.preview_error {
                p { class: "error-message editor-preview-error", role: "status",
                    "Preview could not be refreshed. Showing the last successful preview."
                }
            }
            section { class: "editor-workspace", "aria-label": "Visual editor",
                div { class: "editor-outline-panel",
                    div { class: "section-heading",
                        h2 { "Sections" }
                        span { "{document.order.len()} sections" }
                    }
                    EditorSortableList { state }
                    p { class: "editor-keyboard-help",
                        "Focus a section and press Space. Use the arrow keys to reorder it, Space to drop, or Escape to cancel."
                    }
                    p { class: "sr-only", role: "status", "aria-live": "polite", "aria-atomic": "true",
                        "{view.announcement}"
                    }
                }
                div { class: "editor-preview-panel",
                    div { class: "section-heading",
                        h2 { "Preview" }
                        span { "Sandboxed" }
                    }
                    iframe {
                        class: "editor-preview-frame",
                        title: "Preview of {document.funnel.name}",
                        "sandbox": "",
                        srcdoc: "{document.preview_html}",
                    }
                }
            }
        }
    }
}

#[component]
fn EditorSortableList(state: Signal<EditorState>) -> Element {
    let view = state();
    let Some(document) = view.document else {
        return rsx! { div {} };
    };
    let rows_locked = view.saving || view.retryable || view.stale;
    let sections = document.sections;
    let order = document.order;
    let mut editor_state = state;
    let item_order = order.clone();
    let render_order = order.clone();
    rsx! {
        if rows_locked {
            div { class: "editor-section-list", role: "list", "aria-label": "Funnel sections",
                for (index, section_id) in order.iter().enumerate() {
                    if let Some(section) = sections.iter().find(|section| section.id == *section_id) {
                        EditorSectionRow {
                            state: editor_state,
                            section: section.clone(),
                            index,
                            count: order.len(),
                            disabled: true,
                        }
                    }
                }
            }
        } else {
            SortableList {
                len: order.len(),
                class: "editor-section-list",
                role: "list",
                "aria-label": "Funnel sections",
                touch_handle: true,
                item_key: move |index| item_order.get(index).cloned().unwrap_or_default(),
                handle: move |_| rsx! { span { class: "editor-drag-grip", "⠿" } },
                render: move |index| {
                    let Some(section_id) = render_order.get(index) else { return rsx! { div {} }; };
                    let Some(section) = sections.iter().find(|section| section.id == *section_id) else {
                        return rsx! { div {} };
                    };
                    rsx! {
                        EditorSectionRow {
                            state: editor_state,
                            section: section.clone(),
                            index,
                            count: render_order.len(),
                            disabled: false,
                        }
                    }
                },
                on_sort: move |event: SortEvent| {
                    let section_id = editor_state.with(|state| {
                        state.document.as_ref().and_then(|document| document.order.get(event.from).cloned())
                    });
                    let capture = editor_state.with_mut(|state| state.move_section(event.from, event.to));
                    if let Some(section_id) = section_id {
                        #[cfg(feature = "browser")]
                        spawn(async move { focus_editor_section(section_id).await });
                        #[cfg(not(feature = "browser"))]
                        let _ = section_id;
                    }
                    #[cfg(feature = "browser")]
                    schedule_editor_preview(editor_state, capture);
                    #[cfg(not(feature = "browser"))]
                    let _ = capture;
                },
            }
        }
    }
}

#[component]
fn EditorSectionRow(
    state: Signal<EditorState>,
    section: EditorSection,
    index: usize,
    count: usize,
    disabled: bool,
) -> Element {
    let section_id = section.id.clone();
    let up_section_id = section_id.clone();
    let down_section_id = section_id.clone();
    let title = section_label(&section.section_type);
    rsx! {
        div {
            class: "editor-section-row",
            role: "listitem",
            tabindex: "0",
            "data-editor-section": "{section_id}",
            "aria-label": "Section: {title}, position {index + 1} of {count}. Press Space to pick up and reorder.",
            onkeydown: move |event: KeyboardEvent| {
                if disabled { return; }
                let key = event.key();
                if matches!(&key, Key::Character(value) if value == " ") || key == Key::Enter {
                    event.prevent_default();
                    let active = state.with(|state| state.keyboard_section.clone());
                    if active.as_deref() == Some(section_id.as_str()) {
                        state.with_mut(EditorState::finish_keyboard_drag);
                    } else {
                        state.with_mut(|state| { state.start_keyboard_drag(&section_id); });
                    }
                } else if key == Key::Escape {
                    let capture = state.with_mut(EditorState::cancel_keyboard_drag);
                    if capture.is_some() { event.prevent_default(); }
                    #[cfg(feature = "browser")]
                    schedule_editor_preview(state, capture);
                } else if key == Key::ArrowUp || key == Key::ArrowDown {
                    let active = state.with(|state| state.keyboard_section.clone());
                    if active.as_deref() == Some(section_id.as_str()) {
                        event.prevent_default();
                        let delta = if key == Key::ArrowUp { -1 } else { 1 };
                        let capture = state.with_mut(|state| state.move_section_by_id(&section_id, delta));
                        #[cfg(feature = "browser")]
                        {
                            let focus_id = section_id.clone();
                            spawn(async move { focus_editor_section(focus_id).await });
                            schedule_editor_preview(state, capture);
                        }
                        #[cfg(not(feature = "browser"))]
                        let _ = capture;
                    }
                }
            },
            div { class: "editor-section-copy",
                span { class: "editor-section-type", "{title}" }
                code { "{section.id}" }
            }
            div { class: "editor-section-actions",
                button {
                    class: "editor-move-button",
                    r#type: "button",
                    "aria-label": "Move {title} up",
                    onkeydown: move |event: KeyboardEvent| event.stop_propagation(),
                    disabled: disabled || index == 0,
                    onclick: move |_| {
                        let capture = state.with_mut(|state| state.move_section_by_id(&up_section_id, -1));
                        #[cfg(feature = "browser")]
                        {
                            let focus_id = up_section_id.clone();
                            spawn(async move { focus_editor_section(focus_id).await });
                            schedule_editor_preview(state, capture);
                        }
                        #[cfg(not(feature = "browser"))]
                        let _ = capture;
                    },
                    "Move up"
                }
                button {
                    class: "editor-move-button",
                    r#type: "button",
                    "aria-label": "Move {title} down",
                    onkeydown: move |event: KeyboardEvent| event.stop_propagation(),
                    disabled: disabled || index + 1 >= count,
                    onclick: move |_| {
                        let capture = state.with_mut(|state| state.move_section_by_id(&down_section_id, 1));
                        #[cfg(feature = "browser")]
                        {
                            let focus_id = down_section_id.clone();
                            spawn(async move { focus_editor_section(focus_id).await });
                            schedule_editor_preview(state, capture);
                        }
                        #[cfg(not(feature = "browser"))]
                        let _ = capture;
                    },
                    "Move down"
                }
            }
        }
    }
}

#[cfg(feature = "browser")]
async fn load_editor_state(mut state: Signal<EditorState>, funnel_id: String, route_epoch: u64) {
    let response = load_editor(&funnel_id).await;
    if state.try_peek().is_ok() {
        state.with_mut(|state| state.finish_load(route_epoch, &funnel_id, response));
    }
}

#[cfg(feature = "browser")]
fn clear_save_attempt_if_current(
    state: Signal<EditorState>,
    save_key: &mut Signal<Option<PreparedIdempotencyKey>>,
    save_body: &mut Signal<Option<String>>,
    capture: &EditorSaveCapture,
    prepared_key: &PreparedIdempotencyKey,
) {
    let current = state.with(|state| state.is_current_save(capture));
    let signal_matches = save_key().as_ref() == Some(prepared_key);
    let _ = clear_idempotency_storage(prepared_key);
    if current && signal_matches {
        save_key.set(None);
        save_body.set(None);
    }
}

#[cfg(feature = "browser")]
async fn save_editor_revision(
    mut state: Signal<EditorState>,
    mut save_key: Signal<Option<PreparedIdempotencyKey>>,
    mut save_body: Signal<Option<String>>,
) {
    let capture = state.with_mut(EditorState::begin_save);
    let Some(capture) = capture else {
        return;
    };
    let expected_body = serde_json::json!({
        "base_revision_id": capture.base_revision_id,
        "section_ids": capture.section_ids,
    })
    .to_string();
    let body = if save_body().as_deref() == Some(expected_body.as_str()) {
        expected_body
    } else {
        if let Some(key) = save_key() {
            let _ = clear_idempotency_storage(&key);
        }
        save_key.set(None);
        save_body.set(None);
        expected_body
    };
    let path = format!("/api/funnels/{}/revisions", capture.funnel_id);
    let prepared_key = match prepare_idempotency_key("PUT", &path, Some(&body), save_key()) {
        Ok(key) => key,
        Err(()) => {
            state.with_mut(|state| {
                state.fail_save(
                    &capture,
                    false,
                    "A retry-safe save could not be prepared.".to_owned(),
                )
            });
            return;
        }
    };
    save_key.set(Some(prepared_key.clone()));
    save_body.set(Some(body.clone()));
    let response = api_request("PUT", &path, Some(body), Some(prepared_key.clone())).await;
    match response {
        Ok(response) if response.status == 200 => {
            let saved = serde_json::from_str::<EditorSaveResponse>(&response.body);
            if let Ok(saved) = saved {
                clear_save_attempt_if_current(
                    state,
                    &mut save_key,
                    &mut save_body,
                    &capture,
                    &prepared_key,
                );
                let preview =
                    state.with_mut(|state| state.finish_save(&capture, saved.revision_id));
                schedule_editor_preview(state, preview);
            } else {
                state.with_mut(|state| {
                    state.fail_save(
                        &capture,
                        false,
                        "The save result is uncertain. Retry this save to confirm it.".to_owned(),
                    )
                });
            }
        }
        Ok(response) => {
            let error_code = serde_json::from_str::<ApiErrorBody>(&response.body)
                .map(|error| error.code)
                .unwrap_or_default();
            let stale = error_code == "conflict/edit_stale_revision";
            let definitive_client_error = (400..500).contains(&response.status);
            if definitive_client_error {
                clear_save_attempt_if_current(
                    state,
                    &mut save_key,
                    &mut save_body,
                    &capture,
                    &prepared_key,
                );
            }
            let message = if stale {
                "This funnel changed in another tab. Reload the current revision before saving."
                    .to_owned()
            } else if error_code == "conflict/revision_limit_reached" {
                "This funnel has reached its 100 revision limit.".to_owned()
            } else if response.status == 0 || response.status >= 500 {
                "The save result is uncertain. Retry the same save to confirm it.".to_owned()
            } else {
                error_message(&response.body)
            };
            state.with_mut(|state| state.fail_save(&capture, stale, message));
        }
        Err(()) => {
            state.with_mut(|state| {
                state.fail_save(
                    &capture,
                    false,
                    "The save result is uncertain. Retry the same save to confirm it.".to_owned(),
                )
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded_editor_state() -> EditorState {
        let mut state = EditorState::for_route("funnel-1".to_owned());
        state.finish_load(
            state.route_epoch,
            "funnel-1",
            Ok(EditorReadResponse {
                funnel: EditorFunnel {
                    id: "funnel-1".to_owned(),
                    name: "Offer".to_owned(),
                    slug: "offer".to_owned(),
                    updated_at: "now".to_owned(),
                },
                base_revision_id: "revision-1".to_owned(),
                sections: vec![
                    EditorSection {
                        id: "hero".to_owned(),
                        section_type: "hero".to_owned(),
                    },
                    EditorSection {
                        id: "features".to_owned(),
                        section_type: "features".to_owned(),
                    },
                    EditorSection {
                        id: "cta".to_owned(),
                        section_type: "cta".to_owned(),
                    },
                ],
                preview_html: "preview-1".to_owned(),
            }),
        );
        state
    }

    #[test]
    fn editor_pointer_and_keyboard_reordering_share_the_exact_order_transition() {
        let mut state = loaded_editor_state();
        let first_capture = state.move_section(0, 2).expect("pointer reorder");
        assert_eq!(first_capture.section_ids, ["features", "cta", "hero"]);
        assert!(state.start_keyboard_drag("features"));
        let keyboard_capture = state
            .move_section_by_id("features", 1)
            .expect("keyboard reorder");
        assert_eq!(keyboard_capture.section_ids, ["cta", "features", "hero"]);
        assert!(state.announcement.contains("Picked up Features"));
        let restore = state
            .cancel_keyboard_drag()
            .expect("Escape restores the pre-drag order");
        assert_eq!(restore.section_ids, ["features", "cta", "hero"]);
        assert_eq!(
            state.document.as_ref().map(|document| {
                document
                    .order
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
            }),
            Some(vec!["features", "cta", "hero"])
        );
        assert!(state.announcement.contains("original order was restored"));
    }

    #[test]
    fn editor_save_waits_until_keyboard_reorder_is_dropped_or_canceled() {
        let mut state = loaded_editor_state();
        assert!(state.start_keyboard_drag("hero"));
        state.move_section_by_id("hero", 1);

        assert!(state.begin_save().is_none());
        assert!(!state.saving);

        state.finish_keyboard_drag();
        let capture = state.begin_save().expect("dropped order can be saved");
        assert!(state.saving);
        assert_eq!(capture.section_ids, ["features", "hero", "cta"]);
    }

    #[test]
    fn keyboard_cancel_cannot_invalidate_an_in_flight_save() {
        let mut state = loaded_editor_state();
        state.start_keyboard_drag("hero");
        state.move_section_by_id("hero", 1);
        let order_during_drag = state
            .document
            .as_ref()
            .expect("loaded editor")
            .order
            .clone();
        let generation_during_drag = state.generation;
        state.saving = true;

        assert!(state.cancel_keyboard_drag().is_none());
        assert_eq!(state.generation, generation_during_drag);
        assert_eq!(
            state
                .document
                .as_ref()
                .map(|document| document.order.clone()),
            Some(order_during_drag)
        );
    }

    #[test]
    fn editor_preview_discards_old_generations_and_preserves_last_good_frame_on_error() {
        let mut state = loaded_editor_state();
        let old = state.move_section(0, 1).expect("first reorder");
        let current = state.move_section(1, 2).expect("second reorder");
        state.finish_preview(&old, Ok("stale-preview".to_owned()));
        assert_eq!(
            state
                .document
                .as_ref()
                .map(|document| document.preview_html.as_str()),
            Some("preview-1")
        );
        state.finish_preview(&current, Ok("current-preview".to_owned()));
        state.finish_preview(&current, Err(()));
        assert!(state.preview_error);
        assert_eq!(
            state
                .document
                .as_ref()
                .map(|document| document.preview_html.as_str()),
            Some("current-preview")
        );
    }

    #[test]
    fn successful_editor_save_adopts_its_revision_as_a_clean_base() {
        let mut state = loaded_editor_state();
        state.move_section(0, 2);
        let capture = state.begin_save().expect("dirty editor can save");
        assert!(state.saving);
        let preview = state
            .finish_save(&capture, "revision-2".to_owned())
            .expect("preview refresh");
        assert_eq!(preview.base_revision_id, "revision-2");
        assert_eq!(preview.section_ids, ["features", "cta", "hero"]);
        assert!(!state.can_save());
    }

    #[test]
    fn old_save_responses_cannot_mutate_a_different_route_or_revision() {
        let mut state = loaded_editor_state();
        state.move_section(0, 2);
        let capture = state.begin_save().expect("dirty editor can save");
        state.begin_route("funnel-2".to_owned());
        state.finish_load(
            state.route_epoch,
            "funnel-2",
            Ok(EditorReadResponse {
                funnel: EditorFunnel {
                    id: "funnel-2".to_owned(),
                    name: "Second funnel".to_owned(),
                    slug: "second".to_owned(),
                    updated_at: "now".to_owned(),
                },
                base_revision_id: "revision-20".to_owned(),
                sections: vec![EditorSection {
                    id: "hero-2".to_owned(),
                    section_type: "hero".to_owned(),
                }],
                preview_html: "second preview".to_owned(),
            }),
        );
        assert!(state
            .finish_save(&capture, "revision-2".to_owned())
            .is_none());
        assert!(!state.fail_save(&capture, false, "old response".to_owned()));
        assert_eq!(state.funnel_id, "funnel-2");
        assert_eq!(
            state
                .document
                .as_ref()
                .map(|document| document.base_revision_id.as_str()),
            Some("revision-20")
        );
        assert_eq!(state.save_error, None);
    }
}
