use std::collections::{BTreeMap, BTreeSet};

use eez_domain::{FunnelId, RenderManifest, Revision, Section, ThemePreset};
use eez_render::{content_config_hash, Renderer, SectionContent, ThemeTokens};
use serde::{Deserialize, Serialize};

use super::{
    FunnelCommandError, FunnelSnapshotV1, FunnelSnapshotV2, SnapshotData, ThemeOverrides,
    ValidatedSnapshot,
};

const MAX_CHANGES: usize = 128;
const MAX_FIELD_TEXT_BYTES: usize = 4_096;
const MAX_SETTINGS_CHANGES_BYTES: usize = 12 * 1_024;
const MAX_IMAGE_ALT_BYTES: usize = 1_024;
const MAX_URL_BYTES: usize = 2_048;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SettingChanges {
    schema_version: u16,
    pub(crate) changes: Vec<SettingChange>,
}

impl SettingChanges {
    pub(super) fn validate_envelope(&self) -> Result<(), FunnelCommandError> {
        if self.schema_version != 1 || self.changes.len() > MAX_CHANGES {
            return Err(FunnelCommandError::InvalidParameters);
        }
        if serde_json::to_vec(&self.changes)
            .map_err(|_| FunnelCommandError::Internal)?
            .len()
            > MAX_SETTINGS_CHANGES_BYTES
        {
            return Err(FunnelCommandError::InvalidParameters);
        }
        if self.changes.iter().any(|change| {
            change.id.is_empty()
                || change.id.len() > 160
                || !change
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        }) {
            return Err(FunnelCommandError::InvalidParameters);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SettingChange {
    id: String,
    value: SettingValue,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(super) enum SettingValue {
    Text(Option<String>),
    Color(Option<String>),
    Select(String),
    Toggle(bool),
    Image(Option<eez_render::HeroImage>),
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum ControlKind {
    Text,
    Color,
    Select,
    Toggle,
    Image,
}

#[derive(Serialize)]
struct InspectorSchema {
    version: u16,
    fields: Vec<FieldDescriptor>,
}

#[derive(Clone, Serialize)]
struct FieldDescriptor {
    id: String,
    label: String,
    control: ControlKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    section_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    options: Vec<SelectOption>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    multiline: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    optional: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    nullable: bool,
}

#[derive(Clone, Serialize)]
struct SelectOption {
    value: String,
    label: String,
}

#[derive(Clone)]
struct FieldSpec {
    descriptor: FieldDescriptor,
    value: SettingValue,
    binding: FieldBinding,
}

#[derive(Clone)]
enum FieldBinding {
    ThemePreset,
    PrimaryColor,
    SquareButtons,
    Section {
        section_id: String,
        field: SectionField,
        index: Option<usize>,
    },
}

struct TextFieldSpec<'a> {
    section_id: &'a str,
    collection: Option<&'a str>,
    index: Option<usize>,
    key: &'a str,
    label: &'a str,
    value: &'a str,
    field: SectionField,
    multiline: bool,
}

#[derive(Clone, Copy)]
enum SectionField {
    HeroTitle,
    HeroSubtitle,
    HeroCtaLabel,
    HeroCtaUrl,
    HeroImage,
    FeatureTitle,
    FeatureDescription,
    TestimonialQuote,
    TestimonialAuthor,
    TestimonialAttribution,
    CtaTitle,
    CtaBody,
    CtaLabel,
    CtaUrl,
    FooterLinkLabel,
    FooterLinkUrl,
    FooterCopyright,
}

pub(crate) fn inspector_projection(
    snapshot: &ValidatedSnapshot,
) -> Result<(serde_json::Value, serde_json::Value), FunnelCommandError> {
    let catalog = build_catalog(snapshot)?;
    let schema = InspectorSchema {
        version: 1,
        fields: catalog
            .iter()
            .map(|field| field.descriptor.clone())
            .collect(),
    };
    let effective_primary_color = theme_values(
        snapshot.snapshot.theme_id,
        &snapshot.snapshot.theme_overrides,
    )?
    .remove("color-primary")
    .ok_or(FunnelCommandError::Internal)?;
    let values = catalog
        .into_iter()
        .map(|field| {
            let value = if matches!(field.binding, FieldBinding::PrimaryColor) {
                serde_json::json!({
                    "kind": "color",
                    "value": snapshot.snapshot.theme_overrides.primary_color,
                    "effective_value": effective_primary_color,
                })
            } else {
                serde_json::to_value(field.value).map_err(|_| FunnelCommandError::Internal)?
            };
            Ok::<_, FunnelCommandError>((field.descriptor.id, value))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    Ok((
        serde_json::to_value(schema).map_err(|_| FunnelCommandError::Internal)?,
        serde_json::to_value(values).map_err(|_| FunnelCommandError::Internal)?,
    ))
}

fn build_catalog(snapshot: &ValidatedSnapshot) -> Result<Vec<FieldSpec>, FunnelCommandError> {
    let mut fields = Vec::new();
    let mut preset_options = ThemePreset::all()
        .iter()
        .map(|preset| SelectOption {
            value: preset.id().to_owned(),
            label: preset.name().to_owned(),
        })
        .collect::<Vec<_>>();
    preset_options.sort_by(|left, right| left.label.cmp(&right.label));
    push_field(
        &mut fields,
        FieldSpec {
            descriptor: FieldDescriptor {
                id: "theme.preset".to_owned(),
                label: "Theme preset".to_owned(),
                control: ControlKind::Select,
                section_id: None,
                options: preset_options,
                multiline: false,
                optional: false,
                nullable: false,
            },
            value: SettingValue::Select(snapshot.snapshot.theme_id.id().to_owned()),
            binding: FieldBinding::ThemePreset,
        },
    );

    let theme_values = theme_values(
        snapshot.snapshot.theme_id,
        &snapshot.snapshot.theme_overrides,
    )?;
    let primary_color = theme_values
        .get("color-primary")
        .cloned()
        .ok_or(FunnelCommandError::Internal)?;
    push_field(
        &mut fields,
        FieldSpec {
            descriptor: FieldDescriptor {
                id: "theme.primary_color".to_owned(),
                label: "Primary color".to_owned(),
                control: ControlKind::Color,
                section_id: None,
                options: Vec::new(),
                multiline: false,
                optional: false,
                nullable: true,
            },
            value: SettingValue::Color(Some(primary_color)),
            binding: FieldBinding::PrimaryColor,
        },
    );
    push_field(
        &mut fields,
        FieldSpec {
            descriptor: FieldDescriptor {
                id: "theme.square_buttons".to_owned(),
                label: "Square button corners".to_owned(),
                control: ControlKind::Toggle,
                section_id: None,
                options: Vec::new(),
                multiline: false,
                optional: false,
                nullable: false,
            },
            value: SettingValue::Toggle(snapshot.snapshot.theme_overrides.square_buttons),
            binding: FieldBinding::SquareButtons,
        },
    );

    for section in &snapshot.revision.sections().to_vec() {
        let section_id = section.id().as_str();
        let Some(content) = snapshot.contents.get(section_id) else {
            return Err(FunnelCommandError::Internal);
        };
        match content {
            SectionContent::Hero {
                title,
                subtitle,
                cta_label,
                cta_url,
                image,
            } => {
                push_text(
                    &mut fields,
                    TextFieldSpec {
                        section_id,
                        collection: None,
                        index: None,
                        key: "title",
                        label: "Headline",
                        value: title,
                        field: SectionField::HeroTitle,
                        multiline: false,
                    },
                );
                push_text(
                    &mut fields,
                    TextFieldSpec {
                        section_id,
                        collection: None,
                        index: None,
                        key: "subtitle",
                        label: "Subheading",
                        value: subtitle,
                        field: SectionField::HeroSubtitle,
                        multiline: true,
                    },
                );
                push_text(
                    &mut fields,
                    TextFieldSpec {
                        section_id,
                        collection: None,
                        index: None,
                        key: "cta_label",
                        label: "Button label",
                        value: cta_label,
                        field: SectionField::HeroCtaLabel,
                        multiline: false,
                    },
                );
                push_text(
                    &mut fields,
                    TextFieldSpec {
                        section_id,
                        collection: None,
                        index: None,
                        key: "cta_url",
                        label: "Button destination",
                        value: cta_url,
                        field: SectionField::HeroCtaUrl,
                        multiline: false,
                    },
                );
                push_field(
                    &mut fields,
                    FieldSpec {
                        descriptor: FieldDescriptor {
                            id: format!("section.{section_id}.image"),
                            label: "Hero image".to_owned(),
                            control: ControlKind::Image,
                            section_id: Some(section_id.to_owned()),
                            options: Vec::new(),
                            multiline: false,
                            optional: true,
                            nullable: true,
                        },
                        value: SettingValue::Image(image.clone()),
                        binding: FieldBinding::Section {
                            section_id: section_id.to_owned(),
                            field: SectionField::HeroImage,
                            index: None,
                        },
                    },
                );
            }
            SectionContent::Features { items } => {
                for (index, item) in items.iter().enumerate() {
                    push_text(
                        &mut fields,
                        TextFieldSpec {
                            section_id,
                            collection: Some("items"),
                            index: Some(index),
                            key: "title",
                            label: "Feature title",
                            value: &item.title,
                            field: SectionField::FeatureTitle,
                            multiline: true,
                        },
                    );
                    push_text(
                        &mut fields,
                        TextFieldSpec {
                            section_id,
                            collection: Some("items"),
                            index: Some(index),
                            key: "description",
                            label: "Feature description",
                            value: &item.description,
                            field: SectionField::FeatureDescription,
                            multiline: true,
                        },
                    );
                }
            }
            SectionContent::Testimonials { items } => {
                for (index, item) in items.iter().enumerate() {
                    push_text(
                        &mut fields,
                        TextFieldSpec {
                            section_id,
                            collection: Some("items"),
                            index: Some(index),
                            key: "quote",
                            label: "Quote",
                            value: &item.quote,
                            field: SectionField::TestimonialQuote,
                            multiline: true,
                        },
                    );
                    push_text(
                        &mut fields,
                        TextFieldSpec {
                            section_id,
                            collection: Some("items"),
                            index: Some(index),
                            key: "author",
                            label: "Author",
                            value: &item.author,
                            field: SectionField::TestimonialAuthor,
                            multiline: false,
                        },
                    );
                    push_field(
                        &mut fields,
                        FieldSpec {
                            descriptor: FieldDescriptor {
                                id: format!("section.{section_id}.items.{index}.attribution"),
                                label: "Attribution".to_owned(),
                                control: ControlKind::Text,
                                section_id: Some(section_id.to_owned()),
                                options: Vec::new(),
                                multiline: false,
                                optional: true,
                                nullable: true,
                            },
                            value: SettingValue::Text(item.attribution.clone()),
                            binding: FieldBinding::Section {
                                section_id: section_id.to_owned(),
                                field: SectionField::TestimonialAttribution,
                                index: Some(index),
                            },
                        },
                    );
                }
            }
            SectionContent::Cta {
                title,
                body,
                cta_label,
                cta_url,
            } => {
                push_text(
                    &mut fields,
                    TextFieldSpec {
                        section_id,
                        collection: None,
                        index: None,
                        key: "title",
                        label: "Heading",
                        value: title,
                        field: SectionField::CtaTitle,
                        multiline: false,
                    },
                );
                push_text(
                    &mut fields,
                    TextFieldSpec {
                        section_id,
                        collection: None,
                        index: None,
                        key: "body",
                        label: "Body",
                        value: body,
                        field: SectionField::CtaBody,
                        multiline: true,
                    },
                );
                push_text(
                    &mut fields,
                    TextFieldSpec {
                        section_id,
                        collection: None,
                        index: None,
                        key: "cta_label",
                        label: "Button label",
                        value: cta_label,
                        field: SectionField::CtaLabel,
                        multiline: false,
                    },
                );
                push_text(
                    &mut fields,
                    TextFieldSpec {
                        section_id,
                        collection: None,
                        index: None,
                        key: "cta_url",
                        label: "Button destination",
                        value: cta_url,
                        field: SectionField::CtaUrl,
                        multiline: false,
                    },
                );
            }
            SectionContent::Footer { links, copyright } => {
                for (index, link) in links.iter().enumerate() {
                    push_text(
                        &mut fields,
                        TextFieldSpec {
                            section_id,
                            collection: Some("links"),
                            index: Some(index),
                            key: "label",
                            label: "Link label",
                            value: &link.label,
                            field: SectionField::FooterLinkLabel,
                            multiline: false,
                        },
                    );
                    push_text(
                        &mut fields,
                        TextFieldSpec {
                            section_id,
                            collection: Some("links"),
                            index: Some(index),
                            key: "url",
                            label: "Link destination",
                            value: &link.url,
                            field: SectionField::FooterLinkUrl,
                            multiline: false,
                        },
                    );
                }
                push_text(
                    &mut fields,
                    TextFieldSpec {
                        section_id,
                        collection: None,
                        index: None,
                        key: "copyright",
                        label: "Copyright",
                        value: copyright,
                        field: SectionField::FooterCopyright,
                        multiline: false,
                    },
                );
            }
        }
    }
    Ok(fields)
}

fn push_text(fields: &mut Vec<FieldSpec>, spec: TextFieldSpec<'_>) {
    push_field(
        fields,
        FieldSpec {
            descriptor: FieldDescriptor {
                id: match (spec.collection, spec.index) {
                    (Some(collection), Some(index)) => {
                        format!(
                            "section.{}.{}.{index}.{}",
                            spec.section_id, collection, spec.key
                        )
                    }
                    _ => format!("section.{}.{}", spec.section_id, spec.key),
                },
                label: spec.label.to_owned(),
                control: ControlKind::Text,
                section_id: Some(spec.section_id.to_owned()),
                options: Vec::new(),
                multiline: spec.multiline,
                optional: false,
                nullable: false,
            },
            value: SettingValue::Text(Some(spec.value.to_owned())),
            binding: FieldBinding::Section {
                section_id: spec.section_id.to_owned(),
                field: spec.field,
                index: spec.index,
            },
        },
    );
}

fn push_field(fields: &mut Vec<FieldSpec>, spec: FieldSpec) {
    fields.push(spec);
}

pub(crate) fn apply_editor_changes(
    source: &ValidatedSnapshot,
    changes: &[SettingChange],
) -> Result<ValidatedSnapshot, FunnelCommandError> {
    if changes.len() > MAX_CHANGES {
        return Err(FunnelCommandError::InvalidParameters);
    }
    let catalog = build_catalog(source)?;
    let mut seen = BTreeSet::new();
    for change in changes {
        if !seen.insert(change.id.as_str()) {
            return Err(FunnelCommandError::InvalidParameters);
        }
        let spec = catalog
            .iter()
            .find(|spec| spec.descriptor.id == change.id)
            .ok_or(FunnelCommandError::InvalidParameters)?;
        validate_kind(spec, &change.value)?;
    }

    let mut edited = source.clone();
    for change in changes.iter().filter(|change| change.id == "theme.preset") {
        let SettingValue::Select(value) = &change.value else {
            return Err(FunnelCommandError::InvalidParameters);
        };
        edited.snapshot.theme_id =
            ThemePreset::from_id(value).ok_or(FunnelCommandError::InvalidParameters)?;
    }
    for change in changes {
        if change.id == "theme.preset" {
            continue;
        }
        let spec = catalog
            .iter()
            .find(|spec| spec.descriptor.id == change.id)
            .ok_or(FunnelCommandError::InvalidParameters)?;
        apply_binding(&mut edited, &spec.binding, &change.value)?;
    }

    if edited.snapshot.format_version == 2 || !edited.snapshot.theme_overrides.is_default() {
        edited.snapshot.format_version = 2;
    }

    let theme = effective_theme(edited.snapshot.theme_id, &edited.snapshot.theme_overrides)
        .map_err(|_| FunnelCommandError::InvalidParameters)?;
    let config_hash = content_config_hash(&edited.contents);
    let source_manifest = source.revision.render_manifest();
    let manifest = RenderManifest::new(
        source.revision.id().clone(),
        theme.version(),
        source_manifest.renderer_version(),
        source_manifest.assets_manifest_hash(),
        config_hash,
    )?;
    let sections = source
        .revision
        .sections()
        .iter()
        .map(|section| {
            Section::new(
                Some(section.id().as_str().to_owned()),
                section.section_type().as_str(),
                section.order(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let revision = Revision::new(
        source.revision.id().clone(),
        FunnelId::new(source.revision.funnel_id().as_str().to_owned())?,
        Some(i64::from(source.revision.schema_version().value())),
        sections,
        manifest,
    )?;
    Renderer::new()
        .render(&revision, &theme, &edited.contents)
        .map_err(|_| FunnelCommandError::InvalidParameters)?;
    edited.snapshot.sections = revision
        .sections()
        .iter()
        .map(super::SectionSnapshotV1::from)
        .collect();
    edited.snapshot.schema_version = 1;
    edited.snapshot.contents = edited.contents.clone();
    edited.theme = theme;
    edited.revision = revision;
    Ok(edited)
}

fn validate_kind(spec: &FieldSpec, value: &SettingValue) -> Result<(), FunnelCommandError> {
    let matches_control = matches!(
        (spec.descriptor.control, value),
        (ControlKind::Text, SettingValue::Text(_))
            | (ControlKind::Color, SettingValue::Color(_))
            | (ControlKind::Select, SettingValue::Select(_))
            | (ControlKind::Toggle, SettingValue::Toggle(_))
            | (ControlKind::Image, SettingValue::Image(_))
    );
    if !matches_control {
        return Err(FunnelCommandError::InvalidParameters);
    }
    match (&spec.binding, value) {
        (FieldBinding::ThemePreset, SettingValue::Select(value))
            if ThemePreset::from_id(value).is_some() =>
        {
            Ok(())
        }
        (FieldBinding::PrimaryColor, SettingValue::Color(Some(value))) if is_hex_color(value) => {
            Ok(())
        }
        (FieldBinding::PrimaryColor, SettingValue::Color(None)) => Ok(()),
        (FieldBinding::SquareButtons, SettingValue::Toggle(_)) => Ok(()),
        (
            FieldBinding::Section {
                field: SectionField::HeroImage,
                ..
            },
            SettingValue::Image(value),
        ) => {
            if value.as_ref().is_some_and(|image| {
                image.url.len() > MAX_URL_BYTES || image.alt.len() > MAX_IMAGE_ALT_BYTES
            }) {
                Err(FunnelCommandError::InvalidParameters)
            } else {
                Ok(())
            }
        }
        (
            FieldBinding::Section {
                field: SectionField::TestimonialAttribution,
                ..
            },
            SettingValue::Text(value),
        ) => {
            if value
                .as_ref()
                .is_some_and(|text| text.len() > MAX_FIELD_TEXT_BYTES)
            {
                Err(FunnelCommandError::InvalidParameters)
            } else {
                Ok(())
            }
        }
        (
            FieldBinding::Section {
                field: SectionField::HeroCtaUrl | SectionField::CtaUrl | SectionField::FooterLinkUrl,
                ..
            },
            SettingValue::Text(Some(value)),
        ) => {
            if value.len() > MAX_URL_BYTES {
                Err(FunnelCommandError::InvalidParameters)
            } else {
                Ok(())
            }
        }
        (FieldBinding::Section { .. }, SettingValue::Text(Some(value)))
            if value.len() <= MAX_FIELD_TEXT_BYTES =>
        {
            Ok(())
        }
        _ => Err(FunnelCommandError::InvalidParameters),
    }
}

fn apply_binding(
    snapshot: &mut ValidatedSnapshot,
    binding: &FieldBinding,
    value: &SettingValue,
) -> Result<(), FunnelCommandError> {
    match (binding, value) {
        (FieldBinding::PrimaryColor, SettingValue::Color(value)) => {
            snapshot.snapshot.theme_overrides.primary_color =
                value.as_ref().map(|value| value.to_ascii_lowercase());
        }
        (FieldBinding::SquareButtons, SettingValue::Toggle(value)) => {
            snapshot.snapshot.theme_overrides.square_buttons = *value;
        }
        (
            FieldBinding::Section {
                section_id,
                field,
                index,
            },
            value,
        ) => {
            let content = snapshot
                .contents
                .get_mut(section_id)
                .ok_or(FunnelCommandError::InvalidParameters)?;
            apply_section_field(content, *field, *index, value)?;
        }
        _ => return Err(FunnelCommandError::InvalidParameters),
    }
    Ok(())
}

fn apply_section_field(
    content: &mut SectionContent,
    field: SectionField,
    index: Option<usize>,
    value: &SettingValue,
) -> Result<(), FunnelCommandError> {
    match (content, field, value) {
        (
            SectionContent::Hero { title, .. },
            SectionField::HeroTitle,
            SettingValue::Text(Some(value)),
        ) => *title = value.clone(),
        (
            SectionContent::Hero { subtitle, .. },
            SectionField::HeroSubtitle,
            SettingValue::Text(Some(value)),
        ) => *subtitle = value.clone(),
        (
            SectionContent::Hero { cta_label, .. },
            SectionField::HeroCtaLabel,
            SettingValue::Text(Some(value)),
        ) => *cta_label = value.clone(),
        (
            SectionContent::Hero { cta_url, .. },
            SectionField::HeroCtaUrl,
            SettingValue::Text(Some(value)),
        ) => *cta_url = value.clone(),
        (
            SectionContent::Hero { image, .. },
            SectionField::HeroImage,
            SettingValue::Image(value),
        ) => *image = value.clone(),
        (
            SectionContent::Features { items },
            SectionField::FeatureTitle,
            SettingValue::Text(Some(value)),
        ) => indexed_item(items, index)?.title = value.clone(),
        (
            SectionContent::Features { items },
            SectionField::FeatureDescription,
            SettingValue::Text(Some(value)),
        ) => indexed_item(items, index)?.description = value.clone(),
        (
            SectionContent::Testimonials { items },
            SectionField::TestimonialQuote,
            SettingValue::Text(Some(value)),
        ) => indexed_item(items, index)?.quote = value.clone(),
        (
            SectionContent::Testimonials { items },
            SectionField::TestimonialAuthor,
            SettingValue::Text(Some(value)),
        ) => indexed_item(items, index)?.author = value.clone(),
        (
            SectionContent::Testimonials { items },
            SectionField::TestimonialAttribution,
            SettingValue::Text(value),
        ) => indexed_item(items, index)?.attribution = value.clone(),
        (
            SectionContent::Cta { title, .. },
            SectionField::CtaTitle,
            SettingValue::Text(Some(value)),
        ) => *title = value.clone(),
        (
            SectionContent::Cta { body, .. },
            SectionField::CtaBody,
            SettingValue::Text(Some(value)),
        ) => *body = value.clone(),
        (
            SectionContent::Cta { cta_label, .. },
            SectionField::CtaLabel,
            SettingValue::Text(Some(value)),
        ) => *cta_label = value.clone(),
        (
            SectionContent::Cta { cta_url, .. },
            SectionField::CtaUrl,
            SettingValue::Text(Some(value)),
        ) => *cta_url = value.clone(),
        (
            SectionContent::Footer { links, .. },
            SectionField::FooterLinkLabel,
            SettingValue::Text(Some(value)),
        ) => indexed_item(links, index)?.label = value.clone(),
        (
            SectionContent::Footer { links, .. },
            SectionField::FooterLinkUrl,
            SettingValue::Text(Some(value)),
        ) => indexed_item(links, index)?.url = value.clone(),
        (
            SectionContent::Footer { copyright, .. },
            SectionField::FooterCopyright,
            SettingValue::Text(Some(value)),
        ) => *copyright = value.clone(),
        _ => return Err(FunnelCommandError::InvalidParameters),
    }
    Ok(())
}

fn indexed_item<T>(items: &mut [T], index: Option<usize>) -> Result<&mut T, FunnelCommandError> {
    items
        .get_mut(index.ok_or(FunnelCommandError::InvalidParameters)?)
        .ok_or(FunnelCommandError::InvalidParameters)
}

pub(super) fn effective_theme(
    preset: ThemePreset,
    overrides: &ThemeOverrides,
) -> Result<ThemeTokens, eez_render::RenderError> {
    let mut values = preset
        .tokens()
        .iter()
        .map(|token| (token.name().to_owned(), token.value().to_owned()))
        .collect::<BTreeMap<_, _>>();
    if let Some(color) = &overrides.primary_color {
        values.insert("color-primary".to_owned(), color.clone());
    }
    if overrides.square_buttons {
        values.insert("radius-button".to_owned(), "0".to_owned());
    }
    ThemeTokens::new(values)
}

fn theme_values(
    preset: ThemePreset,
    overrides: &ThemeOverrides,
) -> Result<BTreeMap<String, String>, FunnelCommandError> {
    let mut values = preset
        .tokens()
        .iter()
        .map(|token| (token.name().to_owned(), token.value().to_owned()))
        .collect::<BTreeMap<_, _>>();
    if let Some(color) = &overrides.primary_color {
        values.insert("color-primary".to_owned(), color.clone());
    }
    if overrides.square_buttons {
        values.insert("radius-button".to_owned(), "0".to_owned());
    }
    Ok(values)
}

pub(super) fn is_canonical_hex_color(value: &str) -> bool {
    is_hex_color(value) && value == value.to_ascii_lowercase()
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn encode_snapshot(snapshot: &SnapshotData) -> Result<String, FunnelCommandError> {
    let encoded = if snapshot.format_version == 1 && snapshot.theme_overrides.is_default() {
        serde_json::to_string(&FunnelSnapshotV1 {
            format_version: 1,
            schema_version: snapshot.schema_version,
            blueprint: snapshot.blueprint,
            theme_id: snapshot.theme_id,
            sections: snapshot.sections.clone(),
            contents: snapshot.contents.clone(),
        })
    } else {
        serde_json::to_string(&FunnelSnapshotV2 {
            format_version: 2,
            schema_version: 1,
            blueprint: snapshot.blueprint,
            theme_id: snapshot.theme_id,
            sections: snapshot.sections.clone(),
            contents: snapshot.contents.clone(),
            theme_overrides: snapshot.theme_overrides.clone(),
        })
    }
    .map_err(|_| FunnelCommandError::Internal)?;
    if encoded.len() > super::MAX_SNAPSHOT_BYTES {
        return Err(FunnelCommandError::InvalidParameters);
    }
    Ok(encoded)
}

#[cfg(test)]
mod bounds_tests {
    use super::*;

    fn text_changes_for_serialized_size(size: usize) -> Vec<SettingChange> {
        let mut changes = (0..3)
            .map(|index| SettingChange {
                id: format!("field.{index}"),
                value: SettingValue::Text(Some(String::new())),
            })
            .collect::<Vec<_>>();
        let empty_size = serde_json::to_vec(&changes)
            .expect("serialize empty changes")
            .len();
        let mut remaining = size.saturating_sub(empty_size);
        for change in &mut changes {
            let SettingValue::Text(Some(value)) = &mut change.value else {
                unreachable!();
            };
            let text_bytes = remaining.min(MAX_FIELD_TEXT_BYTES);
            value.push_str(&"x".repeat(text_bytes));
            remaining -= text_bytes;
        }
        assert_eq!(remaining, 0, "three text values fit the aggregate boundary");
        assert_eq!(
            serde_json::to_vec(&changes)
                .expect("serialize bounded changes")
                .len(),
            size
        );
        changes
    }

    #[test]
    fn settings_changes_accept_exact_aggregate_byte_cap_and_reject_one_over() {
        let exact = SettingChanges {
            schema_version: 1,
            changes: text_changes_for_serialized_size(MAX_SETTINGS_CHANGES_BYTES),
        };
        exact
            .validate_envelope()
            .expect("exact aggregate cap is accepted");

        let over = SettingChanges {
            schema_version: 1,
            changes: text_changes_for_serialized_size(MAX_SETTINGS_CHANGES_BYTES + 1),
        };
        assert!(matches!(
            over.validate_envelope(),
            Err(FunnelCommandError::InvalidParameters)
        ));
        let over_request = serde_json::json!({
            "funnel_id":"funnel-boundary",
            "base_revision_id":"revision-boundary",
            "section_ids":["hero","features","cta"],
            "settings": over,
        })
        .to_string();
        assert!(over_request.len() <= 16 * 1_024);
        assert!(super::super::FunnelEditorOrderCommand::from_json(&over_request).is_err());

        let request = serde_json::json!({
            "funnel_id":"funnel-boundary",
            "base_revision_id":"revision-boundary",
            "section_ids":["hero","features","cta"],
            "settings": exact,
        })
        .to_string();
        assert!(request.len() <= 16 * 1_024);
        let command = super::super::FunnelEditorOrderCommand::from_json(&request);
        assert!(
            command.is_ok(),
            "exact settings cap fits the API/command envelope"
        );
    }
}
