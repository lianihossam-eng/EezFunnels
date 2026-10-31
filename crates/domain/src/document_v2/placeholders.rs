use serde::{Deserialize, Serialize};

use super::model::{
    BlockV2, DocumentData, DocumentError, DocumentV2, FormField, FormResult, ImageAlt, PageV2,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneratedPlaceholderCode {
    OfferText,
    TestimonialPrompt,
    LegacyTestimonialQuote,
    LegacyTestimonialAuthor,
    LegacyTestimonialAttribution,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneratedPlaceholderPageField {
    Title,
    SeoTitle,
    SeoDescription,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GeneratedPlaceholderTarget {
    PageField {
        page_id: super::PageId,
        field: GeneratedPlaceholderPageField,
    },
    Block {
        page_id: super::PageId,
        section_id: super::SectionId,
        block_id: super::BlockId,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedPlaceholderMarker {
    pub target: GeneratedPlaceholderTarget,
    pub code: GeneratedPlaceholderCode,
    pub expected_text: String,
}

impl DocumentV2 {
    pub fn generated_placeholders(&self) -> &[GeneratedPlaceholderMarker] {
        &self.data.generated_placeholders
    }

    pub fn reconcile_generated_placeholders_from(
        &self,
        source: &DocumentV2,
    ) -> Result<Self, DocumentError> {
        let mut data = self.data.clone();
        let mut markers = derive_generated_placeholders(source, &data);
        markers.extend(derive_generated_placeholders(self, &data));
        markers.sort();
        markers.dedup();
        data.generated_placeholders = markers;
        Self::from_validated(data)
    }

    pub fn has_valid_generated_placeholder_transition(&self, candidate: &DocumentV2) -> bool {
        derive_generated_placeholders(self, &candidate.data)
            .iter()
            .all(|required| candidate.data.generated_placeholders.contains(required))
    }
}

pub(crate) fn derive_generated_placeholders(
    source: &DocumentV2,
    candidate: &DocumentData,
) -> Vec<GeneratedPlaceholderMarker> {
    let mut markers = Vec::new();
    for source_marker in &source.data.generated_placeholders {
        for page in &candidate.pages {
            for field in [
                GeneratedPlaceholderPageField::Title,
                GeneratedPlaceholderPageField::SeoTitle,
                GeneratedPlaceholderPageField::SeoDescription,
            ] {
                if page_field_value(page, field)
                    .is_some_and(|value| value.contains(&source_marker.expected_text))
                {
                    markers.push(GeneratedPlaceholderMarker {
                        target: GeneratedPlaceholderTarget::PageField {
                            page_id: page.id.clone(),
                            field,
                        },
                        code: source_marker.code,
                        expected_text: source_marker.expected_text.clone(),
                    });
                }
            }
            for section in &page.sections {
                for block in &section.blocks {
                    if block_contains_text(candidate, block, &source_marker.expected_text) {
                        markers.push(GeneratedPlaceholderMarker {
                            target: GeneratedPlaceholderTarget::Block {
                                page_id: page.id.clone(),
                                section_id: section.id.clone(),
                                block_id: block.id().clone(),
                            },
                            code: source_marker.code,
                            expected_text: source_marker.expected_text.clone(),
                        });
                    }
                }
            }
        }
    }
    markers.sort();
    markers.dedup();
    markers
}

pub(super) fn target_contains_text(
    data: &DocumentData,
    target: &GeneratedPlaceholderTarget,
    expected_text: &str,
) -> bool {
    match target {
        GeneratedPlaceholderTarget::PageField { page_id, field } => data
            .pages
            .iter()
            .find(|page| &page.id == page_id)
            .and_then(|page| page_field_value(page, *field))
            .is_some_and(|value| value.contains(expected_text)),
        GeneratedPlaceholderTarget::Block {
            page_id,
            section_id,
            block_id,
        } => data
            .pages
            .iter()
            .find(|page| &page.id == page_id)
            .and_then(|page| {
                page.sections
                    .iter()
                    .find(|section| &section.id == section_id)
            })
            .and_then(|section| section.blocks.iter().find(|block| block.id() == block_id))
            .is_some_and(|block| block_contains_text(data, block, expected_text)),
    }
}

fn page_field_value(page: &PageV2, field: GeneratedPlaceholderPageField) -> Option<&str> {
    match field {
        GeneratedPlaceholderPageField::Title => Some(&page.title),
        GeneratedPlaceholderPageField::SeoTitle => page.seo_title.as_deref(),
        GeneratedPlaceholderPageField::SeoDescription => page.seo_description.as_deref(),
    }
}

fn block_contains_text(data: &DocumentData, block: &BlockV2, expected_text: &str) -> bool {
    match block {
        BlockV2::Heading { text, .. } | BlockV2::Text { text, .. } => text.contains(expected_text),
        BlockV2::List { items, .. } => items.iter().any(|item| item.contains(expected_text)),
        BlockV2::Button { label, .. } => label.contains(expected_text),
        BlockV2::Faq { items, .. } => items.iter().any(|item| {
            item.question.contains(expected_text) || item.answer.contains(expected_text)
        }),
        BlockV2::Steps { items, .. } => items
            .iter()
            .any(|item| item.title.contains(expected_text) || item.detail.contains(expected_text)),
        BlockV2::OfferCard {
            title,
            scope,
            price,
            conditions,
            ..
        } => {
            title.contains(expected_text)
                || scope.contains(expected_text)
                || conditions.contains(expected_text)
                || price.as_ref().is_some_and(|price| {
                    let unit = match price {
                        super::model::OfferPriceV2::OneTime { unit, .. }
                        | super::model::OfferPriceV2::Recurring { unit, .. } => unit,
                    };
                    unit.contains(expected_text)
                })
        }
        BlockV2::OfferComparison { .. } => false,
        BlockV2::Image {
            alt: ImageAlt::Text { value },
            ..
        } => value.contains(expected_text),
        BlockV2::Image {
            alt: ImageAlt::Decorative,
            ..
        } => false,
        BlockV2::Form { form_id, .. } => data
            .forms
            .iter()
            .find(|form| &form.id == form_id)
            .is_some_and(|form| form_contains_text(form, expected_text)),
    }
}

fn form_contains_text(form: &super::model::FormV2, expected_text: &str) -> bool {
    form.purpose.contains(expected_text)
        || form.request_consent.copy.contains(expected_text)
        || form
            .marketing_opt_in
            .as_ref()
            .is_some_and(|consent| consent.copy.contains(expected_text))
        || form
            .presentation
            .as_ref()
            .and_then(|presentation| presentation.submit_label.as_deref())
            .is_some_and(|label| label.contains(expected_text))
        || matches!(&form.result, FormResult::Inline { message } if message.contains(expected_text))
        || form
            .fields
            .iter()
            .any(|field| form_field_contains_text(field, expected_text))
}

fn form_field_contains_text(field: &FormField, expected_text: &str) -> bool {
    field.label().contains(expected_text)
        || match field {
            FormField::Text { help, .. }
            | FormField::Email { help, .. }
            | FormField::Phone { help, .. }
            | FormField::Checkbox { help, .. }
            | FormField::Select { help, .. } => help
                .as_deref()
                .is_some_and(|help| help.contains(expected_text)),
        }
        || matches!(field, FormField::Select { options, .. } if options.iter().any(|option| option.contains(expected_text)))
}
