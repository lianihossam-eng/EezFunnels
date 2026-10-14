use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

use eez_domain::{
    DomainError, Funnel, FunnelBlueprint, FunnelId, RenderManifest, Revision, RevisionId, Section,
    ThemePreset,
};
use eez_render::{
    content_config_hash, FeatureItem, FooterLink, RenderError, Renderer, SectionContent,
    Testimonial, ThemeTokens, RENDERER_VERSION,
};
use eez_storage::repository::{
    FunnelRecord, MutationResult, NewFunnel, NewRevision, Repository, RepositoryError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_FUNNEL_NAME_BYTES: usize = 120;
const MAX_SNAPSHOT_BYTES: usize = 1_048_576;
const EMPTY_ASSETS_HASH: &str =
    "assets-sha256-v1:8b3972abe1696375524a8237889ff77fa13664876dd51ede33db93795b8d5cca";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FunnelCreateCommand {
    name: String,
    blueprint: FunnelBlueprint,
}

impl FunnelCreateCommand {
    pub const ID: &'static str = "funnel.create";

    pub fn from_json(parameters: &str) -> Result<Self, FunnelCommandError> {
        let value: Value = serde_json::from_str(parameters)?;
        let object = value
            .as_object()
            .ok_or(FunnelCommandError::InvalidParameters)?;
        if object.get("name").is_none() || object.get("blueprint").is_none() {
            return Err(FunnelCommandError::MissingField);
        }
        let mut command: Self = serde_json::from_str(parameters)?;
        command.name = validate_name(&command.name)?.to_owned();
        Ok(command)
    }

    pub fn blueprint(&self) -> FunnelBlueprint {
        self.blueprint
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn prepare(
        &self,
        funnel_id: impl Into<String>,
        revision_id: impl Into<String>,
    ) -> Result<PreparedFunnel, FunnelCommandError> {
        let funnel_id = funnel_id.into();
        let revision_id = revision_id.into();
        validate_route_id(&funnel_id)?;
        validate_route_id(&revision_id)?;

        let domain_funnel_id = FunnelId::new(funnel_id.clone())?;
        let domain_revision_id = RevisionId::new(revision_id.clone())?;
        let slug = slug_from_name_and_id(&self.name, &funnel_id);
        let domain_funnel = Funnel::new(domain_funnel_id.clone(), &self.name, &slug)?;
        let (sections, contents) = blueprint_content(self.blueprint, &self.name)?;
        let theme = ThemeTokens::from_preset(ThemePreset::Minimal)?;
        let manifest = RenderManifest::new(
            domain_revision_id.clone(),
            theme.version(),
            RENDERER_VERSION,
            EMPTY_ASSETS_HASH,
            content_config_hash(&contents),
        )?;
        let revision = Revision::new(
            domain_revision_id,
            domain_funnel_id,
            Some(1),
            sections.clone(),
            manifest.clone(),
        )?;
        Renderer::new().render(&revision, &theme, &contents)?;

        let snapshot = FunnelSnapshotV1::new(self.blueprint, &sections, contents);
        let data_json =
            serde_json::to_string(&snapshot).map_err(|_| FunnelCommandError::Internal)?;
        if data_json.len() > MAX_SNAPSHOT_BYTES {
            return Err(FunnelCommandError::InvalidParameters);
        }
        let render_manifest_json = serde_json::to_string(&RenderManifestV1::from(&manifest))
            .map_err(|_| FunnelCommandError::Internal)?;
        let summary = InitialRevisionSummary::from_sections(&revision);

        Ok(PreparedFunnel {
            funnel: NewFunnel {
                id: funnel_id,
                name: domain_funnel.name().to_owned(),
                slug: domain_funnel.slug().to_owned(),
            },
            revision: NewRevision {
                id: revision.id().as_str().to_owned(),
                funnel_id: revision.funnel_id().as_str().to_owned(),
                schema_version: i64::from(revision.schema_version().value()),
                data_json,
                render_manifest_json,
            },
            summary,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FunnelDeleteCommand {
    funnel_id: String,
}

impl FunnelDeleteCommand {
    pub const ID: &'static str = "funnel.delete";

    pub fn from_json(parameters: &str) -> Result<Self, FunnelCommandError> {
        let value: Value = serde_json::from_str(parameters)?;
        let object = value
            .as_object()
            .ok_or(FunnelCommandError::InvalidParameters)?;
        if object.get("funnel_id").is_none() {
            return Err(FunnelCommandError::MissingField);
        }
        let command: Self = serde_json::from_str(parameters)?;
        validate_route_id(&command.funnel_id)?;
        Ok(command)
    }

    pub fn funnel_id(&self) -> &str {
        &self.funnel_id
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FunnelListCommand {}

impl FunnelListCommand {
    pub const ID: &'static str = "funnel.list";

    pub fn from_json(parameters: &str) -> Result<Self, FunnelCommandError> {
        serde_json::from_str(parameters).map_err(Into::into)
    }
}

#[derive(Serialize)]
struct FunnelListResult {
    command_id: &'static str,
    items: Vec<FunnelRecord>,
}

#[derive(Serialize)]
struct FunnelCreateResult<'a> {
    command_id: &'static str,
    funnel: CreatedFunnel<'a>,
    initial_revision: &'a InitialRevisionSummary,
}

#[derive(Serialize)]
struct CreatedFunnel<'a> {
    id: &'a str,
    name: &'a str,
    slug: &'a str,
    published_revision_id: Option<&'a str>,
}

#[derive(Serialize)]
struct FunnelDeleteResult<'a> {
    command_id: &'static str,
    funnel_id: &'a str,
    deleted: bool,
}

pub struct Session<'repo, 'database> {
    repository: &'repo Repository<'database>,
}

impl<'repo, 'database> Session<'repo, 'database> {
    pub fn new(repository: &'repo Repository<'database>) -> Self {
        Self { repository }
    }

    pub async fn execute(
        &self,
        command_id: &str,
        parameters: &str,
        idempotency_key: Option<&str>,
    ) -> Result<Value, FunnelCommandError> {
        match command_id {
            FunnelListCommand::ID => self.execute_list(parameters).await,
            FunnelCreateCommand::ID => {
                self.execute_create(parameters, required_idempotency_key(idempotency_key)?)
                    .await
            }
            FunnelDeleteCommand::ID => {
                self.execute_delete(parameters, required_idempotency_key(idempotency_key)?)
                    .await
            }
            _ => Err(FunnelCommandError::UnknownCommand),
        }
    }

    async fn execute_list(&self, parameters: &str) -> Result<Value, FunnelCommandError> {
        FunnelListCommand::from_json(parameters)?;
        let result = FunnelListResult {
            command_id: FunnelListCommand::ID,
            items: self.repository.list_funnels().await?,
        };
        serde_json::to_value(result).map_err(|_| FunnelCommandError::Internal)
    }

    async fn execute_create(
        &self,
        parameters: &str,
        idempotency_key: &str,
    ) -> Result<Value, FunnelCommandError> {
        let command = FunnelCreateCommand::from_json(parameters)?;
        let canonical_parameters =
            serde_json::to_string(&command).map_err(|_| FunnelCommandError::Internal)?;
        let prepared = command.prepare(
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
        )?;
        let result = FunnelCreateResult {
            command_id: FunnelCreateCommand::ID,
            funnel: CreatedFunnel {
                id: &prepared.funnel.id,
                name: &prepared.funnel.name,
                slug: &prepared.funnel.slug,
                published_revision_id: None,
            },
            initial_revision: &prepared.summary,
        };
        let result_json =
            serde_json::to_string(&result).map_err(|_| FunnelCommandError::Internal)?;
        match self
            .repository
            .create_funnel_with_revision(
                &prepared.funnel,
                &prepared.revision,
                FunnelCreateCommand::ID,
                idempotency_key,
                &canonical_parameters,
                &result_json,
            )
            .await?
        {
            MutationResult::Stored(result) => {
                serde_json::from_str(&result).map_err(|_| FunnelCommandError::Internal)
            }
            MutationResult::PayloadMismatch => Err(FunnelCommandError::IdempotencyPayloadMismatch),
        }
    }

    async fn execute_delete(
        &self,
        parameters: &str,
        idempotency_key: &str,
    ) -> Result<Value, FunnelCommandError> {
        let command = FunnelDeleteCommand::from_json(parameters)?;
        let canonical_parameters =
            serde_json::to_string(&command).map_err(|_| FunnelCommandError::Internal)?;
        let result = FunnelDeleteResult {
            command_id: FunnelDeleteCommand::ID,
            funnel_id: command.funnel_id(),
            deleted: true,
        };
        let result_json =
            serde_json::to_string(&result).map_err(|_| FunnelCommandError::Internal)?;
        match self
            .repository
            .delete_funnel_idempotently(
                command.funnel_id(),
                FunnelDeleteCommand::ID,
                idempotency_key,
                &canonical_parameters,
                &result_json,
            )
            .await?
        {
            MutationResult::Stored(result) => {
                serde_json::from_str(&result).map_err(|_| FunnelCommandError::Internal)
            }
            MutationResult::PayloadMismatch => Err(FunnelCommandError::IdempotencyPayloadMismatch),
        }
    }
}

pub struct PreparedFunnel {
    funnel: NewFunnel,
    revision: NewRevision,
    summary: InitialRevisionSummary,
}

impl PreparedFunnel {
    pub fn funnel(&self) -> &NewFunnel {
        &self.funnel
    }

    pub fn revision(&self) -> &NewRevision {
        &self.revision
    }

    pub fn summary(&self) -> &InitialRevisionSummary {
        &self.summary
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InitialRevisionSummary {
    id: String,
    schema_version: i64,
    section_ids: Vec<String>,
    section_types: Vec<String>,
}

impl InitialRevisionSummary {
    fn from_sections(revision: &Revision) -> Self {
        Self {
            id: revision.id().as_str().to_owned(),
            schema_version: i64::from(revision.schema_version().value()),
            section_ids: revision
                .sections()
                .iter()
                .map(|section| section.id().as_str().to_owned())
                .collect(),
            section_types: revision
                .sections()
                .iter()
                .map(|section| section.section_type().as_str().to_owned())
                .collect(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn schema_version(&self) -> i64 {
        self.schema_version
    }

    pub fn section_types(&self) -> Vec<&str> {
        self.section_types.iter().map(String::as_str).collect()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FunnelSnapshotV1 {
    format_version: u16,
    schema_version: u32,
    blueprint: FunnelBlueprint,
    theme_id: ThemePreset,
    sections: Vec<SectionSnapshotV1>,
    contents: BTreeMap<String, SectionContent>,
}

impl FunnelSnapshotV1 {
    fn new(
        blueprint: FunnelBlueprint,
        sections: &[Section],
        contents: BTreeMap<String, SectionContent>,
    ) -> Self {
        Self {
            format_version: 1,
            schema_version: 1,
            blueprint,
            theme_id: ThemePreset::Minimal,
            sections: sections.iter().map(SectionSnapshotV1::from).collect(),
            contents,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SectionSnapshotV1 {
    id: String,
    #[serde(rename = "type")]
    section_type: String,
    order: usize,
}

impl From<&Section> for SectionSnapshotV1 {
    fn from(section: &Section) -> Self {
        Self {
            id: section.id().as_str().to_owned(),
            section_type: section.section_type().as_str().to_owned(),
            order: section.order(),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RenderManifestV1 {
    revision_id: String,
    theme_version: String,
    renderer_version: String,
    assets_manifest_hash: String,
    config_hash: String,
}

impl From<&RenderManifest> for RenderManifestV1 {
    fn from(manifest: &RenderManifest) -> Self {
        Self {
            revision_id: manifest.revision_id().as_str().to_owned(),
            theme_version: manifest.theme_version().to_owned(),
            renderer_version: manifest.renderer_version().to_owned(),
            assets_manifest_hash: manifest.assets_manifest_hash().to_owned(),
            config_hash: manifest.config_hash().to_owned(),
        }
    }
}

#[derive(Debug)]
pub enum FunnelCommandError {
    InvalidParameters,
    MissingField,
    MissingIdempotencyKey,
    InvalidIdempotencyKey,
    IdempotencyPayloadMismatch,
    FunnelNotFound,
    UnknownCommand,
    Domain(DomainError),
    Render(RenderError),
    Storage(RepositoryError),
    Internal,
}

impl FunnelCommandError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidParameters | Self::InvalidIdempotencyKey => "validation/invalid_json",
            Self::MissingField | Self::MissingIdempotencyKey => "validation/missing_field",
            Self::Domain(_) | Self::Render(_) => "internal/unexpected",
            Self::IdempotencyPayloadMismatch => "conflict/idempotency_payload_mismatch",
            Self::FunnelNotFound => "not_found/funnel",
            Self::Storage(RepositoryError::FunnelLimitReached) => "conflict/funnel_limit_reached",
            Self::UnknownCommand => "validation/operation_not_supported",
            Self::Storage(RepositoryError::FunnelNotFound) => "not_found/funnel",
            Self::Storage(RepositoryError::InvalidJson | RepositoryError::InvalidField(_)) => {
                "validation/invalid_json"
            }
            Self::Storage(_) => "external/unavailable",
            Self::Internal => "internal/unexpected",
        }
    }
}

impl Display for FunnelCommandError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for FunnelCommandError {}

impl From<serde_json::Error> for FunnelCommandError {
    fn from(_error: serde_json::Error) -> Self {
        Self::InvalidParameters
    }
}

impl From<DomainError> for FunnelCommandError {
    fn from(error: DomainError) -> Self {
        Self::Domain(error)
    }
}

impl From<RenderError> for FunnelCommandError {
    fn from(error: RenderError) -> Self {
        Self::Render(error)
    }
}

impl From<RepositoryError> for FunnelCommandError {
    fn from(error: RepositoryError) -> Self {
        Self::Storage(error)
    }
}

pub fn validate_idempotency_key(key: &str) -> Result<(), FunnelCommandError> {
    if !(1..=128).contains(&key.len())
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(FunnelCommandError::InvalidIdempotencyKey);
    }
    Ok(())
}

fn required_idempotency_key(key: Option<&str>) -> Result<&str, FunnelCommandError> {
    let key = key.ok_or(FunnelCommandError::MissingIdempotencyKey)?;
    validate_idempotency_key(key)?;
    Ok(key)
}

fn validate_name(name: &str) -> Result<&str, FunnelCommandError> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_FUNNEL_NAME_BYTES
        || trimmed.chars().any(char::is_control)
    {
        return Err(FunnelCommandError::InvalidParameters);
    }
    Ok(trimmed)
}

fn validate_route_id(id: &str) -> Result<(), FunnelCommandError> {
    if !(1..=64).contains(&id.len())
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(FunnelCommandError::InvalidParameters);
    }
    Ok(())
}

fn slug_from_name_and_id(name: &str, id: &str) -> String {
    let mut stem = String::with_capacity(48);
    for character in name.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            stem.push(character);
        } else if !stem.ends_with('-') && !stem.is_empty() {
            stem.push('-');
        }
        if stem.len() >= 40 {
            break;
        }
    }
    let stem = stem.trim_matches('-');
    if stem.is_empty() {
        format!("funnel-{id}")
    } else {
        format!("{}-{id}", stem.trim_end_matches('-'))
    }
}

fn blueprint_content(
    blueprint: FunnelBlueprint,
    name: &str,
) -> Result<(Vec<Section>, BTreeMap<String, SectionContent>), FunnelCommandError> {
    let section_types: &[&str] = match blueprint {
        FunnelBlueprint::Tripwire => &["hero", "features", "cta"],
        FunnelBlueprint::Vsl => &["hero", "testimonials", "cta"],
        FunnelBlueprint::Webinar => &["hero", "features", "testimonials", "cta"],
        FunnelBlueprint::Launch => &["hero", "features", "testimonials", "cta", "footer"],
        FunnelBlueprint::LeadGen => &["hero", "features", "cta", "footer"],
    };
    let mut sections = Vec::with_capacity(section_types.len());
    let mut contents = BTreeMap::new();

    for (index, section_type) in section_types.iter().enumerate() {
        let id = (*section_type).to_owned();
        let content = match *section_type {
            "hero" => SectionContent::Hero {
                title: name.to_owned(),
                subtitle: format!("A clear next step for people interested in {}.", name),
                cta_label: match blueprint {
                    FunnelBlueprint::Webinar => "Reserve my place",
                    FunnelBlueprint::LeadGen => "Get the resource",
                    _ => "See the offer",
                }
                .to_owned(),
                cta_url: match blueprint {
                    FunnelBlueprint::Webinar => "/#register",
                    FunnelBlueprint::LeadGen => "/#resource",
                    _ => "/#offer",
                }
                .to_owned(),
                image: None,
            },
            "features" => SectionContent::Features {
                items: vec![
                    FeatureItem {
                        title: "A clear starting point".to_owned(),
                        description: "Show the first useful result this offer provides.".to_owned(),
                    },
                    FeatureItem {
                        title: "A focused next step".to_owned(),
                        description: "Explain what visitors can do after they begin.".to_owned(),
                    },
                ],
            },
            "testimonials" => SectionContent::Testimonials {
                items: vec![Testimonial {
                    quote: "Replace this sample with a real customer quote.".to_owned(),
                    author: "Customer name".to_owned(),
                    attribution: Some("Sample content — replace before publishing".to_owned()),
                }],
            },
            "cta" => SectionContent::Cta {
                title: "Ready to take the next step?".to_owned(),
                body: "Review the offer and choose the action that works for you.".to_owned(),
                cta_label: "Continue".to_owned(),
                cta_url: "/#offer".to_owned(),
            },
            "footer" => SectionContent::Footer {
                links: vec![FooterLink {
                    label: "Contact".to_owned(),
                    url: "/contact".to_owned(),
                }],
                copyright: "Your business name".to_owned(),
            },
            _ => return Err(FunnelCommandError::Internal),
        };
        sections.push(Section::new(Some(id.clone()), section_type, index + 1)?);
        contents.insert(id, content);
    }

    Ok((sections, contents))
}
