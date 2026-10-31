use std::collections::{BTreeMap, BTreeSet};

use eez_domain::document_v2::{
    DocumentV2, IdKind, IdSource, PortableTemplate, PortableTemplateError, TemplateMediaAsset,
};
use eez_domain::{Funnel, FunnelId};
use eez_render::document_v2 as render_v2;
use eez_storage::media::{sha256_hex, MediaAssetError, MediaAssetRecord};
use eez_storage::repository::{MutationResult, NewFunnel, NewRevision};
use serde::{Deserialize, Serialize};

use super::{FunnelCommandError, Session, MAX_FUNNEL_NAME_BYTES};

pub(super) const IMPORT_COMMAND_ID: &str = "funnel.template.import.v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortableTemplateInspection {
    pub name: String,
    pub page_count: usize,
    pub form_count: usize,
    pub media: Vec<PortableTemplateMediaAvailability>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortableTemplateMediaAvailability {
    pub asset: TemplateMediaAsset,
    pub available: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortableTemplateImportResult {
    pub funnel_id: String,
    pub name: String,
    pub slug: String,
    pub revision_id: String,
    pub published: bool,
}

pub(super) async fn export(
    session: &Session<'_, '_>,
    funnel_id: &str,
) -> Result<Vec<u8>, FunnelCommandError> {
    let Some(editor) = session.repository.get_editor_revision(funnel_id).await? else {
        return if session.repository.get_funnel(funnel_id).await?.is_some() {
            Err(FunnelCommandError::PortableTemplate(
                PortableTemplateError::UnsupportedVersion,
            ))
        } else {
            Err(FunnelCommandError::FunnelNotFound)
        };
    };
    if editor.revision.schema_version != 2 {
        return Err(FunnelCommandError::PortableTemplate(
            PortableTemplateError::UnsupportedVersion,
        ));
    }
    let document = render_v2::validate_revision_v2_with_assets(
        editor.revision.schema_version,
        &editor.funnel_id,
        &editor.revision.id,
        &editor.revision.data_json,
        &editor.revision.render_manifest_json,
        &session
            .image_asset_digests_for_revision(&editor.revision, funnel_id)
            .await?,
    )
    .map_err(|_| FunnelCommandError::PortableTemplate(PortableTemplateError::Invalid))?;
    let media = template_media_for_document(session, &document).await?;
    PortableTemplate::from_parts(editor.name, document, media)
        .and_then(|template| template.encode())
        .map_err(FunnelCommandError::PortableTemplate)
}

pub(super) async fn inspect(
    session: &Session<'_, '_>,
    raw_template: &[u8],
) -> Result<PortableTemplateInspection, FunnelCommandError> {
    let template =
        PortableTemplate::decode(raw_template).map_err(FunnelCommandError::PortableTemplate)?;
    let asset_ids = template
        .media()
        .iter()
        .map(|asset| asset.asset_id().as_str().to_owned())
        .collect::<Vec<_>>();
    let available = match session.media_assets {
        Some(repository) => repository
            .get_many_ready_with_objects(&asset_ids)
            .await?
            .into_iter()
            .map(|asset| asset.id)
            .collect::<BTreeSet<_>>(),
        None if asset_ids.is_empty() => BTreeSet::new(),
        None => return Err(FunnelCommandError::Internal),
    };
    let document = template.document();
    Ok(PortableTemplateInspection {
        name: template.name().to_owned(),
        page_count: document.pages().len(),
        form_count: document.forms().len(),
        media: template
            .media()
            .iter()
            .cloned()
            .map(|asset| PortableTemplateMediaAvailability {
                available: available.contains(asset.asset_id().as_str()),
                asset,
            })
            .collect(),
    })
}

pub(super) async fn import(
    session: &Session<'_, '_>,
    name: &str,
    raw_template: &[u8],
    replacements: &BTreeMap<String, String>,
    idempotency_key: &str,
) -> Result<PortableTemplateImportResult, FunnelCommandError> {
    super::required_idempotency_key(Some(idempotency_key))?;
    let name = validate_import_name(name)?;
    let template =
        PortableTemplate::decode(raw_template).map_err(FunnelCommandError::PortableTemplate)?;
    let payload_json = import_payload(&template, name, replacements)?;

    if let Some(result) = session
        .repository
        .template_import_replay(idempotency_key, &payload_json)
        .await?
    {
        return match result {
            MutationResult::Stored(result) => {
                serde_json::from_str(&result).map_err(|_| FunnelCommandError::Internal)
            }
            MutationResult::PayloadMismatch => Err(FunnelCommandError::IdempotencyPayloadMismatch),
        };
    }

    let target_assets = validate_replacement_assets(session, &template, replacements).await?;
    let funnel_id = uuid::Uuid::new_v4().to_string();
    let revision_id = uuid::Uuid::new_v4().to_string();
    let mut ids = RandomTemplateImportIds;
    let document = template
        .instantiate(&funnel_id, replacements, &mut ids)
        .map_err(FunnelCommandError::PortableTemplate)?;
    let digests = target_assets
        .iter()
        .map(|asset| render_v2::AssetDigestRecord {
            asset_id: asset.id.clone(),
            sha256: asset.sha256.clone(),
        })
        .collect::<Vec<_>>();
    let prepared = render_v2::prepare_revision_with_assets(&document, &revision_id, &digests)
        .map_err(|_| FunnelCommandError::PortableTemplate(PortableTemplateError::Invalid))?;
    let slug = super::slug_from_name_and_id(name, &funnel_id);
    let funnel = Funnel::new(FunnelId::new(funnel_id.clone())?, name, &slug)?;
    let new_funnel = NewFunnel {
        id: funnel.id().as_str().to_owned(),
        name: funnel.name().to_owned(),
        slug: funnel.slug().to_owned(),
    };
    let revision = NewRevision {
        id: revision_id.clone(),
        funnel_id: funnel_id.clone(),
        schema_version: 2,
        data_json: prepared.data_json,
        render_manifest_json: prepared.render_manifest_json,
    };
    let outcome = PortableTemplateImportResult {
        funnel_id: new_funnel.id.clone(),
        name: new_funnel.name.clone(),
        slug: new_funnel.slug.clone(),
        revision_id,
        published: false,
    };
    let result_json = serde_json::to_string(&outcome).map_err(|_| FunnelCommandError::Internal)?;
    match session
        .repository
        .create_funnel_with_template_import_v2_revision(
            &new_funnel,
            &revision,
            IMPORT_COMMAND_ID,
            idempotency_key,
            &payload_json,
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

async fn template_media_for_document(
    session: &Session<'_, '_>,
    document: &DocumentV2,
) -> Result<Vec<TemplateMediaAsset>, FunnelCommandError> {
    let ids = render_v2::referenced_asset_ids(document);
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let assets = session
        .media_assets
        .ok_or(FunnelCommandError::Internal)?
        .get_many_ready(&ids)
        .await?;
    if assets.len() != ids.len() {
        return Err(FunnelCommandError::MediaAsset(MediaAssetError::NotFound));
    }
    assets
        .into_iter()
        .map(|asset| {
            TemplateMediaAsset::new(asset.id, asset.sha256, asset.mime, asset.size_bytes)
                .map_err(FunnelCommandError::PortableTemplate)
        })
        .collect()
}

async fn validate_replacement_assets(
    session: &Session<'_, '_>,
    template: &PortableTemplate,
    replacements: &BTreeMap<String, String>,
) -> Result<Vec<MediaAssetRecord>, FunnelCommandError> {
    let source_ids = template
        .media()
        .iter()
        .map(|asset| asset.asset_id().as_str().to_owned())
        .collect::<BTreeSet<_>>();
    if replacements
        .keys()
        .any(|source| !source_ids.contains(source))
    {
        return Err(FunnelCommandError::PortableTemplate(
            PortableTemplateError::MediaMismatch,
        ));
    }
    let target_ids = source_ids
        .iter()
        .map(|source| replacements.get(source).unwrap_or(source).clone())
        .collect::<BTreeSet<_>>();
    let target_count = target_ids.len();
    let assets = session
        .media_assets
        .ok_or(FunnelCommandError::Internal)?
        .get_many_ready_with_objects(&target_ids.into_iter().collect::<Vec<_>>())
        .await?;
    if assets.len() != target_count {
        return Err(FunnelCommandError::TemplateMediaMissing);
    }
    Ok(assets)
}

fn import_payload(
    template: &PortableTemplate,
    name: &str,
    replacements: &BTreeMap<String, String>,
) -> Result<String, FunnelCommandError> {
    let replacements_json =
        serde_json::to_vec(replacements).map_err(|_| FunnelCommandError::Internal)?;
    let payload = serde_json::json!({
        "template_sha256": template.sha256().map_err(FunnelCommandError::PortableTemplate)?,
        "name": name,
        "replacements_sha256": sha256_hex(&replacements_json),
    });
    serde_json::to_string(&payload).map_err(|_| FunnelCommandError::Internal)
}

fn validate_import_name(name: &str) -> Result<&str, FunnelCommandError> {
    let name = name.trim();
    if name.is_empty() || name.len() > MAX_FUNNEL_NAME_BYTES || name.chars().any(char::is_control) {
        return Err(FunnelCommandError::PortableTemplate(
            PortableTemplateError::Invalid,
        ));
    }
    Ok(name)
}

struct RandomTemplateImportIds;

impl IdSource for RandomTemplateImportIds {
    fn next_id(&mut self, kind: IdKind) -> Result<String, eez_domain::document_v2::DocumentError> {
        let prefix = match kind {
            IdKind::Page => "page",
            IdKind::Section => "section",
            IdKind::Block => "block",
            IdKind::Form => "form",
            IdKind::Field => "field",
            IdKind::ConsentCopy => "consent",
        };
        Ok(format!("{prefix}-{}", uuid::Uuid::new_v4().simple()))
    }
}
