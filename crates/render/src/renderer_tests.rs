use std::collections::{BTreeMap, HashSet};

use super::{content_config_hash, escape_html_attribute, RenderError, Renderer, ThemeTokens};
use crate::test_support::{
    content_map, revision_with_contents, revision_with_sections, revision_with_theme_version,
    revision_with_theme_version_and_contents, safe_theme_tokens,
};
use eez_domain::ThemePreset;
use eez_domain::{FunnelId, RenderManifest, Revision, RevisionId};

#[test]
fn dispatches_each_supported_section_type_in_revision_order() {
    let contents = content_map(&[
        ("footer-id", "footer"),
        ("hero-id", "hero"),
        ("cta-id", "cta"),
        ("features-id", "features"),
        ("testimonials-id", "testimonials"),
    ]);
    let revision = revision_with_contents(
        vec![
            ("footer-id", "footer", 40),
            ("hero-id", "hero", 0),
            ("cta-id", "cta", 30),
            ("features-id", "features", 10),
            ("testimonials-id", "testimonials", 20),
        ],
        "renderer-v1",
        &contents,
    );
    let html = Renderer::new()
        .render(&revision, &safe_theme_tokens(), &contents)
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
fn preview_blocks_navigation_and_remote_images_with_restrictive_policy() {
    let mut contents = content_map(&[
        ("hero-id", "hero"),
        ("cta-id", "cta"),
        ("footer-id", "footer"),
    ]);
    contents.insert(
        "hero-id".to_owned(),
        super::SectionContent::Hero {
            title: "Title".to_owned(),
            subtitle: "Subtitle".to_owned(),
            cta_label: "Continue".to_owned(),
            cta_url: "https://example.com/offer".to_owned(),
            image: Some(super::HeroImage {
                url: "https://example.com/image.png".to_owned(),
                alt: "Offer".to_owned(),
            }),
        },
    );
    let revision = revision_with_contents(
        vec![
            ("hero-id", "hero", 0),
            ("cta-id", "cta", 1),
            ("footer-id", "footer", 2),
        ],
        "renderer-v2",
        &contents,
    );
    let preview = Renderer::new()
        .render_preview(&revision, &safe_theme_tokens(), &contents)
        .expect("preview renders");

    assert!(preview.contains("default-src 'none'"));
    assert!(preview.contains("style-src 'unsafe-inline'"));
    assert!(preview.contains("<span class=\"button\""));
    assert!(preview.contains("<span>Link</span>"));
    assert!(!preview.contains("<a "));
    assert!(!preview.contains("href="));
    assert!(!preview.contains("<img "));
    assert!(!preview.contains("src="));
    assert!(preview.contains("img-src 'none'"));
    assert!(preview.contains("class=\"hero-image-placeholder\""));
    assert!(preview.contains("Image preview: Offer"));
    assert!(preview.contains("https://example.com/image.png"));
}

#[test]
fn hero_image_placeholder_styles_do_not_change_published_renderer_v2_bytes() {
    let mut contents = content_map(&[("hero-id", "hero")]);
    contents.insert(
        "hero-id".to_owned(),
        super::SectionContent::Hero {
            title: "Title".to_owned(),
            subtitle: "Subtitle".to_owned(),
            cta_label: "Go".to_owned(),
            cta_url: "https://example.com/go".to_owned(),
            image: Some(super::HeroImage {
                url: "https://example.com/image.png".to_owned(),
                alt: "Hero".to_owned(),
            }),
        },
    );
    let revision = revision_with_contents(vec![("hero-id", "hero", 0)], "renderer-v2", &contents);
    let theme = safe_theme_tokens();
    let renderer = Renderer::new();
    let published = renderer
        .render(&revision, &theme, &contents)
        .expect("published revision renders");
    let preview = renderer
        .render_preview(&revision, &theme, &contents)
        .expect("preview renders");
    let stylesheet = include_str!("../snapshots/theme.css").trim();
    let expected_published = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>Funnel</title><style>{stylesheet}</style></head><body><main id=\"funnel\" data-revision-id=\"revision-1\" data-theme-version=\"{}\" data-renderer-version=\"renderer-v2\" style=\"--color-primary:#112233;--spacing-section:2rem\"><section class=\"section section-hero\" data-section-id=\"hero-id\" data-section-type=\"hero\" data-order=\"0\"><div class=\"hero-content\"><h1>Title</h1><p>Subtitle</p><a class=\"button\" href=\"https://example.com/go\">Go</a></div><img class=\"hero-image\" src=\"https://example.com/image.png\" alt=\"Hero\" loading=\"lazy\"></section></main></body></html>",
        theme.version()
    );

    assert_eq!(published, expected_published);
    assert!(preview.contains(".hero-image-placeholder{display:grid"));
    assert!(preview.contains("class=\"hero-image-placeholder\""));
}

#[test]
fn injects_theme_tokens_as_sorted_css_custom_properties() {
    let revision = revision_with_sections(Vec::new(), "renderer-v1");
    let html = Renderer::new()
        .render(&revision, &safe_theme_tokens(), &BTreeMap::new())
        .expect("revision renders");

    assert!(html.contains("style=\"--color-primary:#112233;--spacing-section:2rem\""));
    assert!(!html.contains("<style"));
    assert!(html.contains("data-renderer-version=\"renderer-v1\""));
}

#[test]
fn preset_theme_visual_snapshots_match_tokens_and_renderer_stylesheet() {
    let snapshots = [
        (
            ThemePreset::Minimal,
            include_str!("../snapshots/theme-minimal.properties"),
        ),
        (
            ThemePreset::Bold,
            include_str!("../snapshots/theme-bold.properties"),
        ),
        (
            ThemePreset::Elegant,
            include_str!("../snapshots/theme-elegant.properties"),
        ),
        (
            ThemePreset::Playful,
            include_str!("../snapshots/theme-playful.properties"),
        ),
        (
            ThemePreset::Corporate,
            include_str!("../snapshots/theme-corporate.properties"),
        ),
    ];
    let stylesheet = include_str!("../snapshots/theme.css").trim();

    for (preset, expected_properties) in snapshots {
        let theme = ThemeTokens::from_preset(preset).expect("preset CSS tokens are safe");
        let revision =
            revision_with_theme_version(Vec::new(), super::RENDERER_VERSION, theme.version());
        let html = Renderer::new()
            .render(&revision, &theme, &BTreeMap::new())
            .expect("preset theme renders");
        let style_start = html.find(" style=\"").expect("theme style attribute") + 8;
        let style_end = style_start
            + html[style_start..]
                .find('"')
                .expect("theme style attribute end");

        assert_eq!(&html[style_start..style_end], expected_properties.trim());
        assert!(html.contains(&format!("<style>{stylesheet}</style>")));
        assert!(html.contains("data-renderer-version=\"renderer-v2\""));
        assert!(html.contains("background-color:var(--color-background)"));
        assert!(html.contains("border-radius:var(--radius-card)"));
        assert!(html.contains("background-color:var(--color-primary)"));
    }
}

#[test]
fn theme_version_is_bound_to_normalized_token_contents() {
    let red = ThemeTokens::new(BTreeMap::from([(
        "color-primary".to_owned(),
        "red".to_owned(),
    )]))
    .expect("red theme");
    let blue = ThemeTokens::new(BTreeMap::from([(
        "color-primary".to_owned(),
        "blue".to_owned(),
    )]))
    .expect("blue theme");
    let normalized_red = ThemeTokens::new(BTreeMap::from([(
        "color-primary".to_owned(),
        " red ".to_owned(),
    )]))
    .expect("normalized red theme");

    assert_ne!(red.version(), blue.version());
    assert_eq!(red.version(), normalized_red.version());
    assert_eq!(
        red.version(),
        "sha256-v1:1131ad1f19f9832d429f3b62e5603669c8f5ec8102af4d7929437a15a01eb36d"
    );
}

#[test]
fn accepts_css_identifiers_that_contain_the_letters_url() {
    let theme = ThemeTokens::new(BTreeMap::from([(
        "color-primary".to_owned(),
        "burlywood".to_owned(),
    )]))
    .expect("burlywood is a safe CSS identifier");

    assert_eq!(
        theme.values.get("color-primary").map(String::as_str),
        Some("burlywood")
    );
}

#[test]
fn css_function_rejection_requires_a_css_function_token() {
    let theme = ThemeTokens::new(BTreeMap::from([(
        "color-primary".to_owned(),
        "URL (red)".to_owned(),
    )]))
    .expect("whitespace separates the identifier from a CSS function token");

    assert_eq!(
        theme.values.get("color-primary").map(String::as_str),
        Some("URL (red)")
    );
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
        ("color-primary", "url(red)"),
        ("color-primary", "expression(1 + 1)"),
        ("color-primary", "calc(1px + url(red))"),
        ("color-primary", "javascript:alert(1)"),
        ("color-primary", "</style><script>alert(1)</script>"),
        ("color-primary", "red\" onmouseover=\"alert(1)"),
        ("color-primary", "red\nblue"),
    ] {
        let tokens = BTreeMap::from([(name.to_owned(), value.to_owned())]);
        assert!(ThemeTokens::new(tokens).is_err());
    }
}

#[test]
fn trims_line_breaks_at_css_value_boundaries_before_emission() {
    let tokens = ThemeTokens::new(BTreeMap::from([(
        "color-primary".to_owned(),
        "\n red \n".to_owned(),
    )]))
    .expect("outer whitespace is normalized");
    let revision = revision_with_theme_version(Vec::new(), "renderer-v1", tokens.version());
    let html = Renderer::new()
        .render(&revision, &tokens, &BTreeMap::new())
        .expect("normalized token renders");

    assert!(html.contains("style=\"--color-primary:red\""));
    assert!(!html.contains('\n'));
}

#[test]
fn rejects_renderer_and_theme_versions_outside_the_manifest() {
    let wrong_renderer = revision_with_sections(Vec::new(), "renderer-v3");
    assert_eq!(
        Renderer::new()
            .render(&wrong_renderer, &safe_theme_tokens(), &BTreeMap::new())
            .expect_err("unknown renderer version is rejected"),
        RenderError::UnsupportedRendererVersion,
    );

    let wrong_theme = ThemeTokens::new(BTreeMap::new()).expect("empty theme is valid");
    let current_revision = revision_with_sections(Vec::new(), "renderer-v1");
    assert_eq!(
        Renderer::new()
            .render(&current_revision, &wrong_theme, &BTreeMap::new())
            .expect_err("theme version must match the manifest"),
        RenderError::ThemeVersionMismatch,
    );
}

#[test]
fn rendering_is_deterministic_for_the_render_manifest() {
    let theme = safe_theme_tokens();
    let renderer = Renderer::new();

    let contents = content_map(&[("section-a", "hero"), ("section-b", "footer")]);
    let revision = revision_with_contents(
        vec![("section-a", "hero", 0), ("section-b", "footer", 1)],
        "renderer-v1",
        &contents,
    );
    let first = renderer
        .render(&revision, &theme, &contents)
        .expect("first render");
    let second = renderer
        .render(&revision, &theme, &contents)
        .expect("second render");

    assert_eq!(first, second);
    assert!(first.contains("data-revision-id=\"revision-1\""));
    assert!(first.contains(&format!("data-theme-version=\"{}\"", theme.version())));
}

#[test]
fn escapes_untrusted_render_manifest_attribute_values() {
    let revision_id = RevisionId::new("revision\" onfocus=\"alert(1)<script>")
        .expect("revision IDs are non-empty");
    let theme = safe_theme_tokens();
    let manifest = RenderManifest::new(
        revision_id.clone(),
        theme.version(),
        "renderer-v1",
        "assets-hash",
        content_config_hash(&BTreeMap::new()),
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
    let html = Renderer::new()
        .render(&revision, &theme, &BTreeMap::new())
        .expect("manifest values are escaped");

    assert!(
        html.contains("data-revision-id=\"revision&quot; onfocus=&quot;alert(1)&lt;script&gt;\"")
    );
    assert!(html.contains(&format!("data-theme-version=\"{}\"", theme.version())));
    assert!(!html.to_ascii_lowercase().contains("<script"));
}

#[test]
fn rejects_a_manifest_with_a_tampered_theme_version() {
    let theme = safe_theme_tokens();
    let revision = revision_with_theme_version(
        Vec::new(),
        "renderer-v1",
        "sha256-v1:0000000000000000000000000000000000000000000000000000000000000000",
    );

    assert_eq!(
        Renderer::new()
            .render(&revision, &theme, &BTreeMap::new())
            .expect_err("manifest theme digest must match the token contents"),
        RenderError::ThemeVersionMismatch,
    );
}

struct GeneratedAttackCase {
    index: usize,
    id_fragments: u32,
    css_fragments: u32,
    id_sequence: Vec<u8>,
    css_sequence: Vec<u8>,
    id_length: usize,
    css_length: usize,
    id_insert_position: usize,
    css_insert_position: usize,
    id: String,
    css: String,
}

const HTML_ATTACK_FRAGMENTS: [&str; 32] = [
    "\"",
    "'",
    "<",
    ">",
    "&",
    "&quot;",
    "&#34;",
    "&#x22;",
    " ",
    "=",
    "/",
    "script",
    "svg",
    "img",
    "javascript:",
    "onerror=",
    "onload=",
    "onfocus=",
    "onclick=",
    "onmouseover=",
    "\0",
    "\r",
    "\n",
    "é",
    "🧪",
    "&#0;",
    "style=",
    "href=",
    "src=",
    "</main>",
    "<script>",
    "alert(",
];

const CSS_ATTACK_FRAGMENTS: [&str; 32] = [
    "burlywood",
    "url",
    "URL(",
    "expression(",
    "calc(",
    "var(",
    "linear-gradient(",
    "red",
    "blue",
    "(--x)",
    " ",
    "(",
    ")",
    ";",
    ":",
    "/",
    "\\",
    "\"",
    "'",
    "<",
    ">",
    "{",
    "}",
    "javascript",
    "+",
    "1px",
    "--token",
    "/*",
    "*/",
    "%",
    " rgb",
    " onload",
];

fn next_attack_word(state: &mut u64) -> u64 {
    let mut word = *state;
    word ^= word >> 12;
    word ^= word << 25;
    word ^= word >> 27;
    *state = word;
    word.wrapping_mul(0x2545_f491_4f6c_dd1d)
}

fn bounded_index(word: u64, upper_bound: usize) -> usize {
    ((u128::from(word) * upper_bound as u128) >> 64) as usize
}

fn generated_fragment_sequence(state: &mut u64) -> (Vec<u8>, u32) {
    let length = 2 + bounded_index(next_attack_word(state), 8);
    let mut sequence = Vec::with_capacity(length);
    let mut mask = 0u32;
    for _ in 0..length {
        let fragment_index = ((next_attack_word(state) >> 32) & 31) as u8;
        sequence.push(fragment_index);
        mask |= 1u32 << fragment_index;
    }
    (sequence, mask)
}

fn materialize_attack_input(
    fragments: &[&str; 32],
    sequence: &[u8],
    insertion_position: usize,
    marker: &str,
) -> String {
    let capacity = marker.len()
        + sequence
            .iter()
            .map(|index| fragments[*index as usize].len())
            .sum::<usize>();
    let mut value = String::with_capacity(capacity);
    for position in 0..=sequence.len() {
        if position == insertion_position {
            value.push_str(marker);
        }
        if let Some(fragment_index) = sequence.get(position) {
            value.push_str(fragments[*fragment_index as usize]);
        }
    }
    value
}

fn generated_attack_cases() -> Vec<GeneratedAttackCase> {
    let mut state = 0x6a09_e667_f3bc_c909;
    (0..10_000)
        .map(|index| {
            let (id_sequence, id_fragments) = generated_fragment_sequence(&mut state);
            let (css_sequence, css_fragments) = generated_fragment_sequence(&mut state);
            let nonce = next_attack_word(&mut state);
            let marker = format!("x{index:05x}z{nonce:016x}x");
            let id_insert_position =
                bounded_index(next_attack_word(&mut state), id_sequence.len() + 1);
            let css_insert_position =
                bounded_index(next_attack_word(&mut state), css_sequence.len() + 1);
            let id = materialize_attack_input(
                &HTML_ATTACK_FRAGMENTS,
                &id_sequence,
                id_insert_position,
                &marker,
            );
            let css = materialize_attack_input(
                &CSS_ATTACK_FRAGMENTS,
                &css_sequence,
                css_insert_position,
                &marker,
            );
            GeneratedAttackCase {
                index,
                id_fragments,
                css_fragments,
                id_length: id_sequence.len(),
                css_length: css_sequence.len(),
                id_sequence,
                css_sequence,
                id_insert_position,
                css_insert_position,
                id,
                css,
            }
        })
        .collect()
}

#[test]
fn ten_thousand_generated_html_and_css_inputs_cannot_inject_active_markup() {
    let renderer = Renderer::new();
    let mut section_ids = HashSet::new();
    let mut css_values = HashSet::new();
    let mut id_sequences = HashSet::new();
    let mut css_sequences = HashSet::new();
    let mut id_lengths = [false; 8];
    let mut css_lengths = [false; 8];
    let mut id_insert_positions = [false; 10];
    let mut css_insert_positions = [false; 10];
    let mut id_fragment_coverage = 0u32;
    let mut css_fragment_coverage = 0u32;
    let mut accepted_css = 0usize;
    let mut rejected_css = 0usize;

    for case in generated_attack_cases() {
        section_ids.insert(case.id.clone());
        css_values.insert(case.css.clone());
        id_sequences.insert(case.id_sequence.clone());
        css_sequences.insert(case.css_sequence.clone());
        id_lengths[case.id_length - 2] = true;
        css_lengths[case.css_length - 2] = true;
        id_insert_positions[case.id_insert_position] = true;
        css_insert_positions[case.css_insert_position] = true;
        id_fragment_coverage |= case.id_fragments;
        css_fragment_coverage |= case.css_fragments;

        let tokens = BTreeMap::from([("color-primary".to_owned(), case.css.clone())]);
        let theme = match ThemeTokens::new(tokens) {
            Ok(theme) => {
                accepted_css += 1;
                theme
            }
            Err(_) => {
                rejected_css += 1;
                safe_theme_tokens()
            }
        };
        let contents = content_map(&[(case.id.as_str(), "hero")]);
        let revision = revision_with_theme_version_and_contents(
            vec![(case.id.as_str(), "hero", 0)],
            "renderer-v1",
            theme.version(),
            &contents,
        );
        let html = renderer
            .render(&revision, &theme, &contents)
            .expect("hostile ID is escaped");
        let lower = html.to_ascii_lowercase();

        assert!(
            !lower.contains("<script"),
            "script markup for input {}",
            case.index
        );
        assert!(
            !lower.contains("<style"),
            "style tag for input {}",
            case.index
        );
        assert!(
            !lower.contains("<img"),
            "image markup for input {}",
            case.index
        );
        assert!(
            !lower.contains("<svg"),
            "SVG markup for input {}",
            case.index
        );
        assert_eq!(
            html.matches("<section ").count(),
            1,
            "unexpected section attributes for input {}",
            case.index
        );
        let expected_section = format!(
            "<section class=\"section section-hero\" data-section-id=\"{}\" data-section-type=\"hero\" data-order=\"0\"><div class=\"hero-content\"><h1>Title</h1><p>Subtitle</p><a class=\"button\" href=\"https://example.com/go\">Go</a></div></section>",
            escape_html_attribute(&case.id),
        );
        assert!(
            html.contains(&expected_section),
            "section attributes differ from the safe structure for input {}",
            case.index
        );
        assert!(
            !html.contains('\n'),
            "newline escaped its attribute for input {}",
            case.index
        );
        assert!(
            !html.contains('\r'),
            "carriage return escaped its attribute for input {}",
            case.index
        );
        assert!(
            !html.contains('\0'),
            "NUL escaped its attribute for input {}",
            case.index
        );
        if case.id.contains('<') {
            assert!(
                html.contains("&lt;"),
                "hostile tag was not escaped for input {}",
                case.index
            );
        }
    }

    assert_eq!(section_ids.len(), 10_000);
    assert_eq!(css_values.len(), 10_000);
    assert!(
        id_sequences.len() > 9_000,
        "IDs must use structurally varied fragment sequences"
    );
    assert!(
        css_sequences.len() > 9_000,
        "CSS values must use structurally varied fragment sequences"
    );
    assert!(
        id_lengths.into_iter().all(|seen| seen),
        "all ID sequence lengths must occur"
    );
    assert!(
        css_lengths.into_iter().all(|seen| seen),
        "all CSS sequence lengths must occur"
    );
    assert!(
        id_insert_positions.into_iter().all(|seen| seen),
        "all ID marker positions must occur"
    );
    assert!(
        css_insert_positions.into_iter().all(|seen| seen),
        "all CSS marker positions must occur"
    );
    assert_eq!(
        id_fragment_coverage,
        u32::MAX,
        "every HTML attack fragment must occur"
    );
    assert_eq!(
        css_fragment_coverage,
        u32::MAX,
        "every CSS attack fragment must occur"
    );
    assert!(accepted_css > 0);
    assert!(rejected_css > 0);
}
