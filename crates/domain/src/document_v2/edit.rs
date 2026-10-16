use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::model::{
    BlockV2, DocumentData, DocumentError, DocumentV2, FieldId, FormField, FormId, FormV2, PageId,
    PageV2, SectionId, SectionKind, SectionV2,
};
use crate::ThemePreset;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdKind {
    Section,
    Block,
    Form,
    Field,
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
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    SetThemePreset {
        preset: ThemePreset,
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
    fn id(&self) -> &FieldId {
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
