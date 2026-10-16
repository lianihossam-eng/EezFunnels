use super::{
    render_published_revision, v2_page_projection, FunnelBlueprint, FunnelCreateCommand,
    FunnelCreateCommandV2,
};
use eez_render::{content_config_hash, SectionContent, ThemeTokens, RENDERER_VERSION};
use eez_storage::repository::RevisionRecord;
use serde_json::Value;
use std::collections::BTreeMap;

#[test]
fn funnel_blueprint_catalog_is_closed_and_stable() {
    let ids = FunnelBlueprint::ALL.map(FunnelBlueprint::id);

    assert_eq!(ids, ["tripwire", "vsl", "webinar", "launch", "lead_gen"]);
    assert!(serde_json::from_str::<FunnelBlueprint>("\"unknown\"").is_err());
}

#[test]
fn tripwire_creation_prepares_a_renderable_three_section_initial_revision() {
    let command =
        FunnelCreateCommand::from_json(r#"{"name":"Quick offer","blueprint":"tripwire"}"#)
            .expect("valid create command");
    let prepared = command
        .prepare("funnel-a", "revision-a")
        .expect("valid initial snapshot");

    assert_eq!(prepared.funnel().id, "funnel-a");
    assert_eq!(prepared.revision().funnel_id, "funnel-a");
    assert_eq!(prepared.revision().id, "revision-a");

    let snapshot: Value =
        serde_json::from_str(&prepared.revision().data_json).expect("versioned snapshot JSON");
    assert_eq!(snapshot["format_version"], 1);
    assert_eq!(snapshot["schema_version"], 1);
    assert_eq!(snapshot["blueprint"], "tripwire");
    assert_eq!(snapshot["theme_id"], "minimal");
    assert_eq!(snapshot["sections"].as_array().map(Vec::len), Some(3));
    assert_eq!(
        snapshot["sections"]
            .as_array()
            .expect("section list")
            .iter()
            .map(|section| section["type"].as_str().expect("section type"))
            .collect::<Vec<_>>(),
        ["hero", "features", "cta"]
    );

    let manifest: Value = serde_json::from_str(&prepared.revision().render_manifest_json)
        .expect("render manifest JSON");
    assert_eq!(manifest["revision_id"], "revision-a");
    assert_eq!(manifest["renderer_version"], "renderer-v2");
    assert!(manifest["config_hash"]
        .as_str()
        .is_some_and(|hash| hash.starts_with("content-sha256-v1:")));
    let contents: BTreeMap<String, SectionContent> =
        serde_json::from_value(snapshot["contents"].clone()).expect("typed content map");
    assert_eq!(manifest["config_hash"], content_config_hash(&contents));
    assert_eq!(
        manifest["theme_version"],
        ThemeTokens::from_preset(eez_domain::ThemePreset::Minimal)
            .expect("minimal tokens")
            .version()
    );
    assert_eq!(manifest["renderer_version"], RENDERER_VERSION);
    assert_eq!(
        manifest["assets_manifest_hash"],
        "assets-sha256-v1:8b3972abe1696375524a8237889ff77fa13664876dd51ede33db93795b8d5cca"
    );
    assert_eq!(
        prepared.summary().section_types(),
        ["hero", "features", "cta"]
    );
}

#[test]
fn v2_create_prepares_a_closed_typed_starter_with_renderer_v3_manifest() {
    let command =
        FunnelCreateCommandV2::from_json(r#"{"name":"A typed offer","blueprint":"lead_gen"}"#)
            .expect("valid V2 create command");
    let prepared = command
        .prepare("funnel-v2", "revision-v2")
        .expect("typed starter");

    assert_eq!(prepared.funnel().id, "funnel-v2");
    assert_eq!(prepared.revision().schema_version, 2);
    let document: Value =
        serde_json::from_str(&prepared.revision().data_json).expect("canonical V2 document");
    assert_eq!(document["document_type"], "eezfunnel.funnel-document");
    assert_eq!(document["schema_version"], 2);
    assert_eq!(document["blueprint_id"], "lead_gen");
    assert_eq!(document["pages"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        document["pages"][0]["sections"]
            .as_array()
            .expect("typed blueprint sections")
            .iter()
            .map(|section| section["type"].as_str().expect("section type"))
            .collect::<Vec<_>>(),
        ["hero", "features", "cta", "footer"]
    );

    let html = eez_render::document_v2::render_published(
        prepared.revision().schema_version,
        "funnel-v2",
        "revision-v2",
        &prepared.revision().data_json,
        &prepared.revision().render_manifest_json,
    )
    .expect("starter public page renders");
    assert!(html.contains("Welcome to your new funnel"));
    assert!(html.contains("What makes this offer valuable"));
    assert!(html.contains("Take the next step today."));
}

#[test]
fn published_revision_dispatch_uses_the_stored_schema_version() {
    let legacy =
        FunnelCreateCommand::from_json(r#"{"name":"Legacy offer","blueprint":"tripwire"}"#)
            .expect("legacy create command")
            .prepare("legacy-funnel", "legacy-revision")
            .expect("legacy revision");
    let legacy_revision = legacy.revision();
    let legacy_record = RevisionRecord {
        id: legacy_revision.id.clone(),
        funnel_id: legacy_revision.funnel_id.clone(),
        schema_version: legacy_revision.schema_version,
        data_json: legacy_revision.data_json.clone(),
        render_manifest_json: legacy_revision.render_manifest_json.clone(),
        created_at: "2026-10-16T00:00:00Z".to_owned(),
    };
    let legacy_html = render_published_revision(&legacy_record, "legacy-funnel")
        .expect("V1 reader remains available");
    assert!(legacy_html.contains("Legacy offer"));

    let typed =
        FunnelCreateCommandV2::from_json(r#"{"name":"Typed offer","blueprint":"lead_gen"}"#)
            .expect("V2 create command")
            .prepare("typed-funnel", "typed-revision")
            .expect("typed revision");
    let typed_revision = typed.revision();
    let typed_record = RevisionRecord {
        id: typed_revision.id.clone(),
        funnel_id: typed_revision.funnel_id.clone(),
        schema_version: typed_revision.schema_version,
        data_json: typed_revision.data_json.clone(),
        render_manifest_json: typed_revision.render_manifest_json.clone(),
        created_at: "2026-10-16T00:00:00Z".to_owned(),
    };
    let typed_html = render_published_revision(&typed_record, "typed-funnel")
        .expect("V2 reader selected by the stored row");
    assert!(typed_html.contains("Welcome to your new funnel"));
}

#[test]
fn v2_page_projection_is_bounded_and_does_not_expose_document_copy() {
    let prepared =
        FunnelCreateCommandV2::from_json(r#"{"name":"Private copy","blueprint":"lead_gen"}"#)
            .expect("V2 create command")
            .prepare("projection-funnel", "projection-revision")
            .expect("typed revision");
    let document = eez_domain::document_v2::DocumentV2::decode(
        prepared.revision().data_json.as_bytes(),
        "projection-funnel",
    )
    .expect("typed document");

    let projection = v2_page_projection(&document);
    let serialized = serde_json::to_string(&projection).expect("projection JSON");

    assert_eq!(projection.len(), 1);
    assert_eq!(projection[0]["title"], "Home");
    assert!(!serialized.contains("Welcome to your new funnel"));
    assert!(!serialized.contains("document_json"));
    assert!(serialized.len() < 16 * 1024);
}

#[test]
fn v2_create_rejects_unknown_fields_and_invalid_names() {
    assert!(FunnelCreateCommandV2::from_json(
        r#"{"name":"Typed","blueprint":"lead_gen","schema_version":1}"#
    )
    .is_err());
    assert!(FunnelCreateCommandV2::from_json(r#"{"name":"  ","blueprint":"lead_gen"}"#).is_err());
}

#[test]
fn every_blueprint_has_a_bounded_renderable_initial_snapshot() {
    let expected = [
        (FunnelBlueprint::Tripwire, vec!["hero", "features", "cta"]),
        (FunnelBlueprint::Vsl, vec!["hero", "testimonials", "cta"]),
        (
            FunnelBlueprint::Webinar,
            vec!["hero", "features", "testimonials", "cta"],
        ),
        (
            FunnelBlueprint::Launch,
            vec!["hero", "features", "testimonials", "cta", "footer"],
        ),
        (
            FunnelBlueprint::LeadGen,
            vec!["hero", "features", "cta", "footer"],
        ),
    ];

    for (index, (blueprint, types)) in expected.into_iter().enumerate() {
        let command = FunnelCreateCommand::from_json(&format!(
            "{{\"name\":\"{} starter\",\"blueprint\":\"{}\"}}",
            blueprint.label(),
            blueprint.id()
        ))
        .expect("known blueprint parses");
        let prepared = command
            .prepare(format!("funnel-{index}"), format!("revision-{index}"))
            .expect("blueprint is renderable");
        assert_eq!(prepared.summary().section_types(), types);
        assert!(prepared.revision().data_json.len() < 16 * 1024);

        let typed_command = FunnelCreateCommandV2::from_json(&format!(
            "{{\"name\":\"{} typed starter\",\"blueprint\":\"{}\"}}",
            blueprint.label(),
            blueprint.id()
        ))
        .expect("known typed blueprint parses");
        let typed = typed_command
            .prepare(
                format!("typed-funnel-{index}"),
                format!("typed-revision-{index}"),
            )
            .expect("typed blueprint is renderable");
        let document: Value =
            serde_json::from_str(&typed.revision().data_json).expect("typed document JSON");
        assert_eq!(
            document["pages"][0]["sections"]
                .as_array()
                .expect("typed blueprint sections")
                .iter()
                .map(|section| section["type"].as_str().expect("section type"))
                .collect::<Vec<_>>(),
            types
        );
        let html = eez_render::document_v2::render_published(
            typed.revision().schema_version,
            typed.revision().funnel_id.as_str(),
            typed.revision().id.as_str(),
            &typed.revision().data_json,
            &typed.revision().render_manifest_json,
        )
        .expect("typed blueprint renders");
        assert!(html.contains("Welcome to your new funnel"));
    }
}

#[test]
fn create_command_rejects_unknown_fields_and_unbounded_names() {
    assert!(FunnelCreateCommand::from_json(
        r#"{"name":"A","blueprint":"tripwire","slug":"chosen"}"#
    )
    .is_err());
    assert!(FunnelCreateCommand::from_json(&format!(
        "{{\"name\":\"{}\",\"blueprint\":\"tripwire\"}}",
        "x".repeat(121)
    ))
    .is_err());
}

#[test]
fn create_and_delete_commands_report_missing_required_fields() {
    assert_eq!(
        FunnelCreateCommand::from_json(r#"{"blueprint":"tripwire"}"#)
            .expect_err("missing name")
            .code(),
        "validation/missing_field"
    );
    assert_eq!(
        super::FunnelDeleteCommand::from_json("{}")
            .expect_err("missing funnel id")
            .code(),
        "validation/missing_field"
    );
}
