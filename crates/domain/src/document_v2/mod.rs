mod edit;
mod model;
mod theme;
mod validate;

pub use edit::{Edit, EditOutcome, FocusTarget, IdKind, IdSource, NewBlock};
pub use model::{
    ActionV2, AssetId, BlockId, BlockKind, BlockV2, BlueprintId, DocumentError, DocumentV2,
    FieldId, FormField, FormId, FormResult, FormV2, ImageAlt, PageId, PageV2, RevisionIdV2,
    SectionId, SectionKind, SectionV2, StableDocumentId, V2FunnelId, ValidationRange,
};
pub use theme::{
    ButtonShape, ContentWidth, CornerScale, FontFamily, ResolvedThemeProfile, Shadow, SpacingScale,
    ThemeColors, ThemeOverrides, ThemePartialOverrides, ThemeResponsive, ThemeV2, TypeScale,
};

pub const STRUCTURAL_SCHEMA_VERSION_V2: u32 = 2;
pub const DOCUMENT_VERSION_V2: u32 = 2;
pub const MAX_RAW_DOCUMENT_BYTES: usize = 1_048_576;
pub const MAX_PERSISTED_DOCUMENT_BYTES: usize = 1_048_576;
pub const MAX_DOCUMENT_DEPTH: usize = 8;
