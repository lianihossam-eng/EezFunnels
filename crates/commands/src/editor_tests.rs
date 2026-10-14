use super::funnel::{decode_snapshot, reorder_snapshot, FunnelCommandError};
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
