#![forbid(unsafe_code)]

use std::fmt::{Display, Formatter};

use eez_domain::{DomainError, Revision, RevisionId, ThemePreset};
use eez_render::{RenderError, ThemeTokens, RENDERER_VERSION};
use serde::{Deserialize, Serialize};

pub mod campaign_registry;
mod funnel;
pub mod public_delivery;
pub mod public_delivery_controls;
pub mod public_forms;
pub mod public_interactions;
mod registry;
pub mod service_brief;
pub mod submission_follow_up;
pub mod submission_lifecycle;

pub use eez_domain::FunnelBlueprint;
#[cfg(test)]
pub(crate) use funnel::{render_published_revision, v2_page_projection};
pub use funnel::{
    FunnelCommandError, FunnelCreateCommand, FunnelCreateCommandV2, FunnelDeleteCommand,
    FunnelListCommand, InitialRevisionSummary, PreparedFunnel, PreparedFunnelV2, Session,
};
pub use registry::{
    command_ids, command_registry, CommandActor, CommandDefinition, IdempotencyPolicy, McpExposure,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeSetCommand {
    theme_id: ThemePreset,
}

impl ThemeSetCommand {
    pub const ID: &str = "theme.set";

    pub fn from_json(parameters: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(parameters)
    }

    pub const fn theme_id(&self) -> ThemePreset {
        self.theme_id
    }

    pub fn execute(
        &self,
        source: &Revision,
        next_revision_id: RevisionId,
    ) -> Result<ThemeSetResult, ThemeSetError> {
        let tokens = ThemeTokens::from_preset(self.theme_id)?;
        let revision =
            source.fork_with_theme(next_revision_id, tokens.version(), RENDERER_VERSION)?;
        Ok(ThemeSetResult { revision, tokens })
    }
}

pub struct ThemeSetResult {
    revision: Revision,
    tokens: ThemeTokens,
}

impl ThemeSetResult {
    pub fn revision(&self) -> &Revision {
        &self.revision
    }

    pub fn tokens(&self) -> &ThemeTokens {
        &self.tokens
    }
}

#[derive(Debug)]
pub enum ThemeSetError {
    Domain(DomainError),
    Render(RenderError),
}

impl Display for ThemeSetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Domain(error) => Display::fmt(error, formatter),
            Self::Render(error) => Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for ThemeSetError {}

impl From<DomainError> for ThemeSetError {
    fn from(error: DomainError) -> Self {
        Self::Domain(error)
    }
}

impl From<RenderError> for ThemeSetError {
    fn from(error: RenderError) -> Self {
        Self::Render(error)
    }
}

#[cfg(test)]
mod theme_set_tests;

#[cfg(test)]
mod funnel_create_tests;

#[cfg(test)]
mod editor_tests;

#[cfg(test)]
mod publication_tests;

#[cfg(test)]
mod registry_tests;
