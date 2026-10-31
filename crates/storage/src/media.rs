use std::collections::{BTreeSet, HashMap};
use std::fmt::{Display, Formatter};
use std::io::Cursor;

use image::{ImageFormat, ImageReader, Limits};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use worker::d1::D1Type;
use worker::{Bucket, D1Database, Error, HttpMetadata, Include};

pub const MAX_MEDIA_ASSET_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_MEDIA_ASSET_COUNT: u64 = 1_000;
pub const MAX_MEDIA_LIBRARY_BYTES: u64 = 512 * 1024 * 1024;

const MEDIA_LIST_LIMIT: i32 = 1_000;
const MEDIA_QUERY_ID_LIMIT: usize = 100;
const MEDIA_OBJECT_LIST_PAGE_LIMIT: u32 = 50;
const MEDIA_OBJECT_LIST_PAGE_BUDGET: usize = 30;
const MEDIA_UPLOAD_LEASE_SECONDS: i32 = 300;
const MAX_IMAGE_DIMENSION: u32 = 4_096;
const MAX_DECODE_ALLOCATION: u64 = 64 * 1024 * 1024;
const R2_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MediaAssetRecord {
    pub id: String,
    pub mime: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaAssetContent {
    pub record: MediaAssetRecord,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub enum MediaAssetError {
    Database(Error),
    ObjectStore(Error),
    InvalidUpload,
    UploadTooLarge,
    QuotaExceeded,
    Conflict,
    NotFound,
    IntegrityFailure,
}

impl MediaAssetError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Database(_) | Self::ObjectStore(_) | Self::IntegrityFailure => {
                "external/unavailable"
            }
            Self::InvalidUpload => "validation/media_asset_invalid",
            Self::UploadTooLarge => "validation/media_asset_too_large",
            Self::QuotaExceeded => "rate_limit/exceeded",
            Self::Conflict => "conflict/media_asset_conflict",
            Self::NotFound => "not_found/asset",
        }
    }
}

impl Display for MediaAssetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(error) => {
                write!(formatter, "media asset database operation failed: {error}")
            }
            Self::ObjectStore(error) => {
                write!(formatter, "media asset object operation failed: {error}")
            }
            Self::InvalidUpload => formatter.write_str("validation/media_asset_invalid"),
            Self::UploadTooLarge => formatter.write_str("validation/media_asset_too_large"),
            Self::QuotaExceeded => formatter.write_str("rate_limit/exceeded"),
            Self::Conflict => formatter.write_str("conflict/media_asset_conflict"),
            Self::NotFound => formatter.write_str("not_found/asset"),
            Self::IntegrityFailure => formatter.write_str("external/unavailable"),
        }
    }
}

impl std::error::Error for MediaAssetError {}

#[derive(Deserialize)]
struct MediaAssetRow {
    id: String,
    r2_key: String,
    mime: String,
    size_bytes: u64,
    sha256: Option<String>,
    status: String,
    created_at: String,
    upload_token: Option<String>,
}

impl MediaAssetRow {
    fn public_record(self) -> Result<MediaAssetRecord, MediaAssetError> {
        if self.status != "ready" {
            return Err(MediaAssetError::NotFound);
        }
        let sha256 = self.sha256.ok_or(MediaAssetError::IntegrityFailure)?;
        if !is_lower_sha256(&sha256)
            || self.id != sha256
            || self.r2_key != format!("media/{sha256}")
            || !matches!(
                self.mime.as_str(),
                "image/png" | "image/jpeg" | "image/webp"
            )
            || self.size_bytes == 0
            || self.size_bytes > MAX_MEDIA_ASSET_BYTES as u64
        {
            return Err(MediaAssetError::IntegrityFailure);
        }
        Ok(MediaAssetRecord {
            id: self.id,
            mime: self.mime,
            size_bytes: self.size_bytes,
            sha256,
            created_at: self.created_at,
        })
    }
}

pub struct MediaAssetRepository<'a> {
    database: &'a D1Database,
    bucket: &'a Bucket,
}

impl<'a> MediaAssetRepository<'a> {
    pub fn new(database: &'a D1Database, bucket: &'a Bucket) -> Self {
        Self { database, bucket }
    }

    pub async fn list(&self) -> Result<Vec<MediaAssetRecord>, MediaAssetError> {
        let limit = D1Type::Integer(MEDIA_LIST_LIMIT);
        let result = self
            .database
            .prepare(
                "SELECT id, r2_key, mime, size_bytes, sha256, status, created_at, upload_token, upload_started_at FROM assets WHERE status = 'ready' AND sha256 IS NOT NULL ORDER BY created_at DESC, id LIMIT ?",
            )
            .bind_refs([&limit])
            .map_err(MediaAssetError::Database)?
            .all()
            .await
            .map_err(MediaAssetError::Database)?;
        result
            .results::<MediaAssetRow>()
            .map_err(MediaAssetError::Database)?
            .into_iter()
            .map(MediaAssetRow::public_record)
            .collect()
    }

    pub async fn get_many_ready(
        &self,
        asset_ids: &[String],
    ) -> Result<Vec<MediaAssetRecord>, MediaAssetError> {
        let asset_ids: Vec<&str> = asset_ids
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if asset_ids.is_empty() {
            return Ok(Vec::new());
        }
        if asset_ids.len() > MEDIA_LIST_LIMIT as usize {
            return Err(MediaAssetError::InvalidUpload);
        }
        let mut records = Vec::new();
        for asset_ids in asset_ids.chunks(MEDIA_QUERY_ID_LIMIT) {
            let placeholders = std::iter::repeat("?")
                .take(asset_ids.len())
                .collect::<Vec<_>>()
                .join(",");
            let query = format!(
                "SELECT id, r2_key, mime, size_bytes, sha256, status, created_at, upload_token, upload_started_at FROM assets WHERE status = 'ready' AND sha256 IS NOT NULL AND id IN ({placeholders}) ORDER BY id"
            );
            let arguments = asset_ids
                .iter()
                .map(|id| D1Type::Text(id))
                .collect::<Vec<_>>();
            let result = self
                .database
                .prepare(&query)
                .bind_refs(arguments.iter())
                .map_err(MediaAssetError::Database)?
                .all()
                .await
                .map_err(MediaAssetError::Database)?;
            records.extend(
                result
                    .results::<MediaAssetRow>()
                    .map_err(MediaAssetError::Database)?
                    .into_iter()
                    .map(MediaAssetRow::public_record)
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        records.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(records)
    }

    pub async fn get_many_ready_with_objects(
        &self,
        asset_ids: &[String],
    ) -> Result<Vec<MediaAssetRecord>, MediaAssetError> {
        let ready = self.get_many_ready(asset_ids).await?;
        if ready.is_empty() {
            return Ok(Vec::new());
        }
        let mut pending = ready
            .into_iter()
            .map(|record| (format!("media/{}", record.sha256), record))
            .collect::<HashMap<_, _>>();
        let mut verified = Vec::with_capacity(pending.len());
        let mut cursor = None;
        for _ in 0..MEDIA_OBJECT_LIST_PAGE_BUDGET {
            let mut list = self
                .bucket
                .list()
                .prefix("media/")
                .limit(MEDIA_OBJECT_LIST_PAGE_LIMIT)
                .include(vec![Include::HttpMetadata]);
            if let Some(cursor) = cursor.take() {
                list = list.cursor(cursor);
            }
            let objects = list.execute().await.map_err(MediaAssetError::ObjectStore)?;
            for object in objects.objects() {
                let key = object.key();
                let Some(record) = pending.remove(&key) else {
                    continue;
                };
                let metadata = object.http_metadata();
                if object.size() != record.size_bytes
                    || metadata.content_type.as_deref() != Some(record.mime.as_str())
                {
                    return Err(MediaAssetError::IntegrityFailure);
                }
                verified.push(record);
            }
            if pending.is_empty() || !objects.truncated() {
                verified.sort_by(|left, right| left.id.cmp(&right.id));
                return Ok(verified);
            }
            cursor = objects.cursor();
            if cursor.is_none() {
                return Err(MediaAssetError::IntegrityFailure);
            }
        }
        Err(MediaAssetError::IntegrityFailure)
    }

    pub async fn upload(
        &self,
        declared_mime: &str,
        bytes: &[u8],
    ) -> Result<MediaAssetRecord, MediaAssetError> {
        if bytes.is_empty() {
            return Err(MediaAssetError::InvalidUpload);
        }
        if bytes.len() > MAX_MEDIA_ASSET_BYTES {
            return Err(MediaAssetError::UploadTooLarge);
        }
        let Some(mime) = validate_supported_image(bytes) else {
            return Err(MediaAssetError::InvalidUpload);
        };
        if declared_mime != mime {
            return Err(MediaAssetError::InvalidUpload);
        }

        let sha256 = sha256_hex(bytes);
        let id_value = D1Type::Text(&sha256);
        let attempt_token = uuid::Uuid::new_v4().simple().to_string();
        let existing = self.find_any(&sha256).await?;
        match existing {
            Some(asset) => {
                if asset.sha256.as_deref() != Some(&sha256)
                    || asset.mime != mime
                    || asset.size_bytes != bytes.len() as u64
                {
                    return Err(MediaAssetError::Conflict);
                }
                match asset.status.as_str() {
                    "ready" => return asset.public_record(),
                    "pending" => {
                        let token = D1Type::Text(&attempt_token);
                        let lease = D1Type::Integer(MEDIA_UPLOAD_LEASE_SECONDS);
                        let result = self
                            .database
                            .prepare(
                                "UPDATE assets SET upload_token = ?, upload_started_at = unixepoch() WHERE id = ? AND status = 'pending' AND COALESCE(upload_started_at, 0) <= unixepoch() - ?",
                            )
                            .bind_refs([&token, &id_value, &lease])
                            .map_err(MediaAssetError::Database)?
                            .run()
                            .await
                            .map_err(MediaAssetError::Database)?;
                        if result
                            .meta()
                            .map_err(MediaAssetError::Database)?
                            .and_then(|meta| meta.changes)
                            != Some(1)
                        {
                            let current = self
                                .find_any(&sha256)
                                .await?
                                .ok_or(MediaAssetError::IntegrityFailure)?;
                            if current.status == "ready" {
                                return current.public_record();
                            }
                            let token = current.upload_token.as_deref();
                            if token != Some(&attempt_token) {
                                return Err(MediaAssetError::Conflict);
                            }
                        }
                    }
                    _ => return Err(MediaAssetError::IntegrityFailure),
                }
            }
            None => {
                let key = format!("media/{sha256}");
                let key_value = D1Type::Text(&key);
                let mime_value = D1Type::Text(mime);
                let size_value = D1Type::Integer(
                    i32::try_from(bytes.len()).map_err(|_| MediaAssetError::UploadTooLarge)?,
                );
                let digest_value = D1Type::Text(&sha256);
                let token_value = D1Type::Text(&attempt_token);
                if let Err(error) = self
                .database
                .prepare(
                    "INSERT OR IGNORE INTO assets (id, r2_key, mime, size_bytes, sha256, status, upload_token, upload_started_at) VALUES (?, ?, ?, ?, ?, 'pending', ?, unixepoch())",
                )
                .bind_refs([
                    &id_value,
                    &key_value,
                    &mime_value,
                    &size_value,
                    &digest_value,
                    &token_value,
                ])
                .map_err(MediaAssetError::Database)?
                .run()
                .await
            {
                if self.quota_is_full(bytes.len() as u64).await.unwrap_or(false) {
                    return Err(MediaAssetError::QuotaExceeded);
                }
                return Err(MediaAssetError::Database(error));
                }
                let inserted = self
                    .find_any(&sha256)
                    .await?
                    .ok_or(MediaAssetError::IntegrityFailure)?;
                if inserted.sha256.as_deref() != Some(&sha256)
                    || inserted.mime != mime
                    || inserted.size_bytes != bytes.len() as u64
                {
                    return Err(MediaAssetError::Conflict);
                }
                if inserted.status != "pending"
                    || inserted.upload_token.as_deref() != Some(&attempt_token)
                {
                    return inserted.public_record().map_err(|error| match error {
                        MediaAssetError::NotFound => MediaAssetError::Conflict,
                        other => other,
                    });
                }
            }
        }

        let r2_key = format!("media/{sha256}");
        if let Err(error) = self
            .bucket
            .put(r2_key, bytes.to_vec())
            .http_metadata(HttpMetadata {
                content_type: Some(mime.to_owned()),
                cache_control: Some(R2_CACHE_CONTROL.to_owned()),
                ..HttpMetadata::default()
            })
            .execute()
            .await
        {
            self.delete_pending_upload(&sha256, &attempt_token).await?;
            return Err(MediaAssetError::ObjectStore(error));
        }

        let ready_status = D1Type::Text("ready");
        let digest_value = D1Type::Text(&sha256);
        let no_token = D1Type::Null;
        let token_value = D1Type::Text(&attempt_token);
        let ready = self.database
            .prepare("UPDATE assets SET status = ?, upload_token = ?, upload_started_at = NULL WHERE id = ? AND sha256 = ? AND status = 'pending' AND upload_token = ?")
            .bind_refs([&ready_status, &no_token, &id_value, &digest_value, &token_value])
            .map_err(MediaAssetError::Database)?
            .run()
            .await
            .map_err(MediaAssetError::Database)?;
        if ready
            .meta()
            .map_err(MediaAssetError::Database)?
            .and_then(|meta| meta.changes)
            != Some(1)
        {
            return self
                .find_ready(&sha256)
                .await?
                .ok_or(MediaAssetError::Conflict);
        }

        self.find_ready(&sha256)
            .await?
            .ok_or(MediaAssetError::IntegrityFailure)
    }

    pub async fn read(&self, asset_id: &str) -> Result<MediaAssetContent, MediaAssetError> {
        let row = self
            .find_ready_row(asset_id)
            .await?
            .ok_or(MediaAssetError::NotFound)?;
        if row.id != asset_id {
            return Err(MediaAssetError::NotFound);
        }
        let r2_key = row.r2_key.clone();
        let record = row.public_record()?;
        if !is_lower_sha256(asset_id) || record.sha256 != asset_id {
            return Err(MediaAssetError::NotFound);
        }
        let object = self
            .bucket
            .get(r2_key)
            .execute()
            .await
            .map_err(MediaAssetError::ObjectStore)?
            .ok_or(MediaAssetError::NotFound)?;
        let body = object.body().ok_or(MediaAssetError::NotFound)?;
        let bytes = body.bytes().await.map_err(MediaAssetError::ObjectStore)?;
        if bytes.len() as u64 != record.size_bytes || sha256_hex(&bytes) != record.sha256 {
            return Err(MediaAssetError::IntegrityFailure);
        }
        Ok(MediaAssetContent { record, bytes })
    }

    async fn find_any(&self, asset_id: &str) -> Result<Option<MediaAssetRow>, MediaAssetError> {
        self.query_one(
            "SELECT id, r2_key, mime, size_bytes, sha256, status, created_at, upload_token, upload_started_at FROM assets WHERE id = ?",
            asset_id,
        )
        .await
    }

    async fn find_ready(
        &self,
        asset_id: &str,
    ) -> Result<Option<MediaAssetRecord>, MediaAssetError> {
        self.find_ready_row(asset_id)
            .await?
            .map(MediaAssetRow::public_record)
            .transpose()
    }

    async fn find_ready_row(
        &self,
        asset_id: &str,
    ) -> Result<Option<MediaAssetRow>, MediaAssetError> {
        self.query_one(
            "SELECT id, r2_key, mime, size_bytes, sha256, status, created_at, upload_token, upload_started_at FROM assets WHERE id = ? AND status = 'ready' AND sha256 IS NOT NULL",
            asset_id,
        )
        .await
    }

    async fn delete_pending_upload(
        &self,
        asset_id: &str,
        attempt_token: &str,
    ) -> Result<(), MediaAssetError> {
        let id = D1Type::Text(asset_id);
        let token = D1Type::Text(attempt_token);
        self.database
            .prepare("DELETE FROM assets WHERE id = ? AND status = 'pending' AND upload_token = ?")
            .bind_refs([&id, &token])
            .map_err(MediaAssetError::Database)?
            .run()
            .await
            .map_err(MediaAssetError::Database)?;
        Ok(())
    }

    async fn query_one(
        &self,
        query: &str,
        asset_id: &str,
    ) -> Result<Option<MediaAssetRow>, MediaAssetError> {
        let id = D1Type::Text(asset_id);
        let result = self
            .database
            .prepare(query)
            .bind_refs([&id])
            .map_err(MediaAssetError::Database)?
            .all()
            .await
            .map_err(MediaAssetError::Database)?;
        Ok(result
            .results::<MediaAssetRow>()
            .map_err(MediaAssetError::Database)?
            .into_iter()
            .next())
    }

    async fn quota_is_full(&self, next_bytes: u64) -> Result<bool, MediaAssetError> {
        #[derive(Deserialize)]
        struct Quota {
            asset_count: u64,
            total_bytes: u64,
        }
        let result = self
            .database
            .prepare(
                "SELECT COUNT(*) AS asset_count, COALESCE(SUM(size_bytes), 0) AS total_bytes FROM assets",
            )
            .all()
            .await
            .map_err(MediaAssetError::Database)?;
        let quota = result
            .results::<Quota>()
            .map_err(MediaAssetError::Database)?
            .into_iter()
            .next()
            .ok_or(MediaAssetError::IntegrityFailure)?;
        Ok(quota.asset_count >= MAX_MEDIA_ASSET_COUNT
            || quota.total_bytes.saturating_add(next_bytes) > MAX_MEDIA_LIBRARY_BYTES)
    }
}

pub fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
            output
        })
}

pub fn validate_supported_image(bytes: &[u8]) -> Option<&'static str> {
    let (mime, format) = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        ("image/png", ImageFormat::Png)
    } else if bytes.starts_with(b"\xff\xd8\xff") && bytes.ends_with(b"\xff\xd9") {
        ("image/jpeg", ImageFormat::Jpeg)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        ("image/webp", ImageFormat::WebP)
    } else {
        return None;
    };

    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOCATION);
    reader.limits(limits);
    reader.decode().ok().map(|_| mime)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::DynamicImage;

    fn encoded_image(format: ImageFormat) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::new_rgb8(2, 2)
            .write_to(&mut bytes, format)
            .expect("valid image can be encoded");
        bytes.into_inner()
    }

    #[test]
    fn decodes_only_valid_supported_image_files() {
        assert_eq!(
            validate_supported_image(&encoded_image(ImageFormat::Png)),
            Some("image/png")
        );
        assert_eq!(
            validate_supported_image(&encoded_image(ImageFormat::Jpeg)),
            Some("image/jpeg")
        );
        assert_eq!(
            validate_supported_image(&encoded_image(ImageFormat::WebP)),
            Some("image/webp")
        );
        assert_eq!(validate_supported_image(b"\x89PNG\r\n\x1a\n"), None);
        assert_eq!(validate_supported_image(b"\xff\xd8\xffjpeg\xff\xd9"), None);
        assert_eq!(validate_supported_image(b"RIFFxxxxWEBPdata"), None);
        assert_eq!(validate_supported_image(b"<svg></svg>"), None);
        assert_eq!(validate_supported_image(b"not an image"), None);
    }

    #[test]
    fn rejects_images_that_exceed_decode_dimension_limits() {
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::new_rgb8(MAX_IMAGE_DIMENSION + 1, 1)
            .write_to(&mut bytes, ImageFormat::Png)
            .expect("valid image can be encoded");
        assert_eq!(validate_supported_image(bytes.get_ref()), None);
    }

    #[test]
    fn content_ids_are_lowercase_sha256_and_stable_for_same_bytes() {
        let first = sha256_hex(b"same image");
        let second = sha256_hex(b"same image");
        assert_eq!(first, second);
        assert!(is_lower_sha256(&first));
        assert!(!is_lower_sha256(&first.to_uppercase()));
    }
}
