use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::io::Cursor;

use eez_domain::document_v2::{AssetId, DocumentV2};
use eez_domain::media::{
    MediaAssetDescriptor, RasterFormat, ResolvedV2Assets, MAX_MEDIA_DIMENSION, MAX_MEDIA_PIXELS,
    MAX_NORMALIZED_MEDIA_BYTES,
};
use image::{GenericImageView, ImageDecoder, ImageEncoder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use worker::d1::D1Type;
use worker::{Bucket, Conditional, D1Database, Error, HttpMetadata};

pub const MAX_MEDIA_UPLOAD_BYTES: usize = 2 * 1024 * 1024;
const MEDIA_CONTROL_KEY: &str = "private/media-control/sequence-v1";
const MEDIA_OBJECT_PREFIX: &str = "private/media/v1/";
const MEDIA_CONTROL_FORMAT_VERSION: u8 = 1;
const MEDIA_CONTROL_RETRIES: usize = 16;
const MEDIA_LIST_PAGE_SIZE: u32 = 50;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaCapacity {
    byte_budget: u64,
    key_budget: u64,
}

impl MediaCapacity {
    pub fn new(byte_budget: u64, key_budget: u64) -> Option<Self> {
        (byte_budget > 0 && key_budget > 0).then_some(Self {
            byte_budget,
            key_budget,
        })
    }

    pub fn from_env(env: &worker::Env) -> Option<Self> {
        let byte_budget = env
            .var("MEDIA_R2_BYTE_BUDGET")
            .ok()?
            .to_string()
            .parse::<u64>()
            .ok()?;
        let key_budget = env
            .var("MEDIA_R2_KEY_BUDGET")
            .ok()?
            .to_string()
            .parse::<u64>()
            .ok()?;
        Self::new(byte_budget, key_budget)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct MediaControl {
    format_version: u8,
    sequence: u64,
    byte_budget: u64,
    bytes_spent: u64,
    key_budget: u64,
    keys_spent: u64,
}

impl MediaControl {
    fn initial(capacity: MediaCapacity) -> Self {
        Self {
            format_version: MEDIA_CONTROL_FORMAT_VERSION,
            sequence: 0,
            byte_budget: capacity.byte_budget,
            bytes_spent: 0,
            key_budget: capacity.key_budget,
            keys_spent: 0,
        }
    }

    fn validate(&self) -> bool {
        self.format_version == MEDIA_CONTROL_FORMAT_VERSION
            && self.byte_budget > 0
            && self.bytes_spent <= self.byte_budget
            && self.key_budget > 0
            && self.keys_spent <= self.key_budget
    }

    fn increase_capacity(&mut self, capacity: MediaCapacity) -> Result<bool, MediaError> {
        if capacity.byte_budget < self.byte_budget || capacity.key_budget < self.key_budget {
            return Err(MediaError::CapacityControlUnavailable);
        }
        let changed =
            capacity.byte_budget != self.byte_budget || capacity.key_budget != self.key_budget;
        self.byte_budget = capacity.byte_budget;
        self.key_budget = capacity.key_budget;
        Ok(changed)
    }

    fn charge(&mut self, bytes: u64, capacity: MediaCapacity) -> Result<(), MediaError> {
        self.increase_capacity(capacity)?;
        let next_bytes = self
            .bytes_spent
            .checked_add(bytes)
            .ok_or(MediaError::CapacityReached)?;
        let next_keys = self
            .keys_spent
            .checked_add(1)
            .ok_or(MediaError::CapacityReached)?;
        if bytes == 0 || next_bytes > self.byte_budget || next_keys > self.key_budget {
            return Err(MediaError::CapacityReached);
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or(MediaError::CapacityControlUnavailable)?;
        self.bytes_spent = next_bytes;
        self.keys_spent = next_keys;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MediaLibraryItem {
    pub descriptor: MediaAssetDescriptor,
    pub display_name: String,
    pub archived: bool,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MediaLibraryPage {
    pub items: Vec<MediaLibraryItem>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MediaCapacitySnapshot {
    pub enabled: bool,
    pub byte_budget: u64,
    pub bytes_spent: u64,
    pub byte_remaining: u64,
    pub key_budget: u64,
    pub keys_spent: u64,
    pub key_remaining: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaObject {
    pub descriptor: MediaAssetDescriptor,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaAccess {
    Owner,
    Published,
}

#[derive(Debug)]
pub enum MediaError {
    Database(Error),
    InvalidImage,
    InvalidAssetId,
    AssetMissing,
    AssetNotReady,
    AssetIntegrityMismatch,
    CapacityReached,
    CapacityControlUnavailable,
    StorageUnavailable,
}

impl MediaError {
    pub fn safe_kind(&self) -> &'static str {
        match self {
            Self::Database(_) => "database",
            Self::InvalidImage => "invalid_image",
            Self::InvalidAssetId => "invalid_asset_id",
            Self::AssetMissing => "asset_missing",
            Self::AssetNotReady => "asset_not_ready",
            Self::AssetIntegrityMismatch => "asset_integrity_mismatch",
            Self::CapacityReached => "capacity_reached",
            Self::CapacityControlUnavailable => "capacity_control_unavailable",
            Self::StorageUnavailable => "storage_unavailable",
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidImage | Self::InvalidAssetId => "validation/invalid_json",
            Self::AssetMissing => "not_found/revision",
            Self::AssetNotReady | Self::AssetIntegrityMismatch => "external/unavailable",
            Self::CapacityReached => "rate_limit/exceeded",
            Self::CapacityControlUnavailable | Self::StorageUnavailable | Self::Database(_) => {
                "external/unavailable"
            }
        }
    }
}

impl Display for MediaError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl From<Error> for MediaError {
    fn from(error: Error) -> Self {
        Self::Database(error)
    }
}

#[derive(Clone, Debug, Deserialize)]
struct AssetRow {
    id: String,
    r2_key: String,
    mime: String,
    size_bytes: i64,
    sha256: Option<String>,
    format: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    display_name: String,
    state: String,
    archived: i64,
    created_at: String,
}

pub struct MediaRepository<'a> {
    database: &'a D1Database,
    bucket: &'a Bucket,
    capacity: Option<MediaCapacity>,
}

impl<'a> MediaRepository<'a> {
    pub fn new(
        database: &'a D1Database,
        bucket: &'a Bucket,
        capacity: Option<MediaCapacity>,
    ) -> Self {
        Self {
            database,
            bucket,
            capacity,
        }
    }

    pub async fn initialize_capacity_control(&self) -> Result<(), MediaError> {
        let capacity = self
            .capacity
            .ok_or(MediaError::CapacityControlUnavailable)?;
        for _ in 0..MEDIA_CONTROL_RETRIES {
            if let Some((mut state, etag)) = self.read_control().await? {
                if !state.validate() {
                    return Err(MediaError::CapacityControlUnavailable);
                }
                if state.increase_capacity(capacity)? {
                    if self.write_control(&state, Some(etag)).await? {
                        return Ok(());
                    }
                    continue;
                }
                return Ok(());
            }
            let state = MediaControl::initial(capacity);
            if self.write_control(&state, None).await? {
                return Ok(());
            }
        }
        Err(MediaError::CapacityControlUnavailable)
    }

    pub async fn capacity_snapshot(&self) -> Result<MediaCapacitySnapshot, MediaError> {
        let Some(capacity) = self.capacity else {
            return Ok(MediaCapacitySnapshot {
                enabled: false,
                byte_budget: 0,
                bytes_spent: 0,
                byte_remaining: 0,
                key_budget: 0,
                keys_spent: 0,
                key_remaining: 0,
            });
        };
        let mut state = match self.read_control().await? {
            Some((state, _)) => state,
            None => MediaControl::initial(capacity),
        };
        if !state.validate() {
            return Err(MediaError::CapacityControlUnavailable);
        }
        state.increase_capacity(capacity)?;
        Ok(MediaCapacitySnapshot {
            enabled: true,
            byte_budget: state.byte_budget,
            bytes_spent: state.bytes_spent,
            byte_remaining: state.byte_budget - state.bytes_spent,
            key_budget: state.key_budget,
            keys_spent: state.keys_spent,
            key_remaining: state.key_budget - state.keys_spent,
        })
    }

    pub async fn upload(
        &self,
        source_bytes: &[u8],
        original_name: &str,
    ) -> Result<MediaLibraryItem, MediaError> {
        let normalized = normalize_image(source_bytes)?;
        let digest = sha256_hex(&normalized.bytes);
        let id = AssetId::parse(digest.clone()).map_err(|_| MediaError::InvalidAssetId)?;
        let descriptor = MediaAssetDescriptor::new(
            id,
            digest.clone(),
            normalized.format,
            u32::try_from(normalized.bytes.len()).map_err(|_| MediaError::InvalidImage)?,
            normalized.width,
            normalized.height,
        )
        .map_err(|_| MediaError::InvalidImage)?;
        let key = media_object_key(&digest);
        let display_name = sanitize_display_name(original_name, normalized.format);
        let mime = descriptor.format().mime_type().to_owned();
        let size_bytes =
            i32::try_from(descriptor.byte_length()).map_err(|_| MediaError::InvalidImage)?;
        let format = format_name(descriptor.format());
        let width = i32::try_from(descriptor.width()).map_err(|_| MediaError::InvalidImage)?;
        let height = i32::try_from(descriptor.height()).map_err(|_| MediaError::InvalidImage)?;
        let id_value = D1Type::Text(&digest);
        let key_value = D1Type::Text(&key);
        let mime_value = D1Type::Text(&mime);
        let size_value = D1Type::Integer(size_bytes);
        let sha_value = D1Type::Text(&digest);
        let format_value = D1Type::Text(format);
        let width_value = D1Type::Integer(width);
        let height_value = D1Type::Integer(height);
        let name_value = D1Type::Text(&display_name);

        self.database
            .prepare(
                "INSERT OR IGNORE INTO assets (id, r2_key, mime, size_bytes, sha256, format, width, height, display_name, state) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending')",
            )
            .bind_refs([
                &id_value,
                &key_value,
                &mime_value,
                &size_value,
                &sha_value,
                &format_value,
                &width_value,
                &height_value,
                &name_value,
            ])?
            .run()
            .await?;

        let row = self.load_asset_row(&digest).await?;
        ensure_row_matches(&row, &descriptor, &key)?;
        self.ensure_r2_object(&descriptor, &normalized.bytes)
            .await?;
        let digest_value = D1Type::Text(&digest);
        self.database
            .prepare("UPDATE assets SET state = 'ready' WHERE id = ? AND state = 'pending'")
            .bind_refs([&digest_value])?
            .run()
            .await?;
        let ready = self.load_asset_row(&digest).await?;
        Ok(MediaLibraryItem {
            descriptor: descriptor_from_row(&ready)?,
            display_name: ready.display_name,
            archived: ready.archived != 0,
            created_at: ready.created_at,
        })
    }

    pub async fn list(
        &self,
        archived: bool,
        cursor: Option<&str>,
    ) -> Result<MediaLibraryPage, MediaError> {
        if cursor.is_some_and(|cursor| !is_lower_hex_digest(cursor)) {
            return Err(MediaError::InvalidAssetId);
        }
        let limit = i32::try_from(MEDIA_LIST_PAGE_SIZE + 1).unwrap_or(51);
        let archived_value = D1Type::Boolean(archived);
        let limit_value = D1Type::Integer(limit);
        let rows = match cursor {
            Some(cursor) => {
                let cursor_value = D1Type::Text(cursor);
                self.database
                .prepare("SELECT id, r2_key, mime, size_bytes, sha256, format, width, height, display_name, state, archived, created_at FROM assets WHERE state = 'ready' AND archived = ? AND id < ? ORDER BY id DESC LIMIT ?")
                .bind_refs([&archived_value, &cursor_value, &limit_value])?
                .all()
                .await?
                .results::<AssetRow>()?
            }
            None => self.database
                .prepare("SELECT id, r2_key, mime, size_bytes, sha256, format, width, height, display_name, state, archived, created_at FROM assets WHERE state = 'ready' AND archived = ? ORDER BY id DESC LIMIT ?")
                .bind_refs([&archived_value, &limit_value])?
                .all()
                .await?
                .results::<AssetRow>()?,
        };
        let has_more = rows.len() > MEDIA_LIST_PAGE_SIZE as usize;
        let mut items = Vec::with_capacity(rows.len().min(MEDIA_LIST_PAGE_SIZE as usize));
        for row in rows.into_iter().take(MEDIA_LIST_PAGE_SIZE as usize) {
            items.push(MediaLibraryItem {
                descriptor: descriptor_from_row(&row)?,
                display_name: row.display_name,
                archived: row.archived != 0,
                created_at: row.created_at,
            });
        }
        let next_cursor = has_more
            .then(|| {
                items
                    .last()
                    .map(|item| item.descriptor.asset_id().as_str().to_owned())
            })
            .flatten();
        Ok(MediaLibraryPage { items, next_cursor })
    }

    pub async fn set_archived(&self, asset_id: &str, archived: bool) -> Result<(), MediaError> {
        if !is_lower_hex_digest(asset_id) {
            return Err(MediaError::InvalidAssetId);
        }
        let asset_id_value = D1Type::Text(asset_id);
        let archived = D1Type::Boolean(archived);
        let result = self
            .database
            .prepare("UPDATE assets SET archived = ? WHERE id = ? AND state = 'ready'")
            .bind_refs([&archived, &asset_id_value])?
            .run()
            .await?;
        let _ = result;
        let row = self.load_asset_row(asset_id).await?;
        if row.state != "ready" {
            return Err(MediaError::AssetMissing);
        }
        Ok(())
    }

    pub async fn resolve_document(
        &self,
        document: &DocumentV2,
    ) -> Result<ResolvedV2Assets, MediaError> {
        let mut descriptors = Vec::with_capacity(document.referenced_asset_ids().len());
        for asset_id in document.referenced_asset_ids() {
            let row = self.load_asset_row(asset_id.as_str()).await?;
            if row.state != "ready" {
                return Err(MediaError::AssetNotReady);
            }
            descriptors.push(descriptor_from_row(&row)?);
        }
        ResolvedV2Assets::for_document(document, descriptors).map_err(|_| MediaError::AssetMissing)
    }

    pub async fn verify_assets(&self, assets: &ResolvedV2Assets) -> Result<(), MediaError> {
        for descriptor in assets.iter() {
            let object = self
                .read(descriptor.asset_id().as_str(), MediaAccess::Owner)
                .await?;
            if object.descriptor != *descriptor {
                return Err(MediaError::AssetIntegrityMismatch);
            }
        }
        Ok(())
    }

    pub async fn read(
        &self,
        asset_id: &str,
        access: MediaAccess,
    ) -> Result<MediaObject, MediaError> {
        if !is_lower_hex_digest(asset_id) {
            return Err(MediaError::AssetMissing);
        }
        let row = self.load_asset_row(asset_id).await?;
        if row.state != "ready" {
            return Err(MediaError::AssetMissing);
        }
        if access == MediaAccess::Published {
            let referenced = self
                .database
                .prepare("SELECT 1 AS referenced FROM revision_asset_refs refs JOIN revisions ON revisions.id = refs.revision_id JOIN funnels ON funnels.id = revisions.funnel_id AND funnels.published_revision_id = revisions.id WHERE refs.asset_id = ? LIMIT 1")
                .bind_refs([&D1Type::Text(asset_id)])?
                .first::<i64>(Some("referenced"))
                .await?
                .is_some();
            if !referenced {
                return Err(MediaError::AssetMissing);
            }
        }
        let descriptor = descriptor_from_row(&row)?;
        let object = self
            .bucket
            .get(row.r2_key)
            .execute()
            .await
            .map_err(|_| MediaError::StorageUnavailable)?
            .ok_or(MediaError::AssetMissing)?;
        if object.size() != u64::from(descriptor.byte_length()) {
            return Err(MediaError::AssetIntegrityMismatch);
        }
        let bytes = object
            .body()
            .ok_or(MediaError::StorageUnavailable)?
            .bytes()
            .await
            .map_err(|_| MediaError::StorageUnavailable)?;
        if sha256_hex(&bytes) != descriptor.sha256() {
            return Err(MediaError::AssetIntegrityMismatch);
        }
        Ok(MediaObject { descriptor, bytes })
    }

    async fn ensure_r2_object(
        &self,
        descriptor: &MediaAssetDescriptor,
        bytes: &[u8],
    ) -> Result<(), MediaError> {
        let key = media_object_key(descriptor.sha256());
        if let Some(object) = self.read_r2_object(&key).await? {
            return verify_object(object, descriptor, bytes);
        }
        let capacity = self
            .capacity
            .ok_or(MediaError::CapacityControlUnavailable)?;
        self.charge_capacity(bytes.len() as u64, capacity).await?;
        let checksum = Sha256::digest(bytes).to_vec();
        let metadata = HttpMetadata {
            content_type: Some(descriptor.format().mime_type().to_owned()),
            cache_control: Some("public, max-age=31536000, immutable".to_owned()),
            ..HttpMetadata::default()
        };
        let mut custom = HashMap::new();
        custom.insert("sha256".to_owned(), descriptor.sha256().to_owned());
        custom.insert(
            "format".to_owned(),
            format_name(descriptor.format()).to_owned(),
        );
        let stored = self
            .bucket
            .put(key.clone(), bytes.to_vec())
            .http_metadata(metadata)
            .custom_metadata(custom)
            .sha256(checksum)
            .only_if(Conditional {
                etag_does_not_match: Some("*".to_owned()),
                ..Conditional::default()
            })
            .execute()
            .await
            .map_err(|_| MediaError::StorageUnavailable)?;
        if stored.is_some() {
            return Ok(());
        }
        let object = self
            .read_r2_object(&key)
            .await?
            .ok_or(MediaError::StorageUnavailable)?;
        verify_object(object, descriptor, bytes)
    }

    async fn read_r2_object(&self, key: &str) -> Result<Option<Vec<u8>>, MediaError> {
        let Some(object) = self
            .bucket
            .get(key)
            .execute()
            .await
            .map_err(|_| MediaError::StorageUnavailable)?
        else {
            return Ok(None);
        };
        if object.size() > u64::from(MAX_NORMALIZED_MEDIA_BYTES) {
            return Err(MediaError::AssetIntegrityMismatch);
        }
        let bytes = object
            .body()
            .ok_or(MediaError::StorageUnavailable)?
            .bytes()
            .await
            .map_err(|_| MediaError::StorageUnavailable)?;
        Ok(Some(bytes))
    }

    async fn charge_capacity(&self, bytes: u64, capacity: MediaCapacity) -> Result<(), MediaError> {
        for _ in 0..MEDIA_CONTROL_RETRIES {
            let (mut state, etag) = self
                .read_control()
                .await?
                .ok_or(MediaError::CapacityControlUnavailable)?;
            if !state.validate() {
                return Err(MediaError::CapacityControlUnavailable);
            }
            state.charge(bytes, capacity)?;
            if self.write_control(&state, Some(etag)).await? {
                return Ok(());
            }
        }
        Err(MediaError::CapacityControlUnavailable)
    }

    async fn read_control(&self) -> Result<Option<(MediaControl, String)>, MediaError> {
        let object = self
            .bucket
            .get(MEDIA_CONTROL_KEY)
            .execute()
            .await
            .map_err(|_| MediaError::CapacityControlUnavailable)?;
        let Some(object) = object else {
            return Ok(None);
        };
        let etag = object.http_etag();
        if etag.is_empty() {
            return Err(MediaError::CapacityControlUnavailable);
        }
        let bytes = object
            .body()
            .ok_or(MediaError::CapacityControlUnavailable)?
            .bytes()
            .await
            .map_err(|_| MediaError::CapacityControlUnavailable)?;
        if bytes.len() > 4096 {
            return Err(MediaError::CapacityControlUnavailable);
        }
        let state = serde_json::from_slice::<MediaControl>(&bytes)
            .map_err(|_| MediaError::CapacityControlUnavailable)?;
        Ok(Some((state, etag)))
    }

    async fn write_control(
        &self,
        state: &MediaControl,
        expected_etag: Option<String>,
    ) -> Result<bool, MediaError> {
        let bytes =
            serde_json::to_vec(state).map_err(|_| MediaError::CapacityControlUnavailable)?;
        let (condition_name, condition_value) = match expected_etag {
            Some(etag) => ("If-Match", etag),
            None => ("If-None-Match", "*".to_owned()),
        };
        conditional_r2_put(
            self.bucket,
            MEDIA_CONTROL_KEY,
            &bytes,
            condition_name,
            &condition_value,
        )
        .await
        .map_err(|_| MediaError::CapacityControlUnavailable)
    }

    async fn load_asset_row(&self, id: &str) -> Result<AssetRow, MediaError> {
        let id = D1Type::Text(id);
        self.database
            .prepare("SELECT id, r2_key, mime, size_bytes, sha256, format, width, height, display_name, state, archived, created_at FROM assets WHERE id = ?")
            .bind_refs([&id])?
            .first::<AssetRow>(None)
            .await?
            .ok_or(MediaError::AssetMissing)
    }
}

struct NormalizedImage {
    bytes: Vec<u8>,
    format: RasterFormat,
    width: u32,
    height: u32,
}

fn normalize_image(bytes: &[u8]) -> Result<NormalizedImage, MediaError> {
    if bytes.is_empty() || bytes.len() > MAX_MEDIA_UPLOAD_BYTES {
        return Err(MediaError::InvalidImage);
    }
    let image_format = image::guess_format(bytes).map_err(|_| MediaError::InvalidImage)?;
    if !matches!(
        image_format,
        image::ImageFormat::Jpeg | image::ImageFormat::Png
    ) {
        return Err(MediaError::InvalidImage);
    }
    if image_format == image::ImageFormat::Png && has_apng_animation_chunk(bytes) {
        return Err(MediaError::InvalidImage);
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image_format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_MEDIA_DIMENSION);
    limits.max_image_height = Some(MAX_MEDIA_DIMENSION);
    limits.max_alloc = Some(MAX_MEDIA_PIXELS * 8);
    reader.limits(limits.clone());
    let mut decoder = reader
        .into_decoder()
        .map_err(|_| MediaError::InvalidImage)?;
    let orientation = decoder
        .orientation()
        .map_err(|_| MediaError::InvalidImage)?;
    decoder
        .set_limits(limits)
        .map_err(|_| MediaError::InvalidImage)?;
    let mut decoded =
        image::DynamicImage::from_decoder(decoder).map_err(|_| MediaError::InvalidImage)?;
    decoded.apply_orientation(orientation);
    let (width, height) = decoded.dimensions();
    if width == 0
        || height == 0
        || width > MAX_MEDIA_DIMENSION
        || height > MAX_MEDIA_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_MEDIA_PIXELS
    {
        return Err(MediaError::InvalidImage);
    }
    let (bytes, format) = match image_format {
        image::ImageFormat::Jpeg => {
            let rgb = decoded.to_rgb8();
            let mut encoded = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 86)
                .encode(rgb.as_raw(), width, height, image::ExtendedColorType::Rgb8)
                .map_err(|_| MediaError::InvalidImage)?;
            (encoded, RasterFormat::Jpeg)
        }
        image::ImageFormat::Png => {
            let rgba = decoded.to_rgba8();
            let mut encoded = Vec::new();
            image::codecs::png::PngEncoder::new_with_quality(
                &mut encoded,
                image::codecs::png::CompressionType::Fast,
                image::codecs::png::FilterType::Adaptive,
            )
            .write_image(
                rgba.as_raw(),
                width,
                height,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|_| MediaError::InvalidImage)?;
            (encoded, RasterFormat::Png)
        }
        _ => return Err(MediaError::InvalidImage),
    };
    if bytes.is_empty() || bytes.len() > MAX_NORMALIZED_MEDIA_BYTES as usize {
        return Err(MediaError::InvalidImage);
    }
    Ok(NormalizedImage {
        bytes,
        format,
        width,
        height,
    })
}

fn has_apng_animation_chunk(bytes: &[u8]) -> bool {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if !bytes.starts_with(PNG_SIGNATURE) {
        return false;
    }
    let mut offset = 8usize;
    while offset.checked_add(12).is_some_and(|end| end <= bytes.len()) {
        let length = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let Some(end) = offset
            .checked_add(12)
            .and_then(|start| start.checked_add(length))
        else {
            return false;
        };
        if end > bytes.len() {
            return false;
        }
        if &bytes[offset + 4..offset + 8] == b"acTL" {
            return true;
        }
        offset = end;
    }
    false
}

fn descriptor_from_row(row: &AssetRow) -> Result<MediaAssetDescriptor, MediaError> {
    if row.state != "ready" {
        return Err(MediaError::AssetNotReady);
    }
    let format = match row.format.as_deref() {
        Some("jpeg") if row.mime == "image/jpeg" => RasterFormat::Jpeg,
        Some("png") if row.mime == "image/png" => RasterFormat::Png,
        _ => return Err(MediaError::AssetIntegrityMismatch),
    };
    let id = AssetId::parse(row.id.clone()).map_err(|_| MediaError::AssetIntegrityMismatch)?;
    MediaAssetDescriptor::new(
        id,
        row.sha256
            .clone()
            .ok_or(MediaError::AssetIntegrityMismatch)?,
        format,
        u32::try_from(row.size_bytes).map_err(|_| MediaError::AssetIntegrityMismatch)?,
        u32::try_from(row.width.ok_or(MediaError::AssetIntegrityMismatch)?)
            .map_err(|_| MediaError::AssetIntegrityMismatch)?,
        u32::try_from(row.height.ok_or(MediaError::AssetIntegrityMismatch)?)
            .map_err(|_| MediaError::AssetIntegrityMismatch)?,
    )
    .map_err(|_| MediaError::AssetIntegrityMismatch)
}

fn ensure_row_matches(
    row: &AssetRow,
    expected: &MediaAssetDescriptor,
    expected_key: &str,
) -> Result<(), MediaError> {
    if row.r2_key != expected_key || descriptor_from_pending_row(row, expected)? != *expected {
        return Err(MediaError::AssetIntegrityMismatch);
    }
    Ok(())
}

fn descriptor_from_pending_row(
    row: &AssetRow,
    _expected: &MediaAssetDescriptor,
) -> Result<MediaAssetDescriptor, MediaError> {
    if row.state != "pending" && row.state != "ready" {
        return Err(MediaError::AssetIntegrityMismatch);
    }
    let format = match row.format.as_deref() {
        Some("jpeg") if row.mime == "image/jpeg" => RasterFormat::Jpeg,
        Some("png") if row.mime == "image/png" => RasterFormat::Png,
        _ => return Err(MediaError::AssetIntegrityMismatch),
    };
    MediaAssetDescriptor::new(
        AssetId::parse(row.id.clone()).map_err(|_| MediaError::AssetIntegrityMismatch)?,
        row.sha256
            .clone()
            .ok_or(MediaError::AssetIntegrityMismatch)?,
        format,
        u32::try_from(row.size_bytes).map_err(|_| MediaError::AssetIntegrityMismatch)?,
        u32::try_from(row.width.ok_or(MediaError::AssetIntegrityMismatch)?)
            .map_err(|_| MediaError::AssetIntegrityMismatch)?,
        u32::try_from(row.height.ok_or(MediaError::AssetIntegrityMismatch)?)
            .map_err(|_| MediaError::AssetIntegrityMismatch)?,
    )
    .map_err(|_| MediaError::AssetIntegrityMismatch)
}

fn verify_object(
    object: Vec<u8>,
    descriptor: &MediaAssetDescriptor,
    expected_bytes: &[u8],
) -> Result<(), MediaError> {
    if object.len() != descriptor.byte_length() as usize
        || sha256_hex(&object) != descriptor.sha256()
        || object != expected_bytes
    {
        return Err(MediaError::AssetIntegrityMismatch);
    }
    Ok(())
}

fn sanitize_display_name(original: &str, format: RasterFormat) -> String {
    let basename = original.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut sanitized = String::new();
    for character in basename.chars().filter(|character| !character.is_control()) {
        if sanitized.len() + character.len_utf8() > 120 {
            break;
        }
        sanitized.push(character);
    }
    let sanitized = sanitized.trim();
    if sanitized.is_empty() {
        format!("image.{}", format.extension())
    } else {
        sanitized.to_owned()
    }
}

fn media_object_key(sha256: &str) -> String {
    format!("{MEDIA_OBJECT_PREFIX}{sha256}")
}

fn format_name(format: RasterFormat) -> &'static str {
    match format {
        RasterFormat::Jpeg => "jpeg",
        RasterFormat::Png => "png",
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    encode_hex(&digest)
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn is_lower_hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

async fn conditional_r2_put(
    bucket: &Bucket,
    key: &str,
    bytes: &[u8],
    condition_name: &str,
    condition_value: &str,
) -> worker::Result<bool> {
    use worker::wasm_bindgen::{JsCast, JsValue};
    use worker::wasm_bindgen_futures::JsFuture;
    use worker::worker_sys::R2Bucket;

    let headers = worker::Headers::new();
    headers.set(condition_name, condition_value)?;
    let options = worker::js_sys::Object::new();
    worker::js_sys::Reflect::set(&options, &JsValue::from_str("onlyIf"), headers.0.as_ref())?;
    let array = worker::js_sys::Uint8Array::new_with_length(bytes.len() as u32);
    array.copy_from(bytes);
    let raw_bucket: R2Bucket = bucket.as_ref().clone().unchecked_into();
    let promise = raw_bucket.put(key.to_owned(), array.into(), options.into())?;
    let result = JsFuture::from(promise).await?;
    Ok(!result.is_null())
}

#[cfg(test)]
mod tests {
    use super::{
        encode_hex, has_apng_animation_chunk, is_lower_hex_digest, media_object_key,
        normalize_image, sanitize_display_name, MediaCapacity, MediaControl,
    };
    use image::ImageEncoder;

    #[test]
    fn media_control_charges_capacity_monotonically_and_without_refunds() {
        let initial_capacity = MediaCapacity::new(100, 2).unwrap();
        let mut control = MediaControl::initial(initial_capacity);
        control.charge(40, initial_capacity).unwrap();
        assert_eq!(control.bytes_spent, 40);
        assert_eq!(control.keys_spent, 1);
        assert!(control.charge(61, initial_capacity).is_err());
        assert!(control
            .increase_capacity(MediaCapacity::new(99, 2).unwrap())
            .is_err());
        control
            .increase_capacity(MediaCapacity::new(200, 3).unwrap())
            .unwrap();
        control
            .charge(60, MediaCapacity::new(200, 3).unwrap())
            .unwrap();
        assert_eq!(control.bytes_spent, 100);
        assert_eq!(control.keys_spent, 2);
        assert!(control.validate());
    }

    #[test]
    fn raster_normalization_reencodes_pixels_and_drops_animation_chunks() {
        let mut input = Vec::new();
        image::codecs::png::PngEncoder::new(&mut input)
            .write_image(&[255, 0, 0, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        let normalized = normalize_image(&input).unwrap();
        assert_eq!(normalized.format.mime_type(), "image/png");
        assert_eq!((normalized.width, normalized.height), (1, 1));
        assert_eq!(normalized.bytes, input);
        let mut animated = input;
        animated.splice(
            33..33,
            [0, 0, 0, 8, b'a', b'c', b'T', b'L', 0, 0, 0, 1, 0, 0, 0, 0],
        );
        assert!(has_apng_animation_chunk(&animated));
        assert!(normalize_image(&animated).is_err());
    }

    #[test]
    fn media_names_and_keys_never_control_storage_paths() {
        assert_eq!(
            sanitize_display_name("../../family.png", eez_domain::media::RasterFormat::Png),
            "family.png"
        );
        assert_eq!(
            sanitize_display_name("/", eez_domain::media::RasterFormat::Jpeg),
            "image.jpg"
        );
        let key = media_object_key(&"a".repeat(64));
        assert_eq!(key, format!("private/media/v1/{}", "a".repeat(64)));
        assert!(is_lower_hex_digest(&"f".repeat(64)));
        assert!(!is_lower_hex_digest(&"F".repeat(64)));
        assert_eq!(encode_hex(&[0, 15, 255]), "000fff");
    }
}
