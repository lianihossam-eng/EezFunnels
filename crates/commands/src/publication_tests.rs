use super::funnel::{
    FunnelCommandError, FunnelPublicReadCommand, FunnelPublishCommand, FunnelUnpublishCommand,
};
use eez_storage::repository::RepositoryError;

#[test]
fn publish_requires_explicit_cas_baseline_and_rejects_duplicate_or_unknown_fields() {
    let command = FunnelPublishCommand::from_json(
        r#"{"funnel_id":"funnel-a","revision_id":"revision-a","expected_published_revision_id":null}"#,
    )
    .expect("null explicitly means unpublished baseline");
    assert_eq!(command.expected_published_revision_id, None);

    assert_eq!(
        FunnelPublishCommand::from_json(r#"{"funnel_id":"funnel-a","revision_id":"revision-a"}"#)
            .expect_err("CAS baseline is required")
            .code(),
        "validation/missing_field"
    );
    assert_eq!(
        FunnelPublishCommand::from_json(
            r#"{"funnel_id":"funnel-a","revision_id":"revision-a","expected_published_revision_id":null,"revision_id":"revision-b"}"#
        )
        .expect_err("duplicate revision is ambiguous")
        .code(),
        "validation/invalid_json"
    );
    assert!(FunnelPublishCommand::from_json(
        r#"{"funnel_id":"funnel-a","revision_id":"revision-a","expected_published_revision_id":null,"html":"<h1>untrusted</h1>"}"#
    )
    .is_err());
}

#[test]
fn publish_missing_revision_is_classified_before_typed_deserialization() {
    assert_eq!(
        FunnelPublishCommand::from_json(
            r#"{"funnel_id":"funnel-a","expected_published_revision_id":null}"#
        )
        .expect_err("revision_id is required")
        .code(),
        "validation/missing_field"
    );
}

#[test]
fn unpublish_requires_a_non_null_expected_published_revision() {
    assert!(FunnelUnpublishCommand::from_json(
        r#"{"funnel_id":"funnel-a","expected_published_revision_id":"revision-a"}"#
    )
    .is_ok());
    assert!(FunnelUnpublishCommand::from_json(
        r#"{"funnel_id":"funnel-a","expected_published_revision_id":null}"#
    )
    .is_err());
    assert!(FunnelUnpublishCommand::from_json(
        r#"{"funnel_id":"funnel-a","expected_published_revision_id":"revision-a","slug":"other"}"#
    )
    .is_err());
}

#[test]
fn unpublish_missing_expected_revision_is_classified_before_typed_deserialization() {
    assert_eq!(
        FunnelUnpublishCommand::from_json(r#"{"funnel_id":"funnel-a"}"#)
            .expect_err("expected revision is required")
            .code(),
        "validation/missing_field"
    );
}

#[test]
fn public_read_accepts_only_a_bounded_generated_slug() {
    assert!(FunnelPublicReadCommand::from_json(r#"{"slug":"quick-offer-revision-a"}"#).is_ok());
    assert!(FunnelPublicReadCommand::from_json(r#"{"slug":"../private"}"#).is_err());
    assert!(FunnelPublicReadCommand::from_json(r#"{"slug":"Quick-offer"}"#).is_err());
    assert!(FunnelPublicReadCommand::from_json(
        &serde_json::json!({"slug": "a".repeat(129)}).to_string()
    )
    .is_err());
}

#[test]
fn public_read_missing_slug_is_classified_before_typed_deserialization() {
    assert_eq!(
        FunnelPublicReadCommand::from_json("{}")
            .expect_err("slug is required")
            .code(),
        "validation/missing_field"
    );
}

#[test]
fn public_unsupported_schema_is_a_validation_error() {
    assert_eq!(
        FunnelCommandError::Storage(RepositoryError::UnsupportedSchemaVersion(2)).code(),
        "validation/schema_version_unsupported"
    );
}
