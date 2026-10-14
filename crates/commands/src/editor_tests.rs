use super::funnel::inspector::{
    apply_editor_changes, encode_snapshot, inspector_projection, SettingChanges,
};
use super::funnel::{decode_snapshot, reorder_snapshot, FunnelCommandError, RenderManifestV1};
use super::FunnelCreateCommand;
use eez_storage::repository::RevisionRecord;
use serde_json::Value;

fn stored_tripwire() -> RevisionRecord {
    let prepared =
        FunnelCreateCommand::from_json(r#"{"name":"Editor test","blueprint":"tripwire"}"#)
            .expect("create command")
            .prepare("funnel-editor-test", "revision-editor-test")
            .expect("prepared funnel");
    RevisionRecord {
        id: prepared.revision().id.clone(),
        funnel_id: prepared.revision().funnel_id.clone(),
        schema_version: prepared.revision().schema_version,
        data_json: prepared.revision().data_json.clone(),
        render_manifest_json: prepared.revision().render_manifest_json.clone(),
        created_at: "2026-10-14 00:00:00".to_owned(),
    }
}

fn stored_launch() -> RevisionRecord {
    let prepared =
        FunnelCreateCommand::from_json(r#"{"name":"Inspector test","blueprint":"launch"}"#)
            .expect("create command")
            .prepare("funnel-inspector-test", "revision-inspector-test")
            .expect("prepared funnel");
    RevisionRecord {
        id: prepared.revision().id.clone(),
        funnel_id: prepared.revision().funnel_id.clone(),
        schema_version: prepared.revision().schema_version,
        data_json: prepared.revision().data_json.clone(),
        render_manifest_json: prepared.revision().render_manifest_json.clone(),
        created_at: "2026-10-14 00:00:00".to_owned(),
    }
}

#[test]
fn strict_decoder_reconstructs_and_preflights_valid_initial_snapshot() {
    let record = stored_tripwire();
    let decoded = decode_snapshot(&record, &record.funnel_id).expect("stored snapshot decodes");

    assert_eq!(decoded.revision.id().as_str(), record.id);
    assert_eq!(decoded.revision.funnel_id().as_str(), record.funnel_id);
    assert_eq!(decoded.revision.sections().len(), 3);
    assert_eq!(decoded.contents.len(), 3);
}

#[test]
fn strict_decoder_rejects_unknown_fields_versions_manifest_mismatch_and_duplicate_orders() {
    let valid = stored_tripwire();
    let mut cases = Vec::new();

    let mut unknown_field = valid.clone();
    let mut snapshot: Value = serde_json::from_str(&unknown_field.data_json).expect("snapshot");
    snapshot["unexpected"] = serde_json::json!(true);
    unknown_field.data_json = snapshot.to_string();
    cases.push(unknown_field);

    let mut unknown_version = valid.clone();
    let mut snapshot: Value = serde_json::from_str(&unknown_version.data_json).expect("snapshot");
    snapshot["format_version"] = serde_json::json!(2);
    unknown_version.data_json = snapshot.to_string();
    cases.push(unknown_version);

    let mut manifest_mismatch = valid.clone();
    let mut manifest: Value =
        serde_json::from_str(&manifest_mismatch.render_manifest_json).expect("manifest");
    manifest["revision_id"] = serde_json::json!("other-revision");
    manifest_mismatch.render_manifest_json = manifest.to_string();
    cases.push(manifest_mismatch);

    let mut duplicate_order = valid.clone();
    let mut snapshot: Value = serde_json::from_str(&duplicate_order.data_json).expect("snapshot");
    snapshot["sections"][1]["order"] = serde_json::json!(1);
    duplicate_order.data_json = snapshot.to_string();
    cases.push(duplicate_order);

    for corrupted in cases {
        assert!(decode_snapshot(&corrupted, &corrupted.funnel_id).is_err());
    }
}

#[test]
fn v1_decoder_rejects_duplicate_keys_before_order_save_transcoding() {
    let valid = stored_launch();
    for duplicate_json in [
        valid.data_json.replacen(
            "\"format_version\":1,",
            "\"format_version\":1,\"format_version\":1,",
            1,
        ),
        valid.data_json.replacen(
            "\"id\":\"hero\",\"type\"",
            "\"id\":\"hero\",\"id\":\"hero\",\"type\"",
            1,
        ),
    ] {
        let mut corrupted = valid.clone();
        corrupted.data_json = duplicate_json;
        assert!(decode_snapshot(&corrupted, &corrupted.funnel_id).is_err());
    }
}

#[test]
fn editor_reorder_accepts_only_an_exact_permutation_of_existing_section_ids() {
    let record = stored_tripwire();
    let decoded = decode_snapshot(&record, &record.funnel_id).expect("stored snapshot decodes");
    let reversed = decoded
        .revision
        .sections()
        .iter()
        .rev()
        .map(|section| section.id().as_str().to_owned())
        .collect::<Vec<_>>();
    let reordered = reorder_snapshot(&decoded, &reversed).expect("valid section permutation");

    assert_eq!(
        reordered
            .revision
            .sections()
            .iter()
            .map(|section| section.id().as_str())
            .collect::<Vec<_>>(),
        reversed.iter().map(String::as_str).collect::<Vec<_>>()
    );
    assert_eq!(decoded.revision.sections()[0].id().as_str(), "hero");

    let duplicate = vec![
        reversed[0].clone(),
        reversed[0].clone(),
        reversed[2].clone(),
    ];
    assert!(matches!(
        reorder_snapshot(&decoded, &duplicate),
        Err(FunnelCommandError::DuplicateSectionOrder)
    ));
    let unknown = vec![
        reversed[0].clone(),
        reversed[1].clone(),
        "unknown".to_owned(),
    ];
    assert!(reorder_snapshot(&decoded, &unknown).is_err());
    assert!(reorder_snapshot(&decoded, &reversed[..2]).is_err());
}

#[test]
fn inspector_schema_exposes_all_five_controls_and_existing_repeated_fields() {
    let record = stored_launch();
    let decoded = decode_snapshot(&record, &record.funnel_id).expect("stored snapshot decodes");
    let (schema, values) = inspector_projection(&decoded).expect("schema projection");
    let fields = schema["fields"].as_array().expect("field descriptors");
    let controls = fields
        .iter()
        .filter_map(|field| field["control"].as_str())
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(
        controls,
        ["color", "image", "select", "text", "toggle"].into()
    );
    for field_id in [
        "theme.preset",
        "theme.primary_color",
        "theme.square_buttons",
        "section.hero.title",
        "section.features.items.0.title",
        "section.testimonials.items.0.quote",
        "section.footer.links.0.label",
    ] {
        assert!(
            fields.iter().any(|field| field["id"] == field_id),
            "{field_id}"
        );
        assert!(
            values.get(field_id).is_some(),
            "missing value for {field_id}"
        );
    }
    assert!(values["theme.primary_color"]["value"].is_null());
    assert_eq!(values["theme.primary_color"]["effective_value"], "#334155");
}

#[test]
fn inspector_clears_nullable_testimonial_attribution_to_null() {
    let original = stored_launch();
    let decoded = decode_snapshot(&original, &original.funnel_id).expect("V1 decodes");
    let (_, values) = inspector_projection(&decoded).expect("schema projection");
    let field_id = "section.testimonials.items.0.attribution";
    assert_eq!(
        values[field_id]["value"],
        "Sample content — replace before publishing"
    );
    let settings: SettingChanges = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "changes": [{"id": field_id, "value": {"kind":"text", "value":null}}]
    }))
    .expect("nullable text change");
    let cleared = apply_editor_changes(&decoded, &settings.changes).expect("clear attribution");
    let encoded = encode_snapshot(&cleared.snapshot).expect("V1 content-only save");
    assert_eq!(
        serde_json::from_str::<Value>(&encoded).unwrap()["format_version"],
        1
    );

    let mut cleared_record = original.clone();
    cleared_record.data_json = encoded;
    cleared_record.render_manifest_json =
        serde_json::to_string(&RenderManifestV1::from(cleared.revision.render_manifest()))
            .expect("updated manifest");
    let restored =
        decode_snapshot(&cleared_record, &cleared_record.funnel_id).expect("cleared V1 decodes");
    let (_, restored_values) = inspector_projection(&restored).expect("reloaded schema");
    assert!(restored_values[field_id]["value"].is_null());
}

#[test]
fn changing_theme_preset_updates_inherited_color_projection_without_creating_override() {
    let record = stored_launch();
    let decoded = decode_snapshot(&record, &record.funnel_id).expect("V1 decodes");
    let changes: SettingChanges = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "changes": [{"id":"theme.preset", "value":{"kind":"select", "value":"bold"}}]
    }))
    .expect("preset change");
    let edited = apply_editor_changes(&decoded, &changes.changes).expect("preset preview");
    let (_, values) = inspector_projection(&edited).expect("preview schema projection");
    assert_eq!(values["theme.preset"]["value"], "bold");
    assert!(values["theme.primary_color"]["value"].is_null());
    assert_eq!(values["theme.primary_color"]["effective_value"], "#f97316");
}

#[test]
fn inspector_applies_sparse_typed_changes_and_round_trips_v1_as_v2() {
    let original = stored_launch();
    let decoded = decode_snapshot(&original, &original.funnel_id).expect("V1 decodes");
    let settings: SettingChanges = serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "changes": [
            {"id":"section.hero.title", "value":{"kind":"text", "value":"A safer headline"}},
            {"id":"theme.primary_color", "value":{"kind":"color", "value":"#1263ab"}},
            {"id":"theme.preset", "value":{"kind":"select", "value":"bold"}},
            {"id":"theme.square_buttons", "value":{"kind":"toggle", "value":true}},
            {"id":"section.hero.image", "value":{"kind":"image", "value":{"url":"https://example.test/hero.png", "alt":"Product overview"}}},
            {"id":"section.features.items.0.description", "value":{"kind":"text", "value":"Updated feature"}},
            {"id":"section.testimonials.items.0.attribution", "value":{"kind":"text", "value":"A real attribution"}},
            {"id":"section.footer.links.0.url", "value":{"kind":"text", "value":"https://example.test/contact"}}
        ]
    }))
    .expect("well-typed sparse changes");
    let edited = apply_editor_changes(&decoded, &settings.changes).expect("valid edits");
    let encoded = encode_snapshot(&edited.snapshot).expect("V2 serialization");
    let encoded_value: Value = serde_json::from_str(&encoded).expect("V2 JSON");
    assert_eq!(encoded_value["format_version"], 2);
    assert_eq!(encoded_value["schema_version"], 1);
    assert_eq!(encoded_value["theme_overrides"]["primary_color"], "#1263ab");
    assert_eq!(encoded_value["theme_overrides"]["square_buttons"], true);

    let mut v2_record = original.clone();
    v2_record.data_json = encoded;
    v2_record.render_manifest_json =
        serde_json::to_string(&RenderManifestV1::from(edited.revision.render_manifest()))
            .expect("updated render manifest");
    let restored = decode_snapshot(&v2_record, &v2_record.funnel_id).expect("V2 decodes");
    let (_, restored_values) = inspector_projection(&restored).expect("reloaded schema");
    assert_eq!(
        restored_values["section.hero.title"]["value"],
        "A safer headline"
    );
    assert_eq!(restored_values["theme.preset"]["value"], "bold");
    assert_eq!(restored_values["theme.primary_color"]["value"], "#1263ab");
    assert_eq!(restored_values["theme.square_buttons"]["value"], true);
    assert_eq!(
        restored_values["section.hero.image"]["value"]["alt"],
        "Product overview"
    );
    assert_eq!(
        restored_values["section.features.items.0.description"]["value"],
        "Updated feature"
    );
    assert_eq!(
        restored_values["section.testimonials.items.0.attribution"]["value"],
        "A real attribution"
    );
    assert_eq!(
        restored_values["section.footer.links.0.url"]["value"],
        "https://example.test/contact"
    );
}

#[test]
fn inspector_rejects_unknown_fields_duplicates_wrong_controls_and_invalid_values() {
    let record = stored_launch();
    let decoded = decode_snapshot(&record, &record.funnel_id).expect("V1 decodes");
    for malformed in [
        serde_json::json!({"schema_version":1,"changes":[{"id":"theme.primary_color","value":{"kind":"color","value":"#123456"},"path":"theme"}]}),
        serde_json::json!({"schema_version":1,"changes":[{"id":"theme.primary_color","value":{"kind":"color","value":"#123456","extra":true}}]}),
        serde_json::json!({"schema_version":1,"changes":[{"id":"theme.primary_color","value":{"kind":"text","value":"#123456"}}]}),
        serde_json::json!({"schema_version":1,"changes":[{"id":"theme.primary_color","value":{"kind":"color","value":"red"}}]}),
    ] {
        let accepted = serde_json::from_value::<SettingChanges>(malformed)
            .map(|settings| apply_editor_changes(&decoded, &settings.changes).is_err())
            .unwrap_or(true);
        assert!(accepted, "malformed field change was accepted");
    }

    assert!(serde_json::from_value::<SettingChanges>(serde_json::json!({
        "schema_version":1,
        "unexpected":true,
        "changes":[]
    }))
    .is_err());
    assert!(serde_json::from_value::<SettingChanges>(serde_json::json!({
        "schema_version":1,
        "changes":[
            {"id":"theme.primary_color","value":{"kind":"color","value":"#123456"}},
            {"id":"theme.primary_color","value":{"kind":"color","value":"#654321"}}
        ]
    }))
    .is_ok());
    let duplicates: SettingChanges = serde_json::from_value(serde_json::json!({
        "schema_version":1,
        "changes":[
            {"id":"theme.primary_color","value":{"kind":"color","value":"#123456"}},
            {"id":"theme.primary_color","value":{"kind":"color","value":"#654321"}}
        ]
    }))
    .expect("duplicate parses before semantic validation");
    assert!(apply_editor_changes(&decoded, &duplicates.changes).is_err());
}

#[test]
fn v1_order_only_snapshot_encoding_stays_v1_and_v2_unknown_fields_fail_closed() {
    let original = stored_launch();
    let decoded = decode_snapshot(&original, &original.funnel_id).expect("V1 decodes");
    let encoded_v1 = encode_snapshot(&decoded.snapshot).expect("V1 order-only serialization");
    assert_eq!(encoded_v1, original.data_json);
    assert_eq!(
        serde_json::from_str::<Value>(&encoded_v1).unwrap()["format_version"],
        1
    );

    let mut unknown_v2 = original.clone();
    let mut snapshot: Value = serde_json::from_str(&unknown_v2.data_json).expect("V1 JSON");
    snapshot["format_version"] = serde_json::json!(2);
    snapshot["theme_overrides"] =
        serde_json::json!({"primary_color":null,"square_buttons":false,"extra":"no"});
    unknown_v2.data_json = snapshot.to_string();
    assert!(decode_snapshot(&unknown_v2, &unknown_v2.funnel_id).is_err());
}
