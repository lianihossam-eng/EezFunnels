use super::inspector::{apply_editor_changes, encode_snapshot, SettingChanges};
use super::{
    convert_v1_to_v2, decode_snapshot, publication_readiness, FunnelCreateCommand,
    FunnelCreateCommandV2,
};
use eez_domain::{document_v2::DocumentV2, FunnelBlueprint};
use eez_storage::repository::RevisionRecord;
use serde_json::{json, Value};

fn stored_v1(blueprint: FunnelBlueprint) -> RevisionRecord {
    let prepared = FunnelCreateCommand::from_json(
        &json!({
            "name": "Readiness test",
            "blueprint": blueprint.id(),
        })
        .to_string(),
    )
    .expect("valid V1 create command")
    .prepare("funnel-readiness-v1", "revision-readiness-v1")
    .expect("V1 revision prepares");
    RevisionRecord {
        id: prepared.revision().id.clone(),
        funnel_id: prepared.revision().funnel_id.clone(),
        schema_version: prepared.revision().schema_version,
        data_json: prepared.revision().data_json.clone(),
        render_manifest_json: prepared.revision().render_manifest_json.clone(),
        created_at: "2026-10-30T00:00:00Z".to_owned(),
    }
}

fn stored_v2(blueprint: FunnelBlueprint) -> RevisionRecord {
    let prepared = FunnelCreateCommandV2::from_json(
        &json!({
            "name": "Readiness test",
            "blueprint": blueprint.id(),
        })
        .to_string(),
    )
    .expect("valid V2 create command")
    .prepare("funnel-readiness-v2", "revision-readiness-v2")
    .expect("V2 revision prepares");
    RevisionRecord {
        id: prepared.revision().id.clone(),
        funnel_id: prepared.revision().funnel_id.clone(),
        schema_version: prepared.revision().schema_version,
        data_json: prepared.revision().data_json.clone(),
        render_manifest_json: prepared.revision().render_manifest_json.clone(),
        created_at: "2026-10-30T00:00:00Z".to_owned(),
    }
}

#[test]
fn generated_v1_placeholders_block_publication_but_markerless_legacy_revisions_remain_eligible() {
    let record = stored_v1(FunnelBlueprint::Vsl);
    let readiness = publication_readiness::for_revision(&record, &record.funnel_id)
        .expect("generated V1 revision has a readiness report");
    let findings = serde_json::to_value(&readiness).expect("readiness serializes");
    assert!(!readiness.can_publish());
    assert_eq!(findings["findings"].as_array().map(Vec::len), Some(3));
    assert!(findings["findings"]
        .as_array()
        .unwrap()
        .iter()
        .all(|finding| {
            finding["code"] == "publication/generated_placeholder_unresolved"
                && finding["severity"] == "blocking"
        }));

    let mut historical = record;
    let mut snapshot: Value = serde_json::from_str(&historical.data_json).expect("snapshot JSON");
    snapshot
        .as_object_mut()
        .expect("snapshot object")
        .remove("generated_placeholders");
    historical.data_json = serde_json::to_string(&snapshot).expect("historical snapshot JSON");
    let legacy_readiness = publication_readiness::for_revision(&historical, &historical.funnel_id)
        .expect("markerless historical revision remains readable");
    assert!(legacy_readiness.can_publish());
    assert_eq!(
        serde_json::to_value(legacy_readiness).unwrap()["findings"],
        json!([])
    );
}

#[test]
fn v1_owner_edits_resolve_only_the_generated_fields_they_replace_or_remove() {
    let record = stored_v1(FunnelBlueprint::Launch);
    let decoded = decode_snapshot(&record, &record.funnel_id).expect("generated V1 decodes");
    let changes: SettingChanges = serde_json::from_value(json!({
        "schema_version": 1,
        "changes": [
            {"id":"section.testimonials.items.0.quote", "value":{"kind":"text", "value":"A verified customer quote."}},
            {"id":"section.testimonials.items.0.attribution", "value":{"kind":"text", "value":null}}
        ]
    }))
    .expect("valid testimonial edits");

    let edited = apply_editor_changes(&decoded, &changes.changes)
        .expect("valid owner edits preserve a valid V1 revision");
    let readiness = publication_readiness::for_v1(&record.id, &edited.snapshot);
    let findings = serde_json::to_value(&readiness).expect("readiness serializes");
    assert!(!readiness.can_publish());
    assert_eq!(findings["findings"].as_array().map(Vec::len), Some(1));
    assert_eq!(findings["findings"][0]["target"]["field"], "items.0.author");
}

#[test]
fn markerless_v1_snapshots_keep_their_canonical_bytes() {
    let record = stored_v1(FunnelBlueprint::LeadGen);
    assert!(!record.data_json.contains("generated_placeholders"));

    let decoded = decode_snapshot(&record, &record.funnel_id).expect("markerless V1 decodes");
    let reencoded = encode_snapshot(&decoded.snapshot).expect("markerless V1 re-encodes");

    assert_eq!(reencoded, record.data_json);
}

#[test]
fn v1_edits_keep_a_blocker_while_the_generated_sentence_remains_in_the_field() {
    let record = stored_v1(FunnelBlueprint::Launch);
    let decoded = decode_snapshot(&record, &record.funnel_id).expect("generated V1 decodes");
    let changes: SettingChanges = serde_json::from_value(json!({
        "schema_version": 1,
        "changes": [{
            "id":"section.testimonials.items.0.quote",
            "value":{"kind":"text", "value":"Replace this sample with a real customer quote. This is still the generated prompt."}
        }]
    }))
    .expect("valid text edit");

    let edited = apply_editor_changes(&decoded, &changes.changes)
        .expect("a partial prompt edit preserves a valid V1 revision");
    let readiness = publication_readiness::for_v1(&record.id, &edited.snapshot);
    let findings = serde_json::to_value(&readiness).expect("readiness serializes");
    assert!(!readiness.can_publish());
    assert_eq!(findings["findings"].as_array().map(Vec::len), Some(3));
}

#[test]
fn v1_generated_text_moved_to_another_field_keeps_its_blocker() {
    let record = stored_v1(FunnelBlueprint::Launch);
    let decoded = decode_snapshot(&record, &record.funnel_id).expect("generated V1 decodes");
    let quote_prompt = decoded
        .snapshot
        .generated_placeholders
        .iter()
        .find(|marker| marker.code == super::V1GeneratedPlaceholderCode::Quote)
        .expect("starter quote marker")
        .expected_text
        .clone();
    let changes: SettingChanges = serde_json::from_value(json!({
        "schema_version": 1,
        "changes": [
            {"id":"section.testimonials.items.0.quote", "value":{"kind":"text", "value":"A verified customer quote."}},
            {"id":"section.testimonials.items.0.author", "value":{"kind":"text", "value":quote_prompt}}
        ]
    }))
    .expect("valid testimonial edits");

    let edited = apply_editor_changes(&decoded, &changes.changes)
        .expect("valid owner edits preserve a valid V1 revision");
    let quote_marker = edited
        .snapshot
        .generated_placeholders
        .iter()
        .find(|marker| marker.code == super::V1GeneratedPlaceholderCode::Quote)
        .expect("the moved generated quote remains marked");
    let readiness = publication_readiness::for_v1(&record.id, &edited.snapshot);
    let findings = serde_json::to_value(readiness).expect("readiness serializes");

    assert_eq!(quote_marker.section_id, "testimonials");
    assert_eq!(quote_marker.field, "items.0.author");
    assert!(findings["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| {
            finding["target"]["section_id"] == "testimonials"
                && finding["target"]["field"] == "items.0.author"
        }));
}

#[test]
fn v1_to_v2_conversion_preserves_unresolved_placeholder_markers() {
    let record = stored_v1(FunnelBlueprint::Vsl);
    let decoded = decode_snapshot(&record, &record.funnel_id).expect("generated V1 decodes");
    let changes: SettingChanges = serde_json::from_value(json!({
        "schema_version": 1,
        "changes": [
            {"id":"section.hero.cta_url", "value":{"kind":"text", "value":"https://example.test/offer"}},
            {"id":"section.cta.cta_url", "value":{"kind":"text", "value":"https://example.test/offer"}}
        ]
    }))
    .expect("valid destination changes");
    let edited = apply_editor_changes(&decoded, &changes.changes)
        .expect("V1 destinations can be changed to supported absolute URLs");

    let converted = convert_v1_to_v2(&edited, &record.funnel_id)
        .expect("VSL revision converts without dropping generated-content checks");
    let readiness = publication_readiness::for_v2(&record.id, &converted.document);

    assert_eq!(converted.document.generated_placeholders().len(), 3);
    assert!(!readiness.can_publish());
    assert!(converted
        .document
        .generated_placeholders()
        .iter()
        .all(|marker| {
            marker.code != eez_domain::document_v2::GeneratedPlaceholderCode::OfferText
                && marker.code
                    != eez_domain::document_v2::GeneratedPlaceholderCode::TestimonialPrompt
        }));
}

#[test]
fn v2_readiness_blocks_explicit_starter_prompts_and_keeps_editorial_advice_nonblocking() {
    let generated = stored_v2(FunnelBlueprint::Tripwire);
    let generated_document =
        DocumentV2::decode(generated.data_json.as_bytes(), &generated.funnel_id)
            .expect("generated V2 document decodes");
    let generated_readiness = publication_readiness::for_revision(&generated, &generated.funnel_id)
        .expect("generated V2 revision has a readiness report");
    let generated_findings =
        serde_json::to_value(generated_readiness).expect("generated readiness serializes");
    assert!(!generated_document.generated_placeholders().is_empty());
    assert!(generated_findings["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| {
            finding["code"] == "publication/generated_placeholder_unresolved"
                && finding["severity"] == "blocking"
        }));
    assert!(generated_findings["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| {
            finding["code"] == "editorial/search_description_missing"
                && finding["severity"] == "warning"
        }));

    let service = stored_v2(FunnelBlueprint::LeadGen);
    let service_readiness = publication_readiness::for_revision(&service, &service.funnel_id)
        .expect("service revision has a readiness report");
    let service_findings = serde_json::to_value(&service_readiness).expect("readiness serializes");
    assert!(service_readiness.can_publish());
    assert!(service_findings["findings"]
        .as_array()
        .unwrap()
        .iter()
        .all(|finding| { finding["severity"] == "warning" }));
}

#[test]
fn v2_readiness_exposes_page_metadata_placeholder_targets() {
    let generated = FunnelCreateCommandV2::from_json(
        &json!({"name":"Readiness metadata target","blueprint":"tripwire"}).to_string(),
    )
    .expect("valid V2 create command")
    .prepare(
        "funnel-readiness-page-target",
        "revision-readiness-page-target",
    )
    .expect("V2 revision prepares");
    let mut document_json: Value =
        serde_json::from_str(&generated.revision().data_json).expect("starter JSON");
    let expected_text = document_json["generated_placeholders"][0]["expected_text"]
        .as_str()
        .expect("generated text")
        .to_owned();
    document_json["pages"][0]["seo_description"] = json!(expected_text);
    document_json["pages"][0]["sections"][0]["blocks"][0]["text"] =
        json!("A defined service offer");
    document_json["generated_placeholders"][0]["target"] = json!({
        "kind":"page_field",
        "page_id":"page-home",
        "field":"seo_description"
    });
    let document = DocumentV2::decode(
        serde_json::to_string(&document_json)
            .expect("document JSON serializes")
            .as_bytes(),
        "funnel-readiness-page-target",
    )
    .expect("metadata target is a valid placeholder target");
    let readiness = publication_readiness::for_v2("revision-readiness-page-target", &document);
    let findings = serde_json::to_value(readiness).expect("readiness serializes");

    assert_eq!(
        findings["findings"][0]["target"],
        json!({
            "kind":"v2_page_field",
            "page_id":"page-home",
            "field":"seo_description"
        })
    );
}
