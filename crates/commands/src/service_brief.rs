use eez_domain::service_brief::{BriefVersion, ServiceBrief};
use eez_storage::service_brief::{ServiceBriefRepository, ServiceBriefStorageError};
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceBriefSnapshot {
    pub funnel_id: String,
    pub version: BriefVersion,
    pub format_version: u8,
    pub brief: ServiceBrief,
    pub updated_at: Option<String>,
}

#[derive(Debug)]
pub enum ServiceBriefCommandError {
    InvalidVersion,
    Storage(ServiceBriefStorageError),
}

impl ServiceBriefCommandError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidVersion => "validation/invalid_json",
            Self::Storage(error) => error.code(),
        }
    }
}

impl Display for ServiceBriefCommandError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for ServiceBriefCommandError {}

impl From<ServiceBriefStorageError> for ServiceBriefCommandError {
    fn from(error: ServiceBriefStorageError) -> Self {
        Self::Storage(error)
    }
}

pub async fn read_service_brief(
    repository: &ServiceBriefRepository<'_>,
    funnel_id: &str,
) -> Result<ServiceBriefSnapshot, ServiceBriefCommandError> {
    let record = repository.read(funnel_id).await?;
    Ok(ServiceBriefSnapshot {
        funnel_id: record.funnel_id,
        version: record.version,
        format_version: record.format_version,
        brief: record.brief,
        updated_at: record.updated_at,
    })
}

pub async fn save_service_brief(
    repository: &ServiceBriefRepository<'_>,
    funnel_id: &str,
    expected_version: i32,
    brief: ServiceBrief,
) -> Result<ServiceBriefSnapshot, ServiceBriefCommandError> {
    let expected_version = BriefVersion::try_from(expected_version)
        .map_err(|_| ServiceBriefCommandError::InvalidVersion)?;
    let record = repository.save(funnel_id, expected_version, &brief).await?;
    Ok(ServiceBriefSnapshot {
        funnel_id: record.funnel_id,
        version: record.version,
        format_version: record.format_version,
        brief: record.brief,
        updated_at: record.updated_at,
    })
}
