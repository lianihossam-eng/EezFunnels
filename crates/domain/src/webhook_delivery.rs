use std::collections::{BTreeMap, BTreeSet};

use hmac::{Hmac, Mac};
use serde::Serialize;
use serde_json::Value;
use sha2::Sha256;
use url::{Host, Url};

use crate::document_v2::{DocumentV2, FieldId, FormId, PageId};

const MAX_DESTINATION_BYTES: usize = 2_048;
const MAX_MAPPING_BYTES: usize = 16_384;
const MAX_MAPPING_FIELDS: usize = 50;
const MAX_SUBMISSION_BYTES: usize = 512 * 1024;
const SIGNATURE_DOMAIN: &[u8] = b"eezfunnel:webhook:v1\n";

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Eq, PartialEq)]
pub struct ValidatedWebhookDestination(String);

impl ValidatedWebhookDestination {
    pub fn parse(value: &str) -> Result<Self, WebhookPreparationError> {
        if value.len() > MAX_DESTINATION_BYTES
            || value.chars().any(|character| {
                character.is_control()
                    || character.is_whitespace()
                    || matches!(character, '"' | '\'' | '<' | '>' | '\\')
            })
        {
            return Err(WebhookPreparationError::InvalidDestination);
        }

        let parsed = Url::parse(value).map_err(|_| WebhookPreparationError::InvalidDestination)?;
        let Some(host) = parsed.host() else {
            return Err(WebhookPreparationError::InvalidDestination);
        };
        let authority_has_user_info = value
            .split_once("://")
            .and_then(|(_, remainder)| {
                remainder
                    .find(['/', '?', '#'])
                    .map_or(Some(remainder), |end| remainder.get(..end))
            })
            .is_some_and(|authority| authority.contains('@'));
        let Host::Domain(host) = host else {
            return Err(WebhookPreparationError::InvalidDestination);
        };
        if parsed.scheme() != "https"
            || authority_has_user_info
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.fragment().is_some()
            || parsed.port_or_known_default() != Some(443)
            || is_reserved_delivery_host(host)
        {
            return Err(WebhookPreparationError::InvalidDestination);
        }

        Ok(Self(parsed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_reserved_delivery_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    [
        "localhost",
        "local",
        "internal",
        "test",
        "invalid",
        "example",
    ]
    .iter()
    .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
        || ["example.com", "example.net", "example.org"]
            .iter()
            .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
}

#[derive(Clone, Eq, PartialEq)]
pub struct SelectedWebhookFields(Vec<FieldId>);

impl SelectedWebhookFields {
    pub fn parse(value: &str) -> Result<Self, WebhookPreparationError> {
        if value.len() > MAX_MAPPING_BYTES {
            return Err(WebhookPreparationError::IncompatibleMapping);
        }
        let fields = serde_json::from_str::<Vec<FieldId>>(value)
            .map_err(|_| WebhookPreparationError::IncompatibleMapping)?;
        if fields.is_empty() || fields.len() > MAX_MAPPING_FIELDS {
            return Err(WebhookPreparationError::IncompatibleMapping);
        }
        let unique = fields.iter().collect::<BTreeSet<_>>();
        if unique.len() != fields.len() {
            return Err(WebhookPreparationError::IncompatibleMapping);
        }
        Ok(Self(fields))
    }
}

pub struct WebhookSubmissionSnapshot<'a> {
    pub delivery_id: &'a str,
    pub funnel_id: &'a str,
    pub revision_id: &'a str,
    pub page_id: &'a str,
    pub form_id: &'a str,
    pub submitted_at: i64,
    pub document_json: &'a str,
    pub selected_fields_json: &'a str,
    pub field_values_json: &'a str,
}

pub struct PreparedWebhookDelivery {
    destination: ValidatedWebhookDestination,
    delivery_id: String,
    body: Vec<u8>,
}

#[derive(Serialize)]
struct SubmissionEventV1<'a> {
    event: &'static str,
    version: u8,
    delivery_id: &'a str,
    submission: SubmissionSnapshotV1<'a>,
}

#[derive(Serialize)]
struct SubmissionSnapshotV1<'a> {
    funnel_id: &'a str,
    revision_id: &'a str,
    page_id: &'a str,
    form_id: &'a str,
    submitted_at: i64,
    fields: BTreeMap<String, Value>,
}

pub struct WebhookSigningKey(Vec<u8>);

impl WebhookSigningKey {
    pub fn parse(value: &str) -> Result<Self, WebhookPreparationError> {
        if value.len() < 32 {
            return Err(WebhookPreparationError::InvalidSigningKey);
        }
        Ok(Self(value.as_bytes().to_vec()))
    }
}

pub struct SignedWebhookDelivery {
    destination: ValidatedWebhookDestination,
    delivery_id: String,
    timestamp: u64,
    body: Vec<u8>,
    signature: String,
}

impl SignedWebhookDelivery {
    pub fn destination(&self) -> &str {
        self.destination.as_str()
    }

    pub fn delivery_id(&self) -> &str {
        &self.delivery_id
    }

    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    pub fn signature(&self) -> &str {
        &self.signature
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebhookPreparationError {
    InvalidDestination,
    InvalidSigningKey,
    InvalidDeliveryId,
    InvalidTimestamp,
    IncompatibleMapping,
    InvalidSubmission,
    SigningFailed,
    EncodingFailure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebhookTerminalReason {
    InvalidDestination,
    IncompatibleMapping,
    InvalidSubmission,
    SigningFailed,
    RedirectRefused,
    ReceiverRejected,
    InvalidResponse,
    RetryExhausted,
    DeliveryWindowElapsed,
}

impl WebhookTerminalReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidDestination => "invalid_destination",
            Self::IncompatibleMapping => "incompatible_mapping",
            Self::InvalidSubmission => "invalid_submission",
            Self::SigningFailed => "signing_failed",
            Self::RedirectRefused => "redirect_refused",
            Self::ReceiverRejected => "receiver_rejected",
            Self::InvalidResponse => "invalid_response",
            Self::RetryExhausted => "retry_exhausted",
            Self::DeliveryWindowElapsed => "delivery_window_elapsed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebhookHttpOutcome {
    Accepted {
        status: u16,
    },
    Retryable {
        status: u16,
    },
    Terminal {
        status: u16,
        reason: WebhookTerminalReason,
    },
}

pub fn prepare_webhook_payload(
    destination: ValidatedWebhookDestination,
    snapshot: WebhookSubmissionSnapshot<'_>,
) -> Result<PreparedWebhookDelivery, WebhookPreparationError> {
    if !is_uuid_v4(snapshot.delivery_id) {
        return Err(WebhookPreparationError::InvalidDeliveryId);
    }
    let delivery_id = snapshot.delivery_id.to_owned();
    let document = DocumentV2::decode(snapshot.document_json.as_bytes(), snapshot.funnel_id)
        .map_err(|_| WebhookPreparationError::InvalidSubmission)?;
    if document.funnel_id().as_str() != snapshot.funnel_id {
        return Err(WebhookPreparationError::InvalidSubmission);
    }
    let page_id = PageId::parse(snapshot.page_id.to_owned())
        .map_err(|_| WebhookPreparationError::InvalidSubmission)?;
    let form_id = FormId::parse(snapshot.form_id.to_owned())
        .map_err(|_| WebhookPreparationError::InvalidSubmission)?;
    let page = document
        .page(&page_id)
        .ok_or(WebhookPreparationError::InvalidSubmission)?;
    let form = document
        .form(&form_id)
        .ok_or(WebhookPreparationError::IncompatibleMapping)?;
    if !page.sections().iter().any(|section| {
        section.blocks().iter().any(
            |block| matches!(block, crate::document_v2::BlockV2::Form { form_id: placed, .. } if placed == &form_id),
        )
    }) {
        return Err(WebhookPreparationError::InvalidSubmission);
    }

    let selected_fields = SelectedWebhookFields::parse(snapshot.selected_fields_json)?;
    let values = if snapshot.field_values_json.len() <= MAX_SUBMISSION_BYTES {
        serde_json::from_str::<BTreeMap<String, Value>>(snapshot.field_values_json)
            .map_err(|_| WebhookPreparationError::InvalidSubmission)?
    } else {
        return Err(WebhookPreparationError::InvalidSubmission);
    };
    let mut mapped_values = BTreeMap::new();
    for selected in selected_fields.0 {
        let field = form
            .fields()
            .iter()
            .find(|field| field.id() == &selected)
            .ok_or(WebhookPreparationError::IncompatibleMapping)?;
        let value = values
            .get(field.submission_key())
            .cloned()
            .unwrap_or(Value::Null);
        if !matches!(value, Value::Null | Value::Bool(_) | Value::String(_)) {
            return Err(WebhookPreparationError::InvalidSubmission);
        }
        mapped_values.insert(selected.as_str().to_owned(), value);
    }

    let event = SubmissionEventV1 {
        event: "submission.received",
        version: 1,
        delivery_id: &delivery_id,
        submission: SubmissionSnapshotV1 {
            funnel_id: snapshot.funnel_id,
            revision_id: snapshot.revision_id,
            page_id: snapshot.page_id,
            form_id: snapshot.form_id,
            submitted_at: snapshot.submitted_at,
            fields: mapped_values,
        },
    };
    let body = serde_json_canonicalizer::to_vec(&event)
        .map_err(|_| WebhookPreparationError::EncodingFailure)?;
    Ok(PreparedWebhookDelivery {
        destination,
        delivery_id,
        body,
    })
}

pub fn sign_webhook_delivery(
    prepared: PreparedWebhookDelivery,
    timestamp: u64,
    signing_key: &WebhookSigningKey,
) -> Result<SignedWebhookDelivery, WebhookPreparationError> {
    if timestamp == 0 {
        return Err(WebhookPreparationError::InvalidTimestamp);
    }
    let signature = sign_webhook_bytes(
        signing_key,
        timestamp,
        &prepared.delivery_id,
        &prepared.body,
    )?;
    Ok(SignedWebhookDelivery {
        destination: prepared.destination,
        delivery_id: prepared.delivery_id,
        timestamp,
        body: prepared.body,
        signature,
    })
}

pub fn classify_webhook_status(status: u16) -> WebhookHttpOutcome {
    match status {
        200..=299 => WebhookHttpOutcome::Accepted { status },
        408 | 429 | 500..=599 => WebhookHttpOutcome::Retryable { status },
        300..=399 => WebhookHttpOutcome::Terminal {
            status,
            reason: WebhookTerminalReason::RedirectRefused,
        },
        100..=499 => WebhookHttpOutcome::Terminal {
            status,
            reason: WebhookTerminalReason::ReceiverRejected,
        },
        _ => WebhookHttpOutcome::Terminal {
            status,
            reason: WebhookTerminalReason::InvalidResponse,
        },
    }
}

pub const fn retry_delay_seconds(attempt_number: u8) -> Option<i64> {
    match attempt_number {
        1 => Some(60),
        2 => Some(300),
        3 => Some(1_800),
        4 => Some(7_200),
        _ => None,
    }
}

pub fn verify_webhook_signature(
    signing_key: &WebhookSigningKey,
    timestamp: u64,
    delivery_id: &str,
    body: &[u8],
    signature: &str,
) -> bool {
    let Ok(signature_bytes) = decode_signature(signature) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(&signing_key.0) else {
        return false;
    };
    mac.update(SIGNATURE_DOMAIN);
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b"\n");
    mac.update(delivery_id.as_bytes());
    mac.update(b"\n");
    mac.update(body);
    mac.verify_slice(&signature_bytes).is_ok()
}

fn sign_webhook_bytes(
    signing_key: &WebhookSigningKey,
    timestamp: u64,
    delivery_id: &str,
    body: &[u8],
) -> Result<String, WebhookPreparationError> {
    let mut mac = HmacSha256::new_from_slice(&signing_key.0)
        .map_err(|_| WebhookPreparationError::InvalidSigningKey)?;
    mac.update(SIGNATURE_DOMAIN);
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b"\n");
    mac.update(delivery_id.as_bytes());
    mac.update(b"\n");
    mac.update(body);
    Ok(encode_hex(&mac.finalize().into_bytes()))
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn decode_signature(value: &str) -> Result<Vec<u8>, ()> {
    let value = value.strip_prefix("v1=").ok_or(())?;
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(());
    }
    let mut bytes = Vec::with_capacity(32);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = (pair[0] as char).to_digit(16).ok_or(())?;
        let low = (pair[1] as char).to_digit(16).ok_or(())?;
        bytes.push(((high << 4) | low) as u8);
    }
    Ok(bytes)
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
    use crate::document_v2::BlueprintId;
    use crate::ThemePreset;

    const DELIVERY_ID: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    const SIGNING_SECRET: &str = "PUB-009 fictional webhook signing secret 32 bytes";

    fn document() -> DocumentV2 {
        DocumentV2::starter("funnel-public", BlueprintId::LeadGen, ThemePreset::Minimal)
            .expect("lead generation starter is valid")
    }

    fn prepared(
        destination: &str,
        selected_fields_json: &str,
        field_values_json: &str,
    ) -> Result<SignedWebhookDelivery, WebhookPreparationError> {
        let document = document();
        let signing_key =
            WebhookSigningKey::parse(SIGNING_SECRET).expect("test key is long enough");
        let prepared = prepare_webhook_payload(
            ValidatedWebhookDestination::parse(destination).expect("destination is valid"),
            WebhookSubmissionSnapshot {
                delivery_id: DELIVERY_ID,
                funnel_id: "funnel-public",
                revision_id: "revision-accepted",
                page_id: "page-home",
                form_id: "form-service-inquiry",
                submitted_at: 1_800_000_000,
                document_json: std::str::from_utf8(document.canonical_bytes())
                    .expect("canonical document is utf8"),
                selected_fields_json,
                field_values_json,
            },
        )?;
        sign_webhook_delivery(prepared, 1_800_000_030, &signing_key)
    }

    #[test]
    fn destination_accepts_only_validated_operator_https_hosts() {
        assert_eq!(
            ValidatedWebhookDestination::parse("https://hooks.public-site.dev/path")
                .expect("public HTTPS destination")
                .as_str(),
            "https://hooks.public-site.dev/path"
        );
        for destination in [
            "http://hooks.public-site.dev/path",
            "https://user:pass@hooks.public-site.dev/path",
            "https://@hooks.public-site.dev/path",
            "https://127.0.0.1/path",
            "https://[::1]/path",
            "https://localhost/path",
            "https://hooks.local/path",
            "https://hooks.test/path",
            "https://receiver.invalid/path",
            "https://receiver.example/path",
            "https://example.com/path",
            "https://hooks.public-site.dev:444/path",
            "https://hooks.public-site.dev/path#fragment",
            " https://hooks.public-site.dev/path",
        ] {
            assert!(
                matches!(
                    ValidatedWebhookDestination::parse(destination),
                    Err(WebhookPreparationError::InvalidDestination)
                ),
                "destination should fail closed: {destination}"
            );
        }
    }

    #[test]
    fn payload_contains_only_selected_values_and_uses_null_for_absent_optional_fields() {
        let delivery = prepared(
            "https://hooks.public-site.dev/receive",
            r#"["field-email","field-name"]"#,
            r#"{"email":"person@example.test","project_details":"private","name":null,"extra":"omit"}"#,
        )
        .expect("mapping matches the exact source revision");
        assert_eq!(
            std::str::from_utf8(delivery.body()).expect("body is utf8"),
            r#"{"delivery_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","event":"submission.received","submission":{"fields":{"field-email":"person@example.test","field-name":null},"form_id":"form-service-inquiry","funnel_id":"funnel-public","page_id":"page-home","revision_id":"revision-accepted","submitted_at":1800000000},"version":1}"#
        );
    }

    #[test]
    fn incompatible_and_duplicate_field_mappings_fail_closed() {
        assert_eq!(
            prepared(
                "https://hooks.public-site.dev/receive",
                r#"["field-email","field-missing"]"#,
                r#"{"email":"person@example.test"}"#,
            )
            .err(),
            Some(WebhookPreparationError::IncompatibleMapping)
        );
        assert_eq!(
            prepared(
                "https://hooks.public-site.dev/receive",
                r#"["field-email","field-email"]"#,
                r#"{"email":"person@example.test"}"#,
            )
            .err(),
            Some(WebhookPreparationError::IncompatibleMapping)
        );
        assert_eq!(
            prepared("https://hooks.public-site.dev/receive", "[]", "{} ").err(),
            Some(WebhookPreparationError::IncompatibleMapping)
        );
    }

    #[test]
    fn signature_authenticates_timestamp_id_and_exact_canonical_body() {
        let key = WebhookSigningKey::parse(SIGNING_SECRET).expect("test key is long enough");
        let delivery = prepared(
            "https://hooks.public-site.dev/receive",
            r#"["field-email"]"#,
            r#"{"email":"person@example.test"}"#,
        )
        .expect("valid delivery");
        assert_eq!(
            delivery.signature(),
            "59cf7b1412836f403e2120ef59f25ba8dca63da1e4883b9258ee1ec14bd19927"
        );
        assert!(verify_webhook_signature(
            &key,
            delivery.timestamp(),
            delivery.delivery_id(),
            delivery.body(),
            &format!("v1={}", delivery.signature()),
        ));
        let repeated = prepared(
            "https://hooks.public-site.dev/receive",
            r#"["field-email"]"#,
            r#"{"email":"person@example.test"}"#,
        )
        .expect("same source can be retried")
        .body()
        .to_vec();
        assert_eq!(repeated, delivery.body());
        assert!(!verify_webhook_signature(
            &key,
            delivery.timestamp() + 1,
            delivery.delivery_id(),
            delivery.body(),
            &format!("v1={}", delivery.signature()),
        ));
        let mut changed_body = delivery.body().to_vec();
        changed_body.push(b' ');
        assert!(!verify_webhook_signature(
            &key,
            delivery.timestamp(),
            delivery.delivery_id(),
            &changed_body,
            &format!("v1={}", delivery.signature()),
        ));
        assert!(!verify_webhook_signature(
            &key,
            delivery.timestamp(),
            "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
            delivery.body(),
            &format!("v1={}", delivery.signature()),
        ));
    }

    #[test]
    fn response_and_retry_policies_have_explicit_bounded_outcomes() {
        assert_eq!(
            classify_webhook_status(200),
            WebhookHttpOutcome::Accepted { status: 200 }
        );
        assert_eq!(
            classify_webhook_status(302),
            WebhookHttpOutcome::Terminal {
                status: 302,
                reason: WebhookTerminalReason::RedirectRefused,
            }
        );
        assert_eq!(
            classify_webhook_status(429),
            WebhookHttpOutcome::Retryable { status: 429 }
        );
        assert_eq!(
            classify_webhook_status(503),
            WebhookHttpOutcome::Retryable { status: 503 }
        );
        assert_eq!(
            classify_webhook_status(422),
            WebhookHttpOutcome::Terminal {
                status: 422,
                reason: WebhookTerminalReason::ReceiverRejected,
            }
        );
        assert_eq!(retry_delay_seconds(1), Some(60));
        assert_eq!(retry_delay_seconds(2), Some(300));
        assert_eq!(retry_delay_seconds(3), Some(1_800));
        assert_eq!(retry_delay_seconds(4), Some(7_200));
        assert_eq!(retry_delay_seconds(5), None);
    }

    #[test]
    fn external_destination_mapping_and_secret_parsers_survive_deterministic_fuzz_inputs() {
        let seeds = [
            "https://hooks.public-site.dev/receive",
            "http://localhost:8787",
            "[\"field-email\"]",
            "[]",
            "not-json",
            "short signing secret",
            SIGNING_SECRET,
        ];
        let mutations = deterministic_fuzz_inputs(&seeds);
        assert_eq!(mutations.len(), 10_000);
        for input in mutations {
            let _ = ValidatedWebhookDestination::parse(&input);
            let _ = SelectedWebhookFields::parse(&input);
            let _ = WebhookSigningKey::parse(&input);
        }
    }

    #[test]
    fn document_mapping_and_submission_value_parsers_survive_mutated_snapshots() {
        let document = document();
        let document_json = std::str::from_utf8(document.canonical_bytes())
            .expect("starter document is utf8")
            .to_owned();
        let parse_snapshot = |document_json: &str, mapping_json: &str, values_json: &str| {
            let destination =
                ValidatedWebhookDestination::parse("https://hooks.public-site.dev/receive")
                    .expect("test destination is valid");
            let _ = prepare_webhook_payload(
                destination,
                WebhookSubmissionSnapshot {
                    delivery_id: DELIVERY_ID,
                    funnel_id: "funnel-public",
                    revision_id: "revision-accepted",
                    page_id: "page-home",
                    form_id: "form-service-inquiry",
                    submitted_at: 1_800_000_000,
                    document_json,
                    selected_fields_json: mapping_json,
                    field_values_json: values_json,
                },
            );
        };
        for (index, input) in deterministic_fuzz_inputs(&[&document_json])
            .into_iter()
            .step_by(10)
            .enumerate()
        {
            if index % 3 == 0 {
                parse_snapshot(
                    &input,
                    r#"["field-email"]"#,
                    r#"{"email":"ok@example.test"}"#,
                );
            } else if index % 3 == 1 {
                parse_snapshot(&document_json, &input, r#"{"email":"ok@example.test"}"#);
            } else {
                parse_snapshot(&document_json, r#"["field-email"]"#, &input);
            }
        }
    }

    fn deterministic_fuzz_inputs(seeds: &[&str]) -> Vec<String> {
        let mut inputs = Vec::with_capacity(10_000);
        let alphabet = [
            ' ', '\0', '\n', '\r', '\\', '"', '\'', '<', '>', '[', ']', '{', '}', '/', ':', '@',
            '?', '#', '%', 'a', '0', 'é', '🦀',
        ];
        for index in 0..10_000 {
            let seed = seeds[index % seeds.len()];
            let mut value = seed.to_owned();
            let character = alphabet[(index / seeds.len()) % alphabet.len()];
            match (index / (seeds.len() * alphabet.len())) % 3 {
                0 if !value.is_empty() => {
                    let position = (index / alphabet.len()) % value.len();
                    if value.is_char_boundary(position) {
                        value.insert(position, character);
                    } else {
                        value.push(character);
                    }
                }
                1 => value.push(character),
                _ if !value.is_empty() => {
                    let position = (index / alphabet.len()) % value.len();
                    if value.is_char_boundary(position) {
                        value.remove(position);
                    }
                }
                _ => {}
            }
            inputs.push(value);
        }
        inputs
    }
}
