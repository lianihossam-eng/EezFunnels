use super::*;
#[cfg(feature = "browser")]
use crate::routes::RouteView;
use eez_domain::document_v2::{
    is_valid_page_slug, ActionV2, BlockV2, DocumentV2, Edit, FocusTarget, FormField, FormResult,
    IdKind, IdSource, NewBlock, SectionKind,
};
#[cfg(feature = "browser")]
use eez_domain::document_v2::{AssetId, BlockId, ImageAlt, PageId, SectionId};
#[cfg(any(feature = "browser", test))]
use eez_domain::document_v2::{
    ButtonShape, ContentWidth, CornerScale, FontFamily, ResolvedThemeProfile, Shadow, SpacingScale,
    ThemeColors, ThemeOverrides, TypeScale,
};
#[cfg(any(feature = "browser", test))]
use eez_domain::ThemePreset;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[cfg(feature = "browser")]
mod form_builder;
#[cfg(feature = "browser")]
use form_builder::V2FormEditor;
#[cfg(feature = "browser")]
use uuid::Uuid;

#[cfg(feature = "browser")]
use super::super::api::{api_request, error_message};
#[cfg(feature = "browser")]
use super::super::idempotency::{
    clear_idempotency_storage, prepare_idempotency_key, PreparedIdempotencyKey,
};

mod api;
mod drafts;
mod orchestration;
mod state;
mod ui;

#[cfg(feature = "browser")]
pub(super) use ui::V2FunnelDetail;
