use std::collections::HashSet;

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

pub const INTERACTION_EVENT_SCHEMA_VERSION: u16 = 1;
pub const MAX_INTERACTION_EVENT_BYTES: usize = 2_048;
pub const DEFAULT_INTERACTION_RETENTION_DAYS: u8 = 30;
pub const MAX_INTERACTION_RETENTION_DAYS: u8 = 90;

const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
const MIN_REPLAY_KEY_BYTES: usize = 32;

type HmacSha256 = Hmac<Sha256>;

/// Consent to interaction measurement is separate from form processing and marketing consent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InteractionConsent {
    Missing,
    Refused,
    Granted,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionEventKind {
    PageViewed,
    FormViewed,
    FormStarted,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct InteractionEventId(String);

impl InteractionEventId {
    fn parse(value: String) -> Result<Self, InteractionContractError> {
        if !is_uuid_v4(&value) {
            return Err(InteractionContractError::InvalidEventId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct InteractionResourceId(String);

impl InteractionResourceId {
    fn parse(value: String) -> Result<Self, InteractionContractError> {
        if !is_stable_id(&value) {
            return Err(InteractionContractError::InvalidResourceId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct CampaignId(String);

impl CampaignId {
    pub fn parse(value: impl Into<String>) -> Result<Self, InteractionContractError> {
        let value = value.into();
        if !is_stable_id(&value) {
            return Err(InteractionContractError::InvalidCampaignId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Exact owner-configured campaign IDs. Unregistered visitor values are never retained.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CampaignRegistry {
    ids: HashSet<CampaignId>,
}

impl CampaignRegistry {
    pub fn new<I, S>(ids: I) -> Result<Self, InteractionContractError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let ids = ids
            .into_iter()
            .map(CampaignId::parse)
            .collect::<Result<HashSet<_>, _>>()?;
        Ok(Self { ids })
    }

    fn resolve(&self, visitor_value: Option<String>) -> Option<CampaignId> {
        let value = visitor_value?;
        self.ids.get(&CampaignId(value)).cloned()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InteractionEvent {
    event_id: InteractionEventId,
    replay_fingerprint: ReplayFingerprint,
    kind: InteractionEventKind,
    observed_at_unix_seconds: u64,
    funnel_id: InteractionResourceId,
    revision_id: InteractionResourceId,
    page_id: InteractionResourceId,
    form_id: Option<InteractionResourceId>,
    campaign_id: Option<CampaignId>,
}

impl InteractionEvent {
    pub fn event_id(&self) -> &InteractionEventId {
        &self.event_id
    }

    pub fn kind(&self) -> InteractionEventKind {
        self.kind
    }

    /// This client-supplied timestamp is untrusted and is not used for reports or retention.
    pub fn observed_at_unix_seconds(&self) -> u64 {
        self.observed_at_unix_seconds
    }

    pub fn funnel_id(&self) -> &InteractionResourceId {
        &self.funnel_id
    }

    pub fn revision_id(&self) -> &InteractionResourceId {
        &self.revision_id
    }

    pub fn page_id(&self) -> &InteractionResourceId {
        &self.page_id
    }

    pub fn form_id(&self) -> Option<&InteractionResourceId> {
        self.form_id.as_ref()
    }

    pub fn campaign_id(&self) -> Option<&CampaignId> {
        self.campaign_id.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InteractionContractError {
    PayloadTooLarge,
    InvalidPayload,
    InvalidReplayKey,
    UnsupportedSchemaVersion,
    InvalidEventId,
    InvalidResourceId,
    InvalidCampaignId,
    InvalidEventTimestamp,
    FormReferenceMismatch,
    InvalidRetentionDays,
    RetentionTimestampOverflow,
}

/// Validate a bounded version-1 event, gated by explicit interaction consent.
///
/// Consent is checked before reading the body so missing or refused consent cannot
/// produce an event. The registry filters unknown campaign IDs to unattributed.
pub fn decode_interaction_event(
    payload: &[u8],
    consent: InteractionConsent,
    campaigns: &CampaignRegistry,
    replay_key: &[u8],
) -> Result<Option<InteractionEvent>, InteractionContractError> {
    if consent != InteractionConsent::Granted {
        return Ok(None);
    }
    if payload.len() > MAX_INTERACTION_EVENT_BYTES {
        return Err(InteractionContractError::PayloadTooLarge);
    }
    if replay_key.len() < MIN_REPLAY_KEY_BYTES {
        return Err(InteractionContractError::InvalidReplayKey);
    }

    let raw: RawInteractionEvent =
        serde_json::from_slice(payload).map_err(|_| InteractionContractError::InvalidPayload)?;
    if raw.schema_version != INTERACTION_EVENT_SCHEMA_VERSION {
        return Err(InteractionContractError::UnsupportedSchemaVersion);
    }
    if raw.observed_at_unix_seconds == 0 {
        return Err(InteractionContractError::InvalidEventTimestamp);
    }
    let canonical_payload =
        serde_json::to_vec(&raw).map_err(|_| InteractionContractError::InvalidPayload)?;
    let replay_fingerprint = fingerprint(replay_key, &canonical_payload);

    let form_id = raw.form_id.map(InteractionResourceId::parse).transpose()?;
    let requires_form = matches!(
        raw.kind,
        InteractionEventKind::FormViewed | InteractionEventKind::FormStarted
    );
    if requires_form != form_id.is_some() {
        return Err(InteractionContractError::FormReferenceMismatch);
    }

    let event = InteractionEvent {
        event_id: InteractionEventId::parse(raw.event_id)?,
        replay_fingerprint,
        kind: raw.kind,
        observed_at_unix_seconds: raw.observed_at_unix_seconds,
        funnel_id: InteractionResourceId::parse(raw.funnel_id)?,
        revision_id: InteractionResourceId::parse(raw.revision_id)?,
        page_id: InteractionResourceId::parse(raw.page_id)?,
        form_id,
        campaign_id: campaigns.resolve(raw.campaign_id),
    };
    Ok(Some(event))
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawInteractionEvent {
    schema_version: u16,
    event_id: String,
    kind: InteractionEventKind,
    observed_at_unix_seconds: u64,
    funnel_id: String,
    revision_id: String,
    page_id: String,
    #[serde(default)]
    form_id: Option<String>,
    #[serde(default)]
    campaign_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayDisposition {
    NewEvent,
    ExactDuplicate,
    ConflictingEventId,
}

/// Classify a row found by the incoming event ID without extending its retention.
pub fn classify_replay(
    existing: Option<&InteractionEvent>,
    incoming: &InteractionEvent,
) -> ReplayDisposition {
    match existing {
        None => ReplayDisposition::NewEvent,
        Some(existing) if existing.event_id != incoming.event_id => ReplayDisposition::NewEvent,
        Some(existing) if existing.replay_fingerprint == incoming.replay_fingerprint => {
            ReplayDisposition::ExactDuplicate
        }
        Some(_) => ReplayDisposition::ConflictingEventId,
    }
}

/// An installation-keyed fingerprint of the recognized, validated request fields.
/// It allows replay comparison without retaining unregistered campaign input.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReplayFingerprint([u8; 32]);

impl ReplayFingerprint {
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InteractionRetention {
    days: u8,
}

impl InteractionRetention {
    pub fn from_days(days: u8) -> Result<Self, InteractionContractError> {
        if !(1..=MAX_INTERACTION_RETENTION_DAYS).contains(&days) {
            return Err(InteractionContractError::InvalidRetentionDays);
        }
        Ok(Self { days })
    }

    pub const fn days(self) -> u8 {
        self.days
    }

    pub const fn seconds(self) -> u64 {
        self.days as u64 * SECONDS_PER_DAY
    }
}

impl Default for InteractionRetention {
    fn default() -> Self {
        Self {
            days: DEFAULT_INTERACTION_RETENTION_DAYS,
        }
    }
}

/// Server receipt time is trusted and controls report dates and event expiration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceivedInteractionEvent {
    event: InteractionEvent,
    received_at_unix_seconds: u64,
    expires_at_unix_seconds: u64,
}

impl ReceivedInteractionEvent {
    pub fn new(
        event: InteractionEvent,
        received_at_unix_seconds: u64,
        retention: InteractionRetention,
    ) -> Result<Self, InteractionContractError> {
        if received_at_unix_seconds == 0 {
            return Err(InteractionContractError::InvalidEventTimestamp);
        }
        let expires_at_unix_seconds = received_at_unix_seconds
            .checked_add(retention.seconds())
            .ok_or(InteractionContractError::RetentionTimestampOverflow)?;
        Ok(Self {
            event,
            received_at_unix_seconds,
            expires_at_unix_seconds,
        })
    }

    pub fn event(&self) -> &InteractionEvent {
        &self.event
    }

    pub const fn report_timestamp_unix_seconds(&self) -> u64 {
        self.received_at_unix_seconds
    }

    pub const fn expires_at_unix_seconds(&self) -> u64 {
        self.expires_at_unix_seconds
    }

    /// Rows expire at the boundary, matching `expires_at <= now` cleanup semantics.
    pub const fn is_expired_at(&self, now_unix_seconds: u64) -> bool {
        now_unix_seconds >= self.expires_at_unix_seconds
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionReportPeriod {
    Days7,
    Days30,
    Days90,
}

impl InteractionReportPeriod {
    pub const fn from_days(days: u8) -> Option<Self> {
        match days {
            7 => Some(Self::Days7),
            30 => Some(Self::Days30),
            90 => Some(Self::Days90),
            _ => None,
        }
    }

    pub const fn days(self) -> u8 {
        match self {
            Self::Days7 => 7,
            Self::Days30 => 30,
            Self::Days90 => 90,
        }
    }
}

fn is_stable_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn fingerprint(key: &[u8], canonical_payload: &[u8]) -> ReplayFingerprint {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any length");
    mac.update(b"eezfunnel:interaction-event:v1\0");
    mac.update(canonical_payload);
    let digest = mac.finalize().into_bytes();
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(&digest);
    ReplayFingerprint(bytes)
}

fn is_uuid_v4(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23]
            .into_iter()
            .all(|index| bytes[index] == b'-')
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit())
        && bytes[14] == b'4'
        && matches!(bytes[19], b'8' | b'9' | b'a' | b'b')
        && value.bytes().all(|byte| !byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVENT_ID: &str = "018f47a2-8b3c-4d61-8e2a-7f4c9b12d6a0";
    const CAMPAIGN_ID: &str = "spring-launch";
    const REPLAY_KEY: &[u8] = b"contract-test-replay-key-32-bytes";

    fn registry() -> CampaignRegistry {
        CampaignRegistry::new([CAMPAIGN_ID]).expect("valid registry")
    }

    fn payload(kind: &str, form_id: Option<&str>, campaign_id: Option<&str>) -> Vec<u8> {
        let mut value = serde_json::json!({
            "schema_version": 1,
            "event_id": EVENT_ID,
            "kind": kind,
            "observed_at_unix_seconds": 1_700_000_000,
            "funnel_id": "funnel-public",
            "revision_id": "revision-2",
            "page_id": "page-home",
        });
        if let Some(form_id) = form_id {
            value["form_id"] = serde_json::json!(form_id);
        }
        if let Some(campaign_id) = campaign_id {
            value["campaign_id"] = serde_json::json!(campaign_id);
        }
        serde_json::to_vec(&value).expect("serialize event")
    }

    fn decode(bytes: &[u8]) -> Result<Option<InteractionEvent>, InteractionContractError> {
        decode_interaction_event(bytes, InteractionConsent::Granted, &registry(), REPLAY_KEY)
    }

    fn page_view() -> InteractionEvent {
        decode(&payload("page_viewed", None, None))
            .expect("valid event")
            .expect("granted consent")
    }

    #[test]
    fn missing_or_refused_consent_creates_no_event_without_reading_body() {
        for consent in [InteractionConsent::Missing, InteractionConsent::Refused] {
            assert_eq!(
                decode_interaction_event(b"not json", consent, &registry(), REPLAY_KEY),
                Ok(None)
            );
        }
    }

    #[test]
    fn accepts_a_bounded_version_one_event_and_exact_registered_campaign() {
        let event = decode(&payload(
            "form_started",
            Some("form-lead"),
            Some(CAMPAIGN_ID),
        ))
        .expect("valid event")
        .expect("granted consent");
        assert_eq!(event.kind(), InteractionEventKind::FormStarted);
        assert_eq!(
            event.form_id().map(InteractionResourceId::as_str),
            Some("form-lead")
        );
        assert_eq!(
            event.campaign_id().map(CampaignId::as_str),
            Some(CAMPAIGN_ID)
        );
    }

    #[test]
    fn unknown_campaign_ids_are_discarded_and_never_become_attribution() {
        let event = decode(&payload("page_viewed", None, Some("unconfigured-campaign")))
            .expect("event is otherwise valid")
            .expect("granted consent");
        assert_eq!(event.campaign_id(), None);

        let event = decode(&payload("page_viewed", None, Some("Spring-Launch")))
            .expect("campaign matching is exact")
            .expect("granted consent");
        assert_eq!(event.campaign_id(), None);
    }

    #[test]
    fn rejects_unsupported_versions_unknown_fields_and_untrusted_url_fields() {
        let mut unsupported: serde_json::Value =
            serde_json::from_slice(&payload("page_viewed", None, None)).unwrap();
        unsupported["schema_version"] = serde_json::json!(2);
        assert_eq!(
            decode(&serde_json::to_vec(&unsupported).unwrap()),
            Err(InteractionContractError::UnsupportedSchemaVersion)
        );

        for field in ["url", "referrer", "visitor_id", "session_id", "utm_source"] {
            let mut untrusted: serde_json::Value =
                serde_json::from_slice(&payload("page_viewed", None, None)).unwrap();
            untrusted[field] = serde_json::json!("untrusted-value");
            assert_eq!(
                decode(&serde_json::to_vec(&untrusted).unwrap()),
                Err(InteractionContractError::InvalidPayload),
                "unexpectedly accepted {field}"
            );
        }
    }

    #[test]
    fn rejects_oversized_invalid_ids_timestamps_and_mismatched_form_references() {
        let oversized = vec![b' '; MAX_INTERACTION_EVENT_BYTES + 1];
        assert_eq!(
            decode(&oversized),
            Err(InteractionContractError::PayloadTooLarge)
        );

        let mut invalid: serde_json::Value =
            serde_json::from_slice(&payload("page_viewed", None, None)).unwrap();
        invalid["event_id"] = serde_json::json!("visitor@example.test");
        assert_eq!(
            decode(&serde_json::to_vec(&invalid).unwrap()),
            Err(InteractionContractError::InvalidEventId)
        );

        invalid["event_id"] = serde_json::json!(EVENT_ID);
        invalid["page_id"] = serde_json::json!("page?token=secret");
        assert_eq!(
            decode(&serde_json::to_vec(&invalid).unwrap()),
            Err(InteractionContractError::InvalidResourceId)
        );

        invalid["page_id"] = serde_json::json!("page-home");
        invalid["observed_at_unix_seconds"] = serde_json::json!(0);
        assert_eq!(
            decode(&serde_json::to_vec(&invalid).unwrap()),
            Err(InteractionContractError::InvalidEventTimestamp)
        );

        assert_eq!(
            decode(&payload("page_viewed", Some("form-lead"), None)),
            Err(InteractionContractError::FormReferenceMismatch)
        );
        assert_eq!(
            decode(&payload("form_viewed", None, None)),
            Err(InteractionContractError::FormReferenceMismatch)
        );
    }

    #[test]
    fn duplicate_ids_are_idempotent_but_conflicting_reuse_is_rejected() {
        let original = page_view();
        assert_eq!(
            classify_replay(None, &original),
            ReplayDisposition::NewEvent
        );
        assert_eq!(
            classify_replay(Some(&original), &original),
            ReplayDisposition::ExactDuplicate
        );

        let changed = decode(&payload("form_started", Some("form-lead"), None))
            .expect("valid event")
            .expect("granted consent");
        assert_eq!(
            classify_replay(Some(&original), &changed),
            ReplayDisposition::ConflictingEventId
        );
    }

    #[test]
    fn replay_identity_is_stable_across_campaign_registry_changes() {
        let bytes = payload("page_viewed", None, Some(CAMPAIGN_ID));
        let before_registration = decode_interaction_event(
            &bytes,
            InteractionConsent::Granted,
            &CampaignRegistry::default(),
            REPLAY_KEY,
        )
        .unwrap()
        .unwrap();
        let after_registration =
            decode_interaction_event(&bytes, InteractionConsent::Granted, &registry(), REPLAY_KEY)
                .unwrap()
                .unwrap();

        assert_eq!(before_registration.campaign_id(), None);
        assert_eq!(
            after_registration.campaign_id().map(CampaignId::as_str),
            Some(CAMPAIGN_ID)
        );
        assert_eq!(
            classify_replay(Some(&before_registration), &after_registration),
            ReplayDisposition::ExactDuplicate
        );
    }

    #[test]
    fn distinct_unknown_campaign_inputs_do_not_collapse_to_the_same_replay() {
        let registry = CampaignRegistry::default();
        let first = decode_interaction_event(
            &payload("page_viewed", None, Some("unregistered-one")),
            InteractionConsent::Granted,
            &registry,
            REPLAY_KEY,
        )
        .unwrap()
        .unwrap();
        let second = decode_interaction_event(
            &payload("page_viewed", None, Some("unregistered-two")),
            InteractionConsent::Granted,
            &registry,
            REPLAY_KEY,
        )
        .unwrap()
        .unwrap();

        assert_eq!(first.campaign_id(), None);
        assert_eq!(second.campaign_id(), None);
        assert_eq!(
            classify_replay(Some(&first), &second),
            ReplayDisposition::ConflictingEventId
        );
        assert_ne!(
            first.replay_fingerprint.as_bytes(),
            second.replay_fingerprint.as_bytes()
        );
    }

    #[test]
    fn replay_fingerprinting_requires_a_server_key_of_at_least_32_bytes() {
        assert_eq!(
            decode_interaction_event(
                &payload("page_viewed", None, None),
                InteractionConsent::Granted,
                &registry(),
                b"too-short",
            ),
            Err(InteractionContractError::InvalidReplayKey)
        );
    }

    #[test]
    fn delayed_events_use_server_receipt_time_for_reports_and_expiration() {
        let delayed = page_view();
        let retention = InteractionRetention::from_days(1).unwrap();
        let received = ReceivedInteractionEvent::new(delayed, 2_000_000_000, retention).unwrap();
        let expires_at = 2_000_000_000 + SECONDS_PER_DAY;

        assert_eq!(received.report_timestamp_unix_seconds(), 2_000_000_000);
        assert_eq!(received.expires_at_unix_seconds(), expires_at);
        assert!(!received.is_expired_at(expires_at - 1));
        assert!(received.is_expired_at(expires_at));
    }

    #[test]
    fn retention_and_report_windows_are_closed_and_bounded() {
        assert_eq!(
            InteractionRetention::default().days(),
            DEFAULT_INTERACTION_RETENTION_DAYS
        );
        assert_eq!(InteractionRetention::from_days(1).unwrap().days(), 1);
        assert_eq!(
            InteractionRetention::from_days(MAX_INTERACTION_RETENTION_DAYS)
                .unwrap()
                .days(),
            MAX_INTERACTION_RETENTION_DAYS
        );
        for days in [0, 91, u8::MAX] {
            assert_eq!(
                InteractionRetention::from_days(days),
                Err(InteractionContractError::InvalidRetentionDays)
            );
        }

        for days in [7, 30, 90] {
            assert_eq!(
                InteractionReportPeriod::from_days(days).unwrap().days(),
                days
            );
        }
        for days in [0, 1, 8, 29, 31, 89, 91, u8::MAX] {
            assert_eq!(InteractionReportPeriod::from_days(days), None);
        }
    }

    #[test]
    fn campaign_registry_rejects_invalid_owner_configured_ids() {
        assert_eq!(
            CampaignRegistry::new(["contains space"]),
            Err(InteractionContractError::InvalidCampaignId)
        );
    }
}
