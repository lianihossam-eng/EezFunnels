use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

use eez_domain::document_v2::{
    ActionV2, BlockV2, DocumentV2, FormField, FormResult, FormV2, PageId, SectionKind,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const RENDERER_VERSION_V2: &str = "renderer-v6";
const PREVIOUS_RENDERER_VERSION_V2: &str = "renderer-v5";
const EARLIER_RENDERER_VERSION_V2: &str = "renderer-v4";
const OLDEST_RENDERER_VERSION_V2: &str = "renderer-v3";
pub const PUBLIC_FORM_SUBMISSION_KEY_MARKER: &str = "EEZ_FORM_SUBMISSION_KEY";
pub const DOCUMENT_VALIDATOR_VERSION_V2: &str = "funnel-document-v2-validator-v1";
pub const CANONICALIZER_PROFILE_V2: &str = "rfc8785-jcs-v1";
pub const THEME_PROFILE_VERSION_V2: &str = "theme-profile-v1";
const THEME_DIGEST_DOMAIN: &[u8] = b"eezfunnel-theme-tokens-v2\0";
const ASSETS_DIGEST_DOMAIN: &[u8] = b"eezfunnel-assets-manifest-v2\0";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum V2RenderError {
    InvalidManifest,
    IdentityMismatch,
    UnsupportedRenderer,
    DigestMismatch,
    PageMissing,
    ImagesNotEnabled,
    FormsNotEnabled,
    SerializationFailed,
}

impl Display for V2RenderError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let code = match self {
            Self::InvalidManifest => "validation/v2_manifest_invalid",
            Self::IdentityMismatch => "validation/v2_identity_mismatch",
            Self::UnsupportedRenderer => "validation/v2_renderer_unsupported",
            Self::DigestMismatch => "validation/v2_digest_mismatch",
            Self::PageMissing => "not_found/page",
            Self::ImagesNotEnabled => "validation/v2_images_not_enabled",
            Self::FormsNotEnabled => "validation/v2_forms_not_enabled",
            Self::SerializationFailed => "internal/unexpected",
        };
        formatter.write_str(code)
    }
}

impl std::error::Error for V2RenderError {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenderManifestV2 {
    manifest_type: String,
    manifest_version: u32,
    schema_version: u32,
    revision_id: eez_domain::document_v2::RevisionIdV2,
    renderer_version: String,
    document_validator_version: String,
    canonicalizer_profile: String,
    theme_profile_version: String,
    theme_tokens_sha256: String,
    document_sha256: String,
    assets_manifest_sha256: String,
}

impl RenderManifestV2 {
    pub fn revision_id(&self) -> &str {
        self.revision_id.as_str()
    }

    pub fn renderer_version(&self) -> &str {
        &self.renderer_version
    }

    pub fn document_sha256(&self) -> &str {
        &self.document_sha256
    }

    pub fn theme_tokens_sha256(&self) -> &str {
        &self.theme_tokens_sha256
    }

    pub fn assets_manifest_sha256(&self) -> &str {
        &self.assets_manifest_sha256
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedV2Revision {
    pub data_json: String,
    pub render_manifest_json: String,
    pub manifest: RenderManifestV2,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AssetDigestRecord {
    pub asset_id: String,
    pub sha256: String,
}

#[derive(Clone, Copy, Debug)]
pub struct PublishedRevisionInput<'a> {
    pub schema_version: i64,
    pub funnel_id: &'a str,
    pub revision_id: &'a str,
    pub data_json: &'a str,
    pub render_manifest_json: &'a str,
}

pub fn prepare_revision(
    document: &DocumentV2,
    revision_id: &str,
) -> Result<PreparedV2Revision, V2RenderError> {
    prepare_revision_with_assets(document, revision_id, &[])
}

pub fn prepare_revision_with_assets(
    document: &DocumentV2,
    revision_id: &str,
    assets: &[AssetDigestRecord],
) -> Result<PreparedV2Revision, V2RenderError> {
    let assets = canonical_asset_records(document, assets)?;
    let revision_id = eez_domain::document_v2::RevisionIdV2::parse(revision_id.to_owned())
        .map_err(|_| V2RenderError::IdentityMismatch)?;
    let theme_bytes = serde_json_canonicalizer::to_vec(&document.resolved_theme_profile())
        .map_err(|_| V2RenderError::SerializationFailed)?;
    let document_bytes = document.canonical_bytes();
    let assets_bytes = serde_json_canonicalizer::to_vec(&assets)
        .map_err(|_| V2RenderError::SerializationFailed)?;
    let manifest = RenderManifestV2 {
        manifest_type: "eezfunnel.render-manifest".to_owned(),
        manifest_version: 2,
        schema_version: 2,
        revision_id,
        renderer_version: RENDERER_VERSION_V2.to_owned(),
        document_validator_version: DOCUMENT_VALIDATOR_VERSION_V2.to_owned(),
        canonicalizer_profile: CANONICALIZER_PROFILE_V2.to_owned(),
        theme_profile_version: THEME_PROFILE_VERSION_V2.to_owned(),
        theme_tokens_sha256: prefixed_sha256(THEME_DIGEST_DOMAIN, &theme_bytes),
        document_sha256: sha256(document_bytes),
        assets_manifest_sha256: prefixed_sha256(ASSETS_DIGEST_DOMAIN, &assets_bytes),
    };
    let manifest_bytes = serde_json_canonicalizer::to_vec(&manifest)
        .map_err(|_| V2RenderError::SerializationFailed)?;
    let manifest_json =
        String::from_utf8(manifest_bytes).map_err(|_| V2RenderError::SerializationFailed)?;
    if manifest_json.len() > 16_384 {
        return Err(V2RenderError::SerializationFailed);
    }
    let data_json = String::from_utf8(document_bytes.to_vec())
        .map_err(|_| V2RenderError::SerializationFailed)?;
    Ok(PreparedV2Revision {
        data_json,
        render_manifest_json: manifest_json,
        manifest,
    })
}

pub fn render_owner_preview(
    document: &DocumentV2,
    page_id: &PageId,
) -> Result<String, V2RenderError> {
    render_owner_preview_with_assets(document, page_id, &[])
}

pub fn render_owner_preview_with_assets(
    document: &DocumentV2,
    page_id: &PageId,
    assets: &[AssetDigestRecord],
) -> Result<String, V2RenderError> {
    let assets = canonical_asset_records(document, assets)?;
    render_page(document, page_id, LinkMode::Inert, &assets)
}

pub fn render_published(
    row_schema_version: i64,
    row_funnel_id: &str,
    row_revision_id: &str,
    data_json: &str,
    render_manifest_json: &str,
) -> Result<String, V2RenderError> {
    render_published_route_with_slug(
        PublishedRevisionInput {
            schema_version: row_schema_version,
            funnel_id: row_funnel_id,
            revision_id: row_revision_id,
            data_json,
            render_manifest_json,
        },
        None,
        None,
        &[],
    )
}

pub fn render_published_route(
    row_schema_version: i64,
    row_funnel_id: &str,
    row_revision_id: &str,
    data_json: &str,
    render_manifest_json: &str,
    public_funnel_slug: &str,
    page_slug: Option<&str>,
) -> Result<String, V2RenderError> {
    render_published_route_with_slug(
        PublishedRevisionInput {
            schema_version: row_schema_version,
            funnel_id: row_funnel_id,
            revision_id: row_revision_id,
            data_json,
            render_manifest_json,
        },
        Some(public_funnel_slug),
        page_slug,
        &[],
    )
}

pub fn render_published_route_with_assets(
    revision: PublishedRevisionInput<'_>,
    public_funnel_slug: &str,
    page_slug: Option<&str>,
    assets: &[AssetDigestRecord],
) -> Result<String, V2RenderError> {
    render_published_route_with_slug(revision, Some(public_funnel_slug), page_slug, assets)
}

fn render_published_route_with_slug(
    revision: PublishedRevisionInput<'_>,
    public_funnel_slug: Option<&str>,
    page_slug: Option<&str>,
    assets: &[AssetDigestRecord],
) -> Result<String, V2RenderError> {
    let (document, manifest) = validate_revision_v2_with_manifest(
        revision.schema_version,
        revision.funnel_id,
        revision.revision_id,
        revision.data_json,
        revision.render_manifest_json,
        assets,
    )?;
    if document.has_forms()
        && !matches!(
            manifest.renderer_version(),
            RENDERER_VERSION_V2 | PREVIOUS_RENDERER_VERSION_V2
        )
    {
        return Err(V2RenderError::FormsNotEnabled);
    }
    let (page, link_mode) = match manifest.renderer_version() {
        OLDEST_RENDERER_VERSION_V2 => {
            if page_slug.is_some() {
                return Err(V2RenderError::PageMissing);
            }
            (
                document
                    .page(document.entry_page_id())
                    .ok_or(V2RenderError::PageMissing)?,
                LinkMode::PublishedV3,
            )
        }
        EARLIER_RENDERER_VERSION_V2 | PREVIOUS_RENDERER_VERSION_V2 | RENDERER_VERSION_V2 => {
            let page = match page_slug {
                Some(slug) => document
                    .pages()
                    .iter()
                    .find(|page| page.slug() == slug)
                    .ok_or(V2RenderError::PageMissing)?,
                None => document
                    .page(document.entry_page_id())
                    .ok_or(V2RenderError::PageMissing)?,
            };
            (
                page,
                LinkMode::Published {
                    funnel_slug: public_funnel_slug,
                },
            )
        }
        _ => return Err(V2RenderError::UnsupportedRenderer),
    };
    let assets = canonical_asset_records(&document, assets)?;
    render_page(&document, page.id(), link_mode, &assets)
}

pub fn validate_revision_v2(
    row_schema_version: i64,
    row_funnel_id: &str,
    row_revision_id: &str,
    data_json: &str,
    render_manifest_json: &str,
) -> Result<DocumentV2, V2RenderError> {
    validate_revision_v2_with_assets(
        row_schema_version,
        row_funnel_id,
        row_revision_id,
        data_json,
        render_manifest_json,
        &[],
    )
}

pub fn validate_revision_v2_with_assets(
    row_schema_version: i64,
    row_funnel_id: &str,
    row_revision_id: &str,
    data_json: &str,
    render_manifest_json: &str,
    assets: &[AssetDigestRecord],
) -> Result<DocumentV2, V2RenderError> {
    validate_revision_v2_with_manifest(
        row_schema_version,
        row_funnel_id,
        row_revision_id,
        data_json,
        render_manifest_json,
        assets,
    )
    .map(|(document, _manifest)| document)
}

fn validate_revision_v2_with_manifest(
    row_schema_version: i64,
    row_funnel_id: &str,
    row_revision_id: &str,
    data_json: &str,
    render_manifest_json: &str,
    assets: &[AssetDigestRecord],
) -> Result<(DocumentV2, RenderManifestV2), V2RenderError> {
    if row_schema_version != 2
        || eez_domain::document_v2::RevisionIdV2::parse(row_revision_id.to_owned()).is_err()
    {
        return Err(V2RenderError::IdentityMismatch);
    }
    let document = DocumentV2::decode(data_json.as_bytes(), row_funnel_id)
        .map_err(|_| V2RenderError::IdentityMismatch)?;
    if document.canonical_bytes() != data_json.as_bytes() {
        return Err(V2RenderError::DigestMismatch);
    }
    let manifest: RenderManifestV2 =
        serde_json::from_str(render_manifest_json).map_err(|_| V2RenderError::InvalidManifest)?;
    let canonical_manifest = serde_json_canonicalizer::to_vec(&manifest)
        .map_err(|_| V2RenderError::SerializationFailed)?;
    if canonical_manifest != render_manifest_json.as_bytes() {
        return Err(V2RenderError::InvalidManifest);
    }
    validate_manifest(&document, &manifest, row_funnel_id, row_revision_id, assets)?;
    Ok((document, manifest))
}

fn validate_manifest(
    document: &DocumentV2,
    manifest: &RenderManifestV2,
    row_funnel_id: &str,
    row_revision_id: &str,
    assets: &[AssetDigestRecord],
) -> Result<(), V2RenderError> {
    if manifest.manifest_type != "eezfunnel.render-manifest"
        || manifest.manifest_version != 2
        || manifest.schema_version != 2
        || manifest.revision_id.as_str() != row_revision_id
        || document.funnel_id().as_str() != row_funnel_id
    {
        return Err(V2RenderError::IdentityMismatch);
    }
    if !matches!(
        manifest.renderer_version.as_str(),
        RENDERER_VERSION_V2
            | PREVIOUS_RENDERER_VERSION_V2
            | EARLIER_RENDERER_VERSION_V2
            | OLDEST_RENDERER_VERSION_V2
    ) || manifest.document_validator_version != DOCUMENT_VALIDATOR_VERSION_V2
        || manifest.canonicalizer_profile != CANONICALIZER_PROFILE_V2
        || manifest.theme_profile_version != THEME_PROFILE_VERSION_V2
    {
        return Err(V2RenderError::UnsupportedRenderer);
    }
    let theme_bytes = serde_json_canonicalizer::to_vec(&document.resolved_theme_profile())
        .map_err(|_| V2RenderError::SerializationFailed)?;
    let assets = canonical_asset_records(document, assets)?;
    let assets_bytes = serde_json_canonicalizer::to_vec(&assets)
        .map_err(|_| V2RenderError::SerializationFailed)?;
    if manifest.document_sha256 != sha256(document.canonical_bytes())
        || manifest.theme_tokens_sha256 != prefixed_sha256(THEME_DIGEST_DOMAIN, &theme_bytes)
        || manifest.assets_manifest_sha256 != prefixed_sha256(ASSETS_DIGEST_DOMAIN, &assets_bytes)
    {
        return Err(V2RenderError::DigestMismatch);
    }
    Ok(())
}

fn canonical_asset_records(
    document: &DocumentV2,
    assets: &[AssetDigestRecord],
) -> Result<Vec<AssetDigestRecord>, V2RenderError> {
    let mut expected = BTreeSet::new();
    for page in document.pages() {
        for section in page.sections() {
            for block in section.blocks() {
                if let BlockV2::Image { asset_id, .. } = block {
                    expected.insert(asset_id.as_str().to_owned());
                }
            }
        }
    }
    if expected.is_empty() && assets.is_empty() {
        return Ok(Vec::new());
    }
    if !expected.is_empty() && assets.is_empty() {
        return Err(V2RenderError::ImagesNotEnabled);
    }
    let mut actual = BTreeMap::new();
    for asset in assets {
        if !is_lower_sha256(&asset.asset_id)
            || !is_lower_sha256(&asset.sha256)
            || asset.asset_id != asset.sha256
            || actual
                .insert(asset.asset_id.as_str(), asset.sha256.as_str())
                .is_some()
        {
            return Err(V2RenderError::InvalidManifest);
        }
    }
    if expected.len() != actual.len()
        || expected
            .iter()
            .any(|asset_id| !actual.contains_key(asset_id.as_str()))
    {
        return Err(V2RenderError::ImagesNotEnabled);
    }
    let mut records = assets.to_vec();
    records.sort_by(|left, right| left.asset_id.cmp(&right.asset_id));
    Ok(records)
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Clone, Copy)]
enum LinkMode<'a> {
    Inert,
    PublishedV3,
    Published { funnel_slug: Option<&'a str> },
}

fn render_page(
    document: &DocumentV2,
    page_id: &PageId,
    link_mode: LinkMode<'_>,
    assets: &[AssetDigestRecord],
) -> Result<String, V2RenderError> {
    let page = document.page(page_id).ok_or(V2RenderError::PageMissing)?;
    let profile = document.resolved_theme_profile();
    let mut html = String::with_capacity(document.canonical_bytes().len() * 2);
    html.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>");
    html.push_str(&crate::escape_html_text(page.title()));
    html.push_str("</title><style>");
    html.push_str("html{color-scheme:light}body{margin:0}#funnel{box-sizing:border-box;min-height:100vh;background:var(--color-background);color:var(--color-text);font-family:var(--font-family);font-size:calc(1rem * var(--font-scale));line-height:1.6}#funnel *{box-sizing:inherit}.v2-main{max-width:var(--content-width);margin:0 auto}.v2-section{margin:0 auto;padding:var(--spacing-section) 1.5rem}.v2-section--hero{text-align:center}.v2-section--cta{text-align:center;background:var(--color-surface);border-radius:var(--radius-card);box-shadow:var(--shadow-card)}.v2-section--footer{border-top:1px solid var(--color-border);color:var(--color-muted)}.v2-section h1,.v2-section h2,.v2-section p{margin-top:0}.v2-list{padding-inline-start:1.5rem}.v2-button{display:inline-block;padding:.75rem 1.25rem;background:var(--color-primary);color:var(--color-on-primary);border-radius:var(--radius-button);font-weight:700;text-decoration:none}.v2-text{white-space:pre-wrap;overflow-wrap:anywhere}.v2-form{display:grid;gap:1.25rem;max-width:40rem;margin:0 auto}.v2-form fieldset{min-width:0;margin:0;padding:0;border:0}.v2-form legend{margin-bottom:1rem;font-size:1.25rem;font-weight:700}.v2-form-field{display:grid;gap:.35rem;margin-block:1rem;text-align:left}.v2-form-field label,.v2-form-consent label{font-weight:600}.v2-form input:not([type=checkbox]),.v2-form select{width:100%;min-height:2.75rem;padding:.6rem .75rem;border:1px solid var(--color-border);border-radius:var(--radius-button);background:var(--color-background);color:var(--color-text);font:inherit}.v2-form input[type=checkbox]{width:1.25rem;height:1.25rem;min-width:1.25rem;min-height:1.25rem;margin:.15rem 0;accent-color:var(--color-primary)}.v2-form :focus-visible,.v2-form-result:focus-visible{outline:3px solid var(--color-primary);outline-offset:3px}.v2-form-help{margin:0;color:var(--color-muted);font-size:.95rem;overflow-wrap:anywhere}.v2-form-consent{display:grid;grid-template-columns:auto minmax(0,1fr);align-items:start;gap:.75rem;margin-block:.75rem;text-align:left}.v2-section .v2-form-result{display:none;max-width:40rem;margin:1.25rem auto 0;padding:1rem;border:1px solid var(--color-border);border-radius:var(--radius-card);background:var(--color-surface);overflow-wrap:anywhere}.v2-form-result:target{display:block}");
    html.push_str(".v2-image{display:block;max-width:100%;height:auto}");
    html.push_str("#funnel{--color-background:");
    html.push_str(&profile.desktop["color-background"]);
    html.push_str(";--color-border:");
    html.push_str(&profile.desktop["color-border"]);
    html.push_str(";--color-muted:");
    html.push_str(&profile.desktop["color-muted"]);
    html.push_str(";--color-on-primary:");
    html.push_str(&profile.desktop["color-on-primary"]);
    html.push_str(";--color-primary:");
    html.push_str(&profile.desktop["color-primary"]);
    html.push_str(";--color-surface:");
    html.push_str(&profile.desktop["color-surface"]);
    html.push_str(";--color-text:");
    html.push_str(&profile.desktop["color-text"]);
    html.push_str(";--font-family:");
    html.push_str(&profile.desktop["font-family"]);
    html.push_str(";--font-scale:");
    html.push_str(&profile.desktop["font-scale"]);
    html.push_str(";--radius-button:");
    html.push_str(&profile.desktop["radius-button"]);
    html.push_str(";--radius-card:");
    html.push_str(&profile.desktop["radius-card"]);
    html.push_str(";--shadow-card:");
    html.push_str(&profile.desktop["shadow-card"]);
    html.push_str(";--spacing-section:");
    html.push_str(&profile.desktop["spacing-section"]);
    html.push_str(";--content-width:");
    html.push_str(&profile.desktop["content-width"]);
    html.push('}');
    html.push_str("@media (min-width:640px) and (max-width:1023px){#funnel{");
    push_profile_variables(&mut html, &profile.tablet);
    html.push_str("}}@media (max-width:639px){#funnel{");
    push_profile_variables(&mut html, &profile.mobile);
    html.push_str("}}</style></head><body><main id=\"funnel\"><div class=\"v2-main\">");
    for section in page.sections() {
        html.push_str("<section class=\"v2-section v2-section--");
        html.push_str(section_kind_name(section.section_type()));
        html.push_str("\" data-section-id=\"");
        html.push_str(&crate::escape_html_attribute(section.id().as_str()));
        html.push_str("\">");
        for block in section.blocks() {
            match block {
                BlockV2::Heading { level, text, .. } => {
                    html.push_str("<h");
                    html.push(char::from(b'0' + *level));
                    html.push('>');
                    html.push_str(&crate::escape_html_text(text));
                    html.push_str("</h");
                    html.push(char::from(b'0' + *level));
                    html.push('>');
                }
                BlockV2::Text { text, .. } => {
                    html.push_str("<p class=\"v2-text\">");
                    html.push_str(&crate::escape_html_text_preserving_whitespace(text));
                    html.push_str("</p>");
                }
                BlockV2::List { items, .. } => {
                    html.push_str("<ul class=\"v2-list\">");
                    for item in items {
                        html.push_str("<li>");
                        html.push_str(&crate::escape_html_text(item));
                        html.push_str("</li>");
                    }
                    html.push_str("</ul>");
                }
                BlockV2::Button { label, action, .. } => match action {
                    ActionV2::ExternalHttps { url }
                        if matches!(
                            link_mode,
                            LinkMode::PublishedV3 | LinkMode::Published { .. }
                        ) =>
                    {
                        html.push_str("<a class=\"v2-button\" href=\"");
                        html.push_str(&crate::escape_html_attribute(url));
                        html.push_str("\" target=\"_blank\" rel=\"noopener noreferrer\">");
                        html.push_str(&crate::escape_html_text(label));
                        html.push_str("</a>");
                    }
                    ActionV2::NavigatePage { page_id } => {
                        if let LinkMode::Published {
                            funnel_slug: Some(funnel_slug),
                        } = link_mode
                        {
                            let target =
                                document.page(page_id).ok_or(V2RenderError::PageMissing)?;
                            html.push_str("<a class=\"v2-button\" href=\"/f/");
                            html.push_str(&crate::escape_html_attribute(funnel_slug));
                            if target.id() != document.entry_page_id() {
                                html.push('/');
                                html.push_str(&crate::escape_html_attribute(target.slug()));
                            }
                            html.push_str("\">");
                            html.push_str(&crate::escape_html_text(label));
                            html.push_str("</a>");
                        } else {
                            html.push_str("<span class=\"v2-button\" aria-disabled=\"true\">");
                            html.push_str(&crate::escape_html_text(label));
                            html.push_str("</span>");
                        }
                    }
                    ActionV2::ExternalHttps { .. } => {
                        html.push_str("<span class=\"v2-button\" aria-disabled=\"true\">");
                        html.push_str(&crate::escape_html_text(label));
                        html.push_str("</span>");
                    }
                },
                BlockV2::Image { asset_id, alt, .. } => {
                    let asset_id = asset_id.as_str();
                    if !assets.iter().any(|asset| asset.asset_id == asset_id) {
                        return Err(V2RenderError::ImagesNotEnabled);
                    }
                    html.push_str("<img class=\"v2-image\" src=\"/media/");
                    html.push_str(&crate::escape_html_attribute(asset_id));
                    html.push_str("\" alt=\"");
                    if let eez_domain::document_v2::ImageAlt::Text { value } = alt {
                        html.push_str(&crate::escape_html_attribute(value));
                    }
                    html.push_str("\" loading=\"lazy\" decoding=\"async\">");
                }
                BlockV2::Form { form_id, .. } => match link_mode {
                    LinkMode::Published {
                        funnel_slug: Some(funnel_slug),
                    } => {
                        let form = document
                            .form(form_id)
                            .ok_or(V2RenderError::IdentityMismatch)?;
                        push_form(&mut html, form, page.id(), Some(funnel_slug));
                    }
                    LinkMode::Inert | LinkMode::Published { funnel_slug: None } => {
                        let form = document
                            .form(form_id)
                            .ok_or(V2RenderError::IdentityMismatch)?;
                        push_form(&mut html, form, page.id(), None);
                    }
                    LinkMode::PublishedV3 => return Err(V2RenderError::FormsNotEnabled),
                },
            }
        }
        html.push_str("</section>");
    }
    html.push_str("</div></main></body></html>");
    Ok(html)
}

fn push_form(html: &mut String, form: &FormV2, page_id: &PageId, funnel_slug: Option<&str>) {
    let form_id = form.id().as_str();
    let form_dom_id = format!("form-{form_id}");
    html.push_str("<form class=\"v2-form\"");
    if let Some(funnel_slug) = funnel_slug {
        let action = format!(
            "/api/public/funnels/{}/pages/{}/forms/{}/submissions?key={PUBLIC_FORM_SUBMISSION_KEY_MARKER}",
            funnel_slug,
            page_id.as_str(),
            form_id,
        );
        html.push_str(" method=\"post\" enctype=\"application/x-www-form-urlencoded\" accept-charset=\"UTF-8\" action=\"");
        html.push_str(&crate::escape_html_attribute(&action));
        html.push('"');
    } else {
        html.push_str(" data-preview=\"true\"");
    }
    html.push_str("><fieldset><legend>");
    html.push_str(&crate::escape_html_text(form.purpose()));
    html.push_str("</legend>");
    for field in form.fields() {
        push_form_field(html, field, &form_dom_id);
    }
    html.push_str("</fieldset>");

    let request_id = format!("{form_dom_id}-request-consent");
    html.push_str("<div class=\"v2-form-consent\"><input type=\"checkbox\" id=\"");
    html.push_str(&crate::escape_html_attribute(&request_id));
    html.push_str("\" name=\"_request_consent\" value=\"on\" required><label for=\"");
    html.push_str(&crate::escape_html_attribute(&request_id));
    html.push_str("\">");
    html.push_str(&crate::escape_html_text(form.request_consent().copy()));
    html.push_str("</label></div>");

    if let Some(consent) = form.marketing_opt_in() {
        let marketing_id = format!("{form_dom_id}-marketing-opt-in");
        html.push_str("<div class=\"v2-form-consent\"><input type=\"checkbox\" id=\"");
        html.push_str(&crate::escape_html_attribute(&marketing_id));
        html.push_str("\" name=\"_marketing_opt_in\" value=\"on\"><label for=\"");
        html.push_str(&crate::escape_html_attribute(&marketing_id));
        html.push_str("\">");
        html.push_str(&crate::escape_html_text(consent.copy()));
        html.push_str("</label></div>");
    }

    if funnel_slug.is_some() {
        html.push_str("<button class=\"v2-button\" type=\"submit\">Send</button></form>");
    } else {
        html.push_str("<button class=\"v2-button\" type=\"button\" aria-disabled=\"true\" title=\"Preview only\">Send</button></form>");
    }
    if let FormResult::Inline { message } = form.result() {
        html.push_str("<p class=\"v2-form-result\" id=\"form-success-");
        html.push_str(&crate::escape_html_attribute(form_id));
        html.push_str("\" role=\"status\" aria-live=\"polite\" tabindex=\"-1\">");
        html.push_str(&crate::escape_html_text(message));
        html.push_str("</p>");
    }
}

fn push_form_field(html: &mut String, field: &FormField, form_dom_id: &str) {
    let (id, name, label, help, required) = match field {
        FormField::Text {
            id,
            name,
            label,
            help,
            required,
            ..
        }
        | FormField::Email {
            id,
            name,
            label,
            help,
            required,
            ..
        }
        | FormField::Phone {
            id,
            name,
            label,
            help,
            required,
            ..
        }
        | FormField::Checkbox {
            id,
            name,
            label,
            help,
            required,
        }
        | FormField::Select {
            id,
            name,
            label,
            help,
            required,
            ..
        } => (id, name, label, help.as_deref(), *required),
    };
    let input_id = format!("{form_dom_id}-field-{}", id.as_str());
    let help_id = format!("{input_id}-help");
    html.push_str("<div class=\"v2-form-field\">");

    match field {
        FormField::Checkbox { .. } => {
            html.push_str("<div class=\"v2-form-consent\"><input type=\"checkbox\" id=\"");
            html.push_str(&crate::escape_html_attribute(&input_id));
            html.push_str("\" name=\"");
            html.push_str(&crate::escape_html_attribute(name));
            html.push_str("\" value=\"on\"");
            push_form_required(html, required);
            push_form_description(html, help, &help_id);
            html.push_str("><label for=\"");
            html.push_str(&crate::escape_html_attribute(&input_id));
            html.push_str("\">");
            html.push_str(&crate::escape_html_text(label));
            html.push_str("</label></div>");
        }
        FormField::Select { options, .. } => {
            push_form_label(html, label, &input_id);
            html.push_str("<select id=\"");
            html.push_str(&crate::escape_html_attribute(&input_id));
            html.push_str("\" name=\"");
            html.push_str(&crate::escape_html_attribute(name));
            html.push('\"');
            push_form_required(html, required);
            push_form_description(html, help, &help_id);
            html.push_str("><option value=\"\">Choose an option</option>");
            for option in options {
                html.push_str("<option value=\"");
                html.push_str(&crate::escape_html_attribute(option));
                html.push_str("\">");
                html.push_str(&crate::escape_html_text(option));
                html.push_str("</option>");
            }
            html.push_str("</select>");
        }
        FormField::Text { validation, .. }
        | FormField::Email { validation, .. }
        | FormField::Phone { validation, .. } => {
            let input_type = match field {
                FormField::Text { .. } => "text",
                FormField::Email { .. } => "email",
                FormField::Phone { .. } => "tel",
                FormField::Checkbox { .. } | FormField::Select { .. } => return,
            };
            push_form_label(html, label, &input_id);
            html.push_str("<input type=\"");
            html.push_str(input_type);
            html.push_str("\" id=\"");
            html.push_str(&crate::escape_html_attribute(&input_id));
            html.push_str("\" name=\"");
            html.push_str(&crate::escape_html_attribute(name));
            html.push_str("\" maxlength=\"");
            let maximum = validation
                .as_ref()
                .and_then(|range| range.max_length)
                .unwrap_or(match field {
                    FormField::Text { .. } => 8192,
                    FormField::Email { .. } => 320,
                    FormField::Phone { .. } => 64,
                    FormField::Checkbox { .. } | FormField::Select { .. } => 0,
                });
            html.push_str(&maximum.to_string());
            html.push('"');
            if let Some(minimum) = validation.as_ref().and_then(|range| range.min_length) {
                html.push_str(" minlength=\"");
                html.push_str(&minimum.to_string());
                html.push('"');
            }
            if matches!(field, FormField::Email { .. }) {
                html.push_str(" autocomplete=\"email\"");
            } else if matches!(field, FormField::Phone { .. }) {
                html.push_str(" autocomplete=\"tel\"");
            }
            push_form_required(html, required);
            push_form_description(html, help, &help_id);
            html.push('>');
        }
    }

    if let Some(help) = help {
        html.push_str("<p class=\"v2-form-help\" id=\"");
        html.push_str(&crate::escape_html_attribute(&help_id));
        html.push_str("\">");
        html.push_str(&crate::escape_html_text(help));
        html.push_str("</p>");
    }
    html.push_str("</div>");
}

fn push_form_label(html: &mut String, label: &str, input_id: &str) {
    html.push_str("<label for=\"");
    html.push_str(&crate::escape_html_attribute(input_id));
    html.push_str("\">");
    html.push_str(&crate::escape_html_text(label));
    html.push_str("</label>");
}

fn push_form_required(html: &mut String, required: bool) {
    if required {
        html.push_str(" required");
    }
}

fn push_form_description(html: &mut String, help: Option<&str>, help_id: &str) {
    if help.is_some() {
        html.push_str(" aria-describedby=\"");
        html.push_str(&crate::escape_html_attribute(help_id));
        html.push('"');
    }
}

fn push_profile_variables(html: &mut String, values: &std::collections::BTreeMap<String, String>) {
    for (name, value) in values {
        html.push_str("--");
        html.push_str(name);
        html.push(':');
        html.push_str(value);
        html.push(';');
    }
}

fn section_kind_name(kind: SectionKind) -> &'static str {
    match kind {
        SectionKind::Hero => "hero",
        SectionKind::Features => "features",
        SectionKind::Testimonials => "testimonials",
        SectionKind::Cta => "cta",
        SectionKind::Footer => "footer",
    }
}

fn prefixed_sha256(domain: &[u8], bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(bytes);
    encode_hex(&digest.finalize())
}

fn sha256(bytes: &[u8]) -> String {
    encode_hex(&Sha256::digest(bytes))
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(result, "{byte:02x}");
    }
    result
}
