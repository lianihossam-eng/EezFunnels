use super::FunnelDocumentSaveV2Command;

#[test]
fn v2_save_command_is_closed_and_requires_a_bounded_stage_capability() {
    let stage_id = "ab".repeat(32);
    let digest = "cd".repeat(32);
    let valid = format!(
        r#"{{"funnel_id":"funnel-a","base_revision_id":"revision-a","stage_id":"{stage_id}","expected_length":1048576,"expected_sha256":"{digest}","document_version":2,"schema_version":2,"manifest_version":2}}"#
    );
    let parsed = FunnelDocumentSaveV2Command::from_json(&valid).expect("valid save command");
    assert_eq!(parsed.funnel_id, "funnel-a");
    assert_eq!(parsed.base_revision_id, "revision-a");
    assert_eq!(parsed.stage_id, stage_id);
    assert_eq!(parsed.expected_length, 1_048_576);
    assert_eq!(parsed.expected_sha256, digest);

    let invalid_capability = valid.replace(&stage_id, &"AB".repeat(32));
    assert!(FunnelDocumentSaveV2Command::from_json(&invalid_capability).is_err());
    let unknown = valid.replace("expected_length", "upload_length");
    assert!(FunnelDocumentSaveV2Command::from_json(&unknown).is_err());
    let too_large = valid.replace("1048576", "1048577");
    assert!(FunnelDocumentSaveV2Command::from_json(&too_large).is_err());
    let bad_digest = valid.replace(&digest, &"CD".repeat(32));
    assert!(FunnelDocumentSaveV2Command::from_json(&bad_digest).is_err());
}
