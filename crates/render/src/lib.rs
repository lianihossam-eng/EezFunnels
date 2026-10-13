#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

use eez_domain::{Revision, Section, SectionType};

pub const RENDERER_VERSION: &str = "renderer-v1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RenderError {
    InvalidThemeVersion,
    InvalidTokenName,
    UnsafeTokenValue,
    UnsupportedRendererVersion,
    ThemeVersionMismatch,
}

impl Display for RenderError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidThemeVersion => "theme version must not be empty",
            Self::InvalidTokenName => "theme token name is invalid",
            Self::UnsafeTokenValue => "theme token value is not a safe CSS token",
            Self::UnsupportedRendererVersion => "render manifest uses an unsupported renderer",
            Self::ThemeVersionMismatch => "theme version does not match the render manifest",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for RenderError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeTokens {
    version: String,
    values: BTreeMap<String, String>,
}

impl ThemeTokens {
    pub fn new(
        version: impl Into<String>,
        mut values: BTreeMap<String, String>,
    ) -> Result<Self, RenderError> {
        let version = version.into();
        if version.trim().is_empty() {
            return Err(RenderError::InvalidThemeVersion);
        }

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

        Ok(Self { version, values })
    }

    pub fn version(&self) -> &str {
        &self.version
    }
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

    let lowercase = value.to_ascii_lowercase();
    !lowercase.contains("url")
        && !lowercase.contains("expression")
        && !lowercase.contains("javascript")
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Renderer;

impl Renderer {
    pub const fn new() -> Self {
        Self
    }

    pub fn render(&self, revision: &Revision, theme: &ThemeTokens) -> Result<String, RenderError> {
        let manifest = revision.render_manifest();
        if manifest.renderer_version() != RENDERER_VERSION {
            return Err(RenderError::UnsupportedRendererVersion);
        }
        if manifest.theme_version() != theme.version() {
            return Err(RenderError::ThemeVersionMismatch);
        }

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
            html.push_str(&render_section(section));
        }
        html.push_str("</main></body></html>");
        Ok(html)
    }
}

fn render_section(section: &Section) -> String {
    let section_type = match section.section_type() {
        SectionType::Hero => "hero",
        SectionType::Features => "features",
        SectionType::Testimonials => "testimonials",
        SectionType::Cta => "cta",
        SectionType::Footer => "footer",
    };
    format!(
        "<section class=\"section section-{section_type}\" data-section-id=\"{}\" data-section-type=\"{section_type}\" data-order=\"{}\"></section>",
        escape_html_attribute(section.id().as_str()),
        section.order(),
    )
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{RenderError, Renderer, ThemeTokens};
    use eez_domain::{FunnelId, RenderManifest, Revision, RevisionId, Section};

    fn revision_with_sections(
        sections: Vec<(&str, &str, usize)>,
        renderer_version: &str,
    ) -> Revision {
        let revision_id = RevisionId::new("revision-1").expect("valid revision ID");
        let sections = sections
            .into_iter()
            .map(|(id, section_type, order)| {
                Section::new(Some(id.to_owned()), section_type, order).expect("valid section")
            })
            .collect();
        let manifest = RenderManifest::new(
            revision_id.clone(),
            "theme-v1",
            renderer_version,
            "assets-hash",
            "config-hash",
        )
        .expect("valid render manifest");
        Revision::new(
            revision_id,
            FunnelId::new("funnel-1").expect("valid funnel ID"),
            Some(1),
            sections,
            manifest,
        )
        .expect("valid revision")
    }

    fn safe_theme_tokens() -> ThemeTokens {
        ThemeTokens::new(
            "theme-v1",
            BTreeMap::from([
                ("color-primary".to_owned(), "#112233".to_owned()),
                ("spacing-section".to_owned(), "2rem".to_owned()),
            ]),
        )
        .expect("safe theme tokens")
    }

    #[test]
    fn dispatches_each_supported_section_type_in_revision_order() {
        let revision = revision_with_sections(
            vec![
                ("footer-id", "footer", 40),
                ("hero-id", "hero", 0),
                ("cta-id", "cta", 30),
                ("features-id", "features", 10),
                ("testimonials-id", "testimonials", 20),
            ],
            "renderer-v1",
        );
        let html = Renderer::new()
            .render(&revision, &safe_theme_tokens())
            .expect("revision renders");

        let expected_order = [
            "section-hero",
            "section-features",
            "section-testimonials",
            "section-cta",
            "section-footer",
        ];
        let positions: Vec<_> = expected_order
            .iter()
            .map(|class| html.find(class).expect("section type was dispatched"))
            .collect();
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(html.contains("data-section-id=\"hero-id\""));
    }

    #[test]
    fn injects_theme_tokens_as_sorted_css_custom_properties() {
        let revision = revision_with_sections(Vec::new(), "renderer-v1");
        let html = Renderer::new()
            .render(&revision, &safe_theme_tokens())
            .expect("revision renders");

        assert!(html.contains("style=\"--color-primary:#112233;--spacing-section:2rem\""));
        assert!(!html.contains("<style"));
    }

    #[test]
    fn rejects_unsafe_token_names_and_values() {
        for (name, value) in [
            ("color-primary; background", "red"),
            (
                "color-primary",
                "red; background-image:url(https://evil.test/x)",
            ),
            ("color-primary", "url(javascript:alert(1))"),
            ("color-primary", "</style><script>alert(1)</script>"),
            ("color-primary", "red\" onmouseover=\"alert(1)"),
            ("color-primary", "red\nblue"),
        ] {
            let tokens = BTreeMap::from([(name.to_owned(), value.to_owned())]);
            assert!(ThemeTokens::new("theme-v1", tokens).is_err());
        }
    }

    #[test]
    fn trims_line_breaks_at_css_value_boundaries_before_emission() {
        let tokens = ThemeTokens::new(
            "theme-v1",
            BTreeMap::from([("color-primary".to_owned(), "\n red \n".to_owned())]),
        )
        .expect("outer whitespace is normalized");
        let revision = revision_with_sections(Vec::new(), "renderer-v1");
        let html = Renderer::new()
            .render(&revision, &tokens)
            .expect("normalized token renders");

        assert!(html.contains("style=\"--color-primary:red\""));
        assert!(!html.contains('\n'));
    }

    #[test]
    fn rejects_renderer_and_theme_versions_outside_the_manifest() {
        let wrong_renderer = revision_with_sections(Vec::new(), "renderer-v2");
        assert_eq!(
            Renderer::new()
                .render(&wrong_renderer, &safe_theme_tokens())
                .expect_err("unknown renderer version is rejected"),
            RenderError::UnsupportedRendererVersion,
        );

        let wrong_theme =
            ThemeTokens::new("theme-v2", BTreeMap::new()).expect("non-empty theme version");
        let current_revision = revision_with_sections(Vec::new(), "renderer-v1");
        assert_eq!(
            Renderer::new()
                .render(&current_revision, &wrong_theme)
                .expect_err("theme version must match the manifest"),
            RenderError::ThemeVersionMismatch,
        );
    }

    #[test]
    fn rendering_is_deterministic_for_the_render_manifest() {
        let revision = revision_with_sections(
            vec![("section-a", "hero", 0), ("section-b", "footer", 1)],
            "renderer-v1",
        );
        let theme = safe_theme_tokens();
        let renderer = Renderer::new();

        let first = renderer.render(&revision, &theme).expect("first render");
        let second = renderer.render(&revision, &theme).expect("second render");

        assert_eq!(first, second);
        assert!(first.contains("data-revision-id=\"revision-1\""));
        assert!(first.contains("data-theme-version=\"theme-v1\""));
    }

    #[test]
    fn escapes_untrusted_render_manifest_attribute_values() {
        let revision_id = RevisionId::new("revision\" onfocus=\"alert(1)<script>")
            .expect("revision IDs are non-empty");
        let theme_version = "theme\" onmouseover=\"alert(1)<script>";
        let manifest = RenderManifest::new(
            revision_id.clone(),
            theme_version,
            "renderer-v1",
            "assets-hash",
            "config-hash",
        )
        .expect("render manifest");
        let revision = Revision::new(
            revision_id,
            FunnelId::new("funnel-1").expect("valid funnel ID"),
            Some(1),
            Vec::new(),
            manifest,
        )
        .expect("valid revision");
        let theme = ThemeTokens::new(theme_version, BTreeMap::new())
            .expect("theme version is escaped in HTML context");
        let html = Renderer::new()
            .render(&revision, &theme)
            .expect("manifest values are escaped");

        assert!(html
            .contains("data-revision-id=\"revision&quot; onfocus=&quot;alert(1)&lt;script&gt;\""));
        assert!(html.contains(
            "data-theme-version=\"theme&quot; onmouseover=&quot;alert(1)&lt;script&gt;\""
        ));
        assert!(!html.to_ascii_lowercase().contains("<script"));
    }

    #[test]
    fn ten_thousand_untrusted_html_and_css_inputs_cannot_inject_active_markup() {
        let payloads = [
            "\"><script>alert(1)</script>",
            "' autofocus onfocus='alert(1)",
            "<img src=x onerror=alert(1)>",
            "</main><script>document.cookie</script>",
            "javascript:alert(1)",
            "\0\r\n<script>alert(1)</script>",
        ];
        let css_payloads = [
            "</style><script>alert(1)</script>",
            "red; background-image:url(https://evil.test/x)",
            "url(javascript:alert(1))",
            "red\" onmouseover=\"alert(1)",
            "red\nblue",
        ];
        let renderer = Renderer::new();
        let theme = safe_theme_tokens();

        for index in 0..10_000 {
            let id = format!("{}-{index}", payloads[index % payloads.len()]);
            let revision = revision_with_sections(vec![(id.as_str(), "hero", 0)], "renderer-v1");
            let html = renderer
                .render(&revision, &theme)
                .expect("hostile ID is escaped");
            let lower = html.to_ascii_lowercase();

            assert!(
                !lower.contains("<script"),
                "script markup for input {index}"
            );
            assert!(!lower.contains("<style"), "style tag for input {index}");
            assert!(!lower.contains("<img"), "image markup for input {index}");
            assert_eq!(
                html.matches('<').count(),
                15,
                "markup changed for input {index}"
            );
            assert_eq!(
                html.matches('"').count(),
                26,
                "attribute breakout for input {index}"
            );
            assert!(
                !html.contains('\n'),
                "newline escaped its attribute for input {index}"
            );
            assert!(
                !html.contains('\r'),
                "carriage return escaped its attribute for input {index}"
            );
            assert!(
                !html.contains('\0'),
                "NUL escaped its attribute for input {index}"
            );
            if id.contains('<') {
                assert!(
                    html.contains("&lt;"),
                    "hostile tag was not escaped for input {index}"
                );
            }

            let unsafe_value = css_payloads[index % css_payloads.len()].to_owned();
            let tokens = BTreeMap::from([("color-primary".to_owned(), unsafe_value)]);
            assert!(ThemeTokens::new("theme-v1", tokens).is_err());

            let unsafe_name = format!("color-primary\" onerror={index}");
            let tokens = BTreeMap::from([(unsafe_name, "red".to_owned())]);
            assert!(ThemeTokens::new("theme-v1", tokens).is_err());
        }
    }
}
