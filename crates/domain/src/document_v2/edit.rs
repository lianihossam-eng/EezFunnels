use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::model::{
    BlockId, BlockV2, ConsentCopy, DocumentData, DocumentError, DocumentV2, FieldId, FormField,
    FormId, FormResult, FormV2, PageId, PageV2, SectionId, SectionKind, SectionV2, ValidationRange,
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
    SetPageTitle {
        page: PageId,
        value: String,
    },
    SetPageSlug {
        page: PageId,
        value: String,
    },
    AddPage {
        from: PageId,
        title: String,
        slug: String,
    },
    DeletePage {
        page: PageId,
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
    Page(PageId),
    Section(SectionId),
    Block(super::BlockId),
}

impl FocusTarget {
    pub fn section_id(&self) -> Option<&SectionId> {
        match self {
            Self::Section(section) => Some(section),
            Self::Block(_) | Self::Page(_) => None,
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
            Edit::SetPageTitle { page, value } => {
                page_mut(&mut data, &page)?.title = value;
                FocusTarget::Page(page)
            }
            Edit::SetPageSlug { page, value } => {
                page_mut(&mut data, &page)?.slug = value;
                FocusTarget::Page(page)
            }
            Edit::AddPage { from, title, slug } => {
                let source_index = data
                    .pages
                    .iter()
                    .position(|page| page.id == from)
                    .ok_or_else(|| DocumentError::at("validation/page_missing", "page_id"))?;
                let page_id =
                    PageId::from_validated(allocate_id(ids, IdKind::Page, page_ids(&data))?);
                let mut used_section_ids = section_ids(&data);
                let section_id = SectionId::from_validated(allocate_id(
                    ids,
                    IdKind::Section,
                    used_section_ids.clone(),
                )?);
                used_section_ids.insert(section_id.as_str().to_owned());
                let mut used_block_ids = block_ids(&data);
                let hero_title_id = BlockId::from_validated(allocate_id(
                    ids,
                    IdKind::Block,
                    used_block_ids.clone(),
                )?);
                used_block_ids.insert(hero_title_id.as_str().to_owned());
                let hero_copy_id = BlockId::from_validated(allocate_id(
                    ids,
                    IdKind::Block,
                    used_block_ids.clone(),
                )?);
                used_block_ids.insert(hero_copy_id.as_str().to_owned());
                let button_id = BlockId::from_validated(allocate_id(
                    ids,
                    IdKind::Block,
                    used_block_ids.clone(),
                )?);
                used_block_ids.insert(button_id.as_str().to_owned());
                let new_page_title = title.clone();
                let new_page = PageV2 {
                    id: page_id.clone(),
                    slug,
                    title,
                    seo_title: None,
                    seo_description: None,
                    noindex: false,
                    sections: vec![SectionV2 {
                        id: section_id,
                        section_type: SectionKind::Hero,
                        blocks: vec![
                            BlockV2::Heading {
                                id: hero_title_id,
                                level: 1,
                                text: new_page_title,
                            },
                            BlockV2::Text {
                                id: hero_copy_id,
                                text: "Replace this text with your page content.".to_owned(),
                            },
                        ],
                    }],
                };

                let route_button = BlockV2::Button {
                    id: button_id,
                    label: "Continue".to_owned(),
                    action: super::ActionV2::NavigatePage {
                        page_id: page_id.clone(),
                    },
                };
                let cta_section = data.pages[source_index]
                    .sections
                    .iter()
                    .position(|section| {
                        section.section_type == SectionKind::Cta && section.blocks.len() < 50
                    });
                if let Some(section_index) = cta_section {
                    data.pages[source_index].sections[section_index]
                        .blocks
                        .push(route_button);
                } else {
                    let cta_section_id = SectionId::from_validated(allocate_id(
                        ids,
                        IdKind::Section,
                        used_section_ids,
                    )?);
                    let cta_heading_id = BlockId::from_validated(allocate_id(
                        ids,
                        IdKind::Block,
                        used_block_ids.clone(),
                    )?);
                    used_block_ids.insert(cta_heading_id.as_str().to_owned());
                    let cta_copy_id =
                        BlockId::from_validated(allocate_id(ids, IdKind::Block, used_block_ids)?);
                    data.pages[source_index].sections.push(SectionV2 {
                        id: cta_section_id,
                        section_type: SectionKind::Cta,
                        blocks: vec![
                            BlockV2::Heading {
                                id: cta_heading_id,
                                level: 2,
                                text: "Continue to the next step".to_owned(),
                            },
                            BlockV2::Text {
                                id: cta_copy_id,
                                text: "Use this button to open the new page.".to_owned(),
                            },
                            route_button,
                        ],
                    });
                }
                data.pages.push(new_page);
                FocusTarget::Page(page_id)
            }
            Edit::DeletePage { page } => {
                if page == data.entry_page_id {
                    return Err(DocumentError::at(
                        "validation/entry_page_delete",
                        "pages.id",
                    ));
                }
                if !data.pages.iter().any(|candidate| candidate.id == page) {
                    return Err(DocumentError::at("validation/page_missing", "page_id"));
                }
                data.pages.retain(|candidate| candidate.id != page);
                for candidate in &mut data.pages {
                    for section in &mut candidate.sections {
                        section.blocks.retain(|block| {
                            !matches!(block, BlockV2::Button {
                                action: super::ActionV2::NavigatePage { page_id }, ..
                            } if page_id == &page)
                        });
                    }
                }
                for form in &mut data.forms {
                    if matches!(
                        &form.result,
                        FormResult::SuccessPage { page_id } if page_id == &page
                    ) {
                        form.result = FormResult::Inline {
                            message: DEFAULT_FORM_CONFIRMATION.to_owned(),
                        };
                    }
                }
                prune_unplaced_forms(&mut data);
                FocusTarget::Page(data.entry_page_id.clone())
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
                let sections = &mut page_mut(&mut data, &page)?.sections;
                let index = sections
                    .iter()
                    .position(|candidate| candidate.id == section)
                    .ok_or_else(|| DocumentError::at("validation/section_missing", "section_id"))?;
                sections.remove(index);
                let focus = select_after_delete(sections, index, page);
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
                let blocks = &mut section_mut(&mut data, &section)?.blocks;
                let index = blocks
                    .iter()
                    .position(|candidate| candidate.id() == &block)
                    .ok_or_else(|| DocumentError::at("validation/block_missing", "block_id"))?;
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
                FocusTarget::Page(data.entry_page_id.clone())
            }
            Edit::SetFormResult { form, result } => {
                form_mut(&mut data, &form)?.result = result;
                FocusTarget::Page(data.entry_page_id.clone())
            }
            Edit::SetFormRequestConsent { form, copy } => {
                let version = allocate_consent_version(ids, &data)?;
                let target = form_mut(&mut data, &form)?;
                target.request_consent = ConsentCopy {
                    copy_version: super::StableDocumentId::from_validated(version),
                    copy,
                };
                FocusTarget::Page(data.entry_page_id.clone())
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
                FocusTarget::Page(data.entry_page_id.clone())
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
                FocusTarget::Page(data.entry_page_id.clone())
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
                FocusTarget::Page(data.entry_page_id.clone())
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
                FocusTarget::Page(data.entry_page_id.clone())
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
                FocusTarget::Page(data.entry_page_id.clone())
            }
        };

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
        }
    }
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
            } => FormField::Text {
                id,
                name,
                label,
                help,
                required,
                validation,
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
    Ok(FormV2 {
        id: form_id,
        fields: vec![
            FormField::Text {
                id: FieldId::from_validated(name_id),
                name: "name".to_owned(),
                label: "Your name".to_owned(),
                help: None,
                required: false,
                validation: None,
            },
            FormField::Email {
                id: FieldId::from_validated(email_id),
                name: "email".to_owned(),
                label: "Email address".to_owned(),
                help: None,
                required: true,
                validation: None,
            },
        ],
        purpose: "Respond to this request.".to_owned(),
        request_consent: ConsentCopy {
            copy_version: super::StableDocumentId::from_validated(copy_version),
            copy: "I agree to be contacted about this request.".to_owned(),
        },
        marketing_opt_in: None,
        result: FormResult::Inline {
            message: DEFAULT_FORM_CONFIRMATION.to_owned(),
        },
    })
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
    for block in &mut source.blocks {
        let old = block.clone();
        let value = allocate_id(ids, IdKind::Block, used_block_ids.clone())?;
        used_block_ids.insert(value.clone());
        let new_id = super::BlockId::from_validated(value);
        *block = duplicate_block_with_id(data, old, new_id, ids)?;
    }
    Ok(source)
}

fn duplicate_block(
    data: &mut DocumentData,
    source: BlockV2,
    ids: &mut impl IdSource,
) -> Result<BlockV2, DocumentError> {
    let id = super::BlockId::from_validated(allocate_id(ids, IdKind::Block, block_ids(data))?);
    duplicate_block_with_id(data, source, id, ids)
}

fn duplicate_block_with_id(
    data: &mut DocumentData,
    source: BlockV2,
    id: super::BlockId,
    ids: &mut impl IdSource,
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

const DEFAULT_FORM_CONFIRMATION: &str = "Thanks — your request has been received.";

fn page_ids(data: &DocumentData) -> BTreeSet<String> {
    data.pages
        .iter()
        .map(|page| page.id.as_str().to_owned())
        .collect()
}

fn section_ids(data: &DocumentData) -> BTreeSet<String> {
    data.pages
        .iter()
        .flat_map(|page| page.sections.iter())
        .map(|section| section.id.as_str().to_owned())
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
            BlueprintId::LeadGen,
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

    fn apply(document: &DocumentV2, edit: Edit, ids: &mut TestIds) -> DocumentV2 {
        document
            .apply(edit, ids)
            .expect("valid typed edit")
            .document
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
