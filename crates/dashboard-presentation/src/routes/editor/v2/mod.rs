use super::*;
#[cfg(feature = "browser")]
use crate::routes::RouteView;
use eez_domain::document_v2::{
    is_valid_page_slug, ActionV2, BlockV2, DocumentV2, Edit, FaqItem, FocusTarget, FormField,
    FormResult, IdKind, IdSource, NewBlock, OfferPriceV2, SectionKind, StepItem,
    MAX_FUNCTIONAL_BLOCK_ITEMS, SAMPLE_EMAIL_ADDRESS, SAMPLE_TELEPHONE_NUMBER,
};
#[cfg(feature = "browser")]
use eez_domain::document_v2::{
    AssetId, BlockId, CurrencyCode, EvidenceImage, ExactPriceAmount, ImageAlt, PricePeriod,
    MAX_EVIDENCE_CONTEXT_BYTES, MAX_EVIDENCE_IMAGE_ALT_BYTES, MAX_EVIDENCE_TEXT_BYTES,
    MAX_EVIDENCE_TITLE_BYTES, MAX_OFFER_COMPARISON_ITEMS,
};
#[cfg(any(feature = "browser", test))]
use eez_domain::document_v2::{
    ButtonShape, ContentWidth, CornerScale, FontFamily, ResolvedThemeProfile, Shadow, SpacingScale,
    ThemeColors, ThemeOverrides, TypeScale,
};
use eez_domain::document_v2::{EvidenceKind, EvidenceStatus, PublicLanguage};
#[cfg(any(feature = "browser", test))]
use eez_domain::ThemePreset;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[cfg(feature = "browser")]
mod evidence;
#[cfg(feature = "browser")]
mod form_builder;
#[cfg(feature = "browser")]
mod offers;
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

fn public_default_text(
    language: PublicLanguage,
    english: &'static str,
    french: &'static str,
) -> String {
    match language {
        PublicLanguage::English => english,
        PublicLanguage::French => french,
    }
    .to_owned()
}

fn new_faq_item(language: PublicLanguage) -> FaqItem {
    FaqItem {
        question: public_default_text(language, "Common question", "Question fréquente"),
        answer: public_default_text(
            language,
            "Write an answer for your visitors.",
            "Rédigez une réponse pour vos visiteurs.",
        ),
    }
}

fn new_step_item(language: PublicLanguage) -> StepItem {
    StepItem {
        title: public_default_text(language, "Step title", "Titre de l’étape"),
        detail: public_default_text(
            language,
            "Describe what happens in this step.",
            "Décrivez ce qui se passe à cette étape.",
        ),
    }
}

#[cfg(feature = "browser")]
pub(super) use ui::V2FunnelDetail;
