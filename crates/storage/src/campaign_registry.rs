use eez_domain::interaction_event::{CampaignId, CampaignRegistry, MAX_CAMPAIGN_IDS_PER_FUNNEL};
use serde::Deserialize;
use worker::d1::D1Type;
use worker::D1Database;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CampaignRegistryStorageError {
    Database,
    NotFound,
    CapacityReached,
    ConcurrentChange,
    InvalidRecord,
}

#[derive(Deserialize)]
struct CampaignRow {
    campaign_id: Option<String>,
}

pub struct CampaignRegistryRepository<'a> {
    database: &'a D1Database,
}

impl<'a> CampaignRegistryRepository<'a> {
    pub fn new(database: &'a D1Database) -> Self {
        Self { database }
    }

    pub async fn read(
        &self,
        funnel_id: &str,
    ) -> Result<CampaignRegistry, CampaignRegistryStorageError> {
        let funnel = D1Type::Text(funnel_id);
        let limit = D1Type::Integer(
            i32::try_from(MAX_CAMPAIGN_IDS_PER_FUNNEL + 1)
                .map_err(|_| CampaignRegistryStorageError::InvalidRecord)?,
        );
        let rows = self
            .database
            .prepare(
                "SELECT funnel_campaigns.campaign_id FROM funnels LEFT JOIN funnel_campaigns ON funnel_campaigns.funnel_id = funnels.id WHERE funnels.id = ? ORDER BY funnel_campaigns.campaign_id ASC LIMIT ?",
            )
            .bind_refs([&funnel, &limit])
            .map_err(|_| CampaignRegistryStorageError::Database)?
            .all()
            .await
            .map_err(|_| CampaignRegistryStorageError::Database)?
            .results::<CampaignRow>()
            .map_err(|_| CampaignRegistryStorageError::Database)?;
        if rows.is_empty() {
            return Err(CampaignRegistryStorageError::NotFound);
        }
        let ids = rows
            .into_iter()
            .filter_map(|row| row.campaign_id)
            .collect::<Vec<_>>();
        CampaignRegistry::new(ids).map_err(|_| CampaignRegistryStorageError::InvalidRecord)
    }

    pub async fn add(
        &self,
        funnel_id: &str,
        campaign_id: &CampaignId,
    ) -> Result<(), CampaignRegistryStorageError> {
        let funnel = D1Type::Text(funnel_id);
        let campaign = D1Type::Text(campaign_id.as_str());
        let capacity = D1Type::Integer(
            i32::try_from(MAX_CAMPAIGN_IDS_PER_FUNNEL)
                .map_err(|_| CampaignRegistryStorageError::InvalidRecord)?,
        );

        for attempt in 0..2 {
            let inserted = self
                .database
                .prepare(
                    "INSERT INTO funnel_campaigns (funnel_id, campaign_id) SELECT ?, ? WHERE EXISTS (SELECT 1 FROM funnels WHERE id = ?) AND (SELECT COUNT(*) FROM funnel_campaigns WHERE funnel_id = ?) < ? ON CONFLICT (funnel_id, campaign_id) DO NOTHING RETURNING campaign_id",
                )
                .bind_refs([
                    &funnel,
                    &campaign,
                    &funnel,
                    &funnel,
                    &capacity,
                ])
                .map_err(|_| CampaignRegistryStorageError::Database)?
                .first::<CampaignRow>(None)
                .await
                .map_err(|_| CampaignRegistryStorageError::Database)?;
            if inserted.is_some() {
                return Ok(());
            }

            let current = self.read(funnel_id).await?;
            if current
                .campaign_ids()
                .any(|registered| registered == campaign_id)
            {
                return Ok(());
            }
            if current.campaign_ids().count() >= MAX_CAMPAIGN_IDS_PER_FUNNEL {
                return Err(CampaignRegistryStorageError::CapacityReached);
            }
            if attempt == 1 {
                return Err(CampaignRegistryStorageError::ConcurrentChange);
            }
        }

        Err(CampaignRegistryStorageError::ConcurrentChange)
    }

    pub async fn remove(
        &self,
        funnel_id: &str,
        campaign_id: &CampaignId,
    ) -> Result<(), CampaignRegistryStorageError> {
        let funnel = D1Type::Text(funnel_id);
        let campaign = D1Type::Text(campaign_id.as_str());
        self.database
            .prepare(
                "DELETE FROM funnel_campaigns WHERE funnel_id = ? AND campaign_id = ? RETURNING campaign_id",
            )
            .bind_refs([&funnel, &campaign])
            .map_err(|_| CampaignRegistryStorageError::Database)?
            .first::<CampaignRow>(None)
            .await
            .map_err(|_| CampaignRegistryStorageError::Database)?;
        self.read(funnel_id).await.map(|_| ())
    }
}
