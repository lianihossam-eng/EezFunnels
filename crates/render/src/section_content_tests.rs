use std::collections::{BTreeMap, HashSet};

use super::{
    escape_html_text, FeatureItem, FooterLink, HeroImage, RenderError, Renderer, SectionContent,
    Testimonial,
};
use crate::test_support::{revision_with_sections, revision_with_theme_version, safe_theme_tokens};

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

fn rendered_section(html: &str) -> &str {
    let start = html.find("<section ").expect("section wrapper exists");
    let remaining = &html[start..];
    let end = remaining
        .find("</section>")
        .expect("section wrapper closes")
        + "</section>".len();
    &remaining[..end]
}

fn render_one_section(id: &str, section_type: &str, content: SectionContent) -> String {
    let revision = revision_with_sections(vec![(id, section_type, 0)], "renderer-v1");
    let theme = safe_theme_tokens();
    let contents = BTreeMap::from([(id.to_owned(), content)]);
    Renderer::new()
        .render(&revision, &theme, &contents)
        .expect("matching content renders")
}

fn empty_hero_content() -> SectionContent {
    SectionContent::Hero {
        title: "".to_owned(),
        subtitle: "".to_owned(),
        cta_label: "".to_owned(),
        cta_url: "https://example.com/".to_owned(),
        image: None,
    }
}

#[test]
fn hero_section_snapshot_renders_content_and_optional_image() {
    let html = render_one_section(
        "hero-1",
        "hero",
        SectionContent::Hero {
            title: "Start here".to_owned(),
            subtitle: "Make it simple".to_owned(),
            cta_label: "Join".to_owned(),
            cta_url: "https://example.com/start".to_owned(),
            image: Some(HeroImage {
                url: "https://cdn.example.com/hero.png".to_owned(),
                alt: "A bright <future>".to_owned(),
            }),
        },
    );

    assert_eq!(
            rendered_section(&html),
            "<section class=\"section section-hero\" data-section-id=\"hero-1\" data-section-type=\"hero\" data-order=\"0\"><div class=\"hero-content\"><h1>Start here</h1><p>Make it simple</p><a class=\"button\" href=\"https://example.com/start\">Join</a></div><img class=\"hero-image\" src=\"https://cdn.example.com/hero.png\" alt=\"A bright &lt;future&gt;\" loading=\"lazy\"></section>"
        );
}

#[test]
fn features_section_snapshot_renders_a_grid() {
    let html = render_one_section(
        "features-1",
        "features",
        SectionContent::Features {
            items: vec![FeatureItem {
                title: "Fast".to_owned(),
                description: "Ready today".to_owned(),
            }],
        },
    );

    assert_eq!(
            rendered_section(&html),
            "<section class=\"section section-features\" data-section-id=\"features-1\" data-section-type=\"features\" data-order=\"0\"><div class=\"features-grid\"><article class=\"feature\"><h2>Fast</h2><p>Ready today</p></article></div></section>"
        );
}

#[test]
fn testimonials_section_snapshot_renders_a_list() {
    let html = render_one_section(
        "testimonial-1",
        "testimonials",
        SectionContent::Testimonials {
            items: vec![Testimonial {
                quote: "It works.".to_owned(),
                author: "Sam".to_owned(),
                attribution: Some("Founder".to_owned()),
            }],
        },
    );

    assert_eq!(
            rendered_section(&html),
            "<section class=\"section section-testimonials\" data-section-id=\"testimonial-1\" data-section-type=\"testimonials\" data-order=\"0\"><div class=\"testimonials-list\"><figure class=\"testimonial\"><blockquote><p>It works.</p></blockquote><figcaption><cite>Sam</cite><span class=\"testimonial-attribution\">Founder</span></figcaption></figure></div></section>"
        );
}

#[test]
fn cta_section_snapshot_renders_a_button() {
    let html = render_one_section(
        "cta-1",
        "cta",
        SectionContent::Cta {
            title: "Ready?".to_owned(),
            body: "Start in a minute.".to_owned(),
            cta_label: "Begin".to_owned(),
            cta_url: "https://example.com/begin".to_owned(),
        },
    );

    assert_eq!(
            rendered_section(&html),
            "<section class=\"section section-cta\" data-section-id=\"cta-1\" data-section-type=\"cta\" data-order=\"0\"><div class=\"cta-content\"><h2>Ready?</h2><p>Start in a minute.</p><a class=\"button\" href=\"https://example.com/begin\">Begin</a></div></section>"
        );
}

#[test]
fn footer_section_snapshot_renders_links_and_copyright() {
    let html = render_one_section(
        "footer-1",
        "footer",
        SectionContent::Footer {
            links: vec![FooterLink {
                label: "Privacy".to_owned(),
                url: "https://example.com/privacy".to_owned(),
            }],
            copyright: "© 2026 Eez Funnel".to_owned(),
        },
    );

    assert_eq!(
            rendered_section(&html),
            "<section class=\"section section-footer\" data-section-id=\"footer-1\" data-section-type=\"footer\" data-order=\"0\"><nav aria-label=\"Footer\"><ul><li><a href=\"https://example.com/privacy\">Privacy</a></li></ul></nav><small>© 2026 Eez Funnel</small></section>"
        );
}

#[test]
fn content_text_escapes_markup_delimiters_and_replaces_controls() {
    let payload = "<script onerror=\"run()\">alert('&')</script>\0\n";
    let html = render_one_section(
        "hero-xss",
        "hero",
        SectionContent::Hero {
            title: payload.to_owned(),
            subtitle: "".to_owned(),
            cta_label: "Continue".to_owned(),
            cta_url: "https://example.com/continue".to_owned(),
            image: None,
        },
    );

    assert!(html.contains(&escape_html_text(payload)));
    assert!(!html.to_ascii_lowercase().contains("<script"));
    assert!(!html.contains('\0'));
    assert!(!html.contains('\n'));
    assert_eq!(html.matches("<section ").count(), 1);
}

#[test]
fn content_requires_exact_id_coverage_and_matching_section_types() {
    let revision = revision_with_sections(vec![("hero-1", "hero", 0)], "renderer-v1");
    let theme = safe_theme_tokens();

    assert_eq!(
        Renderer::new()
            .render(&revision, &theme, &BTreeMap::new())
            .expect_err("missing content must fail closed"),
        RenderError::MissingSectionContent,
    );

    let unexpected = BTreeMap::from([
        ("hero-1".to_owned(), empty_hero_content()),
        ("unreferenced".to_owned(), empty_hero_content()),
    ]);
    assert_eq!(
        Renderer::new()
            .render(&revision, &theme, &unexpected)
            .expect_err("extra content must fail closed"),
        RenderError::UnexpectedSectionContent,
    );

    let mismatched = BTreeMap::from([(
        "hero-1".to_owned(),
        SectionContent::Features { items: Vec::new() },
    )]);
    assert_eq!(
        Renderer::new()
            .render(&revision, &theme, &mismatched)
            .expect_err("content for a different section type must fail closed"),
        RenderError::SectionContentTypeMismatch,
    );
}

#[test]
fn section_content_rejects_urls_outside_same_origin_or_https_policy() {
    for url in [
        "javascript:alert(1)",
        "data:text/html,<script>alert(1)</script>",
        "http://example.com/image.png",
        "//evil.example/image.png",
        "https://user@example.com/path",
        "https://example.com/with space",
        "https://example.com\\@evil.test/path",
        "https://example.com/\" onerror=\"alert(1)",
        "https://example.com/%0aheader",
    ] {
        let revision = revision_with_sections(vec![("hero-url", "hero", 0)], "renderer-v1");
        let theme = safe_theme_tokens();
        let content = BTreeMap::from([(
            "hero-url".to_owned(),
            SectionContent::Hero {
                title: "Safe title".to_owned(),
                subtitle: "Safe subtitle".to_owned(),
                cta_label: "Continue".to_owned(),
                cta_url: "https://example.com/continue".to_owned(),
                image: Some(HeroImage {
                    url: url.to_owned(),
                    alt: "Image".to_owned(),
                }),
            },
        )]);

        assert_eq!(
            Renderer::new()
                .render(&revision, &theme, &content)
                .expect_err("unsafe image URL must be rejected"),
            RenderError::UnsafeSectionUrl,
            "URL should be rejected: {url}"
        );
    }
}

#[test]
fn section_content_accepts_same_worker_root_relative_paths() {
    let cases = [
        (
            "hero-root-path",
            "hero",
            SectionContent::Hero {
                title: "Title".to_owned(),
                subtitle: "Subtitle".to_owned(),
                cta_label: "Go".to_owned(),
                cta_url: "/start".to_owned(),
                image: Some(HeroImage {
                    url: "/assets/hero-image".to_owned(),
                    alt: "Hero".to_owned(),
                }),
            },
            ["href=\"/start\"", "src=\"/assets/hero-image\""],
        ),
        (
            "cta-root-path",
            "cta",
            SectionContent::Cta {
                title: "Title".to_owned(),
                body: "Body".to_owned(),
                cta_label: "Go".to_owned(),
                cta_url: "/start".to_owned(),
            },
            ["href=\"/start\"", ""],
        ),
        (
            "footer-root-path",
            "footer",
            SectionContent::Footer {
                links: vec![FooterLink {
                    label: "Privacy".to_owned(),
                    url: "/privacy".to_owned(),
                }],
                copyright: "Copyright".to_owned(),
            },
            ["href=\"/privacy\"", ""],
        ),
    ];

    for (id, section_type, content, expected_attributes) in cases {
        let html = render_one_section(id, section_type, content);
        for expected_attribute in expected_attributes {
            if !expected_attribute.is_empty() {
                assert!(html.contains(expected_attribute));
            }
        }
    }
}

#[test]
fn section_content_enforces_an_aggregate_one_mib_utf8_budget() {
    let revision = revision_with_sections(vec![("hero-budget", "hero", 0)], "renderer-v1");
    let theme = safe_theme_tokens();
    let contents = BTreeMap::from([(
        "hero-budget".to_owned(),
        SectionContent::Hero {
            title: "t".repeat(600_000),
            subtitle: "s".repeat(450_000),
            cta_label: "Go".to_owned(),
            cta_url: "https://example.com/start".to_owned(),
            image: None,
        },
    )]);

    assert_eq!(
        Renderer::new()
            .render(&revision, &theme, &contents)
            .expect_err("aggregate UTF-8 content above C-01's limit must be rejected"),
        RenderError::SectionContentLimitExceeded,
    );
}

#[test]
fn render_context_errors_precede_content_validation_errors() {
    let theme = safe_theme_tokens();
    let unsupported_renderer =
        revision_with_theme_version(vec![("hero-1", "hero", 0)], "renderer-v2", "theme-v1");
    assert_eq!(
        Renderer::new()
            .render(&unsupported_renderer, &theme, &BTreeMap::new())
            .expect_err("renderer version must be checked before content coverage"),
        RenderError::UnsupportedRendererVersion,
    );

    let wrong_theme =
        revision_with_theme_version(vec![("hero-1", "hero", 0)], "renderer-v1", "other");
    assert_eq!(
        Renderer::new()
            .render(&wrong_theme, &theme, &BTreeMap::new())
            .expect_err("theme digest must be checked before content coverage"),
        RenderError::ThemeVersionMismatch,
    );
}

#[test]
fn content_lists_are_bounded_by_c01_max_blocks_per_section() {
    let items = (0..51)
        .map(|index| FeatureItem {
            title: format!("Feature {index}"),
            description: "Description".to_owned(),
        })
        .collect();
    let revision = revision_with_sections(vec![("features-1", "features", 0)], "renderer-v1");
    let theme = safe_theme_tokens();
    let contents = BTreeMap::from([("features-1".to_owned(), SectionContent::Features { items })]);

    assert_eq!(
        Renderer::new()
            .render(&revision, &theme, &contents)
            .expect_err("more than 50 content blocks must be rejected"),
        RenderError::SectionContentLimitExceeded,
    );
}

#[test]
fn ten_thousand_generated_section_text_values_cannot_inject_markup() {
    const FRAGMENTS: [&str; 16] = [
        "<script>",
        "</section>",
        "\" onerror=\"",
        "&quot;",
        "\0",
        "\r",
        "\n",
        "🧪",
        "&",
        ">",
        "'",
        "onclick=",
        "javascript:",
        "<img src=x>",
        "&lt;svg&gt;",
        " ",
    ];
    let mut state = 0xbb67_ae85_84ca_a73b;
    let revision = revision_with_sections(vec![("hero-fuzz", "hero", 0)], "renderer-v1");
    let theme = safe_theme_tokens();
    let renderer = Renderer::new();

    for index in 0..10_000 {
        let length = 2 + bounded_index(next_attack_word(&mut state), 7);
        let mut payload = String::new();
        for _ in 0..length {
            let fragment = ((next_attack_word(&mut state) >> 32) & 15) as usize;
            payload.push_str(FRAGMENTS[fragment]);
        }
        payload.push_str(&format!("marker-{index:05x}"));
        let content = BTreeMap::from([(
            "hero-fuzz".to_owned(),
            SectionContent::Hero {
                title: payload.clone(),
                subtitle: "safe".to_owned(),
                cta_label: "Continue".to_owned(),
                cta_url: format!("https://example.com/path/{index}"),
                image: None,
            },
        )]);
        let html = renderer
            .render(&revision, &theme, &content)
            .expect("hostile content text must be escaped");

        assert!(!html.to_ascii_lowercase().contains("<script"));
        assert!(!html.to_ascii_lowercase().contains("<img"));
        let expected_section = format!(
                "<section class=\"section section-hero\" data-section-id=\"hero-fuzz\" data-section-type=\"hero\" data-order=\"0\"><div class=\"hero-content\"><h1>{}</h1><p>safe</p><a class=\"button\" href=\"https://example.com/path/{index}\">Continue</a></div></section>",
                escape_html_text(&payload),
            );
        assert!(
            html.contains(&expected_section),
            "rendered content must remain text inside the fixed section structure"
        );
        assert!(!html.contains('\0'));
        assert!(!html.contains('\r'));
        assert!(!html.contains('\n'));
    }
}

#[test]
fn every_url_sink_rejects_unsafe_schemes() {
    let unsafe_url = "javascript:alert(1)";
    let cases = [
        (
            "hero-button",
            "hero",
            SectionContent::Hero {
                title: "Title".to_owned(),
                subtitle: "Subtitle".to_owned(),
                cta_label: "Go".to_owned(),
                cta_url: unsafe_url.to_owned(),
                image: None,
            },
        ),
        (
            "hero-image",
            "hero",
            SectionContent::Hero {
                title: "Title".to_owned(),
                subtitle: "Subtitle".to_owned(),
                cta_label: "Go".to_owned(),
                cta_url: "https://example.com/go".to_owned(),
                image: Some(HeroImage {
                    url: unsafe_url.to_owned(),
                    alt: "Alt".to_owned(),
                }),
            },
        ),
        (
            "cta-button",
            "cta",
            SectionContent::Cta {
                title: "Title".to_owned(),
                body: "Body".to_owned(),
                cta_label: "Go".to_owned(),
                cta_url: unsafe_url.to_owned(),
            },
        ),
        (
            "footer-link",
            "footer",
            SectionContent::Footer {
                links: vec![FooterLink {
                    label: "Link".to_owned(),
                    url: unsafe_url.to_owned(),
                }],
                copyright: "Copyright".to_owned(),
            },
        ),
    ];

    for (id, section_type, content) in cases {
        let revision = revision_with_sections(vec![(id, section_type, 0)], "renderer-v1");
        let theme = safe_theme_tokens();
        let contents = BTreeMap::from([(id.to_owned(), content)]);
        assert_eq!(
            Renderer::new()
                .render(&revision, &theme, &contents)
                .expect_err("every URL-bearing field must fail closed"),
            RenderError::UnsafeSectionUrl,
            "unsafe URL was accepted for section {id}"
        );
    }
}

#[test]
fn ten_thousand_generated_urls_fail_closed_across_url_boundaries() {
    const BAD_SCHEMES: [&str; 5] = ["javascript:", "data:text/html,", "http://", "file://", "//"];
    const BAD_AUTHORITIES: [&str; 5] = [
        "bad host",
        "example.com@evil.test",
        "example..com",
        ":443",
        "[invalid",
    ];
    const UNSAFE_PERCENT: [&str; 4] = ["%0a", "%22", "%5c", "%zz"];
    const RAW_DELIMITERS: [&str; 6] = [" ", "\n", "\r", "\0", "\\", "\" onload=\""];

    let mut state = 0x3c6e_f372_fe94_f82b;
    let mut generated = HashSet::new();
    let mut categories = [false; 6];
    let revision = revision_with_sections(vec![("url-fuzz", "hero", 0)], "renderer-v1");
    let theme = safe_theme_tokens();
    let renderer = Renderer::new();

    for index in 0..10_000 {
        let category = bounded_index(next_attack_word(&mut state), 6);
        let nonce = format!("case-{index:05x}");
        let url = match category {
            0 => {
                let scheme =
                    BAD_SCHEMES[bounded_index(next_attack_word(&mut state), BAD_SCHEMES.len())];
                format!("{scheme}example.com/{nonce}")
            }
            1 => {
                let delimiter = RAW_DELIMITERS
                    [bounded_index(next_attack_word(&mut state), RAW_DELIMITERS.len())];
                format!("https://example.com/{nonce}{delimiter}tail")
            }
            2 => {
                let escape = UNSAFE_PERCENT
                    [bounded_index(next_attack_word(&mut state), UNSAFE_PERCENT.len())];
                format!("https://example.com/{nonce}{escape}tail")
            }
            3 => {
                let authority = BAD_AUTHORITIES
                    [bounded_index(next_attack_word(&mut state), BAD_AUTHORITIES.len())];
                format!("https://{authority}/{nonce}")
            }
            4 => {
                let user = format!("user-{nonce}");
                let host = format!("host-{}.example.com", next_attack_word(&mut state));
                format!("https://{user}@{host}/{nonce}")
            }
            _ => {
                let root_path_attacks = [
                    "//evil.example/path",
                    "/\\\\evil.example/path",
                    "/%0aheader",
                    "/%22 onerror=\"alert(1)",
                    "/<script>alert(1)</script>",
                ];
                let attack = root_path_attacks
                    [bounded_index(next_attack_word(&mut state), root_path_attacks.len())];
                format!("{attack}/{nonce}")
            }
        };
        generated.insert(url.clone());
        categories[category] = true;
        let contents = BTreeMap::from([(
            "url-fuzz".to_owned(),
            SectionContent::Hero {
                title: "Safe".to_owned(),
                subtitle: "Text".to_owned(),
                cta_label: "Go".to_owned(),
                cta_url: "https://example.com/go".to_owned(),
                image: Some(HeroImage {
                    url,
                    alt: "Image".to_owned(),
                }),
            },
        )]);

        assert_eq!(
            renderer
                .render(&revision, &theme, &contents)
                .expect_err("hostile URL must be rejected before rendering"),
            RenderError::UnsafeSectionUrl,
            "generated URL attack {index} was accepted"
        );
    }

    assert_eq!(generated.len(), 10_000);
    assert!(categories.into_iter().all(|seen| seen));
}
