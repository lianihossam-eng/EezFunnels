use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};
use worker::d1::D1Type;
use worker::{D1Database, Error};

const SUPPORTED_SCHEMA_VERSION: i32 = 1;
const MAX_FUNNEL_BYTES: usize = 1_048_576;
const MAX_FUNNELS: i32 = 100;
const MAX_REVISIONS_RETAINED: i32 = 100;

pub type RepositoryResult<T> = std::result::Result<T, RepositoryError>;

#[derive(Debug)]
pub enum RepositoryError {
    Database(Error),
    UnsupportedSchemaVersion(i64),
    InvalidJson,
    PayloadTooLarge,
    InvalidField(&'static str),
    FunnelNotFound,
    FunnelLimitReached,
    RevisionLimitReached,
}

impl Display for RepositoryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "D1 repository operation failed: {error}"),
            Self::UnsupportedSchemaVersion(version) => {
                write!(
                    formatter,
                    "validation/schema_version_unsupported: {version}"
                )
            }
            Self::InvalidJson => formatter.write_str("validation/json_invalid"),
            Self::PayloadTooLarge => formatter.write_str("validation/payload_too_large"),
            Self::InvalidField(field) => write!(formatter, "validation/field_invalid: {field}"),
            Self::FunnelNotFound => formatter.write_str("not_found/funnel"),
            Self::FunnelLimitReached => formatter.write_str("conflict/funnel_limit_reached"),
            Self::RevisionLimitReached => formatter.write_str("validation/revision_limit_reached"),
        }
    }
}

impl From<Error> for RepositoryError {
    fn from(error: Error) -> Self {
        Self::Database(error)
    }
}

impl From<RepositoryError> for Error {
    fn from(error: RepositoryError) -> Self {
        Self::RustError(error.to_string())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct FunnelRecord {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub published_revision_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct RevisionRecord {
    pub id: String,
    pub funnel_id: String,
    pub schema_version: i64,
    pub data_json: String,
    pub render_manifest_json: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ThemeRecord {
    pub id: String,
    pub name: String,
    pub tokens_json: String,
    pub is_default: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewFunnel {
    pub id: String,
    pub name: String,
    pub slug: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewRevision {
    pub id: String,
    pub funnel_id: String,
    pub schema_version: i64,
    pub data_json: String,
    pub render_manifest_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewTheme {
    pub id: String,
    pub name: String,
    pub tokens_json: String,
    pub is_default: bool,
}

pub enum MutationResult {
    Stored(String),
    PayloadMismatch,
}

#[derive(Deserialize)]
struct CommandReceipt {
    command_id: String,
    payload_json: String,
    result_json: String,
}

pub struct Repository<'a> {
    database: &'a D1Database,
}

impl<'a> Repository<'a> {
    pub fn new(database: &'a D1Database) -> Self {
        Self { database }
    }

    pub async fn create_funnel(&self, input: &NewFunnel) -> RepositoryResult<FunnelRecord> {
        validate_required_field(&input.id, "id")?;
        validate_required_field(&input.name, "name")?;
        validate_required_field(&input.slug, "slug")?;

        let id = D1Type::Text(&input.id);
        let name = D1Type::Text(&input.name);
        let slug = D1Type::Text(&input.slug);
        self.database
            .prepare("INSERT INTO funnels (id, name, slug) VALUES (?, ?, ?)")
            .bind_refs([&id, &name, &slug])?
            .run()
            .await?;
        self.get_funnel(&input.id)
            .await?
            .ok_or_else(|| Error::RustError("created funnel could not be read".to_owned()).into())
    }

    pub async fn create_funnel_with_revision(
        &self,
        funnel: &NewFunnel,
        revision: &NewRevision,
        command_id: &str,
        idempotency_key: &str,
        payload_json: &str,
        result_json: &str,
    ) -> RepositoryResult<MutationResult> {
        validate_required_field(&funnel.id, "id")?;
        validate_required_field(&funnel.name, "name")?;
        validate_required_field(&funnel.slug, "slug")?;
        validate_required_field(&revision.id, "revision_id")?;
        validate_required_field(command_id, "command_id")?;
        validate_idempotency_key(idempotency_key)?;
        if revision.funnel_id != funnel.id {
            return Err(RepositoryError::InvalidField("funnel_id"));
        }
        validate_schema_version(revision.schema_version)?;
        validate_payload_size(&revision.data_json)?;
        self.validate_json(&revision.data_json).await?;
        self.validate_json(&revision.render_manifest_json).await?;
        self.validate_json(payload_json).await?;
        self.validate_json(result_json).await?;

        if let Some(receipt) = self.active_receipt(idempotency_key).await? {
            return Ok(classify_receipt(receipt, command_id, payload_json));
        }

        let key = D1Type::Text(idempotency_key);
        let command = D1Type::Text(command_id);
        let payload = D1Type::Text(payload_json);
        let result = D1Type::Text(result_json);
        let delete_expired_receipts = self.database.prepare(
            "DELETE FROM command_receipts WHERE expires_at <= CAST(strftime('%s', 'now') AS INTEGER)",
        );
        let max_funnels = D1Type::Integer(MAX_FUNNELS);
        let insert_receipt = self
            .database
            .prepare(
                "INSERT INTO command_receipts (idempotency_key, command_id, payload_json, result_json, expires_at) SELECT ?, ?, ?, ?, CAST(strftime('%s', 'now') AS INTEGER) + 86400 WHERE (SELECT COUNT(*) FROM funnels) < ? ON CONFLICT(idempotency_key) DO UPDATE SET command_id = excluded.command_id, payload_json = excluded.payload_json, result_json = excluded.result_json, expires_at = excluded.expires_at WHERE command_receipts.expires_at <= CAST(strftime('%s', 'now') AS INTEGER)",
            )
            .bind_refs([&key, &command, &payload, &result, &max_funnels])?;

        let funnel_id = D1Type::Text(&funnel.id);
        let funnel_name = D1Type::Text(&funnel.name);
        let funnel_slug = D1Type::Text(&funnel.slug);
        let insert_funnel = self
            .database
            .prepare(
                "INSERT INTO funnels (id, name, slug) SELECT ?, ?, ? WHERE EXISTS (SELECT 1 FROM command_receipts WHERE idempotency_key = ? AND command_id = ? AND payload_json = ? AND result_json = ? AND expires_at > CAST(strftime('%s', 'now') AS INTEGER)) AND (SELECT COUNT(*) FROM funnels) < ?",
            )
            .bind_refs([
                &funnel_id,
                &funnel_name,
                &funnel_slug,
                &key,
                &command,
                &payload,
                &result,
                &max_funnels,
            ])?;

        let revision_id = D1Type::Text(&revision.id);
        let revision_funnel_id = D1Type::Text(&revision.funnel_id);
        let schema_version = D1Type::Integer(1);
        let data_json = D1Type::Text(&revision.data_json);
        let manifest_json = D1Type::Text(&revision.render_manifest_json);
        let insert_revision = self
            .database
            .prepare(
                "INSERT INTO revisions (id, funnel_id, schema_version, data_json, render_manifest_json) SELECT ?, ?, ?, ?, ? WHERE EXISTS (SELECT 1 FROM funnels WHERE id = ?) AND EXISTS (SELECT 1 FROM command_receipts WHERE idempotency_key = ? AND command_id = ? AND payload_json = ? AND result_json = ? AND expires_at > CAST(strftime('%s', 'now') AS INTEGER))",
            )
            .bind_refs([
                &revision_id,
                &revision_funnel_id,
                &schema_version,
                &data_json,
                &manifest_json,
                &revision_funnel_id,
                &key,
                &command,
                &payload,
                &result,
            ])?;

        if let Err(error) = self
            .database
            .batch(vec![
                delete_expired_receipts,
                insert_receipt,
                insert_funnel,
                insert_revision,
            ])
            .await
        {
            if let Some(receipt) = self.active_receipt(idempotency_key).await? {
                return Ok(classify_receipt(receipt, command_id, payload_json));
            }
            if self.funnel_count().await? >= i64::from(MAX_FUNNELS) {
                return Err(RepositoryError::FunnelLimitReached);
            }
            return Err(error.into());
        }

        match self.active_receipt(idempotency_key).await? {
            Some(receipt) => Ok(classify_receipt(receipt, command_id, payload_json)),
            None if self.funnel_count().await? >= i64::from(MAX_FUNNELS) => {
                Err(RepositoryError::FunnelLimitReached)
            }
            None => Err(Error::RustError("command receipt was not committed".to_owned()).into()),
        }
    }

    pub async fn delete_funnel_idempotently(
        &self,
        funnel_id: &str,
        command_id: &str,
        idempotency_key: &str,
        payload_json: &str,
        result_json: &str,
    ) -> RepositoryResult<MutationResult> {
        validate_required_field(funnel_id, "funnel_id")?;
        validate_required_field(command_id, "command_id")?;
        validate_idempotency_key(idempotency_key)?;
        self.validate_json(payload_json).await?;
        self.validate_json(result_json).await?;

        if let Some(receipt) = self.active_receipt(idempotency_key).await? {
            return Ok(classify_receipt(receipt, command_id, payload_json));
        }

        let key = D1Type::Text(idempotency_key);
        let command = D1Type::Text(command_id);
        let payload = D1Type::Text(payload_json);
        let result = D1Type::Text(result_json);
        let funnel_id_value = D1Type::Text(funnel_id);
        let delete_expired_receipts = self.database.prepare(
            "DELETE FROM command_receipts WHERE expires_at <= CAST(strftime('%s', 'now') AS INTEGER)",
        );
        let insert_receipt = self
            .database
            .prepare(
                "INSERT INTO command_receipts (idempotency_key, command_id, payload_json, result_json, expires_at) SELECT ?, ?, ?, ?, CAST(strftime('%s', 'now') AS INTEGER) + 86400 WHERE EXISTS (SELECT 1 FROM funnels WHERE id = ?) ON CONFLICT(idempotency_key) DO UPDATE SET command_id = excluded.command_id, payload_json = excluded.payload_json, result_json = excluded.result_json, expires_at = excluded.expires_at WHERE command_receipts.expires_at <= CAST(strftime('%s', 'now') AS INTEGER)",
            )
            .bind_refs([&key, &command, &payload, &result, &funnel_id_value])?;
        let delete_funnel = self
            .database
            .prepare(
                "DELETE FROM funnels WHERE id = ? AND EXISTS (SELECT 1 FROM command_receipts WHERE idempotency_key = ? AND command_id = ? AND payload_json = ? AND result_json = ? AND expires_at > CAST(strftime('%s', 'now') AS INTEGER))",
            )
            .bind_refs([&funnel_id_value, &key, &command, &payload, &result])?;

        if let Err(error) = self
            .database
            .batch(vec![delete_expired_receipts, insert_receipt, delete_funnel])
            .await
        {
            if let Some(receipt) = self.active_receipt(idempotency_key).await? {
                return Ok(classify_receipt(receipt, command_id, payload_json));
            }
            return Err(error.into());
        }

        match self.active_receipt(idempotency_key).await? {
            Some(receipt) => Ok(classify_receipt(receipt, command_id, payload_json)),
            None => Err(RepositoryError::FunnelNotFound),
        }
    }

    async fn active_receipt(
        &self,
        idempotency_key: &str,
    ) -> RepositoryResult<Option<CommandReceipt>> {
        let key = D1Type::Text(idempotency_key);
        self.database
            .prepare(
                "SELECT command_id, payload_json, result_json FROM command_receipts WHERE idempotency_key = ? AND expires_at > CAST(strftime('%s', 'now') AS INTEGER)",
            )
            .bind_refs([&key])?
            .first::<CommandReceipt>(None)
            .await
            .map_err(Into::into)
    }

    pub async fn get_funnel(&self, id: &str) -> RepositoryResult<Option<FunnelRecord>> {
        let id = D1Type::Text(id);
        self.database
            .prepare(
                "SELECT id, name, slug, published_revision_id, created_at, updated_at FROM funnels WHERE id = ?",
            )
            .bind_refs([&id])?
            .first::<FunnelRecord>(None)
            .await
            .map_err(Into::into)
    }

    pub async fn list_funnels(&self) -> RepositoryResult<Vec<FunnelRecord>> {
        let limit = D1Type::Integer(MAX_FUNNELS);
        let result = self
            .database
            .prepare(
                "SELECT id, name, slug, published_revision_id, created_at, updated_at FROM funnels ORDER BY created_at, id LIMIT ?",
            )
            .bind_refs([&limit])?
            .all()
            .await?;
        result.results::<FunnelRecord>().map_err(Into::into)
    }

    async fn funnel_count(&self) -> RepositoryResult<i64> {
        self.database
            .prepare("SELECT COUNT(*) AS funnel_count FROM funnels")
            .first::<i64>(Some("funnel_count"))
            .await?
            .ok_or_else(|| {
                Error::RustError("workspace funnel count could not be read".to_owned()).into()
            })
    }

    pub async fn update_funnel(
        &self,
        id: &str,
        name: &str,
        slug: &str,
    ) -> RepositoryResult<Option<FunnelRecord>> {
        validate_required_field(name, "name")?;
        validate_required_field(slug, "slug")?;
        let id = D1Type::Text(id);
        let name = D1Type::Text(name);
        let slug = D1Type::Text(slug);
        let result = self
            .database
            .prepare(
                "UPDATE funnels SET name = ?, slug = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ? RETURNING id, name, slug, published_revision_id, created_at, updated_at",
            )
            .bind_refs([&name, &slug, &id])?
            .first::<FunnelRecord>(None)
            .await?;
        Ok(result)
    }

    pub async fn delete_funnel(&self, id: &str) -> RepositoryResult<bool> {
        let id = D1Type::Text(id);
        let deleted = self
            .database
            .prepare("DELETE FROM funnels WHERE id = ? RETURNING id")
            .bind_refs([&id])?
            .first::<String>(Some("id"))
            .await?;
        Ok(deleted.is_some())
    }

    pub async fn create_revision(&self, input: &NewRevision) -> RepositoryResult<RevisionRecord> {
        validate_required_field(&input.id, "id")?;
        validate_required_field(&input.funnel_id, "funnel_id")?;
        if self.get_funnel(&input.funnel_id).await?.is_none() {
            return Err(RepositoryError::FunnelNotFound);
        }
        validate_schema_version(input.schema_version)?;
        validate_payload_size(&input.data_json)?;
        self.validate_json(&input.data_json).await?;
        self.validate_json(&input.render_manifest_json).await?;

        let id = D1Type::Text(&input.id);
        let funnel_id = D1Type::Text(&input.funnel_id);
        let schema_version = D1Type::Integer(1);
        let data_json = D1Type::Text(&input.data_json);
        let render_manifest_json = D1Type::Text(&input.render_manifest_json);
        let result = self
            .database
            .prepare(
                "INSERT INTO revisions (id, funnel_id, schema_version, data_json, render_manifest_json) SELECT ?, ?, ?, ?, ? WHERE (SELECT COUNT(*) FROM revisions WHERE funnel_id = ?) < ? RETURNING id, funnel_id, schema_version, data_json, render_manifest_json, created_at",
            )
            .bind_refs([
                &id,
                &funnel_id,
                &schema_version,
                &data_json,
                &render_manifest_json,
                &funnel_id,
                &D1Type::Integer(MAX_REVISIONS_RETAINED),
            ])?
            .first::<RevisionRecord>(None)
            .await?;
        let revision = result.ok_or(RepositoryError::RevisionLimitReached)?;
        validate_schema_version(revision.schema_version)?;
        Ok(revision)
    }

    pub async fn get_revision(&self, id: &str) -> RepositoryResult<Option<RevisionRecord>> {
        let id = D1Type::Text(id);
        let revision = self
            .database
            .prepare(
                "SELECT id, funnel_id, schema_version, data_json, render_manifest_json, created_at FROM revisions WHERE id = ?",
            )
            .bind_refs([&id])?
            .first::<RevisionRecord>(None)
            .await?;
        if let Some(revision) = revision {
            validate_schema_version(revision.schema_version)?;
            Ok(Some(revision))
        } else {
            Ok(None)
        }
    }

    pub async fn list_revisions(&self, funnel_id: &str) -> RepositoryResult<Vec<RevisionRecord>> {
        let funnel_id = D1Type::Text(funnel_id);
        let result = self
            .database
            .prepare(
                "SELECT id, funnel_id, schema_version, data_json, render_manifest_json, created_at FROM revisions WHERE funnel_id = ? ORDER BY created_at, id",
            )
            .bind_refs([&funnel_id])?
            .all()
            .await?;
        let revisions = result.results::<RevisionRecord>()?;
        for revision in &revisions {
            validate_schema_version(revision.schema_version)?;
        }
        Ok(revisions)
    }

    pub async fn delete_revision(&self, id: &str) -> RepositoryResult<bool> {
        let id = D1Type::Text(id);
        let deleted = self
            .database
            .prepare(
                "DELETE FROM revisions WHERE id = ? AND NOT EXISTS (SELECT 1 FROM funnels WHERE published_revision_id = ?) RETURNING id",
            )
            .bind_refs([&id, &id])?
            .first::<String>(Some("id"))
            .await?;
        Ok(deleted.is_some())
    }

    pub async fn publish_revision(
        &self,
        funnel_id: &str,
        revision_id: &str,
        expected_published_revision_id: Option<&str>,
    ) -> RepositoryResult<bool> {
        let funnel_id = D1Type::Text(funnel_id);
        let revision_id = D1Type::Text(revision_id);
        let expected = expected_published_revision_id
            .map(D1Type::Text)
            .unwrap_or(D1Type::Null);
        let supported_schema_version = D1Type::Integer(SUPPORTED_SCHEMA_VERSION);
        let published = self
            .database
            .prepare(
                "UPDATE funnels SET published_revision_id = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ? AND published_revision_id IS ? AND published_revision_id IS NOT ? AND EXISTS (SELECT 1 FROM revisions WHERE id = ? AND funnel_id = ? AND schema_version = ?) RETURNING id",
            )
            .bind_refs([
                &revision_id,
                &funnel_id,
                &expected,
                &revision_id,
                &revision_id,
                &funnel_id,
                &supported_schema_version,
            ])?
            .first::<String>(Some("id"))
            .await?;
        Ok(published.is_some())
    }

    pub async fn create_theme(&self, input: &NewTheme) -> RepositoryResult<ThemeRecord> {
        validate_required_field(&input.id, "id")?;
        validate_required_field(&input.name, "name")?;
        self.validate_json(&input.tokens_json).await?;
        let id = D1Type::Text(&input.id);
        let name = D1Type::Text(&input.name);
        let tokens_json = D1Type::Text(&input.tokens_json);
        let is_default = D1Type::Boolean(input.is_default);
        self.database
            .prepare("INSERT INTO themes (id, name, tokens_json, is_default) VALUES (?, ?, ?, ?)")
            .bind_refs([&id, &name, &tokens_json, &is_default])?
            .run()
            .await?;
        self.get_theme(&input.id)
            .await?
            .ok_or_else(|| Error::RustError("created theme could not be read".to_owned()).into())
    }

    pub async fn get_theme(&self, id: &str) -> RepositoryResult<Option<ThemeRecord>> {
        let id = D1Type::Text(id);
        self.database
            .prepare(
                "SELECT id, name, tokens_json, is_default, created_at, updated_at FROM themes WHERE id = ?",
            )
            .bind_refs([&id])?
            .first::<ThemeRecord>(None)
            .await
            .map_err(Into::into)
    }

    pub async fn list_themes(&self) -> RepositoryResult<Vec<ThemeRecord>> {
        let result = self
            .database
            .prepare(
                "SELECT id, name, tokens_json, is_default, created_at, updated_at FROM themes ORDER BY name, id",
            )
            .all()
            .await?;
        result.results::<ThemeRecord>().map_err(Into::into)
    }

    pub async fn update_theme(
        &self,
        id: &str,
        name: &str,
        tokens_json: &str,
        is_default: bool,
    ) -> RepositoryResult<Option<ThemeRecord>> {
        validate_required_field(name, "name")?;
        self.validate_json(tokens_json).await?;
        let id = D1Type::Text(id);
        let name = D1Type::Text(name);
        let tokens_json = D1Type::Text(tokens_json);
        let is_default = D1Type::Boolean(is_default);
        let result = self
            .database
            .prepare(
                "UPDATE themes SET name = ?, tokens_json = ?, is_default = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ? RETURNING id, name, tokens_json, is_default, created_at, updated_at",
            )
            .bind_refs([&name, &tokens_json, &is_default, &id])?
            .first::<ThemeRecord>(None)
            .await?;
        Ok(result)
    }

    pub async fn delete_theme(&self, id: &str) -> RepositoryResult<bool> {
        let id = D1Type::Text(id);
        let deleted = self
            .database
            .prepare("DELETE FROM themes WHERE id = ? RETURNING id")
            .bind_refs([&id])?
            .first::<String>(Some("id"))
            .await?;
        Ok(deleted.is_some())
    }

    async fn validate_json(&self, value: &str) -> RepositoryResult<()> {
        let value = D1Type::Text(value);
        let is_valid = self
            .database
            .prepare("SELECT json_valid(?) AS is_valid")
            .bind_refs([&value])?
            .first::<i64>(Some("is_valid"))
            .await?
            .unwrap_or(0);
        if is_valid == 1 {
            Ok(())
        } else {
            Err(RepositoryError::InvalidJson)
        }
    }
}

pub fn validate_schema_version(version: i64) -> RepositoryResult<()> {
    if version == i64::from(SUPPORTED_SCHEMA_VERSION) {
        Ok(())
    } else {
        Err(RepositoryError::UnsupportedSchemaVersion(version))
    }
}

fn classify_receipt(
    receipt: CommandReceipt,
    command_id: &str,
    payload_json: &str,
) -> MutationResult {
    if receipt.command_id == command_id && receipt.payload_json == payload_json {
        MutationResult::Stored(receipt.result_json)
    } else {
        MutationResult::PayloadMismatch
    }
}

fn validate_idempotency_key(value: &str) -> RepositoryResult<()> {
    if !(1..=128).contains(&value.len())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        Err(RepositoryError::InvalidField("idempotency_key"))
    } else {
        Ok(())
    }
}

fn validate_required_field(value: &str, field: &'static str) -> RepositoryResult<()> {
    if value.trim().is_empty() {
        Err(RepositoryError::InvalidField(field))
    } else {
        Ok(())
    }
}

fn validate_payload_size(value: &str) -> RepositoryResult<()> {
    if value.len() > MAX_FUNNEL_BYTES {
        Err(RepositoryError::PayloadTooLarge)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        validate_payload_size, validate_required_field, validate_schema_version, RepositoryError,
    };

    #[test]
    fn schema_version_one_is_supported() {
        assert!(validate_schema_version(1).is_ok());
    }

    #[test]
    fn unsupported_schema_versions_are_rejected() {
        assert!(matches!(
            validate_schema_version(0),
            Err(RepositoryError::UnsupportedSchemaVersion(0))
        ));
        assert!(matches!(
            validate_schema_version(2),
            Err(RepositoryError::UnsupportedSchemaVersion(2))
        ));
        assert!(matches!(
            validate_schema_version(i64::MAX),
            Err(RepositoryError::UnsupportedSchemaVersion(i64::MAX))
        ));
    }

    #[test]
    fn payload_limit_counts_utf8_bytes() {
        let payload = "😀".repeat(262_144);
        assert!(validate_payload_size(&payload).is_ok());
        let payload_with_one_more_byte = format!("{payload}x");
        assert!(matches!(
            validate_payload_size(&payload_with_one_more_byte),
            Err(RepositoryError::PayloadTooLarge)
        ));
    }

    #[test]
    fn required_fields_reject_empty_and_whitespace_values() {
        assert!(validate_required_field("id", "id").is_ok());
        assert!(matches!(
            validate_required_field("  ", "id"),
            Err(RepositoryError::InvalidField("id"))
        ));
    }
}
