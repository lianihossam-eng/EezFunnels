use super::*;
use eez_domain::document_v2::{
    ActionV2, BlockV2, DocumentV2, Edit, FocusTarget, IdKind, IdSource, NewBlock, SectionKind,
};
#[cfg(feature = "browser")]
use eez_domain::document_v2::{BlockId, PageId, SectionId};
#[cfg(any(feature = "browser", test))]
use eez_domain::ThemePreset;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(feature = "browser")]
use uuid::Uuid;

#[cfg(feature = "browser")]
use super::super::{
    api_request, clear_idempotency_storage, error_message, prepare_idempotency_key,
    PreparedIdempotencyKey,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct V2EditorMetadata {
    schema_version: u32,
    funnel: EditorFunnel,
    base_revision_id: String,
    published_revision_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct V2RevisionSummary {
    revision_id: String,
    schema_version: i64,
    created_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct V2RevisionHistory {
    revisions: Vec<V2RevisionSummary>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct V2StageCapacity {
    orphan_byte_budget: u64,
    orphan_bytes_spent: u64,
    orphan_key_budget: u64,
    orphan_keys_spent: u64,
    worst_case_orphan_bytes: u64,
    worst_case_orphan_keys: u64,
}

impl V2StageCapacity {
    fn is_valid(&self) -> bool {
        self.orphan_byte_budget > 0
            && self.orphan_bytes_spent <= self.orphan_byte_budget
            && self.orphan_key_budget > 0
            && self.orphan_keys_spent <= self.orphan_key_budget
            && self.worst_case_orphan_bytes == self.orphan_byte_budget
            && self.worst_case_orphan_keys == self.orphan_key_budget
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct V2LocalDraft {
    version: u8,
    funnel_id: String,
    base_revision_id: String,
    document_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct V2EditorState {
    funnel_id: String,
    route_epoch: u64,
    generation: u64,
    loading: bool,
    load_error: bool,
    pending: bool,
    saving: bool,
    save_retryable: bool,
    stale: bool,
    funnel: Option<EditorFunnel>,
    base_revision_id: String,
    published_revision_id: Option<String>,
    document: Option<DocumentV2>,
    saved_document_json: String,
    preview_html: String,
    selected_page: String,
    selected_section: Option<String>,
    new_section_kind: SectionKind,
    new_block_kind: V2NewBlockKind,
    history: Vec<String>,
    history_cursor: usize,
    revisions: Vec<V2RevisionSummary>,
    stage_capacity: Option<V2StageCapacity>,
    compare_revision_id: Option<String>,
    compare_document: Option<DocumentV2>,
    compare_loading: bool,
    recovered_draft: Option<V2LocalDraft>,
    recovered_current_draft: bool,
    recovery_error: Option<String>,
    error: Option<String>,
    status: Option<String>,
    announcement: String,
    publication_pending: bool,
    publication_retryable: bool,
    #[cfg(feature = "browser")]
    publication_attempt: Option<PublicationAction>,
    publication_error: Option<String>,
    publication_status: Option<String>,
    unpublish_confirmation: bool,
    #[cfg(feature = "browser")]
    stage_key: Option<PreparedIdempotencyKey>,
    #[cfg(feature = "browser")]
    stage_body: Option<String>,
    #[cfg(feature = "browser")]
    save_key: Option<PreparedIdempotencyKey>,
    #[cfg(feature = "browser")]
    save_body: Option<String>,
    #[cfg(feature = "browser")]
    publication_key: Option<PreparedIdempotencyKey>,
    #[cfg(feature = "browser")]
    publication_body: Option<String>,
}

impl Default for V2EditorState {
    fn default() -> Self {
        Self {
            funnel_id: String::new(),
            route_epoch: 0,
            generation: 0,
            loading: false,
            load_error: false,
            pending: false,
            saving: false,
            save_retryable: false,
            stale: false,
            funnel: None,
            base_revision_id: String::new(),
            published_revision_id: None,
            document: None,
            saved_document_json: String::new(),
            preview_html: String::new(),
            selected_page: String::new(),
            selected_section: None,
            new_section_kind: SectionKind::Features,
            new_block_kind: V2NewBlockKind::default(),
            history: Vec::new(),
            history_cursor: 0,
            revisions: Vec::new(),
            stage_capacity: None,
            compare_revision_id: None,
            compare_document: None,
            compare_loading: false,
            recovered_draft: None,
            recovered_current_draft: false,
            recovery_error: None,
            error: None,
            status: None,
            announcement: String::new(),
            publication_pending: false,
            publication_retryable: false,
            #[cfg(feature = "browser")]
            publication_attempt: None,
            publication_error: None,
            publication_status: None,
            unpublish_confirmation: false,
            #[cfg(feature = "browser")]
            stage_key: None,
            #[cfg(feature = "browser")]
            stage_body: None,
            #[cfg(feature = "browser")]
            save_key: None,
            #[cfg(feature = "browser")]
            save_body: None,
            #[cfg(feature = "browser")]
            publication_key: None,
            #[cfg(feature = "browser")]
            publication_body: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum V2NewBlockKind {
    #[default]
    Heading,
    Text,
    List,
    Button,
}

impl V2NewBlockKind {
    const ALL: [Self; 4] = [Self::Heading, Self::Text, Self::List, Self::Button];

    const fn id(self) -> &'static str {
        match self {
            Self::Heading => "heading",
            Self::Text => "text",
            Self::List => "list",
            Self::Button => "button",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Heading => "Heading",
            Self::Text => "Text",
            Self::List => "List",
            Self::Button => "Button",
        }
    }

    fn from_id(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.id() == value)
    }

    fn create(self) -> NewBlock {
        match self {
            Self::Heading => NewBlock::Heading {
                level: 2,
                text: "New heading".to_owned(),
            },
            Self::Text => NewBlock::Text {
                text: "Add your text here.".to_owned(),
            },
            Self::List => NewBlock::List {
                items: vec!["First item".to_owned(), "Second item".to_owned()],
            },
            Self::Button => NewBlock::Button {
                label: "Continue".to_owned(),
                action: ActionV2::ExternalHttps {
                    url: "https://example.com".to_owned(),
                },
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct V2EditCapture {
    route_epoch: u64,
    generation: u64,
    funnel_id: String,
    base_revision_id: String,
    page_id: String,
    operation_id: String,
    edit: Edit,
    base_document_json: String,
    document_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct V2SaveCapture {
    route_epoch: u64,
    generation: u64,
    funnel_id: String,
    base_revision_id: String,
    document_json: String,
}

struct DeterministicIds {
    operation_id: String,
    next: u32,
}

impl IdSource for DeterministicIds {
    fn next_id(&mut self, _kind: IdKind) -> Result<String, eez_domain::document_v2::DocumentError> {
        self.next = self.next.wrapping_add(1);
        Ok(format!("{}-{}", self.operation_id, self.next))
    }
}

impl V2EditorState {
    fn for_route(funnel_id: String) -> Self {
        Self {
            funnel_id,
            loading: true,
            new_section_kind: SectionKind::Features,
            new_block_kind: V2NewBlockKind::Text,
            ..Self::default()
        }
    }

    fn begin_route(&mut self, funnel_id: String) -> u64 {
        let route_epoch = self.route_epoch.wrapping_add(1);
        *self = Self::for_route(funnel_id);
        self.route_epoch = route_epoch;
        route_epoch
    }

    fn finish_load(
        &mut self,
        route_epoch: u64,
        funnel_id: &str,
        loaded: Result<V2LoadedEditor, ()>,
    ) {
        if self.route_epoch != route_epoch || self.funnel_id != funnel_id {
            return;
        }
        self.loading = false;
        match loaded {
            Ok(loaded) if loaded.metadata.funnel.id == funnel_id => {
                self.funnel = Some(loaded.metadata.funnel);
                self.base_revision_id = loaded.metadata.base_revision_id;
                self.published_revision_id = loaded.metadata.published_revision_id;
                self.saved_document_json = loaded.saved_document_json.clone();
                self.document = Some(loaded.document.clone());
                self.preview_html = loaded.preview_html;
                self.selected_page = loaded.selected_page;
                self.selected_section = loaded.selected_section;
                self.revisions = loaded.revisions;
                self.stage_capacity = Some(loaded.stage_capacity);
                self.recovered_draft = loaded.recovered_draft;
                self.recovered_current_draft = loaded.recovered_current_draft;
                self.recovery_error = loaded.recovery_error;
                self.history = vec![self.saved_document_json.clone()];
                self.history_cursor = 0;
                if self.recovered_current_draft {
                    if let Some(document) = &self.document {
                        self.push_history(document.canonical_json());
                    }
                    self.status = Some("Recovered the unsaved draft from this browser.".to_owned());
                }
                self.load_error = false;
            }
            _ => self.load_error = true,
        }
    }

    fn has_unsaved_changes(&self) -> bool {
        self.document
            .as_ref()
            .is_some_and(|document| document.canonical_json() != self.saved_document_json)
    }

    fn can_edit(&self) -> bool {
        self.document.is_some() && !self.loading && !self.pending && !self.saving && !self.stale
    }

    fn can_undo(&self) -> bool {
        self.can_edit() && self.history_cursor > 0
    }

    fn can_redo(&self) -> bool {
        self.can_edit() && self.history_cursor + 1 < self.history.len()
    }

    fn can_save(&self) -> bool {
        self.document.as_ref().is_some_and(|document| {
            !self.loading
                && !self.pending
                && !self.saving
                && !self.stale
                && !document.has_images()
                && (self.save_retryable || self.has_unsaved_changes())
        })
    }

    fn can_publish(&self) -> bool {
        !self.loading
            && !self.pending
            && !self.saving
            && !self.stale
            && !self.publication_pending
            && !self.has_unsaved_changes()
            && self.document.is_some()
            && self.published_revision_id.as_deref() != Some(&self.base_revision_id)
    }

    fn can_unpublish(&self) -> bool {
        self.published_revision_id.is_some()
            && !self.publication_pending
            && !self.publication_retryable
            && !self.loading
    }

    fn apply_local_edit(&mut self, edit: Edit, operation_id: String) -> Option<V2EditCapture> {
        if !self.can_edit() {
            return None;
        }
        let document = self.document.as_ref()?.clone();
        let base_document_json = document.canonical_json();
        let mut ids = DeterministicIds {
            operation_id: operation_id.clone(),
            next: 0,
        };
        let outcome = document.apply(edit.clone(), &mut ids).ok()?;
        let document_json = outcome.document.canonical_json();
        if document_json == document.canonical_json() {
            return None;
        }
        self.select_focus(&outcome.focus);
        self.document = Some(outcome.document);
        self.push_history(document_json.clone());
        self.generation = self.generation.wrapping_add(1);
        self.pending = true;
        self.error = None;
        self.status =
            Some("Draft change saved in this browser; checking the shared command…".to_owned());
        Some(V2EditCapture {
            route_epoch: self.route_epoch,
            generation: self.generation,
            funnel_id: self.funnel_id.clone(),
            base_revision_id: self.base_revision_id.clone(),
            page_id: self.selected_page.clone(),
            operation_id,
            edit,
            base_document_json,
            document_json,
        })
    }

    #[cfg(feature = "browser")]
    fn finish_edit(
        &mut self,
        capture: &V2EditCapture,
        response: Result<V2EditResponse, V2RequestFailure>,
    ) {
        if !self.is_current_edit(capture) {
            return;
        }
        self.pending = false;
        match response {
            Ok(response)
                if response.base_revision_id == capture.base_revision_id
                    && response.document_json == capture.document_json =>
            {
                self.preview_html = response.preview_html;
                self.status = Some("Draft change verified by the Rust command.".to_owned());
                self.error = None;
            }
            Ok(_) => {
                self.stale = true;
                self.error = Some(
                    "The Worker returned a different draft. Your browser copy is preserved."
                        .to_owned(),
                );
            }
            Err(failure) => {
                self.stale = failure.code.as_deref() == Some("conflict/edit_stale_revision");
                self.error = Some(failure.message);
                self.status = None;
            }
        }
    }

    #[cfg(feature = "browser")]
    fn is_current_edit(&self, capture: &V2EditCapture) -> bool {
        self.route_epoch == capture.route_epoch
            && self.generation == capture.generation
            && self.funnel_id == capture.funnel_id
            && self.base_revision_id == capture.base_revision_id
            && self.pending
    }

    fn apply_history(&mut self, offset: isize) -> Option<(u64, String, String, String)> {
        if !self.can_edit() {
            return None;
        }
        let next = self.history_cursor.checked_add_signed(offset)?;
        let snapshot = self.history.get(next)?.clone();
        let document = DocumentV2::decode(snapshot.as_bytes(), &self.funnel_id).ok()?;
        self.history_cursor = next;
        self.selected_page = document.entry_page_id().as_str().to_owned();
        self.selected_section = document
            .pages()
            .iter()
            .find(|page| page.id().as_str() == self.selected_page)
            .and_then(|page| page.sections().first())
            .map(|section| section.id().as_str().to_owned());
        self.document = Some(document);
        self.generation = self.generation.wrapping_add(1);
        self.error = None;
        self.status = Some(if offset < 0 {
            "Undid the latest draft change.".to_owned()
        } else {
            "Redid the draft change.".to_owned()
        });
        Some((
            self.generation,
            self.funnel_id.clone(),
            self.base_revision_id.clone(),
            snapshot,
        ))
    }

    fn select_page(&mut self, page_id: &str) -> Option<(u64, String, String, String)> {
        let document = self.document.as_ref()?;
        let page = document
            .pages()
            .iter()
            .find(|page| page.id().as_str() == page_id)?;
        self.selected_page = page_id.to_owned();
        self.selected_section = page
            .sections()
            .first()
            .map(|section| section.id().as_str().to_owned());
        self.generation = self.generation.wrapping_add(1);
        Some((
            self.generation,
            self.funnel_id.clone(),
            self.base_revision_id.clone(),
            document.canonical_json(),
        ))
    }

    fn restore_document(&mut self, document: DocumentV2) -> Option<(u64, String, String, String)> {
        if !self.can_edit() || document.funnel_id().as_str() != self.funnel_id {
            return None;
        }
        let snapshot = document.canonical_json();
        if self
            .document
            .as_ref()
            .is_some_and(|current| current.canonical_json() == snapshot)
        {
            return None;
        }
        self.selected_page = document.entry_page_id().as_str().to_owned();
        self.selected_section = document
            .pages()
            .iter()
            .find(|page| page.id().as_str() == self.selected_page)
            .and_then(|page| page.sections().first())
            .map(|section| section.id().as_str().to_owned());
        self.document = Some(document);
        self.push_history(snapshot.clone());
        self.generation = self.generation.wrapping_add(1);
        self.status =
            Some("Restored as an editable draft; saving will create a new revision.".to_owned());
        self.recovered_draft = None;
        Some((
            self.generation,
            self.funnel_id.clone(),
            self.base_revision_id.clone(),
            snapshot,
        ))
    }

    fn begin_save(&mut self) -> Option<V2SaveCapture> {
        if !self.can_save() {
            return None;
        }
        let capture = V2SaveCapture {
            route_epoch: self.route_epoch,
            generation: self.generation,
            funnel_id: self.funnel_id.clone(),
            base_revision_id: self.base_revision_id.clone(),
            document_json: self.document.as_ref()?.canonical_json(),
        };
        self.saving = true;
        self.save_retryable = false;
        self.error = None;
        self.status = Some("Saving a new immutable revision…".to_owned());
        Some(capture)
    }

    #[cfg(feature = "browser")]
    fn finish_save(
        &mut self,
        capture: &V2SaveCapture,
        response: Result<V2SaveResponse, V2RequestFailure>,
    ) {
        if !self.is_current_save(capture) {
            return;
        }
        self.saving = false;
        match response {
            Ok(response)
                if response.funnel_id == capture.funnel_id
                    && response.base_revision_id == capture.base_revision_id
                    && response.schema_version == 2
                    && !response.revision_id.is_empty() =>
            {
                self.base_revision_id = response.revision_id;
                self.saved_document_json = capture.document_json.clone();
                self.history = vec![self.saved_document_json.clone()];
                self.history_cursor = 0;
                self.save_retryable = false;
                self.stale = false;
                self.recovered_current_draft = false;
                self.recovered_draft = None;
                self.recovery_error = None;
                self.error = None;
                self.status = Some(
                    "Revision saved. The previous revisions remain available in history."
                        .to_owned(),
                );
                self.revisions.insert(
                    0,
                    V2RevisionSummary {
                        revision_id: self.base_revision_id.clone(),
                        schema_version: 2,
                        created_at: "Just now".to_owned(),
                    },
                );
            }
            Ok(_) => {
                self.error =
                    Some("The save response did not match this funnel revision.".to_owned());
                self.status = None;
            }
            Err(failure) => {
                self.stale = failure.code.as_deref() == Some("conflict/edit_stale_revision");
                self.save_retryable = failure.retryable && !self.stale;
                self.error = Some(failure.message);
                self.status = None;
            }
        }
    }

    #[cfg(feature = "browser")]
    fn is_current_save(&self, capture: &V2SaveCapture) -> bool {
        self.route_epoch == capture.route_epoch
            && self.generation == capture.generation
            && self.funnel_id == capture.funnel_id
            && self.base_revision_id == capture.base_revision_id
            && self.saving
    }

    fn select_section(&mut self, section_id: &str) {
        let exists = self.document.as_ref().is_some_and(|document| {
            document
                .pages()
                .iter()
                .flat_map(|page| page.sections())
                .any(|section| section.id().as_str() == section_id)
        });
        if exists {
            self.selected_section = Some(section_id.to_owned());
        }
    }

    fn push_history(&mut self, snapshot: String) {
        if self
            .history
            .get(self.history_cursor)
            .is_some_and(|current| current == &snapshot)
        {
            return;
        }
        self.history.truncate(self.history_cursor.saturating_add(1));
        self.history.push(snapshot);
        if self.history.len() > EDITOR_HISTORY_LIMIT {
            self.history.remove(0);
        }
        self.history_cursor = self.history.len().saturating_sub(1);
    }

    fn select_focus(&mut self, focus: &FocusTarget) {
        match focus {
            FocusTarget::Page(page) => self.selected_page = page.as_str().to_owned(),
            FocusTarget::Section(section) => {
                self.selected_section = Some(section.as_str().to_owned())
            }
            FocusTarget::Block(_) => {}
        }
    }
}

const EDITOR_HISTORY_LIMIT: usize = 100;

trait CanonicalDocumentJson {
    fn canonical_json(&self) -> String;
}

impl CanonicalDocumentJson for DocumentV2 {
    fn canonical_json(&self) -> String {
        String::from_utf8(self.canonical_bytes().to_vec()).unwrap_or_default()
    }
}

struct V2LoadedEditor {
    metadata: V2EditorMetadata,
    document: DocumentV2,
    saved_document_json: String,
    preview_html: String,
    selected_page: String,
    selected_section: Option<String>,
    revisions: Vec<V2RevisionSummary>,
    stage_capacity: V2StageCapacity,
    recovered_draft: Option<V2LocalDraft>,
    recovered_current_draft: bool,
    recovery_error: Option<String>,
}

#[cfg(feature = "browser")]
#[derive(Clone, Debug, Deserialize)]
struct V2EditResponse {
    base_revision_id: String,
    document_json: String,
    preview_html: String,
}

#[cfg(feature = "browser")]
#[derive(Clone, Debug, Deserialize)]
struct V2PreviewResponse {
    preview_html: String,
}

#[cfg(feature = "browser")]
#[derive(Clone, Debug, Deserialize)]
struct V2StageResponse {
    stage_id: String,
}

#[cfg(feature = "browser")]
#[derive(Clone, Debug, Deserialize)]
struct V2SaveResponse {
    funnel_id: String,
    base_revision_id: String,
    revision_id: String,
    schema_version: u32,
}

#[cfg(feature = "browser")]
#[derive(Clone, Debug, Default, Deserialize)]
struct V2RequestFailure {
    code: Option<String>,
    retryable: bool,
    message: String,
}

#[cfg(feature = "browser")]
#[derive(Clone, Debug, Deserialize)]
struct V2LocalStorageResponse {
    ok: bool,
    value: Option<String>,
}

#[cfg(feature = "browser")]
const V2_DRAFT_STORAGE: &str = r#"
const request = await dioxus.recv();
try {
    if (request.action === "read") {
        dioxus.send({ok: true, value: localStorage.getItem(request.key)});
    } else if (request.action === "write") {
        localStorage.setItem(request.key, JSON.stringify(request.value));
        dioxus.send({ok: true, value: null});
    } else if (request.action === "remove") {
        localStorage.removeItem(request.key);
        dioxus.send({ok: true, value: null});
    } else {
        dioxus.send({ok: false, value: null});
    }
} catch (_) {
    dioxus.send({ok: false, value: null});
}
"#;

#[cfg(feature = "browser")]
const V2_BINARY_UPLOAD: &str = r#"
const request = await dioxus.recv();
try {
    const bytes = new TextEncoder().encode(request.document_json);
    const response = await fetch(request.path, {
        method: "PUT",
        headers: {
            "Authorization": `Bearer ${request.stage_id}`,
            "Content-Type": "application/octet-stream"
        },
        body: bytes
    });
    dioxus.send({status: response.status, body: await response.text()});
} catch (_) {
    dioxus.send({status: 0, body: ""});
}
"#;

#[cfg(feature = "browser")]
async fn load_v2_editor(funnel_id: &str) -> Result<V2LoadedEditor, ()> {
    let metadata_response = api_request("GET", &format!("/api/funnels/{funnel_id}"), None, None)
        .await
        .map_err(|_| ())?;
    if metadata_response.status != 200 {
        return Err(());
    }
    let metadata: V2EditorMetadata =
        serde_json::from_str(&metadata_response.body).map_err(|_| ())?;
    if metadata.schema_version != 2 || metadata.funnel.id != funnel_id {
        return Err(());
    }
    let capacity_response = api_request("GET", "/api/v2/stage-capacity", None, None)
        .await
        .map_err(|_| ())?;
    if capacity_response.status != 200 {
        return Err(());
    }
    let stage_capacity: V2StageCapacity =
        serde_json::from_str(&capacity_response.body).map_err(|_| ())?;
    if !stage_capacity.is_valid() {
        return Err(());
    }
    let document_response = api_request(
        "GET",
        &format!("/api/funnels/{funnel_id}/document"),
        None,
        None,
    )
    .await
    .map_err(|_| ())?;
    if document_response.status != 200 {
        return Err(());
    }
    let saved_document =
        DocumentV2::decode(document_response.body.as_bytes(), funnel_id).map_err(|_| ())?;
    let saved_document_json = saved_document.canonical_json();
    let history_response = api_request(
        "GET",
        &format!("/api/funnels/{funnel_id}/revisions-v2"),
        None,
        None,
    )
    .await
    .map_err(|_| ())?;
    if history_response.status != 200 {
        return Err(());
    }
    let revisions: Vec<V2RevisionSummary> =
        serde_json::from_str::<V2RevisionHistory>(&history_response.body)
            .map_err(|_| ())?
            .revisions
            .into_iter()
            .filter(|revision| revision.schema_version == 2)
            .collect();

    let mut document = saved_document.clone();
    let mut recovered_draft = None;
    let mut recovered_current_draft = false;
    let mut recovery_error = None;
    if let Some(raw_draft) = read_v2_local_draft(funnel_id).await {
        match serde_json::from_str::<V2LocalDraft>(&raw_draft) {
            Ok(draft) if draft.version == 1 && draft.funnel_id == funnel_id => {
                match DocumentV2::decode(draft.document_json.as_bytes(), funnel_id) {
                    Ok(local_document)
                        if draft.base_revision_id == metadata.base_revision_id
                            && local_document.canonical_json() != saved_document_json =>
                    {
                        document = local_document;
                        recovered_current_draft = true;
                    }
                    Ok(local_document)
                        if draft.base_revision_id != metadata.base_revision_id
                            && local_document.canonical_json() != saved_document_json =>
                    {
                        recovered_draft = Some(draft);
                    }
                    Ok(_) => {
                        let _ = remove_v2_local_draft(funnel_id).await;
                    }
                    Err(_) => {
                        recovery_error =
                            Some("The saved browser draft is invalid and was preserved.".to_owned())
                    }
                }
            }
            Ok(_) => {
                recovery_error = Some(
                    "The saved browser draft belongs to a different version or funnel.".to_owned(),
                )
            }
            Err(_) => {
                recovery_error =
                    Some("The saved browser draft could not be read and was preserved.".to_owned())
            }
        }
    }
    let selected_page = document.entry_page_id().as_str().to_owned();
    let selected_section = document
        .pages()
        .iter()
        .find(|page| page.id().as_str() == selected_page)
        .and_then(|page| page.sections().first())
        .map(|section| section.id().as_str().to_owned());
    let preview_html = fetch_v2_preview(
        funnel_id,
        &metadata.base_revision_id,
        &selected_page,
        &document.canonical_json(),
    )
    .await
    .map_err(|_| ())?;
    Ok(V2LoadedEditor {
        metadata,
        document,
        saved_document_json,
        preview_html,
        selected_page,
        selected_section,
        revisions,
        stage_capacity,
        recovered_draft,
        recovered_current_draft,
        recovery_error,
    })
}

#[cfg(feature = "browser")]
async fn fetch_v2_preview(
    funnel_id: &str,
    base_revision_id: &str,
    page_id: &str,
    document_json: &str,
) -> Result<String, ()> {
    let body = serde_json::json!({
        "base_revision_id": base_revision_id,
        "page_id": page_id,
        "document_json": document_json,
    })
    .to_string();
    let response = api_request(
        "POST",
        &format!("/api/funnels/{funnel_id}/preview-v2"),
        Some(body),
        None,
    )
    .await
    .map_err(|_| ())?;
    if response.status != 200 {
        return Err(());
    }
    serde_json::from_str::<V2PreviewResponse>(&response.body)
        .map(|response| response.preview_html)
        .map_err(|_| ())
}

#[cfg(feature = "browser")]
async fn read_v2_local_draft(funnel_id: &str) -> Option<String> {
    let mut request = document::eval(V2_DRAFT_STORAGE);
    request
        .send(serde_json::json!({
            "action": "read",
            "key": v2_draft_key(funnel_id),
        }))
        .ok()?;
    let response = request.recv::<V2LocalStorageResponse>().await.ok()?;
    (response.ok).then_some(response.value).flatten()
}

#[cfg(feature = "browser")]
async fn write_v2_local_draft(draft: &V2LocalDraft) -> bool {
    let Ok(value) = serde_json::to_value(draft) else {
        return false;
    };
    let mut request = document::eval(V2_DRAFT_STORAGE);
    if request
        .send(serde_json::json!({
            "action": "write",
            "key": v2_draft_key(&draft.funnel_id),
            "value": value,
        }))
        .is_err()
    {
        return false;
    }
    request
        .recv::<V2LocalStorageResponse>()
        .await
        .is_ok_and(|response| response.ok)
}

#[cfg(feature = "browser")]
async fn remove_v2_local_draft(funnel_id: &str) -> bool {
    let mut request = document::eval(V2_DRAFT_STORAGE);
    if request
        .send(serde_json::json!({
            "action": "remove",
            "key": v2_draft_key(funnel_id),
        }))
        .is_err()
    {
        return false;
    }
    request
        .recv::<V2LocalStorageResponse>()
        .await
        .is_ok_and(|response| response.ok)
}

fn v2_draft_key(funnel_id: &str) -> String {
    format!("eezfunnel:draft:v2:{funnel_id}")
}

#[cfg(feature = "browser")]
async fn upload_v2_stage(
    funnel_id: &str,
    stage_id: &str,
    document_json: &str,
) -> Result<super::super::BrowserResponse, ()> {
    let mut request = document::eval(V2_BINARY_UPLOAD);
    request
        .send(serde_json::json!({
            "path": format!("/api/funnels/{funnel_id}/stages-v2"),
            "stage_id": stage_id,
            "document_json": document_json,
        }))
        .map_err(|_| ())?;
    request.recv().await.map_err(|_| ())
}

#[cfg(feature = "browser")]
fn v2_failure(response: &super::super::BrowserResponse) -> V2RequestFailure {
    let code = serde_json::from_str::<serde_json::Value>(&response.body)
        .ok()
        .and_then(|value| {
            value
                .get("code")
                .and_then(|code| code.as_str())
                .map(str::to_owned)
        });
    V2RequestFailure {
        code,
        retryable: response.status == 0 || response.status >= 500,
        message: if response.status == 0 || response.status >= 500 {
            "The result is uncertain. Retry the same action; the draft remains in this browser."
                .to_owned()
        } else {
            error_message(&response.body)
        },
    }
}

#[cfg(feature = "browser")]
async fn load_v2_editor_state(
    mut state: Signal<V2EditorState>,
    funnel_id: String,
    route_epoch: u64,
) {
    let loaded = load_v2_editor(&funnel_id).await;
    if state.try_peek().is_ok() {
        state.with_mut(|state| state.finish_load(route_epoch, &funnel_id, loaded));
    }
}

#[cfg(feature = "browser")]
fn dispatch_v2_edit(mut state: Signal<V2EditorState>, edit: Edit) {
    let operation_id = Uuid::new_v4().simple().to_string();
    let capture = state.with_mut(|state| state.apply_local_edit(edit, operation_id));
    let Some(capture) = capture else {
        return;
    };
    let draft = V2LocalDraft {
        version: 1,
        funnel_id: capture.funnel_id.clone(),
        base_revision_id: capture.base_revision_id.clone(),
        document_json: capture.document_json.clone(),
    };
    spawn(async move {
        let local_write_succeeded = write_v2_local_draft(&draft).await;
        let body = serde_json::json!({
            "base_revision_id": capture.base_revision_id,
            "page_id": capture.page_id,
            "document_json": capture.base_document_json,
            "operation_id": capture.operation_id,
            "edit": capture.edit,
        })
        .to_string();
        let response = api_request(
            "POST",
            &format!("/api/funnels/{}/edit-v2", capture.funnel_id),
            Some(body),
            None,
        )
        .await;
        let result = match response {
            Ok(response) if response.status == 200 => {
                serde_json::from_str::<V2EditResponse>(&response.body)
                    .map_err(|_| V2RequestFailure {
                        code: None,
                        retryable: false,
                        message: "The Worker returned an unreadable edit result. The local draft is preserved.".to_owned(),
                    })
            }
            Ok(response) => Err(v2_failure(&response)),
            Err(()) => Err(V2RequestFailure {
                code: None,
                retryable: true,
                message: "The edit result is uncertain. The local draft is preserved; retry after checking the connection.".to_owned(),
            }),
        };
        if state.try_peek().is_ok() {
            state.with_mut(|state| {
                state.finish_edit(&capture, result);
                if !local_write_succeeded {
                    state.recovery_error =
                        Some("The browser could not persist this draft locally.".to_owned());
                }
            });
        }
    });
}

#[cfg(feature = "browser")]
fn schedule_v2_preview(
    mut state: Signal<V2EditorState>,
    request: Option<(u64, String, String, String)>,
) {
    let Some((generation, funnel_id, base_revision_id, document_json)) = request else {
        return;
    };
    let page_id = state.with(|state| state.selected_page.clone());
    spawn(async move {
        let result =
            fetch_v2_preview(&funnel_id, &base_revision_id, &page_id, &document_json).await;
        if state.try_peek().is_ok() {
            state.with_mut(|state| {
                if state.generation == generation && state.base_revision_id == base_revision_id {
                    match result {
                        Ok(preview_html) => {
                            state.preview_html = preview_html;
                            state.status = Some("Draft preview refreshed.".to_owned());
                        }
                        Err(()) => {
                            state.error =
                                Some("The draft preview could not be refreshed.".to_owned())
                        }
                    }
                }
            });
        }
    });
}

#[cfg(feature = "browser")]
async fn save_v2_revision(mut state: Signal<V2EditorState>) {
    let capture = state.with_mut(V2EditorState::begin_save);
    let Some(capture) = capture else {
        return;
    };
    let digest = sha256_hex(capture.document_json.as_bytes());
    let stage_body = serde_json::json!({
        "base_revision_id": capture.base_revision_id,
        "expected_length": capture.document_json.len(),
        "expected_sha256": digest,
    })
    .to_string();
    let stage_path = format!("/api/funnels/{}/stages-v2", capture.funnel_id);
    let previous_stage_key = state.with(|state| {
        (state.stage_body.as_deref() == Some(stage_body.as_str()))
            .then(|| state.stage_key.clone())
            .flatten()
    });
    let stage_key =
        prepare_idempotency_key("POST", &stage_path, Some(&stage_body), previous_stage_key);
    let stage_key = match stage_key {
        Ok(key) => key,
        Err(()) => {
            state.with_mut(|state| {
                state.saving = false;
                state.error =
                    Some("The browser could not prepare a retry-safe stage request.".to_owned());
                state.status = None;
            });
            return;
        }
    };
    state.with_mut(|state| {
        state.stage_key = Some(stage_key.clone());
        state.stage_body = Some(stage_body.clone());
    });
    let stage_response = api_request(
        "POST",
        &stage_path,
        Some(stage_body),
        Some(stage_key.clone()),
    )
    .await;
    let stage_id = match stage_response {
        Ok(response) if response.status == 201 => {
            match serde_json::from_str::<V2StageResponse>(&response.body) {
                Ok(stage) => stage.stage_id,
                Err(_) => {
                    fail_v2_save(
                        state,
                        &capture,
                        V2RequestFailure {
                            code: None,
                            retryable: false,
                            message:
                                "The stage response was invalid. Your browser draft is preserved."
                                    .to_owned(),
                        },
                    );
                    return;
                }
            }
        }
        Ok(response) => {
            fail_v2_save(state, &capture, v2_failure(&response));
            return;
        }
        Err(()) => {
            fail_v2_save(
                state,
                &capture,
                V2RequestFailure {
                    code: None,
                    retryable: true,
                    message: "The stage request result is uncertain. Retry the same save."
                        .to_owned(),
                },
            );
            return;
        }
    };
    let upload_response =
        upload_v2_stage(&capture.funnel_id, &stage_id, &capture.document_json).await;
    match upload_response {
        Ok(response) if response.status == 204 => {}
        Ok(response) => {
            fail_v2_save(state, &capture, v2_failure(&response));
            return;
        }
        Err(()) => {
            fail_v2_save(
                state,
                &capture,
                V2RequestFailure {
                    code: None,
                    retryable: true,
                    message: "The upload result is uncertain. Retry the same save to confirm it."
                        .to_owned(),
                },
            );
            return;
        }
    }
    let save_body = serde_json::json!({
        "base_revision_id": capture.base_revision_id,
        "stage_id": stage_id,
        "expected_length": capture.document_json.len(),
        "expected_sha256": digest,
    })
    .to_string();
    let save_path = format!("/api/funnels/{}/revisions-v2", capture.funnel_id);
    let previous_save_key = state.with(|state| {
        (state.save_body.as_deref() == Some(save_body.as_str()))
            .then(|| state.save_key.clone())
            .flatten()
    });
    let save_key =
        match prepare_idempotency_key("POST", &save_path, Some(&save_body), previous_save_key) {
            Ok(key) => key,
            Err(()) => {
                fail_v2_save(
                    state,
                    &capture,
                    V2RequestFailure {
                        code: None,
                        retryable: false,
                        message: "The browser could not prepare a retry-safe final save."
                            .to_owned(),
                    },
                );
                return;
            }
        };
    state.with_mut(|state| {
        state.save_key = Some(save_key.clone());
        state.save_body = Some(save_body.clone());
    });
    let save_response =
        api_request("POST", &save_path, Some(save_body), Some(save_key.clone())).await;
    let result = match save_response {
        Ok(response) if response.status == 201 => {
            serde_json::from_str::<V2SaveResponse>(&response.body).map_err(|_| V2RequestFailure {
                code: None,
                retryable: false,
                message: "The save response was invalid. The local draft remains available."
                    .to_owned(),
            })
        }
        Ok(response) => Err(v2_failure(&response)),
        Err(()) => Err(V2RequestFailure {
            code: None,
            retryable: true,
            message: "The final save result is uncertain. Retry the same action to confirm it."
                .to_owned(),
        }),
    };
    let succeeded = result.is_ok();
    if state.try_peek().is_ok() {
        state.with_mut(|state| state.finish_save(&capture, result));
    }
    if succeeded {
        let _ = clear_idempotency_storage(&stage_key);
        let _ = clear_idempotency_storage(&save_key);
        let _ = remove_v2_local_draft(&capture.funnel_id).await;
        if state.try_peek().is_ok() {
            state.with_mut(|state| {
                state.stage_key = None;
                state.stage_body = None;
                state.save_key = None;
                state.save_body = None;
            });
        }
    }
}

#[cfg(feature = "browser")]
fn fail_v2_save(
    mut state: Signal<V2EditorState>,
    capture: &V2SaveCapture,
    failure: V2RequestFailure,
) {
    if state.try_peek().is_ok() && state.with(|state| state.is_current_save(capture)) {
        state.with_mut(|state| {
            state.finish_save(capture, Err(failure));
        });
    }
}

#[cfg(feature = "browser")]
async fn compare_v2_revision(mut state: Signal<V2EditorState>, revision_id: String) {
    if revision_id.is_empty() {
        return;
    }
    state.with_mut(|state| {
        state.compare_loading = true;
        state.compare_revision_id = Some(revision_id.clone());
        state.compare_document = None;
        state.error = None;
    });
    let response = api_request(
        "GET",
        &format!(
            "/api/funnels/{}/document?revision_id={revision_id}",
            state.with(|state| state.funnel_id.clone())
        ),
        None,
        None,
    )
    .await;
    let result = match response {
        Ok(response) if response.status == 200 => {
            let funnel_id = state.with(|state| state.funnel_id.clone());
            DocumentV2::decode(response.body.as_bytes(), &funnel_id).map_err(|_| ())
        }
        _ => Err(()),
    };
    if state.try_peek().is_ok() {
        state.with_mut(|state| {
            state.compare_loading = false;
            match result {
                Ok(document) => state.compare_document = Some(document),
                Err(()) => {
                    state.error =
                        Some("That revision could not be loaded for comparison.".to_owned())
                }
            }
        });
    }
}

fn section_kind_label(kind: SectionKind) -> &'static str {
    match kind {
        SectionKind::Hero => "Hero",
        SectionKind::Features => "Features",
        SectionKind::Testimonials => "Testimonials",
        SectionKind::Cta => "Call to action",
        SectionKind::Footer => "Footer",
    }
}

fn section_kind_from_id(value: &str) -> Option<SectionKind> {
    match value {
        "hero" => Some(SectionKind::Hero),
        "features" => Some(SectionKind::Features),
        "testimonials" => Some(SectionKind::Testimonials),
        "cta" => Some(SectionKind::Cta),
        "footer" => Some(SectionKind::Footer),
        _ => None,
    }
}

fn block_kind_label(block: &BlockV2) -> &'static str {
    match block {
        BlockV2::Heading { .. } => "Heading",
        BlockV2::Text { .. } => "Text",
        BlockV2::List { .. } => "List",
        BlockV2::Button { .. } => "Button",
        BlockV2::Image { .. } => "Image",
        BlockV2::Form { .. } => "Form",
    }
}

fn block_text(block: &BlockV2) -> String {
    match block {
        BlockV2::Heading { text, .. } | BlockV2::Text { text, .. } => text.clone(),
        BlockV2::List { items, .. } => items.join("\n"),
        BlockV2::Button { label, .. } => label.clone(),
        BlockV2::Image { .. } => "Image assets are added in the media library phase.".to_owned(),
        BlockV2::Form { .. } => "Form editing is added in the forms phase.".to_owned(),
    }
}

fn block_value_with_text(block: &BlockV2, text: String) -> Option<NewBlock> {
    match block {
        BlockV2::Heading { level, .. } => Some(NewBlock::Heading {
            level: *level,
            text,
        }),
        BlockV2::Text { .. } => Some(NewBlock::Text { text }),
        BlockV2::List { .. } => Some(NewBlock::List {
            items: text.lines().map(str::to_owned).collect(),
        }),
        BlockV2::Button { action, .. } => Some(NewBlock::Button {
            label: text,
            action: action.clone(),
        }),
        BlockV2::Image { asset_id, alt, .. } => Some(NewBlock::Image {
            asset_id: asset_id.clone(),
            alt: alt.clone(),
        }),
        BlockV2::Form { form_id, .. } => Some(NewBlock::Form {
            form_id: form_id.clone(),
        }),
    }
}

fn revision_comparison(current: &DocumentV2, previous: &DocumentV2) -> Vec<String> {
    let mut changes = Vec::new();
    if current.theme() != previous.theme() {
        changes.push("The theme changed.".to_owned());
    }
    let current_pages = current
        .pages()
        .iter()
        .map(|page| (page.id().as_str().to_owned(), page))
        .collect::<BTreeMap<_, _>>();
    let previous_pages = previous
        .pages()
        .iter()
        .map(|page| (page.id().as_str().to_owned(), page))
        .collect::<BTreeMap<_, _>>();
    for (id, page) in &current_pages {
        match previous_pages.get(id) {
            None => changes.push(format!("Page added: {} ({id}).", page.title())),
            Some(old) if *old != *page => {
                changes.push(format!("Page changed: {} ({id}).", page.title()))
            }
            Some(_) => {}
        }
    }
    for (id, page) in &previous_pages {
        if !current_pages.contains_key(id) {
            changes.push(format!("Page removed: {} ({id}).", page.title()));
        }
    }
    let current_sections = current
        .pages()
        .iter()
        .flat_map(|page| page.sections())
        .map(|section| (section.id().as_str().to_owned(), section))
        .collect::<BTreeMap<_, _>>();
    let previous_sections = previous
        .pages()
        .iter()
        .flat_map(|page| page.sections())
        .map(|section| (section.id().as_str().to_owned(), section))
        .collect::<BTreeMap<_, _>>();
    for (id, section) in &current_sections {
        match previous_sections.get(id) {
            None => changes.push(format!(
                "Section added: {} ({id}).",
                section_kind_label(section.section_type())
            )),
            Some(old) if *old != *section => changes.push(format!(
                "Section changed: {} ({id}).",
                section_kind_label(section.section_type())
            )),
            Some(_) => {}
        }
    }
    for (id, section) in &previous_sections {
        if !current_sections.contains_key(id) {
            changes.push(format!(
                "Section removed: {} ({id}).",
                section_kind_label(section.section_type())
            ));
        }
    }
    if current.forms() != previous.forms() {
        changes.push("Form definitions changed.".to_owned());
    }
    if changes.is_empty() {
        changes.push("There are no document differences.".to_owned());
    }
    changes
}

#[cfg(feature = "browser")]
#[component]
pub(super) fn V2FunnelDetail(id: String) -> Element {
    let mut state = use_signal(|| V2EditorState::for_route(id.clone()));
    let load_error_reload_id = id.clone();
    let stale_reload_id = id.clone();
    let discard_recovered_id = id.clone();
    let restore_older_draft_id = id.clone();
    let discard_older_draft_id = id.clone();
    let mut responsive_viewport = use_signal(|| PreviewViewport::Desktop);
    let selected_viewport = responsive_viewport();

    use_effect(use_reactive!(|(id)| {
        let requested_id = id.clone();
        let route_epoch = state.with_mut(|state| state.begin_route(requested_id.clone()));
        spawn(async move { load_v2_editor_state(state, requested_id, route_epoch).await });
    }));

    let view = state();
    let title = view
        .funnel
        .as_ref()
        .map(|funnel| funnel.name.clone())
        .unwrap_or_else(|| "Typed funnel editor".to_owned());
    let current_page = view
        .document
        .as_ref()
        .and_then(|document| {
            document
                .pages()
                .iter()
                .find(|page| page.id().as_str() == view.selected_page)
        })
        .cloned();
    let selected_section = current_page
        .as_ref()
        .and_then(|page| {
            page.sections()
                .iter()
                .find(|section| Some(section.id().as_str()) == view.selected_section.as_deref())
        })
        .cloned();
    let locked = !view.can_edit();
    let dirty = view.has_unsaved_changes();
    let recovered_document = view
        .recovered_draft
        .as_ref()
        .and_then(|draft| DocumentV2::decode(draft.document_json.as_bytes(), &view.funnel_id).ok());

    rsx! {
        div {
            class: "editor-page",
            section { class: "page-heading",
                p { class: "eyebrow", "Funnels / Typed editor" }
                h1 { "{title}" }
                p { class: "page-description", "Compose typed sections and blocks. Changes stay as a local draft until you save a new revision." }
                code { class: "detail-id", "Funnel ID: {id}" }
            }
            if view.loading {
                section { class: "empty-state editor-loading", role: "status", "aria-label": "Typed funnel editor",
                    "Loading the current typed revision…"
                }
            }
            if view.load_error {
                section { class: "empty-state editor-load-error", role: "alert", "aria-label": "Typed editor error",
                    p { "The typed funnel editor could not be loaded." }
                    button {
                        class: "secondary-button",
                        r#type: "button",
                        onclick: move |_| {
                            let route_epoch = state.with_mut(|state| state.begin_route(load_error_reload_id.clone()));
                            let requested_id = load_error_reload_id.clone();
                            spawn(async move { load_v2_editor_state(state, requested_id, route_epoch).await });
                        },
                        "Try again"
                    }
                }
            }
            if let Some(document) = view.document.clone() {
                section { class: "editor-toolbar", "aria-label": "Typed editor actions",
                    div {
                        p { class: "eyebrow", "Draft revision" }
                        code { "{view.base_revision_id}" }
                        if dirty {
                            span { class: "editor-unsaved-badge", "Unsaved changes" }
                        } else {
                            span { class: "editor-saved-badge", "All changes saved" }
                        }
                    }
                    div { class: "editor-actions",
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            disabled: !view.can_undo(),
                            onclick: move |_| {
                                let request = state.with_mut(|state| state.apply_history(-1));
                                if let Some((_, funnel_id, base_revision_id, document_json)) = request.clone() {
                                    let draft = V2LocalDraft { version: 1, funnel_id, base_revision_id, document_json };
                                    spawn(async move { let _ = write_v2_local_draft(&draft).await; });
                                }
                                schedule_v2_preview(state, request);
                            },
                            "Undo"
                        }
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            disabled: !view.can_redo(),
                            onclick: move |_| {
                                let request = state.with_mut(|state| state.apply_history(1));
                                if let Some((_, funnel_id, base_revision_id, document_json)) = request.clone() {
                                    let draft = V2LocalDraft { version: 1, funnel_id, base_revision_id, document_json };
                                    spawn(async move { let _ = write_v2_local_draft(&draft).await; });
                                }
                                schedule_v2_preview(state, request);
                            },
                            "Redo"
                        }
                        button {
                            class: "primary-button",
                            r#type: "button",
                            disabled: !view.can_save(),
                            onclick: move |_| {
                                spawn(async move { save_v2_revision(state).await; });
                            },
                            if view.saving { "Saving…" } else if view.save_retryable { "Retry save" } else { "Save revision" }
                        }
                    }
                }
                if let Some(capacity) = view.stage_capacity.as_ref() {
                    p {
                        class: "editor-capacity-note",
                        role: "note",
                        "aria-label": "Temporary R2 storage budget",
                        "Worst-case orphan exposure is capped at {capacity.worst_case_orphan_bytes} bytes across {capacity.worst_case_orphan_keys} object writes. Charged to date: {capacity.orphan_bytes_spent} bytes across {capacity.orphan_keys_spent} writes. Charges are never refunded."
                    }
                }
                if let Some(status) = view.status.as_ref() {
                    p { class: "editor-save-status", role: "status", "aria-live": "polite", "aria-atomic": "true", "{status}" }
                }
                if let Some(error) = view.error.as_ref() {
                    p { class: "error-message editor-save-error", role: "alert", "{error}" }
                }
                if view.stale {
                    p { class: "error-message", role: "status", "Your draft is still in this browser. Reload the current revision to compare it or restore it as a new draft." }
                    button {
                        class: "secondary-button",
                        r#type: "button",
                        onclick: move |_| {
                            let route_epoch = state.with_mut(|state| state.begin_route(stale_reload_id.clone()));
                            let requested_id = stale_reload_id.clone();
                            spawn(async move { load_v2_editor_state(state, requested_id, route_epoch).await });
                        },
                        "Reload current revision"
                    }
                }
                if let Some(error) = view.recovery_error.as_ref() {
                    p { class: "error-message", role: "status", "{error}" }
                }
                if view.recovered_current_draft {
                    section { class: "editor-recovery-panel", role: "status", "aria-label": "Recovered local draft",
                        p { "Recovered your unsaved draft from this browser." }
                        button {
                            class: "text-button",
                            r#type: "button",
                            onclick: move |_| {
                                state.with_mut(|state| {
                                    state.document = DocumentV2::decode(state.saved_document_json.as_bytes(), &state.funnel_id).ok();
                                    state.recovered_current_draft = false;
                                    state.history = vec![state.saved_document_json.clone()];
                                    state.history_cursor = 0;
                                    state.status = Some("Recovered draft discarded.".to_owned());
                                });
                                let funnel_id = discard_recovered_id.clone();
                                let _ = spawn(async move { let _ = remove_v2_local_draft(&funnel_id).await; });
                            },
                            "Discard recovered draft"
                        }
                    }
                }
                if let Some(draft) = view.recovered_draft.as_ref() {
                    section { class: "editor-recovery-panel", role: "region", "aria-label": "Older recovered draft",
                        h2 { "A draft from an older revision is available" }
                        p { "It was based on revision {draft.base_revision_id}. Compare it with the current draft before restoring it." }
                        if let Some(old_document) = recovered_document.as_ref() {
                            ul {
                                for line in revision_comparison(&document, old_document) {
                                    li { "{line}" }
                                }
                            }
                        }
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            disabled: locked || recovered_document.is_none(),
                            onclick: move |_| {
                                if let Some(draft) = state.with(|state| state.recovered_draft.clone()) {
                                    if let Ok(document) = DocumentV2::decode(draft.document_json.as_bytes(), &restore_older_draft_id) {
                                        let request = state.with_mut(|state| state.restore_document(document));
                                        if let Some((_, funnel_id, base_revision_id, document_json)) = request.clone() {
                                            let local = V2LocalDraft { version: 1, funnel_id, base_revision_id, document_json };
                                            spawn(async move { let _ = write_v2_local_draft(&local).await; });
                                        }
                                        schedule_v2_preview(state, request);
                                    }
                                }
                            },
                            "Use this content as a new draft"
                        }
                        button {
                            class: "text-button",
                            r#type: "button",
                            onclick: move |_| {
                                state.with_mut(|state| state.recovered_draft = None);
                                let funnel_id = discard_older_draft_id.clone();
                                spawn(async move { let _ = remove_v2_local_draft(&funnel_id).await; });
                            },
                            "Discard older draft"
                        }
                    }
                }
                section { class: "editor-publication-panel", role: "region", "aria-label": "Publication",
                    div { class: "section-heading",
                        h2 { "Publication" }
                        span { "Draft and live status" }
                    }
                    div { class: "editor-publication-facts",
                        p { strong { "Draft revision " } code { "{view.base_revision_id}" } }
                        p {
                            strong { "Live revision " }
                            if let Some(live) = view.published_revision_id.as_ref() { code { "{live}" } }
                            else { span { "Not published" } }
                        }
                        if view.published_revision_id.is_some() {
                            if let Some(funnel) = view.funnel.as_ref() {
                                a { class: "editor-live-link", href: "/f/{funnel.slug}", target: "_blank", rel: "noopener noreferrer", "Open live funnel" }
                            }
                        }
                    }
                    if let Some(status) = view.publication_status.as_ref() {
                        p { class: "editor-publication-status", role: "status", "aria-live": "polite", "aria-atomic": "true", "{status}" }
                    }
                    if let Some(error) = view.publication_error.as_ref() {
                        p { class: "error-message", role: "alert", "{error}" }
                    }
                    if dirty {
                        p { class: "editor-publication-help", "Save draft changes before publishing." }
                    }
                    div { class: "editor-publication-actions",
                        button {
                            class: "primary-button",
                            r#type: "button",
                            disabled: !view.can_publish(),
                            onclick: move |_| {
                                spawn(async move { publish_v2_revision(state, true).await; });
                            },
                            if view.publication_pending && matches!(view.publication_attempt, Some(PublicationAction::Publish { .. })) { "Publishing…" }
                            else if view.publication_retryable && matches!(view.publication_attempt, Some(PublicationAction::Publish { .. })) { "Retry publish" }
                            else { "Publish saved revision" }
                        }
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            disabled: !view.can_unpublish(),
                            onclick: move |_| {
                                if view.publication_retryable && matches!(view.publication_attempt, Some(PublicationAction::Unpublish { .. })) {
                                    spawn(async move { publish_v2_revision(state, false).await; });
                                } else {
                                    state.with_mut(|state| state.unpublish_confirmation = true);
                                    spawn(async move { super::super::set_modal_dialog("v2-unpublish-dialog", true).await; });
                                }
                            },
                            if view.publication_pending && matches!(view.publication_attempt, Some(PublicationAction::Unpublish { .. })) { "Unpublishing…" }
                            else { "Unpublish" }
                        }
                    }
                }
                if view.unpublish_confirmation {
                    dialog {
                        id: "v2-unpublish-dialog",
                        class: "funnel-dialog confirmation-dialog",
                        "aria-modal": "true",
                        "aria-labelledby": "v2-unpublish-title",
                        "aria-describedby": "v2-unpublish-description",
                        oncancel: move |event| {
                            event.prevent_default();
                            state.with_mut(|state| state.unpublish_confirmation = false);
                            spawn(async move { super::super::set_modal_dialog("v2-unpublish-dialog", false).await; });
                        },
                        h2 { id: "v2-unpublish-title", "Unpublish this funnel?" }
                        p { id: "v2-unpublish-description", "This removes the live revision pointer. Already viewed copies may remain available in browsers or intermediary caches." }
                        div { class: "dialog-actions",
                            button {
                                class: "secondary-button",
                                r#type: "button",
                                autofocus: true,
                                onclick: move |_| {
                                    state.with_mut(|state| state.unpublish_confirmation = false);
                                    spawn(async move { super::super::set_modal_dialog("v2-unpublish-dialog", false).await; });
                                },
                                "Cancel"
                            }
                            button {
                                class: "primary-button danger-button",
                                r#type: "button",
                                onclick: move |_| {
                                    state.with_mut(|state| state.unpublish_confirmation = false);
                                    spawn(async move {
                                        super::super::set_modal_dialog("v2-unpublish-dialog", false).await;
                                        publish_v2_revision(state, false).await;
                                    });
                                },
                                "Confirm unpublish"
                            }
                        }
                    }
                }
                section { class: "editor-workspace", "aria-label": "Typed visual editor",
                    div { class: "editor-outline-panel",
                        div { class: "section-heading",
                            h2 { "Pages and sections" }
                            span { "{document.pages().len()} pages" }
                        }
                        if document.pages().len() > 1 {
                            label { class: "editor-setting-control",
                                "Current page"
                                select {
                                    value: "{view.selected_page}",
                                    disabled: view.pending || view.saving,
                                    onchange: move |event| {
                                        let request = state.with_mut(|state| state.select_page(&event.value()));
                                        schedule_v2_preview(state, request);
                                    },
                                    for page in document.pages() {
                                        option { value: "{page.id().as_str()}", "{page.title()}" }
                                    }
                                }
                            }
                        }
                        fieldset { class: "editor-setting-group",
                            legend { "Add a section" }
                            div { class: "editor-add-row" }
                            label { class: "editor-setting-control",
                                "Section type"
                                select {
                                    value: "{section_kind_id(view.new_section_kind)}",
                                    disabled: locked,
                                    onchange: move |event| {
                                        if let Some(kind) = section_kind_from_id(&event.value()) {
                                            state.with_mut(|state| state.new_section_kind = kind);
                                        }
                                    },
                                    for kind in [SectionKind::Hero, SectionKind::Features, SectionKind::Testimonials, SectionKind::Cta, SectionKind::Footer] {
                                        option { value: "{section_kind_id(kind)}", "{section_kind_label(kind)}" }
                                    }
                                }
                            }
                            button {
                                class: "secondary-button",
                                r#type: "button",
                                disabled: locked,
                                onclick: move |_| {
                                    let kind = state.with(|state| state.new_section_kind);
                                    let page = state.with(|state| PageId::parse(state.selected_page.clone()).ok());
                                    if let Some(page) = page {
                                        dispatch_v2_edit(state, Edit::AddSection { page, after: None, section_type: kind });
                                    }
                                },
                                "Add section"
                            }
                        }
                        if let Some(page) = current_page {
                            div { class: "editor-section-list", role: "list", "aria-label": "Sections on this page",
                                for (index, section) in page.sections().iter().enumerate() {
                                    {
                                        let section_id = section.id().as_str().to_owned();
                                        let selected = view.selected_section.as_deref() == Some(section_id.as_str());
                                        let label = section_kind_label(section.section_type());
                                        let page_id_up = page.id().clone();
                                        let page_id_down = page.id().clone();
                                        let page_id_duplicate = page.id().clone();
                                        let page_id_delete = page.id().clone();
                                        let duplicate_id = section.id().clone();
                                        let delete_id = section.id().clone();
                                        let up_id = section.id().clone();
                                        let down_id = section.id().clone();
                                        let section_ids = page.sections().iter().map(|item| item.id().clone()).collect::<Vec<_>>();
                                        let section_count = section_ids.len();
                                        let section_ids_up = section_ids.clone();
                                        let section_ids_down = section_ids;
                                        rsx! {
                                            div {
                                                class: "editor-section-row",
                                                role: "listitem",
                                                "aria-current": if selected { "true" } else { "false" },
                                                div { class: "editor-section-copy",
                                                    button {
                                                        class: "text-button",
                                                        r#type: "button",
                                                        disabled: view.pending,
                                                        onclick: move |_| state.with_mut(|state| state.select_section(&section_id)),
                                                        "{label}"
                                                    }
                                                    code { "{section.id().as_str()}" }
                                                }
                                                div { class: "editor-section-actions",
                                                    button {
                                                        class: "editor-move-button",
                                                        r#type: "button",
                                                        disabled: locked || index == 0,
                                                        "aria-label": "Move {label} up",
                                                        onclick: move |_| {
                                                            let mut ordered = section_ids_up.clone();
                                                            if index > 0 { ordered.swap(index, index - 1); }
                                                            dispatch_v2_edit(state, Edit::ReorderSections { page: page_id_up.clone(), ordered });
                                                            let _ = &up_id;
                                                        },
                                                        "Up"
                                                    }
                                                    button {
                                                        class: "editor-move-button",
                                                        r#type: "button",
                                                        disabled: locked || index + 1 >= section_count,
                                                        "aria-label": "Move {label} down",
                                                        onclick: move |_| {
                                                            let mut ordered = section_ids_down.clone();
                                                            if index + 1 < ordered.len() { ordered.swap(index, index + 1); }
                                                            dispatch_v2_edit(state, Edit::ReorderSections { page: page_id_down.clone(), ordered });
                                                            let _ = &down_id;
                                                        },
                                                        "Down"
                                                    }
                                                    button {
                                                        class: "editor-move-button",
                                                        r#type: "button",
                                                        disabled: locked,
                                                        "aria-label": "Duplicate {label}",
                                                        onclick: move |_| dispatch_v2_edit(state, Edit::DuplicateSection { page: page_id_duplicate.clone(), section: duplicate_id.clone() }),
                                                        "Duplicate"
                                                    }
                                                    button {
                                                        class: "editor-move-button",
                                                        r#type: "button",
                                                        disabled: locked,
                                                        "aria-label": "Delete {label}",
                                                        onclick: move |_| dispatch_v2_edit(state, Edit::DeleteSection { page: page_id_delete.clone(), section: delete_id.clone() }),
                                                        "Delete"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "editor-inspector-panel", "aria-label": "Typed content inspector",
                        div { class: "section-heading",
                            h2 { "Content and theme" }
                            span { "Typed document v2" }
                        }
                        fieldset { class: "editor-setting-group",
                            legend { "Theme preset" }
                            label { class: "editor-setting-control",
                                "Preset"
                                select {
                                    value: "{document.theme().preset_id().id()}",
                                    disabled: locked,
                                    onchange: move |event| {
                                        if let Some(preset) = ThemePreset::from_id(&event.value()) {
                                            dispatch_v2_edit(state, Edit::SetThemePreset { preset });
                                        }
                                    },
                                    for preset in ThemePreset::all() {
                                        option { value: "{preset.id()}", "{preset.name()}" }
                                    }
                                }
                            }
                        }
                        if let Some(section) = selected_section {
                            fieldset { class: "editor-setting-group",
                                legend { "{section_kind_label(section.section_type())}" }
                                p { class: "editor-selected-section", "Section ID: {section.id().as_str()}" }
                                div { class: "editor-block-list", role: "list", "aria-label": "Blocks in selected section",
                                    for (index, block) in section.blocks().iter().enumerate() {
                                        V2BlockEditor {
                                            state,
                                            section_id: section.id().clone(),
                                            block: block.clone(),
                                            index,
                                            block_ids: section.blocks().iter().map(|item| item.id().clone()).collect::<Vec<_>>(),
                                            locked,
                                        }
                                    }
                                }
                                fieldset { class: "editor-setting-group",
                                    legend { "Add a block" }
                                    label { class: "editor-setting-control",
                                        "Block type"
                                        select {
                                            value: "{view.new_block_kind.id()}",
                                            disabled: locked,
                                            onchange: move |event| {
                                                if let Some(kind) = V2NewBlockKind::from_id(&event.value()) {
                                                    state.with_mut(|state| state.new_block_kind = kind);
                                                }
                                            },
                                            for kind in V2NewBlockKind::ALL {
                                                option { value: "{kind.id()}", "{kind.label()}" }
                                            }
                                        }
                                    }
                                    button {
                                        class: "secondary-button",
                                        r#type: "button",
                                        disabled: locked,
                                        onclick: move |_| {
                                            let kind = state.with(|state| state.new_block_kind);
                                            dispatch_v2_edit(state, Edit::AddBlock {
                                                section: section.id().clone(),
                                                after: section.blocks().last().map(|block| block.id().clone()),
                                                block: kind.create(),
                                            });
                                        },
                                        "Add block"
                                    }
                                }
                            }
                        } else {
                            p { class: "editor-keyboard-help", "Select a section to edit its blocks." }
                        }
                    }
                    div { class: "editor-preview-panel",
                        div { class: "section-heading", h2 { "Preview" }, span { "Sandboxed" } }
                        div { class: "editor-preview-switcher", role: "group", "aria-label": "Preview viewport",
                            for option in PreviewViewport::ALL {
                                button {
                                    class: "secondary-button",
                                    r#type: "button",
                                    "aria-pressed": "{selected_viewport == option}",
                                    onclick: move |_| responsive_viewport.set(option),
                                    "{option.label()} ({option.width_px()} px)"
                                }
                            }
                        }
                        div { class: "editor-preview-canvas", role: "region", tabindex: "0", "aria-label": "Preview canvas, {selected_viewport.width_px()} pixels wide",
                            iframe {
                                class: "editor-preview-frame",
                                title: "Preview of {title}",
                                width: "{selected_viewport.width_px()}",
                                "sandbox": "",
                                srcdoc: "{view.preview_html}",
                            }
                        }
                    }
                }
                section { class: "editor-history-panel", role: "region", "aria-label": "Revision history",
                    div { class: "section-heading", h2 { "Revision history" }, span { "{view.revisions.len()} saved revisions" } }
                    if let Some(compare_document) = view.compare_document.as_ref() {
                        div { class: "editor-revision-comparison",
                            h3 { "Compare current draft with {view.compare_revision_id.as_deref().unwrap_or_default()}" }
                            ul {
                                for line in revision_comparison(&document, compare_document) {
                                    li { "{line}" }
                                }
                            }
                            button {
                                class: "secondary-button",
                                r#type: "button",
                                disabled: locked,
                                onclick: move |_| {
                                    if let Some(old) = state.with(|state| state.compare_document.clone()) {
                                        let request = state.with_mut(|state| state.restore_document(old));
                                        if let Some((_, funnel_id, base_revision_id, document_json)) = request.clone() {
                                            let draft = V2LocalDraft { version: 1, funnel_id, base_revision_id, document_json };
                                            spawn(async move { let _ = write_v2_local_draft(&draft).await; });
                                        }
                                        schedule_v2_preview(state, request);
                                    }
                                },
                                "Restore this revision as a new draft"
                            }
                            button {
                                class: "text-button",
                                r#type: "button",
                                onclick: move |_| state.with_mut(|state| {
                                    state.compare_document = None;
                                    state.compare_revision_id = None;
                                }),
                                "Close comparison"
                            }
                        }
                    }
                    if view.compare_loading {
                        p { role: "status", "Loading the selected revision…" }
                    }
                    div { class: "editor-revision-list", role: "list", "aria-label": "Saved revisions",
                        for revision in view.revisions.clone() {
                            {
                                let revision_id = revision.revision_id.clone();
                                rsx! {
                                    div { class: "editor-revision-row", role: "listitem",
                                        div {
                                            strong { "{revision.revision_id}" }
                                            span { "Schema v{revision.schema_version} · {revision.created_at}" }
                                        }
                                        button {
                                            class: "secondary-button",
                                            r#type: "button",
                                            disabled: revision.revision_id == view.base_revision_id || view.compare_loading,
                                            onclick: move |_| {
                                                let revision_id = revision_id.clone();
                                                spawn(async move { compare_v2_revision(state, revision_id).await; });
                                            },
                                            "Compare"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                p { class: "sr-only", role: "status", "aria-live": "polite", "aria-atomic": "true", "{view.announcement}" }
            }
        }
    }
}

#[cfg(feature = "browser")]
#[component]
fn V2BlockEditor(
    state: Signal<V2EditorState>,
    section_id: SectionId,
    block: BlockV2,
    index: usize,
    block_ids: Vec<BlockId>,
    locked: bool,
) -> Element {
    let block_id = block.id().clone();
    let text = block_text(&block);
    let label = block_kind_label(&block);
    let control_label = format!("{label} content");
    let section_up = section_id.clone();
    let section_down = section_id.clone();
    let section_duplicate = section_id.clone();
    let section_delete = section_id;
    let id_up = block_id.clone();
    let id_down = block_id.clone();
    let id_duplicate = block_id.clone();
    let id_delete = block_id.clone();
    let block_for_edit = block.clone();
    let block_count = block_ids.len();
    let block_ids_up = block_ids.clone();
    let block_ids_down = block_ids;
    rsx! {
        div { class: "editor-block-row", role: "listitem",
            div { class: "editor-block-heading",
                strong { "{label}" }
                code { "{block.id().as_str()}" }
            }
            if matches!(block, BlockV2::Image { .. } | BlockV2::Form { .. }) {
                p { class: "editor-keyboard-help", "{text}" }
            } else {
                label { class: "editor-setting-control",
                    "{control_label}"
                    textarea {
                        rows: if matches!(block, BlockV2::List { .. }) { "4" } else { "3" },
                        value: "{text}",
                        disabled: locked,
                        "aria-label": "{control_label} for {block.id().as_str()}",
                        onchange: move |event| {
                            if let Some(value) = block_value_with_text(&block_for_edit, event.value()) {
                                dispatch_v2_edit(state, Edit::SetBlock { block: block_for_edit.id().clone(), value });
                            }
                        },
                    }
                }
            }
            div { class: "editor-section-actions",
                button {
                    class: "editor-move-button",
                    r#type: "button",
                    disabled: locked || index == 0,
                    "aria-label": "Move {label} up",
                    onclick: move |_| {
                        let mut ordered = block_ids_up.clone();
                        if index > 0 { ordered.swap(index, index - 1); }
                        dispatch_v2_edit(state, Edit::ReorderBlocks { section: section_up.clone(), ordered });
                        let _ = &id_up;
                    },
                    "Up"
                }
                button {
                    class: "editor-move-button",
                    r#type: "button",
                    disabled: locked || index + 1 >= block_count,
                    "aria-label": "Move {label} down",
                    onclick: move |_| {
                        let mut ordered = block_ids_down.clone();
                        if index + 1 < ordered.len() { ordered.swap(index, index + 1); }
                        dispatch_v2_edit(state, Edit::ReorderBlocks { section: section_down.clone(), ordered });
                        let _ = &id_down;
                    },
                    "Down"
                }
                button {
                    class: "editor-move-button",
                    r#type: "button",
                    disabled: locked,
                    "aria-label": "Duplicate {label}",
                    onclick: move |_| dispatch_v2_edit(state, Edit::DuplicateBlock { section: section_duplicate.clone(), block: id_duplicate.clone() }),
                    "Duplicate"
                }
                button {
                    class: "editor-move-button",
                    r#type: "button",
                    disabled: locked,
                    "aria-label": "Delete {label}",
                    onclick: move |_| dispatch_v2_edit(state, Edit::DeleteBlock { section: section_delete.clone(), block: id_delete.clone() }),
                    "Delete"
                }
            }
        }
    }
}

#[cfg(feature = "browser")]
fn section_kind_id(kind: SectionKind) -> &'static str {
    match kind {
        SectionKind::Hero => "hero",
        SectionKind::Features => "features",
        SectionKind::Testimonials => "testimonials",
        SectionKind::Cta => "cta",
        SectionKind::Footer => "footer",
    }
}

#[cfg(feature = "browser")]
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
            output
        })
}

#[cfg(feature = "browser")]
async fn publish_v2_revision(mut state: Signal<V2EditorState>, publish: bool) {
    let action = state.with_mut(|state| {
        if state.publication_pending {
            return None;
        }
        let attempt = match state.publication_attempt.clone() {
            Some(action @ PublicationAction::Publish { .. })
                if publish && state.publication_retryable =>
            {
                action
            }
            Some(action @ PublicationAction::Unpublish { .. })
                if !publish && state.publication_retryable =>
            {
                action
            }
            _ if publish && state.can_publish() => PublicationAction::Publish {
                revision_id: state.base_revision_id.clone(),
                expected_published_revision_id: state.published_revision_id.clone(),
            },
            _ if !publish && state.published_revision_id.is_some() => {
                PublicationAction::Unpublish {
                    expected_published_revision_id: state.published_revision_id.clone()?,
                }
            }
            _ => return None,
        };
        state.publication_pending = true;
        state.publication_retryable = false;
        state.publication_attempt = Some(attempt.clone());
        state.publication_error = None;
        state.publication_status = Some(
            if publish {
                "Publishing the saved revision…"
            } else {
                "Unpublishing the live revision…"
            }
            .to_owned(),
        );
        Some(attempt)
    });
    let Some(action) = action else {
        return;
    };
    let path = action.path(&state.with(|state| state.funnel_id.clone()));
    let body = action.body();
    let previous_key = state.with(|state| {
        (state.publication_body.as_deref() == Some(body.as_str()))
            .then(|| state.publication_key.clone())
            .flatten()
    });
    let key = match prepare_idempotency_key("POST", &path, Some(&body), previous_key) {
        Ok(key) => key,
        Err(()) => {
            state.with_mut(|state| {
                state.publication_pending = false;
                state.publication_error =
                    Some("The browser could not prepare this publication request.".to_owned());
            });
            return;
        }
    };
    state.with_mut(|state| {
        state.publication_key = Some(key.clone());
        state.publication_body = Some(body.clone());
    });
    let response = api_request("POST", &path, Some(body), Some(key.clone())).await;
    match response {
        Ok(response) if response.status == 200 => {
            match serde_json::from_str::<PublicationResponse>(&response.body) {
                Ok(response)
                    if response.published_revision_id.as_deref()
                        == action.resulting_revision_id() =>
                {
                    state.with_mut(|state| {
                        state.publication_pending = false;
                        state.publication_retryable = false;
                        state.publication_attempt = None;
                        state.published_revision_id = response.published_revision_id;
                        state.publication_status = Some(
                            if publish {
                                "Revision published."
                            } else {
                                "Funnel unpublished."
                            }
                            .to_owned(),
                        );
                        state.publication_error = None;
                        state.publication_key = None;
                        state.publication_body = None;
                    });
                    let _ = clear_idempotency_storage(&key);
                }
                _ => {
                    state.with_mut(|state| {
                        state.publication_pending = false;
                        state.publication_error =
                            Some("The publication response could not be verified.".to_owned());
                        state.publication_status = None;
                    });
                }
            }
        }
        Ok(response) => {
            let failure = v2_failure(&response);
            state.with_mut(|state| {
                state.publication_pending = false;
                state.publication_retryable = failure.retryable;
                if !failure.retryable {
                    state.publication_attempt = None;
                    state.publication_key = None;
                    state.publication_body = None;
                }
                state.publication_error = Some(failure.message);
                state.publication_status = None;
            });
            if !failure.retryable {
                let _ = clear_idempotency_storage(&key);
            }
        }
        Err(()) => state.with_mut(|state| {
            state.publication_pending = false;
            state.publication_retryable = true;
            state.publication_error = Some(
                "The publication result is uncertain. Retry the same request to confirm it."
                    .to_owned(),
            );
            state.publication_status = None;
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eez_domain::document_v2::BlueprintId;

    struct TestIds(u32);

    impl IdSource for TestIds {
        fn next_id(
            &mut self,
            _kind: IdKind,
        ) -> Result<String, eez_domain::document_v2::DocumentError> {
            self.0 += 1;
            Ok(format!("test-{}", self.0))
        }
    }

    fn document() -> DocumentV2 {
        DocumentV2::starter("funnel-a", BlueprintId::LeadGen, ThemePreset::Minimal)
            .expect("valid V2 starter")
    }

    #[test]
    fn typed_edit_capture_sends_the_base_document_and_predicts_the_worker_result() {
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.loading = false;
        let original = document();
        let original_json = original.canonical_json();
        state.document = Some(original.clone());
        state.base_revision_id = "revision-a".to_owned();
        state.selected_page = original.entry_page_id().as_str().to_owned();
        state.saved_document_json = original_json.clone();

        let operation_id = "ab".repeat(16);
        let capture = state
            .apply_local_edit(
                Edit::AddSection {
                    page: original.entry_page_id().clone(),
                    after: None,
                    section_type: SectionKind::Testimonials,
                },
                operation_id,
            )
            .expect("valid local section addition");
        let mut worker_ids = DeterministicIds {
            operation_id: capture.operation_id.clone(),
            next: 0,
        };
        let worker_result = DocumentV2::decode(capture.base_document_json.as_bytes(), "funnel-a")
            .expect("worker input is the saved base")
            .apply(capture.edit.clone(), &mut worker_ids)
            .expect("worker applies the typed operation");

        assert_eq!(capture.base_document_json, original_json);
        assert_eq!(
            worker_result.document.canonical_json(),
            capture.document_json
        );
        assert_ne!(capture.base_document_json, capture.document_json);
    }

    #[test]
    fn history_is_bounded_and_redo_is_removed_after_a_new_edit() {
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.loading = false;
        let document = document();
        state.base_revision_id = "revision-a".to_owned();
        state.saved_document_json = document.canonical_json();
        state.document = Some(document.clone());
        state.history = vec![state.saved_document_json.clone()];
        let page = document.pages()[0].id().clone();
        for index in 0..3 {
            let mut ids = TestIds(index);
            let updated = state
                .document
                .as_ref()
                .expect("current document")
                .apply(
                    Edit::AddSection {
                        page: page.clone(),
                        after: None,
                        section_type: SectionKind::Features,
                    },
                    &mut ids,
                )
                .expect("valid section addition");
            state.document = Some(updated.document);
            let snapshot = state.document.as_ref().expect("updated").canonical_json();
            state.push_history(snapshot);
        }
        assert!(state.can_undo());
        state.history_cursor = 1;
        state.history.truncate(2);
        let mut ids = TestIds(10);
        let updated = state
            .document
            .as_ref()
            .expect("current document")
            .apply(
                Edit::SetThemePreset {
                    preset: ThemePreset::Bold,
                },
                &mut ids,
            )
            .expect("valid theme change");
        state.document = Some(updated.document);
        state.push_history(state.document.as_ref().expect("updated").canonical_json());
        assert!(!state.can_redo());
    }

    #[test]
    fn comparison_reports_added_and_changed_sections_and_theme() {
        let original = document();
        let page = original.pages()[0].id().clone();
        let mut ids = TestIds(0);
        let changed = original
            .apply(
                Edit::AddSection {
                    page,
                    after: None,
                    section_type: SectionKind::Features,
                },
                &mut ids,
            )
            .expect("valid section addition");
        let lines = revision_comparison(&changed.document, &original);
        assert!(lines.iter().any(|line| line.starts_with("Section added:")));
        let with_theme = changed
            .document
            .apply(
                Edit::SetThemePreset {
                    preset: ThemePreset::Elegant,
                },
                &mut ids,
            )
            .expect("valid theme change");
        assert!(revision_comparison(&with_theme.document, &original)
            .iter()
            .any(|line| line == "The theme changed."));
    }
}
