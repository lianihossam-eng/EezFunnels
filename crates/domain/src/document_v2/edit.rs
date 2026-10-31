use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::model::{
    BlockId, BlockV2, ConsentCopy, DocumentData, DocumentError, DocumentV2, FaqItem, FieldId,
    FormField, FormId, FormPresentation, FormResult, FormV2, OfferPriceV2, PageId, PageV2,
    PublicLanguage, SectionId, SectionKind, SectionV2, StepItem, TextPresentation, ValidationRange,
};
use super::theme::ThemeOverrides;
use crate::ThemePreset;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdKind {
    Page,
    Section,
    Block,
    Form,
    Field,
    ConsentCopy,
}

pub trait IdSource {
    fn next_id(&mut self, kind: IdKind) -> Result<String, DocumentError>;
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NewBlock {
    Heading {
        level: u8,
        text: String,
    },
    Text {
        text: String,
    },
    List {
        items: Vec<String>,
    },
    Button {
        label: String,
        action: super::ActionV2,
    },
    Image {
        asset_id: super::AssetId,
        alt: super::ImageAlt,
    },
    Form {
        form_id: FormId,
    },
    Faq {
        items: Vec<FaqItem>,
    },
    Steps {
        items: Vec<StepItem>,
    },
    OfferCard {
        title: String,
        scope: String,
        price: Option<OfferPriceV2>,
        conditions: String,
    },
    OfferComparison {
        offer_ids: Vec<BlockId>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NewFormField {
    Text {
        name: String,
        label: String,
        help: Option<String>,
        required: bool,
        validation: Option<ValidationRange>,
        #[serde(default, skip_serializing_if = "TextPresentation::is_single_line")]
        presentation: TextPresentation,
    },
    Email {
        name: String,
        label: String,
        help: Option<String>,
        required: bool,
        validation: Option<ValidationRange>,
    },
    Phone {
        name: String,
        label: String,
        help: Option<String>,
        required: bool,
        validation: Option<ValidationRange>,
    },
    Checkbox {
        name: String,
        label: String,
        help: Option<String>,
        required: bool,
    },
    Select {
        name: String,
        label: String,
        help: Option<String>,
        required: bool,
        options: Vec<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    SetPublicLanguage {
        language: Option<PublicLanguage>,
    },
    AddPage {
        from: PageId,
        title: String,
        slug: String,
    },
    SetPage {
        page: PageId,
        title: String,
        slug: String,
    },
    DeletePage {
        page: PageId,
    },
    SetThemePreset {
        preset: ThemePreset,
    },
    SetThemeOverrides {
        overrides: Box<ThemeOverrides>,
    },
    SetPageSeoTitle {
        page: PageId,
        value: String,
    },
    SetPageSeoDescription {
        page: PageId,
        value: String,
    },
    SetPageNoIndex {
        page: PageId,
        noindex: bool,
    },
    AddSection {
        page: PageId,
        after: Option<SectionId>,
        section_type: SectionKind,
    },
    DuplicateSection {
        page: PageId,
        section: SectionId,
    },
    DeleteSection {
        page: PageId,
        section: SectionId,
    },
    ReorderSections {
        page: PageId,
        ordered: Vec<SectionId>,
    },
    AddBlock {
        section: SectionId,
        after: Option<super::BlockId>,
        block: NewBlock,
    },
    DuplicateBlock {
        section: SectionId,
        block: super::BlockId,
    },
    DeleteBlock {
        section: SectionId,
        block: super::BlockId,
    },
    ReorderBlocks {
        section: SectionId,
        ordered: Vec<super::BlockId>,
    },
    SetBlock {
        block: super::BlockId,
        value: NewBlock,
    },
    CreateForm {
        section: SectionId,
        after: Option<super::BlockId>,
    },
    SetFormPurpose {
        form: FormId,
        purpose: String,
    },
    SetFormPresentation {
        form: FormId,
        presentation: Option<FormPresentation>,
    },
    SetFormResult {
        form: FormId,
        result: FormResult,
    },
    SetFormRequestConsent {
        form: FormId,
        copy: String,
    },
    SetFormMarketingOptIn {
        form: FormId,
        copy: Option<String>,
    },
    AddFormField {
        form: FormId,
        after: Option<FieldId>,
        field: NewFormField,
    },
    SetFormField {
        form: FormId,
        field: FieldId,
        value: NewFormField,
    },
    DeleteFormField {
        form: FormId,
        field: FieldId,
    },
    ReorderFormFields {
        form: FormId,
        ordered: Vec<FieldId>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum FocusTarget {
    None,
    Page(PageId),
    Section(SectionId),
    Block(super::BlockId),
}

impl FocusTarget {
    pub fn section_id(&self) -> Option<&SectionId> {
        match self {
            Self::Section(section) => Some(section),
            Self::Block(_) | Self::None | Self::Page(_) => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditOutcome {
    pub document: DocumentV2,
    pub focus: FocusTarget,
}

impl DocumentV2 {
    pub fn apply(&self, edit: Edit, ids: &mut impl IdSource) -> Result<EditOutcome, DocumentError> {
        let mut data = self.data.clone();
        let focus = match edit {
            Edit::SetPublicLanguage { language } => {
                data.language = language;
                FocusTarget::None
            }
            Edit::AddPage { from, title, slug } => {
                if data.pages.len() >= 50 {
                    return Err(DocumentError::at("validation/page_count", "pages"));
                }
                let source_index = data
                    .pages
                    .iter()
                    .position(|page| page.id == from)
                    .ok_or_else(|| DocumentError::at("validation/page_missing", "page_id"))?;
                let id = PageId::from_validated(allocate_id(ids, IdKind::Page, page_ids(&data))?);
                data.pages.push(PageV2 {
                    id: id.clone(),
                    slug,
                    title,
                    seo_title: None,
                    seo_description: None,
                    noindex: false,
                    sections: Vec::new(),
                });
                let block_id =
                    BlockId::from_validated(allocate_id(ids, IdKind::Block, block_ids(&data))?);
                let cta_section_index =
                    data.pages[source_index]
                        .sections
                        .iter()
                        .rposition(|section| {
                            section.section_type == SectionKind::Cta && section.blocks.len() < 50
                        });
                let continue_label = localized_default(data.language, "Continue", "Continuer");
                if let Some(section_index) = cta_section_index {
                    data.pages[source_index].sections[section_index]
                        .blocks
                        .push(BlockV2::Button {
                            id: block_id,
                            label: continue_label,
                            action: super::ActionV2::NavigatePage {
                                page_id: id.clone(),
                            },
                        });
                } else {
                    let section_id = SectionId::from_validated(allocate_id(
                        ids,
                        IdKind::Section,
                        section_ids(&data),
                    )?);
                    data.pages[source_index].sections.push(SectionV2 {
                        id: section_id,
                        section_type: SectionKind::Cta,
                        blocks: vec![BlockV2::Button {
                            id: block_id,
                            label: continue_label,
                            action: super::ActionV2::NavigatePage {
                                page_id: id.clone(),
                            },
                        }],
                    });
                }
                FocusTarget::Page(id)
            }
            Edit::SetPage { page, title, slug } => {
                let target = page_mut(&mut data, &page)?;
                target.title = title;
                target.slug = slug;
                FocusTarget::Page(page)
            }
            Edit::DeletePage { page } => {
                if page == data.entry_page_id {
                    return Err(DocumentError::at("validation/entry_page_delete", "page_id"));
                }
                let page_exists = data.pages.iter().any(|candidate| candidate.id == page);
                if !page_exists {
                    return Err(DocumentError::at("validation/page_missing", "page_id"));
                }
                for source_page in &mut data.pages {
                    for section in &mut source_page.sections {
                        section.blocks.retain(|block| {
                            !matches!(
                                block,
                                BlockV2::Button {
                                    action: super::ActionV2::NavigatePage { page_id },
                                    ..
                                } if page_id == &page
                            )
                        });
                    }
                }
                let inline_confirmation = localized_default(data.language, "Thank you.", "Merci.");
                for form in &mut data.forms {
                    if matches!(
                        &form.result,
                        FormResult::SuccessPage { page_id } if page_id == &page
                    ) {
                        form.result = FormResult::Inline {
                            message: inline_confirmation.clone(),
                        };
                    }
                }
                let index = data
                    .pages
                    .iter()
                    .position(|candidate| candidate.id == page)
                    .ok_or_else(|| DocumentError::at("validation/page_missing", "page_id"))?;
                data.pages.remove(index);
                prune_unplaced_forms(&mut data);
                FocusTarget::Page(data.entry_page_id.clone())
            }
            Edit::SetThemePreset { preset } => {
                data.theme.preset_id = preset;
                FocusTarget::Page(data.entry_page_id.clone())
            }
            Edit::SetThemeOverrides { overrides } => {
                data.theme.overrides = *overrides;
                FocusTarget::Page(data.entry_page_id.clone())
            }
            Edit::SetPageSeoTitle { page, value } => {
                page_mut(&mut data, &page)?.seo_title = Some(value);
                FocusTarget::Page(page)
            }
            Edit::SetPageSeoDescription { page, value } => {
                page_mut(&mut data, &page)?.seo_description = Some(value);
                FocusTarget::Page(page)
            }
            Edit::SetPageNoIndex { page, noindex } => {
                page_mut(&mut data, &page)?.noindex = noindex;
                FocusTarget::Page(page)
            }
            Edit::AddSection {
                page,
                after,
                section_type,
            } => {
                let id = allocate_id(ids, IdKind::Section, section_ids(&data))?;
                let sections = &mut page_mut(&mut data, &page)?.sections;
                let insertion = insertion_index(sections, after.as_ref(), |section| &section.id)?;
                sections.insert(
                    insertion,
                    SectionV2 {
                        id: SectionId::from_validated(id),
                        section_type,
                        blocks: Vec::new(),
                    },
                );
                FocusTarget::Section(sections[insertion].id.clone())
            }
            Edit::DuplicateSection { page, section } => {
                let source = page_ref(&data, &page)?
                    .sections
                    .iter()
                    .find(|candidate| candidate.id == section)
                    .cloned()
                    .ok_or_else(|| DocumentError::at("validation/section_missing", "section_id"))?;
                let duplicate = duplicate_section(&mut data, source, ids)?;
                let sections = &mut page_mut(&mut data, &page)?.sections;
                let source_index = sections
                    .iter()
                    .position(|candidate| candidate.id == section)
                    .ok_or_else(|| DocumentError::at("validation/section_missing", "section_id"))?;
                let insertion = source_index + 1;
                sections.insert(insertion, duplicate);
                FocusTarget::Section(sections[insertion].id.clone())
            }
            Edit::DeleteSection { page, section } => {
                let focus = {
                    let sections = &mut page_mut(&mut data, &page)?.sections;
                    let index = sections
                        .iter()
                        .position(|candidate| candidate.id == section)
                        .ok_or_else(|| {
                            DocumentError::at("validation/section_missing", "section_id")
                        })?;
                    sections.remove(index);
                    for source_section in sections.iter_mut() {
                        source_section.blocks.retain(|block| {
                            !matches!(
                                block,
                                BlockV2::Button {
                                    action: super::ActionV2::NavigateSection { section_id },
                                    ..
                                } if section_id == &section
                            )
                        });
                    }
                    select_after_delete(sections, index, page)
                };
                prune_unplaced_forms(&mut data);
                focus
            }
            Edit::ReorderSections { page, ordered } => {
                let sections = &mut page_mut(&mut data, &page)?.sections;
                if !is_exact_permutation(sections.iter().map(|item| &item.id), &ordered) {
                    return Err(DocumentError::at(
                        "validation/section_order_invalid",
                        "section_ids",
                    ));
                }
                let mut prior = std::mem::take(sections);
                let mut reordered = Vec::with_capacity(ordered.len());
                for id in &ordered {
                    let index = prior
                        .iter()
                        .position(|item| &item.id == id)
                        .ok_or_else(|| {
                            DocumentError::at("validation/section_order_invalid", "section_ids")
                        })?;
                    reordered.push(prior.remove(index));
                }
                *sections = reordered;
                ordered
                    .first()
                    .cloned()
                    .map(FocusTarget::Section)
                    .unwrap_or(FocusTarget::Page(page))
            }
            Edit::AddBlock {
                section,
                after,
                block,
            } => {
                let id = allocate_id(ids, IdKind::Block, block_ids(&data))?;
                let blocks = &mut section_mut(&mut data, &section)?.blocks;
                let insertion = insertion_index(blocks, after.as_ref(), |block| block.id())?;
                blocks.insert(insertion, block.with_id(super::BlockId::from_validated(id)));
                FocusTarget::Block(blocks[insertion].id().clone())
            }
            Edit::DuplicateBlock { section, block } => {
                let source = section_ref(&data, &section)?
                    .blocks
                    .iter()
                    .find(|candidate| candidate.id() == &block)
                    .cloned()
                    .ok_or_else(|| DocumentError::at("validation/block_missing", "block_id"))?;
                let duplicate = duplicate_block(&mut data, source, ids)?;
                let blocks = &mut section_mut(&mut data, &section)?.blocks;
                let source_index = blocks
                    .iter()
                    .position(|candidate| candidate.id() == &block)
                    .ok_or_else(|| DocumentError::at("validation/block_missing", "block_id"))?;
                let insertion = source_index + 1;
                blocks.insert(insertion, duplicate);
                FocusTarget::Block(blocks[insertion].id().clone())
            }
            Edit::DeleteBlock { section, block } => {
                let references_offer = offer_is_referenced(&data, &block);
                let blocks = &mut section_mut(&mut data, &section)?.blocks;
                let index = blocks
                    .iter()
                    .position(|candidate| candidate.id() == &block)
                    .ok_or_else(|| DocumentError::at("validation/block_missing", "block_id"))?;
                if matches!(blocks[index], BlockV2::OfferCard { .. }) && references_offer {
                    return Err(DocumentError::at(
                        "validation/offer_in_use",
                        "blocks.offer_ids",
                    ));
                }
                blocks.remove(index);
                prune_unplaced_forms(&mut data);
                select_after_block_delete(&data, &section, index)?
            }
            Edit::ReorderBlocks { section, ordered } => {
                let blocks = &mut section_mut(&mut data, &section)?.blocks;
                if !is_exact_permutation(blocks.iter().map(BlockV2::id), &ordered) {
                    return Err(DocumentError::at(
                        "validation/block_order_invalid",
                        "block_ids",
                    ));
                }
                let mut prior = std::mem::take(blocks);
                let mut reordered = Vec::with_capacity(ordered.len());
                for id in &ordered {
                    let index = prior
                        .iter()
                        .position(|item| item.id() == id)
                        .ok_or_else(|| {
                            DocumentError::at("validation/block_order_invalid", "block_ids")
                        })?;
                    reordered.push(prior.remove(index));
                }
                *blocks = reordered;
                ordered
                    .first()
                    .cloned()
                    .map(FocusTarget::Block)
                    .unwrap_or(FocusTarget::Section(section))
            }
            Edit::SetBlock { block, value } => {
                let location = self
                    .index
                    .blocks
                    .get(&block)
                    .copied()
                    .ok_or_else(|| DocumentError::at("validation/block_missing", "block_id"))?;
                let data_block = data
                    .pages
                    .get_mut(location.0)
                    .and_then(|page| page.sections.get_mut(location.1))
                    .and_then(|section| section.blocks.get_mut(location.2))
                    .ok_or_else(|| DocumentError::new("internal/document_index_invalid"))?;
                *data_block = value.with_id(block.clone());
                prune_unplaced_forms(&mut data);
                FocusTarget::Block(block)
            }
            Edit::CreateForm { section, after } => {
                let block_id =
                    BlockId::from_validated(allocate_id(ids, IdKind::Block, block_ids(&data))?);
                let form = new_form(&data, ids)?;
                let form_id = form.id.clone();
                data.forms.push(form);
                let blocks = &mut section_mut(&mut data, &section)?.blocks;
                let insertion = insertion_index(blocks, after.as_ref(), BlockV2::id)?;
                blocks.insert(
                    insertion,
                    BlockV2::Form {
                        id: block_id.clone(),
                        form_id,
                    },
                );
                FocusTarget::Block(block_id)
            }
            Edit::SetFormPurpose { form, purpose } => {
                form_mut(&mut data, &form)?.purpose = purpose;
                form_focus_target(&data, &form)?
            }
            Edit::SetFormPresentation { form, presentation } => {
                form_mut(&mut data, &form)?.presentation = presentation;
                form_focus_target(&data, &form)?
            }
            Edit::SetFormResult { form, result } => {
                form_mut(&mut data, &form)?.result = result;
                form_focus_target(&data, &form)?
            }
            Edit::SetFormRequestConsent { form, copy } => {
                let unchanged = form_ref(&data, &form)?.request_consent.copy == copy;
                if !unchanged {
                    let version = allocate_consent_version(ids, &data)?;
                    form_mut(&mut data, &form)?.request_consent = ConsentCopy {
                        copy_version: super::StableDocumentId::from_validated(version),
                        copy,
                    };
                }
                form_focus_target(&data, &form)?
            }
            Edit::SetFormMarketingOptIn { form, copy } => {
                let current = data
                    .forms
                    .iter()
                    .find(|candidate| candidate.id == form)
                    .ok_or_else(|| DocumentError::at("validation/form_missing", "form_id"))?;
                let unchanged = copy.as_deref()
                    == current
                        .marketing_opt_in
                        .as_ref()
                        .map(|consent| consent.copy.as_str());
                let updated = if unchanged {
                    current.marketing_opt_in.clone()
                } else if let Some(copy) = copy {
                    let version = allocate_consent_version(ids, &data)?;
                    Some(ConsentCopy {
                        copy_version: super::StableDocumentId::from_validated(version),
                        copy,
                    })
                } else {
                    None
                };
                form_mut(&mut data, &form)?.marketing_opt_in = updated;
                form_focus_target(&data, &form)?
            }
            Edit::AddFormField { form, after, field } => {
                let existing = form_ref(&data, &form)?.fields.len();
                if existing >= 50 {
                    return Err(DocumentError::at(
                        "validation/form_field_count",
                        "forms.fields",
                    ));
                }
                let id =
                    FieldId::from_validated(allocate_id(ids, IdKind::Field, field_ids(&data))?);
                let target = form_mut(&mut data, &form)?;
                let insertion = insertion_index(&target.fields, after.as_ref(), FormField::id)?;
                target.fields.insert(insertion, field.with_id(id));
                form_focus_target(&data, &form)?
            }
            Edit::SetFormField { form, field, value } => {
                let target = form_mut(&mut data, &form)?;
                let existing = target
                    .fields
                    .iter()
                    .position(|candidate| candidate.id() == &field)
                    .ok_or_else(|| {
                        DocumentError::at("validation/form_field_missing", "field_id")
                    })?;
                target.fields[existing] = value.with_id(field);
                form_focus_target(&data, &form)?
            }
            Edit::DeleteFormField { form, field } => {
                let target = form_mut(&mut data, &form)?;
                if target.fields.len() <= 1 {
                    return Err(DocumentError::at(
                        "validation/form_field_count",
                        "forms.fields",
                    ));
                }
                let index = target
                    .fields
                    .iter()
                    .position(|candidate| candidate.id() == &field)
                    .ok_or_else(|| {
                        DocumentError::at("validation/form_field_missing", "field_id")
                    })?;
                target.fields.remove(index);
                form_focus_target(&data, &form)?
            }
            Edit::ReorderFormFields { form, ordered } => {
                let target = form_mut(&mut data, &form)?;
                if !is_exact_permutation(target.fields.iter().map(FormField::id), &ordered) {
                    return Err(DocumentError::at(
                        "validation/form_field_order_invalid",
                        "forms.fields",
                    ));
                }
                let mut prior = std::mem::take(&mut target.fields);
                let mut reordered = Vec::with_capacity(ordered.len());
                for id in &ordered {
                    let index = prior
                        .iter()
                        .position(|candidate| candidate.id() == id)
                        .ok_or_else(|| {
                            DocumentError::at("validation/form_field_order_invalid", "forms.fields")
                        })?;
                    reordered.push(prior.remove(index));
                }
                target.fields = reordered;
                form_focus_target(&data, &form)?
            }
        };

        data.generated_placeholders =
            super::placeholders::derive_generated_placeholders(self, &data);
        let expected_funnel_id = data.funnel_id.as_str().to_owned();
        super::validate::validate_data(&mut data, &expected_funnel_id)?;
        let document = DocumentV2::from_validated(data)?;
        Ok(EditOutcome { document, focus })
    }
}

impl NewBlock {
    fn with_id(self, id: super::BlockId) -> BlockV2 {
        match self {
            Self::Heading { level, text } => BlockV2::Heading { id, level, text },
            Self::Text { text } => BlockV2::Text { id, text },
            Self::List { items } => BlockV2::List { id, items },
            Self::Button { label, action } => BlockV2::Button { id, label, action },
            Self::Image { asset_id, alt } => BlockV2::Image { id, asset_id, alt },
            Self::Form { form_id } => BlockV2::Form { id, form_id },
            Self::Faq { items } => BlockV2::Faq { id, items },
            Self::Steps { items } => BlockV2::Steps { id, items },
            Self::OfferCard {
                title,
                scope,
                price,
                conditions,
            } => BlockV2::OfferCard {
                id,
                title,
                scope,
                price,
                conditions,
            },
            Self::OfferComparison { offer_ids } => BlockV2::OfferComparison { id, offer_ids },
        }
    }
}

fn offer_is_referenced(data: &DocumentData, offer_id: &BlockId) -> bool {
    data.pages.iter().any(|page| {
        page.sections.iter().any(|section| {
            section.blocks.iter().any(|block| {
                matches!(block, BlockV2::OfferComparison { offer_ids, .. } if offer_ids.contains(offer_id))
            })
        })
    })
}

impl NewFormField {
    fn with_id(self, id: FieldId) -> FormField {
        match self {
            Self::Text {
                name,
                label,
                help,
                required,
                validation,
                presentation,
            } => FormField::Text {
                id,
                name,
                label,
                help,
                required,
                validation,
                presentation,
            },
            Self::Email {
                name,
                label,
                help,
                required,
                validation,
            } => FormField::Email {
                id,
                name,
                label,
                help,
                required,
                validation,
            },
            Self::Phone {
                name,
                label,
                help,
                required,
                validation,
            } => FormField::Phone {
                id,
                name,
                label,
                help,
                required,
                validation,
            },
            Self::Checkbox {
                name,
                label,
                help,
                required,
            } => FormField::Checkbox {
                id,
                name,
                label,
                help,
                required,
            },
            Self::Select {
                name,
                label,
                help,
                required,
                options,
            } => FormField::Select {
                id,
                name,
                label,
                help,
                required,
                options,
            },
        }
    }
}

fn new_form(data: &DocumentData, ids: &mut impl IdSource) -> Result<FormV2, DocumentError> {
    let form_id = FormId::from_validated(allocate_id(ids, IdKind::Form, form_ids(data))?);
    let mut used_fields = field_ids(data);
    let name_id = allocate_id(ids, IdKind::Field, used_fields.clone())?;
    used_fields.insert(name_id.clone());
    let email_id = allocate_id(ids, IdKind::Field, used_fields)?;
    let copy_version = allocate_consent_version(ids, data)?;
    let english = data.language.unwrap_or(PublicLanguage::English) == PublicLanguage::English;
    Ok(FormV2 {
        id: form_id,
        fields: vec![
            FormField::Text {
                id: FieldId::from_validated(name_id),
                name: "name".to_owned(),
                label: if english { "Your name" } else { "Votre nom" }.to_owned(),
                help: None,
                required: false,
                validation: None,
                presentation: TextPresentation::SingleLine,
            },
            FormField::Email {
                id: FieldId::from_validated(email_id),
                name: "email".to_owned(),
                label: if english {
                    "Email address"
                } else {
                    "Adresse e-mail"
                }
                .to_owned(),
                help: None,
                required: true,
                validation: None,
            },
        ],
        purpose: localized_default(
            data.language,
            "Respond to this request.",
            "Répondre à cette demande.",
        ),
        request_consent: ConsentCopy {
            copy_version: super::StableDocumentId::from_validated(copy_version),
            copy: localized_default(
                data.language,
                "I agree to be contacted about this request.",
                "J’accepte d’être contacté au sujet de cette demande.",
            ),
        },
        marketing_opt_in: None,
        result: FormResult::Inline {
            message: localized_default(
                data.language,
                "Thanks — your request has been received.",
                "Merci, votre demande a bien été reçue.",
            ),
        },
        presentation: None,
    })
}

fn localized_default(language: Option<PublicLanguage>, english: &str, french: &str) -> String {
    match language.unwrap_or(PublicLanguage::English) {
        PublicLanguage::English => english,
        PublicLanguage::French => french,
    }
    .to_owned()
}

fn allocate_consent_version(
    ids: &mut impl IdSource,
    data: &DocumentData,
) -> Result<String, DocumentError> {
    let used = data
        .forms
        .iter()
        .flat_map(|form| {
            std::iter::once(form.request_consent.copy_version.as_str().to_owned()).chain(
                form.marketing_opt_in
                    .iter()
                    .map(|consent| consent.copy_version.as_str().to_owned()),
            )
        })
        .collect();
    allocate_id(ids, IdKind::ConsentCopy, used)
}

fn duplicate_section(
    data: &mut DocumentData,
    mut source: SectionV2,
    ids: &mut impl IdSource,
) -> Result<SectionV2, DocumentError> {
    source.id = SectionId::from_validated(allocate_id(ids, IdKind::Section, section_ids(data))?);
    let mut used_block_ids = block_ids(data);
    let mut copied_blocks = Vec::with_capacity(source.blocks.len());
    let mut copied_block_ids = BTreeMap::new();
    for block in &source.blocks {
        let value = allocate_id(ids, IdKind::Block, used_block_ids.clone())?;
        used_block_ids.insert(value.clone());
        let new_id = super::BlockId::from_validated(value);
        copied_block_ids.insert(block.id().clone(), new_id.clone());
        copied_blocks.push((block.clone(), new_id));
    }
    for (block, (original, new_id)) in source.blocks.iter_mut().zip(copied_blocks) {
        *block = duplicate_block_with_id(data, original, new_id, ids, &copied_block_ids)?;
    }
    Ok(source)
}

fn duplicate_block(
    data: &mut DocumentData,
    source: BlockV2,
    ids: &mut impl IdSource,
) -> Result<BlockV2, DocumentError> {
    let id = super::BlockId::from_validated(allocate_id(ids, IdKind::Block, block_ids(data))?);
    duplicate_block_with_id(data, source, id, ids, &BTreeMap::new())
}

fn duplicate_block_with_id(
    data: &mut DocumentData,
    source: BlockV2,
    id: super::BlockId,
    ids: &mut impl IdSource,
    copied_block_ids: &BTreeMap<BlockId, BlockId>,
) -> Result<BlockV2, DocumentError> {
    Ok(match source {
        BlockV2::Heading { level, text, .. } => BlockV2::Heading { id, level, text },
        BlockV2::Text { text, .. } => BlockV2::Text { id, text },
        BlockV2::List { items, .. } => BlockV2::List { id, items },
        BlockV2::Button { label, action, .. } => BlockV2::Button { id, label, action },
        BlockV2::Image { asset_id, alt, .. } => BlockV2::Image { id, asset_id, alt },
        BlockV2::Form { form_id, .. } => {
            let original = data
                .forms
                .iter()
                .find(|form| form.id == form_id)
                .cloned()
                .ok_or_else(|| DocumentError::at("validation/form_missing", "form_id"))?;
            let copied = duplicate_form(original, data, ids)?;
            let copied_id = copied.id.clone();
            data.forms.push(copied);
            BlockV2::Form {
                id,
                form_id: copied_id,
            }
        }
        BlockV2::Faq { items, .. } => BlockV2::Faq { id, items },
        BlockV2::Steps { items, .. } => BlockV2::Steps { id, items },
        BlockV2::OfferCard {
            title,
            scope,
            price,
            conditions,
            ..
        } => BlockV2::OfferCard {
            id,
            title,
            scope,
            price,
            conditions,
        },
        BlockV2::OfferComparison { offer_ids, .. } => BlockV2::OfferComparison {
            id,
            offer_ids: offer_ids
                .into_iter()
                .map(|offer_id| copied_block_ids.get(&offer_id).cloned().unwrap_or(offer_id))
                .collect(),
        },
    })
}

fn duplicate_form(
    mut form: FormV2,
    data: &DocumentData,
    ids: &mut impl IdSource,
) -> Result<FormV2, DocumentError> {
    form.id = FormId::from_validated(allocate_id(ids, IdKind::Form, form_ids(data))?);
    let mut used = field_ids(data);
    for field in &mut form.fields {
        let value = allocate_id(ids, IdKind::Field, used.clone())?;
        used.insert(value.clone());
        let fresh = FieldId::from_validated(value);
        field.replace_id(fresh);
    }
    Ok(form)
}

fn prune_unplaced_forms(data: &mut DocumentData) {
    let mut placed = BTreeSet::new();
    for page in &data.pages {
        for section in &page.sections {
            for block in &section.blocks {
                if let BlockV2::Form { form_id, .. } = block {
                    placed.insert(form_id.clone());
                }
            }
        }
    }
    data.forms.retain(|form| placed.contains(&form.id));
}

fn select_after_delete(sections: &[SectionV2], index: usize, page: PageId) -> FocusTarget {
    sections
        .get(index)
        .or_else(|| {
            index
                .checked_sub(1)
                .and_then(|previous| sections.get(previous))
        })
        .map(|section| FocusTarget::Section(section.id.clone()))
        .unwrap_or(FocusTarget::Page(page))
}

fn select_after_block_delete(
    data: &DocumentData,
    section_id: &SectionId,
    index: usize,
) -> Result<FocusTarget, DocumentError> {
    let section = data
        .pages
        .iter()
        .flat_map(|page| &page.sections)
        .find(|section| &section.id == section_id)
        .ok_or_else(|| DocumentError::new("internal/document_index_invalid"))?;
    Ok(section
        .blocks
        .get(index)
        .or_else(|| {
            index
                .checked_sub(1)
                .and_then(|previous| section.blocks.get(previous))
        })
        .map(|block| FocusTarget::Block(block.id().clone()))
        .unwrap_or_else(|| FocusTarget::Section(section_id.clone())))
}

fn is_exact_permutation<'a, I, T>(existing: I, requested: &[T]) -> bool
where
    I: Iterator<Item = &'a T>,
    T: Ord + 'a,
{
    let existing: BTreeSet<&T> = existing.collect();
    let requested_set: BTreeSet<&T> = requested.iter().collect();
    existing.len() == requested.len() && existing == requested_set
}

fn allocate_id(
    ids: &mut impl IdSource,
    kind: IdKind,
    used: BTreeSet<String>,
) -> Result<String, DocumentError> {
    let value = ids.next_id(kind)?;
    if !super::model::is_valid_stable_id(&value) || used.contains(&value) {
        return Err(DocumentError::at("validation/new_id_not_fresh", "id"));
    }
    Ok(value)
}

fn section_ids(data: &DocumentData) -> BTreeSet<String> {
    data.pages
        .iter()
        .flat_map(|page| page.sections.iter())
        .map(|section| section.id.as_str().to_owned())
        .collect()
}

fn page_ids(data: &DocumentData) -> BTreeSet<String> {
    data.pages
        .iter()
        .map(|page| page.id.as_str().to_owned())
        .collect()
}

fn block_ids(data: &DocumentData) -> BTreeSet<String> {
    data.pages
        .iter()
        .flat_map(|page| page.sections.iter())
        .flat_map(|section| section.blocks.iter())
        .map(|block| block.id().as_str().to_owned())
        .collect()
}

fn form_ids(data: &DocumentData) -> BTreeSet<String> {
    data.forms
        .iter()
        .map(|form| form.id.as_str().to_owned())
        .collect()
}

fn form_ref<'a>(data: &'a DocumentData, id: &FormId) -> Result<&'a FormV2, DocumentError> {
    data.forms
        .iter()
        .find(|form| &form.id == id)
        .ok_or_else(|| DocumentError::at("validation/form_missing", "form_id"))
}

fn form_mut<'a>(data: &'a mut DocumentData, id: &FormId) -> Result<&'a mut FormV2, DocumentError> {
    data.forms
        .iter_mut()
        .find(|form| &form.id == id)
        .ok_or_else(|| DocumentError::at("validation/form_missing", "form_id"))
}

fn form_focus_target(data: &DocumentData, id: &FormId) -> Result<FocusTarget, DocumentError> {
    data.pages
        .iter()
        .flat_map(|page| &page.sections)
        .flat_map(|section| &section.blocks)
        .find_map(|block| match block {
            BlockV2::Form {
                id: block_id,
                form_id,
            } if form_id == id => Some(FocusTarget::Block(block_id.clone())),
            _ => None,
        })
        .ok_or_else(|| DocumentError::new("internal/form_placement_missing"))
}

fn field_ids(data: &DocumentData) -> BTreeSet<String> {
    data.forms
        .iter()
        .flat_map(|form| form.fields.iter())
        .map(|field| field.id().as_str().to_owned())
        .collect()
}

fn page_ref<'a>(data: &'a DocumentData, id: &PageId) -> Result<&'a PageV2, DocumentError> {
    data.pages
        .iter()
        .find(|page| &page.id == id)
        .ok_or_else(|| DocumentError::at("validation/page_missing", "page_id"))
}

fn page_mut<'a>(data: &'a mut DocumentData, id: &PageId) -> Result<&'a mut PageV2, DocumentError> {
    data.pages
        .iter_mut()
        .find(|page| &page.id == id)
        .ok_or_else(|| DocumentError::at("validation/page_missing", "page_id"))
}

fn section_ref<'a>(data: &'a DocumentData, id: &SectionId) -> Result<&'a SectionV2, DocumentError> {
    data.pages
        .iter()
        .flat_map(|page| &page.sections)
        .find(|section| &section.id == id)
        .ok_or_else(|| DocumentError::at("validation/section_missing", "section_id"))
}

fn section_mut<'a>(
    data: &'a mut DocumentData,
    id: &SectionId,
) -> Result<&'a mut SectionV2, DocumentError> {
    data.pages
        .iter_mut()
        .flat_map(|page| &mut page.sections)
        .find(|section| &section.id == id)
        .ok_or_else(|| DocumentError::at("validation/section_missing", "section_id"))
}

fn insertion_index<T, Id: Ord>(
    items: &[T],
    after: Option<&Id>,
    id_of: impl Fn(&T) -> &Id,
) -> Result<usize, DocumentError> {
    let Some(after) = after else {
        return Ok(items.len());
    };
    items
        .iter()
        .position(|item| id_of(item) == after)
        .map(|index| index + 1)
        .ok_or_else(|| DocumentError::at("validation/insertion_anchor_missing", "after"))
}

impl FormField {
    pub fn id(&self) -> &FieldId {
        match self {
            Self::Text { id, .. }
            | Self::Email { id, .. }
            | Self::Phone { id, .. }
            | Self::Checkbox { id, .. }
            | Self::Select { id, .. } => id,
        }
    }

    fn replace_id(&mut self, new_id: FieldId) {
        match self {
            Self::Text { id, .. }
            | Self::Email { id, .. }
            | Self::Phone { id, .. }
            | Self::Checkbox { id, .. }
            | Self::Select { id, .. } => *id = new_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{document_v2::BlueprintId, ThemePreset};

    struct TestIds(u32);

    impl IdSource for TestIds {
        fn next_id(&mut self, _kind: IdKind) -> Result<String, DocumentError> {
            self.0 += 1;
            Ok(format!("edit-test-{}", self.0))
        }
    }

    struct OperationIds {
        operation_id: String,
        next: u32,
    }

    impl IdSource for OperationIds {
        fn next_id(&mut self, _kind: IdKind) -> Result<String, DocumentError> {
            self.next += 1;
            Ok(format!("{}-{}", self.operation_id, self.next))
        }
    }

    fn starter() -> DocumentV2 {
        DocumentV2::starter(
            "funnel-edit-test",
            BlueprintId::Tripwire,
            ThemePreset::Minimal,
        )
        .expect("valid starter document")
    }

    #[test]
    fn form_creation_works_with_worker_operation_ids_and_existing_block_anchor() {
        let mut value: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/document_v2_minimal.json"
        ))
        .expect("minimal fixture");
        value["funnel_id"] = serde_json::json!("funnel-worker-edit-test");
        let raw = serde_json::to_vec(&value).expect("fixture JSON");
        let document = DocumentV2::decode(&raw, "funnel-worker-edit-test").expect("valid fixture");
        let mut ids = OperationIds {
            operation_id: "a".repeat(32),
            next: 0,
        };
        let outcome = document.apply(
            Edit::CreateForm {
                section: SectionId::parse("section-hero").unwrap(),
                after: Some(BlockId::parse("block-copy").unwrap()),
            },
            &mut ids,
        );
        assert!(outcome.is_ok(), "{}", outcome.err().unwrap());
    }

    #[test]
    fn form_edits_keep_focus_on_the_form_placement() {
        let mut ids = TestIds(0);
        let created = starter()
            .apply(
                Edit::CreateForm {
                    section: SectionId::parse("section-hero").unwrap(),
                    after: None,
                },
                &mut ids,
            )
            .expect("form can be created");
        let form = created.document.forms()[0].id().clone();
        let block = created
            .document
            .pages()
            .iter()
            .flat_map(|page| page.sections())
            .flat_map(|section| section.blocks())
            .find(|block| matches!(block, BlockV2::Form { form_id, .. } if form_id == &form))
            .expect("form placement exists")
            .id()
            .clone();

        let edited = created
            .document
            .apply(
                Edit::SetFormPurpose {
                    form,
                    purpose: "Collect a qualified request.".to_owned(),
                },
                &mut ids,
            )
            .expect("form purpose can be edited");

        assert_eq!(edited.focus, FocusTarget::Block(block));
    }

    fn apply(document: &DocumentV2, edit: Edit, ids: &mut TestIds) -> DocumentV2 {
        document
            .apply(edit, ids)
            .expect("valid typed edit")
            .document
    }

    #[test]
    fn page_creation_and_metadata_edits_keep_the_entry_route_stable() {
        let document = starter();
        let entry_page_id = document.entry_page_id().clone();
        let mut ids = TestIds(0);
        let created = document
            .apply(
                Edit::AddPage {
                    from: document.entry_page_id().clone(),
                    title: "Step 2".to_owned(),
                    slug: "step-2".to_owned(),
                },
                &mut ids,
            )
            .expect("page can be added");
        let page_id = created
            .document
            .pages()
            .last()
            .expect("new page")
            .id()
            .clone();
        assert_eq!(page_id.as_str(), "edit-test-1");
        assert_eq!(created.document.pages()[1].title(), "Step 2");
        assert_eq!(created.document.pages()[1].slug(), "step-2");
        assert_eq!(created.focus, FocusTarget::Page(page_id.clone()));
        assert_eq!(created.document.entry_page_id(), &entry_page_id);

        let renamed = created
            .document
            .apply(
                Edit::SetPage {
                    page: page_id.clone(),
                    title: "Order details".to_owned(),
                    slug: "order-details".to_owned(),
                },
                &mut ids,
            )
            .expect("page metadata can be edited");
        let updated_page = renamed.document.page(&page_id).expect("updated page");
        assert_eq!(updated_page.title(), "Order details");
        assert_eq!(updated_page.slug(), "order-details");
        assert_eq!(renamed.focus, FocusTarget::Page(page_id));
        assert_eq!(renamed.document.entry_page_id(), &entry_page_id);
    }

    #[test]
    fn page_deletion_protects_the_entry_page_and_removes_inbound_navigation() {
        let mut ids = TestIds(0);
        let entry = starter();
        let entry_error = entry
            .apply(
                Edit::DeletePage {
                    page: entry.entry_page_id().clone(),
                },
                &mut ids,
            )
            .expect_err("entry page cannot be removed");
        assert_eq!(entry_error.code(), "validation/entry_page_delete");

        let mut value: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../tests/fixtures/document_v2_minimal.json"
        ))
        .expect("minimal fixture JSON");
        value["pages"]
            .as_array_mut()
            .expect("pages array")
            .push(serde_json::json!({
                "id": "page-next",
                "slug": "next",
                "title": "Next",
                "sections": []
            }));
        value["pages"][0]["sections"][0]["blocks"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "id": "block-next",
                "kind": "button",
                "label": "Continue",
                "action": { "kind": "navigate_page", "page_id": "page-next" }
            }));
        let raw = serde_json::to_vec(&value).expect("document JSON");
        let document = DocumentV2::decode(&raw, "funnel-01").expect("valid linked page");
        let deleted = document
            .apply(
                Edit::DeletePage {
                    page: PageId::parse("page-next").expect("page id"),
                },
                &mut ids,
            )
            .expect("linked page can be deleted with its navigation link");
        assert_eq!(deleted.document.pages().len(), 1);
        assert_eq!(
            deleted.focus,
            FocusTarget::Page(deleted.document.entry_page_id().clone())
        );
        assert!(!deleted.document.pages()[0]
            .sections()
            .iter()
            .flat_map(|section| section.blocks())
            .any(|block| matches!(block, BlockV2::Button { action: super::super::ActionV2::NavigatePage { page_id }, .. } if page_id.as_str() == "page-next")));
    }

    #[test]
    fn deleting_a_page_removes_its_unplaced_form() {
        let mut ids = TestIds(0);
        let document = starter();
        let created_page = document
            .apply(
                Edit::AddPage {
                    from: document.entry_page_id().clone(),
                    title: "Step 2".to_owned(),
                    slug: "step-2".to_owned(),
                },
                &mut ids,
            )
            .expect("page can be added");
        let page_id = created_page.document.pages()[1].id().clone();
        let section = created_page
            .document
            .apply(
                Edit::AddSection {
                    page: page_id.clone(),
                    after: None,
                    section_type: SectionKind::Hero,
                },
                &mut ids,
            )
            .expect("section can be added");
        let section_id = match &section.focus {
            FocusTarget::Section(id) => id.clone(),
            _ => panic!("new section should be focused"),
        };
        let with_form = section
            .document
            .apply(
                Edit::CreateForm {
                    section: section_id,
                    after: None,
                },
                &mut ids,
            )
            .expect("form can be placed on new page");
        assert_eq!(with_form.document.forms().len(), 1);

        let deleted = with_form
            .document
            .apply(Edit::DeletePage { page: page_id }, &mut ids)
            .expect("unreferenced page can be deleted");
        assert_eq!(deleted.document.pages().len(), 1);
        assert!(deleted.document.forms().is_empty());
        assert_eq!(
            deleted.focus,
            FocusTarget::Page(deleted.document.entry_page_id().clone())
        );
    }

    #[test]
    fn deleting_a_page_moves_form_confirmation_inline() {
        let mut ids = TestIds(0);
        let entry = starter();
        let page = entry
            .apply(
                Edit::AddPage {
                    from: entry.entry_page_id().clone(),
                    title: "Thanks".to_owned(),
                    slug: "thanks".to_owned(),
                },
                &mut ids,
            )
            .expect("success page can be added");
        let page_id = page.document.pages()[1].id().clone();
        let section_id = page.document.pages()[0].sections()[0].id().clone();
        let form = page
            .document
            .apply(
                Edit::CreateForm {
                    section: section_id,
                    after: None,
                },
                &mut ids,
            )
            .expect("form can be placed on the entry page");
        let form_id = form.document.forms()[0].id().clone();
        let configured = form
            .document
            .apply(
                Edit::SetFormResult {
                    form: form_id.clone(),
                    result: FormResult::SuccessPage {
                        page_id: page_id.clone(),
                    },
                },
                &mut ids,
            )
            .expect("form can redirect to the new page");

        let deleted = configured
            .document
            .apply(Edit::DeletePage { page: page_id }, &mut ids)
            .expect("deleting the success page keeps the form usable");
        assert!(matches!(
            deleted.document.form(&form_id).expect("retained form").result(),
            FormResult::Inline { message } if message == "Thank you."
        ));
    }

    #[test]
    fn page_deletion_fails_atomically_when_it_would_orphan_a_later_step() {
        let mut ids = TestIds(0);
        let entry = starter();
        let first_step = entry
            .apply(
                Edit::AddPage {
                    from: entry.entry_page_id().clone(),
                    title: "Step 2".to_owned(),
                    slug: "step-2".to_owned(),
                },
                &mut ids,
            )
            .expect("first step can be added");
        let first_step_id = first_step.document.pages()[1].id().clone();
        let later_step = first_step
            .document
            .apply(
                Edit::AddPage {
                    from: first_step_id.clone(),
                    title: "Step 3".to_owned(),
                    slug: "step-3".to_owned(),
                },
                &mut ids,
            )
            .expect("later step can be added");
        let error = later_step
            .document
            .apply(
                Edit::DeletePage {
                    page: first_step_id,
                },
                &mut ids,
            )
            .expect_err("deletion must not orphan the later step");
        assert_eq!(error.code(), "validation/page_unreachable");
        assert_eq!(later_step.document.pages().len(), 3);
    }

    #[test]
    fn form_creation_and_configuration_preserve_valid_typed_definitions() {
        let mut ids = TestIds(0);
        let original = starter();
        let page = original.pages()[0].id().clone();
        let section = original.pages()[0].sections()[0].id().clone();
        let original_block_count = original.pages()[0].sections()[0].blocks().len();
        let created = apply(
            &original,
            Edit::CreateForm {
                section: section.clone(),
                after: None,
            },
            &mut ids,
        );
        assert_eq!(created.forms().len(), 1);
        assert_eq!(
            created.pages()[0].sections()[0].blocks().len(),
            original_block_count + 1
        );
        let form = created.forms()[0].clone();
        assert_eq!(form.fields().len(), 2);
        assert!(form
            .fields()
            .iter()
            .any(|field| matches!(field, FormField::Email { required: true, .. })));

        let updated = apply(
            &created,
            Edit::SetFormPurpose {
                form: form.id().clone(),
                purpose: "Answer the visitor's question.".to_owned(),
            },
            &mut ids,
        );
        let updated = apply(
            &updated,
            Edit::SetFormRequestConsent {
                form: form.id().clone(),
                copy: "I agree to receive a response.".to_owned(),
            },
            &mut ids,
        );
        let consent_version = updated
            .form(form.id())
            .unwrap()
            .request_consent()
            .copy_version()
            .clone();
        assert_ne!(
            consent_version,
            form.request_consent().copy_version().clone()
        );

        let updated = apply(
            &updated,
            Edit::SetFormMarketingOptIn {
                form: form.id().clone(),
                copy: Some("Send me occasional product updates.".to_owned()),
            },
            &mut ids,
        );
        assert!(updated
            .form(form.id())
            .unwrap()
            .marketing_opt_in()
            .is_some());

        let new_field = NewFormField::Select {
            name: "interest".to_owned(),
            label: "What are you interested in?".to_owned(),
            help: Some("Choose one option.".to_owned()),
            required: false,
            options: vec!["Consulting".to_owned(), "Training".to_owned()],
        };
        let after = form.fields().last().unwrap().id().clone();
        let updated = apply(
            &updated,
            Edit::AddFormField {
                form: form.id().clone(),
                after: Some(after),
                field: new_field,
            },
            &mut ids,
        );
        let added_id = updated.form(form.id()).unwrap().fields()[2].id().clone();
        let updated = apply(
            &updated,
            Edit::ReorderFormFields {
                form: form.id().clone(),
                ordered: vec![
                    added_id.clone(),
                    form.fields()[0].id().clone(),
                    form.fields()[1].id().clone(),
                ],
            },
            &mut ids,
        );
        assert_eq!(updated.form(form.id()).unwrap().fields()[0].id(), &added_id);

        let round_trip = DocumentV2::decode(updated.canonical_bytes(), "funnel-edit-test")
            .expect("edited form remains decodable");
        assert_eq!(round_trip.canonical_bytes(), updated.canonical_bytes());

        let deleted = apply(
            &updated,
            Edit::DeleteFormField {
                form: form.id().clone(),
                field: added_id,
            },
            &mut ids,
        );
        assert_eq!(deleted.form(form.id()).unwrap().fields().len(), 2);
        assert_eq!(deleted.pages()[0].id(), &page);
    }

    #[test]
    fn changing_required_consent_copy_creates_a_fresh_evidence_version() {
        let mut ids = TestIds(0);
        let original = starter();
        let section = original.pages()[0].sections()[0].id().clone();
        let created = apply(
            &original,
            Edit::CreateForm {
                section,
                after: None,
            },
            &mut ids,
        );
        let form = created.forms()[0].clone();
        let updated = apply(
            &created,
            Edit::SetFormRequestConsent {
                form: form.id().clone(),
                copy: "Updated request consent wording.".to_owned(),
            },
            &mut ids,
        );
        assert_ne!(
            form.request_consent().copy_version(),
            updated
                .form(form.id())
                .unwrap()
                .request_consent()
                .copy_version()
        );
    }

    #[test]
    fn setting_identical_request_consent_preserves_its_evidence_version() {
        let mut create_ids = TestIds(0);
        let original = starter();
        let section = original.pages()[0].sections()[0].id().clone();
        let created = apply(
            &original,
            Edit::CreateForm {
                section,
                after: None,
            },
            &mut create_ids,
        );
        let form = created.forms()[0].clone();
        let before = created.canonical_bytes();
        let consent_version = form.request_consent().copy_version().clone();
        let mut update_ids = TestIds(100);

        let updated = created
            .apply(
                Edit::SetFormRequestConsent {
                    form: form.id().clone(),
                    copy: form.request_consent().copy().to_owned(),
                },
                &mut update_ids,
            )
            .expect("setting the same copy remains valid")
            .document;

        assert_eq!(updated.canonical_bytes(), before);
        assert_eq!(
            update_ids.0, 100,
            "no new evidence version should be allocated"
        );
        assert_eq!(
            updated
                .form(form.id())
                .unwrap()
                .request_consent()
                .copy_version(),
            &consent_version
        );
    }

    #[test]
    fn form_field_cannot_be_deleted_below_the_required_minimum() {
        let mut ids = TestIds(0);
        let original = starter();
        let section = original.pages()[0].sections()[0].id().clone();
        let created = apply(
            &original,
            Edit::CreateForm {
                section,
                after: None,
            },
            &mut ids,
        );
        let form = created.forms()[0].clone();
        let first = apply(
            &created,
            Edit::DeleteFormField {
                form: form.id().clone(),
                field: form.fields()[0].id().clone(),
            },
            &mut ids,
        );
        assert!(first
            .apply(
                Edit::DeleteFormField {
                    form: form.id().clone(),
                    field: first.form(form.id()).unwrap().fields()[0].id().clone(),
                },
                &mut ids,
            )
            .is_err());
    }
}
