use std::fmt::{Display, Formatter};

use eez_domain::document_v2::{ActionV2, BlockV2, DocumentV2, PageId, SectionKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const RENDERER_VERSION_V2: &str = "renderer-v4";
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

pub fn prepare_revision(
    document: &DocumentV2,
    revision_id: &str,
) -> Result<PreparedV2Revision, V2RenderError> {
    if document.has_images() {
        return Err(V2RenderError::ImagesNotEnabled);
    }
    let revision_id = eez_domain::document_v2::RevisionIdV2::parse(revision_id.to_owned())
        .map_err(|_| V2RenderError::IdentityMismatch)?;
    let theme_bytes = serde_json_canonicalizer::to_vec(&document.resolved_theme_profile())
        .map_err(|_| V2RenderError::SerializationFailed)?;
    let document_bytes = document.canonical_bytes();
    let assets_bytes = serde_json_canonicalizer::to_vec(&Vec::<AssetDigestRecord>::new())
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
    render_page(document, page_id, LinkMode::Inert)
}

pub fn render_published(
    row_schema_version: i64,
    row_funnel_id: &str,
    row_revision_id: &str,
    data_json: &str,
    render_manifest_json: &str,
) -> Result<String, V2RenderError> {
    render_published_route_with_slug(
        row_schema_version,
        row_funnel_id,
        row_revision_id,
        data_json,
        render_manifest_json,
        None,
        None,
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
        row_schema_version,
        row_funnel_id,
        row_revision_id,
        data_json,
        render_manifest_json,
        Some(public_funnel_slug),
        page_slug,
    )
}

fn render_published_route_with_slug(
    row_schema_version: i64,
    row_funnel_id: &str,
    row_revision_id: &str,
    data_json: &str,
    render_manifest_json: &str,
    public_funnel_slug: Option<&str>,
    page_slug: Option<&str>,
) -> Result<String, V2RenderError> {
    let document = validate_revision_v2(
        row_schema_version,
        row_funnel_id,
        row_revision_id,
        data_json,
        render_manifest_json,
    )?;
    if document.has_images() {
        return Err(V2RenderError::ImagesNotEnabled);
    }
    if document.has_forms() {
        return Err(V2RenderError::FormsNotEnabled);
    }
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
    render_page(
        &document,
        page.id(),
        LinkMode::Published {
            funnel_slug: public_funnel_slug,
        },
    )
}

pub fn validate_revision_v2(
    row_schema_version: i64,
    row_funnel_id: &str,
    row_revision_id: &str,
    data_json: &str,
    render_manifest_json: &str,
) -> Result<DocumentV2, V2RenderError> {
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
    validate_manifest(&document, &manifest, row_funnel_id, row_revision_id)?;
    Ok(document)
}

fn validate_manifest(
    document: &DocumentV2,
    manifest: &RenderManifestV2,
    row_funnel_id: &str,
    row_revision_id: &str,
) -> Result<(), V2RenderError> {
    if manifest.manifest_type != "eezfunnel.render-manifest"
        || manifest.manifest_version != 2
        || manifest.schema_version != 2
        || manifest.revision_id.as_str() != row_revision_id
        || document.funnel_id().as_str() != row_funnel_id
    {
        return Err(V2RenderError::IdentityMismatch);
    }
    if manifest.renderer_version != RENDERER_VERSION_V2
        || manifest.document_validator_version != DOCUMENT_VALIDATOR_VERSION_V2
        || manifest.canonicalizer_profile != CANONICALIZER_PROFILE_V2
        || manifest.theme_profile_version != THEME_PROFILE_VERSION_V2
    {
        return Err(V2RenderError::UnsupportedRenderer);
    }
    let theme_bytes = serde_json_canonicalizer::to_vec(&document.resolved_theme_profile())
        .map_err(|_| V2RenderError::SerializationFailed)?;
    let assets_bytes = serde_json_canonicalizer::to_vec(&Vec::<AssetDigestRecord>::new())
        .map_err(|_| V2RenderError::SerializationFailed)?;
    if manifest.document_sha256 != sha256(document.canonical_bytes())
        || manifest.theme_tokens_sha256 != prefixed_sha256(THEME_DIGEST_DOMAIN, &theme_bytes)
        || manifest.assets_manifest_sha256 != prefixed_sha256(ASSETS_DIGEST_DOMAIN, &assets_bytes)
    {
        return Err(V2RenderError::DigestMismatch);
    }
    Ok(())
}

#[derive(Serialize)]
struct AssetDigestRecord {
    asset_id: String,
    sha256: String,
}

#[derive(Clone, Copy)]
enum LinkMode<'a> {
    Inert,
    Published { funnel_slug: Option<&'a str> },
}

fn render_page(
    document: &DocumentV2,
    page_id: &PageId,
    link_mode: LinkMode<'_>,
) -> Result<String, V2RenderError> {
    if document.has_images() {
        return Err(V2RenderError::ImagesNotEnabled);
    }
    if document.has_forms() {
        return Err(V2RenderError::FormsNotEnabled);
    }
    let page = document.page(page_id).ok_or(V2RenderError::PageMissing)?;
    let profile = document.resolved_theme_profile();
    let mut html = String::with_capacity(document.canonical_bytes().len() * 2);
    html.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>");
    html.push_str(&crate::escape_html_text(page.title()));
    html.push_str("</title><style>");
    html.push_str("html{color-scheme:light}body{margin:0}#funnel{box-sizing:border-box;min-height:100vh;background:var(--color-background);color:var(--color-text);font-family:var(--font-family);font-size:calc(1rem * var(--font-scale));line-height:1.6}#funnel *{box-sizing:inherit}.v2-main{max-width:var(--content-width);margin:0 auto}.v2-section{margin:0 auto;padding:var(--spacing-section) 1.5rem}.v2-section--hero{text-align:center}.v2-section--cta{text-align:center;background:var(--color-surface);border-radius:var(--radius-card)}.v2-section--footer{border-top:1px solid var(--color-border);color:var(--color-muted)}.v2-section h1,.v2-section h2,.v2-section p{margin-top:0}.v2-list{padding-inline-start:1.5rem}.v2-button{display:inline-block;padding:.75rem 1.25rem;background:var(--color-primary);color:var(--color-on-primary);border-radius:var(--radius-button);font-weight:700;text-decoration:none}.v2-text{white-space:pre-wrap;overflow-wrap:anywhere}");
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
    html.push_str("}}</style></head><body><main id=\"funnel\" class=\"v2-main\">");
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
                    html.push_str(&crate::escape_html_text(text));
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
                        if matches!(link_mode, LinkMode::Published { .. }) =>
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
                BlockV2::Image { .. } => return Err(V2RenderError::ImagesNotEnabled),
                BlockV2::Form { .. } => return Err(V2RenderError::FormsNotEnabled),
            }
        }
        html.push_str("</section>");
    }
    html.push_str("</main></body></html>");
    Ok(html)
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
