use super::{render_v2, FunnelDocumentEditV2Command};
use eez_domain::document_v2::DocumentError;
use eez_domain::document_v2::{DocumentV2, IdKind, IdSource, PageId};

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

#[test]
fn form_builder_operations_decode_as_closed_typed_commands() {
    let mut parameters = serde_json::json!({
        "funnel_id": "funnel-01",
        "base_revision_id": "revision-01",
        "page_id": "page-home",
        "document_json": MINIMAL_DOCUMENT,
        "operation_id": "cd".repeat(16),
        "edit": {
            "operation": "create_form",
            "section": "section-hero",
            "after": "block-copy",
        },
    });
    assert!(FunnelDocumentEditV2Command::from_json(&parameters.to_string()).is_ok());

    parameters["edit"] = serde_json::json!({
        "operation": "add_form_field",
        "form": "form-lead",
        "after": "field-email",
        "field": {
            "kind": "select",
            "name": "interest",
            "label": "Topic",
            "help": "Choose one topic.",
            "required": false,
            "options": ["Consulting", "Training"],
        },
    });
    assert!(FunnelDocumentEditV2Command::from_json(&parameters.to_string()).is_ok());

    parameters["edit"]["field"]["unexpected"] = serde_json::json!(true);
    assert!(FunnelDocumentEditV2Command::from_json(&parameters.to_string()).is_err());
}

#[test]
fn form_builder_command_applies_and_renders_a_new_form_from_the_minimal_document() {
    struct OperationIds {
        operation_id: String,
        next: u32,
    }
    impl IdSource for OperationIds {
        fn next_id(&mut self, _kind: IdKind) -> Result<String, DocumentError> {
            self.next += 1;
            Ok(format!("{}-{}", self.operation_id, self.next))
        }
    }

    let mut document_value: serde_json::Value =
        serde_json::from_str(MINIMAL_DOCUMENT).expect("minimal V2 document");
    document_value["funnel_id"] = serde_json::json!("funnel-01");
    let document_json = document_value.to_string();
    let document = DocumentV2::decode(document_json.as_bytes(), "funnel-01")
        .expect("document has a valid funnel id");
    let command_json = serde_json::json!({
        "funnel_id": "funnel-01",
        "base_revision_id": "revision-01",
        "page_id": "page-home",
        "document_json": document_json,
        "operation_id": "ab".repeat(16),
        "edit": {
            "operation": "create_form",
            "section": "section-hero",
            "after": "block-copy",
        },
    });
    let command = FunnelDocumentEditV2Command::from_json(&command_json.to_string())
        .expect("form creation command parses");
    let mut ids = OperationIds {
        operation_id: command.operation_id,
        next: 0,
    };
    let outcome = document
        .apply(command.edit, &mut ids)
        .expect("form creation applies to the document");
    assert_eq!(outcome.document.forms().len(), 1);
    assert!(!render_v2::render_owner_preview(
        &outcome.document,
        &PageId::parse("page-home").expect("page id"),
    )
    .expect("form preview renders")
    .is_empty());
}
