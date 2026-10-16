use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::model::deserialize_optional_non_null;
use crate::ThemePreset;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeV2 {
    pub(crate) preset_id: ThemePreset,
    pub(crate) overrides: ThemeOverrides,
}

impl ThemeV2 {
    pub fn preset_id(&self) -> ThemePreset {
        self.preset_id
    }

    pub fn overrides(&self) -> &ThemeOverrides {
        &self.overrides
    }

    pub fn resolve(&self) -> ResolvedThemeProfile {
        let mut desktop = preset_tokens(self.preset_id);
        apply_overrides(&mut desktop, &self.overrides.partial());

        let mut tablet = desktop.clone();
        if let Some(responsive) = &self.overrides.responsive {
            if let Some(overrides) = &responsive.tablet {
                apply_overrides(&mut tablet, overrides);
            }
        }

        let mut mobile = desktop.clone();
        if let Some(responsive) = &self.overrides.responsive {
            if let Some(overrides) = &responsive.mobile {
                apply_overrides(&mut mobile, overrides);
            }
        }

        ResolvedThemeProfile {
            profile_version: "theme-profile-v1".to_owned(),
            desktop,
            tablet,
            mobile,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeOverrides {
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub colors: Option<ThemeColors>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub type_scale: Option<TypeScale>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub font_family: Option<FontFamily>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub spacing_scale: Option<SpacingScale>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub corner_scale: Option<CornerScale>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub content_width: Option<ContentWidth>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub button_shape: Option<ButtonShape>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub shadow: Option<Shadow>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub responsive: Option<ThemeResponsive>,
}

impl ThemeOverrides {
    pub fn responsive(&self) -> Option<&ThemeResponsive> {
        self.responsive.as_ref()
    }

    fn partial(&self) -> ThemePartialOverrides {
        ThemePartialOverrides {
            colors: self.colors.clone(),
            type_scale: self.type_scale,
            font_family: self.font_family,
            spacing_scale: self.spacing_scale,
            corner_scale: self.corner_scale,
            content_width: self.content_width,
            button_shape: self.button_shape,
            shadow: self.shadow,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ThemePartialOverrides {
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub colors: Option<ThemeColors>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub type_scale: Option<TypeScale>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub font_family: Option<FontFamily>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub spacing_scale: Option<SpacingScale>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub corner_scale: Option<CornerScale>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub content_width: Option<ContentWidth>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub button_shape: Option<ButtonShape>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub shadow: Option<Shadow>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeColors {
    #[serde(
        rename = "color-background",
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub color_background: Option<String>,
    #[serde(
        rename = "color-border",
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub color_border: Option<String>,
    #[serde(
        rename = "color-muted",
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub color_muted: Option<String>,
    #[serde(
        rename = "color-on-primary",
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub color_on_primary: Option<String>,
    #[serde(
        rename = "color-primary",
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub color_primary: Option<String>,
    #[serde(
        rename = "color-surface",
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub color_surface: Option<String>,
    #[serde(
        rename = "color-text",
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub color_text: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeResponsive {
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub tablet: Option<ThemePartialOverrides>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_null",
        skip_serializing_if = "Option::is_none"
    )]
    pub mobile: Option<ThemePartialOverrides>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeScale {
    Compact,
    Default,
    Large,
    Display,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FontFamily {
    SystemUi,
    Arial,
    Georgia,
    Verdana,
    TrebuchetMs,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpacingScale {
    Compact,
    Default,
    Relaxed,
    Airy,
    Spacious,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CornerScale {
    Square,
    Soft,
    Rounded,
    Large,
    ExtraLarge,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentWidth {
    Narrow,
    Default,
    Wide,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonShape {
    Square,
    Rounded,
    Soft,
    Pill,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Shadow {
    None,
    Subtle,
    Raised,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedThemeProfile {
    pub profile_version: String,
    pub desktop: BTreeMap<String, String>,
    pub tablet: BTreeMap<String, String>,
    pub mobile: BTreeMap<String, String>,
}

fn preset_tokens(preset: ThemePreset) -> BTreeMap<String, String> {
    let (
        background,
        border,
        muted,
        on_primary,
        primary,
        surface,
        text,
        font,
        font_scale,
        button_radius,
        card_radius,
        shadow,
        spacing,
    ) = match preset {
        ThemePreset::Minimal => (
            "#f8fafc",
            "#e2e8f0",
            "#64748b",
            "#ffffff",
            "#334155",
            "#ffffff",
            "#1f2937",
            "system-ui, sans-serif",
            "1",
            "0.375rem",
            "0.75rem",
            "0 1px 3px rgba(15, 23, 42, 0.08)",
            "4rem",
        ),
        ThemePreset::Bold => (
            "#111113",
            "#3f3f46",
            "#a1a1aa",
            "#1c1917",
            "#f97316",
            "#18181b",
            "#fafafa",
            "Arial, sans-serif",
            "1.08",
            "0",
            "0",
            "0 8px 32px rgba(0, 0, 0, 0.32)",
            "6rem",
        ),
        ThemePreset::Elegant => (
            "#faf7f2",
            "#e8ded1",
            "#7c6f64",
            "#fffaf5",
            "#8b5e4c",
            "#fffdf9",
            "#3b3028",
            "Georgia, serif",
            "1.04",
            "0.75rem",
            "1rem",
            "0 4px 20px rgba(59, 48, 40, 0.08)",
            "5rem",
        ),
        ThemePreset::Playful => (
            "#fff7fd",
            "#f0d8fb",
            "#715a81",
            "#ffffff",
            "#d946ef",
            "#ffffff",
            "#38204d",
            "system-ui, sans-serif",
            "1.05",
            "1.5rem",
            "1.5rem",
            "0 8px 24px rgba(192, 38, 211, 0.18)",
            "4.5rem",
        ),
        ThemePreset::Corporate => (
            "#f1f5f9",
            "#cbd5e1",
            "#475569",
            "#ffffff",
            "#2563eb",
            "#ffffff",
            "#0f172a",
            "Arial, sans-serif",
            "1",
            "0.375rem",
            "0.375rem",
            "0 2px 8px rgba(15, 23, 42, 0.08)",
            "4rem",
        ),
    };
    BTreeMap::from([
        ("color-background".to_owned(), background.to_owned()),
        ("color-border".to_owned(), border.to_owned()),
        ("color-muted".to_owned(), muted.to_owned()),
        ("color-on-primary".to_owned(), on_primary.to_owned()),
        ("color-primary".to_owned(), primary.to_owned()),
        ("color-surface".to_owned(), surface.to_owned()),
        ("color-text".to_owned(), text.to_owned()),
        ("content-width".to_owned(), "72rem".to_owned()),
        ("font-family".to_owned(), font.to_owned()),
        ("font-scale".to_owned(), font_scale.to_owned()),
        ("radius-button".to_owned(), button_radius.to_owned()),
        ("radius-card".to_owned(), card_radius.to_owned()),
        ("shadow-card".to_owned(), shadow.to_owned()),
        ("spacing-section".to_owned(), spacing.to_owned()),
    ])
}

fn apply_overrides(tokens: &mut BTreeMap<String, String>, overrides: &ThemePartialOverrides) {
    if let Some(colors) = &overrides.colors {
        set_if_some(tokens, "color-background", &colors.color_background);
        set_if_some(tokens, "color-border", &colors.color_border);
        set_if_some(tokens, "color-muted", &colors.color_muted);
        set_if_some(tokens, "color-on-primary", &colors.color_on_primary);
        set_if_some(tokens, "color-primary", &colors.color_primary);
        set_if_some(tokens, "color-surface", &colors.color_surface);
        set_if_some(tokens, "color-text", &colors.color_text);
    }
    if let Some(value) = overrides.type_scale {
        tokens.insert(
            "font-scale".to_owned(),
            match value {
                TypeScale::Compact => "0.875",
                TypeScale::Default => "1",
                TypeScale::Large => "1.125",
                TypeScale::Display => "1.25",
            }
            .to_owned(),
        );
    }
    if let Some(value) = overrides.font_family {
        tokens.insert(
            "font-family".to_owned(),
            match value {
                FontFamily::SystemUi => "system-ui, sans-serif",
                FontFamily::Arial => "Arial, sans-serif",
                FontFamily::Georgia => "Georgia, serif",
                FontFamily::Verdana => "Verdana, sans-serif",
                FontFamily::TrebuchetMs => "Trebuchet MS, sans-serif",
            }
            .to_owned(),
        );
    }
    if let Some(value) = overrides.spacing_scale {
        tokens.insert(
            "spacing-section".to_owned(),
            match value {
                SpacingScale::Compact => "3rem",
                SpacingScale::Default => "4rem",
                SpacingScale::Relaxed => "5rem",
                SpacingScale::Airy => "4.5rem",
                SpacingScale::Spacious => "6rem",
            }
            .to_owned(),
        );
    }
    if let Some(value) = overrides.corner_scale {
        tokens.insert(
            "radius-card".to_owned(),
            match value {
                CornerScale::Square => "0",
                CornerScale::Soft => "0.375rem",
                CornerScale::Rounded => "0.75rem",
                CornerScale::Large => "1rem",
                CornerScale::ExtraLarge => "1.5rem",
            }
            .to_owned(),
        );
    }
    if let Some(value) = overrides.content_width {
        tokens.insert(
            "content-width".to_owned(),
            match value {
                ContentWidth::Narrow => "56rem",
                ContentWidth::Default => "72rem",
                ContentWidth::Wide => "80rem",
            }
            .to_owned(),
        );
    }
    if let Some(value) = overrides.button_shape {
        tokens.insert(
            "radius-button".to_owned(),
            match value {
                ButtonShape::Square => "0",
                ButtonShape::Rounded => "0.375rem",
                ButtonShape::Soft => "0.75rem",
                ButtonShape::Pill => "9999px",
            }
            .to_owned(),
        );
    }
    if let Some(value) = overrides.shadow {
        tokens.insert(
            "shadow-card".to_owned(),
            match value {
                Shadow::None => "none",
                Shadow::Subtle => "0 1px 3px rgba(15, 23, 42, 0.08)",
                Shadow::Raised => "0 8px 32px rgba(0, 0, 0, 0.32)",
            }
            .to_owned(),
        );
    }
}

fn set_if_some(tokens: &mut BTreeMap<String, String>, key: &str, value: &Option<String>) {
    if let Some(value) = value {
        tokens.insert(key.to_owned(), value.clone());
    }
}
