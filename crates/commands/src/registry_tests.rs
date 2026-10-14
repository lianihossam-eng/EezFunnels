use super::{command_registry, CommandActor, IdempotencyPolicy};
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
            "funnel.delete",
            "funnel.editor.preview",
            "funnel.editor.read",
            "funnel.list",
            "funnel.public.read",
            "funnel.publish",
            "funnel.revision.save",
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
            "funnel.delete",
            "funnel.publish",
            "funnel.revision.save",
            "funnel.unpublish",
        ]
    );
}

#[test]
fn dispatch_preflight_requires_keys_only_for_registered_mutations() {
    for id in [
        "funnel.create",
        "funnel.delete",
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

    for id in ["funnel.editor.preview", "funnel.editor.read", "funnel.list"] {
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
