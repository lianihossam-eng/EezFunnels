use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::document_v2::{AssetId, DocumentV2};

pub const MAX_MEDIA_ASSETS_PER_DOCUMENT: usize = 32;
pub const MAX_MEDIA_DIMENSION: u32 = 2048;
pub const MAX_MEDIA_PIXELS: u64 = 4_194_304;
pub const MAX_NORMALIZED_MEDIA_BYTES: u32 = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RasterFormat {
    Jpeg,
    Png,
}

impl RasterFormat {
    pub fn mime_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
        }
    }
}

/// Immutable description of normalized bytes stored under a content-addressed asset ID.
/// Fields stay private so callers cannot construct a descriptor from unvalidated HTTP or SQL.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "MediaAssetDescriptorUnchecked")]
pub struct MediaAssetDescriptor {
    asset_id: AssetId,
    sha256: String,
    format: RasterFormat,
    byte_length: u32,
    width: u32,
    height: u32,
}

impl MediaAssetDescriptor {
    pub fn new(
        asset_id: AssetId,
        sha256: String,
        format: RasterFormat,
        byte_length: u32,
        width: u32,
        height: u32,
    ) -> Result<Self, MediaModelError> {
        let descriptor = Self {
            asset_id,
            sha256,
            format,
            byte_length,
            width,
            height,
        };
        descriptor.validate()?;
        Ok(descriptor)
    }

    pub fn asset_id(&self) -> &AssetId {
        &self.asset_id
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn format(&self) -> RasterFormat {
        self.format
    }

    pub fn byte_length(&self) -> u32 {
        self.byte_length
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn validate(&self) -> Result<(), MediaModelError> {
        let digest = self.sha256.as_bytes();
        if digest.len() != 64
            || !digest
                .iter()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
            || self.asset_id.as_str() != self.sha256
        {
            return Err(MediaModelError::InvalidDigest);
        }
        if self.width == 0
            || self.height == 0
            || self.width > MAX_MEDIA_DIMENSION
            || self.height > MAX_MEDIA_DIMENSION
            || u64::from(self.width) * u64::from(self.height) > MAX_MEDIA_PIXELS
        {
            return Err(MediaModelError::InvalidDimensions);
        }
        if self.byte_length == 0 || self.byte_length > MAX_NORMALIZED_MEDIA_BYTES {
            return Err(MediaModelError::InvalidByteLength);
        }
        Ok(())
    }
}

/// Exact validated asset set used to prepare or render one immutable V2 document revision.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResolvedV2Assets {
    entries: BTreeMap<AssetId, MediaAssetDescriptor>,
}

impl ResolvedV2Assets {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn for_document(
        document: &DocumentV2,
        descriptors: Vec<MediaAssetDescriptor>,
    ) -> Result<Self, MediaModelError> {
        let expected = document.referenced_asset_ids();
        if expected.len() > MAX_MEDIA_ASSETS_PER_DOCUMENT {
            return Err(MediaModelError::TooManyAssets);
        }
        if descriptors.len() != expected.len() {
            return Err(MediaModelError::AssetSetMismatch);
        }

        let mut entries = BTreeMap::new();
        let mut previous_id: Option<AssetId> = None;
        for descriptor in descriptors {
            descriptor.validate()?;
            let id = descriptor.asset_id().clone();
            if previous_id.as_ref().is_some_and(|previous| previous >= &id) {
                return Err(MediaModelError::AssetOrderInvalid);
            }
            previous_id = Some(id.clone());
            if entries.insert(id, descriptor).is_some() {
                return Err(MediaModelError::AssetSetMismatch);
            }
        }
        if entries.keys().ne(expected.iter()) {
            return Err(MediaModelError::AssetSetMismatch);
        }
        Ok(Self { entries })
    }

    pub fn get(&self, asset_id: &AssetId) -> Option<&MediaAssetDescriptor> {
        self.entries.get(asset_id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &MediaAssetDescriptor> {
        self.entries.values()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaModelError {
    InvalidDigest,
    InvalidDimensions,
    InvalidByteLength,
    AssetSetMismatch,
    AssetOrderInvalid,
    TooManyAssets,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MediaAssetDescriptorUnchecked {
    asset_id: AssetId,
    sha256: String,
    format: RasterFormat,
    byte_length: u32,
    width: u32,
    height: u32,
}

impl TryFrom<MediaAssetDescriptorUnchecked> for MediaAssetDescriptor {
    type Error = MediaModelError;

    fn try_from(unchecked: MediaAssetDescriptorUnchecked) -> Result<Self, Self::Error> {
        let descriptor = Self {
            asset_id: unchecked.asset_id,
            sha256: unchecked.sha256,
            format: unchecked.format,
            byte_length: unchecked.byte_length,
            width: unchecked.width,
            height: unchecked.height,
        };
        descriptor.validate()?;
        Ok(descriptor)
    }
}

impl Display for MediaModelError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let code = match self {
            Self::InvalidDigest => "validation/media_digest_invalid",
            Self::InvalidDimensions => "validation/media_dimensions_invalid",
            Self::InvalidByteLength => "validation/media_size_invalid",
            Self::AssetSetMismatch => "validation/v2_assets_missing",
            Self::AssetOrderInvalid => "validation/v2_asset_order_invalid",
            Self::TooManyAssets => "validation/v2_asset_limit",
        };
        formatter.write_str(code)
    }
}

impl std::error::Error for MediaModelError {}

#[cfg(test)]
mod tests {
    use super::{MediaAssetDescriptor, MAX_MEDIA_DIMENSION, MAX_NORMALIZED_MEDIA_BYTES};

    #[test]
    fn deserialization_rejects_invalid_media_descriptors() {
        let digest = "a".repeat(64);
        let mut descriptor = serde_json::json!({
            "asset_id": digest,
            "sha256": digest,
            "format": "png",
            "byte_length": 1,
            "width": 1,
            "height": 1
        });
        assert!(serde_json::from_value::<MediaAssetDescriptor>(descriptor.clone()).is_ok());

        descriptor["sha256"] = serde_json::Value::String("b".repeat(64));
        assert!(serde_json::from_value::<MediaAssetDescriptor>(descriptor.clone()).is_err());

        descriptor["sha256"] = serde_json::Value::String("a".repeat(64));
        descriptor["width"] = serde_json::json!(MAX_MEDIA_DIMENSION + 1);
        assert!(serde_json::from_value::<MediaAssetDescriptor>(descriptor.clone()).is_err());

        descriptor["width"] = serde_json::json!(1);
        descriptor["byte_length"] = serde_json::json!(MAX_NORMALIZED_MEDIA_BYTES + 1);
        assert!(serde_json::from_value::<MediaAssetDescriptor>(descriptor).is_err());
    }
}
