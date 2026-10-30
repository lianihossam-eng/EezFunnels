use serde::Deserialize;
use sha2::{Digest, Sha256};
use worker::d1::D1Type;
use worker::{D1Database, Error};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebhookDestinationRecord {
    pub enabled: bool,
    pub destination_version_id: Option<String>,
    pub destination_url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebhookFormMappingRecord {
    pub form_id: String,
    pub enabled: bool,
    pub expected_destination_version_id: Option<String>,
    pub mapping_version_id: Option<String>,
    pub selected_fields_json: Option<String>,
    pub selected_field_contract_json: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct SaveWebhookFormMapping<'a> {
    pub idempotency_key: &'a str,
    pub funnel_id: &'a str,
    pub form_id: &'a str,
    pub expected_published_revision_id: &'a str,
    pub destination_version_id: &'a str,
    pub expected_mapping_version_id: Option<&'a str>,
    pub selected_fields_json: &'a str,
    pub selected_field_contract_json: &'a str,
    pub enabled: bool,
}

fn form_mapping_payload_json(input: SaveWebhookFormMapping<'_>) -> String {
    serde_json::json!({
        "funnel_id": input.funnel_id,
        "form_id": input.form_id,
        "expected_published_revision_id": input.expected_published_revision_id,
        "destination_version_id": input.destination_version_id,
        "expected_mapping_version_id": input.expected_mapping_version_id,
        "selected_fields_json": input.selected_fields_json,
        "selected_field_contract_sha256": sha256_hex(
            input.selected_field_contract_json.as_bytes()
        ),
        "enabled": input.enabled,
    })
    .to_string()
}

fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryControlError {
    Database,
    NotFound,
    Conflict,
    InvalidRecord,
    InvalidInput,
}

impl DeliveryControlError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Database | Self::InvalidRecord => "external/unavailable",
            Self::InvalidInput => "validation/invalid_json",
            Self::NotFound => "not_found/submission",
            Self::Conflict => "conflict/webhook_delivery_stale",
        }
    }
}

#[derive(Deserialize)]
struct InstallationRow {
    enabled: i64,
    destination_version_id: Option<String>,
    destination_url: Option<String>,
}

#[derive(Deserialize)]
struct FormMappingRow {
    form_id: String,
    enabled: i64,
    expected_destination_version_id: Option<String>,
    mapping_version_id: Option<String>,
    selected_fields_json: Option<String>,
    selected_field_contract_json: Option<String>,
}

#[derive(Deserialize)]
struct MappingVersionRow {
    funnel_id: String,
    form_id: String,
    destination_version_id: String,
    selected_fields_json: String,
    selected_field_contract_json: Option<String>,
}

#[derive(Deserialize)]
struct TaskIdRow {
    task_id: String,
}

#[derive(Deserialize)]
struct ExistsRow {
    present: i64,
}

#[derive(Deserialize)]
struct IdempotencyReceiptRow {
    command_id: String,
    payload_json: String,
    result_json: String,
}

#[derive(Deserialize)]
struct DestinationMutationResult {
    destination_version_id: String,
}

#[derive(Deserialize)]
struct FormMappingMutationResult {
    mapping_version_id: String,
}

pub struct DeliveryControlRepository<'a> {
    database: &'a D1Database,
}

impl<'a> DeliveryControlRepository<'a> {
    pub fn new(database: &'a D1Database) -> Self {
        Self { database }
    }

    pub async fn destination(&self) -> Result<WebhookDestinationRecord, DeliveryControlError> {
        let row = self
            .database
            .prepare(
                "SELECT installation.enabled, installation.destination_version_id, destination.destination_url FROM webhook_installation_delivery AS installation LEFT JOIN webhook_destination_versions AS destination ON destination.destination_version_id = installation.destination_version_id WHERE installation.installation_id = 1",
            )
            .first::<InstallationRow>(None)
            .await
            .map_err(|_| DeliveryControlError::Database)?
            .ok_or(DeliveryControlError::InvalidRecord)?;
        decode_installation(row)
    }

    pub async fn save_destination(
        &self,
        version_id: &str,
        expected_version_id: Option<&str>,
        expected_enabled: bool,
        destination_url: &str,
        enabled: bool,
    ) -> Result<WebhookDestinationRecord, DeliveryControlError> {
        validate_uuid(version_id)?;
        let command_id = "internal.public_delivery.destination_save";
        let payload_json = serde_json::json!({
            "expected_version_id": expected_version_id,
            "expected_enabled": expected_enabled,
            "destination_url": destination_url,
            "enabled": enabled,
        })
        .to_string();
        if self.active_receipt(version_id).await?.is_some() {
            return self
                .confirm_destination_receipt(
                    version_id,
                    command_id,
                    &payload_json,
                    destination_url,
                    enabled,
                )
                .await;
        }
        let current = self.destination().await?;
        if current.destination_version_id.as_deref() != expected_version_id
            || current.enabled != expected_enabled
        {
            if self.active_receipt(version_id).await?.is_some() {
                return self
                    .confirm_destination_receipt(
                        version_id,
                        command_id,
                        &payload_json,
                        destination_url,
                        enabled,
                    )
                    .await;
            }
            return Err(DeliveryControlError::Conflict);
        }
        if self.destination_version_exists(version_id).await? {
            if self.active_receipt(version_id).await?.is_some() {
                return self
                    .confirm_destination_receipt(
                        version_id,
                        command_id,
                        &payload_json,
                        destination_url,
                        enabled,
                    )
                    .await;
            }
            return Err(DeliveryControlError::Conflict);
        }

        let current_destination_version_id = current.destination_version_id.as_deref();
        let result_destination_version_id =
            if current.destination_url.as_deref() == Some(destination_url) {
                current_destination_version_id
                    .ok_or(DeliveryControlError::InvalidRecord)?
                    .to_owned()
            } else {
                version_id.to_owned()
            };
        let result_json = serde_json::json!({
            "destination_version_id": result_destination_version_id,
        })
        .to_string();
        let idempotency_key_value = D1Type::Text(version_id);
        let command_id_value = D1Type::Text(command_id);
        let payload_json_value = D1Type::Text(&payload_json);
        let result_json_value = D1Type::Text(&result_json);
        let expected_version_id_value = expected_version_id
            .map(D1Type::Text)
            .unwrap_or(D1Type::Null);
        let expected_enabled_value = D1Type::Boolean(expected_enabled);

        let delete_expired_receipt = self
            .database
            .prepare(
                "DELETE FROM command_receipts WHERE idempotency_key = ? AND expires_at <= unixepoch()",
            )
            .bind_refs([&idempotency_key_value])?;

        if current.destination_url.as_deref() == Some(destination_url) && current.enabled == enabled
        {
            let insert_receipt = self
                .database
                .prepare(
                    "INSERT INTO command_receipts (idempotency_key, command_id, payload_json, result_json, expires_at) SELECT ?, ?, ?, ?, unixepoch() + 315360000 WHERE EXISTS (SELECT 1 FROM webhook_installation_delivery WHERE installation_id = 1 AND destination_version_id IS ? AND enabled = ?) ON CONFLICT(idempotency_key) DO NOTHING RETURNING 1 AS present",
                )
                .bind_refs([
                    &idempotency_key_value,
                    &command_id_value,
                    &payload_json_value,
                    &result_json_value,
                    &expected_version_id_value,
                    &expected_enabled_value,
                ])?;
            self.database
                .batch(vec![delete_expired_receipt, insert_receipt])
                .await
                .map_err(|_| DeliveryControlError::Database)?;
            return self
                .confirm_destination_receipt(
                    version_id,
                    command_id,
                    &payload_json,
                    destination_url,
                    enabled,
                )
                .await;
        }

        let enabled_value = D1Type::Boolean(enabled);
        let destination_url_value = D1Type::Text(destination_url);
        if current.destination_url.as_deref() == Some(destination_url) {
            let destination_version_id =
                current_destination_version_id.ok_or(DeliveryControlError::InvalidRecord)?;
            let destination_version_id_value = D1Type::Text(destination_version_id);
            let insert_receipt = self
                .database
                .prepare(
                    "INSERT INTO command_receipts (idempotency_key, command_id, payload_json, result_json, expires_at) SELECT ?, ?, ?, ?, unixepoch() + 315360000 WHERE EXISTS (SELECT 1 FROM webhook_installation_delivery WHERE installation_id = 1 AND destination_version_id IS ? AND enabled = ?) ON CONFLICT(idempotency_key) DO NOTHING",
                )
                .bind_refs([
                    &idempotency_key_value,
                    &command_id_value,
                    &payload_json_value,
                    &result_json_value,
                    &expected_version_id_value,
                    &expected_enabled_value,
                ])?;
            let update_installation = self
                .database
                .prepare(
                    "UPDATE webhook_installation_delivery SET enabled = ? WHERE installation_id = 1 AND destination_version_id = ? AND destination_version_id IS ? AND enabled = ? AND changes() = 1 AND EXISTS (SELECT 1 FROM command_receipts WHERE idempotency_key = ? AND command_id = ? AND payload_json = ? AND result_json = ? AND expires_at > unixepoch())",
                )
                .bind_refs([
                    &enabled_value,
                    &destination_version_id_value,
                    &expected_version_id_value,
                    &expected_enabled_value,
                    &idempotency_key_value,
                    &command_id_value,
                    &payload_json_value,
                    &result_json_value,
                ])?;
            self.database
                .batch(vec![
                    delete_expired_receipt,
                    insert_receipt,
                    update_installation,
                ])
                .await
                .map_err(|_| DeliveryControlError::Database)?;
            return self
                .confirm_destination_receipt(
                    version_id,
                    command_id,
                    &payload_json,
                    destination_url,
                    enabled,
                )
                .await;
        }

        let destination_version_id_value = D1Type::Text(version_id);
        let insert_version = self
            .database
            .prepare(
                "INSERT INTO webhook_destination_versions (destination_version_id, destination_url, created_at) SELECT ?, ?, unixepoch() WHERE EXISTS (SELECT 1 FROM webhook_installation_delivery WHERE installation_id = 1 AND destination_version_id IS ? AND enabled = ?) ON CONFLICT(destination_version_id) DO NOTHING",
            )
            .bind_refs([
                &destination_version_id_value,
                &destination_url_value,
                &expected_version_id_value,
                &expected_enabled_value,
            ])?;
        let insert_receipt = self
            .database
            .prepare(
                "INSERT INTO command_receipts (idempotency_key, command_id, payload_json, result_json, expires_at) SELECT ?, ?, ?, ?, unixepoch() + 315360000 WHERE changes() = 1 AND EXISTS (SELECT 1 FROM webhook_installation_delivery WHERE installation_id = 1 AND destination_version_id IS ? AND enabled = ?) AND EXISTS (SELECT 1 FROM webhook_destination_versions WHERE destination_version_id = ? AND destination_url = ?) ON CONFLICT(idempotency_key) DO NOTHING",
            )
            .bind_refs([
                &idempotency_key_value,
                &command_id_value,
                &payload_json_value,
                &result_json_value,
                &expected_version_id_value,
                &expected_enabled_value,
                &destination_version_id_value,
                &destination_url_value,
            ])?;
        let update_installation = self
            .database
            .prepare(
                "UPDATE webhook_installation_delivery SET enabled = ?, destination_version_id = ? WHERE installation_id = 1 AND destination_version_id IS ? AND enabled = ? AND changes() = 1 AND EXISTS (SELECT 1 FROM command_receipts WHERE idempotency_key = ? AND command_id = ? AND payload_json = ? AND result_json = ? AND expires_at > unixepoch())",
            )
            .bind_refs([
                &enabled_value,
                &destination_version_id_value,
                &expected_version_id_value,
                &expected_enabled_value,
                &idempotency_key_value,
                &command_id_value,
                &payload_json_value,
                &result_json_value,
            ])?;
        self.database
            .batch(vec![
                delete_expired_receipt,
                insert_version,
                insert_receipt,
                update_installation,
            ])
            .await
            .map_err(|_| DeliveryControlError::Database)?;
        self.confirm_destination_receipt(
            version_id,
            command_id,
            &payload_json,
            destination_url,
            enabled,
        )
        .await
    }

    pub async fn form_mappings(
        &self,
        funnel_id: &str,
    ) -> Result<Vec<WebhookFormMappingRecord>, DeliveryControlError> {
        let funnel_id = D1Type::Text(funnel_id);
        let rows = self
            .database
            .prepare(
                "SELECT settings.form_id, settings.enabled, settings.expected_destination_version_id, settings.mapping_version_id, mapping.selected_fields_json, mapping.selected_field_contract_json FROM public_form_delivery_settings AS settings LEFT JOIN public_form_delivery_mapping_versions AS mapping ON mapping.mapping_version_id = settings.mapping_version_id AND mapping.funnel_id = settings.funnel_id AND mapping.form_id = settings.form_id AND mapping.destination_version_id = settings.expected_destination_version_id WHERE settings.funnel_id = ? ORDER BY settings.form_id",
            )
            .bind_refs([&funnel_id])?
            .all()
            .await
            .map_err(|_| DeliveryControlError::Database)?
            .results::<FormMappingRow>()
            .map_err(|_| DeliveryControlError::Database)?;
        rows.into_iter().map(decode_form_mapping).collect()
    }

    pub async fn save_form_mapping(
        &self,
        input: SaveWebhookFormMapping<'_>,
    ) -> Result<WebhookFormMappingRecord, DeliveryControlError> {
        let SaveWebhookFormMapping {
            idempotency_key: version_id,
            funnel_id,
            form_id,
            expected_published_revision_id,
            destination_version_id,
            expected_mapping_version_id,
            selected_fields_json,
            selected_field_contract_json,
            enabled,
        } = input;
        validate_uuid(version_id)?;
        validate_uuid(destination_version_id)?;
        validate_route_revision_id(expected_published_revision_id)?;
        let command_id = "internal.public_delivery.form_mapping_save";
        let payload_json = form_mapping_payload_json(input);
        if self.active_receipt(version_id).await?.is_some() {
            return self
                .confirm_form_mapping_receipt(command_id, &payload_json, input)
                .await;
        }

        let current_destination = self.destination().await?;
        if current_destination.destination_version_id.as_deref() != Some(destination_version_id) {
            return Err(DeliveryControlError::Conflict);
        }

        let current = self.form_mapping(funnel_id, form_id).await?;
        if current
            .as_ref()
            .and_then(|current| current.mapping_version_id.as_deref())
            != expected_mapping_version_id
        {
            if self.active_receipt(version_id).await?.is_some() {
                return self
                    .confirm_form_mapping_receipt(command_id, &payload_json, input)
                    .await;
            }
            return Err(DeliveryControlError::Conflict);
        }
        if self
            .mapping_version(version_id)
            .await?
            .is_some_and(|version| {
                version.funnel_id != funnel_id
                    || version.form_id != form_id
                    || version.destination_version_id != destination_version_id
                    || version.selected_fields_json != selected_fields_json
                    || version.selected_field_contract_json.as_deref()
                        != Some(selected_field_contract_json)
            })
        {
            return Err(DeliveryControlError::Conflict);
        }

        let version_id_value = D1Type::Text(version_id);
        let funnel_id_value = D1Type::Text(funnel_id);
        let form_id_value = D1Type::Text(form_id);
        let destination_version_id_value = D1Type::Text(destination_version_id);
        let selected_fields_json_value = D1Type::Text(selected_fields_json);
        let selected_field_contract_json_value = D1Type::Text(selected_field_contract_json);
        let enabled_value = D1Type::Boolean(enabled);
        let idempotency_key_value = D1Type::Text(version_id);
        let command_id_value = D1Type::Text(command_id);
        let payload_json_value = D1Type::Text(&payload_json);
        let expected_published_revision_id_value = D1Type::Text(expected_published_revision_id);
        let result_json = serde_json::json!({ "mapping_version_id": version_id }).to_string();
        let result_json_value = D1Type::Text(&result_json);
        let expected_mapping_version_id_value = expected_mapping_version_id
            .map(D1Type::Text)
            .unwrap_or(D1Type::Null);

        let delete_expired_receipt = self
            .database
            .prepare(
                "DELETE FROM command_receipts WHERE idempotency_key = ? AND expires_at <= unixepoch()",
            )
            .bind_refs([&idempotency_key_value])?;
        let insert_version = self
            .database
            .prepare(
                "INSERT INTO public_form_delivery_mapping_versions (mapping_version_id, funnel_id, form_id, destination_version_id, selected_fields_json, selected_field_contract_json, created_at) SELECT ?, ?, ?, ?, ?, ?, unixepoch() WHERE EXISTS (SELECT 1 FROM funnels WHERE id = ? AND published_revision_id = ?) AND EXISTS (SELECT 1 FROM webhook_installation_delivery WHERE installation_id = 1 AND destination_version_id = ?) AND ((? IS NULL AND NOT EXISTS (SELECT 1 FROM public_form_delivery_settings WHERE funnel_id = ? AND form_id = ?)) OR EXISTS (SELECT 1 FROM public_form_delivery_settings WHERE funnel_id = ? AND form_id = ? AND mapping_version_id IS ?)) ON CONFLICT(mapping_version_id) DO NOTHING",
            )
            .bind_refs([
                &version_id_value,
                &funnel_id_value,
                &form_id_value,
                &destination_version_id_value,
                &selected_fields_json_value,
                &selected_field_contract_json_value,
                &funnel_id_value,
                &expected_published_revision_id_value,
                &destination_version_id_value,
                &expected_mapping_version_id_value,
                &funnel_id_value,
                &form_id_value,
                &funnel_id_value,
                &form_id_value,
                &expected_mapping_version_id_value,
            ])?;
        let insert_receipt = self
            .database
            .prepare(
                "INSERT INTO command_receipts (idempotency_key, command_id, payload_json, result_json, expires_at) SELECT ?, ?, ?, ?, unixepoch() + 315360000 WHERE changes() = 1 AND EXISTS (SELECT 1 FROM funnels WHERE id = ? AND published_revision_id = ?) AND EXISTS (SELECT 1 FROM webhook_installation_delivery WHERE installation_id = 1 AND destination_version_id = ?) AND EXISTS (SELECT 1 FROM public_form_delivery_mapping_versions WHERE mapping_version_id = ? AND funnel_id = ? AND form_id = ? AND destination_version_id = ? AND selected_fields_json = ? AND selected_field_contract_json = ?) AND ((? IS NULL AND NOT EXISTS (SELECT 1 FROM public_form_delivery_settings WHERE funnel_id = ? AND form_id = ?)) OR EXISTS (SELECT 1 FROM public_form_delivery_settings WHERE funnel_id = ? AND form_id = ? AND mapping_version_id IS ?)) ON CONFLICT(idempotency_key) DO NOTHING",
            )
            .bind_refs([
                &idempotency_key_value,
                &command_id_value,
                &payload_json_value,
                &result_json_value,
                &funnel_id_value,
                &expected_published_revision_id_value,
                &destination_version_id_value,
                &version_id_value,
                &funnel_id_value,
                &form_id_value,
                &destination_version_id_value,
                &selected_fields_json_value,
                &selected_field_contract_json_value,
                &expected_mapping_version_id_value,
                &funnel_id_value,
                &form_id_value,
                &funnel_id_value,
                &form_id_value,
                &expected_mapping_version_id_value,
            ])?;
        let write_setting = self
            .database
            .prepare(
                "INSERT INTO public_form_delivery_settings (funnel_id, form_id, enabled, expected_destination_version_id, mapping_version_id) SELECT ?, ?, ?, ?, ? WHERE changes() = 1 AND EXISTS (SELECT 1 FROM command_receipts WHERE idempotency_key = ? AND command_id = ? AND payload_json = ? AND result_json = ? AND expires_at > unixepoch()) AND EXISTS (SELECT 1 FROM public_form_delivery_mapping_versions WHERE mapping_version_id = ? AND funnel_id = ? AND form_id = ? AND destination_version_id = ? AND selected_fields_json = ? AND selected_field_contract_json = ?) AND EXISTS (SELECT 1 FROM funnels WHERE id = ? AND published_revision_id = ?) AND ((? IS NULL AND NOT EXISTS (SELECT 1 FROM public_form_delivery_settings WHERE funnel_id = ? AND form_id = ?)) OR EXISTS (SELECT 1 FROM public_form_delivery_settings WHERE funnel_id = ? AND form_id = ? AND mapping_version_id IS ?)) ON CONFLICT(funnel_id, form_id) DO UPDATE SET enabled = excluded.enabled, expected_destination_version_id = excluded.expected_destination_version_id, mapping_version_id = excluded.mapping_version_id WHERE public_form_delivery_settings.mapping_version_id IS ? RETURNING mapping_version_id",
            )
            .bind_refs([
                &funnel_id_value,
                &form_id_value,
                &enabled_value,
                &destination_version_id_value,
                &version_id_value,
                &idempotency_key_value,
                &command_id_value,
                &payload_json_value,
                &result_json_value,
                &version_id_value,
                &funnel_id_value,
                &form_id_value,
                &destination_version_id_value,
                &selected_fields_json_value,
                &selected_field_contract_json_value,
                &funnel_id_value,
                &expected_published_revision_id_value,
                &expected_mapping_version_id_value,
                &funnel_id_value,
                &form_id_value,
                &funnel_id_value,
                &form_id_value,
                &expected_mapping_version_id_value,
                &expected_mapping_version_id_value,
            ])?;
        self.database
            .batch(vec![
                delete_expired_receipt,
                insert_version,
                insert_receipt,
                write_setting,
            ])
            .await
            .map_err(|_| DeliveryControlError::Database)?;
        self.confirm_form_mapping_receipt(command_id, &payload_json, input)
            .await
    }
    pub async fn retry_delivery(
        &self,
        submission_id: &str,
        idempotency_key: &str,
    ) -> Result<(), DeliveryControlError> {
        self.apply_delivery_action(submission_id, idempotency_key, DeliveryAction::Retry)
            .await
    }

    pub async fn cancel_delivery(
        &self,
        submission_id: &str,
        idempotency_key: &str,
    ) -> Result<(), DeliveryControlError> {
        self.apply_delivery_action(submission_id, idempotency_key, DeliveryAction::Cancel)
            .await
    }

    async fn confirm_destination_receipt(
        &self,
        idempotency_key: &str,
        command_id: &str,
        payload_json: &str,
        destination_url: &str,
        enabled: bool,
    ) -> Result<WebhookDestinationRecord, DeliveryControlError> {
        let receipt = self
            .active_receipt(idempotency_key)
            .await?
            .ok_or(DeliveryControlError::Conflict)?;
        if receipt.command_id != command_id || receipt.payload_json != payload_json {
            return Err(DeliveryControlError::Conflict);
        }
        let result = serde_json::from_str::<DestinationMutationResult>(&receipt.result_json)
            .map_err(|_| DeliveryControlError::InvalidRecord)?;
        let current = self.destination().await?;
        if current.destination_version_id.as_deref() != Some(result.destination_version_id.as_str())
            || current.destination_url.as_deref() != Some(destination_url)
            || current.enabled != enabled
        {
            return Err(DeliveryControlError::Conflict);
        }
        Ok(current)
    }

    async fn confirm_form_mapping_receipt(
        &self,
        command_id: &str,
        payload_json: &str,
        input: SaveWebhookFormMapping<'_>,
    ) -> Result<WebhookFormMappingRecord, DeliveryControlError> {
        let SaveWebhookFormMapping {
            idempotency_key,
            funnel_id,
            form_id,
            expected_published_revision_id: _,
            destination_version_id,
            expected_mapping_version_id: _,
            selected_fields_json,
            selected_field_contract_json,
            enabled,
        } = input;
        let receipt = self
            .active_receipt(idempotency_key)
            .await?
            .ok_or(DeliveryControlError::Conflict)?;
        if receipt.command_id != command_id || receipt.payload_json != payload_json {
            return Err(DeliveryControlError::Conflict);
        }
        let result = serde_json::from_str::<FormMappingMutationResult>(&receipt.result_json)
            .map_err(|_| DeliveryControlError::InvalidRecord)?;
        let current = self
            .form_mapping(funnel_id, form_id)
            .await?
            .ok_or(DeliveryControlError::Conflict)?;
        if result.mapping_version_id != idempotency_key
            || current.mapping_version_id.as_deref() != Some(result.mapping_version_id.as_str())
            || current.expected_destination_version_id.as_deref() != Some(destination_version_id)
            || current.selected_fields_json.as_deref() != Some(selected_fields_json)
            || current.selected_field_contract_json.as_deref() != Some(selected_field_contract_json)
            || current.enabled != enabled
        {
            return Err(DeliveryControlError::Conflict);
        }
        Ok(current)
    }

    async fn apply_delivery_action(
        &self,
        submission_id: &str,
        idempotency_key: &str,
        action: DeliveryAction,
    ) -> Result<(), DeliveryControlError> {
        validate_uuid(submission_id)?;
        validate_uuid(idempotency_key)?;
        let command_id = D1Type::Text(action.command_id());
        let submission_id_value = D1Type::Text(submission_id);
        let idempotency_key_value = D1Type::Text(idempotency_key);
        let payload_json = serde_json::json!({"submission_id": submission_id}).to_string();
        let result_json = serde_json::json!({
            "submission_id": submission_id,
            "action": action.as_str(),
            "accepted": true,
        })
        .to_string();
        let payload_json_value = D1Type::Text(&payload_json);
        let result_json_value = D1Type::Text(&result_json);
        if let Some(receipt) = self.active_receipt(idempotency_key).await? {
            return if receipt.command_id == action.command_id()
                && receipt.payload_json == payload_json
            {
                Ok(())
            } else {
                Err(DeliveryControlError::Conflict)
            };
        }

        let delete_expired_receipt = self
            .database
            .prepare(
                "DELETE FROM command_receipts WHERE idempotency_key = ? AND expires_at <= unixepoch()",
            )
            .bind_refs([&idempotency_key_value])?;
        let insert_receipt = self
            .database
            .prepare(format!(
                "INSERT INTO command_receipts (idempotency_key, command_id, payload_json, result_json, expires_at) SELECT ?, ?, ?, ?, unixepoch() + 315360000 WHERE EXISTS (SELECT 1 FROM retained_visible_public_submission_delivery AS visible JOIN public_submission_delivery_state AS state ON state.task_id = visible.task_id WHERE visible.task_id = ? AND {}) ON CONFLICT(idempotency_key) DO NOTHING",
                action.eligibility_sql()
            ))
            .bind_refs([
                &idempotency_key_value,
                &command_id,
                &payload_json_value,
                &result_json_value,
                &submission_id_value,
            ])?;
        let update_delivery = self
            .database
            .prepare(format!(
                "UPDATE public_submission_delivery_state SET {} WHERE task_id = ? AND changes() = 1 AND EXISTS (SELECT 1 FROM command_receipts WHERE idempotency_key = ? AND command_id = ? AND payload_json = ? AND result_json = ? AND expires_at > unixepoch()) AND EXISTS (SELECT 1 FROM retained_visible_public_submission_delivery AS visible JOIN public_submission_delivery_state AS state ON state.task_id = visible.task_id WHERE visible.task_id = public_submission_delivery_state.task_id AND {}) RETURNING task_id",
                action.update_sql(),
                action.eligibility_sql()
            ))
            .bind_refs([
                &submission_id_value,
                &idempotency_key_value,
                &command_id,
                &payload_json_value,
                &result_json_value,
            ])?;
        let results = self
            .database
            .batch(vec![
                delete_expired_receipt,
                insert_receipt,
                update_delivery,
            ])
            .await
            .map_err(|_| DeliveryControlError::Database)?;
        let updated = results
            .get(2)
            .ok_or(DeliveryControlError::InvalidRecord)?
            .results::<TaskIdRow>()
            .map_err(|_| DeliveryControlError::Database)?
            .into_iter()
            .next();
        if updated.is_some_and(|updated| !updated.task_id.is_empty()) {
            return Ok(());
        }
        if let Some(receipt) = self.active_receipt(idempotency_key).await? {
            return if receipt.command_id == action.command_id()
                && receipt.payload_json == payload_json
            {
                Ok(())
            } else {
                Err(DeliveryControlError::Conflict)
            };
        }
        if self.visible_task_exists(submission_id).await? {
            Err(DeliveryControlError::Conflict)
        } else {
            Err(DeliveryControlError::NotFound)
        }
    }

    async fn destination_version_exists(
        &self,
        version_id: &str,
    ) -> Result<bool, DeliveryControlError> {
        let version_id = D1Type::Text(version_id);
        self.database
            .prepare(
                "SELECT 1 AS present FROM webhook_destination_versions WHERE destination_version_id = ?",
            )
            .bind_refs([&version_id])?
            .first::<ExistsRow>(None)
            .await
            .map(|row| row.is_some_and(|row| row.present == 1))
            .map_err(|_| DeliveryControlError::Database)
    }

    async fn form_mapping(
        &self,
        funnel_id: &str,
        form_id: &str,
    ) -> Result<Option<WebhookFormMappingRecord>, DeliveryControlError> {
        let funnel_id = D1Type::Text(funnel_id);
        let form_id = D1Type::Text(form_id);
        let row = self
            .database
            .prepare(
                "SELECT settings.form_id, settings.enabled, settings.expected_destination_version_id, settings.mapping_version_id, mapping.selected_fields_json, mapping.selected_field_contract_json FROM public_form_delivery_settings AS settings LEFT JOIN public_form_delivery_mapping_versions AS mapping ON mapping.mapping_version_id = settings.mapping_version_id AND mapping.funnel_id = settings.funnel_id AND mapping.form_id = settings.form_id AND mapping.destination_version_id = settings.expected_destination_version_id WHERE settings.funnel_id = ? AND settings.form_id = ?",
            )
            .bind_refs([&funnel_id, &form_id])?
            .first::<FormMappingRow>(None)
            .await
            .map_err(|_| DeliveryControlError::Database)?;
        row.map(decode_form_mapping).transpose()
    }

    async fn mapping_version(
        &self,
        version_id: &str,
    ) -> Result<Option<MappingVersionRow>, DeliveryControlError> {
        let version_id = D1Type::Text(version_id);
        self.database
            .prepare(
                "SELECT funnel_id, form_id, destination_version_id, selected_fields_json, selected_field_contract_json FROM public_form_delivery_mapping_versions WHERE mapping_version_id = ?",
            )
            .bind_refs([&version_id])?
            .first::<MappingVersionRow>(None)
            .await
            .map_err(|_| DeliveryControlError::Database)
    }

    async fn active_receipt(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<IdempotencyReceiptRow>, DeliveryControlError> {
        let idempotency_key = D1Type::Text(idempotency_key);
        self.database
            .prepare(
                "SELECT command_id, payload_json, result_json FROM command_receipts WHERE idempotency_key = ? AND expires_at > unixepoch()",
            )
            .bind_refs([&idempotency_key])?
            .first::<IdempotencyReceiptRow>(None)
            .await
            .map_err(|_| DeliveryControlError::Database)
    }

    async fn visible_task_exists(&self, submission_id: &str) -> Result<bool, DeliveryControlError> {
        let submission_id = D1Type::Text(submission_id);
        self.database
            .prepare(
                "SELECT 1 AS present FROM retained_visible_public_submission_delivery WHERE task_id = ?",
            )
            .bind_refs([&submission_id])?
            .first::<ExistsRow>(None)
            .await
            .map(|row| row.is_some_and(|row| row.present == 1))
            .map_err(|_| DeliveryControlError::Database)
    }
}

#[derive(Clone, Copy)]
enum DeliveryAction {
    Retry,
    Cancel,
}

impl DeliveryAction {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Retry => "retry",
            Self::Cancel => "cancel",
        }
    }

    const fn command_id(self) -> &'static str {
        match self {
            Self::Retry => "internal.public_delivery.retry",
            Self::Cancel => "internal.public_delivery.cancel",
        }
    }

    const fn eligibility_sql(self) -> &'static str {
        match self {
            Self::Retry => "((state.status = 'pending' AND state.attempt_count < 5 AND visible.created_at + 86400 > unixepoch()) OR (state.status = 'failed' AND state.attempt_count < 5 AND visible.created_at + 86400 > unixepoch() AND state.last_outcome_code NOT IN ('invalid_destination', 'incompatible_mapping', 'invalid_submission', 'signing_failed', 'retry_exhausted', 'delivery_window_elapsed', 'cancelled_by_owner'))) ",
            Self::Cancel => "state.status = 'pending'",
        }
    }

    const fn update_sql(self) -> &'static str {
        match self {
            Self::Retry => "status = 'pending', next_attempt_at = unixepoch(), completed_at = NULL, lease_token = NULL, lease_expires_at = NULL, updated_at = unixepoch()",
            Self::Cancel => "status = 'failed', last_outcome_code = 'cancelled_by_owner', completed_at = unixepoch(), lease_token = NULL, lease_expires_at = NULL, updated_at = unixepoch()",
        }
    }
}

fn decode_installation(
    row: InstallationRow,
) -> Result<WebhookDestinationRecord, DeliveryControlError> {
    let enabled = match row.enabled {
        0 => false,
        1 => true,
        _ => return Err(DeliveryControlError::InvalidRecord),
    };
    if row.destination_version_id.is_some() != row.destination_url.is_some()
        || (enabled && row.destination_version_id.is_none())
    {
        return Err(DeliveryControlError::InvalidRecord);
    }
    Ok(WebhookDestinationRecord {
        enabled,
        destination_version_id: row.destination_version_id,
        destination_url: row.destination_url,
    })
}

fn decode_form_mapping(
    row: FormMappingRow,
) -> Result<WebhookFormMappingRecord, DeliveryControlError> {
    let enabled = match row.enabled {
        0 => false,
        1 => true,
        _ => return Err(DeliveryControlError::InvalidRecord),
    };
    if row.mapping_version_id.is_some() != row.selected_fields_json.is_some()
        || row.mapping_version_id.is_some() != row.expected_destination_version_id.is_some()
        || (enabled && row.mapping_version_id.is_none())
    {
        return Err(DeliveryControlError::InvalidRecord);
    }
    Ok(WebhookFormMappingRecord {
        form_id: row.form_id,
        enabled,
        expected_destination_version_id: row.expected_destination_version_id,
        mapping_version_id: row.mapping_version_id,
        selected_fields_json: row.selected_fields_json,
        selected_field_contract_json: row.selected_field_contract_json,
    })
}

fn validate_uuid(value: &str) -> Result<(), DeliveryControlError> {
    uuid::Uuid::parse_str(value)
        .map(|id| id.to_string() == value)
        .ok()
        .filter(|canonical| *canonical)
        .map(|_| ())
        .ok_or(DeliveryControlError::InvalidInput)
}

fn validate_route_revision_id(value: &str) -> Result<(), DeliveryControlError> {
    if (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        Ok(())
    } else {
        Err(DeliveryControlError::InvalidInput)
    }
}

impl From<Error> for DeliveryControlError {
    fn from(_: Error) -> Self {
        Self::Database
    }
}

#[cfg(test)]
mod tests {
    use super::{form_mapping_payload_json, SaveWebhookFormMapping};

    #[test]
    fn large_mapping_contracts_keep_receipts_bounded_and_payload_sensitive() {
        let options = (0..50)
            .map(|index| format!("{}-{index:09}", "\\".repeat(1014)))
            .collect::<Vec<_>>();
        let fields = (0..10)
            .map(|index| {
                serde_json::json!({
                    "kind": "select",
                    "id": format!("field-{index}"),
                    "name": format!("service_{index}"),
                    "label": format!("Service {index}"),
                    "required": false,
                    "options": options.clone(),
                })
            })
            .collect::<Vec<_>>();
        let contract = serde_json::to_string(&fields).expect("field contract JSON");
        let duplicated_payload = serde_json::json!({
            "selected_field_contract_json": contract,
        })
        .to_string();
        let input = SaveWebhookFormMapping {
            idempotency_key: "11111111-1111-4111-8111-111111111111",
            funnel_id: "owner-funnel",
            form_id: "contact",
            expected_published_revision_id: "revision-1",
            destination_version_id: "22222222-2222-4222-8222-222222222222",
            expected_mapping_version_id: None,
            selected_fields_json: "[\"service\"]",
            selected_field_contract_json: &contract,
            enabled: true,
        };
        let payload = form_mapping_payload_json(input);
        let payload_json: serde_json::Value =
            serde_json::from_str(&payload).expect("receipt payload is valid JSON");
        let digest = payload_json["selected_field_contract_sha256"]
            .as_str()
            .expect("receipt carries a field contract digest");

        assert!(contract.len() > 1_000_000);
        assert!(contract.len() <= 1_048_576);
        assert!(duplicated_payload.len() > 2_000_000);
        assert!(payload.len() < 2_000_000);
        assert_eq!(digest.len(), 64);
        assert!(digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
        assert!(!payload.contains(&contract));

        let changed_contract = format!("{contract} ");
        let changed_payload = form_mapping_payload_json(SaveWebhookFormMapping {
            selected_field_contract_json: &changed_contract,
            ..input
        });
        assert_ne!(payload, changed_payload);
    }
}
