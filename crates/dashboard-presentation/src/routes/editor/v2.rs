use super::*;
use eez_domain::document_v2::{
    ActionV2, BlockV2, DocumentV2, Edit, FocusTarget, IdKind, IdSource, NewBlock, SectionKind,
};
#[cfg(feature = "browser")]
use eez_domain::document_v2::{
    AssetId, BlockId, FormField, FormResult, FormV2, ImageAlt, NewFormField, PageId, SectionId,
    ValidationRange,
};
#[cfg(any(feature = "browser", test))]
use eez_domain::document_v2::{
    ButtonShape, ContentWidth, CornerScale, FontFamily, Shadow, SpacingScale, ThemeColors,
    ThemeOverrides, TypeScale,
};
#[cfg(any(feature = "browser", test))]
use eez_domain::ThemePreset;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(feature = "browser")]
use uuid::Uuid;

#[cfg(feature = "browser")]
use super::super::{
    api_request, clear_idempotency_storage, error_message, prepare_idempotency_key,
    DashboardMediaItem, DashboardMediaPage, PreparedIdempotencyKey,
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

#[cfg(feature = "browser")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum V2NewFormFieldKind {
    #[default]
    Text,
    Email,
    Phone,
    Checkbox,
    Select,
}

#[cfg(feature = "browser")]
impl V2NewFormFieldKind {
    const ALL: [Self; 5] = [
        Self::Text,
        Self::Email,
        Self::Phone,
        Self::Checkbox,
        Self::Select,
    ];

    const fn id(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Email => "email",
            Self::Phone => "phone",
            Self::Checkbox => "checkbox",
            Self::Select => "select",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Email => "Email",
            Self::Phone => "Phone",
            Self::Checkbox => "Checkbox",
            Self::Select => "Dropdown",
        }
    }

    fn from_id(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.id() == value)
    }

    fn create(self, form: &FormV2) -> NewFormField {
        let mut suffix = form.fields().len() + 1;
        let name = loop {
            let candidate = format!("field_{suffix}");
            if !form
                .fields()
                .iter()
                .any(|field| form_field_name(field) == candidate)
            {
                break candidate;
            }
            suffix += 1;
        };
        let label = match self {
            Self::Text => "New text field",
            Self::Email => "Email address",
            Self::Phone => "Phone number",
            Self::Checkbox => "Checkbox",
            Self::Select => "Choose an option",
        }
        .to_owned();
        match self {
            Self::Text => NewFormField::Text {
                name,
                label,
                help: None,
                required: false,
                validation: None,
            },
            Self::Email => NewFormField::Email {
                name,
                label,
                help: None,
                required: false,
                validation: None,
            },
            Self::Phone => NewFormField::Phone {
                name,
                label,
                help: None,
                required: false,
                validation: None,
            },
            Self::Checkbox => NewFormField::Checkbox {
                name,
                label,
                help: None,
                required: false,
            },
            Self::Select => NewFormField::Select {
                name,
                label,
                help: None,
                required: false,
                options: vec!["Option 1".to_owned()],
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
        self.document.is_some() && {
            !self.loading
                && !self.pending
                && !self.saving
                && !self.stale
                && (self.save_retryable || self.has_unsaved_changes())
        }
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
        let outcome = match document.apply(edit.clone(), &mut ids) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.error = Some(format!(
                    "This change cannot be applied yet ({})",
                    error.code()
                ));
                self.status = None;
                return None;
            }
        };
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
        BlockV2::Image { asset_id, .. } => format!("Image asset {}", asset_id.as_str()),
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

#[cfg(feature = "browser")]
fn form_field_name(field: &FormField) -> &str {
    match field {
        FormField::Text { name, .. }
        | FormField::Email { name, .. }
        | FormField::Phone { name, .. }
        | FormField::Checkbox { name, .. }
        | FormField::Select { name, .. } => name,
    }
}

#[cfg(feature = "browser")]
fn form_field_label(field: &FormField) -> &str {
    match field {
        FormField::Text { label, .. }
        | FormField::Email { label, .. }
        | FormField::Phone { label, .. }
        | FormField::Checkbox { label, .. }
        | FormField::Select { label, .. } => label,
    }
}

#[cfg(feature = "browser")]
fn form_field_help(field: &FormField) -> Option<&str> {
    match field {
        FormField::Text { help, .. }
        | FormField::Email { help, .. }
        | FormField::Phone { help, .. }
        | FormField::Checkbox { help, .. }
        | FormField::Select { help, .. } => help.as_deref(),
    }
}

#[cfg(feature = "browser")]
fn form_field_required(field: &FormField) -> bool {
    match field {
        FormField::Text { required, .. }
        | FormField::Email { required, .. }
        | FormField::Phone { required, .. }
        | FormField::Checkbox { required, .. }
        | FormField::Select { required, .. } => *required,
    }
}

#[cfg(feature = "browser")]
fn form_field_kind_id(field: &FormField) -> &'static str {
    match field {
        FormField::Text { .. } => "text",
        FormField::Email { .. } => "email",
        FormField::Phone { .. } => "phone",
        FormField::Checkbox { .. } => "checkbox",
        FormField::Select { .. } => "select",
    }
}

#[cfg(feature = "browser")]
fn form_field_options(field: &FormField) -> Option<&[String]> {
    match field {
        FormField::Select { options, .. } => Some(options),
        _ => None,
    }
}

#[cfg(feature = "browser")]
fn form_field_validation(field: &FormField) -> Option<&ValidationRange> {
    match field {
        FormField::Text { validation, .. }
        | FormField::Email { validation, .. }
        | FormField::Phone { validation, .. } => validation.as_ref(),
        FormField::Checkbox { .. } | FormField::Select { .. } => None,
    }
}

#[cfg(feature = "browser")]
fn form_field_validation_cap(field: &FormField) -> Option<u32> {
    match field {
        FormField::Text { .. } => Some(8192),
        FormField::Email { .. } => Some(320),
        FormField::Phone { .. } => Some(64),
        FormField::Checkbox { .. } | FormField::Select { .. } => None,
    }
}

#[cfg(feature = "browser")]
fn validation_range_update(field: &FormField, min: &str, max: &str) -> Option<NewFormField> {
    let parse = |value: &str| {
        if value.is_empty() {
            Some(None)
        } else {
            value.parse::<u32>().ok().map(Some)
        }
    };
    let min_length = parse(min)?;
    let max_length = parse(max)?;
    let cap = form_field_validation_cap(field)?;
    if min_length.is_some_and(|value| value > cap)
        || max_length.is_some_and(|value| value == 0 || value > cap)
        || min_length
            .zip(max_length)
            .is_some_and(|(min, max)| min > max)
    {
        return None;
    }
    let validation = (min_length.is_some() || max_length.is_some()).then_some(ValidationRange {
        min_length,
        max_length,
    });
    let name = form_field_name(field).to_owned();
    let label = form_field_label(field).to_owned();
    let help = form_field_help(field).map(str::to_owned);
    let required = form_field_required(field);
    match field {
        FormField::Text { .. } => Some(NewFormField::Text {
            name,
            label,
            help,
            required,
            validation,
        }),
        FormField::Email { .. } => Some(NewFormField::Email {
            name,
            label,
            help,
            required,
            validation,
        }),
        FormField::Phone { .. } => Some(NewFormField::Phone {
            name,
            label,
            help,
            required,
            validation,
        }),
        FormField::Checkbox { .. } | FormField::Select { .. } => None,
    }
}

#[cfg(feature = "browser")]
fn new_form_field_with_common(
    field: &FormField,
    name: String,
    label: String,
    help: Option<String>,
    required: bool,
) -> NewFormField {
    match field {
        FormField::Text { validation, .. } => NewFormField::Text {
            name,
            label,
            help,
            required,
            validation: validation.clone(),
        },
        FormField::Email { validation, .. } => NewFormField::Email {
            name,
            label,
            help,
            required,
            validation: validation.clone(),
        },
        FormField::Phone { validation, .. } => NewFormField::Phone {
            name,
            label,
            help,
            required,
            validation: validation.clone(),
        },
        FormField::Checkbox { .. } => NewFormField::Checkbox {
            name,
            label,
            help,
            required,
        },
        FormField::Select { options, .. } => NewFormField::Select {
            name,
            label,
            help,
            required,
            options: options.clone(),
        },
    }
}

#[cfg(feature = "browser")]
fn new_form_field_as_kind(field: &FormField, kind: V2NewFormFieldKind) -> NewFormField {
    let name = form_field_name(field).to_owned();
    let label = form_field_label(field).to_owned();
    let help = form_field_help(field).map(str::to_owned);
    let required = form_field_required(field);
    match kind {
        V2NewFormFieldKind::Text => NewFormField::Text {
            name,
            label,
            help,
            required,
            validation: None,
        },
        V2NewFormFieldKind::Email => NewFormField::Email {
            name,
            label,
            help,
            required,
            validation: None,
        },
        V2NewFormFieldKind::Phone => NewFormField::Phone {
            name,
            label,
            help,
            required,
            validation: None,
        },
        V2NewFormFieldKind::Checkbox => NewFormField::Checkbox {
            name,
            label,
            help,
            required,
        },
        V2NewFormFieldKind::Select => NewFormField::Select {
            name,
            label,
            help,
            required,
            options: form_field_options(field)
                .map(<[String]>::to_vec)
                .unwrap_or_else(|| vec!["Option 1".to_owned()]),
        },
    }
}

#[cfg(feature = "browser")]
fn form_field_with_options(field: &FormField, options: Vec<String>) -> Option<NewFormField> {
    let FormField::Select {
        name,
        label,
        help,
        required,
        ..
    } = field
    else {
        return None;
    };
    Some(NewFormField::Select {
        name: name.clone(),
        label: label.clone(),
        help: help.clone(),
        required: *required,
        options,
    })
}

#[cfg(feature = "browser")]
fn valid_form_field_name_input(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && value.len() <= 64
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

#[cfg(feature = "browser")]
fn parse_form_options(value: &str) -> Option<Vec<String>> {
    let options = value
        .lines()
        .map(str::trim)
        .filter(|option| !option.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if !(1..=50).contains(&options.len())
        || options.iter().any(|option| option.len() > 1024)
        || options
            .iter()
            .enumerate()
            .any(|(index, option)| options[..index].contains(option))
    {
        return None;
    }
    Some(options)
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
    let add_block_location = selected_section.as_ref().map(|section| {
        (
            section.id().clone(),
            section.blocks().last().map(|block| block.id().clone()),
        )
    });
    let add_form_location = add_block_location.clone();
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
                            {
                                let seo_title_page = page.id().clone();
                                let seo_description_page = page.id().clone();
                                let noindex_page = page.id().clone();
                                rsx! {
                                    fieldset { class: "editor-setting-group",
                                        legend { "Page search and sharing" }
                                        p { "Settings for {page.title()}. Save and publish to update the public page." }
                                        label { class: "editor-setting-control",
                                            "SEO title"
                                            input {
                                                r#type: "text",
                                                value: page.seo_title().unwrap_or_default(),
                                                placeholder: page.title(),
                                                disabled: locked,
                                                "aria-describedby": "v2-seo-title-help",
                                                onchange: move |event| dispatch_v2_edit(state, Edit::SetPageSeoTitle { page: seo_title_page.clone(), value: event.value() }),
                                            }
                                        }
                                        p { id: "v2-seo-title-help", "Up to 120 Unicode scalar values. Whitespace is collapsed. Leave blank to use the page title." }
                                        label { class: "editor-setting-control",
                                            "Meta description"
                                            textarea {
                                                value: page.seo_description().unwrap_or_default(),
                                                rows: 3,
                                                disabled: locked,
                                                "aria-describedby": "v2-seo-description-help",
                                                onchange: move |event| dispatch_v2_edit(state, Edit::SetPageSeoDescription { page: seo_description_page.clone(), value: event.value() }),
                                            }
                                        }
                                        p { id: "v2-seo-description-help", "Up to 320 Unicode scalar values. Whitespace is collapsed. Leave blank to omit the description." }
                                        label { class: "editor-setting-control",
                                            input {
                                                r#type: "checkbox",
                                                checked: page.noindex(),
                                                disabled: locked,
                                                "aria-describedby": "v2-seo-noindex-help",
                                                onchange: move |event| dispatch_v2_edit(state, Edit::SetPageNoIndex { page: noindex_page.clone(), noindex: event.checked() }),
                                            }
                                            "Ask search engines not to index this page"
                                        }
                                        p { id: "v2-seo-noindex-help", "Noindex is an indexing preference, not access control. Anyone with the public URL can still open this page." }
                                    }
                                }
                            }
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
                        V2ThemeCustomization { state, document: document.clone(), locked }
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
                                            let block = kind.create();
                                            if let Some((section, after)) = add_block_location.clone() {
                                                dispatch_v2_edit(state, Edit::AddBlock { section, after, block });
                                            }
                                        },
                                        "Add block"
                                    }
                                    if let Some((section_id, after)) = add_block_location.clone() {
                                        V2ImageBlockPicker { state, section: section_id, after, locked }
                                    }
                                    button {
                                        class: "secondary-button",
                                        r#type: "button",
                                        disabled: locked,
                                        onclick: move |_| {
                                            if let Some((section, after)) = add_form_location.clone() {
                                                dispatch_v2_edit(state, Edit::CreateForm { section, after });
                                            }
                                        },
                                        "Add form"
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
fn V2FormEditor(state: Signal<V2EditorState>, form: FormV2, locked: bool) -> Element {
    let form_id = form.id().clone();
    let form_id_purpose = form_id.clone();
    let form_id_result = form_id.clone();
    let form_id_consent = form_id.clone();
    let form_id_marketing = form_id.clone();
    let form_id_marketing_copy = form_id.clone();
    let purpose = form.purpose().to_owned();
    let request_consent_copy = form.request_consent().copy().to_owned();
    let marketing_copy = form.marketing_opt_in().map(|copy| copy.copy().to_owned());
    let marketing_enabled = marketing_copy.is_some();
    let marketing_copy_for_toggle = marketing_copy.clone();
    let result_message = match form.result() {
        FormResult::Inline { message } => Some(message.clone()),
        FormResult::SuccessPage { .. } => None,
    };
    let fields = form.fields().to_vec();

    rsx! {
        fieldset { class: "editor-form-builder", "aria-label": "Form settings",
            legend { "Form settings" }
            p { class: "editor-keyboard-help", "Submission purpose, required request consent, and optional marketing consent are stored separately." }
            label { class: "editor-setting-control",
                "Submission purpose"
                textarea {
                    rows: "2",
                    value: "{purpose}",
                    disabled: locked,
                    onchange: move |event| {
                        let value = event.value();
                        if !value.trim().is_empty() && value.len() <= 1024 {
                            dispatch_v2_edit(state, Edit::SetFormPurpose { form: form_id_purpose.clone(), purpose: value });
                        }
                    },
                }
            }
            if let Some(message) = result_message {
                label { class: "editor-setting-control",
                    "After submission"
                    textarea {
                        rows: "2",
                        value: "{message}",
                        disabled: locked,
                        onchange: move |event| {
                            let value = event.value();
                            if !value.trim().is_empty() && value.len() <= 1024 {
                                dispatch_v2_edit(state, Edit::SetFormResult {
                                    form: form_id_result.clone(),
                                    result: FormResult::Inline { message: value },
                                });
                            }
                        },
                    }
                }
            } else {
                if let FormResult::SuccessPage { page_id } = form.result() {
                    p { class: "editor-keyboard-help", "After submission, visitors go to page {page_id.as_str()}." }
                }
            }
            label { class: "editor-setting-control",
                "Required request consent"
                textarea {
                    rows: "2",
                    value: "{request_consent_copy}",
                    disabled: locked,
                    onchange: move |event| {
                        let value = event.value();
                        if !value.trim().is_empty() && value.len() <= 1024 {
                            dispatch_v2_edit(state, Edit::SetFormRequestConsent {
                                form: form_id_consent.clone(),
                                copy: value,
                            });
                        }
                    },
                }
            }
            label { class: "editor-form-checkbox",
                input {
                    r#type: "checkbox",
                    checked: marketing_enabled,
                    disabled: locked,
                    onchange: move |event| {
                        let value = if event.checked() {
                            Some(marketing_copy_for_toggle.clone().unwrap_or_else(|| "Send me occasional product updates.".to_owned()))
                        } else {
                            None
                        };
                        dispatch_v2_edit(state, Edit::SetFormMarketingOptIn {
                            form: form_id_marketing.clone(),
                            copy: value,
                        });
                    },
                }
                "Offer separate, optional marketing consent"
            }
            if let Some(copy) = marketing_copy {
                label { class: "editor-setting-control",
                    "Optional marketing consent wording"
                    textarea {
                        rows: "2",
                        value: "{copy}",
                        disabled: locked,
                        onchange: move |event| {
                            let value = event.value();
                            if !value.trim().is_empty() && value.len() <= 1024 {
                                dispatch_v2_edit(state, Edit::SetFormMarketingOptIn {
                                    form: form_id_marketing_copy.clone(),
                                    copy: Some(value),
                                });
                            }
                        },
                    }
                }
            }
        }
        fieldset { class: "editor-form-builder", "aria-label": "Form fields",
            legend { "Fields ({fields.len()}/50)" }
            div { class: "editor-form-field-list", role: "list", "aria-label": "Form fields",
                for (index, field) in fields.iter().enumerate() {
                    {
                        let field_id = field.id().clone();
                        let field_name_id = field.id().as_str().to_owned();
                        let type_id = format!("form-{}-field-{}-type", form_id.as_str(), field_name_id);
                        let name_id = format!("form-{}-field-{}-name", form_id.as_str(), field_name_id);
                        let label_id = format!("form-{}-field-{}-label", form_id.as_str(), field_name_id);
                        let help_id = format!("form-{}-field-{}-help", form_id.as_str(), field_name_id);
                        let options_id = format!("form-{}-field-{}-options", form_id.as_str(), field_name_id);
                        let current_name = form_field_name(field).to_owned();
                        let current_label = form_field_label(field).to_owned();
                        let current_help = form_field_help(field).unwrap_or_default().to_owned();
                        let current_required = form_field_required(field);
                        let current_kind = form_field_kind_id(field).to_owned();
                        let current_options = form_field_options(field)
                            .map(|values| values.join("\n"))
                            .unwrap_or_default();
                        let current_validation = form_field_validation(field).cloned();
                        let current_min_length = current_validation
                            .as_ref()
                            .and_then(|value| value.min_length)
                            .map(|value| value.to_string())
                            .unwrap_or_default();
                        let current_max_length = current_validation
                            .as_ref()
                            .and_then(|value| value.max_length)
                            .map(|value| value.to_string())
                            .unwrap_or_default();
                        let current_min_length_for_update = current_min_length.clone();
                        let current_max_length_for_update = current_max_length.clone();
                        let field_for_type = field.clone();
                        let field_for_name = field.clone();
                        let field_for_label = field.clone();
                        let field_for_help = field.clone();
                        let field_for_required = field.clone();
                        let field_for_options = field.clone();
                        let field_for_min = field.clone();
                        let field_for_max = field.clone();
                        let form_for_type = form_id.clone();
                        let form_for_name = form_id.clone();
                        let form_for_label = form_id.clone();
                        let form_for_help = form_id.clone();
                        let form_for_required = form_id.clone();
                        let form_for_options = form_id.clone();
                        let form_for_min = form_id.clone();
                        let form_for_max = form_id.clone();
                        let id_for_name = field_id.clone();
                        let id_for_label = field_id.clone();
                        let id_for_help = field_id.clone();
                        let id_for_required = field_id.clone();
                        let id_for_type = field_id.clone();
                        let id_for_options = field_id.clone();
                        let id_for_min = field_id.clone();
                        let id_for_max = field_id.clone();
                        let field_ids = fields.iter().map(|item| item.id().clone()).collect::<Vec<_>>();
                        let field_ids_up = field_ids.clone();
                        let field_ids_down = field_ids;
                        let form_for_up = form_id.clone();
                        let form_for_down = form_id.clone();
                        let form_for_delete = form_id.clone();
                        let field_for_delete = field_id.clone();
                        let count = fields.len();
                        let help_value = current_help.clone();
                        rsx! {
                            div { class: "editor-form-field-row", role: "listitem",
                                fieldset { class: "editor-form-field",
                                    legend { "Field {index + 1}" }
                                label { class: "editor-setting-control", r#for: "{type_id}",
                                    "Field type"
                                    select {
                                        id: "{type_id}",
                                        value: "{current_kind}",
                                        disabled: locked,
                                        onchange: move |event| {
                                            if let Some(kind) = V2NewFormFieldKind::from_id(&event.value()) {
                                                dispatch_v2_edit(state, Edit::SetFormField {
                                                    form: form_for_type.clone(),
                                                    field: id_for_type.clone(),
                                                    value: new_form_field_as_kind(&field_for_type, kind),
                                                });
                                            }
                                        },
                                        for kind in V2NewFormFieldKind::ALL {
                                            option { value: "{kind.id()}", "{kind.label()}" }
                                        }
                                    }
                                }
                                label { class: "editor-setting-control", r#for: "{name_id}",
                                    "Submission key"
                                    input {
                                        id: "{name_id}",
                                        value: "{current_name}",
                                        disabled: locked,
                                        onchange: move |event| {
                                            let value = event.value();
                                            if valid_form_field_name_input(&value) {
                                                dispatch_v2_edit(state, Edit::SetFormField {
                                                    form: form_for_name.clone(),
                                                    field: id_for_name.clone(),
                                                    value: new_form_field_with_common(
                                                        &field_for_name,
                                                        value,
                                                        form_field_label(&field_for_name).to_owned(),
                                                        form_field_help(&field_for_name).map(str::to_owned),
                                                        form_field_required(&field_for_name),
                                                    ),
                                                });
                                            }
                                        },
                                    }
                                }
                                label { class: "editor-setting-control", r#for: "{label_id}",
                                    "Visible label"
                                    input {
                                        id: "{label_id}",
                                        value: "{current_label}",
                                        disabled: locked,
                                        onchange: move |event| {
                                            let value = event.value();
                                            if !value.trim().is_empty() && value.len() <= 1024 {
                                                dispatch_v2_edit(state, Edit::SetFormField {
                                                    form: form_for_label.clone(),
                                                    field: id_for_label.clone(),
                                                    value: new_form_field_with_common(
                                                        &field_for_label,
                                                        form_field_name(&field_for_label).to_owned(),
                                                        value,
                                                        form_field_help(&field_for_label).map(str::to_owned),
                                                        form_field_required(&field_for_label),
                                                    ),
                                                });
                                            }
                                        },
                                    }
                                }
                                label { class: "editor-setting-control", r#for: "{help_id}",
                                    "Help text (optional)"
                                    input {
                                        id: "{help_id}",
                                        value: "{help_value}",
                                        disabled: locked,
                                        onchange: move |event| {
                                            let value = event.value();
                                            if value.len() <= 1024 {
                                                dispatch_v2_edit(state, Edit::SetFormField {
                                                    form: form_for_help.clone(),
                                                    field: id_for_help.clone(),
                                                    value: new_form_field_with_common(
                                                        &field_for_help,
                                                        form_field_name(&field_for_help).to_owned(),
                                                        form_field_label(&field_for_help).to_owned(),
                                                        (!value.is_empty()).then_some(value),
                                                        form_field_required(&field_for_help),
                                                    ),
                                                });
                                            }
                                        },
                                    }
                                }
                                label { class: "editor-form-checkbox",
                                    input {
                                        r#type: "checkbox",
                                        checked: current_required,
                                        disabled: locked,
                                        onchange: move |event| {
                                            dispatch_v2_edit(state, Edit::SetFormField {
                                                form: form_for_required.clone(),
                                                field: id_for_required.clone(),
                                                value: new_form_field_with_common(
                                                    &field_for_required,
                                                    form_field_name(&field_for_required).to_owned(),
                                                    form_field_label(&field_for_required).to_owned(),
                                                    form_field_help(&field_for_required).map(str::to_owned),
                                                    event.checked(),
                                                ),
                                            });
                                        },
                                    }
                                    "Required"
                                }
                                if form_field_validation_cap(field).is_some() {
                                    div { class: "editor-form-length-row",
                                        label { class: "editor-setting-control",
                                            "Minimum characters"
                                            input {
                                                r#type: "number",
                                                min: "0",
                                                max: "{form_field_validation_cap(field).unwrap_or_default()}",
                                                value: "{current_min_length}",
                                                disabled: locked,
                                                onchange: move |event| {
                                                    if let Some(value) = validation_range_update(
                                                        &field_for_min,
                                                        &event.value(),
                                                        &current_max_length_for_update,
                                                    ) {
                                                        dispatch_v2_edit(state, Edit::SetFormField {
                                                            form: form_for_min.clone(),
                                                            field: id_for_min.clone(),
                                                            value,
                                                        });
                                                    }
                                                },
                                            }
                                        }
                                        label { class: "editor-setting-control",
                                            "Maximum characters"
                                            input {
                                                r#type: "number",
                                                min: "1",
                                                max: "{form_field_validation_cap(field).unwrap_or_default()}",
                                                value: "{current_max_length}",
                                                disabled: locked,
                                                onchange: move |event| {
                                                    if let Some(value) = validation_range_update(
                                                        &field_for_max,
                                                        &current_min_length_for_update,
                                                        &event.value(),
                                                    ) {
                                                        dispatch_v2_edit(state, Edit::SetFormField {
                                                            form: form_for_max.clone(),
                                                            field: id_for_max.clone(),
                                                            value,
                                                        });
                                                    }
                                                },
                                            }
                                        }
                                    }
                                }
                                if matches!(field, FormField::Select { .. }) {
                                    label { class: "editor-setting-control", r#for: "{options_id}",
                                        "Dropdown options (one per line)"
                                        textarea {
                                            id: "{options_id}",
                                            rows: "3",
                                            value: "{current_options}",
                                            disabled: locked,
                                            onchange: move |event| {
                                                if let Some(options) = parse_form_options(&event.value()) {
                                                    if let Some(updated) = form_field_with_options(&field_for_options, options) {
                                                        dispatch_v2_edit(state, Edit::SetFormField {
                                                            form: form_for_options.clone(),
                                                            field: id_for_options.clone(),
                                                            value: updated,
                                                        });
                                                    }
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
                                        "aria-label": "Move form field {index + 1} up",
                                        onclick: move |_| {
                                            let mut ordered = field_ids_up.clone();
                                            if index > 0 { ordered.swap(index, index - 1); }
                                            dispatch_v2_edit(state, Edit::ReorderFormFields { form: form_for_up.clone(), ordered });
                                        },
                                        "Up"
                                    }
                                    button {
                                        class: "editor-move-button",
                                        r#type: "button",
                                        disabled: locked || index + 1 >= count,
                                        "aria-label": "Move form field {index + 1} down",
                                        onclick: move |_| {
                                            let mut ordered = field_ids_down.clone();
                                            if index + 1 < ordered.len() { ordered.swap(index, index + 1); }
                                            dispatch_v2_edit(state, Edit::ReorderFormFields { form: form_for_down.clone(), ordered });
                                        },
                                        "Down"
                                    }
                                    button {
                                        class: "editor-move-button",
                                        r#type: "button",
                                        disabled: locked || count <= 1,
                                        "aria-label": "Delete form field {index + 1}",
                                        onclick: move |_| dispatch_v2_edit(state, Edit::DeleteFormField {
                                            form: form_for_delete.clone(),
                                            field: field_for_delete.clone(),
                                        }),
                                        "Delete"
                                    }
                                }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "editor-form-add-fields", "aria-label": "Add a form field",
                for kind in V2NewFormFieldKind::ALL {
                    {
                        let form_for_add = form_id.clone();
                        let field = fields.last().map(|field| field.id().clone());
                        let form_for_field = form.clone();
                        rsx! {
                            button {
                                class: "secondary-button",
                                r#type: "button",
                                disabled: locked || fields.len() >= 50,
                                onclick: move |_| dispatch_v2_edit(state, Edit::AddFormField {
                                    form: form_for_add.clone(),
                                    after: field.clone(),
                                    field: kind.create(&form_for_field),
                                }),
                                "Add {kind.label().to_lowercase()} field"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(any(feature = "browser", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ThemeColorField {
    Background,
    Border,
    Muted,
    OnPrimary,
    Primary,
    Surface,
    Text,
}

#[cfg(any(feature = "browser", test))]
impl ThemeColorField {
    const ALL: [Self; 7] = [
        Self::Background,
        Self::Surface,
        Self::Text,
        Self::Muted,
        Self::Primary,
        Self::OnPrimary,
        Self::Border,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Border => "border",
            Self::Muted => "muted",
            Self::OnPrimary => "on-primary",
            Self::Primary => "primary",
            Self::Surface => "surface",
            Self::Text => "text",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Background => "Page background",
            Self::Border => "Borders",
            Self::Muted => "Secondary text",
            Self::OnPrimary => "Text on buttons",
            Self::Primary => "Buttons and accents",
            Self::Surface => "Cards and form surfaces",
            Self::Text => "Main text",
        }
    }

    fn token(self) -> &'static str {
        match self {
            Self::Background => "color-background",
            Self::Border => "color-border",
            Self::Muted => "color-muted",
            Self::OnPrimary => "color-on-primary",
            Self::Primary => "color-primary",
            Self::Surface => "color-surface",
            Self::Text => "color-text",
        }
    }

    fn override_value(self, colors: Option<&ThemeColors>) -> Option<&str> {
        let colors = colors?;
        match self {
            Self::Background => colors.color_background.as_deref(),
            Self::Border => colors.color_border.as_deref(),
            Self::Muted => colors.color_muted.as_deref(),
            Self::OnPrimary => colors.color_on_primary.as_deref(),
            Self::Primary => colors.color_primary.as_deref(),
            Self::Surface => colors.color_surface.as_deref(),
            Self::Text => colors.color_text.as_deref(),
        }
    }

    fn set_override(self, colors: &mut ThemeColors, value: Option<String>) {
        match self {
            Self::Background => colors.color_background = value,
            Self::Border => colors.color_border = value,
            Self::Muted => colors.color_muted = value,
            Self::OnPrimary => colors.color_on_primary = value,
            Self::Primary => colors.color_primary = value,
            Self::Surface => colors.color_surface = value,
            Self::Text => colors.color_text = value,
        }
    }
}

#[cfg(any(feature = "browser", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ThemeStyleField {
    TypeScale,
    FontFamily,
    SpacingScale,
    CornerScale,
    ContentWidth,
    ButtonShape,
    Shadow,
}

#[cfg(any(feature = "browser", test))]
impl ThemeStyleField {
    const ALL: [Self; 7] = [
        Self::TypeScale,
        Self::FontFamily,
        Self::SpacingScale,
        Self::ContentWidth,
        Self::CornerScale,
        Self::ButtonShape,
        Self::Shadow,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::TypeScale => "type-scale",
            Self::FontFamily => "font-family",
            Self::SpacingScale => "spacing-scale",
            Self::CornerScale => "corner-scale",
            Self::ContentWidth => "content-width",
            Self::ButtonShape => "button-shape",
            Self::Shadow => "shadow",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::TypeScale => "Text size",
            Self::FontFamily => "Font family",
            Self::SpacingScale => "Section spacing",
            Self::CornerScale => "Card corners",
            Self::ContentWidth => "Content width",
            Self::ButtonShape => "Button shape",
            Self::Shadow => "Card shadow",
        }
    }

    fn choices(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::TypeScale => &[
                ("compact", "Compact"),
                ("default", "Default"),
                ("large", "Large"),
                ("display", "Display"),
            ],
            Self::FontFamily => &[
                ("system_ui", "System UI"),
                ("arial", "Arial"),
                ("georgia", "Georgia"),
                ("verdana", "Verdana"),
                ("trebuchet_ms", "Trebuchet MS"),
            ],
            Self::SpacingScale => &[
                ("compact", "Compact"),
                ("default", "Default"),
                ("relaxed", "Relaxed"),
                ("airy", "Airy"),
                ("spacious", "Spacious"),
            ],
            Self::CornerScale => &[
                ("square", "Square"),
                ("soft", "Soft"),
                ("rounded", "Rounded"),
                ("large", "Large"),
                ("extra_large", "Extra large"),
            ],
            Self::ContentWidth => &[
                ("narrow", "Narrow"),
                ("default", "Default"),
                ("wide", "Wide"),
            ],
            Self::ButtonShape => &[
                ("square", "Square"),
                ("rounded", "Rounded"),
                ("soft", "Soft"),
                ("pill", "Pill"),
            ],
            Self::Shadow => &[("none", "None"), ("subtle", "Subtle"), ("raised", "Raised")],
        }
    }

    fn override_id(self, overrides: &ThemeOverrides) -> Option<&'static str> {
        match self {
            Self::TypeScale => overrides.type_scale.map(|value| match value {
                TypeScale::Compact => "compact",
                TypeScale::Default => "default",
                TypeScale::Large => "large",
                TypeScale::Display => "display",
            }),
            Self::FontFamily => overrides.font_family.map(|value| match value {
                FontFamily::SystemUi => "system_ui",
                FontFamily::Arial => "arial",
                FontFamily::Georgia => "georgia",
                FontFamily::Verdana => "verdana",
                FontFamily::TrebuchetMs => "trebuchet_ms",
            }),
            Self::SpacingScale => overrides.spacing_scale.map(|value| match value {
                SpacingScale::Compact => "compact",
                SpacingScale::Default => "default",
                SpacingScale::Relaxed => "relaxed",
                SpacingScale::Airy => "airy",
                SpacingScale::Spacious => "spacious",
            }),
            Self::CornerScale => overrides.corner_scale.map(|value| match value {
                CornerScale::Square => "square",
                CornerScale::Soft => "soft",
                CornerScale::Rounded => "rounded",
                CornerScale::Large => "large",
                CornerScale::ExtraLarge => "extra_large",
            }),
            Self::ContentWidth => overrides.content_width.map(|value| match value {
                ContentWidth::Narrow => "narrow",
                ContentWidth::Default => "default",
                ContentWidth::Wide => "wide",
            }),
            Self::ButtonShape => overrides.button_shape.map(|value| match value {
                ButtonShape::Square => "square",
                ButtonShape::Rounded => "rounded",
                ButtonShape::Soft => "soft",
                ButtonShape::Pill => "pill",
            }),
            Self::Shadow => overrides.shadow.map(|value| match value {
                Shadow::None => "none",
                Shadow::Subtle => "subtle",
                Shadow::Raised => "raised",
            }),
        }
    }

    fn set_override(self, overrides: &mut ThemeOverrides, choice: &str) -> bool {
        match self {
            Self::TypeScale => {
                overrides.type_scale = match choice {
                    "preset" => None,
                    "compact" => Some(TypeScale::Compact),
                    "default" => Some(TypeScale::Default),
                    "large" => Some(TypeScale::Large),
                    "display" => Some(TypeScale::Display),
                    _ => return false,
                }
            }
            Self::FontFamily => {
                overrides.font_family = match choice {
                    "preset" => None,
                    "system_ui" => Some(FontFamily::SystemUi),
                    "arial" => Some(FontFamily::Arial),
                    "georgia" => Some(FontFamily::Georgia),
                    "verdana" => Some(FontFamily::Verdana),
                    "trebuchet_ms" => Some(FontFamily::TrebuchetMs),
                    _ => return false,
                }
            }
            Self::SpacingScale => {
                overrides.spacing_scale = match choice {
                    "preset" => None,
                    "compact" => Some(SpacingScale::Compact),
                    "default" => Some(SpacingScale::Default),
                    "relaxed" => Some(SpacingScale::Relaxed),
                    "airy" => Some(SpacingScale::Airy),
                    "spacious" => Some(SpacingScale::Spacious),
                    _ => return false,
                }
            }
            Self::CornerScale => {
                overrides.corner_scale = match choice {
                    "preset" => None,
                    "square" => Some(CornerScale::Square),
                    "soft" => Some(CornerScale::Soft),
                    "rounded" => Some(CornerScale::Rounded),
                    "large" => Some(CornerScale::Large),
                    "extra_large" => Some(CornerScale::ExtraLarge),
                    _ => return false,
                }
            }
            Self::ContentWidth => {
                overrides.content_width = match choice {
                    "preset" => None,
                    "narrow" => Some(ContentWidth::Narrow),
                    "default" => Some(ContentWidth::Default),
                    "wide" => Some(ContentWidth::Wide),
                    _ => return false,
                }
            }
            Self::ButtonShape => {
                overrides.button_shape = match choice {
                    "preset" => None,
                    "square" => Some(ButtonShape::Square),
                    "rounded" => Some(ButtonShape::Rounded),
                    "soft" => Some(ButtonShape::Soft),
                    "pill" => Some(ButtonShape::Pill),
                    _ => return false,
                }
            }
            Self::Shadow => {
                overrides.shadow = match choice {
                    "preset" => None,
                    "none" => Some(Shadow::None),
                    "subtle" => Some(Shadow::Subtle),
                    "raised" => Some(Shadow::Raised),
                    _ => return false,
                }
            }
        }
        true
    }
}

#[cfg(feature = "browser")]
fn set_theme_color(state: Signal<V2EditorState>, field: ThemeColorField, value: Option<String>) {
    let Some(mut overrides) = state.with(|state| {
        state
            .document
            .as_ref()
            .map(|document| document.theme().overrides().clone())
    }) else {
        return;
    };
    let mut colors = overrides.colors.take().unwrap_or_default();
    field.set_override(&mut colors, value);
    let has_colors = colors.color_background.is_some()
        || colors.color_border.is_some()
        || colors.color_muted.is_some()
        || colors.color_on_primary.is_some()
        || colors.color_primary.is_some()
        || colors.color_surface.is_some()
        || colors.color_text.is_some();
    overrides.colors = if has_colors { Some(colors) } else { None };
    dispatch_v2_edit(
        state,
        Edit::SetThemeOverrides {
            overrides: Box::new(overrides),
        },
    );
}

#[cfg(feature = "browser")]
fn set_theme_style(state: Signal<V2EditorState>, field: ThemeStyleField, choice: &str) {
    let Some(mut overrides) = state.with(|state| {
        state
            .document
            .as_ref()
            .map(|document| document.theme().overrides().clone())
    }) else {
        return;
    };
    if field.set_override(&mut overrides, choice) {
        dispatch_v2_edit(
            state,
            Edit::SetThemeOverrides {
                overrides: Box::new(overrides),
            },
        );
    }
}

#[cfg(any(feature = "browser", test))]
fn theme_contrast_warnings(tokens: &BTreeMap<String, String>) -> Vec<String> {
    let pairs = [
        (
            "color-text",
            "color-background",
            "Main text on page background",
        ),
        ("color-text", "color-surface", "Main text on cards"),
        (
            "color-muted",
            "color-background",
            "Secondary text on page background",
        ),
        ("color-muted", "color-surface", "Secondary text on cards"),
        ("color-on-primary", "color-primary", "Button text"),
    ];
    pairs
        .into_iter()
        .filter_map(|(foreground, background, label)| {
            let foreground = tokens.get(foreground)?;
            let background = tokens.get(background)?;
            let ratio = contrast_ratio(foreground, background)?;
            (ratio < 4.5).then(|| format!("{label} ({ratio:.2}:1)"))
        })
        .collect()
}

#[cfg(any(feature = "browser", test))]
fn contrast_ratio(first: &str, second: &str) -> Option<f64> {
    fn luminance(value: &str) -> Option<f64> {
        let bytes = value.as_bytes();
        if bytes.len() != 7 || bytes[0] != b'#' {
            return None;
        }
        let mut channels = [0.0; 3];
        for (index, channel) in channels.iter_mut().enumerate() {
            let start = index * 2 + 1;
            let value = u8::from_str_radix(std::str::from_utf8(&bytes[start..start + 2]).ok()?, 16)
                .ok()? as f64
                / 255.0;
            *channel = if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            };
        }
        Some(0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2])
    }

    let first = luminance(first)?;
    let second = luminance(second)?;
    let (lighter, darker) = if first >= second {
        (first, second)
    } else {
        (second, first)
    };
    Some((lighter + 0.05) / (darker + 0.05))
}

#[cfg(feature = "browser")]
#[component]
fn V2ThemeCustomization(
    state: Signal<V2EditorState>,
    document: DocumentV2,
    locked: bool,
) -> Element {
    let overrides = document.theme().overrides().clone();
    let profile = document.resolved_theme_profile();
    let warnings = theme_contrast_warnings(&profile.desktop);
    let contrast_message = if warnings.is_empty() {
        "Current text and button colors meet the 4.5:1 contrast target.".to_owned()
    } else {
        format!(
            "Low contrast: {}. Adjust these colors to improve readability.",
            warnings.join(", ")
        )
    };
    let contrast_class = if warnings.is_empty() {
        "theme-contrast-status"
    } else {
        "theme-contrast-status theme-contrast-warning"
    };
    rsx! {
        fieldset { class: "editor-setting-group theme-customization",
            legend { "Customize theme" }
            p { class: "editor-keyboard-help", "These settings update the live preview. Save revision when the design is ready." }
            fieldset { class: "theme-token-group",
                legend { "Colors" }
                p { class: "editor-keyboard-help", "Choose colors for each part of the page. Reset any value to use the selected preset." }
                div { class: "theme-color-grid",
                    for field in ThemeColorField::ALL {
                        {
                            let value = profile.desktop.get(field.token()).cloned().unwrap_or_else(|| "#ffffff".to_owned());
                            let has_override = field.override_value(overrides.colors.as_ref()).is_some();
                            rsx! {
                                V2ThemeColorControl { state, field, value, has_override, locked }
                            }
                        }
                    }
                }
                p {
                    class: "{contrast_class}",
                    role: "status",
                    "aria-live": "polite",
                    "{contrast_message}"
                }
            }
            fieldset { class: "theme-token-group",
                legend { "Type and layout" }
                div { class: "theme-style-grid",
                    for field in ThemeStyleField::ALL {
                        V2ThemeStyleControl { state, field, overrides: overrides.clone(), locked }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "browser")]
#[component]
fn V2ThemeColorControl(
    state: Signal<V2EditorState>,
    field: ThemeColorField,
    value: String,
    has_override: bool,
    locked: bool,
) -> Element {
    let input_id = format!("v2-theme-color-{}", field.id());
    rsx! {
        div { class: "theme-color-control",
            label { r#for: "{input_id}", "{field.label()}" }
            div { class: "theme-color-input-row",
                input {
                    id: "{input_id}",
                    r#type: "color",
                    value: "{value}",
                    disabled: locked,
                    onchange: move |event| set_theme_color(state, field, Some(event.value())),
                }
                button {
                    class: "text-button",
                    r#type: "button",
                    disabled: locked || !has_override,
                    "aria-label": "Use preset {field.label().to_lowercase()}",
                    onclick: move |_| set_theme_color(state, field, None),
                    "Use preset"
                }
            }
        }
    }
}

#[cfg(feature = "browser")]
#[component]
fn V2ThemeStyleControl(
    state: Signal<V2EditorState>,
    field: ThemeStyleField,
    overrides: ThemeOverrides,
    locked: bool,
) -> Element {
    let input_id = format!("v2-theme-{}", field.id());
    let value = field.override_id(&overrides).unwrap_or("preset");
    rsx! {
        label { class: "theme-style-control", r#for: "{input_id}",
            span { "{field.label()}" }
            select {
                id: "{input_id}",
                value: "{value}",
                disabled: locked,
                onchange: move |event| set_theme_style(state, field, &event.value()),
                option { value: "preset", "Preset default" }
                for (choice, label) in field.choices() {
                    option { value: "{choice}", "{label}" }
                }
            }
        }
    }
}

#[cfg(feature = "browser")]
#[component]
fn V2ImageBlockPicker(
    state: Signal<V2EditorState>,
    section: SectionId,
    after: Option<BlockId>,
    locked: bool,
) -> Element {
    let media_items = use_signal(Vec::<DashboardMediaItem>::new);
    let loading = use_signal(|| true);
    let mut selected_asset = use_signal(String::new);
    let mut alt_text = use_signal(String::new);
    let mut decorative = use_signal(|| false);
    let mut message = use_signal(|| None::<String>);

    {
        let mut media_items = media_items;
        let mut loading = loading;
        let mut message = message;
        use_effect(move || {
            spawn(async move {
                let result = api_request("GET", "/api/media", None, None).await;
                match result {
                    Ok(response) if response.status == 200 => {
                        match serde_json::from_str::<DashboardMediaPage>(&response.body) {
                            Ok(page) => {
                                let first = page
                                    .items
                                    .first()
                                    .map(|item| item.descriptor.asset_id().as_str().to_owned());
                                media_items.set(page.items);
                                if let Some(first) = first {
                                    selected_asset.set(first);
                                }
                            }
                            Err(_) => {
                                message.set(Some("The media list could not be read.".to_owned()))
                            }
                        }
                    }
                    _ => message.set(Some(
                        "The media library is unavailable right now.".to_owned(),
                    )),
                }
                loading.set(false);
            });
        });
    }

    let assets = media_items();
    let selected = selected_asset();
    let selected_item = assets
        .iter()
        .find(|item| item.descriptor.asset_id().as_str() == selected)
        .cloned();
    let alt = alt_text();
    let decorative_value = decorative();
    let section_for_add = section.clone();
    let after_for_add = after.clone();
    rsx! {
        div { class: "media-image-picker",
            h3 { "Add an image block" }
            if loading() {
                p { role: "status", "Loading available images…" }
            } else if assets.is_empty() {
                p { "Upload an image before adding it to this page." }
                Link { to: super::super::RouteView::MediaLibrary {}, class: "secondary-button", "Open media library" }
            } else {
                label { class: "editor-setting-control", r#for: "v2-image-asset-select",
                    "Image"
                    select {
                        id: "v2-image-asset-select",
                        value: "{selected}",
                        disabled: locked,
                        onchange: move |event| selected_asset.set(event.value()),
                        option { value: "", "Choose an image" }
                        for item in assets.iter() {
                            option { value: "{item.descriptor.asset_id().as_str()}", "{item.display_name}" }
                        }
                    }
                }
                if let Some(item) = selected_item.as_ref() {
                    div { class: "media-image-choice",
                        img {
                            src: "/api/media/{item.descriptor.asset_id().as_str()}/content",
                            alt: "",
                            width: "{item.descriptor.width()}",
                            height: "{item.descriptor.height()}"
                        }
                        span { "{item.display_name} · {item.descriptor.width()} × {item.descriptor.height()}" }
                    }
                }
                label { class: "editor-setting-control", r#for: "v2-new-image-alt",
                    "Text alternative"
                    textarea {
                        id: "v2-new-image-alt",
                        rows: "2",
                        value: "{alt}",
                        disabled: locked,
                        oninput: move |event| {
                            let value = event.value();
                            if !value.trim().is_empty() {
                                decorative.set(false);
                            }
                            alt_text.set(value);
                        },
                    }
                }
                label { class: "media-decorative-control",
                    input {
                        r#type: "checkbox",
                        checked: decorative_value,
                        disabled: locked,
                        onchange: move |event| decorative.set(event.checked()),
                    }
                    span { "This image is decorative" }
                }
                button {
                    class: "secondary-button",
                    r#type: "button",
                    disabled: locked || selected.is_empty() || (!decorative_value && alt.trim().is_empty()),
                    onclick: move |_| {
                        let Ok(asset_id) = AssetId::parse(selected.clone()) else {
                            message.set(Some("Choose an available image first.".to_owned()));
                            return;
                        };
                        let alt = if decorative_value && alt.trim().is_empty() {
                            ImageAlt::Decorative
                        } else {
                            ImageAlt::Text { value: alt.clone() }
                        };
                        let block = NewBlock::Image { asset_id, alt };
                        dispatch_v2_edit(state, Edit::AddBlock {
                            section: section_for_add.clone(),
                            after: after_for_add.clone(),
                            block,
                        });
                        message.set(Some("Image block added. Review its alternative text before publishing.".to_owned()));
                    },
                    "Add image block"
                }
            }
            if let Some(message) = message() {
                p { class: "media-picker-message", role: "status", "{message}" }
            }
            Link { to: super::super::RouteView::MediaLibrary {}, class: "media-library-inline-link", "Manage images" }
        }
    }
}

#[cfg(feature = "browser")]
#[component]
fn V2ImageBlockSettings(state: Signal<V2EditorState>, block: BlockV2, locked: bool) -> Element {
    let BlockV2::Image { id, asset_id, alt } = block else {
        return rsx! {};
    };
    let alt_text = match &alt {
        ImageAlt::Text { value } => value.clone(),
        ImageAlt::Decorative => String::new(),
    };
    let decorative = matches!(alt, ImageAlt::Decorative);
    let image_url = format!("/api/media/{}/content", asset_id.as_str());
    let block_id = id.clone();
    let asset_for_text = asset_id.clone();
    let text_for_toggle = alt_text.clone();
    let mut message = use_signal(|| None::<String>);
    rsx! {
        div { class: "media-image-settings",
            img { src: "{image_url}", alt: "", loading: "lazy", class: "media-image-settings-preview" }
            label { class: "editor-setting-control", r#for: "v2-image-alt-{id.as_str()}",
                "Text alternative"
                textarea {
                    id: "v2-image-alt-{id.as_str()}",
                    rows: "2",
                    value: "{alt_text}",
                    disabled: locked,
                    onchange: move |event| {
                        let value = event.value();
                        if value.trim().is_empty() {
                            message.set(Some("Add a text alternative or mark the image decorative.".to_owned()));
                        } else {
                            dispatch_v2_edit(state, Edit::SetBlock {
                                block: block_id.clone(),
                                value: NewBlock::Image {
                                    asset_id: asset_for_text.clone(),
                                    alt: ImageAlt::Text { value },
                                },
                            });
                            message.set(None);
                        }
                    },
                }
            }
            label { class: "media-decorative-control",
                input {
                    r#type: "checkbox",
                    checked: decorative,
                    disabled: locked,
                    onchange: move |event| {
                        if event.checked() {
                            dispatch_v2_edit(state, Edit::SetBlock {
                                block: id.clone(),
                                value: NewBlock::Image {
                                    asset_id: asset_id.clone(),
                                    alt: ImageAlt::Decorative,
                                },
                            });
                            message.set(None);
                        } else if !text_for_toggle.trim().is_empty() {
                            dispatch_v2_edit(state, Edit::SetBlock {
                                block: id.clone(),
                                value: NewBlock::Image {
                                    asset_id: asset_id.clone(),
                                    alt: ImageAlt::Text { value: text_for_toggle.clone() },
                                },
                            });
                        } else {
                            message.set(Some("Add a text alternative before turning off decorative.".to_owned()));
                        }
                    },
                }
                span { "This image is decorative" }
            }
            if let Some(message) = message() {
                p { class: "media-picker-message", role: "status", "{message}" }
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
    let form = match &block {
        BlockV2::Form { form_id, .. } => state.with(|state| {
            state
                .document
                .as_ref()
                .and_then(|document| document.form(form_id).cloned())
        }),
        _ => None,
    };
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
            if matches!(block, BlockV2::Image { .. }) {
                V2ImageBlockSettings { state, block: block.clone(), locked }
            } else if let Some(form) = form {
                V2FormEditor { state, form, locked }
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
    fn built_in_v2_theme_text_and_button_pairs_meet_wcag_aa_contrast() {
        for preset in ThemePreset::all() {
            let document = DocumentV2::starter("funnel-theme", BlueprintId::LeadGen, *preset)
                .expect("valid V2 starter");
            let profile = document.resolved_theme_profile();
            let warnings = theme_contrast_warnings(&profile.desktop);
            assert!(warnings.is_empty(), "{}: {warnings:?}", preset.id());
        }
    }

    #[test]
    fn custom_theme_contrast_report_names_low_contrast_pairs_and_ratios() {
        let tokens = BTreeMap::from([
            ("color-background".to_owned(), "#ffffff".to_owned()),
            ("color-surface".to_owned(), "#ffffff".to_owned()),
            ("color-text".to_owned(), "#777777".to_owned()),
            ("color-muted".to_owned(), "#888888".to_owned()),
            ("color-primary".to_owned(), "#ffff00".to_owned()),
            ("color-on-primary".to_owned(), "#ffffff".to_owned()),
        ]);
        let warnings = theme_contrast_warnings(&tokens);
        assert_eq!(warnings.len(), 5);
        assert!(warnings
            .iter()
            .any(|warning| warning.starts_with("Button text (")));
        assert!(warnings.iter().all(|warning| warning.contains(":1)")));
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
    fn image_blocks_can_be_saved_after_the_media_reference_is_added() {
        let original = document();
        let section = original.pages()[0].sections()[0].id().clone();
        let mut ids = TestIds(0);
        let edited = original
            .apply(
                Edit::AddBlock {
                    section,
                    after: None,
                    block: NewBlock::Image {
                        asset_id: eez_domain::document_v2::AssetId::parse("a".repeat(64))
                            .expect("valid fixture asset id"),
                        alt: eez_domain::document_v2::ImageAlt::Text {
                            value: "A sample image".to_owned(),
                        },
                    },
                },
                &mut ids,
            )
            .expect("valid image block");
        assert!(edited.document.has_images());

        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.loading = false;
        state.saved_document_json = original.canonical_json();
        state.document = Some(edited.document);

        assert!(state.can_save());
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
