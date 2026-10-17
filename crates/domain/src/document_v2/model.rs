use std::fmt::{Display, Formatter};

use serde::{Deserialize, Deserializer, Serialize};

use super::theme::{ResolvedThemeProfile, ThemeV2};
use super::validate::{build_index, canonicalize, validate_data, DocumentIndex};
use super::{
    DOCUMENT_VERSION_V2, MAX_DOCUMENT_DEPTH, MAX_PERSISTED_DOCUMENT_BYTES, MAX_RAW_DOCUMENT_BYTES,
    STRUCTURAL_SCHEMA_VERSION_V2,
};

pub const DOCUMENT_TYPE_V2: &str = "eezfunnel.funnel-document";

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn parse(value: impl Into<String>) -> Result<Self, DocumentError> {
                let value = value.into();
                if !is_valid_stable_id(&value) {
                    return Err(DocumentError::new("validation/stable_id_invalid"));
                }
                Ok(Self::from_validated(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub(crate) fn from_validated(value: String) -> Self {
                Self(value)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::parse(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

pub(crate) fn is_valid_stable_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

pub(crate) fn deserialize_optional_non_null<'de, D, T>(
    deserializer: D,
) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

id_type!(StableDocumentId);
id_type!(V2FunnelId);
id_type!(RevisionIdV2);
id_type!(PageId);
id_type!(SectionId);
id_type!(BlockId);
id_type!(FormId);
id_type!(FieldId);
id_type!(AssetId);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentError {
    code: &'static str,
    path: Option<String>,
}

impl DocumentError {
    pub(crate) fn new(code: &'static str) -> Self {
        Self { code, path: None }
    }

    pub(crate) fn at(code: &'static str, path: impl Into<String>) -> Self {
        Self {
            code,
            path: Some(path.into()),
        }
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }
}

impl Display for DocumentError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.path {
            Some(path) => write!(formatter, "{}: {path}", self.code),
            None => formatter.write_str(self.code),
        }
    }
}

impl std::error::Error for DocumentError {}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlueprintId {
    Tripwire,
    Vsl,
    Webinar,
    Launch,
    LeadGen,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    Hero,
    Features,
    Testimonials,
    Cta,
    Footer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockKind {
    Heading,
    Text,
    List,
    Button,
    Image,
    Form,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BlockV2 {
    Heading {
        id: BlockId,
        level: u8,
        text: String,
    },
    Text {
        id: BlockId,
        text: String,
    },
    List {
        id: BlockId,
        items: Vec<String>,
    },
    Button {
        id: BlockId,
        label: String,
        action: ActionV2,
    },
    Image {
        id: BlockId,
        asset_id: AssetId,
        alt: ImageAlt,
    },
    Form {
        id: BlockId,
        form_id: FormId,
    },
}

impl BlockV2 {
    pub fn id(&self) -> &BlockId {
        match self {
            Self::Heading { id, .. }
            | Self::Text { id, .. }
            | Self::List { id, .. }
            | Self::Button { id, .. }
            | Self::Image { id, .. }
            | Self::Form { id, .. } => id,
        }
    }

    pub fn kind(&self) -> BlockKind {
        match self {
            Self::Heading { .. } => BlockKind::Heading,
            Self::Text { .. } => BlockKind::Text,
            Self::List { .. } => BlockKind::List,
            Self::Button { .. } => BlockKind::Button,
            Self::Image { .. } => BlockKind::Image,
            Self::Form { .. } => BlockKind::Form,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionV2 {
    NavigatePage { page_id: PageId },
    ExternalHttps { url: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ImageAlt {
    Text { value: String },
    Decorative,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PageV2 {
    pub(crate) id: PageId,
    pub(crate) slug: String,
    pub(crate) title: String,
    pub(crate) sections: Vec<SectionV2>,
}

impl<'de> Deserialize<'de> for PageV2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct PageWire {
            id: PageId,
            slug: String,
            title: String,
            sections: Vec<SectionV2>,
        }

        let wire = PageWire::deserialize(deserializer)?;
        Ok(Self {
            id: wire.id,
            slug: wire.slug,
            title: wire.title,
            sections: wire.sections,
        })
    }
}

impl PageV2 {
    pub fn id(&self) -> &PageId {
        &self.id
    }

    pub fn slug(&self) -> &str {
        &self.slug
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn sections(&self) -> &[SectionV2] {
        &self.sections
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SectionV2 {
    pub(crate) id: SectionId,
    #[serde(rename = "type")]
    pub(crate) section_type: SectionKind,
    pub(crate) blocks: Vec<BlockV2>,
}

impl SectionV2 {
    pub fn id(&self) -> &SectionId {
        &self.id
    }

    pub fn section_type(&self) -> SectionKind {
        self.section_type
    }

    pub fn blocks(&self) -> &[BlockV2] {
        &self.blocks
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FormV2 {
    pub(crate) id: FormId,
    pub(crate) fields: Vec<FormField>,
    pub(crate) purpose: String,
    pub(crate) request_consent: ConsentCopy,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) marketing_opt_in: Option<ConsentCopy>,
    pub(crate) result: FormResult,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConsentCopy {
    pub(crate) copy_version: StableDocumentId,
    pub(crate) copy: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FormResult {
    Inline { message: String },
    SuccessPage { page_id: PageId },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FormField {
    Text {
        id: FieldId,
        name: String,
        label: String,
        #[serde(
            default,
            deserialize_with = "deserialize_optional_non_null",
            skip_serializing_if = "Option::is_none"
        )]
        help: Option<String>,
        required: bool,
        #[serde(
            default,
            deserialize_with = "deserialize_optional_non_null",
            skip_serializing_if = "Option::is_none"
        )]
        validation: Option<ValidationRange>,
    },
    Email {
        id: FieldId,
        name: String,
        label: String,
        #[serde(
            default,
            deserialize_with = "deserialize_optional_non_null",
            skip_serializing_if = "Option::is_none"
        )]
        help: Option<String>,
        required: bool,
        #[serde(
            default,
            deserialize_with = "deserialize_optional_non_null",
            skip_serializing_if = "Option::is_none"
        )]
        validation: Option<ValidationRange>,
    },
    Phone {
        id: FieldId,
        name: String,
        label: String,
        #[serde(
            default,
            deserialize_with = "deserialize_optional_non_null",
            skip_serializing_if = "Option::is_none"
        )]
        help: Option<String>,
        required: bool,
        #[serde(
            default,
            deserialize_with = "deserialize_optional_non_null",
            skip_serializing_if = "Option::is_none"
        )]
        validation: Option<ValidationRange>,
    },
    Checkbox {
        id: FieldId,
        name: String,
        label: String,
        #[serde(
            default,
            deserialize_with = "deserialize_optional_non_null",
            skip_serializing_if = "Option::is_none"
        )]
        help: Option<String>,
        required: bool,
    },
    Select {
        id: FieldId,
        name: String,
        label: String,
        #[serde(
            default,
            deserialize_with = "deserialize_optional_non_null",
            skip_serializing_if = "Option::is_none"
        )]
        help: Option<String>,
        required: bool,
        options: Vec<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationRange {
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub min_length: Option<u32>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_length: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DocumentData {
    pub(crate) document_type: String,
    pub(crate) document_version: u32,
    pub(crate) schema_version: u32,
    pub(crate) funnel_id: V2FunnelId,
    pub(crate) blueprint_id: BlueprintId,
    pub(crate) theme: ThemeV2,
    pub(crate) entry_page_id: PageId,
    pub(crate) pages: Vec<PageV2>,
    pub(crate) forms: Vec<FormV2>,
}

#[derive(Clone, Debug)]
pub struct DocumentV2 {
    pub(crate) data: DocumentData,
    pub(crate) index: DocumentIndex,
    canonical: Vec<u8>,
}

impl PartialEq for DocumentV2 {
    fn eq(&self, other: &Self) -> bool {
        self.canonical == other.canonical
    }
}

impl Eq for DocumentV2 {}

impl DocumentV2 {
    pub fn starter(
        funnel_id: &str,
        blueprint_id: BlueprintId,
        preset: crate::ThemePreset,
    ) -> Result<Self, DocumentError> {
        let funnel_id = V2FunnelId::parse(funnel_id.to_owned())?;
        let page_id = PageId::parse("page-home")?;
        let section_types = match blueprint_id {
            BlueprintId::Tripwire => {
                &[SectionKind::Hero, SectionKind::Features, SectionKind::Cta][..]
            }
            BlueprintId::Vsl => &[
                SectionKind::Hero,
                SectionKind::Testimonials,
                SectionKind::Cta,
            ],
            BlueprintId::Webinar => &[
                SectionKind::Hero,
                SectionKind::Features,
                SectionKind::Testimonials,
                SectionKind::Cta,
            ],
            BlueprintId::Launch => &[
                SectionKind::Hero,
                SectionKind::Features,
                SectionKind::Testimonials,
                SectionKind::Cta,
                SectionKind::Footer,
            ],
            BlueprintId::LeadGen => &[
                SectionKind::Hero,
                SectionKind::Features,
                SectionKind::Cta,
                SectionKind::Footer,
            ],
        };
        let sections = section_types
            .iter()
            .copied()
            .map(starter_section)
            .collect::<Result<Vec<_>, _>>()?;
        let data = build_data_from_parts(
            funnel_id,
            blueprint_id,
            ThemeV2 {
                preset_id: preset,
                overrides: super::theme::ThemeOverrides::default(),
            },
            page_id.clone(),
            vec![PageV2 {
                id: page_id,
                slug: String::new(),
                title: "Home".to_owned(),
                sections,
            }],
            Vec::new(),
        );
        Self::from_validated(data)
    }

    pub fn decode(raw: &[u8], expected_funnel_id: &str) -> Result<Self, DocumentError> {
        if raw.len() > MAX_RAW_DOCUMENT_BYTES {
            return Err(DocumentError::new("validation/document_too_large"));
        }
        super::validate::check_document_depth(raw, MAX_DOCUMENT_DEPTH)?;
        super::validate::check_unique_object_keys(raw)?;
        std::str::from_utf8(raw)
            .map_err(|_| DocumentError::new("validation/document_invalid_utf8"))?;
        let mut data: DocumentData = serde_json::from_slice(raw)
            .map_err(|_| DocumentError::new("validation/document_invalid"))?;
        validate_data(&mut data, expected_funnel_id)?;
        Self::from_validated(data)
    }

    pub(crate) fn from_validated(mut data: DocumentData) -> Result<Self, DocumentError> {
        let expected_funnel_id = data.funnel_id.as_str().to_owned();
        validate_data(&mut data, &expected_funnel_id)?;
        let index = build_index(&data)?;
        let canonical = canonicalize(&data)?;
        if canonical.len() > MAX_PERSISTED_DOCUMENT_BYTES {
            return Err(DocumentError::new("validation/document_too_large"));
        }
        Ok(Self {
            data,
            index,
            canonical,
        })
    }

    pub fn funnel_id(&self) -> &V2FunnelId {
        &self.data.funnel_id
    }

    pub fn blueprint_id(&self) -> BlueprintId {
        self.data.blueprint_id
    }

    pub fn theme(&self) -> &ThemeV2 {
        &self.data.theme
    }

    pub fn pages(&self) -> &[PageV2] {
        &self.data.pages
    }

    pub fn forms(&self) -> &[FormV2] {
        &self.data.forms
    }

    pub fn entry_page_id(&self) -> &PageId {
        &self.data.entry_page_id
    }

    pub fn page(&self, id: &PageId) -> Option<&PageV2> {
        self.index
            .pages
            .get(id)
            .map(|position| &self.data.pages[*position])
    }

    pub fn section(&self, id: &super::SectionId) -> Option<&SectionV2> {
        let (page_index, section_index) = self.index.sections.get(id).copied()?;
        self.data.pages.get(page_index)?.sections.get(section_index)
    }

    pub fn block(&self, id: &super::BlockId) -> Option<&BlockV2> {
        let (page_index, section_index, block_index) = self.index.blocks.get(id).copied()?;
        self.data
            .pages
            .get(page_index)?
            .sections
            .get(section_index)?
            .blocks
            .get(block_index)
    }

    pub fn form(&self, id: &FormId) -> Option<&FormV2> {
        self.index
            .forms
            .get(id)
            .and_then(|position| self.data.forms.get(*position))
    }

    pub fn field(&self, id: &FieldId) -> Option<&FormField> {
        let (form_index, field_index) = self.index.fields.get(id).copied()?;
        self.data.forms.get(form_index)?.fields.get(field_index)
    }

    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }

    pub fn has_images(&self) -> bool {
        self.data.pages.iter().any(|page| {
            page.sections.iter().any(|section| {
                section
                    .blocks
                    .iter()
                    .any(|block| matches!(block, BlockV2::Image { .. }))
            })
        })
    }

    pub fn has_forms(&self) -> bool {
        !self.data.forms.is_empty()
    }

    pub fn resolved_theme_profile(&self) -> ResolvedThemeProfile {
        self.data.theme.resolve()
    }
}

fn starter_section(section_type: SectionKind) -> Result<SectionV2, DocumentError> {
    let (section_id, blocks) = match section_type {
        SectionKind::Hero => (
            "section-hero",
            vec![
                BlockV2::Heading {
                    id: BlockId::parse("block-hero-title")?,
                    level: 1,
                    text: "Welcome to your new funnel".to_owned(),
                },
                BlockV2::Text {
                    id: BlockId::parse("block-hero-copy")?,
                    text: "Replace this text with your offer.".to_owned(),
                },
            ],
        ),
        SectionKind::Features => (
            "section-features",
            vec![
                BlockV2::Heading {
                    id: BlockId::parse("block-features-title")?,
                    level: 2,
                    text: "What makes this offer valuable".to_owned(),
                },
                BlockV2::List {
                    id: BlockId::parse("block-features-list")?,
                    items: vec![
                        "A clear outcome for your customers".to_owned(),
                        "A simple next step to take action".to_owned(),
                    ],
                },
            ],
        ),
        SectionKind::Testimonials => (
            "section-testimonials",
            vec![
                BlockV2::Heading {
                    id: BlockId::parse("block-testimonials-title")?,
                    level: 2,
                    text: "Trusted by customers".to_owned(),
                },
                BlockV2::Text {
                    id: BlockId::parse("block-testimonials-copy")?,
                    text: "Add a genuine customer story to build confidence.".to_owned(),
                },
            ],
        ),
        SectionKind::Cta => (
            "section-cta",
            vec![
                BlockV2::Heading {
                    id: BlockId::parse("block-cta-title")?,
                    level: 2,
                    text: "Ready to get started?".to_owned(),
                },
                BlockV2::Text {
                    id: BlockId::parse("block-cta-copy")?,
                    text: "Take the next step today.".to_owned(),
                },
                BlockV2::Button {
                    id: BlockId::parse("block-cta-button")?,
                    label: "Get started".to_owned(),
                    action: ActionV2::ExternalHttps {
                        url: "https://example.com".to_owned(),
                    },
                },
            ],
        ),
        SectionKind::Footer => (
            "section-footer",
            vec![BlockV2::Text {
                id: BlockId::parse("block-footer-copy")?,
                text: "Your business · All rights reserved.".to_owned(),
            }],
        ),
    };
    Ok(SectionV2 {
        id: SectionId::parse(section_id.to_owned())?,
        section_type,
        blocks,
    })
}

pub(crate) fn build_data_from_parts(
    funnel_id: V2FunnelId,
    blueprint_id: BlueprintId,
    theme: ThemeV2,
    entry_page_id: PageId,
    pages: Vec<PageV2>,
    forms: Vec<FormV2>,
) -> DocumentData {
    DocumentData {
        document_type: DOCUMENT_TYPE_V2.to_owned(),
        document_version: DOCUMENT_VERSION_V2,
        schema_version: STRUCTURAL_SCHEMA_VERSION_V2,
        funnel_id,
        blueprint_id,
        theme,
        entry_page_id,
        pages,
        forms,
    }
}
