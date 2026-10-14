use crate::funnel::{required_idempotency_key, FunnelCommandError};

pub mod command_ids {
    pub const FUNNEL_CREATE: &str = "funnel.create";
    pub const FUNNEL_DELETE: &str = "funnel.delete";
    pub const FUNNEL_EDITOR_PREVIEW: &str = "funnel.editor.preview";
    pub const FUNNEL_EDITOR_READ: &str = "funnel.editor.read";
    pub const FUNNEL_LIST: &str = "funnel.list";
    pub const FUNNEL_PUBLIC_READ: &str = "funnel.public.read";
    pub const FUNNEL_PUBLISH: &str = "funnel.publish";
    pub const FUNNEL_REVISION_SAVE: &str = "funnel.revision.save";
    pub const FUNNEL_UNPUBLISH: &str = "funnel.unpublish";
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandActor {
    LocalUser,
    Public,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdempotencyPolicy {
    Required,
    NotRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandHandler {
    Create,
    Delete,
    EditorPreview,
    EditorRead,
    List,
    PublicRead,
    Publish,
    RevisionSave,
    Unpublish,
}

impl CommandHandler {
    #[cfg(test)]
    const fn id(self) -> &'static str {
        match self {
            Self::Create => command_ids::FUNNEL_CREATE,
            Self::Delete => command_ids::FUNNEL_DELETE,
            Self::EditorPreview => command_ids::FUNNEL_EDITOR_PREVIEW,
            Self::EditorRead => command_ids::FUNNEL_EDITOR_READ,
            Self::List => command_ids::FUNNEL_LIST,
            Self::PublicRead => command_ids::FUNNEL_PUBLIC_READ,
            Self::Publish => command_ids::FUNNEL_PUBLISH,
            Self::RevisionSave => command_ids::FUNNEL_REVISION_SAVE,
            Self::Unpublish => command_ids::FUNNEL_UNPUBLISH,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandDefinition {
    id: &'static str,
    description: &'static str,
    actor: CommandActor,
    idempotency_policy: IdempotencyPolicy,
    handler: CommandHandler,
}

impl CommandDefinition {
    pub const fn id(&self) -> &'static str {
        self.id
    }

    pub const fn description(&self) -> &'static str {
        self.description
    }

    pub const fn actor(&self) -> CommandActor {
        self.actor
    }

    pub const fn idempotency_policy(&self) -> IdempotencyPolicy {
        self.idempotency_policy
    }

    #[cfg(test)]
    pub(crate) const fn handler_id(&self) -> &'static str {
        self.handler.id()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedCommand<'key> {
    handler: CommandHandler,
    pub(crate) idempotency_key: Option<&'key str>,
}

const COMMAND_REGISTRY: [CommandDefinition; 9] = [
    CommandDefinition {
        id: command_ids::FUNNEL_CREATE,
        description: "Create a funnel from a built-in blueprint.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::Required,
        handler: CommandHandler::Create,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_DELETE,
        description: "Delete a funnel and its revisions.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::Required,
        handler: CommandHandler::Delete,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_EDITOR_PREVIEW,
        description: "Render an editor revision without saving it.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::NotRequired,
        handler: CommandHandler::EditorPreview,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_EDITOR_READ,
        description: "Read an editable funnel revision.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::NotRequired,
        handler: CommandHandler::EditorRead,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_LIST,
        description: "List funnels in the workspace.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::NotRequired,
        handler: CommandHandler::List,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_PUBLIC_READ,
        description: "Read the published funnel for a public slug.",
        actor: CommandActor::Public,
        idempotency_policy: IdempotencyPolicy::NotRequired,
        handler: CommandHandler::PublicRead,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_PUBLISH,
        description: "Publish a revision with an expected-pointer check.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::Required,
        handler: CommandHandler::Publish,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_REVISION_SAVE,
        description: "Save an immutable funnel revision.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::Required,
        handler: CommandHandler::RevisionSave,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_UNPUBLISH,
        description: "Remove the published revision pointer.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::Required,
        handler: CommandHandler::Unpublish,
    },
];

pub fn command_registry() -> &'static [CommandDefinition] {
    &COMMAND_REGISTRY
}

pub(crate) fn resolve_command<'key>(
    actor: CommandActor,
    command_id: &str,
    idempotency_key: Option<&'key str>,
) -> Result<ResolvedCommand<'key>, FunnelCommandError> {
    let definition = COMMAND_REGISTRY
        .iter()
        .find(|definition| definition.id == command_id)
        .ok_or(FunnelCommandError::UnknownCommand)?;
    if definition.actor != actor {
        return Err(FunnelCommandError::UnknownCommand);
    }
    let idempotency_key = match definition.idempotency_policy {
        IdempotencyPolicy::Required => Some(required_idempotency_key(idempotency_key)?),
        IdempotencyPolicy::NotRequired => None,
    };
    Ok(ResolvedCommand {
        handler: definition.handler,
        idempotency_key,
    })
}

impl ResolvedCommand<'_> {
    pub(crate) const fn handler(self) -> CommandHandler {
        self.handler
    }
}
