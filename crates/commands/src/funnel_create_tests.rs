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
fn v2_service_recipes_build_distinct_localized_forms_without_changing_the_blueprint_contract() {
    let cases = [
        (
            "contact",
            "Service inquiry",
            "project_details",
            "Describe your project",
            "Your request has been received.",
            "Demande de service",
            "project_details",
            "Décrivez votre besoin",
            "Votre demande a bien été reçue.",
        ),
        (
            "quote_request",
            "Estimate request",
            "quote_scope",
            "Describe the scope for an estimate",
            "This confirms receipt only; no price or response date is promised.",
            "Demande de devis",
            "quote_scope",
            "Décrivez le périmètre du devis",
            "Ce message confirme seulement la réception ; aucun prix ni délai de réponse n’est promis.",
        ),
        (
            "diagnostic_application",
            "Diagnostic request",
            "current_situation",
            "Describe the current situation",
            "This does not book a session or provide a diagnosis.",
            "Demande de diagnostic",
            "current_situation",
            "Décrivez la situation actuelle",
            "Cette confirmation ne réserve pas de séance et ne fournit pas de diagnostic.",
        ),
    ];

    for (
        service_recipe,
        english_purpose,
        english_field,
        english_label,
        english_boundary,
        french_purpose,
        french_field,
        french_label,
        french_boundary,
    ) in cases
    {
        let parameters = serde_json::json!({
            "name": "Service request",
            "blueprint": "lead_gen",
            "service_recipe": service_recipe,
            "language": "en",
        })
        .to_string();
        let prepared = FunnelCreateCommandV2::from_json(&parameters)
            .expect("known service recipe")
            .prepare(
                format!("funnel-{service_recipe}"),
                format!("revision-{service_recipe}"),
            )
            .expect("service recipe revision");
        let document: Value =
            serde_json::from_str(&prepared.revision().data_json).expect("V2 document JSON");
        assert_eq!(document["blueprint_id"], "lead_gen");
        assert!(document.get("service_recipe").is_none());
        let form = &document["forms"][0];
        assert_eq!(form["purpose"], english_purpose);
        assert_eq!(form["marketing_opt_in"], Value::Null);
        assert_eq!(form["fields"][1]["name"], "email");
        assert_eq!(form["fields"][1]["required"], true);
        assert_eq!(form["fields"][2]["name"], english_field);
        assert_eq!(form["fields"][2]["label"], english_label);
        assert_eq!(form["fields"][2]["required"], true);
        assert_eq!(
            form["request_consent"]["copy"],
            "I have read and understood the information about this request."
        );

        let english_html = eez_render::document_v2::render_published(
            prepared.revision().schema_version,
            prepared.funnel().id.as_str(),
            prepared.revision().id.as_str(),
            &prepared.revision().data_json,
            &prepared.revision().render_manifest_json,
        )
        .expect("English service recipe renders");
        assert!(english_html.contains(english_label));
        assert!(!english_html.contains("example.com"));
        let english_record = RevisionRecord {
            id: prepared.revision().id.clone(),
            funnel_id: prepared.revision().funnel_id.clone(),
            schema_version: prepared.revision().schema_version,
            data_json: prepared.revision().data_json.clone(),
            render_manifest_json: prepared.revision().render_manifest_json.clone(),
            created_at: "2026-10-30T00:00:00Z".to_owned(),
        };
        let english_confirmation = render_published_revision(
            &english_record,
            prepared.funnel().id.as_str(),
            prepared.funnel().slug.as_str(),
            Some("request-received"),
        )
        .expect("English confirmation page renders");
        assert!(english_confirmation.contains(english_boundary));

        let french_parameters = serde_json::json!({
            "name": "Demande de service",
            "blueprint": "lead_gen",
            "service_recipe": service_recipe,
            "language": "fr",
        })
        .to_string();
        let french_prepared = FunnelCreateCommandV2::from_json(&french_parameters)
            .expect("known French service recipe")
            .prepare(
                format!("funnel-fr-{service_recipe}"),
                format!("revision-fr-{service_recipe}"),
            )
            .expect("French service recipe revision");
        let french_document: Value = serde_json::from_str(&french_prepared.revision().data_json)
            .expect("French V2 document JSON");
        let french_form = &french_document["forms"][0];
        assert_eq!(french_form["purpose"], french_purpose);
        assert_eq!(french_form["fields"][2]["name"], french_field);
        assert_eq!(french_form["fields"][2]["label"], french_label);
        assert_eq!(
            french_form["request_consent"]["copy_version"],
            "request-ack-service-fr"
        );

        let french_record = RevisionRecord {
            id: french_prepared.revision().id.clone(),
            funnel_id: french_prepared.revision().funnel_id.clone(),
            schema_version: french_prepared.revision().schema_version,
            data_json: french_prepared.revision().data_json.clone(),
            render_manifest_json: french_prepared.revision().render_manifest_json.clone(),
            created_at: "2026-10-30T00:00:00Z".to_owned(),
        };
        let french_confirmation = render_published_revision(
            &french_record,
            french_prepared.funnel().id.as_str(),
            french_prepared.funnel().slug.as_str(),
            Some("request-received"),
        )
        .expect("localized confirmation page renders");
        assert!(french_confirmation.contains(french_boundary));
    }
}

#[test]
fn contact_recipe_preserves_the_existing_lead_gen_document_and_other_blueprints_reject_recipes() {
    let current = FunnelCreateCommandV2::from_json(
        r#"{"name":"Service request","blueprint":"lead_gen","language":"fr"}"#,
    )
    .expect("existing V2 request remains valid")
    .prepare("funnel-contact", "revision-contact")
    .expect("existing lead-gen starter");
    let contact = FunnelCreateCommandV2::from_json(
        r#"{"name":"Service request","blueprint":"lead_gen","language":"fr","service_recipe":"contact"}"#,
    )
    .expect("contact recipe remains valid")
    .prepare("funnel-contact", "revision-contact")
    .expect("contact recipe starter");

    assert_eq!(current.revision().data_json, contact.revision().data_json);
    assert!(FunnelCreateCommandV2::from_json(
        r#"{"name":"Outline","blueprint":"tripwire","service_recipe":"contact"}"#
    )
    .is_err());
    assert!(FunnelCreateCommandV2::from_json(
        r#"{"name":"Unknown recipe","blueprint":"lead_gen","service_recipe":"quiz"}"#
    )
    .is_err());
}

#[test]
fn v2_creation_recipe_parser_handles_deterministic_untrusted_inputs_without_panicking() {
    let recipes = [
        serde_json::json!("contact"),
        serde_json::json!("quote_request"),
        serde_json::json!("diagnostic_application"),
        serde_json::json!("unknown"),
        serde_json::Value::Null,
        serde_json::json!(3),
        serde_json::json!({"unexpected": true}),
        serde_json::json!(["contact"]),
    ];
    let blueprints = [
        serde_json::json!("lead_gen"),
        serde_json::json!("tripwire"),
        serde_json::Value::Null,
        serde_json::json!(false),
        serde_json::json!("unknown"),
    ];
    let names = [
        serde_json::json!("A service request"),
        serde_json::json!(""),
        serde_json::json!(12),
        serde_json::json!("😀".repeat(40)),
    ];
    let mut state = 0x19d0_19d0_5eed_u64;

    for case_number in 0..10_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let parameters = serde_json::json!({
            "name": names[(state as usize) % names.len()],
            "blueprint": blueprints[((state >> 8) as usize) % blueprints.len()],
            "service_recipe": recipes[((state >> 16) as usize) % recipes.len()],
        })
        .to_string();

        assert!(
            std::panic::catch_unwind(|| FunnelCreateCommandV2::from_json(&parameters)).is_ok(),
            "recipe parser panicked for case {case_number}: {parameters}"
        );
    }
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
