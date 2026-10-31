use eez_domain::interaction_event::{
    CampaignId, InteractionContractError, MAX_CAMPAIGN_IDS_PER_FUNNEL,
};
use eez_storage::campaign_registry::{CampaignRegistryRepository, CampaignRegistryStorageError};
use eez_storage::repository::{Repository, RepositoryError};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignRegistrySnapshot {
    pub funnel_id: String,
    pub funnel_slug: String,
    pub published: bool,
    pub max_campaign_ids: usize,
    pub campaign_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CampaignRegistryCommandError {
    InvalidCampaignId,
    NotFound,
    CapacityReached,
    ConcurrentChange,
    StorageUnavailable,
}

impl CampaignRegistryCommandError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidCampaignId => "validation/campaign_id_invalid",
            Self::NotFound => "not_found/funnel",
            Self::CapacityReached => "conflict/campaign_registry_full",
            Self::ConcurrentChange => "conflict/campaign_registry_changed",
            Self::StorageUnavailable => "external/unavailable",
        }
    }
}

pub async fn read_campaign_registry(
    repository: &Repository<'_>,
    campaigns: &CampaignRegistryRepository<'_>,
    funnel_id: &str,
) -> Result<CampaignRegistrySnapshot, CampaignRegistryCommandError> {
    validate_funnel_id(funnel_id)?;
    snapshot(repository, campaigns, funnel_id).await
}

pub async fn add_campaign_id(
    repository: &Repository<'_>,
    campaigns: &CampaignRegistryRepository<'_>,
    funnel_id: &str,
    value: &str,
) -> Result<CampaignRegistrySnapshot, CampaignRegistryCommandError> {
    validate_funnel_id(funnel_id)?;
    let campaign_id = CampaignId::parse(value).map_err(map_campaign_id_error)?;
    campaigns
        .add(funnel_id, &campaign_id)
        .await
        .map_err(map_campaign_storage_error)?;
    snapshot(repository, campaigns, funnel_id).await
}

pub async fn remove_campaign_id(
    repository: &Repository<'_>,
    campaigns: &CampaignRegistryRepository<'_>,
    funnel_id: &str,
    value: &str,
) -> Result<CampaignRegistrySnapshot, CampaignRegistryCommandError> {
    validate_funnel_id(funnel_id)?;
    let campaign_id = CampaignId::parse(value).map_err(map_campaign_id_error)?;
    campaigns
        .remove(funnel_id, &campaign_id)
        .await
        .map_err(map_campaign_storage_error)?;
    snapshot(repository, campaigns, funnel_id).await
}

async fn snapshot(
    repository: &Repository<'_>,
    campaigns: &CampaignRegistryRepository<'_>,
    funnel_id: &str,
) -> Result<CampaignRegistrySnapshot, CampaignRegistryCommandError> {
    let funnel = repository
        .get_funnel(funnel_id)
        .await
        .map_err(map_repository_error)?
        .ok_or(CampaignRegistryCommandError::NotFound)?;
    let registry = campaigns
        .read(funnel_id)
        .await
        .map_err(map_campaign_storage_error)?;
    let campaign_ids = registry
        .campaign_ids()
        .map(|campaign_id| campaign_id.as_str().to_owned())
        .collect();
    Ok(CampaignRegistrySnapshot {
        funnel_id: funnel.id,
        funnel_slug: funnel.slug,
        published: funnel.published_revision_id.is_some(),
        max_campaign_ids: MAX_CAMPAIGN_IDS_PER_FUNNEL,
        campaign_ids,
    })
}

fn validate_funnel_id(value: &str) -> Result<(), CampaignRegistryCommandError> {
    if (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        Ok(())
    } else {
        Err(CampaignRegistryCommandError::NotFound)
    }
}

fn map_campaign_id_error(_: InteractionContractError) -> CampaignRegistryCommandError {
    CampaignRegistryCommandError::InvalidCampaignId
}

fn map_campaign_storage_error(error: CampaignRegistryStorageError) -> CampaignRegistryCommandError {
    match error {
        CampaignRegistryStorageError::NotFound => CampaignRegistryCommandError::NotFound,
        CampaignRegistryStorageError::CapacityReached => {
            CampaignRegistryCommandError::CapacityReached
        }
        CampaignRegistryStorageError::ConcurrentChange => {
            CampaignRegistryCommandError::ConcurrentChange
        }
        CampaignRegistryStorageError::Database | CampaignRegistryStorageError::InvalidRecord => {
            CampaignRegistryCommandError::StorageUnavailable
        }
    }
}

fn map_repository_error(error: RepositoryError) -> CampaignRegistryCommandError {
    match error.code() {
        "not_found/funnel" => CampaignRegistryCommandError::NotFound,
        _ => CampaignRegistryCommandError::StorageUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::{map_campaign_id_error, validate_funnel_id, CampaignRegistryCommandError};
    use eez_domain::interaction_event::InteractionContractError;

    #[test]
    fn funnel_route_ids_match_the_owner_api_contract() {
        assert_eq!(validate_funnel_id("funnel-01"), Ok(()));
        assert_eq!(
            validate_funnel_id("funnel id"),
            Err(CampaignRegistryCommandError::NotFound)
        );
    }

    #[test]
    fn campaign_validation_errors_have_owner_api_codes() {
        assert_eq!(
            map_campaign_id_error(InteractionContractError::InvalidCampaignId),
            CampaignRegistryCommandError::InvalidCampaignId
        );
        assert_eq!(
            CampaignRegistryCommandError::CapacityReached.code(),
            "conflict/campaign_registry_full"
        );
    }
}
