use super::FunnelDocumentEditV2Command;

const MINIMAL_DOCUMENT: &str =
    include_str!("../../../domain/tests/fixtures/document_v2_minimal.json");

#[test]
fn typed_edit_command_is_closed_and_accepts_stable_operation_identity() {
    let parameters = serde_json::json!({
        "funnel_id": "funnel-01",
        "base_revision_id": "revision-01",
        "page_id": "page-home",
        "document_json": MINIMAL_DOCUMENT,
        "operation_id": "ab".repeat(16),
        "edit": {
            "operation": "add_section",
            "page": "page-home",
            "after": "section-hero",
            "section_type": "features",
        },
    });
    let parsed = FunnelDocumentEditV2Command::from_json(&parameters.to_string())
        .expect("valid typed edit command");
    assert_eq!(parsed.funnel_id, "funnel-01");
    assert_eq!(parsed.base_revision_id, "revision-01");
    assert_eq!(parsed.operation_id, "ab".repeat(16));

    let mut unknown = parameters.clone();
    unknown["unexpected"] = serde_json::json!(true);
    assert!(FunnelDocumentEditV2Command::from_json(&unknown.to_string()).is_err());

    let mut invalid_operation = parameters;
    invalid_operation["operation_id"] = serde_json::json!("AB".repeat(16));
    assert!(FunnelDocumentEditV2Command::from_json(&invalid_operation.to_string()).is_err());
}
