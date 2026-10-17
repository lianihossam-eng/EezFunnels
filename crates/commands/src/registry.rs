use crate::funnel::{required_idempotency_key, FunnelCommandError};

pub mod command_ids {
    pub const FUNNEL_CREATE: &str = "funnel.create";
    pub const FUNNEL_CREATE_V2: &str = "funnel.create.v2";
    pub const FUNNEL_DELETE: &str = "funnel.delete";
    pub const FUNNEL_DOCUMENT_EDIT_V2: &str = "funnel.document.edit.v2";
    pub const FUNNEL_DOCUMENT_CONVERSION_PREVIEW_V2: &str = "funnel.document.conversion.preview.v2";
    pub const FUNNEL_DOCUMENT_CONVERSION_COMMIT_V2: &str = "funnel.document.conversion.commit.v2";
    pub const FUNNEL_DOCUMENT_READ_V2: &str = "funnel.document.read.v2";
    pub const FUNNEL_REVISION_HISTORY_READ_V2: &str = "funnel.revision.history.read.v2";
    pub const FUNNEL_DOCUMENT_PREVIEW_V2: &str = "funnel.document.preview.v2";
    pub const FUNNEL_DOCUMENT_STAGE_V2: &str = "funnel.document.stage.v2";
    pub const FUNNEL_REVISION_SAVE_V2: &str = "funnel.revision.save.v2";
    pub const FUNNEL_EDITOR_PREVIEW: &str = "funnel.editor.preview";
    pub const FUNNEL_EDITOR_READ: &str = "funnel.editor.read";
    pub const FUNNEL_LIST: &str = "funnel.list";
    pub const FUNNEL_PUBLIC_READ: &str = "funnel.public.read";
    pub const FUNNEL_PUBLISH: &str = "funnel.publish";
    pub const FUNNEL_REVISION_SAVE: &str = "funnel.revision.save";
    pub const FUNNEL_UNPUBLISH: &str = "funnel.unpublish";
    pub const FORM_SUBMISSION_LIST: &str = "form.submission.list";
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
pub enum McpExposure {
    Executable,
    Hidden,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandHandler {
    Create,
    CreateV2,
    Delete,
    DocumentEditV2,
    DocumentConversionPreviewV2,
    DocumentConversionCommitV2,
    EditorPreview,
    EditorRead,
    DocumentReadV2,
    DocumentPreviewV2,
    DocumentStageV2,
    RevisionSaveV2,
    RevisionHistoryReadV2,
    List,
    FormSubmissionList,
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
            Self::CreateV2 => command_ids::FUNNEL_CREATE_V2,
            Self::Delete => command_ids::FUNNEL_DELETE,
            Self::DocumentEditV2 => command_ids::FUNNEL_DOCUMENT_EDIT_V2,
            Self::DocumentConversionPreviewV2 => command_ids::FUNNEL_DOCUMENT_CONVERSION_PREVIEW_V2,
            Self::DocumentConversionCommitV2 => command_ids::FUNNEL_DOCUMENT_CONVERSION_COMMIT_V2,
            Self::EditorPreview => command_ids::FUNNEL_EDITOR_PREVIEW,
            Self::EditorRead => command_ids::FUNNEL_EDITOR_READ,
            Self::DocumentReadV2 => command_ids::FUNNEL_DOCUMENT_READ_V2,
            Self::DocumentPreviewV2 => command_ids::FUNNEL_DOCUMENT_PREVIEW_V2,
            Self::DocumentStageV2 => command_ids::FUNNEL_DOCUMENT_STAGE_V2,
            Self::RevisionSaveV2 => command_ids::FUNNEL_REVISION_SAVE_V2,
            Self::RevisionHistoryReadV2 => command_ids::FUNNEL_REVISION_HISTORY_READ_V2,
            Self::List => command_ids::FUNNEL_LIST,
            Self::FormSubmissionList => command_ids::FORM_SUBMISSION_LIST,
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
    mcp_exposure: McpExposure,
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

    pub const fn mcp_exposure(&self) -> McpExposure {
        self.mcp_exposure
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

const COMMAND_REGISTRY: [CommandDefinition; 19] = [
    CommandDefinition {
        id: command_ids::FUNNEL_CREATE,
        description: "Create a funnel from a built-in blueprint.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::Required,
        mcp_exposure: McpExposure::Executable,
        handler: CommandHandler::Create,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_CREATE_V2,
        description: "Create a typed version-2 funnel from a built-in blueprint.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::Required,
        mcp_exposure: McpExposure::Hidden,
        handler: CommandHandler::CreateV2,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_DELETE,
        description: "Delete a funnel and its revisions.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::Required,
        mcp_exposure: McpExposure::Executable,
        handler: CommandHandler::Delete,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_DOCUMENT_EDIT_V2,
        description: "Apply a typed edit to an owner version-2 draft.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::NotRequired,
        mcp_exposure: McpExposure::Hidden,
        handler: CommandHandler::DocumentEditV2,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_DOCUMENT_CONVERSION_PREVIEW_V2,
        description: "Preview a loss-aware conversion of an owner version-1 funnel.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::NotRequired,
        mcp_exposure: McpExposure::Hidden,
        handler: CommandHandler::DocumentConversionPreviewV2,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_DOCUMENT_CONVERSION_COMMIT_V2,
        description: "Save a validated version-2 conversion as a new immutable revision.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::Required,
        mcp_exposure: McpExposure::Hidden,
        handler: CommandHandler::DocumentConversionCommitV2,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_EDITOR_PREVIEW,
        description: "Render an editor revision without saving it.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::NotRequired,
        mcp_exposure: McpExposure::Executable,
        handler: CommandHandler::EditorPreview,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_EDITOR_READ,
        description: "Read an editable funnel revision.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::NotRequired,
        mcp_exposure: McpExposure::Executable,
        handler: CommandHandler::EditorRead,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_DOCUMENT_READ_V2,
        description: "Read a typed version-2 funnel document for its owner editor.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::NotRequired,
        mcp_exposure: McpExposure::Hidden,
        handler: CommandHandler::DocumentReadV2,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_DOCUMENT_PREVIEW_V2,
        description: "Validate and preview a typed version-2 funnel draft.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::NotRequired,
        mcp_exposure: McpExposure::Hidden,
        handler: CommandHandler::DocumentPreviewV2,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_DOCUMENT_STAGE_V2,
        description: "Mint a bounded private stage for a typed version-2 document.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::Required,
        mcp_exposure: McpExposure::Hidden,
        handler: CommandHandler::DocumentStageV2,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_LIST,
        description: "List funnels in the workspace.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::NotRequired,
        mcp_exposure: McpExposure::Executable,
        handler: CommandHandler::List,
    },
    CommandDefinition {
        id: command_ids::FORM_SUBMISSION_LIST,
        description: "List retained public form submissions for the local owner.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::NotRequired,
        mcp_exposure: McpExposure::Hidden,
        handler: CommandHandler::FormSubmissionList,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_PUBLIC_READ,
        description: "Read the published funnel for a public slug.",
        actor: CommandActor::Public,
        idempotency_policy: IdempotencyPolicy::NotRequired,
        mcp_exposure: McpExposure::Executable,
        handler: CommandHandler::PublicRead,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_PUBLISH,
        description: "Publish a revision with an expected-pointer check.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::Required,
        mcp_exposure: McpExposure::Executable,
        handler: CommandHandler::Publish,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_REVISION_SAVE,
        description: "Save an immutable funnel revision.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::Required,
        mcp_exposure: McpExposure::Executable,
        handler: CommandHandler::RevisionSave,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_REVISION_SAVE_V2,
        description: "Validate and save a staged typed version-2 snapshot.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::Required,
        mcp_exposure: McpExposure::Hidden,
        handler: CommandHandler::RevisionSaveV2,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_REVISION_HISTORY_READ_V2,
        description: "Read bounded version-2 revision history for its owner editor.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::NotRequired,
        mcp_exposure: McpExposure::Hidden,
        handler: CommandHandler::RevisionHistoryReadV2,
    },
    CommandDefinition {
        id: command_ids::FUNNEL_UNPUBLISH,
        description: "Remove the published revision pointer.",
        actor: CommandActor::LocalUser,
        idempotency_policy: IdempotencyPolicy::Required,
        mcp_exposure: McpExposure::Executable,
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
