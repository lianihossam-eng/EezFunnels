#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

use eez_domain::{Revision, Section, SectionType};
use sha2::{Digest, Sha256};
use url::{Host, Url};

pub const RENDERER_VERSION: &str = "renderer-v1";
const MAX_BLOCKS_PER_SECTION: usize = 50;
const MAX_SECTION_URL_BYTES: usize = 2_048;
const MAX_SECTION_CONTENT_BYTES: usize = 1_048_576;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RenderError {
    InvalidTokenName,
    UnsafeTokenValue,
    UnsupportedRendererVersion,
    ThemeVersionMismatch,
    MissingSectionContent,
    UnexpectedSectionContent,
    SectionContentTypeMismatch,
    UnsafeSectionUrl,
    SectionContentLimitExceeded,
}

impl Display for RenderError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidTokenName => "theme token name is invalid",
            Self::UnsafeTokenValue => "theme token value is not a safe CSS token",
            Self::UnsupportedRendererVersion => "render manifest uses an unsupported renderer",
            Self::ThemeVersionMismatch => "theme version does not match the render manifest",
            Self::MissingSectionContent => "revision section has no content",
            Self::UnexpectedSectionContent => "content map contains an unknown section ID",
            Self::SectionContentTypeMismatch => "section content does not match its section type",
            Self::UnsafeSectionUrl => "section URL is not an allowed same-origin path or HTTPS URL",
            Self::SectionContentLimitExceeded => "section content exceeds the byte or item limit",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for RenderError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SectionContent {
    Hero {
        title: String,
        subtitle: String,
        cta_label: String,
        cta_url: String,
        image: Option<HeroImage>,
    },
    Features {
        items: Vec<FeatureItem>,
    },
    Testimonials {
        items: Vec<Testimonial>,
    },
    Cta {
        title: String,
        body: String,
        cta_label: String,
        cta_url: String,
    },
    Footer {
        links: Vec<FooterLink>,
        copyright: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeroImage {
    pub url: String,
    pub alt: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeatureItem {
    pub title: String,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Testimonial {
    pub quote: String,
    pub author: String,
    pub attribution: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FooterLink {
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeTokens {
    version: String,
    values: BTreeMap<String, String>,
}

impl ThemeTokens {
    pub fn new(mut values: BTreeMap<String, String>) -> Result<Self, RenderError> {
        for (name, value) in &mut values {
            if !is_valid_token_name(name) {
                return Err(RenderError::InvalidTokenName);
            }
            let trimmed = value.trim();
            if !is_safe_css_token_value(trimmed) {
                return Err(RenderError::UnsafeTokenValue);
            }
            *value = trimmed.to_owned();
        }

        let version = theme_version(&values);
        Ok(Self { version, values })
    }

    pub fn version(&self) -> &str {
        &self.version
    }
}

fn theme_version(values: &BTreeMap<String, String>) -> String {
    let mut digest = Sha256::new();
    digest.update(b"eez-theme-tokens-v1\0");
    for (name, value) in values {
        digest.update(name.as_bytes());
        digest.update([0]);
        digest.update(value.as_bytes());
        digest.update([0]);
    }

    let mut version = String::from("sha256-v1:");
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest.finalize() {
        version.push(HEX[usize::from(byte >> 4)] as char);
        version.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    version
}

fn is_valid_token_name(name: &str) -> bool {
    let mut characters = name.chars();
    matches!(characters.next(), Some('a'..='z'))
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
        && !name.ends_with('-')
        && !name.contains("--")
}

fn is_safe_css_token_value(value: &str) -> bool {
    if value.is_empty()
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(
                    character,
                    ' ' | '-' | '_' | '.' | ',' | '#' | '%' | '(' | ')' | '/' | '+'
                )
        })
    {
        return false;
    }

    let bytes = value.as_bytes();
    let mut index = 0;
    let mut nesting = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if is_css_identifier_start(byte) {
            let start = index;
            index += 1;
            while index < bytes.len() && is_css_identifier_byte(bytes[index]) {
                index += 1;
            }

            let identifier = &value[start..index];
            if index < bytes.len()
                && bytes[index] == b'('
                && (identifier.eq_ignore_ascii_case("url")
                    || identifier.eq_ignore_ascii_case("expression"))
            {
                return false;
            }
            continue;
        }

        match byte {
            b'(' => match nesting.checked_add(1) {
                Some(depth) => nesting = depth,
                None => return false,
            },
            b')' => {
                if nesting == 0 {
                    return false;
                }
                nesting -= 1;
            }
            _ => {}
        }
        index += 1;
    }
    nesting == 0
}

fn is_css_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'-')
}

fn is_css_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Renderer;

impl Renderer {
    pub const fn new() -> Self {
        Self
    }

    pub fn render(
        &self,
        revision: &Revision,
        theme: &ThemeTokens,
        contents: &BTreeMap<String, SectionContent>,
    ) -> Result<String, RenderError> {
        validate_render_context(revision, theme)?;
        validate_section_contents(revision, contents)?;
        self.render_document(revision, theme, contents)
    }

    fn render_document(
        &self,
        revision: &Revision,
        theme: &ThemeTokens,
        contents: &BTreeMap<String, SectionContent>,
    ) -> Result<String, RenderError> {
        let manifest = revision.render_manifest();

        let mut html = String::from(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Funnel</title></head><body><main id=\"funnel\"",
        );
        html.push_str(" data-revision-id=\"");
        html.push_str(&escape_html_attribute(manifest.revision_id().as_str()));
        html.push_str("\" data-theme-version=\"");
        html.push_str(&escape_html_attribute(manifest.theme_version()));
        html.push_str("\" data-renderer-version=\"");
        html.push_str(RENDERER_VERSION);
        html.push('\"');

        if !theme.values.is_empty() {
            html.push_str(" style=\"");
            for (index, (name, value)) in theme.values.iter().enumerate() {
                if index > 0 {
                    html.push(';');
                }
                html.push_str("--");
                html.push_str(name);
                html.push(':');
                html.push_str(&escape_html_attribute(value));
            }
            html.push('\"');
        }

        html.push('>');
        for section in revision.sections() {
            let content = contents
                .get(section.id().as_str())
                .ok_or(RenderError::MissingSectionContent)?;
            html.push_str(&render_section(section, content));
        }
        html.push_str("</main></body></html>");
        Ok(html)
    }
}

fn validate_render_context(revision: &Revision, theme: &ThemeTokens) -> Result<(), RenderError> {
    let manifest = revision.render_manifest();
    if manifest.renderer_version() != RENDERER_VERSION {
        return Err(RenderError::UnsupportedRendererVersion);
    }
    if manifest.theme_version() != theme.version() {
        return Err(RenderError::ThemeVersionMismatch);
    }
    Ok(())
}

fn validate_section_contents(
    revision: &Revision,
    contents: &BTreeMap<String, SectionContent>,
) -> Result<(), RenderError> {
    let mut content_bytes = 0;
    for section in revision.sections() {
        add_content_bytes(&mut content_bytes, section.id().as_str())?;
        let content = contents
            .get(section.id().as_str())
            .ok_or(RenderError::MissingSectionContent)?;
        if !content_matches_section(section.section_type(), content) {
            return Err(RenderError::SectionContentTypeMismatch);
        }

        match content {
            SectionContent::Hero {
                title,
                subtitle,
                cta_label,
                cta_url,
                image,
            } => {
                add_content_bytes(&mut content_bytes, title)?;
                add_content_bytes(&mut content_bytes, subtitle)?;
                add_content_bytes(&mut content_bytes, cta_label)?;
                add_content_bytes(&mut content_bytes, cta_url)?;
                validate_content_url(cta_url)?;
                if let Some(image) = image {
                    add_content_bytes(&mut content_bytes, &image.url)?;
                    add_content_bytes(&mut content_bytes, &image.alt)?;
                    validate_content_url(&image.url)?;
                }
            }
            SectionContent::Features { items } => {
                validate_item_count(items.len())?;
                for item in items {
                    add_content_bytes(&mut content_bytes, &item.title)?;
                    add_content_bytes(&mut content_bytes, &item.description)?;
                }
            }
            SectionContent::Testimonials { items } => {
                validate_item_count(items.len())?;
                for item in items {
                    add_content_bytes(&mut content_bytes, &item.quote)?;
                    add_content_bytes(&mut content_bytes, &item.author)?;
                    if let Some(attribution) = &item.attribution {
                        add_content_bytes(&mut content_bytes, attribution)?;
                    }
                }
            }
            SectionContent::Cta {
                title,
                body,
                cta_label,
                cta_url,
            } => {
                add_content_bytes(&mut content_bytes, title)?;
                add_content_bytes(&mut content_bytes, body)?;
                add_content_bytes(&mut content_bytes, cta_label)?;
                add_content_bytes(&mut content_bytes, cta_url)?;
                validate_content_url(cta_url)?;
            }
            SectionContent::Footer { links, copyright } => {
                validate_item_count(links.len())?;
                add_content_bytes(&mut content_bytes, copyright)?;
                for link in links {
                    add_content_bytes(&mut content_bytes, &link.label)?;
                    add_content_bytes(&mut content_bytes, &link.url)?;
                    validate_content_url(&link.url)?;
                }
            }
        }
    }

    if contents.len() != revision.sections().len() {
        return Err(RenderError::UnexpectedSectionContent);
    }
    Ok(())
}

fn add_content_bytes(total: &mut usize, value: &str) -> Result<(), RenderError> {
    *total = total
        .checked_add(value.len())
        .filter(|size| *size <= MAX_SECTION_CONTENT_BYTES)
        .ok_or(RenderError::SectionContentLimitExceeded)?;
    Ok(())
}

fn content_matches_section(section_type: SectionType, content: &SectionContent) -> bool {
    matches!(
        (section_type, content),
        (SectionType::Hero, SectionContent::Hero { .. })
            | (SectionType::Features, SectionContent::Features { .. })
            | (
                SectionType::Testimonials,
                SectionContent::Testimonials { .. }
            )
            | (SectionType::Cta, SectionContent::Cta { .. })
            | (SectionType::Footer, SectionContent::Footer { .. })
    )
}

fn validate_item_count(count: usize) -> Result<(), RenderError> {
    if count > MAX_BLOCKS_PER_SECTION {
        Err(RenderError::SectionContentLimitExceeded)
    } else {
        Ok(())
    }
}

fn validate_content_url(value: &str) -> Result<(), RenderError> {
    if is_safe_section_url(value) {
        Ok(())
    } else {
        Err(RenderError::UnsafeSectionUrl)
    }
}

fn is_safe_section_url(value: &str) -> bool {
    if value.len() > MAX_SECTION_URL_BYTES
        || value.chars().any(|character| {
            character.is_control()
                || character.is_whitespace()
                || matches!(character, '\'' | '"' | '<' | '>' | '\\')
        })
        || has_unsafe_percent_escape(value)
    {
        return false;
    }

    if value.starts_with('/') {
        if value.starts_with("//") {
            return false;
        }
        return Url::parse(&format!("https://eez.invalid{value}"))
            .map(|url| {
                url.scheme() == "https"
                    && url.host_str() == Some("eez.invalid")
                    && url.username().is_empty()
                    && url.password().is_none()
            })
            .unwrap_or(false);
    }

    if raw_authority_contains_user_info(value) {
        return false;
    }

    Url::parse(value)
        .map(|url| {
            url.scheme() == "https"
                && url.host().is_some_and(is_valid_https_host)
                && url.username().is_empty()
                && url.password().is_none()
        })
        .unwrap_or(false)
}

fn is_valid_https_host(host: Host<&str>) -> bool {
    match host {
        Host::Ipv4(_) | Host::Ipv6(_) => true,
        Host::Domain(domain) => {
            !domain.is_empty()
                && domain.len() <= 253
                && domain.split('.').all(|label| {
                    !label.is_empty()
                        && label.len() <= 63
                        && !label.starts_with('-')
                        && !label.ends_with('-')
                        && label
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                })
        }
    }
}

fn raw_authority_contains_user_info(value: &str) -> bool {
    let Some((_, remainder)) = value.split_once("://") else {
        return false;
    };
    let authority_end = remainder.find(['/', '?', '#']).unwrap_or(remainder.len());
    remainder[..authority_end].contains('@')
}

fn has_unsafe_percent_escape(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            index += 1;
            continue;
        }

        let Some(high) = bytes.get(index + 1).and_then(|byte| hex_value(*byte)) else {
            return true;
        };
        let Some(low) = bytes.get(index + 2).and_then(|byte| hex_value(*byte)) else {
            return true;
        };
        let decoded = high * 16 + low;
        if decoded <= 0x20 || decoded == 0x7f || matches!(decoded, 0x22 | 0x27 | 0x3c | 0x3e | 0x5c)
        {
            return true;
        }
        index += 3;
    }
    false
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn render_section(section: &Section, content: &SectionContent) -> String {
    let section_type = match section.section_type() {
        SectionType::Hero => "hero",
        SectionType::Features => "features",
        SectionType::Testimonials => "testimonials",
        SectionType::Cta => "cta",
        SectionType::Footer => "footer",
    };
    format!(
        "<section class=\"section section-{section_type}\" data-section-id=\"{}\" data-section-type=\"{section_type}\" data-order=\"{}\">{}</section>",
        escape_html_attribute(section.id().as_str()),
        section.order(),
        render_section_content(content),
    )
}

fn render_section_content(content: &SectionContent) -> String {
    let mut html = String::new();
    match content {
        SectionContent::Hero {
            title,
            subtitle,
            cta_label,
            cta_url,
            image,
        } => {
            html.push_str("<div class=\"hero-content\"><h1>");
            html.push_str(&escape_html_text(title));
            html.push_str("</h1><p>");
            html.push_str(&escape_html_text(subtitle));
            html.push_str("</p><a class=\"button\" href=\"");
            html.push_str(&escape_html_attribute(cta_url));
            html.push_str("\">");
            html.push_str(&escape_html_text(cta_label));
            html.push_str("</a></div>");
            if let Some(image) = image {
                html.push_str("<img class=\"hero-image\" src=\"");
                html.push_str(&escape_html_attribute(&image.url));
                html.push_str("\" alt=\"");
                html.push_str(&escape_html_attribute(&image.alt));
                html.push_str("\" loading=\"lazy\">");
            }
        }
        SectionContent::Features { items } => {
            html.push_str("<div class=\"features-grid\">");
            for item in items {
                html.push_str("<article class=\"feature\"><h2>");
                html.push_str(&escape_html_text(&item.title));
                html.push_str("</h2><p>");
                html.push_str(&escape_html_text(&item.description));
                html.push_str("</p></article>");
            }
            html.push_str("</div>");
        }
        SectionContent::Testimonials { items } => {
            html.push_str("<div class=\"testimonials-list\">");
            for item in items {
                html.push_str("<figure class=\"testimonial\"><blockquote><p>");
                html.push_str(&escape_html_text(&item.quote));
                html.push_str("</p></blockquote><figcaption><cite>");
                html.push_str(&escape_html_text(&item.author));
                html.push_str("</cite>");
                if let Some(attribution) = &item.attribution {
                    html.push_str("<span class=\"testimonial-attribution\">");
                    html.push_str(&escape_html_text(attribution));
                    html.push_str("</span>");
                }
                html.push_str("</figcaption></figure>");
            }
            html.push_str("</div>");
        }
        SectionContent::Cta {
            title,
            body,
            cta_label,
            cta_url,
        } => {
            html.push_str("<div class=\"cta-content\"><h2>");
            html.push_str(&escape_html_text(title));
            html.push_str("</h2><p>");
            html.push_str(&escape_html_text(body));
            html.push_str("</p><a class=\"button\" href=\"");
            html.push_str(&escape_html_attribute(cta_url));
            html.push_str("\">");
            html.push_str(&escape_html_text(cta_label));
            html.push_str("</a></div>");
        }
        SectionContent::Footer { links, copyright } => {
            html.push_str("<nav aria-label=\"Footer\"><ul>");
            for link in links {
                html.push_str("<li><a href=\"");
                html.push_str(&escape_html_attribute(&link.url));
                html.push_str("\">");
                html.push_str(&escape_html_text(&link.label));
                html.push_str("</a></li>");
            }
            html.push_str("</ul></nav><small>");
            html.push_str(&escape_html_text(copyright));
            html.push_str("</small>");
        }
    }
    html
}

fn escape_html_attribute(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            control if control.is_control() => escaped.push_str("&#xfffd;"),
            other => escaped.push(other),
        }
    }
    escaped
}

fn escape_html_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            control if control.is_control() => escaped.push_str("&#xfffd;"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod renderer_tests;

#[cfg(test)]
mod section_content_tests;
