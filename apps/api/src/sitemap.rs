use std::collections::BTreeSet;

use eez_domain::document_v2::DocumentV2;
use eez_storage::repository::PublishedRevisionRecord;
use url::Url;

use crate::canonical::public_page_url;

const SITEMAP_XML_PREFIX: &str =
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n";
const SITEMAP_XML_SUFFIX: &str = "</urlset>\n";

pub(crate) fn build_sitemap(
    origin: &Url,
    published_revisions: &[PublishedRevisionRecord],
) -> Result<String, ()> {
    let mut urls = BTreeSet::new();
    let origin = origin.as_str();

    for published in published_revisions {
        let revision = &published.revision;
        match revision.schema_version {
            1 => add_page_url(&mut urls, origin, &published.slug, None)?,
            2 => {
                let document =
                    DocumentV2::decode(revision.data_json.as_bytes(), &revision.funnel_id)
                        .map_err(|_| ())?;
                for page in document.pages().iter().filter(|page| !page.noindex()) {
                    let page_slug = (!page.slug().is_empty()).then_some(page.slug());
                    add_page_url(&mut urls, origin, &published.slug, page_slug)?;
                }
            }
            _ => return Err(()),
        }
    }

    let mut xml = String::with_capacity(SITEMAP_XML_PREFIX.len() + urls.len() * 120);
    xml.push_str(SITEMAP_XML_PREFIX);
    for url in urls {
        xml.push_str("  <url><loc>");
        xml.push_str(&escape_xml_text(&url));
        xml.push_str("</loc></url>\n");
    }
    xml.push_str(SITEMAP_XML_SUFFIX);
    Ok(xml)
}

pub(crate) fn empty_sitemap() -> String {
    format!("{SITEMAP_XML_PREFIX}{SITEMAP_XML_SUFFIX}")
}

pub(crate) fn build_robots(origin: Option<&Url>) -> String {
    let mut robots = String::from(
        "User-agent: *\nDisallow: /\nAllow: /sitemap.xml\nDisallow: /api/\nDisallow: /funnels\nDisallow: /submissions\nDisallow: /mcp\nAllow: /f/\nAllow: /assets/\nAllow: /media/\n",
    );
    if let Some(origin) = origin.and_then(|origin| origin.join("sitemap.xml").ok()) {
        robots.push_str("Sitemap: ");
        robots.push_str(origin.as_str());
        robots.push('\n');
    }
    robots
}

fn add_page_url(
    urls: &mut BTreeSet<String>,
    origin: &str,
    funnel_slug: &str,
    page_slug: Option<&str>,
) -> Result<(), ()> {
    let url = public_page_url(Some(origin), funnel_slug, page_slug).ok_or(())?;
    urls.insert(url);
    Ok(())
}

fn escape_xml_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use eez_storage::repository::{PublishedRevisionRecord, RevisionRecord};
    use url::Url;

    use super::{build_robots, build_sitemap};

    fn revision(
        slug: &str,
        funnel_id: &str,
        schema_version: i64,
        data_json: &str,
    ) -> PublishedRevisionRecord {
        PublishedRevisionRecord {
            slug: slug.to_owned(),
            revision: RevisionRecord {
                id: format!("revision-{funnel_id}"),
                funnel_id: funnel_id.to_owned(),
                schema_version,
                data_json: data_json.to_owned(),
                render_manifest_json: String::new(),
                created_at: String::new(),
            },
        }
    }

    fn v2_document(funnel_id: &str, page_slug: &str, noindex: bool) -> String {
        let mut document: serde_json::Value = serde_json::from_str(include_str!(
            "../../../crates/domain/tests/fixtures/document_v2_minimal.json"
        ))
        .expect("minimal V2 fixture is valid JSON");
        document["funnel_id"] = serde_json::Value::String(funnel_id.to_owned());
        document["pages"][0]["slug"] = serde_json::Value::String(String::new());
        let mut next_page = document["pages"][0].clone();
        next_page["id"] = serde_json::Value::String("page-thank-you".to_owned());
        next_page["slug"] = serde_json::Value::String(page_slug.to_owned());
        next_page["title"] = serde_json::Value::String("Thank you".to_owned());
        next_page["noindex"] = serde_json::Value::Bool(noindex);
        next_page["sections"][0]["id"] = serde_json::Value::String("section-thank-you".to_owned());
        next_page["sections"][0]["blocks"][0]["id"] =
            serde_json::Value::String("block-thank-you".to_owned());
        next_page["sections"][0]["blocks"][1]["id"] =
            serde_json::Value::String("block-thank-you-copy".to_owned());
        document["pages"]
            .as_array_mut()
            .expect("pages")
            .push(next_page);
        document["pages"][0]["sections"][0]["blocks"]
            .as_array_mut()
            .expect("entry blocks")
            .push(serde_json::json!({
                "id": "block-next-thank-you",
                "kind": "button",
                "label": "Next",
                "action": {"kind": "navigate_page", "page_id": "page-thank-you"}
            }));
        document.to_string()
    }

    #[test]
    fn sitemap_lists_published_v1_and_indexable_v2_pages_only() {
        let origin = Url::parse("https://funnels.example/").expect("valid origin");
        let published = [
            revision("legacy-offer", "legacy-funnel", 1, "{}"),
            revision(
                "new-offer",
                "v2-funnel",
                2,
                &v2_document("v2-funnel", "thank-you", false),
            ),
            revision(
                "private-offer",
                "private-funnel",
                2,
                &v2_document("private-funnel", "internal-step", true),
            ),
        ];

        let sitemap = build_sitemap(&origin, &published).expect("valid published revisions");
        assert!(sitemap.contains("https://funnels.example/f/legacy-offer"));
        assert!(sitemap.contains("https://funnels.example/f/new-offer"));
        assert!(sitemap.contains("https://funnels.example/f/new-offer/thank-you"));
        assert!(sitemap.contains("https://funnels.example/f/private-offer"));
        assert!(!sitemap.contains("internal-step"));
        assert_eq!(sitemap.matches("<url>").count(), 4);
    }

    #[test]
    fn sitemap_fails_closed_on_invalid_published_v2_data_or_slugs() {
        let origin = Url::parse("https://funnels.example/").expect("valid origin");
        let malformed = revision("offer", "v2-funnel", 2, "{}");
        assert!(build_sitemap(&origin, &[malformed]).is_err());

        let invalid_slug = revision("bad slug", "legacy-funnel", 1, "{}");
        assert!(build_sitemap(&origin, &[invalid_slug]).is_err());
    }

    #[test]
    fn robots_allows_public_pages_and_assets_but_disallows_owner_routes() {
        let origin = Url::parse("https://funnels.example/").expect("valid origin");
        let robots = build_robots(Some(&origin));
        assert!(robots.contains("Disallow: /\n"));
        assert!(robots.contains("Allow: /sitemap.xml\n"));
        assert!(robots.contains("Allow: /f/\n"));
        assert!(robots.contains("Allow: /assets/\n"));
        assert!(robots.contains("Allow: /media/\n"));
        assert!(robots.contains("Disallow: /api/\n"));
        assert!(robots.contains("Disallow: /funnels\n"));
        assert!(robots.contains("Disallow: /submissions\n"));
        assert!(robots.contains("Disallow: /mcp\n"));
        assert!(robots.contains("Sitemap: https://funnels.example/sitemap.xml\n"));
        assert!(!build_robots(None).contains("Sitemap:"));
    }
}
