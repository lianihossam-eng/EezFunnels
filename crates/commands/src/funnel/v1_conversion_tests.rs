use super::{convert_v1_to_v2, decode_snapshot, FunnelCreateCommand, ValidatedSnapshot};
use eez_domain::document_v2::DocumentV2;
use eez_render::{document_v2 as render_v2, HeroImage, SectionContent};
use eez_storage::repository::RevisionRecord;

fn source_snapshot() -> ValidatedSnapshot {
    let command =
        FunnelCreateCommand::from_json(r#"{"name":"Conversion test","blueprint":"lead_gen"}"#)
            .expect("valid V1 create command");
    let prepared = command
        .prepare("funnel-conversion-1", "revision-conversion-1")
        .expect("V1 revision prepares");
    decode_snapshot(
        &RevisionRecord {
            id: prepared.revision.id.clone(),
            funnel_id: prepared.revision.funnel_id.clone(),
            schema_version: prepared.revision.schema_version,
            data_json: prepared.revision.data_json.clone(),
            render_manifest_json: prepared.revision.render_manifest_json.clone(),
            created_at: "2026-10-16T00:00:00Z".to_owned(),
        },
        "funnel-conversion-1",
    )
    .expect("prepared V1 revision decodes")
}

fn replace_legacy_links_with_https(source: &mut ValidatedSnapshot) {
    for content in source.contents.values_mut() {
        match content {
            SectionContent::Hero { cta_url, .. } | SectionContent::Cta { cta_url, .. } => {
                *cta_url = "https://example.com/offer".to_owned();
            }
            SectionContent::Footer { links, .. } => {
                for link in links {
                    link.url = "https://example.com/contact".to_owned();
                }
            }
            SectionContent::Features { .. } | SectionContent::Testimonials { .. } => {}
        }
    }
}

#[test]
fn conversion_reports_every_unrepresentable_relative_link_with_stable_paths() {
    let source = source_snapshot();

    let blockers = convert_v1_to_v2(&source, "funnel-conversion-1")
        .expect_err("relative paths cannot be silently discarded");
    let paths = blockers
        .iter()
        .map(|blocker| blocker.path.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        paths,
        [
            "contents.hero.cta_url",
            "contents.cta.cta_url",
            "contents.footer.links[0].url",
        ]
    );
    assert!(blockers
        .iter()
        .all(|blocker| blocker.code == "conversion/relative_link_unsupported"));
}

#[test]
fn conversion_preserves_content_order_and_supported_theme_overrides() {
    let mut source = source_snapshot();
    replace_legacy_links_with_https(&mut source);
    source.snapshot.theme_overrides.primary_color = Some("#123456".to_owned());
    source.snapshot.theme_overrides.square_buttons = true;

    let converted = convert_v1_to_v2(&source, "funnel-conversion-1")
        .expect("lossless lead generation content converts");
    let value: serde_json::Value =
        serde_json::from_slice(converted.document.canonical_bytes()).expect("canonical JSON");
    assert_eq!(value["pages"].as_array().map(Vec::len), Some(1));
    assert_eq!(value["forms"].as_array().map(Vec::len), Some(0));
    let section_ids = converted.document.pages()[0]
        .sections()
        .iter()
        .map(|section| section.id().as_str())
        .collect::<Vec<_>>();

    assert_eq!(section_ids, ["hero", "features", "cta", "footer"]);
    assert_eq!(value["theme"]["preset_id"], "minimal");
    assert_eq!(
        value["theme"]["overrides"]["colors"]["color-primary"],
        "#123456"
    );
    assert_eq!(value["theme"]["overrides"]["button_shape"], "square");
    assert_eq!(
        value["pages"][0]["sections"][1]["blocks"][0]["kind"],
        "list"
    );
    assert!(converted
        .warnings
        .iter()
        .any(|warning| warning.code == "conversion/feature_structure_flattened"));
    assert!(render_v2::render_owner_preview(
        &converted.document,
        converted.document.entry_page_id()
    )
    .expect("converted owner preview renders")
    .contains("A clear starting point"));
}

#[test]
fn conversion_blocks_url_backed_images_until_immutable_asset_metadata_exists() {
    let mut source = source_snapshot();
    replace_legacy_links_with_https(&mut source);
    source.contents.insert(
        "hero".to_owned(),
        SectionContent::Hero {
            title: "Headline".to_owned(),
            subtitle: "Subheading".to_owned(),
            cta_label: "Continue".to_owned(),
            cta_url: "https://example.com/offer".to_owned(),
            image: Some(HeroImage {
                url: "https://images.example.com/hero.jpg".to_owned(),
                alt: "Product".to_owned(),
            }),
        },
    );

    let blockers = convert_v1_to_v2(&source, "funnel-conversion-1")
        .expect_err("legacy URL-backed image has no immutable V2 asset ID");

    assert_eq!(blockers.len(), 1);
    assert_eq!(blockers[0].code, "conversion/image_requires_asset");
    assert_eq!(blockers[0].path, "contents.hero.image");
}

#[test]
fn conversion_result_is_a_canonical_document_bound_to_the_source_funnel() {
    let mut source = source_snapshot();
    replace_legacy_links_with_https(&mut source);
    let converted = convert_v1_to_v2(&source, "funnel-conversion-1")
        .expect("convertible source produces a typed document");
    let decoded = DocumentV2::decode(converted.document.canonical_bytes(), "funnel-conversion-1")
        .expect("conversion output decodes under the target funnel ID");

    assert_eq!(converted.document, decoded);
    assert!(DocumentV2::decode(converted.document.canonical_bytes(), "another-funnel").is_err());
}
