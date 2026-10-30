use std::fmt::{Display, Formatter};

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize};

pub const SERVICE_BRIEF_FORMAT_VERSION: u8 = 1;
pub const SERVICE_BRIEF_FIELD_MAX_BYTES: usize = 4_096;
pub const SERVICE_BRIEF_TOTAL_MAX_BYTES: usize = 16_384;
pub const SERVICE_BRIEF_MAX_SERIALIZED_BYTES: usize = 36_864;
pub const SERVICE_BRIEF_MAX_REQUEST_BYTES: usize = SERVICE_BRIEF_MAX_SERIALIZED_BYTES + 4_096;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceIntention {
    Contact,
    QuoteRequest,
    DiagnosticApplication,
}

impl ServiceIntention {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Contact => "contact",
            Self::QuoteRequest => "quote_request",
            Self::DiagnosticApplication => "diagnostic_application",
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ServiceBrief {
    intention: Option<ServiceIntention>,
    audience: Option<String>,
    need: Option<String>,
    offer: Option<String>,
    scope: Option<String>,
    terms: Option<String>,
    action: Option<String>,
    promised_next_step: Option<String>,
    available_evidence: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceBriefDraft {
    pub intention: Option<ServiceIntention>,
    pub audience: Option<String>,
    pub need: Option<String>,
    pub offer: Option<String>,
    pub scope: Option<String>,
    pub terms: Option<String>,
    pub action: Option<String>,
    pub promised_next_step: Option<String>,
    pub available_evidence: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnvalidatedServiceBrief {
    intention: Option<ServiceIntention>,
    audience: Option<String>,
    need: Option<String>,
    offer: Option<String>,
    scope: Option<String>,
    terms: Option<String>,
    action: Option<String>,
    promised_next_step: Option<String>,
    available_evidence: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BriefVersion(i32);

impl BriefVersion {
    pub const fn initial() -> Self {
        Self(0)
    }

    pub const fn value(self) -> i32 {
        self.0
    }

    pub fn next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

impl TryFrom<i32> for BriefVersion {
    type Error = BriefVersionError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        if value < 0 {
            Err(BriefVersionError)
        } else {
            Ok(Self(value))
        }
    }
}

impl Serialize for BriefVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_i32(self.0)
    }
}

impl<'de> Deserialize<'de> for BriefVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = i32::deserialize(deserializer)?;
        Self::try_from(value).map_err(D::Error::custom)
    }
}

impl ServiceBrief {
    pub fn from_draft(draft: ServiceBriefDraft) -> Result<Self, ServiceBriefValidationError> {
        let mut total_bytes = 0;
        let audience = normalize_text(draft.audience, "audience", &mut total_bytes)?;
        let need = normalize_text(draft.need, "need", &mut total_bytes)?;
        let offer = normalize_text(draft.offer, "offer", &mut total_bytes)?;
        let scope = normalize_text(draft.scope, "scope", &mut total_bytes)?;
        let terms = normalize_text(draft.terms, "terms", &mut total_bytes)?;
        let action = normalize_text(draft.action, "action", &mut total_bytes)?;
        let promised_next_step = normalize_text(
            draft.promised_next_step,
            "promised_next_step",
            &mut total_bytes,
        )?;
        let available_evidence = normalize_text(
            draft.available_evidence,
            "available_evidence",
            &mut total_bytes,
        )?;
        if total_bytes > SERVICE_BRIEF_TOTAL_MAX_BYTES {
            return Err(ServiceBriefValidationError::TotalTooLong);
        }

        Ok(Self {
            intention: draft.intention,
            audience,
            need,
            offer,
            scope,
            terms,
            action,
            promised_next_step,
            available_evidence,
        })
    }

    pub fn draft(&self) -> ServiceBriefDraft {
        ServiceBriefDraft {
            intention: self.intention,
            audience: self.audience.clone(),
            need: self.need.clone(),
            offer: self.offer.clone(),
            scope: self.scope.clone(),
            terms: self.terms.clone(),
            action: self.action.clone(),
            promised_next_step: self.promised_next_step.clone(),
            available_evidence: self.available_evidence.clone(),
        }
    }

    pub const fn intention(&self) -> Option<ServiceIntention> {
        self.intention
    }

    pub fn audience(&self) -> Option<&str> {
        self.audience.as_deref()
    }

    pub fn need(&self) -> Option<&str> {
        self.need.as_deref()
    }

    pub fn offer(&self) -> Option<&str> {
        self.offer.as_deref()
    }

    pub fn scope(&self) -> Option<&str> {
        self.scope.as_deref()
    }

    pub fn terms(&self) -> Option<&str> {
        self.terms.as_deref()
    }

    pub fn action(&self) -> Option<&str> {
        self.action.as_deref()
    }

    pub fn promised_next_step(&self) -> Option<&str> {
        self.promised_next_step.as_deref()
    }

    pub fn available_evidence(&self) -> Option<&str> {
        self.available_evidence.as_deref()
    }
}

impl<'de> Deserialize<'de> for ServiceBrief {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = UnvalidatedServiceBrief::deserialize(deserializer)?;
        let draft = ServiceBriefDraft {
            intention: raw.intention,
            audience: raw.audience,
            need: raw.need,
            offer: raw.offer,
            scope: raw.scope,
            terms: raw.terms,
            action: raw.action,
            promised_next_step: raw.promised_next_step,
            available_evidence: raw.available_evidence,
        };
        Self::from_draft(draft).map_err(D::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BriefVersionError;

impl Display for BriefVersionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("service brief version must be nonnegative")
    }
}

impl std::error::Error for BriefVersionError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceBriefValidationError {
    FieldTooLong(&'static str),
    InvalidControlCharacter(&'static str),
    TotalTooLong,
}

impl Display for ServiceBriefValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FieldTooLong(field) => {
                write!(formatter, "service brief field exceeds limit: {field}")
            }
            Self::InvalidControlCharacter(field) => {
                write!(
                    formatter,
                    "service brief field contains an invalid control character: {field}"
                )
            }
            Self::TotalTooLong => formatter.write_str("service brief exceeds its total size limit"),
        }
    }
}

impl std::error::Error for ServiceBriefValidationError {}

fn normalize_text(
    value: Option<String>,
    field: &'static str,
    total_bytes: &mut usize,
) -> Result<Option<String>, ServiceBriefValidationError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return Ok(None);
    }
    if normalized
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
    {
        return Err(ServiceBriefValidationError::InvalidControlCharacter(field));
    }
    let byte_length = normalized.len();
    if byte_length > SERVICE_BRIEF_FIELD_MAX_BYTES {
        return Err(ServiceBriefValidationError::FieldTooLong(field));
    }
    *total_bytes = total_bytes.saturating_add(byte_length);
    Ok(Some(normalized.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{
        BriefVersion, ServiceBrief, ServiceBriefDraft, ServiceBriefValidationError,
        ServiceIntention, SERVICE_BRIEF_FIELD_MAX_BYTES, SERVICE_BRIEF_FORMAT_VERSION,
        SERVICE_BRIEF_MAX_REQUEST_BYTES, SERVICE_BRIEF_MAX_SERIALIZED_BYTES,
    };

    #[test]
    fn each_service_intention_has_a_stable_closed_json_value() {
        assert_eq!(
            serde_json::to_string(&ServiceIntention::Contact).expect("contact JSON"),
            "\"contact\""
        );
        assert_eq!(
            serde_json::to_string(&ServiceIntention::QuoteRequest).expect("quote JSON"),
            "\"quote_request\""
        );
        assert_eq!(
            serde_json::to_string(&ServiceIntention::DiagnosticApplication)
                .expect("diagnostic JSON"),
            "\"diagnostic_application\""
        );
        assert!(serde_json::from_str::<ServiceIntention>("\"booking\"").is_err());
    }

    #[test]
    fn incomplete_brief_normalizes_whitespace_and_line_endings_without_inventing_an_intention() {
        let brief = ServiceBrief::from_draft(ServiceBriefDraft {
            audience: Some("  Owners\r\nof small studios  ".to_owned()),
            need: Some(" \t ".to_owned()),
            ..ServiceBriefDraft::default()
        })
        .expect("incomplete brief is valid");

        assert_eq!(brief.intention(), None);
        assert_eq!(brief.audience(), Some("Owners\nof small studios"));
        assert_eq!(brief.need(), None);
        assert_eq!(
            brief.draft(),
            ServiceBriefDraft {
                audience: Some("Owners\nof small studios".to_owned()),
                ..ServiceBriefDraft::default()
            }
        );
        assert_eq!(
            serde_json::from_str::<ServiceBrief>(
                &serde_json::to_string(&brief).expect("serialize brief")
            )
            .expect("validated brief round trip"),
            brief
        );
    }

    #[test]
    fn service_brief_fields_obey_utf8_byte_and_total_size_limits() {
        let field_at_limit = "x".repeat(SERVICE_BRIEF_FIELD_MAX_BYTES);
        let accepted = ServiceBrief::from_draft(ServiceBriefDraft {
            audience: Some(field_at_limit),
            ..ServiceBriefDraft::default()
        });
        assert!(accepted.is_ok());

        let rejected_field = ServiceBrief::from_draft(ServiceBriefDraft {
            audience: Some("x".repeat(SERVICE_BRIEF_FIELD_MAX_BYTES + 1)),
            ..ServiceBriefDraft::default()
        });
        assert_eq!(
            rejected_field,
            Err(ServiceBriefValidationError::FieldTooLong("audience"))
        );

        let rejected_unicode = ServiceBrief::from_draft(ServiceBriefDraft {
            audience: Some("é".repeat(SERVICE_BRIEF_FIELD_MAX_BYTES / 2 + 1)),
            ..ServiceBriefDraft::default()
        });
        assert_eq!(
            rejected_unicode,
            Err(ServiceBriefValidationError::FieldTooLong("audience"))
        );

        let rejected_total = ServiceBrief::from_draft(ServiceBriefDraft {
            audience: Some("a".repeat(SERVICE_BRIEF_FIELD_MAX_BYTES)),
            need: Some("b".repeat(SERVICE_BRIEF_FIELD_MAX_BYTES)),
            offer: Some("c".repeat(SERVICE_BRIEF_FIELD_MAX_BYTES)),
            scope: Some("d".repeat(SERVICE_BRIEF_FIELD_MAX_BYTES)),
            action: Some("e".to_owned()),
            ..ServiceBriefDraft::default()
        });
        assert_eq!(
            rejected_total,
            Err(ServiceBriefValidationError::TotalTooLong)
        );
    }

    #[test]
    fn maximum_escaped_text_stays_within_storage_and_request_limits() {
        let escaped = "\"".repeat(SERVICE_BRIEF_FIELD_MAX_BYTES);
        let brief = ServiceBrief::from_draft(ServiceBriefDraft {
            audience: Some(escaped.clone()),
            need: Some(escaped.clone()),
            offer: Some(escaped.clone()),
            scope: Some(escaped),
            ..ServiceBriefDraft::default()
        })
        .expect("maximum raw text is valid");
        let serialized = serde_json::to_vec(&brief).expect("serialize brief");
        let request = serde_json::to_vec(&serde_json::json!({
            "expected_version": 0,
            "format_version": SERVICE_BRIEF_FORMAT_VERSION,
            "brief": brief,
        }))
        .expect("serialize request");

        assert!(serialized.len() > 20_480);
        assert!(serialized.len() <= SERVICE_BRIEF_MAX_SERIALIZED_BYTES);
        assert!(request.len() <= SERVICE_BRIEF_MAX_REQUEST_BYTES);
    }

    #[test]
    fn service_brief_rejects_non_whitespace_control_characters_without_echoing_values() {
        let error = ServiceBrief::from_draft(ServiceBriefDraft {
            offer: Some("private\u{0007}note".to_owned()),
            ..ServiceBriefDraft::default()
        })
        .expect_err("bell control character is rejected");

        assert_eq!(
            error,
            ServiceBriefValidationError::InvalidControlCharacter("offer")
        );
        assert!(!error.to_string().contains("private"));
    }

    #[test]
    fn brief_versions_are_nonnegative_and_stop_at_the_database_integer_limit() {
        assert!(BriefVersion::try_from(-1).is_err());
        assert_eq!(BriefVersion::initial().value(), 0);
        assert_eq!(
            BriefVersion::initial().next(),
            Some(BriefVersion::try_from(1).unwrap())
        );
        assert_eq!(BriefVersion::try_from(i32::MAX).unwrap().next(), None);
    }
}
