use std::collections::BTreeMap;

use super::content_config_hash;
use super::{FeatureItem, FooterLink, SectionContent, Testimonial, ThemeTokens};
use eez_domain::{FunnelId, RenderManifest, Revision, RevisionId, Section};

pub(crate) fn revision_with_sections(
    sections: Vec<(&str, &str, usize)>,
    renderer_version: &str,
) -> Revision {
    let theme = safe_theme_tokens();
    revision_with_theme_version(sections, renderer_version, theme.version())
}

pub(crate) fn revision_with_theme_version(
    sections: Vec<(&str, &str, usize)>,
    renderer_version: &str,
    theme_version: &str,
) -> Revision {
    revision_with_theme_version_and_contents(
        sections,
        renderer_version,
        theme_version,
        &BTreeMap::new(),
    )
}

pub(crate) fn revision_with_contents(
    sections: Vec<(&str, &str, usize)>,
    renderer_version: &str,
    contents: &BTreeMap<String, SectionContent>,
) -> Revision {
    let theme = safe_theme_tokens();
    revision_with_theme_version_and_contents(sections, renderer_version, theme.version(), contents)
}

pub(crate) fn revision_with_theme_version_and_contents(
    sections: Vec<(&str, &str, usize)>,
    renderer_version: &str,
    theme_version: &str,
    contents: &BTreeMap<String, SectionContent>,
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
        theme_version,
        renderer_version,
        "assets-hash",
        content_config_hash(contents),
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

pub(crate) fn safe_theme_tokens() -> ThemeTokens {
    ThemeTokens::new(BTreeMap::from([
        ("color-primary".to_owned(), "#112233".to_owned()),
        ("spacing-section".to_owned(), "2rem".to_owned()),
    ]))
    .expect("safe theme tokens")
}

pub(crate) fn content_map(sections: &[(&str, &str)]) -> BTreeMap<String, SectionContent> {
    sections
        .iter()
        .map(|(id, section_type)| {
            let content = match *section_type {
                "hero" => SectionContent::Hero {
                    title: "Title".to_owned(),
                    subtitle: "Subtitle".to_owned(),
                    cta_label: "Go".to_owned(),
                    cta_url: "https://example.com/go".to_owned(),
                    image: None,
                },
                "features" => SectionContent::Features {
                    items: vec![FeatureItem {
                        title: "Feature".to_owned(),
                        description: "Description".to_owned(),
                    }],
                },
                "testimonials" => SectionContent::Testimonials {
                    items: vec![Testimonial {
                        quote: "Quote".to_owned(),
                        author: "Author".to_owned(),
                        attribution: None,
                    }],
                },
                "cta" => SectionContent::Cta {
                    title: "Title".to_owned(),
                    body: "Body".to_owned(),
                    cta_label: "Go".to_owned(),
                    cta_url: "https://example.com/go".to_owned(),
                },
                "footer" => SectionContent::Footer {
                    links: vec![FooterLink {
                        label: "Link".to_owned(),
                        url: "https://example.com/".to_owned(),
                    }],
                    copyright: "Copyright".to_owned(),
                },
                _ => panic!("test requested unsupported section type: {section_type}"),
            };
            ((*id).to_owned(), content)
        })
        .collect()
}
