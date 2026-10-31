use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::model::{
    is_valid_stable_id, ActionV2, AssetId, BlockId, BlockV2, DocumentError, DocumentV2,
    EvidenceImage, FieldId, FormField, FormId, FormResult, PageId, SectionId, StableDocumentId,
};
use super::{IdKind, IdSource, MAX_DOCUMENT_DEPTH, MAX_RAW_DOCUMENT_BYTES};

pub const PORTABLE_TEMPLATE_FORMAT: &str = "eezfunnel.funnel-template";
pub const PORTABLE_TEMPLATE_VERSION: u32 = 1;
pub const MAX_TEMPLATE_MEDIA_ASSETS: usize = 1_000;
pub const MAX_PORTABLE_TEMPLATE_BYTES: usize = 1_400_000;
const MAX_TEMPLATE_NAME_BYTES: usize = 120;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateMediaAsset {
    asset_id: AssetId,
    sha256: String,
    mime: String,
    size_bytes: u64,
}

impl TemplateMediaAsset {
    pub fn new(
        asset_id: impl Into<String>,
        sha256: impl Into<String>,
        mime: impl Into<String>,
        size_bytes: u64,
    ) -> Result<Self, PortableTemplateError> {
        let asset_id =
            AssetId::parse(asset_id.into()).map_err(|_| PortableTemplateError::Invalid)?;
        let asset = Self {
            asset_id,
            sha256: sha256.into(),
            mime: mime.into(),
            size_bytes,
        };
        asset.validate()?;
        Ok(asset)
    }

    pub fn asset_id(&self) -> &AssetId {
        &self.asset_id
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn mime(&self) -> &str {
        &self.mime
    }

    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    fn validate(&self) -> Result<(), PortableTemplateError> {
        if !is_lower_sha256(self.asset_id.as_str())
            || self.sha256 != self.asset_id.as_str()
            || !matches!(
                self.mime.as_str(),
                "image/png" | "image/jpeg" | "image/webp"
            )
            || self.size_bytes == 0
            || self.size_bytes > 8 * 1024 * 1024
        {
            return Err(PortableTemplateError::Invalid);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortableTemplate {
    name: String,
    document: DocumentV2,
    media: Vec<TemplateMediaAsset>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PortableTemplateWire {
    format: String,
    format_version: u32,
    name: String,
    document: Value,
    media: Vec<TemplateMediaAsset>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortableTemplateError {
    Invalid,
    UnsupportedFormat,
    UnsupportedVersion,
    MediaMismatch,
}

impl PortableTemplateError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Invalid | Self::UnsupportedFormat => "validation/template_invalid",
            Self::UnsupportedVersion => "validation/template_version_unsupported",
            Self::MediaMismatch => "validation/template_media_invalid",
        }
    }
}

impl PortableTemplate {
    pub fn from_parts(
        name: impl Into<String>,
        document: DocumentV2,
        mut media: Vec<TemplateMediaAsset>,
    ) -> Result<Self, PortableTemplateError> {
        let name = name.into();
        validate_name(&name)?;
        media.sort_by(|left, right| left.asset_id.cmp(&right.asset_id));
        if media.len() > MAX_TEMPLATE_MEDIA_ASSETS {
            return Err(PortableTemplateError::Invalid);
        }
        for asset in &media {
            asset.validate()?;
        }
        if media
            .windows(2)
            .any(|pair| pair[0].asset_id == pair[1].asset_id)
            || media_asset_ids(&media) != referenced_asset_ids(&document)
        {
            return Err(PortableTemplateError::MediaMismatch);
        }
        Ok(Self {
            name,
            document,
            media,
        })
    }

    pub fn decode(raw: &[u8]) -> Result<Self, PortableTemplateError> {
        if raw.is_empty() || raw.len() > MAX_PORTABLE_TEMPLATE_BYTES {
            return Err(PortableTemplateError::Invalid);
        }
        super::validate::check_document_depth(raw, MAX_DOCUMENT_DEPTH + 2)
            .map_err(|_| PortableTemplateError::Invalid)?;
        super::validate::check_unique_object_keys(raw)
            .map_err(|_| PortableTemplateError::Invalid)?;
        let wire: PortableTemplateWire =
            serde_json::from_slice(raw).map_err(|_| PortableTemplateError::Invalid)?;
        if wire.format != PORTABLE_TEMPLATE_FORMAT {
            return Err(PortableTemplateError::UnsupportedFormat);
        }
        if wire.format_version != PORTABLE_TEMPLATE_VERSION {
            return Err(PortableTemplateError::UnsupportedVersion);
        }
        let funnel_id = wire
            .document
            .get("funnel_id")
            .and_then(Value::as_str)
            .ok_or(PortableTemplateError::Invalid)?;
        let document_json =
            serde_json::to_vec(&wire.document).map_err(|_| PortableTemplateError::Invalid)?;
        if document_json.len() > MAX_RAW_DOCUMENT_BYTES {
            return Err(PortableTemplateError::Invalid);
        }
        let document = DocumentV2::decode(&document_json, funnel_id)
            .map_err(|_| PortableTemplateError::Invalid)?;
        Self::from_parts(wire.name, document, wire.media)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn document(&self) -> &DocumentV2 {
        &self.document
    }

    pub fn media(&self) -> &[TemplateMediaAsset] {
        &self.media
    }

    pub fn encode(&self) -> Result<Vec<u8>, PortableTemplateError> {
        let document = std::str::from_utf8(self.document.canonical_bytes())
            .map_err(|_| PortableTemplateError::Invalid)?;
        let name = serde_json::to_string(&self.name).map_err(|_| PortableTemplateError::Invalid)?;
        let media =
            serde_json::to_string(&self.media).map_err(|_| PortableTemplateError::Invalid)?;
        let encoded = format!(
            "{{\"format\":\"{PORTABLE_TEMPLATE_FORMAT}\",\"format_version\":{PORTABLE_TEMPLATE_VERSION},\"name\":{name},\"document\":{document},\"media\":{media}}}"
        )
        .into_bytes();
        if encoded.len() > MAX_PORTABLE_TEMPLATE_BYTES {
            return Err(PortableTemplateError::Invalid);
        }
        Ok(encoded)
    }

    pub fn sha256(&self) -> Result<String, PortableTemplateError> {
        let digest = Sha256::digest(self.encode()?);
        let mut encoded = String::with_capacity(64);
        for byte in digest {
            encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        Ok(encoded)
    }

    pub fn instantiate(
        &self,
        new_funnel_id: &str,
        asset_replacements: &BTreeMap<String, String>,
        ids: &mut impl IdSource,
    ) -> Result<DocumentV2, PortableTemplateError> {
        self.document
            .remap_for_import(new_funnel_id, asset_replacements, ids)
            .map_err(|_| PortableTemplateError::Invalid)
    }
}

const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

impl DocumentV2 {
    pub fn remap_for_import(
        &self,
        new_funnel_id: &str,
        asset_replacements: &BTreeMap<String, String>,
        ids: &mut impl IdSource,
    ) -> Result<Self, DocumentError> {
        let mut data = self.data.clone();
        data.funnel_id = super::model::V2FunnelId::parse(new_funnel_id.to_owned())?;
        if data.funnel_id == self.data.funnel_id {
            return Err(DocumentError::new("validation/import_funnel_id_unchanged"));
        }

        let page_ids = remap_ids(
            data.pages.iter().map(|page| page.id.clone()),
            IdKind::Page,
            ids,
        )?;
        let section_ids = remap_ids(
            data.pages
                .iter()
                .flat_map(|page| page.sections.iter().map(|section| section.id.clone())),
            IdKind::Section,
            ids,
        )?;
        let block_ids = remap_ids(
            data.pages.iter().flat_map(|page| {
                page.sections
                    .iter()
                    .flat_map(|section| section.blocks.iter().map(|block| block.id().clone()))
            }),
            IdKind::Block,
            ids,
        )?;
        let form_ids = remap_ids(
            data.forms.iter().map(|form| form.id.clone()),
            IdKind::Form,
            ids,
        )?;
        let field_ids = remap_ids(
            data.forms
                .iter()
                .flat_map(|form| form.fields.iter().map(|field| field.id().clone())),
            IdKind::Field,
            ids,
        )?;
        let mut consent_versions = BTreeMap::new();
        let mut used_consent_ids = data
            .forms
            .iter()
            .flat_map(|form| {
                std::iter::once(&form.request_consent).chain(form.marketing_opt_in.iter())
            })
            .map(|consent| consent.copy_version.as_str().to_owned())
            .collect::<BTreeSet<_>>();
        for form in &data.forms {
            for consent in
                std::iter::once(&form.request_consent).chain(form.marketing_opt_in.iter())
            {
                if !consent_versions.contains_key(&consent.copy_version) {
                    let value = allocate_id(ids, IdKind::ConsentCopy, &mut used_consent_ids)?;
                    consent_versions.insert(
                        consent.copy_version.clone(),
                        StableDocumentId::parse(value)?,
                    );
                }
            }
        }

        let asset_ids = resolve_asset_replacements(self, asset_replacements)?;
        data.entry_page_id = mapped(&page_ids, &data.entry_page_id)?;
        for page in &mut data.pages {
            page.id = mapped(&page_ids, &page.id)?;
            for section in &mut page.sections {
                section.id = mapped(&section_ids, &section.id)?;
                for block in &mut section.blocks {
                    remap_block(
                        block,
                        &page_ids,
                        &section_ids,
                        &block_ids,
                        &form_ids,
                        &asset_ids,
                    )?;
                }
            }
        }
        for form in &mut data.forms {
            form.id = mapped(&form_ids, &form.id)?;
            for field in &mut form.fields {
                remap_field(field, &field_ids)?;
            }
            form.result = match &form.result {
                FormResult::Inline { message } => FormResult::Inline {
                    message: message.clone(),
                },
                FormResult::SuccessPage { page_id } => FormResult::SuccessPage {
                    page_id: mapped(&page_ids, page_id)?,
                },
            };
            form.request_consent.copy_version =
                mapped(&consent_versions, &form.request_consent.copy_version)?;
            if let Some(consent) = &mut form.marketing_opt_in {
                consent.copy_version = mapped(&consent_versions, &consent.copy_version)?;
            }
        }
        for marker in &mut data.generated_placeholders {
            marker.target = match &marker.target {
                super::GeneratedPlaceholderTarget::PageField { page_id, field } => {
                    super::GeneratedPlaceholderTarget::PageField {
                        page_id: mapped(&page_ids, page_id)?,
                        field: *field,
                    }
                }
                super::GeneratedPlaceholderTarget::Block {
                    page_id,
                    section_id,
                    block_id,
                } => super::GeneratedPlaceholderTarget::Block {
                    page_id: mapped(&page_ids, page_id)?,
                    section_id: mapped(&section_ids, section_id)?,
                    block_id: mapped(&block_ids, block_id)?,
                },
            };
        }
        Self::from_validated(data)
    }
}

fn validate_name(name: &str) -> Result<(), PortableTemplateError> {
    if name.is_empty() || name.trim() != name || name.len() > MAX_TEMPLATE_NAME_BYTES {
        return Err(PortableTemplateError::Invalid);
    }
    Ok(())
}

fn referenced_asset_ids(document: &DocumentV2) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for page in document.pages() {
        for section in page.sections() {
            for block in section.blocks() {
                match block {
                    BlockV2::Image { asset_id, .. } => {
                        ids.insert(asset_id.as_str().to_owned());
                    }
                    BlockV2::Evidence {
                        image: Some(image), ..
                    } => {
                        ids.insert(image.asset_id.as_str().to_owned());
                    }
                    _ => {}
                }
            }
        }
    }
    ids
}

fn media_asset_ids(media: &[TemplateMediaAsset]) -> BTreeSet<String> {
    media
        .iter()
        .map(|asset| asset.asset_id.as_str().to_owned())
        .collect()
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn remap_ids<I, T>(
    source: I,
    kind: IdKind,
    ids: &mut impl IdSource,
) -> Result<BTreeMap<T, T>, DocumentError>
where
    I: IntoIterator<Item = T>,
    T: Clone + Ord + AsStableId + FromStableId,
{
    let source = source.into_iter().collect::<BTreeSet<_>>();
    let mut used = source
        .iter()
        .map(|id| id.as_stable_id().to_owned())
        .collect::<BTreeSet<_>>();
    source
        .into_iter()
        .map(|id| {
            let value = allocate_id(ids, kind, &mut used)?;
            Ok((id, T::from_stable_id(value)?))
        })
        .collect()
}

trait AsStableId {
    fn as_stable_id(&self) -> &str;
}

trait FromStableId: Sized {
    fn from_stable_id(value: String) -> Result<Self, DocumentError>;
}

macro_rules! stable_id_traits {
    ($($id:ty),+ $(,)?) => {
        $(
            impl AsStableId for $id {
                fn as_stable_id(&self) -> &str { self.as_str() }
            }
            impl FromStableId for $id {
                fn from_stable_id(value: String) -> Result<Self, DocumentError> { Self::parse(value) }
            }
        )+
    };
}

stable_id_traits!(PageId, SectionId, BlockId, FormId, FieldId);

fn allocate_id(
    ids: &mut impl IdSource,
    kind: IdKind,
    used: &mut BTreeSet<String>,
) -> Result<String, DocumentError> {
    let value = ids.next_id(kind)?;
    if !is_valid_stable_id(&value) || !used.insert(value.clone()) {
        return Err(DocumentError::at("validation/new_id_not_fresh", "id"));
    }
    Ok(value)
}

fn mapped<T: Ord + Clone>(map: &BTreeMap<T, T>, source: &T) -> Result<T, DocumentError> {
    map.get(source)
        .cloned()
        .ok_or_else(|| DocumentError::new("internal/import_reference_missing"))
}

fn mapped_asset(map: &BTreeMap<String, AssetId>, source: &str) -> Result<AssetId, DocumentError> {
    map.get(source)
        .cloned()
        .ok_or_else(|| DocumentError::new("internal/import_reference_missing"))
}

fn resolve_asset_replacements(
    document: &DocumentV2,
    replacements: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, AssetId>, DocumentError> {
    let referenced = referenced_asset_ids(document);
    if replacements
        .keys()
        .any(|source| !referenced.contains(source))
    {
        return Err(DocumentError::new(
            "validation/import_asset_mapping_invalid",
        ));
    }
    referenced
        .into_iter()
        .map(|source| {
            let target = replacements.get(&source).unwrap_or(&source).clone();
            let target = AssetId::parse(target)
                .map_err(|_| DocumentError::new("validation/import_asset_mapping_invalid"))?;
            if !is_lower_sha256(target.as_str()) {
                return Err(DocumentError::new(
                    "validation/import_asset_mapping_invalid",
                ));
            }
            Ok((source, target))
        })
        .collect()
}

fn remap_block(
    block: &mut BlockV2,
    pages: &BTreeMap<PageId, PageId>,
    sections: &BTreeMap<SectionId, SectionId>,
    blocks: &BTreeMap<BlockId, BlockId>,
    forms: &BTreeMap<FormId, FormId>,
    assets: &BTreeMap<String, AssetId>,
) -> Result<(), DocumentError> {
    let id = mapped(blocks, block.id())?;
    match block {
        BlockV2::Heading { id: target, .. }
        | BlockV2::Text { id: target, .. }
        | BlockV2::List { id: target, .. }
        | BlockV2::Faq { id: target, .. }
        | BlockV2::Steps { id: target, .. }
        | BlockV2::OfferCard { id: target, .. } => *target = id,
        BlockV2::Button {
            id: target, action, ..
        } => {
            *target = id;
            match action {
                ActionV2::NavigatePage { page_id } => *page_id = mapped(pages, page_id)?,
                ActionV2::NavigateSection { section_id } => {
                    *section_id = mapped(sections, section_id)?
                }
                ActionV2::ExternalHttps { .. }
                | ActionV2::Telephone { .. }
                | ActionV2::Email { .. } => {}
            }
        }
        BlockV2::Image {
            id: target,
            asset_id,
            ..
        } => {
            *target = id;
            *asset_id = mapped_asset(assets, asset_id.as_str())?;
        }
        BlockV2::Form {
            id: target,
            form_id,
        } => {
            *target = id;
            *form_id = mapped(forms, form_id)?;
        }
        BlockV2::OfferComparison {
            id: target,
            offer_ids,
        } => {
            *target = id;
            for offer_id in offer_ids {
                *offer_id = mapped(blocks, offer_id)?;
            }
        }
        BlockV2::Evidence {
            id: target, image, ..
        } => {
            *target = id;
            if let Some(EvidenceImage { asset_id, .. }) = image {
                *asset_id = mapped_asset(assets, asset_id.as_str())?;
            }
        }
    }
    Ok(())
}

fn remap_field(
    field: &mut FormField,
    fields: &BTreeMap<FieldId, FieldId>,
) -> Result<(), DocumentError> {
    match field {
        FormField::Text { id, .. }
        | FormField::Email { id, .. }
        | FormField::Phone { id, .. }
        | FormField::Checkbox { id, .. }
        | FormField::Select { id, .. } => *id = mapped(fields, id)?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{PortableTemplate, PortableTemplateError, TemplateMediaAsset};
    use crate::document_v2::{
        ActionV2, AssetId, BlockV2, BlueprintId, DocumentV2, Edit, EvidenceImage, FocusTarget,
        FormResult, IdKind, IdSource, ImageAlt, NewBlock, PublicLanguage,
    };
    use crate::ThemePreset;
    use serde_json::Value;
    use std::collections::BTreeMap;

    struct ImportIds(u32);

    impl IdSource for ImportIds {
        fn next_id(&mut self, kind: IdKind) -> Result<String, crate::document_v2::DocumentError> {
            self.0 += 1;
            Ok(format!("import-{}-{}", kind_label(kind), self.0))
        }
    }

    fn kind_label(kind: IdKind) -> &'static str {
        match kind {
            IdKind::Page => "page",
            IdKind::Section => "section",
            IdKind::Block => "block",
            IdKind::Form => "form",
            IdKind::Field => "field",
            IdKind::ConsentCopy => "consent",
        }
    }

    fn starter() -> DocumentV2 {
        DocumentV2::starter_with_language(
            "source-funnel",
            BlueprintId::LeadGen,
            ThemePreset::Minimal,
            Some(PublicLanguage::French),
        )
        .expect("starter")
    }

    #[test]
    fn portable_template_round_trips_versioned_document_and_exact_empty_manifest() {
        let document = starter();
        let package = PortableTemplate::from_parts("Service de départ", document.clone(), vec![])
            .expect("template");
        let encoded = package.encode().expect("encoded template");
        let decoded = PortableTemplate::decode(&encoded).expect("decoded template");

        assert_eq!(decoded.name(), "Service de départ");
        assert_eq!(
            decoded.document().canonical_bytes(),
            document.canonical_bytes()
        );
        let json: Value = serde_json::from_slice(&encoded).expect("JSON template");
        assert_eq!(json["format"], "eezfunnel.funnel-template");
        assert_eq!(json["format_version"], 1);
        assert!(json.get("submissions").is_none());
        assert!(json.get("brief").is_none());
        assert!(json.get("secrets").is_none());
    }

    #[test]
    fn manifest_must_match_every_referenced_image_and_evidence_asset() {
        let document_json = r#"{
          "document_type":"eezfunnel.funnel-document","document_version":2,"schema_version":2,
          "funnel_id":"source-funnel","blueprint_id":"lead_gen",
          "theme":{"preset_id":"minimal","overrides":{}},"entry_page_id":"page-home",
          "pages":[{"id":"page-home","slug":"","title":"Home","noindex":false,
            "sections":[{"id":"section-hero","type":"hero","blocks":[
              {"id":"block-image","kind":"image","asset_id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","alt":{"kind":"decorative"}},
              {"id":"block-evidence","kind":"evidence","evidence_kind":"testimonial","title":"A demonstration","content":"Example","status":{"kind":"demonstration"},"limitation":"Fictional","image":{"asset_id":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","alt":"Example"}}
            ]}]}],"forms":[]
        }"#;
        let document = DocumentV2::decode(document_json.as_bytes(), "source-funnel").expect("doc");
        let first = TemplateMediaAsset::new("a".repeat(64), "a".repeat(64), "image/png", 12)
            .expect("asset");
        let second = TemplateMediaAsset::new("b".repeat(64), "b".repeat(64), "image/webp", 13)
            .expect("asset");
        let package = PortableTemplate::from_parts(
            "Images",
            document.clone(),
            vec![second.clone(), first.clone()],
        )
        .expect("matching manifest");
        assert_eq!(package.media()[0], first);
        assert_eq!(package.media()[1], second);
        assert_eq!(
            PortableTemplate::from_parts("Images", document.clone(), vec![first]),
            Err(PortableTemplateError::MediaMismatch)
        );
        assert_eq!(
            PortableTemplate::from_parts("Images", document, vec![second.clone(), second]),
            Err(PortableTemplateError::MediaMismatch)
        );
    }

    #[test]
    fn import_rekeys_graph_references_and_asset_replacements_without_mutating_source() {
        let mut ids = ImportIds(0);
        let mut document = starter();
        let home_page = document.pages()[0].id().clone();
        let home_section = document.pages()[0].sections()[0].id().clone();
        let source_success_page = document.pages()[1].id().clone();
        let first_asset = "a".repeat(64);
        let second_asset = "b".repeat(64);
        let first_replacement = "c".repeat(64);
        let second_replacement = "d".repeat(64);
        let first_asset_id = AssetId::parse(first_asset.clone()).expect("asset id");
        let second_asset_id = AssetId::parse(second_asset.clone()).expect("asset id");

        for block in [
            NewBlock::Image {
                asset_id: first_asset_id,
                alt: ImageAlt::Decorative,
            },
            NewBlock::Evidence {
                evidence_kind: crate::document_v2::EvidenceKind::Testimonial,
                title: "Demonstration".to_owned(),
                content: "Fictional example".to_owned(),
                status: crate::document_v2::EvidenceStatus::Demonstration,
                limitation: "Not customer evidence".to_owned(),
                image: Some(EvidenceImage {
                    asset_id: second_asset_id,
                    alt: "Illustration".to_owned(),
                }),
            },
            NewBlock::Button {
                label: "Skip to details".to_owned(),
                action: ActionV2::NavigateSection {
                    section_id: home_section.clone(),
                },
            },
        ] {
            document = document
                .apply(
                    Edit::AddBlock {
                        section: home_section.clone(),
                        after: None,
                        block,
                    },
                    &mut ids,
                )
                .expect("valid block edit")
                .document;
        }
        document = document
            .apply(
                Edit::AddBlock {
                    section: home_section.clone(),
                    after: None,
                    block: NewBlock::Button {
                        label: "Continue".to_owned(),
                        action: ActionV2::NavigatePage {
                            page_id: source_success_page.clone(),
                        },
                    },
                },
                &mut ids,
            )
            .expect("valid page action")
            .document;
        let first = document
            .apply(
                Edit::AddBlock {
                    section: home_section.clone(),
                    after: None,
                    block: NewBlock::OfferCard {
                        title: "Standard".to_owned(),
                        scope: "One service".to_owned(),
                        price: None,
                        conditions: "By agreement".to_owned(),
                    },
                },
                &mut ids,
            )
            .expect("first offer");
        let offer_one_id = match first.focus {
            FocusTarget::Block(id) => id,
            _ => panic!("offer card focus"),
        };
        document = first.document;
        let second = document
            .apply(
                Edit::AddBlock {
                    section: home_section.clone(),
                    after: None,
                    block: NewBlock::OfferCard {
                        title: "Extended".to_owned(),
                        scope: "Two services".to_owned(),
                        price: None,
                        conditions: "By agreement".to_owned(),
                    },
                },
                &mut ids,
            )
            .expect("second offer");
        let offer_two_id = match second.focus {
            FocusTarget::Block(id) => id,
            _ => panic!("offer card focus"),
        };
        document = second.document;
        document = document
            .apply(
                Edit::AddBlock {
                    section: home_section.clone(),
                    after: None,
                    block: NewBlock::OfferComparison {
                        offer_ids: vec![offer_one_id, offer_two_id],
                    },
                },
                &mut ids,
            )
            .expect("offer comparison")
            .document;
        let original = document.canonical_bytes().to_vec();
        let package = PortableTemplate::from_parts(
            "Service",
            document.clone(),
            vec![
                TemplateMediaAsset::new(first_asset.clone(), first_asset.clone(), "image/png", 12)
                    .expect("media"),
                TemplateMediaAsset::new(
                    second_asset.clone(),
                    second_asset.clone(),
                    "image/webp",
                    14,
                )
                .expect("media"),
            ],
        )
        .expect("portable package");
        let replacements = BTreeMap::from([
            (first_asset, first_replacement.clone()),
            (second_asset, second_replacement.clone()),
        ]);
        let imported = package
            .instantiate("new-funnel", &replacements, &mut ImportIds(0))
            .expect("imported document");

        assert_eq!(imported.funnel_id().as_str(), "new-funnel");
        assert_ne!(imported.entry_page_id(), document.entry_page_id());
        assert_eq!(document.canonical_bytes(), original);
        let mapped_home = match imported
            .pages()
            .iter()
            .find(|page| page.id() == imported.entry_page_id())
        {
            Some(page) => page,
            None => panic!("imported entry page"),
        };
        assert_ne!(mapped_home.id(), &home_page);
        let mapped_section = mapped_home.sections()[0].id();
        assert_ne!(mapped_section, &home_section);
        let home = imported
            .page(imported.entry_page_id())
            .expect("new entry page");
        let link = home
            .sections()
            .iter()
            .flat_map(|section| section.blocks())
            .find_map(|block| match block {
                BlockV2::Button {
                    action: ActionV2::NavigatePage { page_id },
                    ..
                } => Some(page_id),
                _ => None,
            })
            .expect("page navigation");
        assert!(imported.page(link).is_some());
        assert!(home
            .sections()
            .iter()
            .flat_map(|section| section.blocks())
            .any(|block| matches!(block, BlockV2::Button { action: ActionV2::NavigateSection { section_id }, .. } if imported.section(section_id).is_some())));
        let image = home
            .sections()
            .iter()
            .flat_map(|section| section.blocks())
            .find_map(|block| match block {
                BlockV2::Image { asset_id, .. } => Some(asset_id.as_str()),
                _ => None,
            })
            .expect("image");
        assert_eq!(image, first_replacement);
        let evidence_image = home
            .sections()
            .iter()
            .flat_map(|section| section.blocks())
            .find_map(|block| match block {
                BlockV2::Evidence {
                    image: Some(image), ..
                } => Some(image.asset_id.as_str()),
                _ => None,
            })
            .expect("evidence image");
        assert_eq!(evidence_image, second_replacement);
        let form_block = home
            .sections()
            .iter()
            .flat_map(|section| section.blocks())
            .find_map(|block| match block {
                BlockV2::Form { form_id, .. } => Some(form_id),
                _ => None,
            })
            .expect("form block");
        assert_eq!(form_block, imported.forms()[0].id());
        assert!(matches!(
            imported.forms()[0].result(),
            FormResult::SuccessPage { page_id } if imported.page(page_id).is_some()
        ));
        let comparison = home
            .sections()
            .iter()
            .flat_map(|section| section.blocks())
            .find_map(|block| match block {
                BlockV2::OfferComparison { offer_ids, .. } => Some(offer_ids),
                _ => None,
            })
            .expect("comparison");
        assert_eq!(comparison.len(), 2);
        assert!(comparison
            .iter()
            .all(|offer_id| matches!(imported.block(offer_id), Some(BlockV2::OfferCard { .. }))));
        assert_ne!(
            imported.forms()[0].request_consent.copy_version(),
            document.forms()[0].request_consent.copy_version()
        );
        assert!(imported
            .generated_placeholders()
            .iter()
            .all(|marker| match &marker.target {
                super::super::GeneratedPlaceholderTarget::PageField { page_id, .. } =>
                    imported.page(page_id).is_some(),
                super::super::GeneratedPlaceholderTarget::Block {
                    page_id,
                    section_id,
                    block_id,
                } => {
                    imported.page(page_id).is_some()
                        && imported.section(section_id).is_some()
                        && imported.block(block_id).is_some()
                }
            }));
        assert_ne!(imported.pages()[0].id(), &home_page);
    }

    #[test]
    fn decoder_rejects_unknown_fields_duplicate_keys_and_future_versions() {
        let package = PortableTemplate::from_parts("Service", starter(), vec![]).expect("template");
        let encoded = String::from_utf8(package.encode().expect("encode")).expect("utf8");
        let duplicate = encoded.replacen(
            "\"format_version\":1",
            "\"format_version\":1,\"format_version\":1",
            1,
        );
        assert_eq!(
            PortableTemplate::decode(duplicate.as_bytes()),
            Err(PortableTemplateError::Invalid)
        );
        let future = encoded.replace("\"format_version\":1", "\"format_version\":99");
        assert_eq!(
            PortableTemplate::decode(future.as_bytes()),
            Err(PortableTemplateError::UnsupportedVersion)
        );
        let unknown = encoded.replace("\"name\":", "\"private_brief\":{},\"name\":");
        assert_eq!(
            PortableTemplate::decode(unknown.as_bytes()),
            Err(PortableTemplateError::Invalid)
        );
    }

    #[test]
    fn portable_template_decoder_handles_deterministic_untrusted_inputs_without_panicking() {
        let valid = PortableTemplate::from_parts("Service", starter(), vec![])
            .expect("template")
            .encode()
            .expect("encoded template");
        let mut state = 0x6a09_e667_f3bc_c909_u64;
        for case in 0..10_000 {
            let input = match case % 4 {
                0 => {
                    let mut bytes = valid.clone();
                    for _ in 0..(next_fuzz_value(&mut state) % 12 + 1) {
                        let index = (next_fuzz_value(&mut state) as usize) % bytes.len();
                        bytes[index] = (next_fuzz_value(&mut state) & 0xff) as u8;
                    }
                    bytes
                }
                1 => {
                    let depth = (next_fuzz_value(&mut state) % 96) as usize;
                    format!("{{\"format\":\"x\",\"document\":{}null{}}}", "[".repeat(depth), "]".repeat(depth)).into_bytes()
                }
                2 => br#"{"format":"eezfunnel.funnel-template","format":"eezfunnel.funnel-template","format_version":1}"#.to_vec(),
                _ => (0..(next_fuzz_value(&mut state) % 384) as usize)
                    .map(|_| (next_fuzz_value(&mut state) & 0xff) as u8)
                    .collect(),
            };
            let parsed = std::panic::catch_unwind(|| PortableTemplate::decode(&input));
            assert!(parsed.is_ok(), "template decoder panicked for case {case}");
        }
    }

    fn next_fuzz_value(state: &mut u64) -> u64 {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        *state
    }
}
