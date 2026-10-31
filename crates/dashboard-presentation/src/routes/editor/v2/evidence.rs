use super::orchestration::dispatch_v2_edit;
use super::state::V2EditorState;
use super::ui::media_size_label;
use super::*;
use eez_domain::document_v2::BlockId;

#[derive(Clone)]
struct EvidenceDraft {
    block_id: BlockId,
    evidence_kind: EvidenceKind,
    title: String,
    content: String,
    status: EvidenceStatus,
    limitation: String,
    image: Option<EvidenceImage>,
}

impl EvidenceDraft {
    fn from_block(block: BlockV2) -> Option<Self> {
        let BlockV2::Evidence {
            id,
            evidence_kind,
            title,
            content,
            status,
            limitation,
            image,
        } = block
        else {
            return None;
        };
        Some(Self {
            block_id: id,
            evidence_kind,
            title,
            content,
            status,
            limitation,
            image,
        })
    }

    fn save(self, state: Signal<V2EditorState>) {
        dispatch_v2_edit(
            state,
            Edit::SetBlock {
                block: self.block_id,
                value: NewBlock::Evidence {
                    evidence_kind: self.evidence_kind,
                    title: self.title,
                    content: self.content,
                    status: self.status,
                    limitation: self.limitation,
                    image: self.image,
                },
            },
        );
    }

    fn status_label(&self) -> &'static str {
        match self.status {
            EvidenceStatus::Demonstration => "demonstration",
            EvidenceStatus::Real { .. } => "real",
        }
    }

    fn real_context(&self) -> Option<(&str, &str, &str)> {
        match &self.status {
            EvidenceStatus::Demonstration => None,
            EvidenceStatus::Real {
                attribution,
                date_or_period,
                scope,
            } => Some((attribution, date_or_period, scope)),
        }
    }
}

#[derive(Clone, Copy)]
enum EvidenceTextField {
    Title,
    Content,
    Limitation,
    Attribution,
    DateOrPeriod,
    Scope,
    ImageAlt,
}

impl EvidenceTextField {
    fn key(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Content => "content",
            Self::Limitation => "limitation",
            Self::Attribution => "attribution",
            Self::DateOrPeriod => "date_or_period",
            Self::Scope => "scope",
            Self::ImageAlt => "image_alt",
        }
    }

    fn maximum(self) -> usize {
        match self {
            Self::Title | Self::Attribution | Self::DateOrPeriod => MAX_EVIDENCE_TITLE_BYTES,
            Self::Content => MAX_EVIDENCE_TEXT_BYTES,
            Self::Limitation | Self::Scope => MAX_EVIDENCE_CONTEXT_BYTES,
            Self::ImageAlt => MAX_EVIDENCE_IMAGE_ALT_BYTES,
        }
    }

    fn set(self, draft: &mut EvidenceDraft, value: String) {
        match self {
            Self::Title => draft.title = value,
            Self::Content => draft.content = value,
            Self::Limitation => draft.limitation = value,
            Self::Attribution => {
                if let EvidenceStatus::Real { attribution, .. } = &mut draft.status {
                    *attribution = value;
                }
            }
            Self::DateOrPeriod => {
                if let EvidenceStatus::Real { date_or_period, .. } = &mut draft.status {
                    *date_or_period = value;
                }
            }
            Self::Scope => {
                if let EvidenceStatus::Real { scope, .. } = &mut draft.status {
                    *scope = value;
                }
            }
            Self::ImageAlt => {
                if let Some(image) = &mut draft.image {
                    image.alt = value;
                }
            }
        }
    }
}

fn input_key(block_id: &BlockId, field: EvidenceTextField) -> String {
    format!("evidence:{}:{}", block_id.as_str(), field.key())
}

fn input_value(state: Signal<V2EditorState>, key: &str, fallback: Option<&str>) -> String {
    state.with(|state| {
        state
            .form_input_drafts
            .get(key)
            .cloned()
            .or_else(|| fallback.map(str::to_owned))
            .unwrap_or_default()
    })
}

fn input_error(state: Signal<V2EditorState>, key: &str) -> String {
    state
        .with(|state| state.form_input_errors.get(key).cloned())
        .unwrap_or_default()
}

fn update_text(
    mut state: Signal<V2EditorState>,
    mut draft: EvidenceDraft,
    field: EvidenceTextField,
    value: String,
) {
    let key = input_key(&draft.block_id, field);
    if value.len() > field.maximum() {
        state.with_mut(|state| {
            state.form_input_drafts.insert(key.clone(), value);
            state.form_input_errors.insert(
                key,
                format!("Keep this value within {} UTF-8 bytes.", field.maximum()),
            );
        });
        return;
    }
    state.with_mut(|state| {
        state.form_input_drafts.remove(&key);
        state.form_input_errors.remove(&key);
    });
    field.set(&mut draft, value);
    draft.save(state);
}

fn set_status(draft: &mut EvidenceDraft, value: &str) {
    draft.status = match (value, &draft.status) {
        ("real", EvidenceStatus::Real { .. }) => draft.status.clone(),
        ("real", EvidenceStatus::Demonstration) => EvidenceStatus::Real {
            attribution: String::new(),
            date_or_period: String::new(),
            scope: String::new(),
        },
        (_, _) => EvidenceStatus::Demonstration,
    };
}

#[component]
pub(super) fn V2EvidenceBlockEditor(
    state: Signal<V2EditorState>,
    block: BlockV2,
    locked: bool,
) -> Element {
    let Some(draft) = EvidenceDraft::from_block(block) else {
        return rsx! {};
    };
    let block_id = draft.block_id.clone();
    let title_key = input_key(&block_id, EvidenceTextField::Title);
    let content_key = input_key(&block_id, EvidenceTextField::Content);
    let limitation_key = input_key(&block_id, EvidenceTextField::Limitation);
    let title = input_value(state, &title_key, Some(&draft.title));
    let content = input_value(state, &content_key, Some(&draft.content));
    let limitation = input_value(state, &limitation_key, Some(&draft.limitation));
    let title_error = input_error(state, &title_key);
    let content_error = input_error(state, &content_key);
    let limitation_error = input_error(state, &limitation_key);
    let status_label = draft.status_label();
    let evidence_kind = draft.evidence_kind;
    let assets = state.with(|state| state.media_assets.clone());
    let selected_image_id = draft
        .image
        .as_ref()
        .map(|image| image.asset_id.as_str().to_owned())
        .unwrap_or_default();
    let image_is_available =
        selected_image_id.is_empty() || assets.iter().any(|asset| asset.id == selected_image_id);
    let real_context = draft.real_context();
    let attribution_key = input_key(&block_id, EvidenceTextField::Attribution);
    let period_key = input_key(&block_id, EvidenceTextField::DateOrPeriod);
    let scope_key = input_key(&block_id, EvidenceTextField::Scope);
    let attribution = input_value(state, &attribution_key, real_context.map(|fields| fields.0));
    let date_or_period = input_value(state, &period_key, real_context.map(|fields| fields.1));
    let scope = input_value(state, &scope_key, real_context.map(|fields| fields.2));
    let attribution_error = input_error(state, &attribution_key);
    let period_error = input_error(state, &period_key);
    let scope_error = input_error(state, &scope_key);
    let image_alt = draft.image.as_ref().map(|image| image.alt.as_str());
    let image_alt_key = input_key(&block_id, EvidenceTextField::ImageAlt);
    let image_alt_value = input_value(state, &image_alt_key, image_alt);
    let image_alt_error = input_error(state, &image_alt_key);
    let title_error_id = format!("evidence-title-error-{}", block_id.as_str());
    let content_error_id = format!("evidence-content-error-{}", block_id.as_str());
    let limitation_error_id = format!("evidence-limitation-error-{}", block_id.as_str());
    let attribution_error_id = format!("evidence-attribution-error-{}", block_id.as_str());
    let period_error_id = format!("evidence-period-error-{}", block_id.as_str());
    let scope_error_id = format!("evidence-scope-error-{}", block_id.as_str());
    let image_alt_error_id = format!("evidence-image-alt-error-{}", block_id.as_str());
    let title_size = title.len();
    let content_size = content.len();
    let limitation_size = limitation.len();
    let attribution_size = attribution.len();
    let period_size = date_or_period.len();
    let scope_size = scope.len();
    let image_alt_size = image_alt_value.len();
    let draft_for_kind = draft.clone();
    let draft_for_status = draft.clone();
    let draft_for_title = draft.clone();
    let draft_for_content = draft.clone();
    let draft_for_attribution = draft.clone();
    let draft_for_period = draft.clone();
    let draft_for_scope = draft.clone();
    let draft_for_limitation = draft.clone();
    let draft_for_image = draft.clone();
    let draft_for_image_alt = draft.clone();

    rsx! {
        fieldset { class: "editor-setting-group editor-evidence-settings",
            legend { "Evidence content" }
            p { class: "editor-keyboard-help", "Demonstrations are labeled on the public page. Real evidence needs an attribution, date or period, scope, and a limitation near the claim." }
            label { class: "editor-setting-control",
                "Evidence type"
                select {
                    value: if evidence_kind == EvidenceKind::Testimonial { "testimonial" } else { "case_study" },
                    disabled: locked,
                    "aria-label": "Evidence type for {block_id.as_str()}",
                    onchange: move |event| {
                        let mut next = draft_for_kind.clone();
                        next.evidence_kind = if event.value() == "testimonial" { EvidenceKind::Testimonial } else { EvidenceKind::CaseStudy };
                        next.save(state);
                    },
                    option { value: "testimonial", "Testimonial" }
                    option { value: "case_study", "Case study" }
                }
            }
            label { class: "editor-setting-control",
                "Evidence status"
                select {
                    value: "{status_label}",
                    disabled: locked,
                    "aria-label": "Evidence status for {block_id.as_str()}",
                    onchange: move |event| {
                        let mut next = draft_for_status.clone();
                        set_status(&mut next, &event.value());
                        for field in [EvidenceTextField::Attribution, EvidenceTextField::DateOrPeriod, EvidenceTextField::Scope] {
                            let key = input_key(&next.block_id, field);
                            state.with_mut(|state| {
                                state.form_input_drafts.remove(&key);
                                state.form_input_errors.remove(&key);
                            });
                        }
                        next.save(state);
                    },
                    option { value: "demonstration", "Demonstration" }
                    option { value: "real", "Real, attributed" }
                }
            }
            label { class: "editor-setting-control",
                "Title"
                input {
                    r#type: "text",
                    value: "{title}",
                    maxlength: "512",
                    disabled: locked,
                    "aria-label": "Evidence title for {block_id.as_str()}",
                    aria_describedby: "{title_error_id}",
                    onchange: move |event| update_text(state, draft_for_title.clone(), EvidenceTextField::Title, event.value()),
                }
                span { class: "editor-keyboard-help", "{title_size} / {MAX_EVIDENCE_TITLE_BYTES} UTF-8 bytes" }
                span { id: "{title_error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{title_error}" }
            }
            label { class: "editor-setting-control",
                "Evidence content"
                textarea {
                    rows: "5",
                    value: "{content}",
                    maxlength: "8192",
                    disabled: locked,
                    "aria-label": "Evidence content for {block_id.as_str()}",
                    aria_describedby: "{content_error_id}",
                    onchange: move |event| update_text(state, draft_for_content.clone(), EvidenceTextField::Content, event.value()),
                }
                span { class: "editor-keyboard-help", "{content_size} / {MAX_EVIDENCE_TEXT_BYTES} UTF-8 bytes" }
                span { id: "{content_error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{content_error}" }
            }
            if let Some((_, _, _)) = real_context {
                label { class: "editor-setting-control",
                    "Attribution"
                    input {
                        r#type: "text",
                        value: "{attribution}",
                        maxlength: "512",
                        disabled: locked,
                        "aria-label": "Evidence attribution for {block_id.as_str()}",
                        aria_describedby: "{attribution_error_id}",
                        onchange: move |event| update_text(state, draft_for_attribution.clone(), EvidenceTextField::Attribution, event.value()),
                    }
                    span { class: "editor-keyboard-help", "{attribution_size} / {MAX_EVIDENCE_TITLE_BYTES} UTF-8 bytes" }
                    span { id: "{attribution_error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{attribution_error}" }
                }
                label { class: "editor-setting-control",
                    "Date or period"
                    input {
                        r#type: "text",
                        value: "{date_or_period}",
                        maxlength: "512",
                        disabled: locked,
                        "aria-label": "Evidence date or period for {block_id.as_str()}",
                        aria_describedby: "{period_error_id}",
                        onchange: move |event| update_text(state, draft_for_period.clone(), EvidenceTextField::DateOrPeriod, event.value()),
                    }
                    span { class: "editor-keyboard-help", "{period_size} / {MAX_EVIDENCE_TITLE_BYTES} UTF-8 bytes" }
                    span { id: "{period_error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{period_error}" }
                }
                label { class: "editor-setting-control",
                    "Scope"
                    textarea {
                        rows: "3",
                        value: "{scope}",
                        maxlength: "2048",
                        disabled: locked,
                        "aria-label": "Evidence scope for {block_id.as_str()}",
                        aria_describedby: "{scope_error_id}",
                        onchange: move |event| update_text(state, draft_for_scope.clone(), EvidenceTextField::Scope, event.value()),
                    }
                    span { class: "editor-keyboard-help", "{scope_size} / {MAX_EVIDENCE_CONTEXT_BYTES} UTF-8 bytes" }
                    span { id: "{scope_error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{scope_error}" }
                }
            } else {
                p { class: "editor-keyboard-help", role: "note", "This content will be labeled as a demonstration and will not display client attribution." }
            }
            label { class: "editor-setting-control",
                "Limitations"
                textarea {
                    rows: "3",
                    value: "{limitation}",
                    maxlength: "2048",
                    disabled: locked,
                    "aria-label": "Evidence limitations for {block_id.as_str()}",
                    aria_describedby: "{limitation_error_id}",
                    onchange: move |event| update_text(state, draft_for_limitation.clone(), EvidenceTextField::Limitation, event.value()),
                }
                span { class: "editor-keyboard-help", "Displayed next to this content. {limitation_size} / {MAX_EVIDENCE_CONTEXT_BYTES} UTF-8 bytes" }
                span { id: "{limitation_error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{limitation_error}" }
            }
            label { class: "editor-setting-control",
                "Existing image (optional)"
                select {
                    value: "{selected_image_id}",
                    disabled: locked,
                    "aria-label": "Evidence image for {block_id.as_str()}",
                    onchange: move |event| {
                        let selected = event.value();
                        let mut next = draft_for_image.clone();
                        next.image = if selected.is_empty() {
                            None
                        } else {
                            AssetId::parse(selected).ok().map(|asset_id| EvidenceImage {
                                asset_id,
                                alt: next.image.as_ref().map(|image| image.alt.clone()).unwrap_or_default(),
                            })
                        };
                        let key = input_key(&next.block_id, EvidenceTextField::ImageAlt);
                        state.with_mut(|state| {
                            state.form_input_drafts.remove(&key);
                            state.form_input_errors.remove(&key);
                        });
                        next.save(state);
                    },
                    if !image_is_available && !selected_image_id.is_empty() {
                        option { value: "{selected_image_id}", "Unavailable image" }
                    }
                    option { value: "", "No image" }
                    for asset in assets.iter() {
                        option { value: "{asset.id}", "{asset.mime} · {media_size_label(asset.size_bytes)}" }
                    }
                }
            }
            if draft.image.is_some() {
                label { class: "editor-setting-control",
                    "Image alternative text"
                    input {
                        r#type: "text",
                        value: "{image_alt_value}",
                        maxlength: "512",
                        disabled: locked,
                        "aria-label": "Evidence image alternative text for {block_id.as_str()}",
                        aria_describedby: "{image_alt_error_id}",
                        onchange: move |event| update_text(state, draft_for_image_alt.clone(), EvidenceTextField::ImageAlt, event.value()),
                    }
                    span { class: "editor-keyboard-help", "{image_alt_size} / {MAX_EVIDENCE_IMAGE_ALT_BYTES} UTF-8 bytes" }
                    span { id: "{image_alt_error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{image_alt_error}" }
                }
            }
        }
    }
}
