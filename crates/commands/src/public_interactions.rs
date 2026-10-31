use eez_domain::document_v2::{BlockV2, DocumentV2, FormId, PageId};
use eez_domain::interaction_event::{
    decode_interaction_event_payload, CampaignRegistry, InteractionConsent, InteractionEvent,
    InteractionEventKind, InteractionEventPayload,
};
use eez_storage::campaign_registry::{CampaignRegistryRepository, CampaignRegistryStorageError};
use eez_storage::repository::{
    NewPublicInteractionEvent, PublicInteractionEventResult, Repository, RepositoryError,
    RevisionRecord,
};

pub struct RecordInteractionInput<'a> {
    pub funnel_slug: &'a str,
    pub payload: InteractionEventPayload,
    pub replay_key: &'a [u8],
    pub retention_days: u8,
    pub limit_per_minute: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordInteractionOutcome {
    Ignored,
    Recorded,
    Duplicate,
    Conflict,
    RateLimited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordInteractionError {
    InvalidEvent,
    NotFound,
    RepositoryUnavailable,
    Storage { code: &'static str },
}

pub async fn record_interaction(
    repository: Option<&Repository<'_>>,
    campaigns: Option<&CampaignRegistryRepository<'_>>,
    input: RecordInteractionInput<'_>,
) -> Result<RecordInteractionOutcome, RecordInteractionError> {
    let event = decode_interaction_event_payload(
        input.payload.clone(),
        InteractionConsent::Granted,
        &CampaignRegistry::default(),
        input.replay_key,
    )
    .map_err(|_| RecordInteractionError::InvalidEvent)?;
    let Some(event) = event else {
        return Ok(RecordInteractionOutcome::Ignored);
    };

    let Some(repository) = repository else {
        return Err(RecordInteractionError::RepositoryUnavailable);
    };
    let revision = match repository
        .get_published_revision_by_slug(input.funnel_slug)
        .await
    {
        Ok(Some(revision)) => revision,
        _ => return Err(RecordInteractionError::NotFound),
    };
    if !interaction_event_matches_revision(&event, &revision) {
        return Err(RecordInteractionError::NotFound);
    }

    let Some(campaigns) = campaigns else {
        return Err(RecordInteractionError::RepositoryUnavailable);
    };
    let registry = campaigns
        .read(event.funnel_id().as_str())
        .await
        .map_err(map_campaign_registry_error)?;
    let event = decode_interaction_event_payload(
        input.payload,
        InteractionConsent::Granted,
        &registry,
        input.replay_key,
    )
    .map_err(|_| RecordInteractionError::InvalidEvent)?
    .ok_or(RecordInteractionError::InvalidEvent)?;

    let fingerprint = interaction_fingerprint_hex(event.replay_fingerprint().as_bytes());
    let storage_result = repository
        .store_public_interaction_event(NewPublicInteractionEvent {
            event_id: event.event_id().as_str(),
            funnel_id: event.funnel_id().as_str(),
            funnel_slug: input.funnel_slug,
            revision_id: event.revision_id().as_str(),
            page_id: event.page_id().as_str(),
            event_kind: interaction_event_kind_name(event.kind()),
            form_id: event.form_id().map(|value| value.as_str()),
            campaign_id: event.campaign_id().map(|value| value.as_str()),
            replay_fingerprint: &fingerprint,
            retention_days: input.retention_days,
            limit_per_minute: input.limit_per_minute,
        })
        .await
        .map_err(|error: RepositoryError| RecordInteractionError::Storage { code: error.code() })?;

    match storage_result {
        PublicInteractionEventResult::Stored => Ok(RecordInteractionOutcome::Recorded),
        PublicInteractionEventResult::Duplicate => Ok(RecordInteractionOutcome::Duplicate),
        PublicInteractionEventResult::KeyReuseMismatch
        | PublicInteractionEventResult::NotCurrent => Ok(RecordInteractionOutcome::Conflict),
        PublicInteractionEventResult::RateLimited => Ok(RecordInteractionOutcome::RateLimited),
    }
}

fn map_campaign_registry_error(error: CampaignRegistryStorageError) -> RecordInteractionError {
    match error {
        CampaignRegistryStorageError::NotFound => RecordInteractionError::NotFound,
        CampaignRegistryStorageError::Database | CampaignRegistryStorageError::InvalidRecord => {
            RecordInteractionError::Storage {
                code: "external/unavailable",
            }
        }
        CampaignRegistryStorageError::CapacityReached
        | CampaignRegistryStorageError::ConcurrentChange => RecordInteractionError::Storage {
            code: "external/unavailable",
        },
    }
}

fn interaction_event_matches_revision(event: &InteractionEvent, revision: &RevisionRecord) -> bool {
    if event.funnel_id().as_str() != revision.funnel_id
        || event.revision_id().as_str() != revision.id
    {
        return false;
    }
    match revision.schema_version {
        1 => event.page_id().as_str() == "legacy-entry" && event.form_id().is_none(),
        2 => {
            let Ok(document) =
                DocumentV2::decode(revision.data_json.as_bytes(), &revision.funnel_id)
            else {
                return false;
            };
            let Ok(page_id) = PageId::parse(event.page_id().as_str().to_owned()) else {
                return false;
            };
            let Some(page) = document.page(&page_id) else {
                return false;
            };
            let Some(form_id) = event.form_id() else {
                return event.kind() == InteractionEventKind::PageViewed;
            };
            let Ok(form_id) = FormId::parse(form_id.as_str().to_owned()) else {
                return false;
            };
            document.form(&form_id).is_some()
                && page.sections().iter().any(|section| {
                    section.blocks().iter().any(|block| {
                        matches!(block, BlockV2::Form { form_id: placed_id, .. } if placed_id == &form_id)
                    })
                })
        }
        _ => false,
    }
}

fn interaction_event_kind_name(kind: InteractionEventKind) -> &'static str {
    match kind {
        InteractionEventKind::PageViewed => "page_viewed",
        InteractionEventKind::FormViewed => "form_viewed",
        InteractionEventKind::FormStarted => "form_started",
    }
}

fn interaction_fingerprint_hex(bytes: &[u8; 32]) -> String {
    let mut value = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
    }
    value
}

#[cfg(test)]
mod tests {
    use super::interaction_event_matches_revision;
    use eez_domain::interaction_event::{
        decode_interaction_event, CampaignRegistry, InteractionConsent,
    };
    use eez_storage::repository::RevisionRecord;

    #[test]
    fn interaction_events_must_reference_the_current_revision_resources() {
        let revision = RevisionRecord {
            id: "revision-a".to_owned(),
            funnel_id: "funnel-01".to_owned(),
            schema_version: 2,
            data_json: include_str!("../../domain/tests/fixtures/document_v2_minimal.json")
                .to_owned(),
            render_manifest_json: "{}".to_owned(),
            created_at: "2026-10-18 00:00:00".to_owned(),
        };
        let page_payload = serde_json::json!({
            "schema_version": 1,
            "event_id": "f0a364dd-688e-4e55-8ad7-7478ba7ae0bd",
            "kind": "page_viewed",
            "observed_at_unix_seconds": 1,
            "funnel_id": "funnel-01",
            "revision_id": "revision-a",
            "page_id": "page-home",
        });
        let page_event = decode_interaction_event(
            page_payload.to_string().as_bytes(),
            InteractionConsent::Granted,
            &CampaignRegistry::default(),
            &[7; 32],
        )
        .expect("valid event envelope")
        .expect("granted event");
        assert!(interaction_event_matches_revision(&page_event, &revision));

        let unknown_page_payload = serde_json::json!({
            "schema_version": 1,
            "event_id": "e3b7ebc8-4c8e-4901-b1cc-b1fac5014d99",
            "kind": "page_viewed",
            "observed_at_unix_seconds": 1,
            "funnel_id": "funnel-01",
            "revision_id": "revision-a",
            "page_id": "page-other",
        });
        let unknown_page = decode_interaction_event(
            unknown_page_payload.to_string().as_bytes(),
            InteractionConsent::Granted,
            &CampaignRegistry::default(),
            &[7; 32],
        )
        .expect("valid event envelope")
        .expect("granted event");
        assert!(!interaction_event_matches_revision(
            &unknown_page,
            &revision
        ));

        let wrong_revision = RevisionRecord {
            id: "revision-b".to_owned(),
            ..revision.clone()
        };
        assert!(!interaction_event_matches_revision(
            &page_event,
            &wrong_revision
        ));
        let legacy_revision = RevisionRecord {
            schema_version: 1,
            ..revision
        };
        assert!(!interaction_event_matches_revision(
            &page_event,
            &legacy_revision
        ));
    }
}
