#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

pub mod document_v2;
pub mod form_completions;
pub mod form_submission_channel;
pub mod form_submission_status;
pub mod interaction_event;
pub mod interaction_report;

pub use form_completions::FormCompletionPeriod;
pub use form_submission_channel::FormSubmissionChannel;
pub use form_submission_status::FormSubmissionStatus;
pub use interaction_report::InteractionReportPeriod;

const MAX_SECTIONS_PER_REVISION: usize = 100;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FunnelId(String);

impl FunnelId {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(DomainError::MissingIdentifier("funnel_id"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RevisionId(String);

impl RevisionId {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(DomainError::MissingIdentifier("revision_id"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SchemaVersion(u32);

impl SchemaVersion {
    pub fn value(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatibilityStatus {
    Supported,
    Deprecated,
    Unsupported,
}

#[derive(Clone, Copy, Debug)]
pub struct CompatibilityRegistry;

#[derive(Clone, Copy)]
struct CompatibilityEntry {
    version: i64,
    status: CompatibilityStatus,
}

const COMPATIBILITY_ENTRIES: [CompatibilityEntry; 2] = [
    CompatibilityEntry {
        version: 1,
        status: CompatibilityStatus::Supported,
    },
    CompatibilityEntry {
        version: 2,
        status: CompatibilityStatus::Supported,
    },
];

impl CompatibilityRegistry {
    pub fn status(version: i64) -> CompatibilityStatus {
        COMPATIBILITY_ENTRIES
            .iter()
            .find(|entry| entry.version == version)
            .map(|entry| entry.status)
            .unwrap_or(CompatibilityStatus::Unsupported)
    }

    pub fn validate(version: Option<i64>) -> Result<SchemaVersion, DomainError> {
        let version = version.ok_or(DomainError::MissingSchemaVersion)?;
        match Self::status(version) {
            CompatibilityStatus::Supported => u32::try_from(version)
                .map(SchemaVersion)
                .map_err(|_| DomainError::UnsupportedSchemaVersion(version)),
            CompatibilityStatus::Deprecated => Err(DomainError::DeprecatedSchemaVersion(version)),
            CompatibilityStatus::Unsupported => Err(DomainError::UnsupportedSchemaVersion(version)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SectionType {
    Hero,
    Features,
    Testimonials,
    Cta,
    Footer,
}

impl SectionType {
    fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "hero" => Ok(Self::Hero),
            "features" => Ok(Self::Features),
            "testimonials" => Ok(Self::Testimonials),
            "cta" => Ok(Self::Cta),
            "footer" => Ok(Self::Footer),
            other => Err(DomainError::UnknownSectionType(other.to_owned())),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hero => "hero",
            Self::Features => "features",
            Self::Testimonials => "testimonials",
            Self::Cta => "cta",
            Self::Footer => "footer",
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SectionId(String);

impl SectionId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Section {
    id: SectionId,
    section_type: SectionType,
    order: usize,
}

impl Section {
    pub fn new(
        id: Option<String>,
        section_type: impl AsRef<str>,
        order: usize,
    ) -> Result<Self, DomainError> {
        let id = id.ok_or(DomainError::SectionMissingId)?;
        if id.trim().is_empty() {
            return Err(DomainError::SectionMissingId);
        }
        Ok(Self {
            id: SectionId(id),
            section_type: SectionType::parse(section_type.as_ref())?,
            order,
        })
    }

    pub fn id(&self) -> &SectionId {
        &self.id
    }

    pub fn section_type(&self) -> SectionType {
        self.section_type
    }

    pub fn order(&self) -> usize {
        self.order
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreset {
    Minimal,
    Bold,
    Elegant,
    Playful,
    Corporate,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FunnelBlueprint {
    Tripwire,
    Vsl,
    Webinar,
    Launch,
    LeadGen,
}

impl FunnelBlueprint {
    pub const ALL: [Self; 5] = [
        Self::Tripwire,
        Self::Vsl,
        Self::Webinar,
        Self::Launch,
        Self::LeadGen,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Tripwire => "tripwire",
            Self::Vsl => "vsl",
            Self::Webinar => "webinar",
            Self::Launch => "launch",
            Self::LeadGen => "lead_gen",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Tripwire => "Tripwire",
            Self::Vsl => "Video sales letter",
            Self::Webinar => "Webinar",
            Self::Launch => "Product launch",
            Self::LeadGen => "Service inquiry",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::Tripwire => "Page outline only; checkout and payment are not included.",
            Self::Vsl => {
                "Video sales letter outline only; video blocks, embeds, and checkout are not included."
            }
            Self::Webinar => "Event page outline; registration and booking are not included.",
            Self::Launch => {
                "Product launch page outline; checkout and campaign automation are not included."
            }
            Self::LeadGen => "Working service inquiry form with an offer page and confirmation.",
        }
    }
}

const THEME_PRESETS: [ThemePreset; 5] = [
    ThemePreset::Minimal,
    ThemePreset::Bold,
    ThemePreset::Elegant,
    ThemePreset::Playful,
    ThemePreset::Corporate,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeToken {
    name: &'static str,
    value: &'static str,
}

impl ThemeToken {
    pub const fn name(&self) -> &'static str {
        self.name
    }

    pub const fn value(&self) -> &'static str {
        self.value
    }
}

const MINIMAL_TOKENS: [ThemeToken; 13] = [
    ThemeToken {
        name: "color-background",
        value: "#f8fafc",
    },
    ThemeToken {
        name: "color-border",
        value: "#e2e8f0",
    },
    ThemeToken {
        name: "color-muted",
        value: "#64748b",
    },
    ThemeToken {
        name: "color-on-primary",
        value: "#ffffff",
    },
    ThemeToken {
        name: "color-primary",
        value: "#334155",
    },
    ThemeToken {
        name: "color-surface",
        value: "#ffffff",
    },
    ThemeToken {
        name: "color-text",
        value: "#1f2937",
    },
    ThemeToken {
        name: "font-family",
        value: "system-ui, sans-serif",
    },
    ThemeToken {
        name: "font-scale",
        value: "1",
    },
    ThemeToken {
        name: "radius-button",
        value: "0.375rem",
    },
    ThemeToken {
        name: "radius-card",
        value: "0.75rem",
    },
    ThemeToken {
        name: "shadow-card",
        value: "0 1px 3px rgba(15, 23, 42, 0.08)",
    },
    ThemeToken {
        name: "spacing-section",
        value: "4rem",
    },
];

const BOLD_TOKENS: [ThemeToken; 13] = [
    ThemeToken {
        name: "color-background",
        value: "#111113",
    },
    ThemeToken {
        name: "color-border",
        value: "#3f3f46",
    },
    ThemeToken {
        name: "color-muted",
        value: "#a1a1aa",
    },
    ThemeToken {
        name: "color-on-primary",
        value: "#1c1917",
    },
    ThemeToken {
        name: "color-primary",
        value: "#f97316",
    },
    ThemeToken {
        name: "color-surface",
        value: "#18181b",
    },
    ThemeToken {
        name: "color-text",
        value: "#fafafa",
    },
    ThemeToken {
        name: "font-family",
        value: "Arial, sans-serif",
    },
    ThemeToken {
        name: "font-scale",
        value: "1.08",
    },
    ThemeToken {
        name: "radius-button",
        value: "0",
    },
    ThemeToken {
        name: "radius-card",
        value: "0",
    },
    ThemeToken {
        name: "shadow-card",
        value: "0 8px 32px rgba(0, 0, 0, 0.32)",
    },
    ThemeToken {
        name: "spacing-section",
        value: "6rem",
    },
];

const ELEGANT_TOKENS: [ThemeToken; 13] = [
    ThemeToken {
        name: "color-background",
        value: "#faf7f2",
    },
    ThemeToken {
        name: "color-border",
        value: "#e8ded1",
    },
    ThemeToken {
        name: "color-muted",
        value: "#7c6f64",
    },
    ThemeToken {
        name: "color-on-primary",
        value: "#fffaf5",
    },
    ThemeToken {
        name: "color-primary",
        value: "#8b5e4c",
    },
    ThemeToken {
        name: "color-surface",
        value: "#fffdf9",
    },
    ThemeToken {
        name: "color-text",
        value: "#3b3028",
    },
    ThemeToken {
        name: "font-family",
        value: "Georgia, serif",
    },
    ThemeToken {
        name: "font-scale",
        value: "1.04",
    },
    ThemeToken {
        name: "radius-button",
        value: "0.75rem",
    },
    ThemeToken {
        name: "radius-card",
        value: "1rem",
    },
    ThemeToken {
        name: "shadow-card",
        value: "0 4px 20px rgba(59, 48, 40, 0.08)",
    },
    ThemeToken {
        name: "spacing-section",
        value: "5rem",
    },
];

const PLAYFUL_TOKENS: [ThemeToken; 13] = [
    ThemeToken {
        name: "color-background",
        value: "#fff7fd",
    },
    ThemeToken {
        name: "color-border",
        value: "#f0d8fb",
    },
    ThemeToken {
        name: "color-muted",
        value: "#715a81",
    },
    ThemeToken {
        name: "color-on-primary",
        value: "#ffffff",
    },
    ThemeToken {
        name: "color-primary",
        value: "#c026d3",
    },
    ThemeToken {
        name: "color-surface",
        value: "#ffffff",
    },
    ThemeToken {
        name: "color-text",
        value: "#38204d",
    },
    ThemeToken {
        name: "font-family",
        value: "system-ui, sans-serif",
    },
    ThemeToken {
        name: "font-scale",
        value: "1.05",
    },
    ThemeToken {
        name: "radius-button",
        value: "1.5rem",
    },
    ThemeToken {
        name: "radius-card",
        value: "1.5rem",
    },
    ThemeToken {
        name: "shadow-card",
        value: "0 8px 24px rgba(192, 38, 211, 0.18)",
    },
    ThemeToken {
        name: "spacing-section",
        value: "4.5rem",
    },
];

const CORPORATE_TOKENS: [ThemeToken; 13] = [
    ThemeToken {
        name: "color-background",
        value: "#f1f5f9",
    },
    ThemeToken {
        name: "color-border",
        value: "#cbd5e1",
    },
    ThemeToken {
        name: "color-muted",
        value: "#475569",
    },
    ThemeToken {
        name: "color-on-primary",
        value: "#ffffff",
    },
    ThemeToken {
        name: "color-primary",
        value: "#2563eb",
    },
    ThemeToken {
        name: "color-surface",
        value: "#ffffff",
    },
    ThemeToken {
        name: "color-text",
        value: "#0f172a",
    },
    ThemeToken {
        name: "font-family",
        value: "Arial, sans-serif",
    },
    ThemeToken {
        name: "font-scale",
        value: "1",
    },
    ThemeToken {
        name: "radius-button",
        value: "0.375rem",
    },
    ThemeToken {
        name: "radius-card",
        value: "0.375rem",
    },
    ThemeToken {
        name: "shadow-card",
        value: "0 2px 8px rgba(15, 23, 42, 0.08)",
    },
    ThemeToken {
        name: "spacing-section",
        value: "4rem",
    },
];

impl ThemePreset {
    pub const fn all() -> &'static [Self; 5] {
        &THEME_PRESETS
    }

    pub fn from_id(value: &str) -> Option<Self> {
        Self::all()
            .iter()
            .copied()
            .find(|preset| preset.id() == value)
    }

    pub const fn id(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Bold => "bold",
            Self::Elegant => "elegant",
            Self::Playful => "playful",
            Self::Corporate => "corporate",
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Minimal => "Minimal",
            Self::Bold => "Bold",
            Self::Elegant => "Elegant",
            Self::Playful => "Playful",
            Self::Corporate => "Corporate",
        }
    }

    pub const fn tokens(self) -> &'static [ThemeToken] {
        match self {
            Self::Minimal => &MINIMAL_TOKENS,
            Self::Bold => &BOLD_TOKENS,
            Self::Elegant => &ELEGANT_TOKENS,
            Self::Playful => &PLAYFUL_TOKENS,
            Self::Corporate => &CORPORATE_TOKENS,
        }
    }

    pub fn tokens_json(self) -> Result<String, serde_json::Error> {
        let tokens = self
            .tokens()
            .iter()
            .map(|token| (token.name(), token.value()))
            .collect::<std::collections::BTreeMap<_, _>>();
        serde_json::to_string(&tokens)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderManifest {
    revision_id: RevisionId,
    theme_version: String,
    renderer_version: String,
    assets_manifest_hash: String,
    config_hash: String,
}

impl RenderManifest {
    pub fn new(
        revision_id: RevisionId,
        theme_version: impl Into<String>,
        renderer_version: impl Into<String>,
        assets_manifest_hash: impl Into<String>,
        config_hash: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let theme_version = theme_version.into();
        let renderer_version = renderer_version.into();
        let assets_manifest_hash = assets_manifest_hash.into();
        let config_hash = config_hash.into();
        for (field, value) in [
            ("theme_version", theme_version.as_str()),
            ("renderer_version", renderer_version.as_str()),
            ("assets_manifest_hash", assets_manifest_hash.as_str()),
            ("config_hash", config_hash.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(DomainError::MissingIdentifier(field));
            }
        }
        Ok(Self {
            revision_id,
            theme_version,
            renderer_version,
            assets_manifest_hash,
            config_hash,
        })
    }

    pub fn revision_id(&self) -> &RevisionId {
        &self.revision_id
    }

    pub fn theme_version(&self) -> &str {
        &self.theme_version
    }

    pub fn renderer_version(&self) -> &str {
        &self.renderer_version
    }

    pub fn assets_manifest_hash(&self) -> &str {
        &self.assets_manifest_hash
    }

    pub fn config_hash(&self) -> &str {
        &self.config_hash
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Revision {
    id: RevisionId,
    funnel_id: FunnelId,
    schema_version: SchemaVersion,
    sections: Vec<Section>,
    render_manifest: RenderManifest,
}

impl Revision {
    pub fn new(
        id: RevisionId,
        funnel_id: FunnelId,
        schema_version: Option<i64>,
        mut sections: Vec<Section>,
        render_manifest: RenderManifest,
    ) -> Result<Self, DomainError> {
        let schema_version = CompatibilityRegistry::validate(schema_version)?;
        if schema_version.value() != 1 {
            return Err(DomainError::UnsupportedSchemaVersion(i64::from(
                schema_version.value(),
            )));
        }
        if render_manifest.revision_id != id {
            return Err(DomainError::ManifestRevisionMismatch);
        }
        if sections.len() > MAX_SECTIONS_PER_REVISION {
            return Err(DomainError::SectionLimitReached(MAX_SECTIONS_PER_REVISION));
        }

        let mut section_ids = HashSet::with_capacity(sections.len());
        let mut section_orders = HashSet::with_capacity(sections.len());
        for section in &sections {
            if !section_ids.insert(&section.id) {
                return Err(DomainError::DuplicateSectionId(section.id.0.clone()));
            }
            if !section_orders.insert(section.order) {
                return Err(DomainError::DuplicateSectionOrder(section.order));
            }
        }
        sections.sort_by_key(Section::order);

        Ok(Self {
            id,
            funnel_id,
            schema_version,
            sections,
            render_manifest,
        })
    }

    pub fn id(&self) -> &RevisionId {
        &self.id
    }

    pub fn funnel_id(&self) -> &FunnelId {
        &self.funnel_id
    }

    pub fn schema_version(&self) -> SchemaVersion {
        self.schema_version
    }

    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    pub fn render_manifest(&self) -> &RenderManifest {
        &self.render_manifest
    }

    pub fn fork_with_theme(
        &self,
        id: RevisionId,
        theme_version: impl Into<String>,
        renderer_version: impl Into<String>,
    ) -> Result<Self, DomainError> {
        if id == self.id {
            return Err(DomainError::RevisionIdUnchanged);
        }

        let manifest = RenderManifest::new(
            id.clone(),
            theme_version,
            renderer_version,
            self.render_manifest.assets_manifest_hash.clone(),
            self.render_manifest.config_hash.clone(),
        )?;
        Self::new(
            id,
            self.funnel_id.clone(),
            Some(i64::from(self.schema_version.value())),
            self.sections.clone(),
            manifest,
        )
    }
}

#[derive(Debug)]
pub enum DomainError {
    MissingIdentifier(&'static str),
    MissingSchemaVersion,
    UnsupportedSchemaVersion(i64),
    DeprecatedSchemaVersion(i64),
    SectionMissingId,
    UnknownSectionType(String),
    DuplicateSectionId(String),
    DuplicateSectionOrder(usize),
    SectionLimitReached(usize),
    ManifestRevisionMismatch,
    RevisionIdUnchanged,
    RevisionFunnelMismatch,
    PublicationConflict,
    PublicationStateUnavailable,
}

impl Display for DomainError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingIdentifier(field) => {
                write!(formatter, "validation/missing_field: {field}")
            }
            Self::MissingSchemaVersion => formatter.write_str("validation/schema_version_missing"),
            Self::UnsupportedSchemaVersion(version) => {
                write!(
                    formatter,
                    "validation/schema_version_unsupported: {version}"
                )
            }
            Self::DeprecatedSchemaVersion(version) => {
                write!(formatter, "validation/schema_version_deprecated: {version}")
            }
            Self::SectionMissingId => formatter.write_str("validation/section_missing_id"),
            Self::UnknownSectionType(section_type) => {
                write!(formatter, "validation/section_type_unknown: {section_type}")
            }
            Self::DuplicateSectionId(id) => {
                write!(formatter, "validation/section_id_duplicate: {id}")
            }
            Self::DuplicateSectionOrder(order) => {
                write!(formatter, "validation/order_duplicate: {order}")
            }
            Self::SectionLimitReached(limit) => {
                write!(formatter, "validation/section_limit_reached: {limit}")
            }
            Self::ManifestRevisionMismatch => {
                formatter.write_str("validation/manifest_revision_mismatch")
            }
            Self::RevisionIdUnchanged => formatter.write_str("validation/revision_id_unchanged"),
            Self::RevisionFunnelMismatch => {
                formatter.write_str("validation/revision_funnel_mismatch")
            }
            Self::PublicationConflict => formatter.write_str("conflict/publish_stale_revision"),
            Self::PublicationStateUnavailable => formatter.write_str("internal/unexpected"),
        }
    }
}

impl std::error::Error for DomainError {}

pub struct Funnel {
    id: FunnelId,
    name: String,
    slug: String,
    published_revision_id: Mutex<Option<RevisionId>>,
}

impl Funnel {
    pub fn new(
        id: FunnelId,
        name: impl Into<String>,
        slug: impl Into<String>,
    ) -> Result<Self, DomainError> {
        Self::from_state(id, name, slug, None)
    }

    pub fn from_state(
        id: FunnelId,
        name: impl Into<String>,
        slug: impl Into<String>,
        published_revision_id: Option<RevisionId>,
    ) -> Result<Self, DomainError> {
        let name = name.into();
        let slug = slug.into();
        if name.trim().is_empty() {
            return Err(DomainError::MissingIdentifier("name"));
        }
        if slug.trim().is_empty() {
            return Err(DomainError::MissingIdentifier("slug"));
        }
        Ok(Self {
            id,
            name,
            slug,
            published_revision_id: Mutex::new(published_revision_id),
        })
    }

    pub fn id(&self) -> &FunnelId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn slug(&self) -> &str {
        &self.slug
    }

    pub fn published_revision_id(&self) -> Result<Option<RevisionId>, DomainError> {
        self.published_revision_id
            .lock()
            .map(|published| published.clone())
            .map_err(|_| DomainError::PublicationStateUnavailable)
    }

    pub fn publish(
        &self,
        revision: &Revision,
        expected_published_revision_id: Option<&RevisionId>,
    ) -> Result<RenderManifest, DomainError> {
        if revision.funnel_id != self.id {
            return Err(DomainError::RevisionFunnelMismatch);
        }
        let next_revision_id = revision.id.clone();
        let render_manifest = revision.render_manifest.clone();
        let mut published_revision = self
            .published_revision_id
            .lock()
            .map_err(|_| DomainError::PublicationStateUnavailable)?;
        if published_revision.as_ref() != expected_published_revision_id
            || published_revision.as_ref() == Some(&next_revision_id)
        {
            return Err(DomainError::PublicationConflict);
        }
        *published_revision = Some(next_revision_id);
        Ok(render_manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CompatibilityRegistry, Funnel, FunnelId, RenderManifest, Revision, RevisionId, Section,
    };
    use std::sync::{Arc, Barrier};
    use std::thread;

    fn funnel_id(id: &str) -> FunnelId {
        FunnelId::new(id.to_owned()).expect("valid funnel ID")
    }

    fn revision_id(id: &str) -> RevisionId {
        RevisionId::new(id.to_owned()).expect("valid revision ID")
    }

    fn section(id: &str, order: usize) -> Section {
        Section::new(Some(id.to_owned()), "hero", order).expect("valid hero section")
    }

    fn manifest(revision_id: RevisionId) -> RenderManifest {
        RenderManifest::new(
            revision_id,
            "theme-v1".to_owned(),
            "renderer-v1".to_owned(),
            "assets-sha256".to_owned(),
            "config-sha256".to_owned(),
        )
        .expect("valid render manifest")
    }

    fn revision(id_value: &str, funnel_id: &FunnelId, sections: Vec<Section>) -> Revision {
        let id = revision_id(id_value);
        Revision::new(
            id.clone(),
            funnel_id.clone(),
            Some(1),
            sections,
            manifest(id),
        )
        .expect("valid schema v1 revision")
    }

    #[test]
    fn compatibility_registry_accepts_v1_and_rejects_missing_or_unsupported_schema() {
        assert_eq!(
            format!("{:?}", CompatibilityRegistry::status(1)),
            "Supported"
        );
        assert_eq!(
            format!("{:?}", CompatibilityRegistry::status(2)),
            "Supported"
        );
        assert!(CompatibilityRegistry::validate(Some(1)).is_ok());
        assert!(CompatibilityRegistry::validate(Some(2)).is_ok());
        assert_eq!(
            CompatibilityRegistry::validate(None)
                .expect_err("missing schema version must be rejected")
                .to_string(),
            "validation/schema_version_missing"
        );
        for version in [0, -1, 3, i64::MAX] {
            assert_eq!(
                CompatibilityRegistry::validate(Some(version))
                    .expect_err("unsupported schema version must be rejected")
                    .to_string(),
                format!("validation/schema_version_unsupported: {version}")
            );
        }

        let funnel_id = funnel_id("schema-funnel");
        let valid_id = revision_id("schema-v1");
        assert!(Revision::new(
            valid_id.clone(),
            funnel_id.clone(),
            Some(1),
            Vec::new(),
            manifest(valid_id),
        )
        .is_ok());

        let missing_id = revision_id("schema-missing");
        assert!(Revision::new(
            missing_id.clone(),
            funnel_id.clone(),
            None,
            Vec::new(),
            manifest(missing_id),
        )
        .is_err());

        let unsupported_id = revision_id("schema-v2");
        assert_eq!(
            Revision::new(
                unsupported_id.clone(),
                funnel_id,
                Some(2),
                Vec::new(),
                manifest(unsupported_id),
            )
            .expect_err("the V1 revision model must reject V2 data")
            .to_string(),
            "validation/schema_version_unsupported: 2"
        );
    }

    #[test]
    fn section_validation_rejects_unknown_types_missing_ids_and_duplicate_orders() {
        assert_eq!(
            Section::new(Some("unknown-type".to_owned()), "not-a-section", 0,)
                .expect_err("unknown section type must be rejected")
                .to_string(),
            "validation/section_type_unknown: not-a-section"
        );
        assert_eq!(
            Section::new(None, "hero", 0)
                .expect_err("section without an ID must be rejected")
                .to_string(),
            "validation/section_missing_id"
        );

        let funnel_id = funnel_id("duplicate-order-funnel");
        let revision_id = revision_id("duplicate-order-revision");
        assert_eq!(
            Revision::new(
                revision_id.clone(),
                funnel_id,
                Some(1),
                vec![section("first", 0), section("second", 0)],
                manifest(revision_id),
            )
            .expect_err("duplicate section orders must be rejected")
            .to_string(),
            "validation/order_duplicate: 0"
        );
    }

    #[test]
    fn revision_limit_from_c01_config_accepts_100_sections_and_rejects_101() {
        let funnel_id = funnel_id("section-limit-funnel");
        let valid_id = revision_id("section-limit-100");
        let valid_sections = (0..100)
            .map(|order| section(&format!("section-{order}"), order))
            .collect();
        assert!(Revision::new(
            valid_id.clone(),
            funnel_id.clone(),
            Some(1),
            valid_sections,
            manifest(valid_id),
        )
        .is_ok());

        let invalid_id = revision_id("section-limit-101");
        let invalid_sections = (0..101)
            .map(|order| section(&format!("section-{order}"), order))
            .collect();
        assert!(Revision::new(
            invalid_id.clone(),
            funnel_id,
            Some(1),
            invalid_sections,
            manifest(invalid_id),
        )
        .is_err());
    }

    #[test]
    fn revision_snapshot_is_deterministic_and_exposes_only_immutable_accessors() {
        let funnel_id = funnel_id("snapshot-funnel");
        let revision_id = revision_id("snapshot-revision");
        let forward = Revision::new(
            revision_id.clone(),
            funnel_id.clone(),
            Some(1),
            vec![section("first", 0), section("second", 1)],
            manifest(revision_id.clone()),
        )
        .expect("valid revision in input order");
        let reversed = Revision::new(
            revision_id.clone(),
            funnel_id,
            Some(1),
            vec![section("second", 1), section("first", 0)],
            manifest(revision_id.clone()),
        )
        .expect("valid revision in reverse input order");

        assert_eq!(forward, reversed);
        assert_eq!(forward.id(), &revision_id);
        assert_eq!(forward.schema_version().value(), 1);
        assert_eq!(forward.render_manifest().revision_id(), &revision_id);
        assert_eq!(forward.render_manifest().theme_version(), "theme-v1");
        assert_eq!(forward.render_manifest().renderer_version(), "renderer-v1");
        assert_eq!(
            forward.render_manifest().assets_manifest_hash(),
            "assets-sha256"
        );
        assert_eq!(forward.render_manifest().config_hash(), "config-sha256");
        assert_eq!(forward.sections(), reversed.sections());
        assert_eq!(forward.sections()[0].order(), 0);
        assert_eq!(forward.sections()[1].order(), 1);
        assert_eq!(forward.sections()[0].id().as_str(), "first");

        let snapshot = forward.clone();
        assert_eq!(forward, snapshot);
    }

    #[test]
    fn revision_requires_a_manifest_for_its_own_id() {
        let funnel_id = funnel_id("manifest-funnel");
        let revision_id_value = revision_id("manifest-revision");
        let other_revision_id = revision_id("other-manifest-revision");

        assert_eq!(
            Revision::new(
                revision_id_value,
                funnel_id,
                Some(1),
                Vec::new(),
                manifest(other_revision_id),
            )
            .expect_err("a revision cannot carry another revision's manifest")
            .to_string(),
            "validation/manifest_revision_mismatch"
        );
    }

    #[test]
    fn publication_state_errors_do_not_expose_implementation_details() {
        assert_eq!(
            super::DomainError::PublicationStateUnavailable.to_string(),
            "internal/unexpected"
        );
    }

    #[test]
    fn failed_publish_preserves_the_current_pointer() {
        let funnel_id = funnel_id("publish-funnel");
        let funnel = Funnel::new(
            funnel_id.clone(),
            "Publish funnel".to_owned(),
            "publish-funnel".to_owned(),
        )
        .expect("valid funnel");
        let baseline = revision("published-baseline", &funnel_id, Vec::new());
        assert!(funnel.publish(&baseline, None).is_ok());
        assert_eq!(
            funnel
                .published_revision_id()
                .expect("read published revision"),
            Some(revision_id("published-baseline"))
        );

        assert_eq!(
            funnel
                .publish(&baseline, Some(&revision_id("published-baseline")))
                .expect_err("re-publishing the current revision is a no-op conflict")
                .to_string(),
            "conflict/publish_stale_revision"
        );
        assert_eq!(
            funnel
                .published_revision_id()
                .expect("read published revision after no-op rejection"),
            Some(revision_id("published-baseline"))
        );

        let foreign_funnel = Funnel::new(
            FunnelId::new("foreign-funnel").expect("valid foreign funnel ID"),
            "Foreign funnel".to_owned(),
            "foreign-funnel".to_owned(),
        )
        .expect("valid foreign funnel");
        let foreign_revision = revision("foreign-revision", foreign_funnel.id(), Vec::new());
        assert_eq!(
            funnel
                .publish(&foreign_revision, Some(&revision_id("published-baseline")))
                .expect_err("a funnel cannot publish another funnel's revision")
                .to_string(),
            "validation/revision_funnel_mismatch"
        );
        assert_eq!(
            funnel
                .published_revision_id()
                .expect("read published revision after foreign rejection"),
            Some(revision_id("published-baseline"))
        );

        let candidate = revision("rejected-candidate", &funnel_id, Vec::new());
        let stale_expected = revision_id("stale-baseline");
        assert!(funnel.publish(&candidate, Some(&stale_expected)).is_err());
        assert_eq!(
            funnel
                .published_revision_id()
                .expect("read published revision"),
            Some(revision_id("published-baseline"))
        );
    }

    #[test]
    fn publish_succeeds_from_a_hydrated_published_baseline() {
        let funnel_id = funnel_id("hydrated-publish-funnel");
        let baseline_id = revision_id("persisted-baseline");
        let funnel = Funnel::from_state(
            funnel_id.clone(),
            "Hydrated funnel".to_owned(),
            "hydrated-funnel".to_owned(),
            Some(baseline_id.clone()),
        )
        .expect("valid funnel state loaded from storage");
        let candidate = revision("hydrated-candidate", &funnel_id, Vec::new());

        assert_eq!(
            funnel
                .publish(&candidate, Some(&baseline_id))
                .expect("expected published baseline is accepted")
                .revision_id(),
            candidate.id()
        );
        assert_eq!(
            funnel
                .published_revision_id()
                .expect("read new published revision"),
            Some(candidate.id().clone())
        );
    }

    #[test]
    fn stale_publish_from_a_hydrated_state_keeps_the_loaded_pointer() {
        let funnel_id = funnel_id("hydrated-stale-funnel");
        let baseline_id = revision_id("persisted-stale-baseline");
        let funnel = Funnel::from_state(
            funnel_id.clone(),
            "Hydrated stale funnel".to_owned(),
            "hydrated-stale-funnel".to_owned(),
            Some(baseline_id.clone()),
        )
        .expect("valid funnel state loaded from storage");
        let candidate = revision("stale-hydrated-candidate", &funnel_id, Vec::new());
        let stale_expected = revision_id("older-baseline");

        assert_eq!(
            funnel
                .publish(&candidate, Some(&stale_expected))
                .expect_err("stale storage baseline must conflict")
                .to_string(),
            "conflict/publish_stale_revision"
        );
        assert_eq!(
            funnel
                .published_revision_id()
                .expect("read persisted baseline after stale rejection"),
            Some(baseline_id)
        );
    }

    #[test]
    fn one_hundred_concurrent_publications_from_one_baseline_have_one_winner() {
        let funnel_id = funnel_id("concurrent-funnel");
        let baseline_id = revision_id("concurrent-baseline");
        let funnel: Arc<Funnel> = Arc::new(
            Funnel::from_state(
                funnel_id.clone(),
                "Concurrent funnel".to_owned(),
                "concurrent-funnel".to_owned(),
                Some(baseline_id.clone()),
            )
            .expect("valid funnel"),
        );

        let candidates: Vec<_> = (0..100)
            .map(|index| {
                revision(
                    &format!("concurrent-candidate-{index}"),
                    &funnel_id,
                    Vec::new(),
                )
            })
            .collect();
        let barrier = Arc::new(Barrier::new(candidates.len() + 1));
        let handles: Vec<_> = candidates
            .into_iter()
            .map(|candidate| {
                let funnel = Arc::clone(&funnel);
                let barrier = Arc::clone(&barrier);
                let expected = baseline_id.clone();
                thread::spawn(move || {
                    barrier.wait();
                    funnel.publish(&candidate, Some(&expected)).is_ok()
                })
            })
            .collect();

        barrier.wait();
        let winners = handles
            .into_iter()
            .map(|handle| usize::from(handle.join().expect("publisher thread completed")))
            .sum::<usize>();
        assert_eq!(winners, 1);

        let published = funnel
            .published_revision_id()
            .expect("read published revision");
        assert!(published.is_some());
    }
}

#[cfg(test)]
mod theme_tests;
