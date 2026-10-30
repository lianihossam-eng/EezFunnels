use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

use eez_domain::document_v2::MAX_PERSISTED_DOCUMENT_BYTES;
use eez_domain::document_v2::{BlockV2, DocumentV2, FieldId, FormField, FormId};
use eez_domain::webhook_delivery::{SelectedWebhookFields, ValidatedWebhookDestination};
use eez_storage::delivery_controls::{
    DeliveryControlError, DeliveryControlRepository, SaveWebhookFormMapping,
    WebhookDestinationRecord, WebhookFormMappingRecord,
};
use eez_storage::repository::{Repository, RepositoryError};
use serde::{Deserialize, Serialize};

const MAX_SELECTED_FIELDS: usize = 50;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookDestinationSnapshot {
    pub enabled: bool,
    pub destination_version_id: Option<String>,
    pub destination_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookFormFieldOption {
    pub field_id: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookFormDeliveryOption {
    pub form_id: String,
    pub purpose: String,
    pub fields: Vec<WebhookFormFieldOption>,
    pub enabled: bool,
    pub expected_destination_version_id: Option<String>,
    pub mapping_version_id: Option<String>,
    pub selected_field_ids: Vec<String>,
    pub mapping_compatible: bool,
    pub needs_revalidation: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FunnelWebhookDeliverySnapshot {
    pub funnel_id: String,
    pub published_revision_id: Option<String>,
    pub destination_version_id: Option<String>,
    pub forms: Vec<WebhookFormDeliveryOption>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SaveWebhookFormDelivery {
    pub form_id: String,
    pub expected_published_revision_id: String,
    pub destination_version_id: String,
    pub expected_mapping_version_id: Option<String>,
    pub enabled: bool,
    pub selected_field_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicDeliveryControlsError {
    InvalidInput,
    NotFound,
    SubmissionNotFound,
    Conflict,
    StorageUnavailable,
}

impl PublicDeliveryControlsError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "validation/invalid_json",
            Self::NotFound => "not_found/funnel",
            Self::SubmissionNotFound => "not_found/submission",
            Self::Conflict => "conflict/webhook_delivery_stale",
            Self::StorageUnavailable => "external/unavailable",
        }
    }
}

impl Display for PublicDeliveryControlsError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for PublicDeliveryControlsError {}

impl From<RepositoryError> for PublicDeliveryControlsError {
    fn from(error: RepositoryError) -> Self {
        match error {
            RepositoryError::FunnelNotFound | RepositoryError::RevisionNotFound => Self::NotFound,
            RepositoryError::InvalidField(_) | RepositoryError::InvalidJson => Self::InvalidInput,
            _ => Self::StorageUnavailable,
        }
    }
}

impl From<DeliveryControlError> for PublicDeliveryControlsError {
    fn from(error: DeliveryControlError) -> Self {
        match error {
            DeliveryControlError::NotFound => Self::NotFound,
            DeliveryControlError::Conflict => Self::Conflict,
            DeliveryControlError::InvalidInput => Self::InvalidInput,
            DeliveryControlError::Database | DeliveryControlError::InvalidRecord => {
                Self::StorageUnavailable
            }
        }
    }
}

pub async fn read_destination(
    delivery: &DeliveryControlRepository<'_>,
) -> Result<WebhookDestinationSnapshot, PublicDeliveryControlsError> {
    Ok(destination_snapshot(delivery.destination().await?))
}

pub async fn save_destination(
    delivery: &DeliveryControlRepository<'_>,
    idempotency_key: &str,
    expected_version_id: Option<&str>,
    expected_enabled: bool,
    destination_url: &str,
    enabled: bool,
) -> Result<WebhookDestinationSnapshot, PublicDeliveryControlsError> {
    validate_uuid(idempotency_key)?;
    if let Some(expected_version_id) = expected_version_id {
        validate_uuid(expected_version_id)?;
    }
    let destination = ValidatedWebhookDestination::parse(destination_url)
        .map_err(|_| PublicDeliveryControlsError::InvalidInput)?;
    let record = delivery
        .save_destination(
            idempotency_key,
            expected_version_id,
            expected_enabled,
            destination.as_str(),
            enabled,
        )
        .await?;
    Ok(destination_snapshot(record))
}

pub async fn read_funnel_delivery(
    repository: &Repository<'_>,
    delivery: &DeliveryControlRepository<'_>,
    funnel_id: &str,
) -> Result<FunnelWebhookDeliverySnapshot, PublicDeliveryControlsError> {
    validate_route_id(funnel_id)?;
    let funnel = repository
        .get_funnel(funnel_id)
        .await?
        .ok_or(PublicDeliveryControlsError::NotFound)?;
    let destination = delivery.destination().await?;
    let settings = delivery.form_mappings(funnel_id).await?;
    let Some(published_revision_id) = funnel.published_revision_id else {
        return Ok(FunnelWebhookDeliverySnapshot {
            funnel_id: funnel.id,
            published_revision_id: None,
            destination_version_id: destination.destination_version_id,
            forms: Vec::new(),
        });
    };
    let revision = repository
        .get_owned_revision(funnel_id, &published_revision_id)
        .await?
        .ok_or(PublicDeliveryControlsError::NotFound)?;
    if revision.schema_version != 2 {
        return Err(PublicDeliveryControlsError::InvalidInput);
    }
    let document = DocumentV2::decode(revision.data_json.as_bytes(), funnel_id)
        .map_err(|_| PublicDeliveryControlsError::StorageUnavailable)?;
    let settings = settings
        .into_iter()
        .map(|setting| (setting.form_id.clone(), setting))
        .collect::<BTreeMap<_, _>>();
    let placed_form_ids = document
        .pages()
        .iter()
        .flat_map(|page| page.sections())
        .flat_map(|section| section.blocks())
        .filter_map(|block| match block {
            BlockV2::Form { form_id, .. } => Some(form_id.as_str().to_owned()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let mut forms = Vec::new();
    for form in document
        .forms()
        .iter()
        .filter(|form| placed_form_ids.contains(form.id().as_str()))
    {
        let setting = settings.get(form.id().as_str());
        let (enabled, expected_destination_version_id, mapping_version_id, selected_field_ids) =
            match setting {
                Some(setting) => (
                    setting.enabled,
                    setting.expected_destination_version_id.clone(),
                    setting.mapping_version_id.clone(),
                    selected_field_ids(setting)?,
                ),
                None => (false, None, None, Vec::new()),
            };
        let mapping_compatible =
            match setting.and_then(|setting| setting.selected_fields_json.as_deref()) {
                Some(selected_fields_json) => selected_mapping_compatible(
                    selected_fields_json,
                    setting.and_then(|setting| setting.selected_field_contract_json.as_deref()),
                    form.fields(),
                ),
                None => false,
            };
        let has_mapping = setting
            .and_then(|setting| setting.mapping_version_id.as_ref())
            .is_some();
        forms.push(WebhookFormDeliveryOption {
            form_id: form.id().as_str().to_owned(),
            purpose: form.purpose().to_owned(),
            fields: form
                .fields()
                .iter()
                .map(|field| WebhookFormFieldOption {
                    field_id: field.id().as_str().to_owned(),
                    label: field.label().to_owned(),
                })
                .collect(),
            enabled,
            expected_destination_version_id: expected_destination_version_id.clone(),
            mapping_version_id,
            selected_field_ids,
            mapping_compatible,
            needs_revalidation: has_mapping
                && (expected_destination_version_id != destination.destination_version_id
                    || !mapping_compatible),
        });
    }
    Ok(FunnelWebhookDeliverySnapshot {
        funnel_id: funnel.id,
        published_revision_id: Some(published_revision_id),
        destination_version_id: destination.destination_version_id,
        forms,
    })
}

pub async fn save_form_delivery(
    repository: &Repository<'_>,
    delivery: &DeliveryControlRepository<'_>,
    funnel_id: &str,
    idempotency_key: &str,
    input: SaveWebhookFormDelivery,
) -> Result<FunnelWebhookDeliverySnapshot, PublicDeliveryControlsError> {
    validate_route_id(funnel_id)?;
    validate_uuid(idempotency_key)?;
    validate_uuid(&input.destination_version_id)?;
    if let Some(expected_mapping_version_id) = input.expected_mapping_version_id.as_deref() {
        validate_uuid(expected_mapping_version_id)?;
    }
    let funnel = repository
        .get_funnel(funnel_id)
        .await?
        .ok_or(PublicDeliveryControlsError::NotFound)?;
    if funnel.published_revision_id.as_deref()
        != Some(input.expected_published_revision_id.as_str())
    {
        return Err(PublicDeliveryControlsError::Conflict);
    }
    let published_revision_id = funnel
        .published_revision_id
        .as_deref()
        .ok_or(PublicDeliveryControlsError::Conflict)?;
    let revision = repository
        .get_owned_revision(funnel_id, published_revision_id)
        .await?
        .ok_or(PublicDeliveryControlsError::NotFound)?;
    if revision.schema_version != 2 {
        return Err(PublicDeliveryControlsError::InvalidInput);
    }
    let document = DocumentV2::decode(revision.data_json.as_bytes(), funnel_id)
        .map_err(|_| PublicDeliveryControlsError::StorageUnavailable)?;
    let form_id = FormId::parse(input.form_id.clone())
        .map_err(|_| PublicDeliveryControlsError::InvalidInput)?;
    let form = document
        .form(&form_id)
        .ok_or(PublicDeliveryControlsError::NotFound)?;
    let placed = document.pages().iter().any(|page| {
        page.sections().iter().any(|section| {
            section.blocks().iter().any(
                |block| matches!(block, BlockV2::Form { form_id: placed_form, .. } if placed_form == &form_id),
            )
        })
    });
    if !placed {
        return Err(PublicDeliveryControlsError::NotFound);
    }
    let selected_field_ids = canonical_selected_field_ids(&input.selected_field_ids)?;
    let selected_fields_json = serde_json::to_string(&selected_field_ids)
        .map_err(|_| PublicDeliveryControlsError::InvalidInput)?;
    let selected_fields = SelectedWebhookFields::parse(&selected_fields_json)
        .map_err(|_| PublicDeliveryControlsError::InvalidInput)?;
    if selected_fields
        .field_ids()
        .iter()
        .any(|selected| !form.fields().iter().any(|field| field.id() == selected))
    {
        return Err(PublicDeliveryControlsError::Conflict);
    }
    let selected_field_contract_json =
        selected_field_contract_json(form.fields(), selected_fields.field_ids())?;
    if selected_field_contract_json.len() > MAX_PERSISTED_DOCUMENT_BYTES {
        return Err(PublicDeliveryControlsError::InvalidInput);
    }
    delivery
        .save_form_mapping(SaveWebhookFormMapping {
            idempotency_key,
            funnel_id,
            form_id: form.id().as_str(),
            expected_published_revision_id: &input.expected_published_revision_id,
            destination_version_id: &input.destination_version_id,
            expected_mapping_version_id: input.expected_mapping_version_id.as_deref(),
            selected_fields_json: &selected_fields_json,
            selected_field_contract_json: &selected_field_contract_json,
            enabled: input.enabled,
        })
        .await?;
    read_funnel_delivery(repository, delivery, funnel_id).await
}

pub async fn retry_delivery(
    delivery: &DeliveryControlRepository<'_>,
    submission_id: &str,
    idempotency_key: &str,
) -> Result<(), PublicDeliveryControlsError> {
    validate_uuid(submission_id)?;
    validate_uuid(idempotency_key)?;
    delivery
        .retry_delivery(submission_id, idempotency_key)
        .await
        .map_err(|error| match error {
            DeliveryControlError::NotFound => PublicDeliveryControlsError::SubmissionNotFound,
            other => other.into(),
        })?;
    Ok(())
}

pub async fn cancel_delivery(
    delivery: &DeliveryControlRepository<'_>,
    submission_id: &str,
    idempotency_key: &str,
) -> Result<(), PublicDeliveryControlsError> {
    validate_uuid(submission_id)?;
    validate_uuid(idempotency_key)?;
    delivery
        .cancel_delivery(submission_id, idempotency_key)
        .await
        .map_err(|error| match error {
            DeliveryControlError::NotFound => PublicDeliveryControlsError::SubmissionNotFound,
            other => other.into(),
        })?;
    Ok(())
}

fn destination_snapshot(record: WebhookDestinationRecord) -> WebhookDestinationSnapshot {
    WebhookDestinationSnapshot {
        enabled: record.enabled,
        destination_version_id: record.destination_version_id,
        destination_url: record.destination_url,
    }
}

fn selected_field_ids(
    setting: &WebhookFormMappingRecord,
) -> Result<Vec<String>, PublicDeliveryControlsError> {
    match setting.selected_fields_json.as_deref() {
        Some(value) => {
            serde_json::from_str(value).map_err(|_| PublicDeliveryControlsError::StorageUnavailable)
        }
        None => Ok(Vec::new()),
    }
}

fn selected_mapping_compatible(
    selected_fields_json: &str,
    selected_field_contract_json: Option<&str>,
    fields: &[eez_domain::document_v2::FormField],
) -> bool {
    let (Ok(selected), Some(contract_json)) = (
        SelectedWebhookFields::parse(selected_fields_json),
        selected_field_contract_json,
    ) else {
        return false;
    };
    let Ok(contract_fields) = serde_json::from_str::<Vec<FormField>>(contract_json) else {
        return false;
    };
    selected.field_ids().len() == contract_fields.len()
        && selected.field_ids().iter().all(|selected_id| {
            let Some(saved_field) = contract_fields
                .iter()
                .find(|field| field.id() == selected_id)
            else {
                return false;
            };
            fields
                .iter()
                .find(|field| field.id() == selected_id)
                .is_some_and(|current_field| current_field == saved_field)
        })
}

fn selected_field_contract_json(
    fields: &[FormField],
    selected_field_ids: &[FieldId],
) -> Result<String, PublicDeliveryControlsError> {
    let selected_field_ids = selected_field_ids.iter().collect::<BTreeSet<_>>();
    let mut selected_fields = fields
        .iter()
        .filter(|field| selected_field_ids.contains(&field.id()))
        .collect::<Vec<_>>();
    if selected_fields.len() != selected_field_ids.len() {
        return Err(PublicDeliveryControlsError::Conflict);
    }
    selected_fields.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));
    let mut canonical = Vec::new();
    canonical.push(b'[');
    for (index, field) in selected_fields.iter().enumerate() {
        if index > 0 {
            canonical.push(b',');
        }
        canonical.extend(
            field
                .canonical_bytes()
                .map_err(|_| PublicDeliveryControlsError::InvalidInput)?,
        );
    }
    canonical.push(b']');
    String::from_utf8(canonical).map_err(|_| PublicDeliveryControlsError::InvalidInput)
}

fn canonical_selected_field_ids(
    field_ids: &[String],
) -> Result<Vec<String>, PublicDeliveryControlsError> {
    if field_ids.is_empty() || field_ids.len() > MAX_SELECTED_FIELDS {
        return Err(PublicDeliveryControlsError::InvalidInput);
    }
    let mut unique = BTreeSet::new();
    for field_id in field_ids {
        let field_id = FieldId::parse(field_id.clone())
            .map_err(|_| PublicDeliveryControlsError::InvalidInput)?;
        unique.insert(field_id.as_str().to_owned());
    }
    if unique.len() != field_ids.len() {
        return Err(PublicDeliveryControlsError::InvalidInput);
    }
    Ok(unique.into_iter().collect())
}

fn validate_route_id(value: &str) -> Result<(), PublicDeliveryControlsError> {
    if (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        Ok(())
    } else {
        Err(PublicDeliveryControlsError::InvalidInput)
    }
}

fn validate_uuid(value: &str) -> Result<(), PublicDeliveryControlsError> {
    uuid::Uuid::parse_str(value)
        .map(|id| id.to_string() == value)
        .ok()
        .filter(|canonical| *canonical)
        .map(|_| ())
        .ok_or(PublicDeliveryControlsError::InvalidInput)
}

#[cfg(test)]
mod tests {
    use super::{
        canonical_selected_field_ids, selected_field_contract_json, selected_mapping_compatible,
        MAX_PERSISTED_DOCUMENT_BYTES,
    };
    use eez_domain::document_v2::{BlueprintId, DocumentV2, FormField, TextPresentation};
    use eez_domain::webhook_delivery::ValidatedWebhookDestination;
    use eez_domain::ThemePreset;

    #[test]
    fn webhook_destinations_require_safe_public_https_urls() {
        assert_eq!(
            ValidatedWebhookDestination::parse("https://hooks.example.dev/events")
                .expect("public HTTPS endpoint")
                .as_str(),
            "https://hooks.example.dev/events"
        );
        for destination in [
            "http://hooks.example.dev/events",
            "https://localhost/events",
            "https://127.0.0.1/events",
            "https://receiver.test/events",
            "https://user:pass@hooks.example.dev/events",
            "https://hooks.example.dev/events#fragment",
            "https://hooks.example.dev:8443/events",
            "https://hooks.example.dev/with space",
        ] {
            assert!(
                ValidatedWebhookDestination::parse(destination).is_err(),
                "unsafe destination was accepted: {destination}"
            );
        }
    }

    #[test]
    fn form_mapping_sorts_field_ids_and_rejects_empty_duplicate_or_invalid_selections() {
        let document =
            DocumentV2::starter("funnel-pub-010", BlueprintId::LeadGen, ThemePreset::Minimal)
                .expect("lead generation starter");
        let field_ids = document.forms()[0]
            .fields()
            .iter()
            .map(|field| field.id().as_str().to_owned())
            .collect::<Vec<_>>();
        let reversed = field_ids.iter().rev().cloned().collect::<Vec<_>>();
        let canonical = canonical_selected_field_ids(&reversed).expect("unique form fields");
        assert_eq!(
            canonical,
            field_ids
                .iter()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
        );
        let contract_field_ids = document.forms()[0]
            .fields()
            .iter()
            .map(|field| field.id().clone())
            .collect::<Vec<_>>();
        let contract =
            selected_field_contract_json(document.forms()[0].fields(), &contract_field_ids)
                .expect("field contract");
        assert!(selected_mapping_compatible(
            &serde_json::to_string(&canonical).expect("JSON fields"),
            Some(&contract),
            document.forms()[0].fields(),
        ));
        assert!(canonical_selected_field_ids(&[]).is_err());
        assert!(
            canonical_selected_field_ids(&[field_ids[0].clone(), field_ids[0].clone()]).is_err()
        );
        assert!(canonical_selected_field_ids(&["bad field id".to_owned()]).is_err());
        assert!(!selected_mapping_compatible(
            r#"["removed-field"]"#,
            None,
            document.forms()[0].fields(),
        ));
    }

    #[test]
    fn changed_selected_field_with_the_same_id_requires_revalidation() {
        let document =
            DocumentV2::starter("funnel-pub-010", BlueprintId::LeadGen, ThemePreset::Minimal)
                .expect("lead generation starter");
        let original = document.forms()[0].fields()[0].clone();
        let selected_ids = vec![original.id().clone()];
        let selected_fields_json =
            serde_json::to_string(&vec![original.id().as_str()]).expect("selected field IDs");
        let contract = selected_field_contract_json(std::slice::from_ref(&original), &selected_ids)
            .expect("field contract");
        let changed_field = FormField::Text {
            id: original.id().clone(),
            name: "tax_identifier".to_owned(),
            label: "Tax identifier".to_owned(),
            help: None,
            required: true,
            validation: None,
            presentation: TextPresentation::SingleLine,
        };

        assert!(!selected_mapping_compatible(
            &selected_fields_json,
            Some(&contract),
            &[changed_field],
        ));
    }

    #[test]
    fn valid_large_select_fields_keep_a_canonical_revalidatable_contract() {
        let document =
            DocumentV2::starter("funnel-pub-010", BlueprintId::LeadGen, ThemePreset::Minimal)
                .expect("lead generation starter");
        let mut document_json: serde_json::Value =
            serde_json::from_slice(document.canonical_bytes()).expect("starter JSON");
        let options = (0..40)
            .map(|index| format!("{}-{index:02}", "x".repeat(600)))
            .collect::<Vec<_>>();
        document_json["forms"][0]["fields"][0] = serde_json::json!({
            "kind": "select",
            "id": "field-name",
            "name": "service_type",
            "label": "Service type",
            "required": true,
            "options": options,
        });
        let document_json = serde_json::to_vec(&document_json).expect("edited starter JSON");
        let document = DocumentV2::decode(&document_json, "funnel-pub-010")
            .expect("large select remains within valid document limits");
        let field = &document.forms()[0].fields()[0];
        let contract = selected_field_contract_json(
            std::slice::from_ref(field),
            std::slice::from_ref(field.id()),
        )
        .expect("selected field contract");
        let selected_ids =
            serde_json::to_string(&vec![field.id().as_str()]).expect("selected field IDs");

        assert!(contract.len() > 16_384);
        assert!(contract.len() <= MAX_PERSISTED_DOCUMENT_BYTES);
        assert!(selected_mapping_compatible(
            &selected_ids,
            Some(&contract),
            std::slice::from_ref(field),
        ));
        let contract_bytes = contract.as_bytes();
        let canonical_field = &contract_bytes[1..contract_bytes.len() - 1];
        assert!(document
            .canonical_bytes()
            .windows(canonical_field.len())
            .any(|window| window == canonical_field));
    }
}
