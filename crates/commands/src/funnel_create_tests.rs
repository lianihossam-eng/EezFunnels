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
    assert!(FunnelBlueprint::ALL[..4]
        .iter()
        .all(|blueprint| blueprint.description().contains("outline")));
    assert!(FunnelBlueprint::Vsl
        .description()
        .contains("video blocks, embeds"));
    assert_eq!(FunnelBlueprint::LeadGen.id(), "lead_gen");
    assert!(FunnelBlueprint::LeadGen
        .description()
        .to_lowercase()
        .contains("working service inquiry form"));
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
fn v2_create_preserves_omitted_language_and_prepares_current_renderer_manifest() {
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
    assert!(document.get("language").is_none());
    assert_eq!(document["pages"].as_array().map(Vec::len), Some(2));
    assert_eq!(document["pages"][1]["slug"], "request-received");
    assert_eq!(document["pages"][1]["noindex"], true);
    assert_eq!(document["forms"].as_array().map(Vec::len), Some(1));
    let form = &document["forms"][0];
    assert_eq!(form["purpose"], "Service inquiry");
    assert_eq!(form["fields"].as_array().map(Vec::len), Some(3));
    assert_eq!(form["fields"][0]["required"], false);
    assert_eq!(form["fields"][1]["required"], true);
    assert_eq!(form["fields"][2]["required"], true);
    assert_eq!(form["fields"][2]["presentation"], "multiline");
    assert_eq!(
        form["request_consent"]["copy_version"],
        "request-ack-service-en"
    );
    assert_eq!(
        form["request_consent"]["copy"],
        "I have read and understood the information about this request."
    );
    assert!(form.get("marketing_opt_in").is_none());
    assert_eq!(form["result"]["kind"], "success_page");
    assert_eq!(form["result"]["page_id"], document["pages"][1]["id"]);
    assert!(document["pages"][0]["sections"]
        .as_array()
        .expect("home sections")
        .iter()
        .flat_map(|section| section["blocks"].as_array().expect("section blocks"))
        .any(|block| block["kind"] == "form" && block["form_id"] == form["id"]));
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
    assert!(html.contains("A clear service offer"));
    assert!(html.contains("What the service includes"));
    assert!(html.contains("name=\"project_details\" rows=\"4\" maxlength=\"8192\" required"));
    assert!(html.contains("<textarea"));
    assert!(html.contains("Send request"));
    assert!(!html.contains("example.com"));
}

#[test]
fn v2_create_accepts_french_language_and_localizes_generated_public_copy() {
    let command = FunnelCreateCommandV2::from_json(
        r#"{"name":"Offre de service","blueprint":"lead_gen","language":"fr"}"#,
    )
    .expect("French create command");
    let prepared = command
        .prepare("funnel-fr", "revision-fr")
        .expect("French funnel");
    let document: Value =
        serde_json::from_str(&prepared.revision().data_json).expect("canonical French document");

    assert_eq!(document["language"], "fr");
    assert_eq!(document["pages"][0]["title"], "Accueil");
    let html = eez_render::document_v2::render_published(
        prepared.revision().schema_version,
        "funnel-fr",
        "revision-fr",
        &prepared.revision().data_json,
        &prepared.revision().render_manifest_json,
    )
    .expect("French public page");
    assert!(html.contains("<html lang=\"fr\">"));
    assert!(html.contains("Une offre de service claire"));
    assert!(html.contains("Adresse e-mail"));
    assert!(html.contains("Décrivez votre besoin"));
    assert!(html.contains("J’ai pris connaissance des informations relatives à cette demande."));
    assert!(!html.contains("example.com"));
}

#[test]
fn service_starter_confirmation_uses_its_internal_page_route() {
    let prepared = FunnelCreateCommandV2::from_json(
        r#"{"name":"Service request","blueprint":"lead_gen","language":"en"}"#,
    )
    .expect("service inquiry command")
    .prepare("service-funnel", "service-revision")
    .expect("service inquiry revision");
    let revision = prepared.revision();
    let record = RevisionRecord {
        id: revision.id.clone(),
        funnel_id: revision.funnel_id.clone(),
        schema_version: revision.schema_version,
        data_json: revision.data_json.clone(),
        render_manifest_json: revision.render_manifest_json.clone(),
        created_at: "2026-10-30T00:00:00Z".to_owned(),
    };

    let confirmation = render_published_revision(
        &record,
        "service-funnel",
        "service-funnel",
        Some("request-received"),
    )
    .expect("form result slug resolves to the generated confirmation page");

    assert!(confirmation.contains("Your request has been received."));
    assert!(confirmation.contains("This confirms receipt only; it is not a reservation"));
    assert!(!confirmation.contains("example.com"));
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
    let legacy_html =
        render_published_revision(&legacy_record, "legacy-funnel", "legacy-funnel", None)
            .expect("V1 reader remains available");
    assert!(legacy_html.contains("Legacy offer"));
    assert_eq!(
        render_published_revision(
            &legacy_record,
            "legacy-funnel",
            "legacy-funnel",
            Some("next"),
        )
        .expect_err("V1 funnels do not have public page routes")
        .code(),
        "not_found/slug"
    );
    assert_eq!(
        render_published_revision(
            &legacy_record,
            "legacy-funnel",
            "legacy-funnel",
            Some("next"),
        )
        .expect_err("V1 funnels do not have public page routes")
        .code(),
        "not_found/slug"
    );

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
    let typed_html = render_published_revision(&typed_record, "typed-funnel", "typed-funnel", None)
        .expect("V2 reader selected by the stored row");
    assert!(typed_html.contains("A clear service offer"));
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

    assert_eq!(projection.len(), 2);
    assert_eq!(projection[0]["title"], "Home");
    assert_eq!(projection[1]["slug"], "request-received");
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
        assert_eq!(
            document["pages"].as_array().map(Vec::len),
            Some(if blueprint == FunnelBlueprint::LeadGen {
                2
            } else {
                1
            })
        );
        assert_eq!(
            document["forms"].as_array().map(Vec::len),
            Some(if blueprint == FunnelBlueprint::LeadGen {
                1
            } else {
                0
            })
        );
        assert!(!typed.revision().data_json.contains("example.com"));
        assert!(!html.contains("example.com"));
        if blueprint == FunnelBlueprint::LeadGen {
            assert!(html.contains("A clear service offer"));
            assert!(
                html.contains("name=\"project_details\" rows=\"4\" maxlength=\"8192\" required")
            );
        } else {
            assert!(html.contains("Welcome to your new funnel"));
            assert!(html.contains("Choose a real destination before adding a call to action."));
        }
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
