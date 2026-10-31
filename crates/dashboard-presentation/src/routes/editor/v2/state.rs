#[cfg(feature = "browser")]
use super::api::{V2EditResponse, V2RequestFailure, V2SaveResponse};
use super::api::{V2EditorMetadata, V2MediaAsset, V2RevisionSummary, V2StageCapacity};
use super::drafts::V2LocalDraft;
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PublicationDisplayState {
    Checking,
    Unknown,
    Unpublished,
    Published(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct V2EditorState {
    pub(super) funnel_id: String,
    pub(super) route_epoch: u64,
    pub(super) generation: u64,
    pub(super) loading: bool,
    pub(super) load_error: bool,
    pub(super) pending: bool,
    pub(super) saving: bool,
    pub(super) save_retryable: bool,
    pub(super) save_phase: V2SavePhase,
    pub(super) stale: bool,
    pub(super) funnel: Option<EditorFunnel>,
    pub(super) base_revision_id: String,
    pub(super) published_revision_id: Option<String>,
    pub(super) publication_readiness: Option<PublicationReadiness>,
    pub(super) document: Option<DocumentV2>,
    pub(super) saved_document_json: String,
    pub(super) preview_html: String,
    pub(super) selected_page: String,
    pub(super) selected_section: Option<String>,
    pub(super) page_input_page: Option<String>,
    pub(super) page_title_input: String,
    pub(super) page_slug_input: String,
    pub(super) page_input_error: Option<String>,
    pub(super) page_input_error_field: Option<PageInputErrorField>,
    pub(super) delete_page_confirmation: Option<String>,
    pub(super) new_section_kind: SectionKind,
    pub(super) new_block_kind: V2NewBlockKind,
    pub(super) media_assets: Vec<V2MediaAsset>,
    pub(super) media_loading: bool,
    pub(super) media_uploading: bool,
    pub(super) media_error: Option<String>,
    pub(super) selected_media_asset: Option<String>,
    pub(super) history: Vec<String>,
    pub(super) history_cursor: usize,
    pub(super) form_input_drafts: BTreeMap<String, String>,
    pub(super) form_input_errors: BTreeMap<String, String>,
    pub(super) form_pending_field_types: BTreeMap<String, String>,
    pub(super) revisions: Vec<V2RevisionSummary>,
    pub(super) stage_capacity: Option<V2StageCapacity>,
    pub(super) compare_revision_id: Option<String>,
    pub(super) compare_document: Option<DocumentV2>,
    pub(super) compare_loading: bool,
    pub(super) compare_request_epoch: u64,
    pub(super) recovered_draft: Option<V2LocalDraft>,
    pub(super) recovered_current_draft: bool,
    pub(super) recovery_error: Option<String>,
    pub(super) error: Option<String>,
    pub(super) status: Option<String>,
    pub(super) announcement: String,
    pub(super) publication_pending: bool,
    pub(super) publication_retryable: bool,
    pub(super) publication_refreshing: bool,
    pub(super) publication_state_unknown: bool,
    #[cfg(any(feature = "browser", test))]
    pub(super) publication_attempt: Option<PublicationAction>,
    pub(super) publication_error: Option<String>,
    pub(super) publication_status: Option<String>,
    pub(super) unpublish_confirmation: bool,
    #[cfg(feature = "browser")]
    pub(super) stage_key: Option<PreparedIdempotencyKey>,
    #[cfg(feature = "browser")]
    pub(super) stage_body: Option<String>,
    #[cfg(feature = "browser")]
    pub(super) save_key: Option<PreparedIdempotencyKey>,
    #[cfg(feature = "browser")]
    pub(super) save_body: Option<String>,
    #[cfg(feature = "browser")]
    pub(super) save_stage_id: Option<String>,
    #[cfg(feature = "browser")]
    pub(super) publication_key: Option<PreparedIdempotencyKey>,
    #[cfg(feature = "browser")]
    pub(super) publication_body: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PageInputErrorField {
    Title,
    Slug,
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
            save_phase: V2SavePhase::Staging,
            stale: false,
            funnel: None,
            base_revision_id: String::new(),
            published_revision_id: None,
            publication_readiness: None,
            document: None,
            saved_document_json: String::new(),
            preview_html: String::new(),
            selected_page: String::new(),
            selected_section: None,
            page_input_page: None,
            page_title_input: String::new(),
            page_slug_input: String::new(),
            page_input_error: None,
            page_input_error_field: None,
            delete_page_confirmation: None,
            new_section_kind: SectionKind::Features,
            new_block_kind: V2NewBlockKind::default(),
            media_assets: Vec::new(),
            media_loading: true,
            media_uploading: false,
            media_error: None,
            selected_media_asset: None,
            history: Vec::new(),
            history_cursor: 0,
            form_input_drafts: BTreeMap::new(),
            form_input_errors: BTreeMap::new(),
            form_pending_field_types: BTreeMap::new(),
            revisions: Vec::new(),
            stage_capacity: None,
            compare_revision_id: None,
            compare_document: None,
            compare_loading: false,
            compare_request_epoch: 0,
            recovered_draft: None,
            recovered_current_draft: false,
            recovery_error: None,
            error: None,
            status: None,
            announcement: String::new(),
            publication_pending: false,
            publication_retryable: false,
            publication_refreshing: false,
            publication_state_unknown: false,
            #[cfg(any(feature = "browser", test))]
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
            save_stage_id: None,
            #[cfg(feature = "browser")]
            publication_key: None,
            #[cfg(feature = "browser")]
            publication_body: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum V2NewBlockKind {
    #[default]
    Heading,
    Text,
    List,
    Button,
    Faq,
    Steps,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum V2SavePhase {
    #[default]
    Staging,
    Finalizing,
}

impl V2NewBlockKind {
    pub(super) const ALL: [Self; 6] = [
        Self::Heading,
        Self::Text,
        Self::List,
        Self::Button,
        Self::Faq,
        Self::Steps,
    ];

    pub(super) const fn id(self) -> &'static str {
        match self {
            Self::Heading => "heading",
            Self::Text => "text",
            Self::List => "list",
            Self::Button => "button",
            Self::Faq => "faq",
            Self::Steps => "steps",
        }
    }

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Heading => "Heading",
            Self::Text => "Text",
            Self::List => "List",
            Self::Button => "Button",
            Self::Faq => "FAQ",
            Self::Steps => "Steps",
        }
    }

    pub(super) fn from_id(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.id() == value)
    }

    pub(super) fn create(self, language: PublicLanguage) -> NewBlock {
        match self {
            Self::Heading => NewBlock::Heading {
                level: 2,
                text: public_default_text(language, "New heading", "Nouveau titre"),
            },
            Self::Text => NewBlock::Text {
                text: public_default_text(
                    language,
                    "Add your text here.",
                    "Ajoutez votre texte ici.",
                ),
            },
            Self::List => NewBlock::List {
                items: vec![
                    public_default_text(language, "First item", "Premier élément"),
                    public_default_text(language, "Second item", "Deuxième élément"),
                ],
            },
            Self::Button => NewBlock::Button {
                label: public_default_text(language, "Continue", "Continuer"),
                action: ActionV2::ExternalHttps {
                    url: "https://example.invalid".to_owned(),
                },
            },
            Self::Faq => NewBlock::Faq {
                items: vec![new_faq_item(language)],
            },
            Self::Steps => NewBlock::Steps {
                items: vec![new_step_item(language)],
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct V2EditCapture {
    pub(super) route_epoch: u64,
    pub(super) generation: u64,
    pub(super) funnel_id: String,
    pub(super) base_revision_id: String,
    pub(super) page_id: String,
    pub(super) operation_id: String,
    pub(super) edit: Edit,
    pub(super) base_document_json: String,
    pub(super) document_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct V2SaveCapture {
    pub(super) route_epoch: u64,
    pub(super) generation: u64,
    pub(super) funnel_id: String,
    pub(super) base_revision_id: String,
    pub(super) document_json: String,
    pub(super) resume_finalization: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct V2CompareCapture {
    pub(super) route_epoch: u64,
    pub(super) funnel_id: String,
    pub(super) request_epoch: u64,
    pub(super) revision_id: String,
}

#[cfg(any(feature = "browser", test))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct V2PublicationCapture {
    pub(super) route_epoch: u64,
    pub(super) funnel_id: String,
    pub(super) action: PublicationAction,
}

pub(super) struct DeterministicIds {
    pub(super) operation_id: String,
    pub(super) next: u32,
}

impl IdSource for DeterministicIds {
    fn next_id(&mut self, _kind: IdKind) -> Result<String, eez_domain::document_v2::DocumentError> {
        self.next = self.next.wrapping_add(1);
        Ok(format!("{}-{}", self.operation_id, self.next))
    }
}

impl V2EditorState {
    pub(super) fn for_route(funnel_id: String) -> Self {
        Self {
            funnel_id,
            loading: true,
            new_section_kind: SectionKind::Features,
            new_block_kind: V2NewBlockKind::Text,
            ..Self::default()
        }
    }

    pub(super) fn begin_route(&mut self, funnel_id: String) -> u64 {
        let route_epoch = self.route_epoch.wrapping_add(1);
        *self = Self::for_route(funnel_id);
        self.route_epoch = route_epoch;
        route_epoch
    }

    pub(super) fn finish_load(
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
                self.publication_readiness = loaded.metadata.publication_readiness;
                self.saved_document_json = loaded.saved_document_json.clone();
                self.document = Some(loaded.document.clone());
                self.preview_html = loaded.preview_html;
                self.selected_page = loaded.selected_page;
                self.selected_section = loaded.selected_section;
                self.clear_page_inputs();
                self.revisions = loaded.revisions;
                self.stage_capacity = loaded.stage_capacity;
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

    pub(super) fn finish_readiness_refresh(
        &mut self,
        route_epoch: u64,
        funnel_id: &str,
        revision_id: &str,
        metadata: Result<V2EditorMetadata, ()>,
    ) {
        if self.route_epoch != route_epoch
            || self.funnel_id != funnel_id
            || self.base_revision_id != revision_id
        {
            return;
        }
        if let Ok(metadata) = metadata {
            if metadata.schema_version == 2
                && metadata.funnel.id == funnel_id
                && metadata.base_revision_id == revision_id
                && metadata
                    .publication_readiness
                    .as_ref()
                    .is_some_and(|readiness| readiness.revision_id == revision_id)
            {
                self.publication_readiness = metadata.publication_readiness;
            }
        }
    }

    pub(super) fn finish_media_load(
        &mut self,
        route_epoch: u64,
        funnel_id: &str,
        result: Result<Vec<V2MediaAsset>, String>,
    ) {
        if self.route_epoch != route_epoch || self.funnel_id != funnel_id {
            return;
        }
        self.media_loading = false;
        match result {
            Ok(assets) => {
                let mut assets_by_id = assets
                    .into_iter()
                    .map(|asset| (asset.id.clone(), asset))
                    .collect::<BTreeMap<_, _>>();
                for asset in self.media_assets.drain(..) {
                    assets_by_id.entry(asset.id.clone()).or_insert(asset);
                }
                self.media_assets = assets_by_id.into_values().collect();
                self.media_assets.sort_by(|left, right| {
                    right
                        .created_at
                        .cmp(&left.created_at)
                        .then_with(|| left.id.cmp(&right.id))
                });
                if self.selected_media_asset.as_ref().is_some_and(|selected| {
                    !self.media_assets.iter().any(|asset| &asset.id == selected)
                }) {
                    self.selected_media_asset = None;
                }
                self.media_error = None;
            }
            Err(message) => self.media_error = Some(message),
        }
    }

    pub(super) fn has_unsaved_changes(&self) -> bool {
        self.document
            .as_ref()
            .is_some_and(|document| document.canonical_json() != self.saved_document_json)
    }

    pub(super) fn can_edit(&self) -> bool {
        !(self.document.is_none()
            || self.loading
            || self.pending
            || self.saving
            || self.stale
            || self.save_retryable && self.save_phase == V2SavePhase::Finalizing)
    }

    pub(super) fn can_undo(&self) -> bool {
        self.can_edit() && !self.has_uncommitted_editor_inputs() && self.history_cursor > 0
    }

    pub(super) fn can_redo(&self) -> bool {
        self.can_edit()
            && !self.has_uncommitted_editor_inputs()
            && self.history_cursor + 1 < self.history.len()
    }

    pub(super) fn has_uncommitted_editor_inputs(&self) -> bool {
        !self.form_input_drafts.is_empty()
            || !self.form_input_errors.is_empty()
            || !self.form_pending_field_types.is_empty()
            || self.has_page_input_draft()
            || self.page_input_error.is_some()
    }

    pub(super) fn has_page_input_draft(&self) -> bool {
        let Some(page_id) = self.page_input_page.as_deref() else {
            return false;
        };
        self.document
            .as_ref()
            .and_then(|document| {
                document
                    .pages()
                    .iter()
                    .find(|page| page.id().as_str() == page_id)
            })
            .is_some_and(|page| {
                self.page_title_input != page.title() || self.page_slug_input != page.slug()
            })
    }

    pub(super) fn page_input_values(
        &self,
        page_id: &str,
        title: &str,
        slug: &str,
    ) -> (String, String) {
        if self.page_input_page.as_deref() == Some(page_id) {
            (self.page_title_input.clone(), self.page_slug_input.clone())
        } else {
            (title.to_owned(), slug.to_owned())
        }
    }

    pub(super) fn update_page_input(&mut self, page_id: &str, field: &str, value: String) {
        let Some((title, slug)) = self.document.as_ref().and_then(|document| {
            document
                .pages()
                .iter()
                .find(|page| page.id().as_str() == page_id)
                .map(|page| (page.title().to_owned(), page.slug().to_owned()))
        }) else {
            return;
        };
        if self.page_input_page.as_deref() != Some(page_id) {
            self.page_input_page = Some(page_id.to_owned());
            self.page_title_input = title;
            self.page_slug_input = slug;
        }
        match field {
            "title" => self.page_title_input = value,
            "slug" => self.page_slug_input = value,
            _ => return,
        }
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let Some(page) = document
            .pages()
            .iter()
            .find(|page| page.id().as_str() == page_id)
        else {
            return;
        };
        if self.page_title_input == page.title() && self.page_slug_input == page.slug() {
            self.clear_page_inputs();
            return;
        }
        let (error, field) = if self.page_title_input.trim().is_empty()
            || self.page_title_input.len() > 8192
        {
            (
                Some("Enter a page title with 1 to 8192 bytes.".to_owned()),
                Some(PageInputErrorField::Title),
            )
        } else if page.id() != document.entry_page_id()
            && !is_valid_page_slug(&self.page_slug_input)
        {
            (
                Some("Use a lowercase page slug with letters, numbers, and hyphens.".to_owned()),
                Some(PageInputErrorField::Slug),
            )
        } else if page.id() != document.entry_page_id()
            && document
                .pages()
                .iter()
                .any(|other| other.id() != page.id() && other.slug() == self.page_slug_input)
        {
            (
                Some("Each page must have a unique slug.".to_owned()),
                Some(PageInputErrorField::Slug),
            )
        } else {
            (None, None)
        };
        self.page_input_error = error;
        self.page_input_error_field = field;
    }

    pub(super) fn clear_page_inputs(&mut self) {
        self.page_input_page = None;
        self.page_title_input.clear();
        self.page_slug_input.clear();
        self.page_input_error = None;
        self.page_input_error_field = None;
    }

    pub(super) fn prune_page_inputs(&mut self, document: &DocumentV2) {
        let Some(page_id) = self.page_input_page.as_deref() else {
            return;
        };
        let Some(page) = document
            .pages()
            .iter()
            .find(|page| page.id().as_str() == page_id)
        else {
            self.clear_page_inputs();
            return;
        };
        if self.page_title_input == page.title() && self.page_slug_input == page.slug() {
            self.clear_page_inputs();
        }
    }

    pub(super) fn prune_form_ui_state(&mut self, document: &DocumentV2) {
        let is_active = |key: &str, allow_type_setting: bool| {
            let mut parts = key.split(':');
            if parts.next() != Some("form") {
                return false;
            }
            let Some(form_id) = parts.next() else {
                return false;
            };
            let Some(kind) = parts.next() else {
                return false;
            };
            let Some(form) = document
                .forms()
                .iter()
                .find(|form| form.id().as_str() == form_id)
            else {
                return false;
            };
            match kind {
                "setting" => {
                    let Some(setting) = parts.next() else {
                        return false;
                    };
                    if parts.next().is_some() {
                        return false;
                    }
                    match setting {
                        "purpose" | "request-consent" | "submit-label" | "information-url" => true,
                        "result-message" => matches!(form.result(), FormResult::Inline { .. }),
                        "marketing-copy" => form.marketing_opt_in().is_some(),
                        _ => false,
                    }
                }
                "field" => {
                    let Some(field_id) = parts.next() else {
                        return false;
                    };
                    let Some(setting) = parts.next() else {
                        return false;
                    };
                    if parts.next().is_some() {
                        return false;
                    }
                    let Some(field) = form
                        .fields()
                        .iter()
                        .find(|field| field.id().as_str() == field_id)
                    else {
                        return false;
                    };
                    match setting {
                        "name" | "label" | "help" => true,
                        "options" => matches!(field, FormField::Select { .. }),
                        "min" | "max" => matches!(
                            field,
                            FormField::Text { .. }
                                | FormField::Email { .. }
                                | FormField::Phone { .. }
                        ),
                        "presentation" => matches!(field, FormField::Text { .. }),
                        "type" => allow_type_setting,
                        _ => false,
                    }
                }
                _ => false,
            }
        };
        self.form_input_drafts
            .retain(|key, _| is_active(key, false));
        self.form_input_errors
            .retain(|key, _| is_active(key, false));
        self.form_pending_field_types
            .retain(|key, _| is_active(key, true));
    }

    pub(super) fn can_save(&self) -> bool {
        self.document.is_some() && {
            !self.loading
                && !self.pending
                && !self.saving
                && !self.stale
                && !self.has_uncommitted_editor_inputs()
                && self.stage_capacity.is_some()
                && (self.save_retryable || self.has_unsaved_changes())
        }
    }

    pub(super) fn can_publish(&self) -> bool {
        let retrying_publish = self.publication_retryable && self.can_retry_publication_as(true);
        !self.loading
            && !self.pending
            && !self.saving
            && !self.stale
            && !self.publication_pending
            && !self.publication_refreshing
            && !self.publication_state_unknown
            && !self.has_uncommitted_editor_inputs()
            && (retrying_publish || !self.has_unsaved_changes())
            && self.document.is_some()
            && (retrying_publish
                || self.publication_readiness.as_ref().is_none_or(|readiness| {
                    readiness.revision_id == self.base_revision_id && !readiness.has_blockers()
                }))
            && self.published_revision_id.as_deref() != Some(&self.base_revision_id)
            && self.can_retry_publication_as(true)
    }

    pub(super) fn publication_display_state(&self) -> PublicationDisplayState {
        if self.publication_refreshing {
            PublicationDisplayState::Checking
        } else if self.publication_state_unknown {
            PublicationDisplayState::Unknown
        } else if let Some(revision_id) = &self.published_revision_id {
            PublicationDisplayState::Published(revision_id.clone())
        } else {
            PublicationDisplayState::Unpublished
        }
    }

    pub(super) fn show_live_funnel_link(&self) -> bool {
        matches!(
            self.publication_display_state(),
            PublicationDisplayState::Published(_)
        )
    }

    pub(super) fn can_unpublish(&self) -> bool {
        self.published_revision_id.is_some()
            && !self.publication_pending
            && !self.publication_refreshing
            && !self.publication_state_unknown
            && !self.loading
            && self.can_retry_publication_as(false)
    }

    pub(super) fn can_retry_publication_as(&self, publish: bool) -> bool {
        if !self.publication_retryable {
            return true;
        }
        #[cfg(any(feature = "browser", test))]
        {
            matches!(
                (publish, self.publication_attempt.as_ref()),
                (true, Some(PublicationAction::Publish { .. }))
                    | (false, Some(PublicationAction::Unpublish { .. }))
            )
        }
        #[cfg(not(any(feature = "browser", test)))]
        {
            let _ = publish;
            false
        }
    }

    pub(super) fn apply_local_edit(
        &mut self,
        edit: Edit,
        operation_id: String,
    ) -> Option<V2EditCapture> {
        if !self.can_edit() {
            return None;
        }
        let document = self.document.as_ref()?.clone();
        let base_document_json = document.canonical_json();
        let persisted_base =
            match DocumentV2::decode(self.saved_document_json.as_bytes(), &self.funnel_id) {
                Ok(document) => document,
                Err(_) => {
                    self.error =
                        Some("The saved revision could not be read for this edit.".to_owned());
                    self.status = None;
                    return None;
                }
            };
        let mut ids = DeterministicIds {
            operation_id: operation_id.clone(),
            next: 0,
        };
        let outcome = match document.apply(edit.clone(), &mut ids) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.error = Some(match (&edit, error.code()) {
                    (
                        Edit::SetBlock {
                            value: NewBlock::Faq { .. } | NewBlock::Steps { .. },
                            ..
                        },
                        "validation/text_length",
                    ) => {
                        "Each FAQ or step text field has a 2,048-byte UTF-8 limit.".to_owned()
                    }
                    (
                        Edit::SetBlock {
                            value: NewBlock::Faq { .. } | NewBlock::Steps { .. },
                            ..
                        },
                        "validation/functional_block_text_length",
                    ) => "This FAQ or steps block exceeds 16 KiB of UTF-8 text. Shorten an entry or remove one before continuing.".to_owned(),
                    (_, "validation/page_unreachable") => "This page cannot be deleted because it is the only route to a later step. Reconnect those steps before deleting it.".to_owned(),
                    (_, code) => format!("This change cannot be applied yet ({code})"),
                });
                self.status = None;
                return None;
            }
        };
        let edited_document = match outcome
            .document
            .reconcile_generated_placeholders_from(&persisted_base)
            .and_then(|candidate| candidate.reconcile_generated_placeholders_from(&document))
        {
            Ok(document) => document,
            Err(_) => {
                self.error =
                    Some("This change could not preserve generated-content checks.".to_owned());
                self.status = None;
                return None;
            }
        };
        let document_json = edited_document.canonical_json();
        if document_json == document.canonical_json() {
            return None;
        }
        self.prune_form_ui_state(&edited_document);
        self.prune_page_inputs(&edited_document);
        self.select_focus(&outcome.focus);
        self.document = Some(edited_document);
        self.push_history(document_json.clone());
        self.generation = self.generation.wrapping_add(1);
        self.save_retryable = false;
        self.save_phase = V2SavePhase::Staging;
        #[cfg(feature = "browser")]
        {
            self.save_stage_id = None;
        }
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
    pub(super) fn finish_edit(
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
    pub(super) fn is_current_edit(&self, capture: &V2EditCapture) -> bool {
        self.route_epoch == capture.route_epoch
            && self.generation == capture.generation
            && self.funnel_id == capture.funnel_id
            && self.base_revision_id == capture.base_revision_id
            && self.pending
    }

    pub(super) fn apply_history(&mut self, offset: isize) -> Option<(u64, String, String, String)> {
        if !self.can_edit() {
            return None;
        }
        let next = self.history_cursor.checked_add_signed(offset)?;
        let snapshot = self.history.get(next)?.clone();
        let document = DocumentV2::decode(snapshot.as_bytes(), &self.funnel_id).ok()?;
        self.history_cursor = next;
        self.selected_page = document.entry_page_id().as_str().to_owned();
        self.clear_page_inputs();
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

    pub(super) fn select_page(&mut self, page_id: &str) -> Option<(u64, String, String, String)> {
        if self.has_page_input_draft() || self.page_input_error.is_some() {
            return None;
        }
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

    pub(super) fn discard_recovered_current_draft(
        &mut self,
    ) -> Option<(u64, String, String, String)> {
        if !self.recovered_current_draft || self.saved_document_json.is_empty() {
            return None;
        }
        let document =
            DocumentV2::decode(self.saved_document_json.as_bytes(), &self.funnel_id).ok()?;
        self.selected_page = document.entry_page_id().as_str().to_owned();
        self.clear_page_inputs();
        self.selected_section = document
            .pages()
            .iter()
            .find(|page| page.id().as_str() == self.selected_page)
            .and_then(|page| page.sections().first())
            .map(|section| section.id().as_str().to_owned());
        self.document = Some(document);
        self.recovered_current_draft = false;
        self.history = vec![self.saved_document_json.clone()];
        self.history_cursor = 0;
        self.generation = self.generation.wrapping_add(1);
        self.preview_html.clear();
        self.status = Some("Recovered draft discarded.".to_owned());
        Some((
            self.generation,
            self.funnel_id.clone(),
            self.base_revision_id.clone(),
            self.saved_document_json.clone(),
        ))
    }

    pub(super) fn begin_compare(&mut self, revision_id: String) -> Option<V2CompareCapture> {
        if revision_id.is_empty() || self.compare_loading || self.document.is_none() {
            return None;
        }
        self.compare_request_epoch = self.compare_request_epoch.wrapping_add(1);
        self.compare_loading = true;
        self.compare_revision_id = Some(revision_id.clone());
        self.compare_document = None;
        self.error = None;
        Some(V2CompareCapture {
            route_epoch: self.route_epoch,
            funnel_id: self.funnel_id.clone(),
            request_epoch: self.compare_request_epoch,
            revision_id,
        })
    }

    pub(super) fn finish_compare(
        &mut self,
        capture: &V2CompareCapture,
        result: Result<DocumentV2, ()>,
    ) {
        if self.route_epoch != capture.route_epoch
            || self.funnel_id != capture.funnel_id
            || self.compare_request_epoch != capture.request_epoch
            || self.compare_revision_id.as_deref() != Some(capture.revision_id.as_str())
        {
            return;
        }
        self.compare_loading = false;
        match result {
            Ok(document) => self.compare_document = Some(document),
            Err(()) => {
                self.error = Some("That revision could not be loaded for comparison.".to_owned())
            }
        }
    }

    #[cfg(any(feature = "browser", test))]
    pub(super) fn is_current_publication(&self, capture: &V2PublicationCapture) -> bool {
        self.route_epoch == capture.route_epoch && self.funnel_id == capture.funnel_id
    }

    #[cfg(any(feature = "browser", test))]
    pub(super) fn begin_publication(&mut self, publish: bool) -> Option<V2PublicationCapture> {
        if self.publication_pending || self.publication_refreshing || self.publication_state_unknown
        {
            return None;
        }
        let action = match (
            publish,
            self.publication_attempt.as_ref(),
            self.publication_retryable,
        ) {
            (true, Some(PublicationAction::Publish { .. }), true) => {
                self.publication_attempt.clone()?
            }
            (false, Some(PublicationAction::Unpublish { .. }), true) => {
                self.publication_attempt.clone()?
            }
            (true, _, _) if self.can_publish() => PublicationAction::Publish {
                revision_id: self.base_revision_id.clone(),
                expected_published_revision_id: self.published_revision_id.clone(),
            },
            (false, _, _) if self.can_unpublish() => PublicationAction::Unpublish {
                expected_published_revision_id: self.published_revision_id.clone()?,
            },
            _ => return None,
        };
        self.publication_pending = true;
        self.publication_retryable = false;
        self.publication_attempt = Some(action.clone());
        self.publication_error = None;
        self.publication_status = Some(
            if publish {
                "Publishing the saved revision…"
            } else {
                "Unpublishing the live revision…"
            }
            .to_owned(),
        );
        Some(V2PublicationCapture {
            route_epoch: self.route_epoch,
            funnel_id: self.funnel_id.clone(),
            action,
        })
    }

    #[cfg(any(feature = "browser", test))]
    pub(super) fn finish_publication(
        &mut self,
        capture: &V2PublicationCapture,
        result: Result<Option<String>, ()>,
    ) -> bool {
        if !self.is_current_publication(capture) {
            return false;
        }
        let Ok(published_revision_id) = result else {
            self.publication_pending = false;
            self.publication_retryable = true;
            self.publication_error = Some(
                "The publication response could not be verified. Retry the same action.".to_owned(),
            );
            self.publication_status = None;
            return false;
        };
        if published_revision_id.as_deref() != capture.action.resulting_revision_id() {
            self.publication_pending = false;
            self.publication_retryable = true;
            self.publication_error = Some(
                "The publication response could not be verified. Retry the same action.".to_owned(),
            );
            self.publication_status = None;
            return false;
        }
        self.publication_pending = false;
        self.publication_retryable = false;
        self.publication_attempt = None;
        self.published_revision_id = published_revision_id;
        self.publication_state_unknown = false;
        self.publication_status = Some(
            if capture.action.resulting_revision_id().is_some() {
                "Revision published."
            } else {
                "Funnel unpublished."
            }
            .to_owned(),
        );
        self.publication_error = None;
        true
    }

    #[cfg(any(feature = "browser", test))]
    pub(super) fn begin_publication_refresh(&mut self, capture: &V2PublicationCapture) -> bool {
        if !self.is_current_publication(capture) {
            return false;
        }
        self.publication_pending = false;
        self.publication_retryable = false;
        self.publication_attempt = None;
        self.publication_refreshing = true;
        self.publication_state_unknown = true;
        self.publication_status =
            Some("Publication changed elsewhere; refreshing its current state…".to_owned());
        self.publication_error = None;
        true
    }

    #[cfg(any(feature = "browser", test))]
    pub(super) fn finish_publication_refresh(
        &mut self,
        capture: &V2PublicationCapture,
        published_revision_id: Result<Option<String>, ()>,
    ) {
        if !self.is_current_publication(capture) {
            return;
        }
        self.publication_refreshing = false;
        match published_revision_id {
            Ok(published_revision_id) => {
                self.published_revision_id = published_revision_id;
                self.publication_state_unknown = false;
                self.publication_status = Some(
                    "Publication changed in another session. The current state has been refreshed."
                        .to_owned(),
                );
                self.publication_error = None;
            }
            Err(()) => {
                self.publication_state_unknown = true;
                self.publication_status = None;
                self.publication_error = Some(
                    "Publication changed in another session, but its current state could not be loaded. Reload the editor before trying again.".to_owned(),
                );
            }
        }
    }

    pub(super) fn restore_document(
        &mut self,
        document: DocumentV2,
    ) -> Option<(u64, String, String, String)> {
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
        self.clear_page_inputs();
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

    pub(super) fn begin_save(&mut self) -> Option<V2SaveCapture> {
        if !self.can_save() {
            return None;
        }
        let resume_finalization = self.save_retryable && self.save_phase == V2SavePhase::Finalizing;
        let document_json = if resume_finalization {
            self.document.as_ref()?.canonical_json()
        } else {
            let mut document = self.document.as_ref()?.clone();
            let provenance = std::iter::once(&self.saved_document_json).chain(self.history.iter());
            for source_json in provenance {
                let source = DocumentV2::decode(source_json.as_bytes(), &self.funnel_id).ok()?;
                document = document
                    .reconcile_generated_placeholders_from(&source)
                    .ok()?;
            }
            let document_json = document.canonical_json();
            self.document = Some(document);
            if let Some(snapshot) = self.history.get_mut(self.history_cursor) {
                *snapshot = document_json.clone();
            }
            document_json
        };
        let capture = V2SaveCapture {
            route_epoch: self.route_epoch,
            generation: self.generation,
            funnel_id: self.funnel_id.clone(),
            base_revision_id: self.base_revision_id.clone(),
            document_json,
            resume_finalization,
        };
        self.saving = true;
        self.save_retryable = false;
        self.error = None;
        self.status = Some("Saving a new immutable revision…".to_owned());
        Some(capture)
    }

    #[cfg(feature = "browser")]
    pub(super) fn finish_save(
        &mut self,
        capture: &V2SaveCapture,
        response: Result<V2SaveResponse, V2RequestFailure>,
    ) -> bool {
        if !self.is_current_save(capture) {
            return false;
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
                self.publication_readiness = None;
                self.saved_document_json = capture.document_json.clone();
                self.history = vec![self.saved_document_json.clone()];
                self.history_cursor = 0;
                self.save_retryable = false;
                self.save_phase = V2SavePhase::Staging;
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
                true
            }
            Ok(_) => {
                self.save_retryable = true;
                self.error = Some(
                    "The save response could not be verified. Retry the same action to confirm it."
                        .to_owned(),
                );
                self.status = None;
                false
            }
            Err(failure) => {
                self.stale = failure.code.as_deref() == Some("conflict/edit_stale_revision");
                self.save_retryable = failure.retryable && !self.stale;
                self.error = Some(failure.message);
                self.status = None;
                false
            }
        }
    }

    #[cfg(feature = "browser")]
    pub(super) fn is_current_save(&self, capture: &V2SaveCapture) -> bool {
        self.route_epoch == capture.route_epoch
            && self.generation == capture.generation
            && self.funnel_id == capture.funnel_id
            && self.base_revision_id == capture.base_revision_id
            && self.saving
    }

    pub(super) fn select_section(&mut self, section_id: &str) {
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

    pub(super) fn push_history(&mut self, snapshot: String) {
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

    pub(super) fn select_focus(&mut self, focus: &FocusTarget) {
        match focus {
            FocusTarget::Page(page) => self.selected_page = page.as_str().to_owned(),
            FocusTarget::Section(section) => {
                self.selected_section = Some(section.as_str().to_owned())
            }
            FocusTarget::Block(_) | FocusTarget::None => {}
        }
    }
}

const EDITOR_HISTORY_LIMIT: usize = 100;

pub(super) trait CanonicalDocumentJson {
    fn canonical_json(&self) -> String;
}

impl CanonicalDocumentJson for DocumentV2 {
    fn canonical_json(&self) -> String {
        String::from_utf8(self.canonical_bytes().to_vec()).unwrap_or_default()
    }
}

pub(super) struct V2LoadedEditor {
    pub(super) metadata: V2EditorMetadata,
    pub(super) document: DocumentV2,
    pub(super) saved_document_json: String,
    pub(super) preview_html: String,
    pub(super) selected_page: String,
    pub(super) selected_section: Option<String>,
    pub(super) revisions: Vec<V2RevisionSummary>,
    pub(super) stage_capacity: Option<V2StageCapacity>,
    pub(super) recovered_draft: Option<V2LocalDraft>,
    pub(super) recovered_current_draft: bool,
    pub(super) recovery_error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use eez_domain::document_v2::{BlueprintId, PageId, SectionId};

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
        DocumentV2::starter("funnel-a", BlueprintId::Tripwire, ThemePreset::Minimal)
            .expect("valid V2 starter")
    }

    fn document_with_secondary_page() -> DocumentV2 {
        let mut value: serde_json::Value =
            serde_json::from_slice(document().canonical_bytes()).expect("starter document JSON");
        value["pages"]
            .as_array_mut()
            .expect("pages array")
            .push(serde_json::json!({
                "id": "page-thanks",
                "slug": "thanks",
                "title": "Thanks",
                "sections": [{
                    "id": "section-thanks",
                    "type": "hero",
                    "blocks": []
                }]
            }));
        value["pages"][0]["sections"][0]["blocks"]
            .as_array_mut()
            .expect("entry page blocks")
            .push(serde_json::json!({
                "id": "block-to-thanks",
                "kind": "button",
                "label": "Continue",
                "action": { "kind": "navigate_page", "page_id": "page-thanks" }
            }));
        let raw = serde_json::to_vec(&value).expect("updated document JSON");
        DocumentV2::decode(&raw, "funnel-a").expect("valid document with a secondary page")
    }

    fn document_with_section_action() -> DocumentV2 {
        let mut value: serde_json::Value =
            serde_json::from_slice(document().canonical_bytes()).expect("starter document JSON");
        value["pages"][0]["sections"][0]["blocks"]
            .as_array_mut()
            .expect("hero blocks")
            .push(serde_json::json!({
                "id":"block-to-features",
                "kind":"button",
                "label":"View features",
                "action":{"kind":"navigate_section", "section_id":"section-features"}
            }));
        DocumentV2::decode(
            &serde_json::to_vec(&value).expect("document JSON"),
            "funnel-a",
        )
        .expect("valid same-page section action")
    }

    fn stage_capacity() -> V2StageCapacity {
        V2StageCapacity {
            orphan_byte_budget: 1024,
            orphan_bytes_held: 0,
            orphan_key_budget: 10,
            orphan_keys_held: 0,
            worst_case_orphan_bytes: 1024,
            worst_case_orphan_keys: 10,
        }
    }

    fn loaded_editor(
        document: DocumentV2,
        stage_capacity: Option<V2StageCapacity>,
    ) -> V2LoadedEditor {
        let selected_page = document.entry_page_id().as_str().to_owned();
        let selected_section = document
            .pages()
            .iter()
            .find(|page| page.id().as_str() == selected_page)
            .and_then(|page| page.sections().first())
            .map(|section| section.id().as_str().to_owned());
        let saved_document_json = document.canonical_json();
        V2LoadedEditor {
            metadata: V2EditorMetadata {
                schema_version: 2,
                funnel: EditorFunnel {
                    id: "funnel-a".to_owned(),
                    name: "Funnel A".to_owned(),
                    slug: "funnel-a".to_owned(),
                    updated_at: "now".to_owned(),
                },
                base_revision_id: "revision-a".to_owned(),
                published_revision_id: None,
                publication_readiness: Some(PublicationReadiness {
                    revision_id: "revision-a".to_owned(),
                    findings: Vec::new(),
                }),
            },
            document,
            saved_document_json,
            preview_html: "<main>Preview</main>".to_owned(),
            selected_page,
            selected_section,
            revisions: Vec::new(),
            stage_capacity,
            recovered_draft: None,
            recovered_current_draft: false,
            recovery_error: None,
        }
    }

    #[test]
    fn unavailable_stage_capacity_keeps_the_editor_loaded_and_blocks_saves() {
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.finish_load(0, "funnel-a", Ok(loaded_editor(document(), None)));
        assert!(state.document.is_some());
        assert!(!state.load_error);

        let saved = state.document.as_ref().expect("loaded document");
        let mut ids = TestIds(0);
        let changed = saved
            .apply(
                Edit::SetThemePreset {
                    preset: ThemePreset::Bold,
                },
                &mut ids,
            )
            .expect("valid theme edit");
        state.document = Some(changed.document);
        assert!(state.has_unsaved_changes());
        assert!(!state.can_save());

        state.stage_capacity = Some(stage_capacity());
        assert!(state.can_save());
    }

    #[test]
    fn invalid_page_settings_remain_visible_and_block_actions_until_applied() {
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.finish_load(
            0,
            "funnel-a",
            Ok(loaded_editor(
                document_with_secondary_page(),
                Some(stage_capacity()),
            )),
        );
        let _ = state.select_page("page-thanks");
        state.update_page_input("page-thanks", "slug", "Invalid/slug".to_owned());

        assert_eq!(
            state.page_input_error.as_deref(),
            Some("Use a lowercase page slug with letters, numbers, and hyphens.")
        );
        assert!(state.has_uncommitted_editor_inputs());
        assert!(!state.can_save());
        assert!(!state.can_publish());
        assert!(state.select_page("page-home").is_none());

        state.update_page_input("page-thanks", "slug", "thanks-next".to_owned());
        assert!(state.page_input_error.is_none());
        assert!(!state.can_save());
        let capture = state
            .apply_local_edit(
                Edit::SetPage {
                    page: PageId::parse("page-thanks").expect("page id"),
                    title: "Thanks".to_owned(),
                    slug: "thanks-next".to_owned(),
                },
                "operation-page-settings".to_owned(),
            )
            .expect("valid page settings can be applied");
        assert_eq!(capture.page_id, "page-thanks");
        assert_eq!(
            state
                .document
                .as_ref()
                .and_then(|document| document.page(&PageId::parse("page-thanks").ok()?))
                .map(|page| page.slug()),
            Some("thanks-next")
        );
        assert!(!state.has_uncommitted_editor_inputs());
    }

    #[test]
    fn changing_public_language_preserves_the_selected_page_and_its_pending_settings() {
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.finish_load(
            0,
            "funnel-a",
            Ok(loaded_editor(
                document_with_secondary_page(),
                Some(stage_capacity()),
            )),
        );
        let _ = state.select_page("page-thanks");
        state.update_page_input(
            "page-thanks",
            "title",
            "Confirmation in progress".to_owned(),
        );

        let capture = state
            .apply_local_edit(
                Edit::SetPublicLanguage {
                    language: Some(eez_domain::document_v2::PublicLanguage::French),
                },
                "operation-language-change".to_owned(),
            )
            .expect("language change applies without focusing a page");

        assert_eq!(state.selected_page, "page-thanks");
        assert_eq!(capture.page_id, "page-thanks");
        assert_eq!(state.page_input_page.as_deref(), Some("page-thanks"));
        assert_eq!(state.page_title_input, "Confirmation in progress");
        assert!(state.has_uncommitted_editor_inputs());
    }

    #[test]
    fn publication_readiness_blocks_findings_but_allows_warnings_and_receipt_retries() {
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.finish_load(
            0,
            "funnel-a",
            Ok(loaded_editor(document(), Some(stage_capacity()))),
        );
        state.publication_readiness = Some(PublicationReadiness {
            revision_id: "revision-a".to_owned(),
            findings: vec![PublicationReadinessFinding {
                code: "editorial/search_description_missing".to_owned(),
                severity: PublicationReadinessSeverity::Warning,
                target: PublicationReadinessTarget::V2PageField {
                    page_id: "page-home".to_owned(),
                    field: "seo_description".to_owned(),
                },
            }],
        });
        assert!(state.can_publish());

        state.publication_readiness = Some(PublicationReadiness {
            revision_id: "revision-a".to_owned(),
            findings: vec![PublicationReadinessFinding {
                code: "publication/generated_placeholder_unresolved".to_owned(),
                severity: PublicationReadinessSeverity::Blocking,
                target: PublicationReadinessTarget::V2Block {
                    page_id: "page-home".to_owned(),
                    section_id: "section-hero".to_owned(),
                    block_id: "block-hero-copy".to_owned(),
                },
            }],
        });
        assert!(!state.can_publish());

        state.publication_retryable = true;
        state.publication_attempt = Some(PublicationAction::Publish {
            revision_id: "revision-a".to_owned(),
            expected_published_revision_id: None,
        });
        assert!(state.can_publish());
    }

    #[test]
    fn unresolved_form_inputs_block_save_publish_and_history_actions() {
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.finish_load(
            0,
            "funnel-a",
            Ok(loaded_editor(document(), Some(stage_capacity()))),
        );
        assert!(state.can_publish());

        state
            .form_input_drafts
            .insert("form:lead:setting:purpose".to_owned(), String::new());
        state.form_input_errors.insert(
            "form:lead:setting:purpose".to_owned(),
            "Enter a purpose.".to_owned(),
        );
        assert!(!state.can_publish());
        assert!(state.has_uncommitted_editor_inputs());

        state.form_input_drafts.clear();
        state.form_input_errors.clear();
        state
            .apply_local_edit(
                Edit::SetThemePreset {
                    preset: ThemePreset::Bold,
                },
                "form-draft-gating-test".to_owned(),
            )
            .expect("saved document accepts a theme edit");
        state.pending = false;
        assert!(state.can_save());
        assert!(state.can_undo());

        state.form_pending_field_types.insert(
            "form:lead:field:field-name:type".to_owned(),
            "checkbox".to_owned(),
        );
        assert!(!state.can_save());
        assert!(!state.can_undo());

        state.form_pending_field_types.clear();
        state.history_cursor = 0;
        assert!(state.can_redo());
        state.form_pending_field_types.insert(
            "form:lead:field:field-name:type".to_owned(),
            "checkbox".to_owned(),
        );
        assert!(!state.can_redo());
    }

    #[test]
    fn deleting_a_section_and_its_anchor_action_is_undone_as_one_change() {
        let document = document_with_section_action();
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.finish_load(
            0,
            "funnel-a",
            Ok(loaded_editor(document, Some(stage_capacity()))),
        );

        let edit = state.apply_local_edit(
            Edit::DeleteSection {
                page: PageId::parse("page-home").expect("page ID"),
                section: SectionId::parse("section-features").expect("section ID"),
            },
            "delete-linked-section".to_owned(),
        );
        assert!(edit.is_some());
        let deleted = state.document.as_ref().expect("edited document");
        assert!(!deleted.pages()[0]
            .sections()
            .iter()
            .any(|section| section.id().as_str() == "section-features"));
        assert!(!deleted.pages()[0]
            .sections()
            .iter()
            .flat_map(|section| section.blocks())
            .any(|block| matches!(block, BlockV2::Button { action: ActionV2::NavigateSection { section_id }, .. } if section_id.as_str() == "section-features")));

        state.pending = false;
        state
            .apply_history(-1)
            .expect("undo restores the previous document snapshot");
        let restored = state.document.as_ref().expect("restored document");
        assert!(restored.pages()[0]
            .sections()
            .iter()
            .any(|section| section.id().as_str() == "section-features"));
        assert!(restored.pages()[0]
            .sections()
            .iter()
            .flat_map(|section| section.blocks())
            .any(|block| matches!(block, BlockV2::Button { action: ActionV2::NavigateSection { section_id }, .. } if section_id.as_str() == "section-features")));
    }

    #[test]
    fn faq_block_edits_and_addition_are_restored_through_editor_history() {
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.finish_load(
            0,
            "funnel-a",
            Ok(loaded_editor(document(), Some(stage_capacity()))),
        );

        state
            .apply_local_edit(
                Edit::AddBlock {
                    section: SectionId::parse("section-hero").expect("section ID"),
                    after: None,
                    block: V2NewBlockKind::Faq.create(PublicLanguage::English),
                },
                "add-faq-block".to_owned(),
            )
            .expect("FAQ block can be added");
        state.pending = false;
        let faq = state.document.as_ref().expect("edited document").pages()[0].sections()[0]
            .blocks()
            .iter()
            .find(|block| matches!(block, BlockV2::Faq { .. }))
            .expect("new FAQ block exists")
            .clone();
        let faq_id = faq.id().clone();

        state
            .apply_local_edit(
                Edit::SetBlock {
                    block: faq_id.clone(),
                    value: NewBlock::Faq {
                        items: vec![
                            FaqItem {
                                question: "Can I edit the content?".to_owned(),
                                answer: "Yes, every entry is editable.".to_owned(),
                            },
                            FaqItem {
                                question: "Can I add another question?".to_owned(),
                                answer: "Add another entry in the editor.".to_owned(),
                            },
                        ],
                    },
                },
                "edit-faq-block".to_owned(),
            )
            .expect("FAQ content can be edited");
        let edited = state.document.as_ref().expect("updated document").pages()[0].sections()[0]
            .blocks()
            .iter()
            .find(|block| block.id() == &faq_id)
            .expect("edited FAQ block exists");
        assert!(matches!(
            edited,
            BlockV2::Faq { items, .. }
                if items.len() == 2 && items[0].answer == "Yes, every entry is editable."
        ));

        state.pending = false;
        state
            .apply_history(-1)
            .expect("undo restores the original FAQ entry");
        let restored = state.document.as_ref().expect("restored document").pages()[0].sections()[0]
            .blocks()
            .iter()
            .find(|block| block.id() == &faq_id)
            .expect("FAQ block remains after undo");
        assert!(matches!(
            restored,
            BlockV2::Faq { items, .. }
                if items.len() == 1 && items[0].question == "Common question"
        ));

        state
            .apply_history(-1)
            .expect("second undo removes the added block");
        assert!(!state
            .document
            .as_ref()
            .expect("restored original document")
            .pages()[0]
            .sections()[0]
            .blocks()
            .iter()
            .any(|block| matches!(block, BlockV2::Faq { .. })));
    }

    #[test]
    fn faq_limit_errors_explain_utf8_field_and_block_budgets() {
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.finish_load(
            0,
            "funnel-a",
            Ok(loaded_editor(document(), Some(stage_capacity()))),
        );
        state
            .apply_local_edit(
                Edit::AddBlock {
                    section: SectionId::parse("section-hero").expect("section ID"),
                    after: None,
                    block: V2NewBlockKind::Faq.create(PublicLanguage::English),
                },
                "add-faq-for-limit-test".to_owned(),
            )
            .expect("FAQ block can be added");
        state.pending = false;
        let faq_id = state.document.as_ref().expect("edited document").pages()[0].sections()[0]
            .blocks()
            .iter()
            .find(|block| matches!(block, BlockV2::Faq { .. }))
            .expect("FAQ block exists")
            .id()
            .clone();

        let aggregate_error = state.apply_local_edit(
            Edit::SetBlock {
                block: faq_id.clone(),
                value: NewBlock::Faq {
                    items: (0..5)
                        .map(|_| FaqItem {
                            question: "q".repeat(2048),
                            answer: "a".repeat(2048),
                        })
                        .collect(),
                },
            },
            "faq-aggregate-limit-test".to_owned(),
        );
        assert!(aggregate_error.is_none());
        assert_eq!(
            state.error.as_deref(),
            Some("This FAQ or steps block exceeds 16 KiB of UTF-8 text. Shorten an entry or remove one before continuing.")
        );

        state.error = None;
        let field_error = state.apply_local_edit(
            Edit::SetBlock {
                block: faq_id,
                value: NewBlock::Faq {
                    items: vec![FaqItem {
                        question: "é".repeat(1025),
                        answer: "A clear answer.".to_owned(),
                    }],
                },
            },
            "faq-field-limit-test".to_owned(),
        );
        assert!(field_error.is_none());
        assert_eq!(
            state.error.as_deref(),
            Some("Each FAQ or step text field has a 2,048-byte UTF-8 limit.")
        );
    }

    #[test]
    fn editing_a_form_preserves_the_current_page_in_the_preview_capture() {
        let mut loaded = loaded_editor(document_with_secondary_page(), Some(stage_capacity()));
        loaded.selected_page = "page-thanks".to_owned();
        loaded.selected_section = Some("section-thanks".to_owned());
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.finish_load(0, "funnel-a", Ok(loaded));

        let created = state
            .apply_local_edit(
                Edit::CreateForm {
                    section: SectionId::parse("section-thanks").expect("section id"),
                    after: None,
                },
                "create-form-on-secondary-page".to_owned(),
            )
            .expect("form can be created on the selected page");
        assert_eq!(created.page_id, "page-thanks");
        state.pending = false;
        let form = state
            .document
            .as_ref()
            .expect("document remains loaded")
            .forms()[0]
            .id()
            .clone();

        let edited = state
            .apply_local_edit(
                Edit::SetFormPurpose {
                    form,
                    purpose: "Collect a qualified request.".to_owned(),
                },
                "edit-form-on-secondary-page".to_owned(),
            )
            .expect("form can be edited");

        assert_eq!(state.selected_page, "page-thanks");
        assert_eq!(edited.page_id, "page-thanks");
    }

    #[test]
    fn changing_field_types_clears_inapplicable_form_drafts_and_errors() {
        let mut ids = TestIds(0);
        let created = document()
            .apply(
                Edit::CreateForm {
                    section: SectionId::parse("section-hero").expect("section id"),
                    after: None,
                },
                &mut ids,
            )
            .expect("form can be created")
            .document;
        let form = created.forms()[0].id().clone();
        let text_field = created.forms()[0].fields()[0].id().clone();
        let email_field = created.forms()[0].fields()[1].id().clone();
        let created = created
            .apply(
                Edit::SetFormField {
                    form: form.clone(),
                    field: text_field.clone(),
                    value: eez_domain::document_v2::NewFormField::Select {
                        name: "name".to_owned(),
                        label: "Your name".to_owned(),
                        help: None,
                        required: false,
                        options: vec!["Option one".to_owned(), "Option two".to_owned()],
                    },
                },
                &mut ids,
            )
            .expect("select field can be set up")
            .document;
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.finish_load(
            0,
            "funnel-a",
            Ok(loaded_editor(created, Some(stage_capacity()))),
        );

        let options_key = format!(
            "form:{}:field:{}:options",
            form.as_str(),
            text_field.as_str()
        );
        let min_key = format!("form:{}:field:{}:min", form.as_str(), email_field.as_str());
        let max_key = format!("form:{}:field:{}:max", form.as_str(), email_field.as_str());
        for (key, message) in [
            (options_key.as_str(), "Add at least two options."),
            (
                min_key.as_str(),
                "Enter a whole number for the minimum length.",
            ),
            (
                max_key.as_str(),
                "Enter a whole number for the maximum length.",
            ),
        ] {
            state
                .form_input_drafts
                .insert(key.to_owned(), "invalid".to_owned());
            state
                .form_input_errors
                .insert(key.to_owned(), message.to_owned());
        }
        assert!(state.has_uncommitted_editor_inputs());
        assert!(!state.can_save());
        assert!(!state.can_undo());

        state
            .apply_local_edit(
                Edit::SetFormField {
                    form: form.clone(),
                    field: text_field,
                    value: eez_domain::document_v2::NewFormField::Text {
                        name: "name".to_owned(),
                        label: "Your name".to_owned(),
                        help: None,
                        required: false,
                        validation: None,
                        presentation: eez_domain::document_v2::TextPresentation::SingleLine,
                    },
                },
                "convert-select-to-text".to_owned(),
            )
            .expect("select field can be converted to text");
        assert!(!state.form_input_drafts.contains_key(&options_key));
        assert!(!state.form_input_errors.contains_key(&options_key));
        assert!(state.form_input_drafts.contains_key(&min_key));
        state.pending = false;

        state
            .apply_local_edit(
                Edit::SetFormField {
                    form,
                    field: email_field,
                    value: eez_domain::document_v2::NewFormField::Checkbox {
                        name: "email".to_owned(),
                        label: "Email address".to_owned(),
                        help: None,
                        required: true,
                    },
                },
                "convert-email-to-checkbox".to_owned(),
            )
            .expect("email field can be converted to checkbox");
        assert!(!state.form_input_drafts.contains_key(&min_key));
        assert!(!state.form_input_drafts.contains_key(&max_key));
        assert!(!state.form_input_errors.contains_key(&min_key));
        assert!(!state.form_input_errors.contains_key(&max_key));
        assert!(!state.has_uncommitted_editor_inputs());
        state.pending = false;

        assert!(state.can_save());
        assert!(state.can_undo());
        state.saved_document_json = state
            .document
            .as_ref()
            .expect("document remains loaded")
            .canonical_json();
        assert!(state.can_publish());
    }

    #[test]
    fn changing_form_settings_clears_hidden_result_and_marketing_errors() {
        let mut ids = TestIds(0);
        let created = document_with_secondary_page()
            .apply(
                Edit::CreateForm {
                    section: SectionId::parse("section-hero").expect("section id"),
                    after: None,
                },
                &mut ids,
            )
            .expect("form can be created")
            .document;
        let form = created.forms()[0].id().clone();
        let success_page = created.pages()[1].id().clone();
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.finish_load(
            0,
            "funnel-a",
            Ok(loaded_editor(created, Some(stage_capacity()))),
        );
        let result_key = format!("form:{}:setting:result-message", form.as_str());
        let marketing_key = format!("form:{}:setting:marketing-copy", form.as_str());

        state
            .form_input_drafts
            .insert(result_key.clone(), "Thank you".to_owned());
        state
            .form_input_errors
            .insert(result_key.clone(), "Enter a message.".to_owned());
        state
            .apply_local_edit(
                Edit::SetFormResult {
                    form: form.clone(),
                    result: FormResult::SuccessPage {
                        page_id: success_page,
                    },
                },
                "hide-inline-result-message".to_owned(),
            )
            .unwrap_or_else(|| panic!("form result can be changed: {:?}", state.error));
        assert!(!state.form_input_drafts.contains_key(&result_key));
        assert!(!state.form_input_errors.contains_key(&result_key));
        state.pending = false;

        state
            .apply_local_edit(
                Edit::SetFormMarketingOptIn {
                    form: form.clone(),
                    copy: Some("Send me updates.".to_owned()),
                },
                "enable-marketing-opt-in".to_owned(),
            )
            .expect("marketing opt-in can be enabled");
        state.pending = false;
        state
            .form_input_drafts
            .insert(marketing_key.clone(), "invalid".to_owned());
        state
            .form_input_errors
            .insert(marketing_key.clone(), "Enter marketing wording.".to_owned());

        state
            .apply_local_edit(
                Edit::SetFormMarketingOptIn { form, copy: None },
                "disable-marketing-opt-in".to_owned(),
            )
            .expect("marketing opt-in can be disabled");

        assert!(!state.form_input_drafts.contains_key(&marketing_key));
        assert!(!state.form_input_errors.contains_key(&marketing_key));
        assert!(!state.has_uncommitted_editor_inputs());
    }

    #[test]
    fn a_late_revision_comparison_cannot_replace_the_next_route_state() {
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.loading = false;
        state.document = Some(document());
        let capture = state
            .begin_compare("revision-old".to_owned())
            .expect("comparison starts");

        state.begin_route("funnel-b".to_owned());
        state.finish_compare(&capture, Ok(document()));

        assert_eq!(state.funnel_id, "funnel-b");
        assert_eq!(state.compare_document, None);
        assert!(!state.compare_loading);
    }

    #[test]
    fn discarding_a_recovered_draft_restores_saved_selection_and_preview_request() {
        let document = document();
        let page_id = document.entry_page_id().as_str().to_owned();
        let section_id = document.pages()[0].sections()[0].id().as_str().to_owned();
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.loading = false;
        state.document = Some(document.clone());
        state.saved_document_json = document.canonical_json();
        state.base_revision_id = "revision-a".to_owned();
        state.recovered_current_draft = true;
        state.selected_page = "missing-page".to_owned();
        state.selected_section = Some("missing-section".to_owned());
        state.preview_html = "stale preview".to_owned();

        let request = state
            .discard_recovered_current_draft()
            .expect("recovered draft is discarded");

        assert_eq!(state.selected_page, page_id);
        assert_eq!(state.selected_section.as_deref(), Some(section_id.as_str()));
        assert!(state.preview_html.is_empty());
        assert_eq!(request.1, "funnel-a");
        assert_eq!(request.2, "revision-a");
        assert_eq!(request.3, state.saved_document_json);
        assert!(!state.recovered_current_draft);
    }

    #[test]
    fn unpublish_remains_retryable_after_an_uncertain_response() {
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.loading = false;
        state.published_revision_id = Some("revision-live".to_owned());
        state.publication_retryable = true;
        state.publication_attempt = Some(PublicationAction::Unpublish {
            expected_published_revision_id: "revision-live".to_owned(),
        });

        assert!(state.can_unpublish());
        assert!(!state.can_publish());
    }

    #[test]
    fn uncertain_publish_retry_remains_enabled_after_editing_the_draft() {
        let document = document();
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.finish_load(
            0,
            "funnel-a",
            Ok(loaded_editor(document, Some(stage_capacity()))),
        );
        let capture = state
            .begin_publication(true)
            .expect("saved revision can be published");
        assert!(!state.finish_publication(&capture, Err(())));
        assert!(state.publication_retryable);
        assert!(state.can_publish());

        let saved = state.document.as_ref().expect("loaded document");
        let mut ids = TestIds(0);
        let changed = saved
            .apply(
                Edit::SetThemePreset {
                    preset: ThemePreset::Bold,
                },
                &mut ids,
            )
            .expect("edit the draft while publication result is uncertain");
        state.document = Some(changed.document);
        assert!(state.has_unsaved_changes());
        assert!(state.can_publish());

        let retry = state
            .begin_publication(true)
            .expect("retry preserves the uncertain publication action");
        assert_eq!(retry.action, capture.action);
    }

    #[test]
    fn a_final_save_retry_resumes_the_existing_finalization_phase() {
        let document = document();
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.loading = false;
        state.stage_capacity = Some(stage_capacity());
        state.base_revision_id = "revision-a".to_owned();
        state.saved_document_json = "older snapshot".to_owned();
        state.document = Some(document);
        state.save_phase = V2SavePhase::Finalizing;
        state.save_retryable = true;

        assert!(!state.can_edit());
        let unresolved_document = state
            .document
            .as_ref()
            .expect("typed document exists")
            .canonical_json();
        assert!(state
            .apply_local_edit(
                Edit::SetThemePreset {
                    preset: ThemePreset::Bold,
                },
                "edit-during-uncertain-final-save".to_owned(),
            )
            .is_none());
        assert_eq!(
            state
                .document
                .as_ref()
                .expect("typed document remains")
                .canonical_json(),
            unresolved_document
        );

        let capture = state.begin_save().expect("retryable save can be resumed");

        assert!(capture.resume_finalization);
        assert_eq!(state.save_phase, V2SavePhase::Finalizing);
    }

    #[test]
    fn stale_publication_conflict_refreshes_the_live_revision_pointer() {
        let document = document();
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.loading = false;
        state.base_revision_id = "revision-current".to_owned();
        state.saved_document_json = document.canonical_json();
        state.document = Some(document);
        state.published_revision_id = Some("revision-old".to_owned());
        let capture = state
            .begin_publication(true)
            .expect("current revision can be published");

        assert!(state.begin_publication_refresh(&capture));
        assert_eq!(
            state.publication_display_state(),
            PublicationDisplayState::Checking
        );
        assert!(!state.show_live_funnel_link());
        state.finish_publication_refresh(&capture, Ok(Some("revision-live-elsewhere".to_owned())));

        assert_eq!(
            state.published_revision_id.as_deref(),
            Some("revision-live-elsewhere")
        );
        assert!(!state.publication_refreshing);
        assert!(!state.publication_state_unknown);
        assert_eq!(
            state.publication_display_state(),
            PublicationDisplayState::Published("revision-live-elsewhere".to_owned())
        );
        assert!(state.show_live_funnel_link());
        assert!(!state.publication_retryable);
    }

    #[test]
    fn failed_publication_refresh_keeps_actions_disabled_until_reload() {
        let document = document();
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.loading = false;
        state.base_revision_id = "revision-current".to_owned();
        state.saved_document_json = document.canonical_json();
        state.document = Some(document);
        state.published_revision_id = Some("revision-old".to_owned());
        let capture = state
            .begin_publication(true)
            .expect("current revision can be published");

        assert!(state.begin_publication_refresh(&capture));
        state.finish_publication_refresh(&capture, Err(()));

        assert!(!state.publication_refreshing);
        assert!(state.publication_state_unknown);
        assert_eq!(
            state.publication_display_state(),
            PublicationDisplayState::Unknown
        );
        assert!(!state.show_live_funnel_link());
        assert!(!state.can_publish());
        assert!(!state.can_unpublish());
        assert!(state.begin_publication(false).is_none());
    }

    #[test]
    fn a_publication_response_from_an_old_route_cannot_change_the_current_route() {
        let document = document();
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.loading = false;
        state.base_revision_id = "revision-a".to_owned();
        state.saved_document_json = document.canonical_json();
        state.document = Some(document);
        let capture = state
            .begin_publication(true)
            .expect("current revision can be published");

        state.begin_route("funnel-b".to_owned());
        let accepted = state.finish_publication(&capture, Ok(Some("revision-a".to_owned())));

        assert!(!accepted);
        assert_eq!(state.funnel_id, "funnel-b");
        assert_eq!(state.published_revision_id, None);
        assert!(!state.publication_pending);
    }

    #[test]
    fn new_button_defaults_to_an_editable_reserved_external_url() {
        let NewBlock::Button { action, .. } =
            V2NewBlockKind::Button.create(PublicLanguage::English)
        else {
            panic!("button kind creates a button block");
        };

        assert_eq!(
            action,
            ActionV2::ExternalHttps {
                url: "https://example.invalid".to_owned()
            }
        );
    }

    #[test]
    fn new_block_copy_follows_the_public_language() {
        let NewBlock::Heading { text, .. } = V2NewBlockKind::Heading.create(PublicLanguage::French)
        else {
            panic!("heading kind creates a heading block");
        };
        assert_eq!(text, "Nouveau titre");

        let NewBlock::Text { text } = V2NewBlockKind::Text.create(PublicLanguage::French) else {
            panic!("text kind creates a text block");
        };
        assert_eq!(text, "Ajoutez votre texte ici.");

        let NewBlock::List { items } = V2NewBlockKind::List.create(PublicLanguage::French) else {
            panic!("list kind creates a list block");
        };
        assert_eq!(items, ["Premier élément", "Deuxième élément"]);

        let NewBlock::Button { label, .. } = V2NewBlockKind::Button.create(PublicLanguage::French)
        else {
            panic!("button kind creates a button block");
        };
        assert_eq!(label, "Continuer");

        let NewBlock::Faq { items } = V2NewBlockKind::Faq.create(PublicLanguage::French) else {
            panic!("FAQ kind creates a structured FAQ block");
        };
        assert_eq!(items[0].question, "Question fréquente");
        assert_eq!(items[0].answer, "Rédigez une réponse pour vos visiteurs.");

        let NewBlock::Steps { items } = V2NewBlockKind::Steps.create(PublicLanguage::French) else {
            panic!("steps kind creates a structured steps block");
        };
        assert_eq!(items[0].title, "Titre de l’étape");
        assert_eq!(items[0].detail, "Décrivez ce qui se passe à cette étape.");
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
    fn unsaved_placeholder_retyping_keeps_the_persisted_blocker_and_saves_validly() {
        let original = document();
        let expected_text = original.generated_placeholders()[0].expected_text.clone();
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.finish_load(
            0,
            "funnel-a",
            Ok(loaded_editor(original.clone(), Some(stage_capacity()))),
        );

        state
            .apply_local_edit(
                Edit::SetBlock {
                    block: eez_domain::document_v2::BlockId::parse("block-hero-copy")
                        .expect("placeholder block"),
                    value: NewBlock::Text {
                        text: "A defined service offer".to_owned(),
                    },
                },
                "resolve-placeholder".to_owned(),
            )
            .expect("placeholder can be resolved locally");
        state.pending = false;

        let capture = state
            .apply_local_edit(
                Edit::SetBlock {
                    block: eez_domain::document_v2::BlockId::parse("block-hero-copy")
                        .expect("placeholder block"),
                    value: NewBlock::Text {
                        text: expected_text,
                    },
                },
                "restore-placeholder".to_owned(),
            )
            .expect("editing back to the starter text is locally accepted");
        let transient = DocumentV2::decode(capture.base_document_json.as_bytes(), "funnel-a")
            .expect("typed edit starts from the current local draft");
        let persisted_base = DocumentV2::decode(state.saved_document_json.as_bytes(), "funnel-a")
            .expect("saved base remains available");
        let mut ids = DeterministicIds {
            operation_id: capture.operation_id.clone(),
            next: 0,
        };
        let worker_document = transient
            .apply(capture.edit.clone(), &mut ids)
            .expect("worker applies the typed operation")
            .document
            .reconcile_generated_placeholders_from(&persisted_base)
            .and_then(|candidate| candidate.reconcile_generated_placeholders_from(&transient))
            .expect("worker retains the saved marker provenance");

        assert_eq!(worker_document.canonical_json(), capture.document_json);
        state.pending = false;
        state
            .apply_local_edit(
                Edit::SetThemePreset {
                    preset: ThemePreset::Bold,
                },
                "change-theme-with-unresolved-placeholder".to_owned(),
            )
            .expect("other draft changes remain editable");
        assert_eq!(
            state
                .document
                .as_ref()
                .expect("draft remains loaded")
                .generated_placeholders()
                .len(),
            1
        );
        state.pending = false;
        let save = state.begin_save().expect("the draft can be saved");
        let saved_candidate = DocumentV2::decode(save.document_json.as_bytes(), "funnel-a")
            .expect("save capture is a valid document");
        assert!(original.has_valid_generated_placeholder_transition(&saved_candidate));
    }

    #[test]
    fn restoring_a_historical_placeholder_keeps_its_blocker_in_the_save_capture() {
        let original = document();
        let mut ids = TestIds(0);
        let resolved = original
            .apply(
                Edit::SetBlock {
                    block: eez_domain::document_v2::BlockId::parse("block-hero-copy")
                        .expect("placeholder block"),
                    value: NewBlock::Text {
                        text: "A defined service offer".to_owned(),
                    },
                },
                &mut ids,
            )
            .expect("resolve generated offer copy")
            .document;
        let mut state = V2EditorState::for_route("funnel-a".to_owned());
        state.finish_load(
            0,
            "funnel-a",
            Ok(loaded_editor(resolved.clone(), Some(stage_capacity()))),
        );

        state
            .restore_document(original.clone())
            .expect("a prior revision can be restored as a draft");
        let capture = state.begin_save().expect("the restored draft can be saved");
        let restored = DocumentV2::decode(capture.document_json.as_bytes(), "funnel-a")
            .expect("restored save capture is a valid document");

        assert_eq!(restored.generated_placeholders().len(), 1);
        assert!(resolved.has_valid_generated_placeholder_transition(&restored));
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
}
