mod edit;
mod model;
mod starter;
mod theme;
mod validate;

pub use edit::{Edit, EditOutcome, FocusTarget, IdKind, IdSource, NewBlock, NewFormField};
pub use model::{
    ActionV2, AssetId, BlockId, BlockKind, BlockV2, BlueprintId, ConsentCopy, DocumentError,
    DocumentV2, FieldId, FormField, FormId, FormPresentation, FormResult, FormSubmissionError,
    FormSubmissionIssue, FormSubmissionIssueCode, FormSubmissionIssueTarget, FormSubmissionValue,
    FormV2, ImageAlt, PageId, PageV2, PublicLanguage, RevisionIdV2, SectionId, SectionKind,
    SectionV2, StableDocumentId, TextPresentation, V2FunnelId, ValidatedFormSubmission,
    ValidationRange,
};
pub use theme::{
    ButtonShape, ContentWidth, CornerScale, FontFamily, ResolvedThemeProfile, Shadow, SpacingScale,
    ThemeColors, ThemeOverrides, ThemePartialOverrides, ThemeResponsive, ThemeV2, TypeScale,
};
pub use validate::{canonical_information_url, is_valid_page_slug, validate_form_submission};

pub const STRUCTURAL_SCHEMA_VERSION_V2: u32 = 2;
pub const DOCUMENT_VERSION_V2: u32 = 2;
pub const MAX_RAW_DOCUMENT_BYTES: usize = 1_048_576;
pub const MAX_PERSISTED_DOCUMENT_BYTES: usize = 1_048_576;
pub const MAX_DOCUMENT_DEPTH: usize = 8;

pub const MAX_PAGE_SEO_TITLE_SCALARS: usize = 120;
pub const MAX_PAGE_SEO_DESCRIPTION_SCALARS: usize = 320;
