use std::collections::{BTreeMap, HashSet};
use std::fmt::{Display, Formatter};

use crate::registry::{command_ids, resolve_command, CommandActor, CommandHandler};
use eez_domain::{
    document_v2::{BlueprintId, DocumentV2, Edit, IdKind, IdSource},
    DomainError, Funnel, FunnelBlueprint, FunnelId, RenderManifest, Revision, RevisionId, Section,
    ThemePreset,
};
use eez_render::{
    content_config_hash, document_v2 as render_v2, FeatureItem, FooterLink, RenderError, Renderer,
    SectionContent, Testimonial, ThemeTokens, RENDERER_VERSION,
};
use eez_storage::repository::{
    EditorSaveMutation, FunnelRecord, MutationResult, NewFunnel, NewRevision, PublicationMutation,
    Repository, RepositoryError, RevisionRecord,
};
use eez_storage::stage::{
    DocumentStageRepository, StageFinalizeMutation, StageMintMutation, V2_DOCUMENT_STAGE_MAX_BYTES,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[path = "funnel/editor_settings.rs"]
pub(super) mod inspector;

#[cfg(test)]
#[path = "funnel/v2_save_tests.rs"]
mod v2_save_tests;

#[cfg(test)]
#[path = "funnel/v2_edit_tests.rs"]
mod v2_edit_tests;

#[cfg(test)]
#[path = "funnel/v1_conversion_tests.rs"]
mod v1_conversion_tests;

const MAX_FUNNEL_NAME_BYTES: usize = 120;
const MAX_SNAPSHOT_BYTES: usize = 1_048_576;
const MAX_EDITOR_COMMAND_BYTES: usize = 17 * 1_024;
const MAX_V2_PREVIEW_ENVELOPE_BYTES: usize =
    eez_domain::document_v2::MAX_RAW_DOCUMENT_BYTES * 6 + 4 * 1_024;
const EMPTY_ASSETS_HASH: &str =
    "assets-sha256-v1:8b3972abe1696375524a8237889ff77fa13664876dd51ede33db93795b8d5cca";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FunnelCreateCommand {
    name: String,
    blueprint: FunnelBlueprint,
}

impl FunnelCreateCommand {
    pub const ID: &'static str = command_ids::FUNNEL_CREATE;

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
pub struct FunnelCreateCommandV2 {
    name: String,
    blueprint: FunnelBlueprint,
}

impl FunnelCreateCommandV2 {
    pub const ID: &'static str = command_ids::FUNNEL_CREATE_V2;

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

    pub fn prepare(
        &self,
        funnel_id: impl Into<String>,
        revision_id: impl Into<String>,
    ) -> Result<PreparedFunnelV2, FunnelCommandError> {
        let funnel_id = funnel_id.into();
        let revision_id = revision_id.into();
        validate_route_id(&funnel_id)?;
        validate_route_id(&revision_id)?;
        let domain_funnel = Funnel::new(
            FunnelId::new(funnel_id.clone())?,
            &self.name,
            slug_from_name_and_id(&self.name, &funnel_id),
        )?;
        let document = DocumentV2::starter(
            &funnel_id,
            blueprint_id(self.blueprint),
            ThemePreset::Minimal,
        )
        .map_err(|_| FunnelCommandError::InvalidParameters)?;
        let prepared = render_v2::prepare_revision(&document, &revision_id)
            .map_err(|_| FunnelCommandError::InvalidParameters)?;
        let revision = NewRevision {
            id: revision_id.clone(),
            funnel_id: funnel_id.clone(),
            schema_version: 2,
            data_json: prepared.data_json,
            render_manifest_json: prepared.render_manifest_json,
        };
        Ok(PreparedFunnelV2 {
            funnel: NewFunnel {
                id: domain_funnel.id().as_str().to_owned(),
                name: domain_funnel.name().to_owned(),
                slug: domain_funnel.slug().to_owned(),
            },
            revision,
        })
    }
}

pub struct PreparedFunnelV2 {
    funnel: NewFunnel,
    revision: NewRevision,
}

impl PreparedFunnelV2 {
    pub fn funnel(&self) -> &NewFunnel {
        &self.funnel
    }

    pub fn revision(&self) -> &NewRevision {
        &self.revision
    }
}

fn blueprint_id(blueprint: FunnelBlueprint) -> BlueprintId {
    match blueprint {
        FunnelBlueprint::Tripwire => BlueprintId::Tripwire,
        FunnelBlueprint::Vsl => BlueprintId::Vsl,
        FunnelBlueprint::Webinar => BlueprintId::Webinar,
        FunnelBlueprint::Launch => BlueprintId::Launch,
        FunnelBlueprint::LeadGen => BlueprintId::LeadGen,
    }
}

#[derive(Debug, Serialize)]
struct V1ConversionNote {
    code: &'static str,
    path: String,
    message: &'static str,
}

#[derive(Debug)]
struct ConvertedV1Document {
    document: DocumentV2,
    warnings: Vec<V1ConversionNote>,
}

fn convert_v1_to_v2(
    source: &ValidatedSnapshot,
    funnel_id: &str,
) -> Result<ConvertedV1Document, Vec<V1ConversionNote>> {
    use eez_domain::document_v2::SectionId as SectionIdV2;

    let mut blockers = Vec::new();
    if eez_domain::document_v2::V2FunnelId::parse(funnel_id.to_owned()).is_err() {
        blockers.push(V1ConversionNote {
            code: "conversion/unsupported_funnel_id",
            path: "funnel_id".to_owned(),
            message: "This funnel ID is not valid in the typed document format.",
        });
    }

    for (index, section) in source.revision.sections().iter().enumerate() {
        let section_path = format!("sections[{index}]");
        if SectionIdV2::parse(section.id().as_str().to_owned()).is_err() {
            blockers.push(V1ConversionNote {
                code: "conversion/unsupported_section_id",
                path: format!("{section_path}.id"),
                message: "This section ID cannot be represented by the typed document format.",
            });
        }
        let content_path = format!("contents.{}", section.id().as_str());
        let Some(content) = source.contents.get(section.id().as_str()) else {
            blockers.push(V1ConversionNote {
                code: "conversion/section_content_missing",
                path: content_path,
                message: "The source section has no content to convert.",
            });
            continue;
        };
        match content {
            SectionContent::Hero { cta_url, image, .. } => {
                if !is_https_conversion_url(cta_url) {
                    blockers.push(V1ConversionNote {
                        code: "conversion/relative_link_unsupported",
                        path: format!("{content_path}.cta_url"),
                        message: "Typed buttons support external HTTPS URLs or page links; this legacy destination cannot be preserved.",
                    });
                }
                if image.is_some() {
                    blockers.push(V1ConversionNote {
                        code: "conversion/image_requires_asset",
                        path: format!("{content_path}.image"),
                        message: "This URL-backed image needs an immutable media asset before it can be converted.",
                    });
                }
            }
            SectionContent::Features { items } if items.is_empty() => {
                blockers.push(V1ConversionNote {
                    code: "conversion/empty_section_unsupported",
                    path: format!("{content_path}.items"),
                    message: "The typed document requires at least one item in this section.",
                });
            }
            SectionContent::Testimonials { items } if items.is_empty() => {
                blockers.push(V1ConversionNote {
                    code: "conversion/empty_section_unsupported",
                    path: format!("{content_path}.items"),
                    message: "The typed document requires at least one item in this section.",
                });
            }
            SectionContent::Cta { cta_url, .. } => {
                if !is_https_conversion_url(cta_url) {
                    blockers.push(V1ConversionNote {
                        code: "conversion/relative_link_unsupported",
                        path: format!("{content_path}.cta_url"),
                        message: "Typed buttons support external HTTPS URLs or page links; this legacy destination cannot be preserved.",
                    });
                }
            }
            SectionContent::Footer { links, .. } => {
                for (link_index, link) in links.iter().enumerate() {
                    if !is_https_conversion_url(&link.url) {
                        blockers.push(V1ConversionNote {
                            code: "conversion/relative_link_unsupported",
                            path: format!("{content_path}.links[{link_index}].url"),
                            message: "Typed buttons support external HTTPS URLs or page links; this legacy destination cannot be preserved.",
                        });
                    }
                }
                if links.len() >= 50 {
                    blockers.push(V1ConversionNote {
                        code: "conversion/section_block_limit",
                        path: format!("{content_path}.links"),
                        message: "The links and copyright exceed the typed section block limit.",
                    });
                }
            }
            _ => {}
        }
    }
    if !blockers.is_empty() {
        return Err(blockers);
    }

    let mut document_json = serde_json::from_slice::<Value>(
        DocumentV2::starter(
            funnel_id,
            blueprint_id(source.snapshot.blueprint),
            source.snapshot.theme_id,
        )
        .map_err(|_| {
            vec![V1ConversionNote {
                code: "conversion/typed_document_invalid",
                path: "document".to_owned(),
                message: "The typed document could not be initialized from this funnel.",
            }]
        })?
        .canonical_bytes(),
    )
    .map_err(|_| {
        vec![V1ConversionNote {
            code: "conversion/typed_document_invalid",
            path: "document".to_owned(),
            message: "The typed document could not be initialized from this funnel.",
        }]
    })?;

    if let Some(primary_color) = &source.snapshot.theme_overrides.primary_color {
        document_json["theme"]["overrides"]["colors"]["color-primary"] =
            serde_json::json!(primary_color);
    }
    if source.snapshot.theme_overrides.square_buttons {
        document_json["theme"]["overrides"]["button_shape"] = serde_json::json!("square");
    }

    let mut warnings = Vec::new();
    let mut converted_sections = Vec::with_capacity(source.revision.sections().len());
    for (section_index, section) in source.revision.sections().iter().enumerate() {
        let content = source.contents.get(section.id().as_str()).ok_or_else(|| {
            vec![V1ConversionNote {
                code: "conversion/section_content_missing",
                path: format!("contents.{}", section.id().as_str()),
                message: "The source section has no content to convert.",
            }]
        })?;
        let section_id = section.id().as_str();
        let section_type = section.section_type().as_str();
        let mut block_serial = 0usize;
        let mut block_id = || {
            block_serial += 1;
            format!("block-convert-{}-{block_serial}", section_index + 1)
        };
        let mut blocks = Vec::new();
        match content {
            SectionContent::Hero {
                title,
                subtitle,
                cta_label,
                cta_url,
                image: None,
            } => {
                blocks.push(serde_json::json!({
                    "id": block_id(), "kind": "heading", "level": 1, "text": title,
                }));
                blocks.push(serde_json::json!({
                    "id": block_id(), "kind": "text", "text": subtitle,
                }));
                blocks.push(serde_json::json!({
                    "id": block_id(), "kind": "button", "label": cta_label,
                    "action": {"kind": "external_https", "url": cta_url},
                }));
            }
            SectionContent::Features { items } => {
                warnings.push(V1ConversionNote {
                    code: "conversion/feature_structure_flattened",
                    path: format!("contents.{section_id}.items"),
                    message: "Feature titles and descriptions become entries in one typed list.",
                });
                blocks.push(serde_json::json!({
                    "id": block_id(), "kind": "list",
                    "items": items.iter().map(|item| format!("{}: {}", item.title, item.description)).collect::<Vec<_>>(),
                }));
            }
            SectionContent::Testimonials { items } => {
                warnings.push(V1ConversionNote {
                    code: "conversion/testimonial_structure_flattened",
                    path: format!("contents.{section_id}.items"),
                    message: "Quote, author, and attribution text is kept together in plain text blocks.",
                });
                for item in items {
                    let mut text = format!("{}\n\n— {}", item.quote, item.author);
                    if let Some(attribution) = &item.attribution {
                        text.push('\n');
                        text.push_str(attribution);
                    }
                    blocks.push(serde_json::json!({
                        "id": block_id(), "kind": "text", "text": text,
                    }));
                }
            }
            SectionContent::Cta {
                title,
                body,
                cta_label,
                cta_url,
            } => {
                blocks.push(serde_json::json!({
                    "id": block_id(), "kind": "heading", "level": 2, "text": title,
                }));
                blocks.push(serde_json::json!({
                    "id": block_id(), "kind": "text", "text": body,
                }));
                blocks.push(serde_json::json!({
                    "id": block_id(), "kind": "button", "label": cta_label,
                    "action": {"kind": "external_https", "url": cta_url},
                }));
            }
            SectionContent::Footer { links, copyright } => {
                warnings.push(V1ConversionNote {
                    code: "conversion/footer_structure_flattened",
                    path: format!("contents.{section_id}.links"),
                    message:
                        "Footer links become separate typed buttons rather than a navigation list.",
                });
                for link in links {
                    blocks.push(serde_json::json!({
                        "id": block_id(), "kind": "button", "label": link.label,
                        "action": {"kind": "external_https", "url": link.url},
                    }));
                }
                blocks.push(serde_json::json!({
                    "id": block_id(), "kind": "text", "text": copyright,
                }));
            }
            SectionContent::Hero { image: Some(_), .. } => unreachable!(
                "URL-backed hero images are reported as deterministic conversion blockers"
            ),
        }
        converted_sections.push(serde_json::json!({
            "id": section_id,
            "type": section_type,
            "blocks": blocks,
        }));
    }
    document_json["pages"][0]["sections"] = serde_json::Value::Array(converted_sections);
    let raw = serde_json::to_vec(&document_json).map_err(|_| {
        vec![V1ConversionNote {
            code: "conversion/typed_document_invalid",
            path: "document".to_owned(),
            message: "Converted content could not be encoded as a typed document.",
        }]
    })?;
    let document = DocumentV2::decode(&raw, funnel_id).map_err(|error| {
        vec![V1ConversionNote {
            code: error.code(),
            path: error.path().unwrap_or("document").to_owned(),
            message: "The converted content does not satisfy typed document validation.",
        }]
    })?;
    Ok(ConvertedV1Document { document, warnings })
}

fn is_https_conversion_url(value: &str) -> bool {
    let Some((scheme, authority)) = value.split_once("://") else {
        return false;
    };
    scheme.eq_ignore_ascii_case("https")
        && !authority.is_empty()
        && !authority.starts_with(['/', '?', '#'])
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FunnelDeleteCommand {
    funnel_id: String,
}

impl FunnelDeleteCommand {
    pub const ID: &'static str = command_ids::FUNNEL_DELETE;

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
pub(super) struct FunnelPublishCommand {
    funnel_id: String,
    revision_id: String,
    pub(super) expected_published_revision_id: Option<String>,
}

impl FunnelPublishCommand {
    const ID: &'static str = command_ids::FUNNEL_PUBLISH;

    pub(super) fn from_json(parameters: &str) -> Result<Self, FunnelCommandError> {
        let value: Value = serde_json::from_str(parameters)?;
        let object = value
            .as_object()
            .ok_or(FunnelCommandError::InvalidParameters)?;
        if ["funnel_id", "revision_id", "expected_published_revision_id"]
            .iter()
            .any(|field| object.get(*field).is_none())
        {
            return Err(FunnelCommandError::MissingField);
        }
        let command: Self = serde_json::from_str(parameters)?;
        validate_route_id(&command.funnel_id)?;
        validate_route_id(&command.revision_id)?;
        if let Some(expected) = &command.expected_published_revision_id {
            validate_route_id(expected)?;
        }
        Ok(command)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FunnelUnpublishCommand {
    funnel_id: String,
    expected_published_revision_id: String,
}

impl FunnelUnpublishCommand {
    const ID: &'static str = command_ids::FUNNEL_UNPUBLISH;

    pub(super) fn from_json(parameters: &str) -> Result<Self, FunnelCommandError> {
        let value: Value = serde_json::from_str(parameters)?;
        let object = value
            .as_object()
            .ok_or(FunnelCommandError::InvalidParameters)?;
        if ["funnel_id", "expected_published_revision_id"]
            .iter()
            .any(|field| object.get(*field).is_none())
        {
            return Err(FunnelCommandError::MissingField);
        }
        let command: Self = serde_json::from_str(parameters)?;
        validate_route_id(&command.funnel_id)?;
        validate_route_id(&command.expected_published_revision_id)?;
        Ok(command)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FunnelPublicReadCommand {
    slug: String,
}

impl FunnelPublicReadCommand {
    const ID: &'static str = command_ids::FUNNEL_PUBLIC_READ;

    pub(super) fn from_json(parameters: &str) -> Result<Self, FunnelCommandError> {
        let value: Value = serde_json::from_str(parameters)?;
        let object = value
            .as_object()
            .ok_or(FunnelCommandError::InvalidParameters)?;
        if object.get("slug").is_none() {
            return Err(FunnelCommandError::MissingField);
        }
        let command: Self = serde_json::from_str(parameters)?;
        if !valid_public_slug(&command.slug) {
            return Err(FunnelCommandError::InvalidParameters);
        }
        Ok(command)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FunnelListCommand {}

impl FunnelListCommand {
    pub const ID: &'static str = command_ids::FUNNEL_LIST;

    pub fn from_json(parameters: &str) -> Result<Self, FunnelCommandError> {
        serde_json::from_str(parameters).map_err(Into::into)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct FunnelEditorReadCommand {
    funnel_id: String,
}

impl FunnelEditorReadCommand {
    const ID: &'static str = command_ids::FUNNEL_EDITOR_READ;

    fn from_json(parameters: &str) -> Result<Self, FunnelCommandError> {
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
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct FunnelDocumentReadV2Command {
    funnel_id: String,
    #[serde(default)]
    revision_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct FunnelDocumentConversionV2Command {
    funnel_id: String,
    base_revision_id: String,
}

impl FunnelDocumentConversionV2Command {
    fn from_json(parameters: &str) -> Result<Self, FunnelCommandError> {
        let value: Value = serde_json::from_str(parameters)?;
        let object = value
            .as_object()
            .ok_or(FunnelCommandError::InvalidParameters)?;
        if object.get("funnel_id").is_none() || object.get("base_revision_id").is_none() {
            return Err(FunnelCommandError::MissingField);
        }
        let command: Self = serde_json::from_str(parameters)?;
        validate_route_id(&command.funnel_id)?;
        validate_route_id(&command.base_revision_id)?;
        Ok(command)
    }
}

impl FunnelDocumentReadV2Command {
    fn from_json(parameters: &str) -> Result<Self, FunnelCommandError> {
        let value: Value = serde_json::from_str(parameters)?;
        if value
            .as_object()
            .is_none_or(|object| object.get("funnel_id").is_none())
        {
            return Err(FunnelCommandError::MissingField);
        }
        let command: Self = serde_json::from_str(parameters)?;
        validate_route_id(&command.funnel_id)?;
        if let Some(revision_id) = &command.revision_id {
            validate_route_id(revision_id)?;
        }
        Ok(command)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct FunnelRevisionHistoryReadV2Command {
    funnel_id: String,
}

impl FunnelRevisionHistoryReadV2Command {
    fn from_json(parameters: &str) -> Result<Self, FunnelCommandError> {
        let value: Value = serde_json::from_str(parameters)?;
        if value
            .as_object()
            .is_none_or(|object| object.get("funnel_id").is_none())
        {
            return Err(FunnelCommandError::MissingField);
        }
        let command: Self = serde_json::from_str(parameters)?;
        validate_route_id(&command.funnel_id)?;
        Ok(command)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct FunnelDocumentPreviewV2Command {
    funnel_id: String,
    base_revision_id: String,
    page_id: String,
    document_json: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct FunnelDocumentEditV2Command {
    funnel_id: String,
    base_revision_id: String,
    page_id: String,
    document_json: String,
    operation_id: String,
    edit: Edit,
}

struct DeterministicEditIds {
    operation_id: String,
    next: u32,
}

impl IdSource for DeterministicEditIds {
    fn next_id(&mut self, _kind: IdKind) -> Result<String, eez_domain::document_v2::DocumentError> {
        self.next = self.next.wrapping_add(1);
        Ok(format!("{}-{}", self.operation_id, self.next))
    }
}

impl FunnelDocumentEditV2Command {
    fn from_json(parameters: &str) -> Result<Self, FunnelCommandError> {
        if parameters.len() > MAX_V2_PREVIEW_ENVELOPE_BYTES {
            return Err(FunnelCommandError::InvalidParameters);
        }
        let value: Value = serde_json::from_str(parameters)?;
        let object = value
            .as_object()
            .ok_or(FunnelCommandError::InvalidParameters)?;
        if [
            "funnel_id",
            "base_revision_id",
            "page_id",
            "document_json",
            "operation_id",
            "edit",
        ]
        .iter()
        .any(|field| object.get(*field).is_none())
        {
            return Err(FunnelCommandError::MissingField);
        }
        let command: Self = serde_json::from_str(parameters)?;
        validate_route_id(&command.funnel_id)?;
        validate_route_id(&command.base_revision_id)?;
        validate_route_id(&command.page_id)?;
        if command.document_json.len() > eez_domain::document_v2::MAX_RAW_DOCUMENT_BYTES
            || command.operation_id.len() != 32
            || !command
                .operation_id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(FunnelCommandError::InvalidParameters);
        }
        Ok(command)
    }
}

impl FunnelDocumentPreviewV2Command {
    fn from_json(parameters: &str) -> Result<Self, FunnelCommandError> {
        if parameters.len() > MAX_V2_PREVIEW_ENVELOPE_BYTES {
            return Err(FunnelCommandError::InvalidParameters);
        }
        let value: Value = serde_json::from_str(parameters)?;
        let object = value
            .as_object()
            .ok_or(FunnelCommandError::InvalidParameters)?;
        if ["funnel_id", "base_revision_id", "page_id", "document_json"]
            .iter()
            .any(|field| object.get(*field).is_none())
        {
            return Err(FunnelCommandError::MissingField);
        }
        let command: Self = serde_json::from_str(parameters)?;
        validate_route_id(&command.funnel_id)?;
        validate_route_id(&command.base_revision_id)?;
        validate_route_id(&command.page_id)?;
        if command.document_json.len() > eez_domain::document_v2::MAX_RAW_DOCUMENT_BYTES {
            return Err(FunnelCommandError::InvalidParameters);
        }
        Ok(command)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct FunnelDocumentStageV2Command {
    funnel_id: String,
    base_revision_id: String,
    expected_length: u32,
    expected_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct FunnelDocumentSaveV2Command {
    funnel_id: String,
    base_revision_id: String,
    stage_id: String,
    expected_length: u32,
    expected_sha256: String,
    document_version: u32,
    schema_version: u32,
    manifest_version: u32,
}

impl FunnelDocumentSaveV2Command {
    fn from_json(parameters: &str) -> Result<Self, FunnelCommandError> {
        if parameters.len() > MAX_EDITOR_COMMAND_BYTES {
            return Err(FunnelCommandError::InvalidParameters);
        }
        let value: Value = serde_json::from_str(parameters)?;
        let object = value
            .as_object()
            .ok_or(FunnelCommandError::InvalidParameters)?;
        if [
            "funnel_id",
            "base_revision_id",
            "stage_id",
            "expected_length",
            "expected_sha256",
            "document_version",
            "schema_version",
            "manifest_version",
        ]
        .iter()
        .any(|field| !object.contains_key(*field))
        {
            return Err(FunnelCommandError::MissingField);
        }
        let command: Self = serde_json::from_str(parameters)?;
        validate_route_id(&command.funnel_id)?;
        validate_route_id(&command.base_revision_id)?;
        if !is_lower_hex_sha256(&command.stage_id)
            || !is_lower_hex_sha256(&command.expected_sha256)
            || command.expected_length == 0
            || usize::try_from(command.expected_length)
                .map_or(true, |length| length > V2_DOCUMENT_STAGE_MAX_BYTES)
            || command.document_version != 2
            || command.schema_version != 2
            || command.manifest_version != 2
        {
            return Err(FunnelCommandError::InvalidParameters);
        }
        Ok(command)
    }
}

impl FunnelDocumentStageV2Command {
    fn from_json(parameters: &str) -> Result<Self, FunnelCommandError> {
        if parameters.len() > MAX_EDITOR_COMMAND_BYTES {
            return Err(FunnelCommandError::InvalidParameters);
        }
        let value: Value = serde_json::from_str(parameters)?;
        let object = value
            .as_object()
            .ok_or(FunnelCommandError::InvalidParameters)?;
        if [
            "funnel_id",
            "base_revision_id",
            "expected_length",
            "expected_sha256",
        ]
        .iter()
        .any(|field| !object.contains_key(*field))
        {
            return Err(FunnelCommandError::MissingField);
        }
        let command: Self = serde_json::from_str(parameters)?;
        validate_route_id(&command.funnel_id)?;
        validate_route_id(&command.base_revision_id)?;
        if command.expected_length == 0
            || usize::try_from(command.expected_length)
                .map_or(true, |length| length > V2_DOCUMENT_STAGE_MAX_BYTES)
            || !is_lower_hex_sha256(&command.expected_sha256)
        {
            return Err(FunnelCommandError::InvalidParameters);
        }
        Ok(command)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct FunnelEditorOrderCommand {
    funnel_id: String,
    base_revision_id: String,
    section_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    settings: Option<inspector::SettingChanges>,
}

impl FunnelEditorOrderCommand {
    fn from_json(parameters: &str) -> Result<Self, FunnelCommandError> {
        if parameters.len() > MAX_EDITOR_COMMAND_BYTES {
            return Err(FunnelCommandError::InvalidParameters);
        }
        let value: Value = serde_json::from_str(parameters)?;
        let object = value
            .as_object()
            .ok_or(FunnelCommandError::InvalidParameters)?;
        if ["funnel_id", "base_revision_id", "section_ids"]
            .iter()
            .any(|field| object.get(*field).is_none())
        {
            return Err(FunnelCommandError::MissingField);
        }
        if object.get("settings").is_some_and(Value::is_null) {
            return Err(FunnelCommandError::InvalidParameters);
        }
        let command: Self = serde_json::from_str(parameters)?;
        validate_route_id(&command.funnel_id)?;
        validate_route_id(&command.base_revision_id)?;
        if command.section_ids.iter().any(|id| {
            id.is_empty()
                || id.len() > 64
                || !id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        }) {
            return Err(FunnelCommandError::InvalidParameters);
        }
        let mut command = command;
        if let Some(settings) = &command.settings {
            settings.validate_envelope()?;
            if settings.changes.is_empty() {
                command.settings = None;
            }
        }
        Ok(command)
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
    document_stages: Option<&'repo DocumentStageRepository<'database>>,
}

impl<'repo, 'database> Session<'repo, 'database> {
    pub fn new(repository: &'repo Repository<'database>) -> Self {
        Self {
            repository,
            document_stages: None,
        }
    }

    pub fn with_document_stages(
        repository: &'repo Repository<'database>,
        document_stages: &'repo DocumentStageRepository<'database>,
    ) -> Self {
        Self {
            repository,
            document_stages: Some(document_stages),
        }
    }

    pub async fn execute(
        &self,
        actor: CommandActor,
        command_id: &str,
        parameters: &str,
        idempotency_key: Option<&str>,
    ) -> Result<Value, FunnelCommandError> {
        let command = resolve_command(actor, command_id, idempotency_key)?;
        match command.handler() {
            CommandHandler::List => self.execute_list(parameters).await,
            CommandHandler::PublicRead => self.execute_public_read(parameters).await,
            CommandHandler::Publish => {
                self.execute_publish(
                    parameters,
                    command
                        .idempotency_key
                        .ok_or(FunnelCommandError::Internal)?,
                )
                .await
            }
            CommandHandler::Unpublish => {
                self.execute_unpublish(
                    parameters,
                    command
                        .idempotency_key
                        .ok_or(FunnelCommandError::Internal)?,
                )
                .await
            }
            CommandHandler::EditorRead => self.execute_editor_read(parameters).await,
            CommandHandler::DocumentReadV2 => self.execute_document_read_v2(parameters).await,
            CommandHandler::DocumentEditV2 => self.execute_document_edit_v2(parameters).await,
            CommandHandler::DocumentConversionPreviewV2 => {
                self.execute_document_conversion_preview_v2(parameters)
                    .await
            }
            CommandHandler::DocumentConversionCommitV2 => {
                self.execute_document_conversion_commit_v2(
                    parameters,
                    command
                        .idempotency_key
                        .ok_or(FunnelCommandError::MissingIdempotencyKey)?,
                )
                .await
            }
            CommandHandler::DocumentPreviewV2 => self.execute_document_preview_v2(parameters).await,
            CommandHandler::DocumentStageV2 => {
                self.execute_document_stage_v2(
                    parameters,
                    command
                        .idempotency_key
                        .ok_or(FunnelCommandError::MissingIdempotencyKey)?,
                )
                .await
            }
            CommandHandler::RevisionSaveV2 => {
                self.execute_document_save_v2(
                    parameters,
                    command
                        .idempotency_key
                        .ok_or(FunnelCommandError::MissingIdempotencyKey)?,
                )
                .await
            }
            CommandHandler::RevisionHistoryReadV2 => {
                self.execute_revision_history_read_v2(parameters).await
            }
            CommandHandler::EditorPreview => self.execute_editor_preview(parameters).await,
            CommandHandler::RevisionSave => {
                self.execute_editor_save(
                    parameters,
                    command
                        .idempotency_key
                        .ok_or(FunnelCommandError::Internal)?,
                )
                .await
            }
            CommandHandler::Create => {
                self.execute_create(
                    parameters,
                    command
                        .idempotency_key
                        .ok_or(FunnelCommandError::Internal)?,
                )
                .await
            }
            CommandHandler::CreateV2 => {
                self.execute_create_v2(
                    parameters,
                    command
                        .idempotency_key
                        .ok_or(FunnelCommandError::Internal)?,
                )
                .await
            }
            CommandHandler::Delete => {
                self.execute_delete(
                    parameters,
                    command
                        .idempotency_key
                        .ok_or(FunnelCommandError::Internal)?,
                )
                .await
            }
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

    async fn execute_publish(
        &self,
        parameters: &str,
        idempotency_key: &str,
    ) -> Result<Value, FunnelCommandError> {
        let command = match FunnelPublishCommand::from_json(parameters) {
            Ok(command) => command,
            Err(error) => {
                if self
                    .repository
                    .classify_active_receipt(idempotency_key, FunnelPublishCommand::ID, None)
                    .await?
                    .is_some()
                {
                    return Err(FunnelCommandError::IdempotencyPayloadMismatch);
                }
                return Err(error);
            }
        };
        let canonical_parameters =
            serde_json::to_string(&command).map_err(|_| FunnelCommandError::Internal)?;
        if let Some(result) = self
            .repository
            .classify_active_receipt(
                idempotency_key,
                FunnelPublishCommand::ID,
                Some(&canonical_parameters),
            )
            .await?
        {
            return mutation_result_to_value(result);
        }
        let funnel = self
            .repository
            .get_funnel(&command.funnel_id)
            .await?
            .ok_or(FunnelCommandError::FunnelNotFound)?;
        if funnel.published_revision_id != command.expected_published_revision_id
            || funnel.published_revision_id.as_deref() == Some(&command.revision_id)
        {
            return Err(FunnelCommandError::StalePublishedRevision);
        }
        let record = self
            .repository
            .get_owned_revision(&command.funnel_id, &command.revision_id)
            .await?
            .ok_or(FunnelCommandError::RevisionNotFound)?;
        render_published_revision(&record, &command.funnel_id)?;
        let result_json = serde_json::to_string(&serde_json::json!({
            "command_id": FunnelPublishCommand::ID,
            "funnel_id": command.funnel_id,
            "published_revision_id": command.revision_id,
        }))
        .map_err(|_| FunnelCommandError::Internal)?;
        let result = self
            .repository
            .set_published_revision_idempotently(&PublicationMutation {
                funnel_id: &command.funnel_id,
                published_revision_id: Some(&command.revision_id),
                expected_published_revision_id: command.expected_published_revision_id.as_deref(),
                command_id: FunnelPublishCommand::ID,
                idempotency_key,
                payload_json: &canonical_parameters,
                result_json: &result_json,
            })
            .await?;
        mutation_result_to_value(result)
    }

    async fn execute_unpublish(
        &self,
        parameters: &str,
        idempotency_key: &str,
    ) -> Result<Value, FunnelCommandError> {
        let command = match FunnelUnpublishCommand::from_json(parameters) {
            Ok(command) => command,
            Err(error) => {
                if self
                    .repository
                    .classify_active_receipt(idempotency_key, FunnelUnpublishCommand::ID, None)
                    .await?
                    .is_some()
                {
                    return Err(FunnelCommandError::IdempotencyPayloadMismatch);
                }
                return Err(error);
            }
        };
        let canonical_parameters =
            serde_json::to_string(&command).map_err(|_| FunnelCommandError::Internal)?;
        if let Some(result) = self
            .repository
            .classify_active_receipt(
                idempotency_key,
                FunnelUnpublishCommand::ID,
                Some(&canonical_parameters),
            )
            .await?
        {
            return mutation_result_to_value(result);
        }
        let funnel = self
            .repository
            .get_funnel(&command.funnel_id)
            .await?
            .ok_or(FunnelCommandError::FunnelNotFound)?;
        if funnel.published_revision_id.as_deref() != Some(&command.expected_published_revision_id)
        {
            return Err(FunnelCommandError::StalePublishedRevision);
        }
        let result_json = serde_json::to_string(&serde_json::json!({
            "command_id": FunnelUnpublishCommand::ID,
            "funnel_id": command.funnel_id,
            "published_revision_id": null,
            "unpublished": true,
        }))
        .map_err(|_| FunnelCommandError::Internal)?;
        let result = self
            .repository
            .set_published_revision_idempotently(&PublicationMutation {
                funnel_id: &command.funnel_id,
                published_revision_id: None,
                expected_published_revision_id: Some(&command.expected_published_revision_id),
                command_id: FunnelUnpublishCommand::ID,
                idempotency_key,
                payload_json: &canonical_parameters,
                result_json: &result_json,
            })
            .await?;
        mutation_result_to_value(result)
    }

    async fn execute_public_read(&self, parameters: &str) -> Result<Value, FunnelCommandError> {
        let command = FunnelPublicReadCommand::from_json(parameters)?;
        let record = self
            .repository
            .get_published_revision_by_slug(&command.slug)
            .await?
            .ok_or(FunnelCommandError::SlugNotFound)?;
        let html = render_published_revision(&record, &record.funnel_id)?;
        Ok(serde_json::json!({
            "command_id": FunnelPublicReadCommand::ID,
            "revision_id": record.id,
            "html": html,
        }))
    }

    async fn execute_editor_read(&self, parameters: &str) -> Result<Value, FunnelCommandError> {
        let command = FunnelEditorReadCommand::from_json(parameters)?;
        let record = self
            .repository
            .get_editor_revision(&command.funnel_id)
            .await?
            .ok_or(FunnelCommandError::FunnelNotFound)?;
        if record.revision.schema_version == 2 {
            let document = render_v2::validate_revision_v2(
                record.revision.schema_version,
                &record.funnel_id,
                &record.revision.id,
                &record.revision.data_json,
                &record.revision.render_manifest_json,
            )
            .map_err(|_| FunnelCommandError::Internal)?;
            return Ok(serde_json::json!({
                "command_id": FunnelEditorReadCommand::ID,
                "schema_version": 2,
                "funnel": {
                    "id": record.funnel_id,
                    "name": record.name,
                    "slug": record.slug,
                    "updated_at": record.updated_at,
                },
                "base_revision_id": record.head_revision_id,
                "published_revision_id": record.published_revision_id,
                "entry_page_id": document.entry_page_id().as_str(),
                "pages": v2_page_projection(&document),
            }));
        }
        if record.revision.schema_version != 1 {
            return Err(FunnelCommandError::Internal);
        }
        let validated = decode_snapshot(&record.revision, &record.funnel_id)?;
        let preview_html = Renderer::new().render_preview(
            &validated.revision,
            &validated.theme,
            &validated.contents,
        )?;
        let sections = validated
            .revision
            .sections()
            .iter()
            .map(|section| {
                serde_json::json!({
                    "id": section.id().as_str(),
                    "type": section.section_type().as_str(),
                })
            })
            .collect::<Vec<_>>();
        let (settings_schema, settings_values) = inspector::inspector_projection(&validated)?;
        Ok(serde_json::json!({
            "command_id": FunnelEditorReadCommand::ID,
            "funnel": {
                "id": record.funnel_id,
                "name": record.name,
                "slug": record.slug,
                "updated_at": record.updated_at,
            },
            "base_revision_id": record.head_revision_id,
            "published_revision_id": record.published_revision_id,
            "sections": sections,
            "settings_schema": settings_schema,
            "settings_values": settings_values,
            "preview_html": preview_html,
        }))
    }

    async fn execute_document_conversion_preview_v2(
        &self,
        parameters: &str,
    ) -> Result<Value, FunnelCommandError> {
        let command = FunnelDocumentConversionV2Command::from_json(parameters)?;
        let current = self
            .repository
            .get_editor_revision(&command.funnel_id)
            .await?
            .ok_or(FunnelCommandError::FunnelNotFound)?;
        if current.head_revision_id != command.base_revision_id {
            return Err(FunnelCommandError::StaleEditorRevision);
        }
        if current.revision.schema_version != 1 {
            return Err(FunnelCommandError::V2ConversionBlocked);
        }
        let validated = decode_snapshot(&current.revision, &command.funnel_id)?;
        let conversion = match convert_v1_to_v2(&validated, &command.funnel_id) {
            Ok(conversion) => conversion,
            Err(blockers) => {
                return Ok(serde_json::json!({
                    "command_id": command_ids::FUNNEL_DOCUMENT_CONVERSION_PREVIEW_V2,
                    "funnel_id": command.funnel_id,
                    "base_revision_id": current.head_revision_id,
                    "conversion_available": false,
                    "blockers": blockers,
                    "warnings": [],
                    "preview_html": null,
                }));
            }
        };
        let preview_html = render_v2::render_owner_preview(
            &conversion.document,
            conversion.document.entry_page_id(),
        )
        .map_err(|_| FunnelCommandError::InvalidParameters)?;
        Ok(serde_json::json!({
            "command_id": command_ids::FUNNEL_DOCUMENT_CONVERSION_PREVIEW_V2,
            "funnel_id": command.funnel_id,
            "base_revision_id": current.head_revision_id,
            "conversion_available": true,
            "blockers": [],
            "warnings": conversion.warnings,
            "preview_html": preview_html,
        }))
    }

    async fn execute_document_conversion_commit_v2(
        &self,
        parameters: &str,
        idempotency_key: &str,
    ) -> Result<Value, FunnelCommandError> {
        let command = match FunnelDocumentConversionV2Command::from_json(parameters) {
            Ok(command) => command,
            Err(error) => {
                if self
                    .repository
                    .classify_active_receipt(
                        idempotency_key,
                        command_ids::FUNNEL_DOCUMENT_CONVERSION_COMMIT_V2,
                        None,
                    )
                    .await?
                    .is_some()
                {
                    return Err(FunnelCommandError::IdempotencyPayloadMismatch);
                }
                return Err(error);
            }
        };
        let canonical_parameters =
            serde_json::to_string(&command).map_err(|_| FunnelCommandError::Internal)?;
        if let Some(result) = self
            .repository
            .classify_active_receipt(
                idempotency_key,
                command_ids::FUNNEL_DOCUMENT_CONVERSION_COMMIT_V2,
                Some(&canonical_parameters),
            )
            .await?
        {
            return mutation_result_to_value(result);
        }
        let current = self
            .repository
            .get_editor_revision(&command.funnel_id)
            .await?
            .ok_or(FunnelCommandError::FunnelNotFound)?;
        if current.head_revision_id != command.base_revision_id {
            return Err(FunnelCommandError::StaleEditorRevision);
        }
        if current.revision.schema_version != 1 {
            return Err(FunnelCommandError::V2ConversionBlocked);
        }
        let validated = decode_snapshot(&current.revision, &command.funnel_id)?;
        let conversion = convert_v1_to_v2(&validated, &command.funnel_id)
            .map_err(|_| FunnelCommandError::V2ConversionBlocked)?;
        let revision_id = uuid::Uuid::new_v4().to_string();
        let prepared = render_v2::prepare_revision(&conversion.document, &revision_id)
            .map_err(|_| FunnelCommandError::V2ConversionBlocked)?;
        let revision = NewRevision {
            id: revision_id.clone(),
            funnel_id: command.funnel_id.clone(),
            schema_version: 2,
            data_json: prepared.data_json,
            render_manifest_json: prepared.render_manifest_json,
        };
        let result_json = serde_json::to_string(&serde_json::json!({
            "command_id": command_ids::FUNNEL_DOCUMENT_CONVERSION_COMMIT_V2,
            "funnel_id": command.funnel_id,
            "base_revision_id": command.base_revision_id,
            "revision_id": revision_id,
            "schema_version": 2,
        }))
        .map_err(|_| FunnelCommandError::Internal)?;
        let result = self
            .repository
            .save_editor_revision(&EditorSaveMutation {
                command_id: command_ids::FUNNEL_DOCUMENT_CONVERSION_COMMIT_V2,
                funnel_id: &command.funnel_id,
                expected_revision_id: &command.base_revision_id,
                revision: &revision,
                idempotency_key,
                payload_json: &canonical_parameters,
                result_json: &result_json,
            })
            .await?;
        mutation_result_to_value(result)
    }

    async fn execute_document_read_v2(
        &self,
        parameters: &str,
    ) -> Result<Value, FunnelCommandError> {
        let command = FunnelDocumentReadV2Command::from_json(parameters)?;
        let current = self
            .repository
            .get_editor_revision(&command.funnel_id)
            .await?
            .ok_or(FunnelCommandError::FunnelNotFound)?;
        let revision = match command.revision_id.as_deref() {
            Some(revision_id) => self
                .repository
                .get_owned_revision(&command.funnel_id, revision_id)
                .await?
                .ok_or(FunnelCommandError::FunnelNotFound)?,
            None => current.revision.clone(),
        };
        if revision.schema_version != 2 {
            return Err(FunnelCommandError::V2ConversionBlocked);
        }
        let document = render_v2::validate_revision_v2(
            revision.schema_version,
            &command.funnel_id,
            &revision.id,
            &revision.data_json,
            &revision.render_manifest_json,
        )
        .map_err(|_| FunnelCommandError::Internal)?;
        Ok(serde_json::json!({
            "command_id": command_ids::FUNNEL_DOCUMENT_READ_V2,
            "schema_version": 2,
            "funnel": {
                "id": current.funnel_id,
                "name": current.name,
                "slug": current.slug,
                "updated_at": current.updated_at,
            },
            "base_revision_id": current.head_revision_id,
            "document_revision_id": revision.id,
            "published_revision_id": current.published_revision_id,
            "document_json": revision.data_json,
            "pages": v2_page_projection(&document),
        }))
    }

    async fn execute_revision_history_read_v2(
        &self,
        parameters: &str,
    ) -> Result<Value, FunnelCommandError> {
        let command = FunnelRevisionHistoryReadV2Command::from_json(parameters)?;
        let funnel = self
            .repository
            .get_funnel(&command.funnel_id)
            .await?
            .ok_or(FunnelCommandError::FunnelNotFound)?;
        let revisions = self
            .repository
            .list_revision_summaries(&command.funnel_id)
            .await?
            .into_iter()
            .map(|revision| {
                serde_json::json!({
                    "revision_id": revision.id,
                    "schema_version": revision.schema_version,
                    "created_at": revision.created_at,
                })
            })
            .collect::<Vec<_>>();
        Ok(serde_json::json!({
            "command_id": command_ids::FUNNEL_REVISION_HISTORY_READ_V2,
            "funnel_id": funnel.id,
            "revisions": revisions,
        }))
    }

    async fn execute_document_edit_v2(
        &self,
        parameters: &str,
    ) -> Result<Value, FunnelCommandError> {
        let command = FunnelDocumentEditV2Command::from_json(parameters)?;
        let record = self
            .repository
            .get_editor_revision(&command.funnel_id)
            .await?
            .ok_or(FunnelCommandError::FunnelNotFound)?;
        if record.revision.schema_version != 2 {
            return Err(FunnelCommandError::V2ConversionBlocked);
        }
        if record.head_revision_id != command.base_revision_id {
            return Err(FunnelCommandError::StaleEditorRevision);
        }
        render_v2::validate_revision_v2(
            record.revision.schema_version,
            &record.funnel_id,
            &record.revision.id,
            &record.revision.data_json,
            &record.revision.render_manifest_json,
        )
        .map_err(|_| FunnelCommandError::Internal)?;
        let document = DocumentV2::decode(command.document_json.as_bytes(), &command.funnel_id)
            .map_err(|_| FunnelCommandError::InvalidParameters)?;
        let page_id = eez_domain::document_v2::PageId::parse(command.page_id)
            .map_err(|_| FunnelCommandError::InvalidParameters)?;
        let mut ids = DeterministicEditIds {
            operation_id: command.operation_id,
            next: 0,
        };
        let outcome = document
            .apply(command.edit, &mut ids)
            .map_err(|_| FunnelCommandError::InvalidParameters)?;
        let preview_html = render_v2::render_owner_preview(&outcome.document, &page_id)
            .map_err(|_| FunnelCommandError::InvalidParameters)?;
        let document_json = std::str::from_utf8(outcome.document.canonical_bytes())
            .map_err(|_| FunnelCommandError::Internal)?;
        Ok(serde_json::json!({
            "command_id": command_ids::FUNNEL_DOCUMENT_EDIT_V2,
            "funnel_id": command.funnel_id,
            "base_revision_id": command.base_revision_id,
            "page_id": page_id.as_str(),
            "document_json": document_json,
            "focus": outcome.focus,
            "preview_html": preview_html,
        }))
    }

    async fn execute_document_preview_v2(
        &self,
        parameters: &str,
    ) -> Result<Value, FunnelCommandError> {
        let command = FunnelDocumentPreviewV2Command::from_json(parameters)?;
        let record = self
            .repository
            .get_editor_revision(&command.funnel_id)
            .await?
            .ok_or(FunnelCommandError::FunnelNotFound)?;
        if record.revision.schema_version != 2 {
            return Err(FunnelCommandError::V2ConversionBlocked);
        }
        render_v2::validate_revision_v2(
            record.revision.schema_version,
            &record.funnel_id,
            &record.revision.id,
            &record.revision.data_json,
            &record.revision.render_manifest_json,
        )
        .map_err(|_| FunnelCommandError::Internal)?;
        if record.head_revision_id != command.base_revision_id {
            return Err(FunnelCommandError::StaleEditorRevision);
        }
        let document = DocumentV2::decode(command.document_json.as_bytes(), &command.funnel_id)
            .map_err(|_| FunnelCommandError::InvalidParameters)?;
        let page_id = eez_domain::document_v2::PageId::parse(command.page_id)
            .map_err(|_| FunnelCommandError::InvalidParameters)?;
        let preview_html = render_v2::render_owner_preview(&document, &page_id)
            .map_err(|_| FunnelCommandError::InvalidParameters)?;
        Ok(serde_json::json!({
            "command_id": command_ids::FUNNEL_DOCUMENT_PREVIEW_V2,
            "funnel_id": command.funnel_id,
            "base_revision_id": command.base_revision_id,
            "page_id": page_id.as_str(),
            "preview_html": preview_html,
        }))
    }

    async fn execute_document_stage_v2(
        &self,
        parameters: &str,
        idempotency_key: &str,
    ) -> Result<Value, FunnelCommandError> {
        let command = match FunnelDocumentStageV2Command::from_json(parameters) {
            Ok(command) => command,
            Err(error) => {
                if self
                    .repository
                    .classify_active_receipt(
                        idempotency_key,
                        command_ids::FUNNEL_DOCUMENT_STAGE_V2,
                        None,
                    )
                    .await?
                    .is_some()
                {
                    return Err(FunnelCommandError::IdempotencyPayloadMismatch);
                }
                return Err(error);
            }
        };
        let canonical_parameters =
            serde_json::to_string(&command).map_err(|_| FunnelCommandError::Internal)?;
        let Some(document_stages) = self.document_stages else {
            return Err(FunnelCommandError::Internal);
        };
        let result = document_stages
            .mint(&StageMintMutation {
                funnel_id: &command.funnel_id,
                base_revision_id: &command.base_revision_id,
                expected_length: usize::try_from(command.expected_length)
                    .map_err(|_| FunnelCommandError::InvalidParameters)?,
                expected_sha256: &command.expected_sha256,
                command_id: command_ids::FUNNEL_DOCUMENT_STAGE_V2,
                idempotency_key,
                payload_json: &canonical_parameters,
            })
            .await?;
        match result {
            MutationResult::Stored(result_json) => {
                serde_json::from_str(&result_json).map_err(|_| FunnelCommandError::Internal)
            }
            MutationResult::PayloadMismatch => Err(FunnelCommandError::IdempotencyPayloadMismatch),
        }
    }

    async fn execute_document_save_v2(
        &self,
        parameters: &str,
        idempotency_key: &str,
    ) -> Result<Value, FunnelCommandError> {
        let command = match FunnelDocumentSaveV2Command::from_json(parameters) {
            Ok(command) => command,
            Err(error) => {
                if self
                    .repository
                    .classify_active_receipt(
                        idempotency_key,
                        command_ids::FUNNEL_REVISION_SAVE_V2,
                        None,
                    )
                    .await?
                    .is_some()
                {
                    return Err(FunnelCommandError::IdempotencyPayloadMismatch);
                }
                return Err(error);
            }
        };
        let canonical_parameters =
            serde_json::to_string(&command).map_err(|_| FunnelCommandError::Internal)?;
        if let Some(result) = self
            .repository
            .classify_active_receipt(
                idempotency_key,
                command_ids::FUNNEL_REVISION_SAVE_V2,
                Some(&canonical_parameters),
            )
            .await?
        {
            return mutation_result_to_value(result);
        }
        let Some(document_stages) = self.document_stages else {
            return Err(FunnelCommandError::Internal);
        };
        let document_bytes = document_stages
            .read_for_finalize(
                &command.funnel_id,
                &command.base_revision_id,
                &command.stage_id,
                usize::try_from(command.expected_length)
                    .map_err(|_| FunnelCommandError::InvalidParameters)?,
                &command.expected_sha256,
            )
            .await?;
        let document = DocumentV2::decode(&document_bytes, &command.funnel_id)
            .map_err(|_| FunnelCommandError::InvalidParameters)?;
        let revision_id = uuid::Uuid::new_v4().to_string();
        let prepared = render_v2::prepare_revision(&document, &revision_id)
            .map_err(|_| FunnelCommandError::InvalidParameters)?;
        let revision = NewRevision {
            id: revision_id.clone(),
            funnel_id: command.funnel_id.clone(),
            schema_version: 2,
            data_json: prepared.data_json,
            render_manifest_json: prepared.render_manifest_json,
        };
        let result_json = serde_json::to_string(&serde_json::json!({
            "command_id": command_ids::FUNNEL_REVISION_SAVE_V2,
            "funnel_id": command.funnel_id,
            "base_revision_id": command.base_revision_id,
            "revision_id": revision_id,
            "schema_version": 2,
            "document_sha256": prepared.manifest.document_sha256(),
        }))
        .map_err(|_| FunnelCommandError::Internal)?;
        let result = document_stages
            .save_staged_revision(&StageFinalizeMutation {
                funnel_id: &command.funnel_id,
                base_revision_id: &command.base_revision_id,
                stage_id: &command.stage_id,
                expected_length: usize::try_from(command.expected_length)
                    .map_err(|_| FunnelCommandError::InvalidParameters)?,
                expected_sha256: &command.expected_sha256,
                command_id: command_ids::FUNNEL_REVISION_SAVE_V2,
                idempotency_key,
                payload_json: &canonical_parameters,
                result_json: &result_json,
                revision: &revision,
            })
            .await?;
        mutation_result_to_value(result)
    }

    async fn execute_editor_preview(&self, parameters: &str) -> Result<Value, FunnelCommandError> {
        let command = FunnelEditorOrderCommand::from_json(parameters)?;
        let record = self
            .repository
            .get_owned_revision(&command.funnel_id, &command.base_revision_id)
            .await?
            .ok_or(FunnelCommandError::FunnelNotFound)?;
        if record.schema_version == 2 {
            return Err(FunnelCommandError::V2TypedCommandRequired);
        }
        let validated = decode_snapshot(&record, &command.funnel_id)?;
        let ordered = reorder_snapshot(&validated, &command.section_ids)?;
        let edited = apply_command_settings(&ordered, command.settings.as_ref())?;
        let html = Renderer::new()
            .render_preview(&edited.revision, &edited.theme, &edited.contents)
            .map_err(|_| {
                if command.settings.is_some() {
                    FunnelCommandError::InvalidParameters
                } else {
                    FunnelCommandError::Internal
                }
            })?;
        let settings_values = inspector::inspector_projection(&edited)?.1;
        Ok(serde_json::json!({
            "command_id": command_ids::FUNNEL_EDITOR_PREVIEW,
            "base_revision_id": command.base_revision_id,
            "section_ids": command.section_ids,
            "settings_values": settings_values,
            "preview_html": html,
        }))
    }

    async fn execute_editor_save(
        &self,
        parameters: &str,
        idempotency_key: &str,
    ) -> Result<Value, FunnelCommandError> {
        let command = match FunnelEditorOrderCommand::from_json(parameters) {
            Ok(command) => command,
            Err(error) => {
                if self
                    .repository
                    .classify_active_receipt(
                        idempotency_key,
                        command_ids::FUNNEL_REVISION_SAVE,
                        None,
                    )
                    .await?
                    .is_some()
                {
                    return Err(FunnelCommandError::IdempotencyPayloadMismatch);
                }
                return Err(error);
            }
        };
        let canonical_parameters =
            serde_json::to_string(&command).map_err(|_| FunnelCommandError::Internal)?;
        if let Some(result) = self
            .repository
            .classify_active_receipt(
                idempotency_key,
                command_ids::FUNNEL_REVISION_SAVE,
                Some(&canonical_parameters),
            )
            .await?
        {
            return mutation_result_to_value(result);
        }
        let record = self
            .repository
            .get_owned_revision(&command.funnel_id, &command.base_revision_id)
            .await?
            .ok_or(FunnelCommandError::FunnelNotFound)?;
        if record.schema_version == 2 {
            return Err(FunnelCommandError::V2TypedCommandRequired);
        }
        let validated = decode_snapshot(&record, &command.funnel_id)?;
        let ordered = reorder_snapshot(&validated, &command.section_ids)?;
        let mut ordered = apply_command_settings(&ordered, command.settings.as_ref())?;
        let new_revision_id = uuid::Uuid::new_v4().to_string();
        let revision_id = RevisionId::new(new_revision_id.clone())?;
        let manifest = RenderManifest::new(
            revision_id.clone(),
            ordered.revision.render_manifest().theme_version(),
            ordered.revision.render_manifest().renderer_version(),
            ordered.revision.render_manifest().assets_manifest_hash(),
            ordered.revision.render_manifest().config_hash(),
        )?;
        let sections = ordered
            .revision
            .sections()
            .iter()
            .map(|section| {
                Section::new(
                    Some(section.id().as_str().to_owned()),
                    section.section_type().as_str(),
                    section.order(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let revision = Revision::new(
            revision_id,
            FunnelId::new(command.funnel_id.clone())?,
            Some(i64::from(ordered.revision.schema_version().value())),
            sections,
            manifest.clone(),
        )?;
        Renderer::new().render(&revision, &ordered.theme, &ordered.contents)?;
        ordered.snapshot.sections = revision
            .sections()
            .iter()
            .map(SectionSnapshotV1::from)
            .collect();
        let schema_version = i64::from(revision.schema_version().value());
        ordered.revision = revision;
        let data_json = inspector::encode_snapshot(&ordered.snapshot)?;
        let render_manifest_json = serde_json::to_string(&RenderManifestV1::from(&manifest))
            .map_err(|_| FunnelCommandError::Internal)?;
        let new_revision = NewRevision {
            id: new_revision_id.clone(),
            funnel_id: command.funnel_id.clone(),
            schema_version,
            data_json,
            render_manifest_json,
        };
        let result_json = serde_json::to_string(&serde_json::json!({
            "command_id": command_ids::FUNNEL_REVISION_SAVE,
            "funnel_id": command.funnel_id,
            "revision_id": new_revision_id,
            "section_ids": command.section_ids,
            "settings_values": inspector::inspector_projection(&ordered)?.1,
        }))
        .map_err(|_| FunnelCommandError::Internal)?;
        let result = self
            .repository
            .save_editor_revision(&EditorSaveMutation {
                command_id: command_ids::FUNNEL_REVISION_SAVE,
                funnel_id: &command.funnel_id,
                expected_revision_id: &command.base_revision_id,
                revision: &new_revision,
                idempotency_key,
                payload_json: &canonical_parameters,
                result_json: &result_json,
            })
            .await?;
        mutation_result_to_value(result)
    }

    async fn execute_create(
        &self,
        parameters: &str,
        idempotency_key: &str,
    ) -> Result<Value, FunnelCommandError> {
        let command = match FunnelCreateCommand::from_json(parameters) {
            Ok(command) => command,
            Err(error) => {
                if self
                    .repository
                    .classify_active_receipt(idempotency_key, FunnelCreateCommand::ID, None)
                    .await?
                    .is_some()
                {
                    return Err(FunnelCommandError::IdempotencyPayloadMismatch);
                }
                return Err(error);
            }
        };
        let canonical_parameters =
            serde_json::to_string(&command).map_err(|_| FunnelCommandError::Internal)?;
        if let Some(result) = self
            .repository
            .classify_active_receipt(
                idempotency_key,
                FunnelCreateCommand::ID,
                Some(&canonical_parameters),
            )
            .await?
        {
            return mutation_result_to_value(result);
        }
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
        let result = self
            .repository
            .create_funnel_with_revision(
                &prepared.funnel,
                &prepared.revision,
                FunnelCreateCommand::ID,
                idempotency_key,
                &canonical_parameters,
                &result_json,
            )
            .await?;
        mutation_result_to_value(result)
    }

    async fn execute_create_v2(
        &self,
        parameters: &str,
        idempotency_key: &str,
    ) -> Result<Value, FunnelCommandError> {
        let command = match FunnelCreateCommandV2::from_json(parameters) {
            Ok(command) => command,
            Err(error) => {
                if self
                    .repository
                    .classify_active_receipt(idempotency_key, FunnelCreateCommandV2::ID, None)
                    .await?
                    .is_some()
                {
                    return Err(FunnelCommandError::IdempotencyPayloadMismatch);
                }
                return Err(error);
            }
        };
        let canonical_parameters =
            serde_json::to_string(&command).map_err(|_| FunnelCommandError::Internal)?;
        if let Some(result) = self
            .repository
            .classify_active_receipt(
                idempotency_key,
                FunnelCreateCommandV2::ID,
                Some(&canonical_parameters),
            )
            .await?
        {
            return mutation_result_to_value(result);
        }
        let prepared = command.prepare(
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
        )?;
        let result_json = serde_json::to_string(&serde_json::json!({
            "command_id": FunnelCreateCommandV2::ID,
            "funnel": {
                "id": prepared.funnel.id,
                "name": prepared.funnel.name,
                "slug": prepared.funnel.slug,
                "published_revision_id": null,
            },
            "initial_revision": {
                "id": prepared.revision.id,
                "schema_version": 2,
            },
        }))
        .map_err(|_| FunnelCommandError::Internal)?;
        let result = self
            .repository
            .create_funnel_with_revision(
                &prepared.funnel,
                &prepared.revision,
                FunnelCreateCommandV2::ID,
                idempotency_key,
                &canonical_parameters,
                &result_json,
            )
            .await?;
        mutation_result_to_value(result)
    }

    async fn execute_delete(
        &self,
        parameters: &str,
        idempotency_key: &str,
    ) -> Result<Value, FunnelCommandError> {
        let command = match FunnelDeleteCommand::from_json(parameters) {
            Ok(command) => command,
            Err(error) => {
                if self
                    .repository
                    .classify_active_receipt(idempotency_key, FunnelDeleteCommand::ID, None)
                    .await?
                    .is_some()
                {
                    return Err(FunnelCommandError::IdempotencyPayloadMismatch);
                }
                return Err(error);
            }
        };
        let canonical_parameters =
            serde_json::to_string(&command).map_err(|_| FunnelCommandError::Internal)?;
        if let Some(result) = self
            .repository
            .classify_active_receipt(
                idempotency_key,
                FunnelDeleteCommand::ID,
                Some(&canonical_parameters),
            )
            .await?
        {
            return mutation_result_to_value(result);
        }
        let result = FunnelDeleteResult {
            command_id: FunnelDeleteCommand::ID,
            funnel_id: command.funnel_id(),
            deleted: true,
        };
        let result_json =
            serde_json::to_string(&result).map_err(|_| FunnelCommandError::Internal)?;
        let result = self
            .repository
            .delete_funnel_idempotently(
                command.funnel_id(),
                FunnelDeleteCommand::ID,
                idempotency_key,
                &canonical_parameters,
                &result_json,
            )
            .await?;
        mutation_result_to_value(result)
    }
}

fn mutation_result_to_value(result: MutationResult) -> Result<Value, FunnelCommandError> {
    match result {
        MutationResult::Stored(result) => {
            serde_json::from_str(&result).map_err(|_| FunnelCommandError::Internal)
        }
        MutationResult::PayloadMismatch => Err(FunnelCommandError::IdempotencyPayloadMismatch),
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

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FunnelSnapshotV1 {
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

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FunnelSnapshotV2 {
    format_version: u16,
    schema_version: u32,
    blueprint: FunnelBlueprint,
    theme_id: ThemePreset,
    sections: Vec<SectionSnapshotV1>,
    contents: BTreeMap<String, SectionContent>,
    theme_overrides: ThemeOverrides,
}

#[derive(Deserialize)]
struct SnapshotFormatProbe {
    format_version: u16,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ThemeOverrides {
    pub(super) primary_color: Option<String>,
    pub(super) square_buttons: bool,
}

impl ThemeOverrides {
    fn is_default(&self) -> bool {
        self.primary_color.is_none() && !self.square_buttons
    }
}

#[derive(Clone)]
pub(super) struct SnapshotData {
    pub(super) format_version: u16,
    pub(super) schema_version: u32,
    pub(super) blueprint: FunnelBlueprint,
    pub(super) theme_id: ThemePreset,
    pub(super) sections: Vec<SectionSnapshotV1>,
    pub(super) contents: BTreeMap<String, SectionContent>,
    pub(super) theme_overrides: ThemeOverrides,
}

impl From<FunnelSnapshotV1> for SnapshotData {
    fn from(snapshot: FunnelSnapshotV1) -> Self {
        Self {
            format_version: 1,
            schema_version: snapshot.schema_version,
            blueprint: snapshot.blueprint,
            theme_id: snapshot.theme_id,
            sections: snapshot.sections,
            contents: snapshot.contents,
            theme_overrides: ThemeOverrides::default(),
        }
    }
}

impl From<FunnelSnapshotV2> for SnapshotData {
    fn from(snapshot: FunnelSnapshotV2) -> Self {
        Self {
            format_version: snapshot.format_version,
            schema_version: snapshot.schema_version,
            blueprint: snapshot.blueprint,
            theme_id: snapshot.theme_id,
            sections: snapshot.sections,
            contents: snapshot.contents,
            theme_overrides: snapshot.theme_overrides,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SectionSnapshotV1 {
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
pub(super) struct RenderManifestV1 {
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

#[derive(Clone)]
pub(super) struct ValidatedSnapshot {
    pub(super) snapshot: SnapshotData,
    pub(super) revision: Revision,
    pub(super) theme: ThemeTokens,
    pub(super) contents: BTreeMap<String, SectionContent>,
}

pub(super) fn decode_snapshot(
    record: &eez_storage::repository::RevisionRecord,
    expected_funnel_id: &str,
) -> Result<ValidatedSnapshot, FunnelCommandError> {
    if record.data_json.len() > MAX_SNAPSHOT_BYTES || record.funnel_id != expected_funnel_id {
        return Err(FunnelCommandError::Internal);
    }
    let format_version = serde_json::from_str::<SnapshotFormatProbe>(&record.data_json)
        .map_err(|_| FunnelCommandError::Internal)?
        .format_version;
    let snapshot = match format_version {
        1 => {
            let snapshot: FunnelSnapshotV1 = serde_json::from_str(&record.data_json)
                .map_err(|_| FunnelCommandError::Internal)?;
            SnapshotData::from(snapshot)
        }
        2 => {
            let snapshot: FunnelSnapshotV2 = serde_json::from_str(&record.data_json)
                .map_err(|_| FunnelCommandError::Internal)?;
            SnapshotData::from(snapshot)
        }
        _ => return Err(FunnelCommandError::Internal),
    };
    if snapshot.format_version != format_version as u16
        || snapshot.schema_version != 1
        || i64::from(snapshot.schema_version) != record.schema_version
    {
        return Err(FunnelCommandError::Internal);
    }
    let manifest: RenderManifestV1 = serde_json::from_str(&record.render_manifest_json)
        .map_err(|_| FunnelCommandError::Internal)?;
    if manifest.revision_id != record.id || manifest.assets_manifest_hash != EMPTY_ASSETS_HASH {
        return Err(FunnelCommandError::Internal);
    }
    let revision_id =
        RevisionId::new(record.id.clone()).map_err(|_| FunnelCommandError::Internal)?;
    let funnel_id =
        FunnelId::new(expected_funnel_id.to_owned()).map_err(|_| FunnelCommandError::Internal)?;
    let theme = inspector::effective_theme(snapshot.theme_id, &snapshot.theme_overrides)
        .map_err(|_| FunnelCommandError::Internal)?;
    if manifest.theme_version != theme.version() {
        return Err(FunnelCommandError::Internal);
    }
    let render_manifest = RenderManifest::new(
        revision_id.clone(),
        manifest.theme_version,
        manifest.renderer_version,
        manifest.assets_manifest_hash,
        manifest.config_hash,
    )
    .map_err(|_| FunnelCommandError::Internal)?;
    if let Some(color) = &snapshot.theme_overrides.primary_color {
        if !inspector::is_canonical_hex_color(color) {
            return Err(FunnelCommandError::Internal);
        }
    }
    let sections = snapshot
        .sections
        .iter()
        .map(|section| {
            Section::new(
                Some(section.id.clone()),
                &section.section_type,
                section.order,
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| FunnelCommandError::Internal)?;
    let revision = Revision::new(
        revision_id,
        funnel_id,
        Some(record.schema_version),
        sections,
        render_manifest,
    )
    .map_err(|_| FunnelCommandError::Internal)?;
    let contents = snapshot.contents.clone();
    Renderer::new()
        .render(&revision, &theme, &contents)
        .map_err(|_| FunnelCommandError::Internal)?;
    Ok(ValidatedSnapshot {
        snapshot,
        revision,
        theme,
        contents,
    })
}

pub(crate) fn v2_page_projection(document: &DocumentV2) -> Vec<Value> {
    document
        .pages()
        .iter()
        .map(|page| {
            serde_json::json!({
                "id": page.id().as_str(),
                "slug": page.slug(),
                "title": page.title(),
                "section_count": page.sections().len(),
                "block_count": page.sections().iter().map(|section| section.blocks().len()).sum::<usize>(),
            })
        })
        .collect()
}

pub(crate) fn render_published_revision(
    record: &RevisionRecord,
    expected_funnel_id: &str,
) -> Result<String, FunnelCommandError> {
    match record.schema_version {
        1 => {
            let validated = decode_snapshot(record, expected_funnel_id)?;
            Renderer::new()
                .render(&validated.revision, &validated.theme, &validated.contents)
                .map_err(|_| FunnelCommandError::Internal)
        }
        2 => render_v2::render_published(
            record.schema_version,
            expected_funnel_id,
            &record.id,
            &record.data_json,
            &record.render_manifest_json,
        )
        .map_err(|error| match error {
            render_v2::V2RenderError::ImagesNotEnabled
            | render_v2::V2RenderError::FormsNotEnabled
            | render_v2::V2RenderError::MultiPagePublicRouteUnavailable => {
                FunnelCommandError::V2PublicationBlocked
            }
            _ => FunnelCommandError::Internal,
        }),
        _ => Err(FunnelCommandError::Internal),
    }
}

pub(super) fn reorder_snapshot(
    source: &ValidatedSnapshot,
    section_ids: &[String],
) -> Result<ValidatedSnapshot, FunnelCommandError> {
    if section_ids.len() != source.revision.sections().len() {
        return Err(FunnelCommandError::InvalidParameters);
    }
    let mut seen = HashSet::with_capacity(section_ids.len());
    let mut sections = Vec::with_capacity(section_ids.len());
    for (order, id) in section_ids.iter().enumerate() {
        if !seen.insert(id.as_str()) {
            return Err(FunnelCommandError::DuplicateSectionOrder);
        }
        let Some(existing) = source
            .revision
            .sections()
            .iter()
            .find(|section| section.id().as_str() == id)
        else {
            return Err(FunnelCommandError::InvalidParameters);
        };
        sections.push(
            Section::new(Some(id.clone()), existing.section_type().as_str(), order)
                .map_err(|_| FunnelCommandError::InvalidParameters)?,
        );
    }
    let revision = Revision::new(
        source.revision.id().clone(),
        source.revision.funnel_id().clone(),
        Some(i64::from(source.revision.schema_version().value())),
        sections,
        source.revision.render_manifest().clone(),
    )
    .map_err(|_| FunnelCommandError::InvalidParameters)?;
    let mut reordered = source.clone();
    reordered.snapshot.sections = revision
        .sections()
        .iter()
        .map(SectionSnapshotV1::from)
        .collect();
    reordered.revision = revision;
    Ok(reordered)
}

fn apply_command_settings(
    source: &ValidatedSnapshot,
    settings: Option<&inspector::SettingChanges>,
) -> Result<ValidatedSnapshot, FunnelCommandError> {
    match settings {
        Some(settings) => inspector::apply_editor_changes(source, &settings.changes),
        None => Ok(source.clone()),
    }
}

#[derive(Debug)]
pub enum FunnelCommandError {
    InvalidParameters,
    MissingField,
    MissingIdempotencyKey,
    InvalidIdempotencyKey,
    IdempotencyPayloadMismatch,
    DuplicateSectionOrder,
    StaleEditorRevision,
    V2ConversionBlocked,
    V2PublicationBlocked,
    V2TypedCommandRequired,
    StalePublishedRevision,
    EditorRevisionLimitReached,
    FunnelNotFound,
    RevisionNotFound,
    SlugNotFound,
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
            Self::DuplicateSectionOrder => "validation/order_duplicate",
            Self::StaleEditorRevision => "conflict/edit_stale_revision",
            Self::V2ConversionBlocked => "conflict/v2_conversion_blocked",
            Self::V2PublicationBlocked => "conflict/v2_publication_blocked",
            Self::V2TypedCommandRequired => "conflict/v2_typed_command_required",
            Self::StalePublishedRevision => "conflict/publish_stale_revision",
            Self::EditorRevisionLimitReached => "conflict/revision_limit_reached",
            Self::FunnelNotFound => "not_found/funnel",
            Self::RevisionNotFound => "not_found/revision",
            Self::SlugNotFound => "not_found/slug",
            Self::UnknownCommand => "validation/operation_not_supported",
            Self::Storage(error) => error.code(),
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

pub(super) fn required_idempotency_key(key: Option<&str>) -> Result<&str, FunnelCommandError> {
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

fn is_lower_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_public_slug(slug: &str) -> bool {
    (1..=128).contains(&slug.len())
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !slug.starts_with('-')
        && !slug.ends_with('-')
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
