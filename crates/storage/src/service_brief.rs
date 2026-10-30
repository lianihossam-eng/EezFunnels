use std::fmt::{Display, Formatter};

use eez_domain::service_brief::{
    BriefVersion, ServiceBrief, SERVICE_BRIEF_FORMAT_VERSION, SERVICE_BRIEF_MAX_SERIALIZED_BYTES,
};
use serde::Deserialize;
use worker::d1::D1Type;
use worker::{D1Database, Error};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceBriefRecord {
    pub funnel_id: String,
    pub version: BriefVersion,
    pub format_version: u8,
    pub brief: ServiceBrief,
    pub updated_at: Option<String>,
}

#[derive(Debug)]
pub enum ServiceBriefStorageError {
    Database(Error),
    NotFound,
    StaleVersion,
    VersionExhausted,
    UnsupportedFormatVersion(i32),
    InvalidRecord,
}

impl ServiceBriefStorageError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Database(_) | Self::UnsupportedFormatVersion(_) | Self::InvalidRecord => {
                "external/unavailable"
            }
            Self::NotFound => "not_found/funnel",
            Self::StaleVersion | Self::VersionExhausted => "conflict/service_brief_stale",
        }
    }
}

impl Display for ServiceBriefStorageError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for ServiceBriefStorageError {}

impl From<Error> for ServiceBriefStorageError {
    fn from(error: Error) -> Self {
        Self::Database(error)
    }
}

#[derive(Deserialize)]
struct ServiceBriefRow {
    funnel_id: String,
    version: Option<i32>,
    format_version: Option<i32>,
    brief_json: Option<String>,
    updated_at: Option<String>,
}

pub struct ServiceBriefRepository<'a> {
    database: &'a D1Database,
}

impl<'a> ServiceBriefRepository<'a> {
    pub fn new(database: &'a D1Database) -> Self {
        Self { database }
    }

    pub async fn read(
        &self,
        funnel_id: &str,
    ) -> Result<ServiceBriefRecord, ServiceBriefStorageError> {
        let id = D1Type::Text(funnel_id);
        let row = self
            .database
            .prepare(
                "SELECT funnels.id AS funnel_id, funnel_service_briefs.version, funnel_service_briefs.format_version, funnel_service_briefs.brief_json, funnel_service_briefs.updated_at FROM funnels LEFT JOIN funnel_service_briefs ON funnel_service_briefs.funnel_id = funnels.id WHERE funnels.id = ?",
            )
            .bind_refs([&id])?
            .first::<ServiceBriefRow>(None)
            .await?
            .ok_or(ServiceBriefStorageError::NotFound)?;
        Self::decode_row(row)
    }

    pub async fn save(
        &self,
        funnel_id: &str,
        expected_version: BriefVersion,
        brief: &ServiceBrief,
    ) -> Result<ServiceBriefRecord, ServiceBriefStorageError> {
        if funnel_id.trim().is_empty() {
            return Err(ServiceBriefStorageError::InvalidRecord);
        }
        let brief_json =
            serde_json::to_string(brief).map_err(|_| ServiceBriefStorageError::InvalidRecord)?;
        if brief_json.len() > SERVICE_BRIEF_MAX_SERIALIZED_BYTES {
            return Err(ServiceBriefStorageError::InvalidRecord);
        }

        let current = self.read(funnel_id).await?;
        if current.version != expected_version {
            return if current.brief == *brief {
                Ok(current)
            } else {
                Err(ServiceBriefStorageError::StaleVersion)
            };
        }
        if current.brief == *brief {
            return Ok(current);
        }
        let next_version = expected_version
            .next()
            .ok_or(ServiceBriefStorageError::VersionExhausted)?;

        let id = D1Type::Text(funnel_id);
        let version = D1Type::Integer(next_version.value());
        let expected = D1Type::Integer(expected_version.value());
        let format_version = D1Type::Integer(i32::from(SERVICE_BRIEF_FORMAT_VERSION));
        let serialized_brief = D1Type::Text(&brief_json);
        let inserted = self
            .database
            .prepare(
                "INSERT INTO funnel_service_briefs (funnel_id, version, format_version, brief_json, updated_at) SELECT ?, ?, ?, ?, CURRENT_TIMESTAMP WHERE EXISTS (SELECT 1 FROM funnels WHERE id = ?) AND ((? = 0 AND NOT EXISTS (SELECT 1 FROM funnel_service_briefs WHERE funnel_id = ?)) OR EXISTS (SELECT 1 FROM funnel_service_briefs WHERE funnel_id = ? AND version = ?)) ON CONFLICT(funnel_id) DO UPDATE SET version = excluded.version, format_version = excluded.format_version, brief_json = excluded.brief_json, updated_at = CURRENT_TIMESTAMP WHERE funnel_service_briefs.version = ? RETURNING funnel_id, version, format_version, brief_json, updated_at",
            )
            .bind_refs([
                &id,
                &version,
                &format_version,
                &serialized_brief,
                &id,
                &expected,
                &id,
                &id,
                &expected,
                &expected,
            ])?
            .first::<ServiceBriefRow>(None)
            .await?;

        match inserted {
            Some(row) => Self::decode_row(row),
            None => {
                let latest = self.read(funnel_id).await?;
                if latest.brief == *brief {
                    Ok(latest)
                } else {
                    Err(ServiceBriefStorageError::StaleVersion)
                }
            }
        }
    }

    fn decode_row(row: ServiceBriefRow) -> Result<ServiceBriefRecord, ServiceBriefStorageError> {
        match row.brief_json {
            None => {
                if row.version.is_some() || row.format_version.is_some() || row.updated_at.is_some()
                {
                    return Err(ServiceBriefStorageError::InvalidRecord);
                }
                Ok(ServiceBriefRecord {
                    funnel_id: row.funnel_id,
                    version: BriefVersion::initial(),
                    format_version: SERVICE_BRIEF_FORMAT_VERSION,
                    brief: ServiceBrief::default(),
                    updated_at: None,
                })
            }
            Some(brief_json) => {
                let version = row
                    .version
                    .and_then(|version| BriefVersion::try_from(version).ok())
                    .filter(|version| version.value() > 0)
                    .ok_or(ServiceBriefStorageError::InvalidRecord)?;
                let format_version = row
                    .format_version
                    .ok_or(ServiceBriefStorageError::InvalidRecord)?;
                if format_version != i32::from(SERVICE_BRIEF_FORMAT_VERSION) {
                    return Err(ServiceBriefStorageError::UnsupportedFormatVersion(
                        format_version,
                    ));
                }
                if brief_json.len() > SERVICE_BRIEF_MAX_SERIALIZED_BYTES {
                    return Err(ServiceBriefStorageError::InvalidRecord);
                }
                let brief = serde_json::from_str(&brief_json)
                    .map_err(|_| ServiceBriefStorageError::InvalidRecord)?;
                Ok(ServiceBriefRecord {
                    funnel_id: row.funnel_id,
                    version,
                    format_version: SERVICE_BRIEF_FORMAT_VERSION,
                    brief,
                    updated_at: row.updated_at,
                })
            }
        }
    }
}
