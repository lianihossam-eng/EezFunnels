use super::{FunnelBlueprint, FunnelCreateCommand};
use eez_render::{content_config_hash, SectionContent, ThemeTokens, RENDERER_VERSION};
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
