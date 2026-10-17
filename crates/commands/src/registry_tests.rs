use super::{command_registry, CommandActor, IdempotencyPolicy, McpExposure};
use crate::funnel::FunnelCommandError;
use crate::registry::resolve_command;

#[test]
fn registry_lists_each_executable_command_once_with_its_actor_and_key_policy() {
    let definitions = command_registry();
    let ids = definitions
        .iter()
        .map(|definition| definition.id())
        .collect::<Vec<_>>();

    assert_eq!(
        ids,
        [
            "funnel.create",
            "funnel.create.v2",
            "funnel.delete",
            "funnel.document.edit.v2",
            "funnel.document.conversion.preview.v2",
            "funnel.document.conversion.commit.v2",
            "funnel.editor.preview",
            "funnel.editor.read",
            "funnel.document.read.v2",
            "funnel.document.preview.v2",
            "funnel.document.stage.v2",
            "funnel.list",
            "form.submission.list",
            "funnel.public.read",
            "funnel.publish",
            "funnel.revision.save",
            "funnel.revision.save.v2",
            "funnel.revision.history.read.v2",
            "funnel.unpublish",
        ]
    );

    for definition in definitions {
        assert_eq!(definition.id(), definition.handler_id());
        assert!(!definition.description().is_empty());
        let expected_actor = if definition.id() == "funnel.public.read" {
            CommandActor::Public
        } else {
            CommandActor::LocalUser
        };
        assert_eq!(definition.actor(), expected_actor, "{}", definition.id());
    }

    let required_keys = definitions
        .iter()
        .filter(|definition| definition.idempotency_policy() == IdempotencyPolicy::Required)
        .map(|definition| definition.id())
        .collect::<Vec<_>>();
    assert_eq!(
        required_keys,
        [
            "funnel.create",
            "funnel.create.v2",
            "funnel.delete",
            "funnel.document.conversion.commit.v2",
            "funnel.document.stage.v2",
            "funnel.publish",
            "funnel.revision.save",
            "funnel.revision.save.v2",
            "funnel.unpublish",
        ]
    );
    let hidden = definitions
        .iter()
        .filter(|definition| definition.mcp_exposure() == McpExposure::Hidden)
        .map(|definition| definition.id())
        .collect::<Vec<_>>();
    assert_eq!(
        hidden,
        [
            "funnel.create.v2",
            "funnel.document.edit.v2",
            "funnel.document.conversion.preview.v2",
            "funnel.document.conversion.commit.v2",
            "funnel.document.read.v2",
            "funnel.document.preview.v2",
            "funnel.document.stage.v2",
            "form.submission.list",
            "funnel.revision.save.v2",
            "funnel.revision.history.read.v2",
        ]
    );
    for definition in definitions {
        let expected_exposure = if hidden.contains(&definition.id()) {
            McpExposure::Hidden
        } else {
            McpExposure::Executable
        };
        assert_eq!(definition.mcp_exposure(), expected_exposure);
    }
}

#[test]
fn dispatch_preflight_requires_keys_only_for_registered_mutations() {
    for id in [
        "funnel.create",
        "funnel.delete",
        "funnel.document.stage.v2",
        "funnel.document.conversion.commit.v2",
        "funnel.revision.save.v2",
        "funnel.publish",
        "funnel.revision.save",
        "funnel.unpublish",
    ] {
        assert!(matches!(
            resolve_command(CommandActor::LocalUser, id, None),
            Err(FunnelCommandError::MissingIdempotencyKey)
        ));
        assert!(resolve_command(CommandActor::LocalUser, id, Some("request-123")).is_ok());
    }

    for id in [
        "funnel.document.edit.v2",
        "funnel.document.conversion.preview.v2",
        "funnel.revision.history.read.v2",
        "funnel.editor.preview",
        "funnel.editor.read",
        "funnel.list",
        "form.submission.list",
    ] {
        assert!(resolve_command(CommandActor::LocalUser, id, None).is_ok());
    }
    assert!(resolve_command(CommandActor::Public, "funnel.public.read", None).is_ok());
}

#[test]
fn dispatch_preflight_rejects_unknown_ids_and_wrong_actors_with_c04_code() {
    for (actor, id) in [
        (CommandActor::LocalUser, "funnel.missing"),
        (CommandActor::Public, "funnel.create"),
        (CommandActor::LocalUser, "funnel.public.read"),
    ] {
        let error = resolve_command(actor, id, None)
            .expect_err("unknown commands and actor mismatches must fail before storage I/O");
        assert_eq!(error.code(), "validation/operation_not_supported");
    }
}

#[test]
fn dispatch_preflight_rejects_malformed_mutation_keys() {
    for key in ["", "contains spaces", "contains/slash"] {
        let error = resolve_command(CommandActor::LocalUser, "funnel.create", Some(key))
            .expect_err("invalid idempotency keys must be rejected before command execution");
        assert_eq!(error.code(), "validation/invalid_json");
    }
}
