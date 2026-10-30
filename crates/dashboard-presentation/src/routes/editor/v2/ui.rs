#[cfg(feature = "browser")]
use super::drafts::*;
#[cfg(feature = "browser")]
use super::orchestration::*;
#[cfg(feature = "browser")]
use super::state::*;
use super::*;

fn section_kind_label(kind: SectionKind) -> &'static str {
    match kind {
        SectionKind::Hero => "Hero",
        SectionKind::Features => "Features",
        SectionKind::Testimonials => "Testimonials",
        SectionKind::Cta => "Call to action",
        SectionKind::Footer => "Footer",
    }
}

fn section_kind_from_id(value: &str) -> Option<SectionKind> {
    match value {
        "hero" => Some(SectionKind::Hero),
        "features" => Some(SectionKind::Features),
        "testimonials" => Some(SectionKind::Testimonials),
        "cta" => Some(SectionKind::Cta),
        "footer" => Some(SectionKind::Footer),
        _ => None,
    }
}

fn block_kind_label(block: &BlockV2) -> &'static str {
    match block {
        BlockV2::Heading { .. } => "Heading",
        BlockV2::Text { .. } => "Text",
        BlockV2::List { .. } => "List",
        BlockV2::Button { .. } => "Button",
        BlockV2::Image { .. } => "Image",
        BlockV2::Form { .. } => "Form",
    }
}

fn block_text(block: &BlockV2) -> String {
    match block {
        BlockV2::Heading { text, .. } | BlockV2::Text { text, .. } => text.clone(),
        BlockV2::List { items, .. } => items.join("\n"),
        BlockV2::Button { label, .. } => label.clone(),
        BlockV2::Image { .. } => "Image assets are added in the media library phase.".to_owned(),
        BlockV2::Form { .. } => "Form editing is added in the forms phase.".to_owned(),
    }
}

fn block_value_with_text(block: &BlockV2, text: String) -> Option<NewBlock> {
    match block {
        BlockV2::Heading { level, .. } => Some(NewBlock::Heading {
            level: *level,
            text,
        }),
        BlockV2::Text { .. } => Some(NewBlock::Text { text }),
        BlockV2::List { .. } => Some(NewBlock::List {
            items: text.lines().map(str::to_owned).collect(),
        }),
        BlockV2::Button { action, .. } => Some(NewBlock::Button {
            label: text,
            action: action.clone(),
        }),
        BlockV2::Image { asset_id, alt, .. } => Some(NewBlock::Image {
            asset_id: asset_id.clone(),
            alt: alt.clone(),
        }),
        BlockV2::Form { form_id, .. } => Some(NewBlock::Form {
            form_id: form_id.clone(),
        }),
    }
}

fn block_value_with_action(block: &BlockV2, action: ActionV2) -> Option<NewBlock> {
    match block {
        BlockV2::Button { label, .. } => Some(NewBlock::Button {
            label: label.clone(),
            action,
        }),
        _ => None,
    }
}

fn revision_comparison(current: &DocumentV2, previous: &DocumentV2) -> Vec<String> {
    let mut changes = Vec::new();
    if current.theme() != previous.theme() {
        changes.push("The theme changed.".to_owned());
    }
    let current_pages = current
        .pages()
        .iter()
        .map(|page| (page.id().as_str().to_owned(), page))
        .collect::<BTreeMap<_, _>>();
    let previous_pages = previous
        .pages()
        .iter()
        .map(|page| (page.id().as_str().to_owned(), page))
        .collect::<BTreeMap<_, _>>();
    for (id, page) in &current_pages {
        match previous_pages.get(id) {
            None => changes.push(format!("Page added: {} ({id}).", page.title())),
            Some(old) if *old != *page => {
                changes.push(format!("Page changed: {} ({id}).", page.title()))
            }
            Some(_) => {}
        }
    }
    for (id, page) in &previous_pages {
        if !current_pages.contains_key(id) {
            changes.push(format!("Page removed: {} ({id}).", page.title()));
        }
    }
    let current_sections = current
        .pages()
        .iter()
        .flat_map(|page| page.sections())
        .map(|section| (section.id().as_str().to_owned(), section))
        .collect::<BTreeMap<_, _>>();
    let previous_sections = previous
        .pages()
        .iter()
        .flat_map(|page| page.sections())
        .map(|section| (section.id().as_str().to_owned(), section))
        .collect::<BTreeMap<_, _>>();
    for (id, section) in &current_sections {
        match previous_sections.get(id) {
            None => changes.push(format!(
                "Section added: {} ({id}).",
                section_kind_label(section.section_type())
            )),
            Some(old) if *old != *section => changes.push(format!(
                "Section changed: {} ({id}).",
                section_kind_label(section.section_type())
            )),
            Some(_) => {}
        }
    }
    for (id, section) in &previous_sections {
        if !current_sections.contains_key(id) {
            changes.push(format!(
                "Section removed: {} ({id}).",
                section_kind_label(section.section_type())
            ));
        }
    }
    if current.forms() != previous.forms() {
        changes.push("Form definitions changed.".to_owned());
    }
    if changes.is_empty() {
        changes.push("There are no document differences.".to_owned());
    }
    changes
}

#[cfg(any(feature = "browser", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ThemeColorField {
    Background,
    Border,
    Muted,
    OnPrimary,
    Primary,
    Surface,
    Text,
}

#[cfg(any(feature = "browser", test))]
impl ThemeColorField {
    const ALL: [Self; 7] = [
        Self::Background,
        Self::Surface,
        Self::Text,
        Self::Muted,
        Self::Primary,
        Self::OnPrimary,
        Self::Border,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Border => "border",
            Self::Muted => "muted",
            Self::OnPrimary => "on-primary",
            Self::Primary => "primary",
            Self::Surface => "surface",
            Self::Text => "text",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Background => "Page background",
            Self::Border => "Borders",
            Self::Muted => "Secondary text",
            Self::OnPrimary => "Text on buttons",
            Self::Primary => "Buttons and accents",
            Self::Surface => "Cards and form surfaces",
            Self::Text => "Main text",
        }
    }

    fn token(self) -> &'static str {
        match self {
            Self::Background => "color-background",
            Self::Border => "color-border",
            Self::Muted => "color-muted",
            Self::OnPrimary => "color-on-primary",
            Self::Primary => "color-primary",
            Self::Surface => "color-surface",
            Self::Text => "color-text",
        }
    }

    fn override_value(self, colors: Option<&ThemeColors>) -> Option<&str> {
        let colors = colors?;
        match self {
            Self::Background => colors.color_background.as_deref(),
            Self::Border => colors.color_border.as_deref(),
            Self::Muted => colors.color_muted.as_deref(),
            Self::OnPrimary => colors.color_on_primary.as_deref(),
            Self::Primary => colors.color_primary.as_deref(),
            Self::Surface => colors.color_surface.as_deref(),
            Self::Text => colors.color_text.as_deref(),
        }
    }

    fn set_override(self, colors: &mut ThemeColors, value: Option<String>) {
        match self {
            Self::Background => colors.color_background = value,
            Self::Border => colors.color_border = value,
            Self::Muted => colors.color_muted = value,
            Self::OnPrimary => colors.color_on_primary = value,
            Self::Primary => colors.color_primary = value,
            Self::Surface => colors.color_surface = value,
            Self::Text => colors.color_text = value,
        }
    }
}

#[cfg(any(feature = "browser", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ThemeStyleField {
    TypeScale,
    FontFamily,
    SpacingScale,
    CornerScale,
    ContentWidth,
    ButtonShape,
    Shadow,
}

#[cfg(any(feature = "browser", test))]
impl ThemeStyleField {
    const ALL: [Self; 7] = [
        Self::TypeScale,
        Self::FontFamily,
        Self::SpacingScale,
        Self::ContentWidth,
        Self::CornerScale,
        Self::ButtonShape,
        Self::Shadow,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::TypeScale => "type-scale",
            Self::FontFamily => "font-family",
            Self::SpacingScale => "spacing-scale",
            Self::CornerScale => "corner-scale",
            Self::ContentWidth => "content-width",
            Self::ButtonShape => "button-shape",
            Self::Shadow => "shadow",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::TypeScale => "Text size",
            Self::FontFamily => "Font family",
            Self::SpacingScale => "Section spacing",
            Self::CornerScale => "Card corners",
            Self::ContentWidth => "Content width",
            Self::ButtonShape => "Button shape",
            Self::Shadow => "Card shadow",
        }
    }

    fn choices(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::TypeScale => &[
                ("compact", "Compact"),
                ("default", "Default"),
                ("large", "Large"),
                ("display", "Display"),
            ],
            Self::FontFamily => &[
                ("system_ui", "System UI"),
                ("arial", "Arial"),
                ("georgia", "Georgia"),
                ("verdana", "Verdana"),
                ("trebuchet_ms", "Trebuchet MS"),
            ],
            Self::SpacingScale => &[
                ("compact", "Compact"),
                ("default", "Default"),
                ("relaxed", "Relaxed"),
                ("airy", "Airy"),
                ("spacious", "Spacious"),
            ],
            Self::CornerScale => &[
                ("square", "Square"),
                ("soft", "Soft"),
                ("rounded", "Rounded"),
                ("large", "Large"),
                ("extra_large", "Extra large"),
            ],
            Self::ContentWidth => &[
                ("narrow", "Narrow"),
                ("default", "Default"),
                ("wide", "Wide"),
            ],
            Self::ButtonShape => &[
                ("square", "Square"),
                ("rounded", "Rounded"),
                ("soft", "Soft"),
                ("pill", "Pill"),
            ],
            Self::Shadow => &[("none", "None"), ("subtle", "Subtle"), ("raised", "Raised")],
        }
    }

    fn override_id(self, overrides: &ThemeOverrides) -> Option<&'static str> {
        match self {
            Self::TypeScale => overrides.type_scale.map(|value| match value {
                TypeScale::Compact => "compact",
                TypeScale::Default => "default",
                TypeScale::Large => "large",
                TypeScale::Display => "display",
            }),
            Self::FontFamily => overrides.font_family.map(|value| match value {
                FontFamily::SystemUi => "system_ui",
                FontFamily::Arial => "arial",
                FontFamily::Georgia => "georgia",
                FontFamily::Verdana => "verdana",
                FontFamily::TrebuchetMs => "trebuchet_ms",
            }),
            Self::SpacingScale => overrides.spacing_scale.map(|value| match value {
                SpacingScale::Compact => "compact",
                SpacingScale::Default => "default",
                SpacingScale::Relaxed => "relaxed",
                SpacingScale::Airy => "airy",
                SpacingScale::Spacious => "spacious",
            }),
            Self::CornerScale => overrides.corner_scale.map(|value| match value {
                CornerScale::Square => "square",
                CornerScale::Soft => "soft",
                CornerScale::Rounded => "rounded",
                CornerScale::Large => "large",
                CornerScale::ExtraLarge => "extra_large",
            }),
            Self::ContentWidth => overrides.content_width.map(|value| match value {
                ContentWidth::Narrow => "narrow",
                ContentWidth::Default => "default",
                ContentWidth::Wide => "wide",
            }),
            Self::ButtonShape => overrides.button_shape.map(|value| match value {
                ButtonShape::Square => "square",
                ButtonShape::Rounded => "rounded",
                ButtonShape::Soft => "soft",
                ButtonShape::Pill => "pill",
            }),
            Self::Shadow => overrides.shadow.map(|value| match value {
                Shadow::None => "none",
                Shadow::Subtle => "subtle",
                Shadow::Raised => "raised",
            }),
        }
    }

    fn set_override(self, overrides: &mut ThemeOverrides, choice: &str) -> bool {
        match self {
            Self::TypeScale => {
                overrides.type_scale = match choice {
                    "preset" => None,
                    "compact" => Some(TypeScale::Compact),
                    "default" => Some(TypeScale::Default),
                    "large" => Some(TypeScale::Large),
                    "display" => Some(TypeScale::Display),
                    _ => return false,
                }
            }
            Self::FontFamily => {
                overrides.font_family = match choice {
                    "preset" => None,
                    "system_ui" => Some(FontFamily::SystemUi),
                    "arial" => Some(FontFamily::Arial),
                    "georgia" => Some(FontFamily::Georgia),
                    "verdana" => Some(FontFamily::Verdana),
                    "trebuchet_ms" => Some(FontFamily::TrebuchetMs),
                    _ => return false,
                }
            }
            Self::SpacingScale => {
                overrides.spacing_scale = match choice {
                    "preset" => None,
                    "compact" => Some(SpacingScale::Compact),
                    "default" => Some(SpacingScale::Default),
                    "relaxed" => Some(SpacingScale::Relaxed),
                    "airy" => Some(SpacingScale::Airy),
                    "spacious" => Some(SpacingScale::Spacious),
                    _ => return false,
                }
            }
            Self::CornerScale => {
                overrides.corner_scale = match choice {
                    "preset" => None,
                    "square" => Some(CornerScale::Square),
                    "soft" => Some(CornerScale::Soft),
                    "rounded" => Some(CornerScale::Rounded),
                    "large" => Some(CornerScale::Large),
                    "extra_large" => Some(CornerScale::ExtraLarge),
                    _ => return false,
                }
            }
            Self::ContentWidth => {
                overrides.content_width = match choice {
                    "preset" => None,
                    "narrow" => Some(ContentWidth::Narrow),
                    "default" => Some(ContentWidth::Default),
                    "wide" => Some(ContentWidth::Wide),
                    _ => return false,
                }
            }
            Self::ButtonShape => {
                overrides.button_shape = match choice {
                    "preset" => None,
                    "square" => Some(ButtonShape::Square),
                    "rounded" => Some(ButtonShape::Rounded),
                    "soft" => Some(ButtonShape::Soft),
                    "pill" => Some(ButtonShape::Pill),
                    _ => return false,
                }
            }
            Self::Shadow => {
                overrides.shadow = match choice {
                    "preset" => None,
                    "none" => Some(Shadow::None),
                    "subtle" => Some(Shadow::Subtle),
                    "raised" => Some(Shadow::Raised),
                    _ => return false,
                }
            }
        }
        true
    }
}

#[cfg(feature = "browser")]
fn set_theme_color(state: Signal<V2EditorState>, field: ThemeColorField, value: Option<String>) {
    let Some(mut overrides) = state.with(|state| {
        state
            .document
            .as_ref()
            .map(|document| document.theme().overrides().clone())
    }) else {
        return;
    };
    let mut colors = overrides.colors.take().unwrap_or_default();
    field.set_override(&mut colors, value);
    let has_colors = colors.color_background.is_some()
        || colors.color_border.is_some()
        || colors.color_muted.is_some()
        || colors.color_on_primary.is_some()
        || colors.color_primary.is_some()
        || colors.color_surface.is_some()
        || colors.color_text.is_some();
    overrides.colors = if has_colors { Some(colors) } else { None };
    dispatch_v2_edit(
        state,
        Edit::SetThemeOverrides {
            overrides: Box::new(overrides),
        },
    );
}

#[cfg(feature = "browser")]
fn set_theme_style(state: Signal<V2EditorState>, field: ThemeStyleField, choice: &str) {
    let Some(mut overrides) = state.with(|state| {
        state
            .document
            .as_ref()
            .map(|document| document.theme().overrides().clone())
    }) else {
        return;
    };
    if field.set_override(&mut overrides, choice) {
        dispatch_v2_edit(
            state,
            Edit::SetThemeOverrides {
                overrides: Box::new(overrides),
            },
        );
    }
}

#[cfg(any(feature = "browser", test))]
fn theme_contrast_warnings(tokens: &BTreeMap<String, String>) -> Vec<String> {
    let pairs = [
        (
            "color-text",
            "color-background",
            "Main text on page background",
        ),
        ("color-text", "color-surface", "Main text on cards"),
        (
            "color-muted",
            "color-background",
            "Secondary text on page background",
        ),
        ("color-muted", "color-surface", "Secondary text on cards"),
        ("color-on-primary", "color-primary", "Button text"),
    ];
    pairs
        .into_iter()
        .filter_map(|(foreground, background, label)| {
            let foreground = tokens.get(foreground)?;
            let background = tokens.get(background)?;
            let ratio = contrast_ratio(foreground, background)?;
            (ratio < 4.5).then(|| format!("{label} ({ratio:.2}:1)"))
        })
        .collect()
}

#[cfg(any(feature = "browser", test))]
fn theme_profile_contrast_warnings(profile: &ResolvedThemeProfile) -> Vec<String> {
    [
        ("Desktop", &profile.desktop),
        ("Tablet", &profile.tablet),
        ("Mobile", &profile.mobile),
    ]
    .into_iter()
    .flat_map(|(viewport, tokens)| {
        theme_contrast_warnings(tokens)
            .into_iter()
            .map(move |warning| format!("{viewport}: {warning}"))
    })
    .collect()
}

#[cfg(any(feature = "browser", test))]
fn contrast_ratio(first: &str, second: &str) -> Option<f64> {
    fn luminance(value: &str) -> Option<f64> {
        let bytes = value.as_bytes();
        if bytes.len() != 7 || bytes[0] != b'#' {
            return None;
        }
        let mut channels = [0.0; 3];
        for (index, channel) in channels.iter_mut().enumerate() {
            let start = index * 2 + 1;
            let value = u8::from_str_radix(std::str::from_utf8(&bytes[start..start + 2]).ok()?, 16)
                .ok()? as f64
                / 255.0;
            *channel = if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            };
        }
        Some(0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2])
    }

    let first = luminance(first)?;
    let second = luminance(second)?;
    let (lighter, darker) = if first >= second {
        (first, second)
    } else {
        (second, first)
    };
    Some((lighter + 0.05) / (darker + 0.05))
}

#[cfg(feature = "browser")]
#[component]
pub(in crate::routes::editor) fn V2FunnelDetail(id: String) -> Element {
    let mut state = use_signal(|| V2EditorState::for_route(id.clone()));
    let load_error_reload_id = id.clone();
    let stale_reload_id = id.clone();
    let discard_recovered_id = id.clone();
    let restore_older_draft_id = id.clone();
    let discard_older_draft_id = id.clone();
    let mut responsive_viewport = use_signal(|| PreviewViewport::Desktop);
    let selected_viewport = responsive_viewport();

    use_effect(use_reactive!(|(id)| {
        let requested_id = id.clone();
        let route_epoch = state.with_mut(|state| state.begin_route(requested_id.clone()));
        spawn(async move { load_v2_editor_state(state, requested_id, route_epoch).await });
    }));

    let view = state();
    let title = view
        .funnel
        .as_ref()
        .map(|funnel| funnel.name.clone())
        .unwrap_or_else(|| "Typed funnel editor".to_owned());
    let current_page = view
        .document
        .as_ref()
        .and_then(|document| {
            document
                .pages()
                .iter()
                .find(|page| page.id().as_str() == view.selected_page)
        })
        .cloned();
    let selected_section = current_page
        .as_ref()
        .and_then(|page| {
            page.sections()
                .iter()
                .find(|section| Some(section.id().as_str()) == view.selected_section.as_deref())
        })
        .cloned();
    let add_block_location = selected_section.as_ref().map(|section| {
        (
            section.id().clone(),
            section.blocks().last().map(|block| block.id().clone()),
        )
    });
    let add_form_location = add_block_location.clone();
    let locked = !view.can_edit();
    let dirty = view.has_unsaved_changes();
    let recovered_document = view
        .recovered_draft
        .as_ref()
        .and_then(|draft| DocumentV2::decode(draft.document_json.as_bytes(), &view.funnel_id).ok());
    let page_options = view.document.as_ref().map_or_else(Vec::new, |document| {
        document
            .pages()
            .iter()
            .map(|page| (page.id().as_str().to_owned(), page.title().to_owned()))
            .collect()
    });

    rsx! {
        div {
            class: "editor-page",
            section { class: "page-heading",
                p { class: "eyebrow", "Funnels / Typed editor" }
                h1 { "{title}" }
                p { class: "page-description", "Compose typed sections and blocks. Changes stay as a local draft until you save a new revision." }
                code { class: "detail-id", "Funnel ID: {id}" }
                Link { to: RouteView::ServiceBriefPage { id: id.clone() }, class: "secondary-button completion-report-link", "Service brief" }
                Link { to: RouteView::FunnelCompletions { id: id.clone() }, class: "secondary-button completion-report-link", "View form completions" }
                Link { to: RouteView::FunnelInteractionReport { id: id.clone() }, class: "secondary-button completion-report-link", "View interaction counts" }
            }
            if view.loading {
                section { class: "empty-state editor-loading", role: "status", "aria-label": "Typed funnel editor",
                    "Loading the current typed revision…"
                }
            }
            if view.load_error {
                section { class: "empty-state editor-load-error", role: "alert", "aria-label": "Typed editor error",
                    p { "The typed funnel editor could not be loaded." }
                    button {
                        class: "secondary-button",
                        r#type: "button",
                        onclick: move |_| {
                            let route_epoch = state.with_mut(|state| state.begin_route(load_error_reload_id.clone()));
                            let requested_id = load_error_reload_id.clone();
                            spawn(async move { load_v2_editor_state(state, requested_id, route_epoch).await });
                        },
                        "Try again"
                    }
                }
            }
            if let Some(document) = view.document.clone() {
                section { class: "editor-toolbar", "aria-label": "Typed editor actions",
                    label { class: "editor-setting-control", r#for: "public-language",
                        "Public language"
                        select {
                            id: "public-language",
                            value: document.public_language().as_str(),
                            disabled: locked,
                            onchange: move |event| {
                                let language = match event.value().as_str() {
                                    "en" => Some(PublicLanguage::English),
                                    "fr" => Some(PublicLanguage::French),
                                    _ => None,
                                };
                                if let Some(language) = language {
                                    dispatch_v2_edit(state, Edit::SetPublicLanguage { language: Some(language) });
                                }
                            },
                            option { value: "en", "English" }
                            option { value: "fr", "Français" }
                        }
                    }
                    div {
                        p { class: "eyebrow", "Draft revision" }
                        code { "{view.base_revision_id}" }
                        if dirty {
                            span { class: "editor-unsaved-badge", "Unsaved changes" }
                        } else {
                            span { class: "editor-saved-badge", "All changes saved" }
                        }
                    }
                    div { class: "editor-actions",
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            disabled: !view.can_undo(),
                            onclick: move |_| {
                                let request = state.with_mut(|state| state.apply_history(-1));
                                if let Some((_, funnel_id, base_revision_id, document_json)) = request.clone() {
                                    let draft = V2LocalDraft { version: 1, funnel_id, base_revision_id, document_json };
                                    spawn(async move { let _ = write_v2_local_draft(&draft).await; });
                                }
                                schedule_v2_preview(state, request);
                            },
                            "Undo"
                        }
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            disabled: !view.can_redo(),
                            onclick: move |_| {
                                let request = state.with_mut(|state| state.apply_history(1));
                                if let Some((_, funnel_id, base_revision_id, document_json)) = request.clone() {
                                    let draft = V2LocalDraft { version: 1, funnel_id, base_revision_id, document_json };
                                    spawn(async move { let _ = write_v2_local_draft(&draft).await; });
                                }
                                schedule_v2_preview(state, request);
                            },
                            "Redo"
                        }
                        button {
                            class: "primary-button",
                            r#type: "button",
                            disabled: !view.can_save(),
                            onclick: move |_| {
                                spawn(async move { save_v2_revision(state).await; });
                            },
                            if view.saving { "Saving…" } else if view.save_retryable { "Retry save" } else { "Save revision" }
                        }
                    }
                }
                if let Some(capacity) = view.stage_capacity.as_ref() {
                    p {
                        class: "editor-capacity-note",
                        role: "note",
                        "aria-label": "Temporary R2 storage budget",
                        "Worst-case orphan exposure is capped at {capacity.worst_case_orphan_bytes} bytes across {capacity.worst_case_orphan_keys} object writes. Currently reserved: {capacity.orphan_bytes_held} bytes across {capacity.orphan_keys_held} object writes."
                    }
                } else {
                    p {
                        class: "error-message editor-capacity-error",
                        role: "alert",
                        "The temporary storage budget is unavailable. You can keep editing this browser draft, but saving is disabled until the capacity endpoint is available. Reload the editor after resolving the deployment configuration."
                    }
                }
                if let Some(status) = view.status.as_ref() {
                    p { class: "editor-save-status", role: "status", "aria-live": "polite", "aria-atomic": "true", "{status}" }
                }
                if let Some(error) = view.error.as_ref() {
                    p { class: "error-message editor-save-error", role: "alert", "{error}" }
                }
                if view.has_uncommitted_editor_inputs() {
                    p {
                        class: "error-message editor-input-draft-note",
                        role: "status",
                        aria_live: "polite",
                        "The preview and saved revision show the last valid settings. Correct unfinished form or page edits before saving or publishing."
                    }
                }
                if view.stale {
                    p { class: "error-message", role: "status", "Your draft is still in this browser. Reload the current revision to compare it or restore it as a new draft." }
                    button {
                        class: "secondary-button",
                        r#type: "button",
                        onclick: move |_| {
                            let route_epoch = state.with_mut(|state| state.begin_route(stale_reload_id.clone()));
                            let requested_id = stale_reload_id.clone();
                            spawn(async move { load_v2_editor_state(state, requested_id, route_epoch).await });
                        },
                        "Reload current revision"
                    }
                }
                if let Some(error) = view.recovery_error.as_ref() {
                    p { class: "error-message", role: "status", "{error}" }
                }
                if view.recovered_current_draft {
                    section { class: "editor-recovery-panel", role: "status", "aria-label": "Recovered local draft",
                        p { "Recovered your unsaved draft from this browser." }
                        button {
                            class: "text-button",
                            r#type: "button",
                            onclick: move |_| {
                                let request = state.with_mut(V2EditorState::discard_recovered_current_draft);
                                schedule_v2_preview(state, request);
                                let funnel_id = discard_recovered_id.clone();
                                let _ = spawn(async move { let _ = remove_v2_local_draft(&funnel_id).await; });
                            },
                            "Discard recovered draft"
                        }
                    }
                }
                if let Some(draft) = view.recovered_draft.as_ref() {
                    section { class: "editor-recovery-panel", role: "region", "aria-label": "Older recovered draft",
                        h2 { "A draft from an older revision is available" }
                        p { "It was based on revision {draft.base_revision_id}. Compare it with the current draft before restoring it." }
                        if let Some(old_document) = recovered_document.as_ref() {
                            ul {
                                for line in revision_comparison(&document, old_document) {
                                    li { "{line}" }
                                }
                            }
                        }
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            disabled: locked || recovered_document.is_none(),
                            onclick: move |_| {
                                if let Some(draft) = state.with(|state| state.recovered_draft.clone()) {
                                    if let Ok(document) = DocumentV2::decode(draft.document_json.as_bytes(), &restore_older_draft_id) {
                                        let request = state.with_mut(|state| state.restore_document(document));
                                        if let Some((_, funnel_id, base_revision_id, document_json)) = request.clone() {
                                            let local = V2LocalDraft { version: 1, funnel_id, base_revision_id, document_json };
                                            spawn(async move { let _ = write_v2_local_draft(&local).await; });
                                        }
                                        schedule_v2_preview(state, request);
                                    }
                                }
                            },
                            "Use this content as a new draft"
                        }
                        button {
                            class: "text-button",
                            r#type: "button",
                            onclick: move |_| {
                                state.with_mut(|state| state.recovered_draft = None);
                                let funnel_id = discard_older_draft_id.clone();
                                spawn(async move { let _ = remove_v2_local_draft(&funnel_id).await; });
                            },
                            "Discard older draft"
                        }
                    }
                }
                section { class: "editor-publication-panel", role: "region", "aria-label": "Publication",
                    div { class: "section-heading",
                        h2 { "Publication" }
                        span { "Draft and live status" }
                    }
                    div { class: "editor-publication-facts",
                        p { strong { "Draft revision " } code { "{view.base_revision_id}" } }
                        p {
                            strong { "Live revision " }
                            if view.publication_refreshing {
                                span { "Checking…" }
                            } else if view.publication_state_unknown {
                                span { "Status unavailable" }
                            } else if let Some(live) = view.published_revision_id.as_ref() {
                                code { "{live}" }
                            } else {
                                span { "Not published" }
                            }
                        }
                        if view.show_live_funnel_link() {
                            if let Some(funnel) = view.funnel.as_ref() {
                                a { class: "editor-live-link", href: "/f/{funnel.slug}", target: "_blank", rel: "noopener noreferrer", "Open live funnel" }
                            }
                        }
                    }
                    if dirty {
                        p { class: "editor-publication-help", "Save the draft to check its publication readiness." }
                    } else if let Some(readiness) = view.publication_readiness.as_ref().filter(|readiness| readiness.revision_id == view.base_revision_id) {
                        if !readiness.findings.is_empty() {
                            ul { class: "editor-readiness-findings", "aria-label": "Publication readiness findings",
                                for finding in &readiness.findings {
                                    li {
                                        class: if finding.severity == PublicationReadinessSeverity::Blocking { "editor-readiness-blocker" } else { "editor-readiness-warning" },
                                        strong { if finding.severity == PublicationReadinessSeverity::Blocking { "Must fix" } else { "Suggestion" } }
                                        p { "{finding.message()}" }
                                        code { "{finding.target_label()}" }
                                    }
                                }
                            }
                        }
                    } else if view.publication_readiness.is_some() {
                        p { class: "editor-publication-help", role: "status", "Checking publication readiness for this saved revision…" }
                    }
                    if let Some(status) = view.publication_status.as_ref() {
                        p { class: "editor-publication-status", role: "status", "aria-live": "polite", "aria-atomic": "true", "{status}" }
                    }
                    if let Some(error) = view.publication_error.as_ref() {
                        p { class: "error-message", role: "alert", "{error}" }
                    }
                    if dirty {
                        p { class: "editor-publication-help", "Save draft changes before publishing." }
                    }
                    div { class: "editor-publication-actions",
                        button {
                            class: "primary-button",
                            r#type: "button",
                            disabled: !view.can_publish(),
                            onclick: move |_| {
                                spawn(async move { publish_v2_revision(state, true).await; });
                            },
                            if view.publication_pending && matches!(view.publication_attempt, Some(PublicationAction::Publish { .. })) { "Publishing…" }
                            else if view.publication_retryable && matches!(view.publication_attempt, Some(PublicationAction::Publish { .. })) { "Retry publish" }
                            else { "Publish saved revision" }
                        }
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            disabled: !view.can_unpublish(),
                            onclick: move |_| {
                                if view.publication_retryable && matches!(view.publication_attempt, Some(PublicationAction::Unpublish { .. })) {
                                    spawn(async move { publish_v2_revision(state, false).await; });
                                } else {
                                    state.with_mut(|state| state.unpublish_confirmation = true);
                                    spawn(async move { crate::routes::browser::set_modal_dialog("v2-unpublish-dialog", true).await; });
                                }
                            },
                            if view.publication_pending && matches!(view.publication_attempt, Some(PublicationAction::Unpublish { .. })) { "Unpublishing…" }
                            else { "Unpublish" }
                        }
                    }
                }
                if view.unpublish_confirmation {
                    dialog {
                        id: "v2-unpublish-dialog",
                        class: "funnel-dialog confirmation-dialog",
                        "aria-modal": "true",
                        "aria-labelledby": "v2-unpublish-title",
                        "aria-describedby": "v2-unpublish-description",
                        oncancel: move |event| {
                            event.prevent_default();
                            state.with_mut(|state| state.unpublish_confirmation = false);
                            spawn(async move { crate::routes::browser::set_modal_dialog("v2-unpublish-dialog", false).await; });
                        },
                        h2 { id: "v2-unpublish-title", "Unpublish this funnel?" }
                        p { id: "v2-unpublish-description", "This removes the live revision pointer. Already viewed copies may remain available in browsers or intermediary caches." }
                        div { class: "dialog-actions",
                            button {
                                class: "secondary-button",
                                r#type: "button",
                                autofocus: true,
                                onclick: move |_| {
                                    state.with_mut(|state| state.unpublish_confirmation = false);
                                    spawn(async move { crate::routes::browser::set_modal_dialog("v2-unpublish-dialog", false).await; });
                                },
                                "Cancel"
                            }
                            button {
                                class: "primary-button danger-button",
                                r#type: "button",
                                onclick: move |_| {
                                    state.with_mut(|state| state.unpublish_confirmation = false);
                                    spawn(async move {
                                        crate::routes::browser::set_modal_dialog("v2-unpublish-dialog", false).await;
                                        publish_v2_revision(state, false).await;
                                    });
                                },
                                "Confirm unpublish"
                            }
                        }
                    }
                }
                if let Some(page_id) = view.delete_page_confirmation.as_ref() {
                    {
                        let confirming_page_id = page_id.clone();
                        let confirming_page_title = document
                            .pages()
                            .iter()
                            .find(|page| page.id().as_str() == page_id)
                            .map(|page| page.title().to_owned())
                            .unwrap_or_else(|| "this page".to_owned());
                        rsx! {
                            dialog {
                                id: "v2-delete-page-dialog",
                                class: "funnel-dialog confirmation-dialog",
                                "aria-modal": "true",
                                "aria-labelledby": "v2-delete-page-title",
                                "aria-describedby": "v2-delete-page-description",
                                oncancel: move |event| {
                                    event.prevent_default();
                                    state.with_mut(|state| state.delete_page_confirmation = None);
                                    spawn(async move { crate::routes::browser::set_modal_dialog("v2-delete-page-dialog", false).await; });
                                },
                                h2 { id: "v2-delete-page-title", "Delete {confirming_page_title}?" }
                                p { id: "v2-delete-page-description", "Buttons that navigate to this page will be removed. Forms that redirect here will show the inline message ‘Thank you.’ Forms placed on this page will also be removed. Other steps must remain reachable. You can undo these changes before saving." }
                                div { class: "dialog-actions",
                                    button {
                                        class: "secondary-button",
                                        r#type: "button",
                                        autofocus: true,
                                        onclick: move |_| {
                                            state.with_mut(|state| state.delete_page_confirmation = None);
                                            spawn(async move { crate::routes::browser::set_modal_dialog("v2-delete-page-dialog", false).await; });
                                        },
                                        "Cancel"
                                    }
                                    button {
                                        class: "primary-button danger-button",
                                        r#type: "button",
                                        onclick: move |_| {
                                            state.with_mut(|state| state.delete_page_confirmation = None);
                                            let page_id = PageId::parse(confirming_page_id.clone()).ok();
                                            spawn(async move {
                                                crate::routes::browser::set_modal_dialog("v2-delete-page-dialog", false).await;
                                                if let Some(page) = page_id {
                                                    dispatch_v2_edit(state, Edit::DeletePage { page });
                                                }
                                            });
                                        },
                                        "Delete page"
                                    }
                                }
                            }
                        }
                    }
                }
                section { class: "editor-workspace", "aria-label": "Typed visual editor",
                    div { class: "editor-outline-panel",
                        div { class: "section-heading",
                            h2 { "Pages and sections" }
                            span { "{document.pages().len()} pages" }
                        }
                        if document.pages().len() > 1 {
                            label { class: "editor-setting-control",
                                "Current page"
                                select {
                                    value: "{view.selected_page}",
                                    disabled: view.pending || view.saving || view.has_page_input_draft() || view.page_input_error.is_some(),
                                    onchange: move |event| {
                                        let request = state.with_mut(|state| state.select_page(&event.value()));
                                        schedule_v2_preview(state, request);
                                    },
                                    for page in document.pages() {
                                        option { value: "{page.id().as_str()}", "{page.title()}" }
                                    }
                                }
                            }
                        }
                        if let Some(page) = current_page.as_ref() {
                            {
                                let page_id = page.id().as_str().to_owned();
                                let page_id_title = page.id().as_str().to_owned();
                                let page_id_slug = page.id().as_str().to_owned();
                                let page_id_add = page.id().clone();
                                let page_id_update = page.id().clone();
                                let page_id_delete = page.id().clone();
                                let page_title = page.title().to_owned();
                                let page_slug = page.slug().to_owned();
                                let (page_title_input, page_slug_input) = view.page_input_values(
                                    &page_id,
                                    &page_title,
                                    &page_slug,
                                );
                                let page_input_error_message =
                                    view.page_input_error.clone().unwrap_or_default();
                                let is_entry_page = page.id() == document.entry_page_id();
                                let page_is_referenced = page_is_referenced(&document, page.id());
                                rsx! {
                                    fieldset { class: "editor-setting-group editor-page-settings",
                                        legend { "Page settings" }
                                        label { class: "editor-setting-control",
                                            "Page title"
                                            input {
                                                r#type: "text",
                                                value: "{page_title_input}",
                                                maxlength: "8192",
                                                disabled: locked,
                                                "aria-label": "Page title",
                                                "aria-invalid": "{view.page_input_error_field == Some(PageInputErrorField::Title)}",
                                                "aria-describedby": "page-settings-error",
                                                oninput: move |event| state.with_mut(|state| state.update_page_input(&page_id_title, "title", event.value())),
                                            }
                                        }
                                        if !is_entry_page {
                                            label { class: "editor-setting-control",
                                                "Page slug"
                                                input {
                                                    r#type: "text",
                                                    value: "{page_slug_input}",
                                                    maxlength: "64",
                                                    disabled: locked,
                                                    "aria-label": "Page slug",
                                                    "aria-invalid": "{view.page_input_error_field == Some(PageInputErrorField::Slug)}",
                                                    "aria-describedby": "page-settings-error",
                                                    oninput: move |event| state.with_mut(|state| state.update_page_input(&page_id_slug, "slug", event.value())),
                                                }
                                            }
                                        } else {
                                            p { class: "editor-keyboard-help", "The entry page uses the funnel URL and keeps its route fixed." }
                                        }
                                        p { id: "page-settings-error", class: "editor-form-input-error editor-page-input-error", role: "alert", aria_live: "polite", "{page_input_error_message}" }
                                        button {
                                            class: "secondary-button",
                                            r#type: "button",
                                            disabled: locked || !view.has_page_input_draft() || view.page_input_error.is_some(),
                                            onclick: move |_| dispatch_v2_edit(state, Edit::SetPage {
                                                page: page_id_update.clone(),
                                                title: page_title_input.clone(),
                                                slug: page_slug_input.clone(),
                                            }),
                                            "Apply page settings"
                                        }
                                        if !is_entry_page {
                                            button {
                                                class: "secondary-button danger-button",
                                                r#type: "button",
                                                disabled: locked || view.has_page_input_draft() || view.page_input_error.is_some(),
                                                "aria-label": "Delete page {page.title()}",
                                                onclick: move |_| {
                                                    state.with_mut(|state| state.delete_page_confirmation = Some(page_id_delete.as_str().to_owned()));
                                                    spawn(async move { crate::routes::browser::set_modal_dialog("v2-delete-page-dialog", true).await; });
                                                },
                                                "Delete page"
                                            }
                                            p { class: "editor-keyboard-help", if page_is_referenced { "Deleting this page removes buttons that navigate here and changes its form confirmations to inline text." } else { "Deleting this page also removes forms placed on it. You can undo before saving." } }
                                        }
                                        button {
                                            class: "secondary-button",
                                            r#type: "button",
                                            disabled: locked || view.has_uncommitted_editor_inputs() || document.pages().len() >= 50,
                                            onclick: move |_| {
                                                let defaults = state.with(|state| state.document.as_ref().and_then(next_page_defaults));
                                                if let Some((title, slug)) = defaults {
                                                    dispatch_v2_edit(state, Edit::AddPage {
                                                        from: page_id_add.clone(),
                                                        title,
                                                        slug,
                                                    });
                                                }
                                            },
                                            "Add page"
                                        }
                                    }
                                }
                            }
                        }
                        fieldset { class: "editor-setting-group",
                            legend { "Add a section" }
                            div { class: "editor-add-row" }
                            label { class: "editor-setting-control",
                                "Section type"
                                select {
                                    value: "{section_kind_id(view.new_section_kind)}",
                                    disabled: locked,
                                    onchange: move |event| {
                                        if let Some(kind) = section_kind_from_id(&event.value()) {
                                            state.with_mut(|state| state.new_section_kind = kind);
                                        }
                                    },
                                    for kind in [SectionKind::Hero, SectionKind::Features, SectionKind::Testimonials, SectionKind::Cta, SectionKind::Footer] {
                                        option { value: "{section_kind_id(kind)}", "{section_kind_label(kind)}" }
                                    }
                                }
                            }
                            button {
                                class: "secondary-button",
                                r#type: "button",
                                disabled: locked,
                                onclick: move |_| {
                                    let kind = state.with(|state| state.new_section_kind);
                                    let page = state.with(|state| PageId::parse(state.selected_page.clone()).ok());
                                    if let Some(page) = page {
                                        dispatch_v2_edit(state, Edit::AddSection { page, after: None, section_type: kind });
                                    }
                                },
                                "Add section"
                            }
                        }
                        if let Some(page) = current_page {
                            {
                                let seo_title_page = page.id().clone();
                                let seo_description_page = page.id().clone();
                                let noindex_page = page.id().clone();
                                rsx! {
                                    fieldset { class: "editor-setting-group",
                                        legend { "Page search and sharing" }
                                        p { "Settings for {page.title()}. Save and publish to update the public page." }
                                        label { class: "editor-setting-control",
                                            "SEO title"
                                            input {
                                                r#type: "text",
                                                value: page.seo_title().unwrap_or_default(),
                                                placeholder: page.title(),
                                                disabled: locked,
                                                "aria-describedby": "v2-seo-title-help",
                                                onchange: move |event| dispatch_v2_edit(state, Edit::SetPageSeoTitle { page: seo_title_page.clone(), value: event.value() }),
                                            }
                                        }
                                        p { id: "v2-seo-title-help", "Up to 120 Unicode scalar values. Whitespace is collapsed. Leave blank to use the page title." }
                                        label { class: "editor-setting-control",
                                            "Meta description"
                                            textarea {
                                                value: page.seo_description().unwrap_or_default(),
                                                rows: 3,
                                                disabled: locked,
                                                "aria-describedby": "v2-seo-description-help",
                                                onchange: move |event| dispatch_v2_edit(state, Edit::SetPageSeoDescription { page: seo_description_page.clone(), value: event.value() }),
                                            }
                                        }
                                        p { id: "v2-seo-description-help", "Up to 320 Unicode scalar values. Whitespace is collapsed. Leave blank to omit the description." }
                                        label { class: "editor-setting-control",
                                            input {
                                                r#type: "checkbox",
                                                checked: page.noindex(),
                                                disabled: locked,
                                                "aria-describedby": "v2-seo-noindex-help",
                                                onchange: move |event| dispatch_v2_edit(state, Edit::SetPageNoIndex { page: noindex_page.clone(), noindex: event.checked() }),
                                            }
                                            "Ask search engines not to index this page"
                                        }
                                        p { id: "v2-seo-noindex-help", "Noindex is an indexing preference, not access control. Anyone with the public URL can still open this page." }
                                    }
                                }
                            }
                            div { class: "editor-section-list", role: "list", "aria-label": "Sections on this page",
                                for (index, section) in page.sections().iter().enumerate() {
                                    {
                                        let section_id = section.id().as_str().to_owned();
                                        let selected = view.selected_section.as_deref() == Some(section_id.as_str());
                                        let label = section_kind_label(section.section_type());
                                        let page_id_up = page.id().clone();
                                        let page_id_down = page.id().clone();
                                        let page_id_duplicate = page.id().clone();
                                        let page_id_delete = page.id().clone();
                                        let duplicate_id = section.id().clone();
                                        let delete_id = section.id().clone();
                                        let up_id = section.id().clone();
                                        let down_id = section.id().clone();
                                        let section_ids = page.sections().iter().map(|item| item.id().clone()).collect::<Vec<_>>();
                                        let section_count = section_ids.len();
                                        let section_ids_up = section_ids.clone();
                                        let section_ids_down = section_ids;
                                        rsx! {
                                            div {
                                                class: "editor-section-row",
                                                role: "listitem",
                                                "aria-current": if selected { "true" } else { "false" },
                                                div { class: "editor-section-copy",
                                                    button {
                                                        class: "text-button",
                                                        r#type: "button",
                                                        disabled: view.pending,
                                                        onclick: move |_| state.with_mut(|state| state.select_section(&section_id)),
                                                        "{label}"
                                                    }
                                                    code { "{section.id().as_str()}" }
                                                }
                                                div { class: "editor-section-actions",
                                                    button {
                                                        class: "editor-move-button",
                                                        r#type: "button",
                                                        disabled: locked || index == 0,
                                                        "aria-label": "Move {label} up",
                                                        onclick: move |_| {
                                                            let mut ordered = section_ids_up.clone();
                                                            if index > 0 { ordered.swap(index, index - 1); }
                                                            dispatch_v2_edit(state, Edit::ReorderSections { page: page_id_up.clone(), ordered });
                                                            let _ = &up_id;
                                                        },
                                                        "Up"
                                                    }
                                                    button {
                                                        class: "editor-move-button",
                                                        r#type: "button",
                                                        disabled: locked || index + 1 >= section_count,
                                                        "aria-label": "Move {label} down",
                                                        onclick: move |_| {
                                                            let mut ordered = section_ids_down.clone();
                                                            if index + 1 < ordered.len() { ordered.swap(index, index + 1); }
                                                            dispatch_v2_edit(state, Edit::ReorderSections { page: page_id_down.clone(), ordered });
                                                            let _ = &down_id;
                                                        },
                                                        "Down"
                                                    }
                                                    button {
                                                        class: "editor-move-button",
                                                        r#type: "button",
                                                        disabled: locked,
                                                        "aria-label": "Duplicate {label}",
                                                        onclick: move |_| dispatch_v2_edit(state, Edit::DuplicateSection { page: page_id_duplicate.clone(), section: duplicate_id.clone() }),
                                                        "Duplicate"
                                                    }
                                                    button {
                                                        class: "editor-move-button",
                                                        r#type: "button",
                                                        disabled: locked,
                                                        "aria-label": "Delete {label}",
                                                        onclick: move |_| dispatch_v2_edit(state, Edit::DeleteSection { page: page_id_delete.clone(), section: delete_id.clone() }),
                                                        "Delete"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "editor-inspector-panel", "aria-label": "Typed content inspector",
                        div { class: "section-heading",
                            h2 { "Content and theme" }
                            span { "Typed document v2" }
                        }
                        fieldset { class: "editor-setting-group",
                            legend { "Theme preset" }
                            label { class: "editor-setting-control",
                                "Preset"
                                select {
                                    value: "{document.theme().preset_id().id()}",
                                    disabled: locked,
                                    onchange: move |event| {
                                        if let Some(preset) = ThemePreset::from_id(&event.value()) {
                                            dispatch_v2_edit(state, Edit::SetThemePreset { preset });
                                        }
                                    },
                                    for preset in ThemePreset::all() {
                                        option { value: "{preset.id()}", "{preset.name()}" }
                                    }
                                }
                            }
                        }
                        V2MediaLibrary {
                            state,
                            add_block_location: add_block_location.clone(),
                            locked,
                        }
                        V2ThemeCustomization { state, document: document.clone(), locked }
                        if let Some(section) = selected_section {
                            fieldset { class: "editor-setting-group",
                                legend { "{section_kind_label(section.section_type())}" }
                                p { class: "editor-selected-section", "Section ID: {section.id().as_str()}" }
                                div { class: "editor-block-list", role: "list", "aria-label": "Blocks in selected section",
                                    for (index, block) in section.blocks().iter().enumerate() {
                                        V2BlockEditor {
                                            state,
                                            page_options: page_options.clone(),
                                            section_id: section.id().clone(),
                                            block: block.clone(),
                                            index,
                                            block_ids: section.blocks().iter().map(|item| item.id().clone()).collect::<Vec<_>>(),
                                            locked,
                                        }
                                    }
                                }
                                fieldset { class: "editor-setting-group",
                                    legend { "Add a block" }
                                    label { class: "editor-setting-control",
                                        "Block type"
                                        select {
                                            value: "{view.new_block_kind.id()}",
                                            disabled: locked,
                                            onchange: move |event| {
                                                if let Some(kind) = V2NewBlockKind::from_id(&event.value()) {
                                                    state.with_mut(|state| state.new_block_kind = kind);
                                                }
                                            },
                                            for kind in V2NewBlockKind::ALL {
                                                option { value: "{kind.id()}", "{kind.label()}" }
                                            }
                                        }
                                    }
                                    button {
                                        class: "secondary-button",
                                        r#type: "button",
                                        disabled: locked,
                                        onclick: move |_| {
                                            let kind = state.with(|state| state.new_block_kind);
                                            let language = state.with(|state| {
                                                state
                                                    .document
                                                    .as_ref()
                                                    .map(DocumentV2::public_language)
                                                    .unwrap_or(PublicLanguage::English)
                                            });
                                            let block = kind.create(language);
                                            if let Some((section, after)) = add_block_location.clone() {
                                                dispatch_v2_edit(state, Edit::AddBlock { section, after, block });
                                            }
                                        },
                                        "Add block"
                                    }
                                    button {
                                        class: "secondary-button",
                                        r#type: "button",
                                        disabled: locked,
                                        onclick: move |_| {
                                            if let Some((section, after)) = add_form_location.clone() {
                                                dispatch_v2_edit(state, Edit::CreateForm { section, after });
                                            }
                                        },
                                        "Add form"
                                    }
                                }
                            }
                        } else {
                            p { class: "editor-keyboard-help", "Select a section to edit its blocks." }
                        }
                    }
                    div { class: "editor-preview-panel",
                        div { class: "section-heading", h2 { "Preview" }, span { "Sandboxed" } }
                        div { class: "editor-preview-switcher", role: "group", "aria-label": "Preview viewport",
                            for option in PreviewViewport::ALL {
                                button {
                                    class: "secondary-button",
                                    r#type: "button",
                                    "aria-pressed": "{selected_viewport == option}",
                                    onclick: move |_| responsive_viewport.set(option),
                                    "{option.label()} ({option.width_px()} px)"
                                }
                            }
                        }
                        div { class: "editor-preview-canvas", role: "region", tabindex: "0", "aria-label": "Preview canvas, {selected_viewport.width_px()} pixels wide",
                            iframe {
                                class: "editor-preview-frame",
                                title: "Preview of {title}",
                                width: "{selected_viewport.width_px()}",
                                "sandbox": "",
                                srcdoc: "{view.preview_html}",
                            }
                        }
                    }
                }
                section { class: "editor-history-panel", role: "region", "aria-label": "Revision history",
                    div { class: "section-heading", h2 { "Revision history" }, span { "{view.revisions.len()} saved revisions" } }
                    if let Some(compare_document) = view.compare_document.as_ref() {
                        div { class: "editor-revision-comparison",
                            h3 { "Compare current draft with {view.compare_revision_id.as_deref().unwrap_or_default()}" }
                            ul {
                                for line in revision_comparison(&document, compare_document) {
                                    li { "{line}" }
                                }
                            }
                            button {
                                class: "secondary-button",
                                r#type: "button",
                                disabled: locked,
                                onclick: move |_| {
                                    if let Some(old) = state.with(|state| state.compare_document.clone()) {
                                        let request = state.with_mut(|state| state.restore_document(old));
                                        if let Some((_, funnel_id, base_revision_id, document_json)) = request.clone() {
                                            let draft = V2LocalDraft { version: 1, funnel_id, base_revision_id, document_json };
                                            spawn(async move { let _ = write_v2_local_draft(&draft).await; });
                                        }
                                        schedule_v2_preview(state, request);
                                    }
                                },
                                "Restore this revision as a new draft"
                            }
                            button {
                                class: "text-button",
                                r#type: "button",
                                onclick: move |_| state.with_mut(|state| {
                                    state.compare_document = None;
                                    state.compare_revision_id = None;
                                }),
                                "Close comparison"
                            }
                        }
                    }
                    if view.compare_loading {
                        p { role: "status", "Loading the selected revision…" }
                    }
                    div { class: "editor-revision-list", role: "list", "aria-label": "Saved revisions",
                        for revision in view.revisions.clone() {
                            {
                                let revision_id = revision.revision_id.clone();
                                rsx! {
                                    div { class: "editor-revision-row", role: "listitem",
                                        div {
                                            strong { "{revision.revision_id}" }
                                            span { "Schema v{revision.schema_version} · {revision.created_at}" }
                                        }
                                        button {
                                            class: "secondary-button",
                                            r#type: "button",
                                            disabled: revision.revision_id == view.base_revision_id || view.compare_loading,
                                            onclick: move |_| {
                                                let revision_id = revision_id.clone();
                                                spawn(async move { compare_v2_revision(state, revision_id).await; });
                                            },
                                            "Compare"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                p { class: "sr-only", role: "status", "aria-live": "polite", "aria-atomic": "true", "{view.announcement}" }
            }
        }
    }
}

#[cfg(feature = "browser")]
#[component]
fn V2MediaLibrary(
    state: Signal<V2EditorState>,
    add_block_location: Option<(SectionId, Option<BlockId>)>,
    locked: bool,
) -> Element {
    let view = state();
    let selected_asset = view.selected_media_asset.clone();
    let refresh_funnel_id = view.funnel_id.clone();
    let refresh_route_epoch = view.route_epoch;
    rsx! {
        fieldset { class: "editor-setting-group editor-media-library",
            legend { "Media library" }
            p { class: "editor-keyboard-help", "Upload PNG, JPEG, or WebP images up to 8 MiB, then add one to the selected section." }
            p { class: "editor-media-privacy", role: "note", "Uploaded images are publicly retrievable by their content-addressed URL. Do not upload private images." }
            if let Some(error) = view.media_error.as_ref() {
                p { class: "error-message", role: "alert", "{error}" }
            }
            if view.media_loading {
                p { role: "status", "Loading images…" }
            } else if view.media_assets.is_empty() {
                p { class: "editor-keyboard-help", "No images yet. Upload an image to start your library." }
            } else {
                div { class: "editor-media-grid", role: "list", "aria-label": "Media images",
                    for asset in view.media_assets.clone() {
                        {
                            let asset_id = asset.id.clone();
                            let short_id = asset.id.chars().take(8).collect::<String>();
                            let selected = selected_asset.as_deref() == Some(asset_id.as_str());
                            let source = format!("/media/{}", asset.id);
                            rsx! {
                                div { class: "editor-media-card", role: "listitem",
                                    button {
                                        class: "editor-media-select",
                                        r#type: "button",
                                        disabled: locked,
                                        "aria-pressed": "{selected}",
                                        "aria-label": "Select image {short_id}",
                                        onclick: move |_| state.with_mut(|state| {
                                            state.selected_media_asset = Some(asset_id.clone());
                                        }),
                                        img { src: "{source}", alt: "", loading: "lazy" }
                                        span { "Image {short_id}" }
                                    }
                                    small { "{media_size_label(asset.size_bytes)} · {asset.mime}" }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "editor-media-actions",
                label { class: "secondary-button editor-media-upload",
                    if view.media_uploading { "Uploading…" } else { "Upload image" }
                    input {
                        id: "v2-media-upload",
                        r#type: "file",
                        accept: "image/png,image/jpeg,image/webp",
                        disabled: locked || view.media_uploading,
                        onchange: move |_| { spawn(async move { upload_v2_media(state).await; }); },
                    }
                }
                button {
                    class: "secondary-button",
                    r#type: "button",
                    disabled: view.media_loading,
                    onclick: move |_| {
                        let funnel_id = refresh_funnel_id.clone();
                        spawn(async move { load_v2_media_assets(state, funnel_id, refresh_route_epoch).await; });
                    },
                    "Refresh library"
                }
                button {
                    class: "secondary-button",
                    r#type: "button",
                    disabled: locked || selected_asset.is_none() || add_block_location.is_none(),
                    onclick: move |_| {
                        let (Some(asset_id), Some((section, after))) = (
                            state.with(|state| state.selected_media_asset.clone()),
                            add_block_location.clone(),
                        ) else { return; };
                        if let Ok(asset_id) = AssetId::parse(asset_id) {
                            dispatch_v2_edit(state, Edit::AddBlock {
                                section,
                                after,
                                block: NewBlock::Image { asset_id, alt: ImageAlt::Decorative },
                            });
                        }
                    },
                    "Add selected image to section"
                }
            }
            if add_block_location.is_none() {
                p { class: "editor-keyboard-help", "Select a section before adding an image." }
            }
        }
    }
}

#[cfg(feature = "browser")]
#[component]
fn V2ImageBlockEditor(state: Signal<V2EditorState>, block: BlockV2, locked: bool) -> Element {
    let BlockV2::Image { id, asset_id, alt } = block else {
        return rsx! {};
    };
    let block_id = id.clone();
    let assets = state.with(|state| state.media_assets.clone());
    let current_asset_id = asset_id.as_str().to_owned();
    let alt_text = match &alt {
        ImageAlt::Text { value } => value.clone(),
        ImageAlt::Decorative => String::new(),
    };
    let alt_kind = if matches!(alt, ImageAlt::Decorative) {
        "decorative"
    } else {
        "text"
    };
    let block_for_asset = BlockV2::Image {
        id,
        asset_id: asset_id.clone(),
        alt: alt.clone(),
    };
    let block_for_alt = block_for_asset.clone();
    let block_for_kind = block_for_asset.clone();
    let asset_for_kind = asset_id.clone();
    let asset_for_alt = asset_id;
    let alt_text_for_kind = alt_text.clone();
    let assets_empty = assets.is_empty();
    let current_asset_is_available = assets.iter().any(|asset| asset.id == current_asset_id);
    rsx! {
        div { class: "editor-image-settings",
            label { class: "editor-setting-control",
                "Image source"
                select {
                    value: "{current_asset_id}",
                    disabled: locked || assets_empty,
                    "aria-label": "Image source for {block_id.as_str()}",
                    onchange: move |event| {
                        if let Ok(next_asset_id) = AssetId::parse(event.value()) {
                            if let Some(value) = block_value_with_image(&block_for_asset, next_asset_id, alt.clone()) {
                                dispatch_v2_edit(state, Edit::SetBlock { block: block_for_asset.id().clone(), value });
                            }
                        }
                    },
                    if !current_asset_is_available {
                        option { value: "{current_asset_id}", "Unavailable image" }
                    }
                    for asset in assets.iter() {
                        option { value: "{asset.id}", "{asset.mime} · {media_size_label(asset.size_bytes)}" }
                    }
                }
            }
            label { class: "editor-setting-control",
                "Alternative text"
                select {
                    value: "{alt_kind}",
                    disabled: locked,
                    "aria-label": "Alternative text type for {block_id.as_str()}",
                    onchange: move |event| {
                        let next_alt = if event.value() == "decorative" {
                            Some(ImageAlt::Decorative)
                        } else if event.value() == "text" {
                            Some(ImageAlt::Text { value: if alt_text_for_kind.is_empty() { "Describe this image".to_owned() } else { alt_text_for_kind.clone() } })
                        } else { None };
                        if let Some(next_alt) = next_alt {
                            if let Some(value) = block_value_with_image(&block_for_kind, asset_for_kind.clone(), next_alt) {
                                dispatch_v2_edit(state, Edit::SetBlock { block: block_for_kind.id().clone(), value });
                            }
                        }
                    },
                    option { value: "text", "Meaningful image" }
                    option { value: "decorative", "Decorative image" }
                }
            }
            label { class: "editor-setting-control",
                "Text alternative"
                input {
                    r#type: "text",
                    value: "{alt_text}",
                    maxlength: "8192",
                    disabled: locked || alt_kind == "decorative",
                    "aria-label": "Alternative text for {block_id.as_str()}",
                    onchange: move |event| {
                        let value = event.value();
                        if !value.trim().is_empty() {
                            if let Some(next) = block_value_with_image(&block_for_alt, asset_for_alt.clone(), ImageAlt::Text { value }) {
                                dispatch_v2_edit(state, Edit::SetBlock { block: block_for_alt.id().clone(), value: next });
                            }
                        }
                    },
                }
            }
            p { class: "editor-keyboard-help", "Choose decorative for visual flourish; meaningful images need a concise text alternative." }
        }
    }
}

#[cfg(feature = "browser")]
fn media_size_label(size_bytes: u64) -> String {
    if size_bytes >= 1024 * 1024 {
        format!("{:.1} MiB", size_bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} KiB", size_bytes.div_ceil(1024))
    }
}

#[cfg(feature = "browser")]
fn block_value_with_image(block: &BlockV2, asset_id: AssetId, alt: ImageAlt) -> Option<NewBlock> {
    matches!(block, BlockV2::Image { .. }).then_some(NewBlock::Image { asset_id, alt })
}

#[cfg(feature = "browser")]
#[component]
fn V2BlockEditor(
    state: Signal<V2EditorState>,
    page_options: Vec<(String, String)>,
    section_id: SectionId,
    block: BlockV2,
    index: usize,
    block_ids: Vec<BlockId>,
    locked: bool,
) -> Element {
    let block_id = block.id().clone();
    let text = block_text(&block);
    let form = match &block {
        BlockV2::Form { form_id, .. } => state.with(|state| {
            state
                .document
                .as_ref()
                .and_then(|document| document.form(form_id).cloned())
        }),
        _ => None,
    };
    let label = block_kind_label(&block);
    let control_label = format!("{label} content");
    let section_up = section_id.clone();
    let section_down = section_id.clone();
    let section_duplicate = section_id.clone();
    let section_delete = section_id;
    let id_up = block_id.clone();
    let id_down = block_id.clone();
    let id_duplicate = block_id.clone();
    let id_delete = block_id.clone();
    let block_for_text_edit = block.clone();
    let block_for_destination_type = block.clone();
    let block_for_page_edit = block.clone();
    let block_for_url_edit = block.clone();
    let button_destination = match &block {
        BlockV2::Button { action, .. } => Some(action.clone()),
        _ => None,
    };
    let block_count = block_ids.len();
    let block_ids_up = block_ids.clone();
    let block_ids_down = block_ids;
    rsx! {
        div { class: "editor-block-row", role: "listitem",
            div { class: "editor-block-heading",
                strong { "{label}" }
                code { "{block.id().as_str()}" }
            }
            if matches!(block, BlockV2::Image { .. }) {
                V2ImageBlockEditor { state, block: block.clone(), locked }
            } else if let Some(form) = form {
                V2FormEditor { state, form, locked }
            } else {
                label { class: "editor-setting-control",
                    "{control_label}"
                    textarea {
                        rows: if matches!(block, BlockV2::List { .. }) { "4" } else { "3" },
                        value: "{text}",
                        disabled: locked,
                        "aria-label": "{control_label} for {block.id().as_str()}",
                        onchange: move |event| {
                            if let Some(value) = block_value_with_text(&block_for_text_edit, event.value()) {
                                dispatch_v2_edit(state, Edit::SetBlock { block: block_for_text_edit.id().clone(), value });
                            }
                        },
                    }
                }
            }
            if let (BlockV2::Button { .. }, Some(action)) = (&block, button_destination) {
                {
                    let action_kind = match action {
                        ActionV2::NavigatePage { .. } => "page",
                        ActionV2::ExternalHttps { .. } => "external",
                    };
                    let type_page_options = page_options.clone();
                    rsx! {
                        label { class: "editor-setting-control",
                            "Button destination type"
                            select {
                                value: "{action_kind}",
                                disabled: locked,
                                "aria-label": "Button destination type for {block.id().as_str()}",
                                onchange: move |event| {
                                    let next_action = match event.value().as_str() {
                                        "page" => type_page_options.first().and_then(|(id, _)| PageId::parse(id.clone()).ok()).map(|page_id| ActionV2::NavigatePage { page_id }),
                                        "external" => Some(ActionV2::ExternalHttps { url: "https://example.invalid".to_owned() }),
                                        _ => None,
                                    };
                                    if let Some(next_action) = next_action {
                                        if let Some(value) = block_value_with_action(&block_for_destination_type, next_action) {
                                            dispatch_v2_edit(state, Edit::SetBlock { block: block_for_destination_type.id().clone(), value });
                                        }
                                    }
                                },
                                option { value: "page", "Internal page" }
                                option { value: "external", "External HTTPS URL" }
                            }
                        }
                        match action {
                            ActionV2::NavigatePage { page_id } => {
                                let selected_page = page_id.as_str().to_owned();
                                rsx! {
                                    label { class: "editor-setting-control",
                                        "Destination page"
                                        select {
                                            value: "{selected_page}",
                                            disabled: locked,
                                            "aria-label": "Destination page for button {block.id().as_str()}",
                                            onchange: move |event| {
                                                if let Ok(page_id) = PageId::parse(event.value()) {
                                                    if let Some(value) = block_value_with_action(&block_for_page_edit, ActionV2::NavigatePage { page_id }) {
                                                        dispatch_v2_edit(state, Edit::SetBlock { block: block_for_page_edit.id().clone(), value });
                                                    }
                                                }
                                            },
                                            for (page_id, title) in page_options.clone() {
                                                option { value: "{page_id}", "{title}" }
                                            }
                                        }
                                    }
                                }
                            }
                            ActionV2::ExternalHttps { url } => {
                                rsx! {
                                    label { class: "editor-setting-control",
                                        "Destination URL"
                                        input {
                                            r#type: "url",
                                            value: "{url}",
                                            disabled: locked,
                                            "aria-label": "Destination URL for button {block.id().as_str()}",
                                            onchange: move |event| {
                                                if let Some(value) = block_value_with_action(&block_for_url_edit, ActionV2::ExternalHttps { url: event.value() }) {
                                                    dispatch_v2_edit(state, Edit::SetBlock { block: block_for_url_edit.id().clone(), value });
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "editor-section-actions",
                button {
                    class: "editor-move-button",
                    r#type: "button",
                    disabled: locked || index == 0,
                    "aria-label": "Move {label} up",
                    onclick: move |_| {
                        let mut ordered = block_ids_up.clone();
                        if index > 0 { ordered.swap(index, index - 1); }
                        dispatch_v2_edit(state, Edit::ReorderBlocks { section: section_up.clone(), ordered });
                        let _ = &id_up;
                    },
                    "Up"
                }
                button {
                    class: "editor-move-button",
                    r#type: "button",
                    disabled: locked || index + 1 >= block_count,
                    "aria-label": "Move {label} down",
                    onclick: move |_| {
                        let mut ordered = block_ids_down.clone();
                        if index + 1 < ordered.len() { ordered.swap(index, index + 1); }
                        dispatch_v2_edit(state, Edit::ReorderBlocks { section: section_down.clone(), ordered });
                        let _ = &id_down;
                    },
                    "Down"
                }
                button {
                    class: "editor-move-button",
                    r#type: "button",
                    disabled: locked,
                    "aria-label": "Duplicate {label}",
                    onclick: move |_| dispatch_v2_edit(state, Edit::DuplicateBlock { section: section_duplicate.clone(), block: id_duplicate.clone() }),
                    "Duplicate"
                }
                button {
                    class: "editor-move-button",
                    r#type: "button",
                    disabled: locked,
                    "aria-label": "Delete {label}",
                    onclick: move |_| dispatch_v2_edit(state, Edit::DeleteBlock { section: section_delete.clone(), block: id_delete.clone() }),
                    "Delete"
                }
            }
        }
    }
}

#[cfg(feature = "browser")]
fn section_kind_id(kind: SectionKind) -> &'static str {
    match kind {
        SectionKind::Hero => "hero",
        SectionKind::Features => "features",
        SectionKind::Testimonials => "testimonials",
        SectionKind::Cta => "cta",
        SectionKind::Footer => "footer",
    }
}

#[cfg(feature = "browser")]
fn next_page_defaults(document: &DocumentV2) -> Option<(String, String)> {
    (2..=50).find_map(|number| {
        let slug = format!("step-{number}");
        (!document.pages().iter().any(|page| page.slug() == slug)).then(|| {
            let title = match document.public_language() {
                PublicLanguage::English => format!("Step {number}"),
                PublicLanguage::French => format!("Étape {number}"),
            };
            (title, slug)
        })
    })
}

#[cfg(feature = "browser")]
fn page_is_referenced(document: &DocumentV2, page_id: &PageId) -> bool {
    document.pages().iter().any(|page| {
        page.sections().iter().any(|section| {
            section.blocks().iter().any(|block| {
                matches!(
                    block,
                    BlockV2::Button {
                        action: ActionV2::NavigatePage { page_id: target },
                        ..
                    } if target == page_id
                )
            })
        })
    }) || document.forms().iter().any(|form| {
        matches!(
            form.result(),
            FormResult::SuccessPage { page_id: target } if target == page_id
        )
    })
}

#[cfg(feature = "browser")]
#[component]
fn V2ThemeCustomization(
    state: Signal<V2EditorState>,
    document: DocumentV2,
    locked: bool,
) -> Element {
    let overrides = document.theme().overrides().clone();
    let profile = document.resolved_theme_profile();
    let warnings = theme_profile_contrast_warnings(&profile);
    let contrast_message = if warnings.is_empty() {
        "Current text and button colors meet the 4.5:1 contrast target across desktop, tablet, and mobile.".to_owned()
    } else {
        format!(
            "Low contrast: {}. Adjust these colors to improve readability.",
            warnings.join(", ")
        )
    };
    let contrast_class = if warnings.is_empty() {
        "theme-contrast-status"
    } else {
        "theme-contrast-status theme-contrast-warning"
    };
    rsx! {
        fieldset { class: "editor-setting-group theme-customization",
            legend { "Customize theme" }
            p { class: "editor-keyboard-help", "These settings update the live preview. Save revision when the design is ready." }
            fieldset { class: "theme-token-group",
                legend { "Colors" }
                p { class: "editor-keyboard-help", "Choose colors for each part of the page. Reset any value to use the selected preset." }
                div { class: "theme-color-grid",
                    for field in ThemeColorField::ALL {
                        {
                            let value = profile.desktop.get(field.token()).cloned().unwrap_or_else(|| "#ffffff".to_owned());
                            let has_override = field.override_value(overrides.colors.as_ref()).is_some();
                            rsx! {
                                V2ThemeColorControl { state, field, value, has_override, locked }
                            }
                        }
                    }
                }
                p {
                    class: "{contrast_class}",
                    role: "status",
                    "aria-live": "polite",
                    "{contrast_message}"
                }
            }
            fieldset { class: "theme-token-group",
                legend { "Type and layout" }
                div { class: "theme-style-grid",
                    for field in ThemeStyleField::ALL {
                        V2ThemeStyleControl { state, field, overrides: overrides.clone(), locked }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "browser")]
#[component]
fn V2ThemeColorControl(
    state: Signal<V2EditorState>,
    field: ThemeColorField,
    value: String,
    has_override: bool,
    locked: bool,
) -> Element {
    let input_id = format!("v2-theme-color-{}", field.id());
    rsx! {
        div { class: "theme-color-control",
            label { r#for: "{input_id}", "{field.label()}" }
            div { class: "theme-color-input-row",
                input {
                    id: "{input_id}",
                    r#type: "color",
                    value: "{value}",
                    disabled: locked,
                    onchange: move |event| set_theme_color(state, field, Some(event.value())),
                }
                button {
                    class: "text-button",
                    r#type: "button",
                    disabled: locked || !has_override,
                    "aria-label": "Use preset {field.label().to_lowercase()}",
                    onclick: move |_| set_theme_color(state, field, None),
                    "Use preset"
                }
            }
        }
    }
}

#[cfg(feature = "browser")]
#[component]
fn V2ThemeStyleControl(
    state: Signal<V2EditorState>,
    field: ThemeStyleField,
    overrides: ThemeOverrides,
    locked: bool,
) -> Element {
    let input_id = format!("v2-theme-{}", field.id());
    let value = field.override_id(&overrides).unwrap_or("preset");
    rsx! {
        label { class: "theme-style-control", r#for: "{input_id}",
            span { "{field.label()}" }
            select {
                id: "{input_id}",
                value: "{value}",
                disabled: locked,
                onchange: move |event| set_theme_style(state, field, &event.value()),
                option { value: "preset", "Preset default" }
                for (choice, label) in field.choices() {
                    option { value: "{choice}", "{label}" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eez_domain::document_v2::{BlockId, BlueprintId, IdKind, IdSource};
    use eez_domain::ThemePreset;

    struct TestIds(u32);

    impl IdSource for TestIds {
        fn next_id(
            &mut self,
            _kind: IdKind,
        ) -> Result<String, eez_domain::document_v2::DocumentError> {
            self.0 += 1;
            Ok(format!("test-{}", self.0))
        }
    }

    fn document() -> DocumentV2 {
        DocumentV2::starter("funnel-a", BlueprintId::Tripwire, ThemePreset::Minimal)
            .expect("valid V2 starter")
    }

    #[test]
    fn button_action_edit_preserves_its_label() {
        let button = BlockV2::Button {
            id: BlockId::parse("test-button").expect("valid button ID"),
            label: "Contact the service provider".to_owned(),
            action: ActionV2::ExternalHttps {
                url: "https://service.example.test/contact".to_owned(),
            },
        };
        let expected_label = match &button {
            BlockV2::Button { label, .. } => label.clone(),
            _ => unreachable!("selected block is a button"),
        };
        let NewBlock::Button { label, action } = block_value_with_action(
            &button,
            ActionV2::ExternalHttps {
                url: "https://example.invalid".to_owned(),
            },
        )
        .expect("button accepts destination edits") else {
            panic!("button action edit remains a button");
        };

        assert_eq!(label, expected_label);
        assert_eq!(
            action,
            ActionV2::ExternalHttps {
                url: "https://example.invalid".to_owned()
            }
        );
    }

    #[test]
    fn comparison_reports_added_and_changed_sections_and_theme() {
        let original = document();
        let page = original.pages()[0].id().clone();
        let mut ids = TestIds(0);
        let changed = original
            .apply(
                Edit::AddSection {
                    page,
                    after: None,
                    section_type: SectionKind::Features,
                },
                &mut ids,
            )
            .expect("valid section addition");
        let lines = revision_comparison(&changed.document, &original);
        assert!(lines.iter().any(|line| line.starts_with("Section added:")));
        let with_theme = changed
            .document
            .apply(
                Edit::SetThemePreset {
                    preset: ThemePreset::Elegant,
                },
                &mut ids,
            )
            .expect("valid theme change");
        assert!(revision_comparison(&with_theme.document, &original)
            .iter()
            .any(|line| line == "The theme changed."));
    }

    #[test]
    fn responsive_contrast_report_catches_low_contrast_on_mobile() {
        let compliant = BTreeMap::from([
            ("color-background".to_owned(), "#ffffff".to_owned()),
            ("color-border".to_owned(), "#cbd5e1".to_owned()),
            ("color-muted".to_owned(), "#666666".to_owned()),
            ("color-on-primary".to_owned(), "#ffffff".to_owned()),
            ("color-primary".to_owned(), "#2c5282".to_owned()),
            ("color-surface".to_owned(), "#ffffff".to_owned()),
            ("color-text".to_owned(), "#111111".to_owned()),
        ]);
        let low_contrast_mobile = BTreeMap::from([
            ("color-background".to_owned(), "#ffffff".to_owned()),
            ("color-border".to_owned(), "#cbd5e1".to_owned()),
            ("color-muted".to_owned(), "#666666".to_owned()),
            ("color-on-primary".to_owned(), "#ffffff".to_owned()),
            ("color-primary".to_owned(), "#ffffff".to_owned()),
            ("color-surface".to_owned(), "#ffffff".to_owned()),
            ("color-text".to_owned(), "#111111".to_owned()),
        ]);
        let profile = ResolvedThemeProfile {
            profile_version: "theme-profile-v1".to_owned(),
            desktop: compliant.clone(),
            tablet: compliant,
            mobile: low_contrast_mobile,
        };

        assert_eq!(
            theme_profile_contrast_warnings(&profile),
            vec!["Mobile: Button text (1.00:1)".to_owned()]
        );
    }
}
