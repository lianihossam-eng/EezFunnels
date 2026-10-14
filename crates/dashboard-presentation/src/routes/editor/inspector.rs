use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(super) struct EditorSettingsSchema {
    pub(super) version: u16,
    pub(super) fields: Vec<EditorSettingField>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(super) struct EditorSettingField {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) control: String,
    pub(super) section_id: Option<String>,
    #[serde(default)]
    pub(super) options: Vec<EditorSettingOption>,
    #[serde(default)]
    pub(super) multiline: bool,
    #[serde(default)]
    pub(super) optional: bool,
    #[serde(default)]
    pub(super) nullable: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(super) struct EditorSettingOption {
    pub(super) value: String,
    pub(super) label: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(super) struct EditorSettingValue {
    pub(super) kind: String,
    pub(super) value: serde_json::Value,
    #[serde(default, skip_serializing)]
    pub(super) effective_value: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct EditorSettingChange {
    pub(super) id: String,
    pub(super) value: EditorSettingValue,
}

impl EditorState {
    pub(super) fn set_setting(
        &mut self,
        field_id: &str,
        value: EditorSettingValue,
    ) -> Option<EditorPreviewCapture> {
        if self.saving || self.retryable || self.stale {
            return None;
        }
        let document = self.document.as_mut()?;
        let field = document
            .settings_schema
            .fields
            .iter()
            .find(|field| field.id == field_id)?;
        if field.control != value.kind
            || !value_matches_control(&value)
            || (value.value.is_null() && !field.nullable)
        {
            return None;
        }
        let mut value = value;
        if value.kind == "color" {
            value.effective_value = document
                .settings_values
                .get(field_id)
                .and_then(|current| current.effective_value.clone());
        }
        document
            .settings_values
            .insert(field_id.to_owned(), value.clone());
        if document
            .saved_settings_values
            .get(field_id)
            .is_some_and(|saved| setting_value_intent_eq(saved, &value))
        {
            document.settings_changes.remove(field_id);
        } else {
            document.settings_changes.insert(field_id.to_owned(), value);
        }
        Some(self.begin_preview())
    }

    pub(super) fn set_image_property(
        &mut self,
        field_id: &str,
        property: &str,
        value: String,
    ) -> Option<EditorPreviewCapture> {
        let document = self.document.as_ref()?;
        let current = document.settings_values.get(field_id)?;
        if current.kind != "image" || !matches!(property, "url" | "alt") {
            return None;
        }
        let current_value = &current.value;
        let current_url = current_value
            .get("url")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let current_alt = current_value
            .get("alt")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let (url, alt) = if property == "url" {
            (value.as_str(), current_alt)
        } else {
            (current_url, value.as_str())
        };
        let image_value = if url.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::json!({ "url": url, "alt": alt })
        };
        self.set_setting(
            field_id,
            EditorSettingValue {
                kind: "image".to_owned(),
                value: image_value,
                effective_value: None,
            },
        )
    }
}

pub(super) fn setting_changes(
    changes: &BTreeMap<String, EditorSettingValue>,
) -> Vec<EditorSettingChange> {
    changes
        .iter()
        .map(|(id, value)| EditorSettingChange {
            id: id.clone(),
            value: value.clone(),
        })
        .collect()
}

fn value_matches_control(value: &EditorSettingValue) -> bool {
    match value.kind.as_str() {
        "text" | "color" => value.value.is_string() || value.value.is_null(),
        "select" => value.value.is_string(),
        "toggle" => value.value.is_boolean(),
        "image" => value.value.is_null() || value.value.is_object(),
        _ => false,
    }
}

fn setting_value_intent_eq(left: &EditorSettingValue, right: &EditorSettingValue) -> bool {
    left.kind == right.kind && left.value == right.value
}

pub(super) fn text_setting_value(text: String, nullable: bool) -> EditorSettingValue {
    let value = if nullable && text.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::json!(text)
    };
    EditorSettingValue {
        kind: "text".to_owned(),
        value,
        effective_value: None,
    }
}

pub(super) fn color_control_value(value: &EditorSettingValue) -> String {
    value
        .value
        .as_str()
        .filter(|value| !value.is_empty())
        .or(value.effective_value.as_deref())
        .unwrap_or("#000000")
        .to_owned()
}

#[cfg(feature = "browser")]
#[derive(Deserialize)]
pub(super) struct EditorPreviewResponse {
    pub(super) preview_html: String,
    pub(super) settings_values: BTreeMap<String, EditorSettingValue>,
}

#[component]
pub(super) fn EditorInspector(state: Signal<EditorState>) -> Element {
    let view = state();
    let Some(document) = view.document.as_ref() else {
        return rsx! { div { class: "editor-inspector-panel" } };
    };
    let selected_section = document.selected_section.clone();
    let selected_label = selected_section
        .as_deref()
        .map(|section_id| view.section_name(section_id))
        .unwrap_or_else(|| "Section".to_owned());
    let locked = view.saving || view.retryable || view.stale;
    let theme_fields = document
        .settings_schema
        .fields
        .iter()
        .filter(|field| field.section_id.is_none())
        .cloned()
        .collect::<Vec<_>>();
    let section_fields = document
        .settings_schema
        .fields
        .iter()
        .filter(|field| field.section_id.as_deref() == selected_section.as_deref())
        .cloned()
        .collect::<Vec<_>>();
    rsx! {
        div { class: "editor-inspector-panel", "aria-label": "Settings inspector",
            div { class: "section-heading",
                h2 { "Inspector" }
                span { "Schema v{document.settings_schema.version}" }
            }
            fieldset { class: "editor-setting-group",
                legend { "Theme" }
                for field in theme_fields {
                    if let Some(value) = document.settings_values.get(&field.id) {
                        EditorSettingControl { state, field, value: value.clone(), locked }
                    }
                }
            }
            if let Some(section_id) = selected_section {
                fieldset { class: "editor-setting-group editor-section-settings",
                    legend { "{selected_label}" }
                    for field in section_fields {
                        if let Some(value) = document.settings_values.get(&field.id) {
                            EditorSettingControl { state, field, value: value.clone(), locked }
                        }
                    }
                }
                p { class: "editor-selected-section", "Editing section: {section_id}" }
            } else {
                p { class: "editor-keyboard-help", "Choose a section to edit its settings." }
            }
        }
    }
}

#[component]
fn EditorSettingControl(
    state: Signal<EditorState>,
    field: EditorSettingField,
    value: EditorSettingValue,
    locked: bool,
) -> Element {
    let id = field.id.clone();
    let text_value = value.value.as_str().unwrap_or_default().to_owned();
    let image_url = value
        .value
        .get("url")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let image_alt = value
        .value
        .get("alt")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let boolean_value = value.value.as_bool().unwrap_or(false);
    let field_id_for_textarea = id.clone();
    let field_id_for_text_input = id.clone();
    let field_id_for_color_input = id.clone();
    let field_id_for_color_reset = id.clone();
    let field_id_for_select = id.clone();
    let field_id_for_toggle = id.clone();
    let field_id_for_image_url = id.clone();
    let field_id_for_image_alt = id.clone();
    let field_id_for_remove = id.clone();
    let control = field.control.clone();
    let nullable_text = field.nullable;
    let color_value = color_control_value(&value);
    rsx! {
        div { class: "editor-setting-control", "data-setting-id": "{id}",
            if field.optional {
                span { class: "editor-setting-optional", "Optional" }
            }
            if control == "text" && field.multiline {
                label { r#for: "setting-{id}", "{field.label}" }
                textarea {
                    id: "setting-{id}",
                    rows: "3",
                    value: "{text_value}",
                    disabled: locked,
                    oninput: move |event| {
                        let next = text_setting_value(event.value(), nullable_text);
                        let capture = state.with_mut(|state| state.set_setting(&field_id_for_textarea, next));
                        schedule_editor_preview(state, capture);
                    },
                }
            } else if control == "text" {
                label { r#for: "setting-{id}", "{field.label}" }
                input {
                    id: "setting-{id}",
                    r#type: "text",
                    value: "{text_value}",
                    disabled: locked,
                    oninput: move |event| {
                        let next = text_setting_value(event.value(), nullable_text);
                        let capture = state.with_mut(|state| state.set_setting(&field_id_for_text_input, next));
                        schedule_editor_preview(state, capture);
                    },
                }
            } else if control == "color" {
                label { r#for: "setting-{id}", "{field.label}" }
                div { class: "editor-color-control",
                    input {
                        id: "setting-{id}",
                        r#type: "color",
                        value: "{color_value}",
                        disabled: locked,
                        oninput: move |event| {
                            let next = EditorSettingValue { kind: "color".to_owned(), value: serde_json::json!(event.value()), effective_value: None };
                            let capture = state.with_mut(|state| state.set_setting(&field_id_for_color_input, next));
                            schedule_editor_preview(state, capture);
                        },
                    }
                    button {
                        class: "secondary-button editor-reset-color",
                        r#type: "button",
                        disabled: locked,
                        onclick: move |_| {
                            let next = EditorSettingValue { kind: "color".to_owned(), value: serde_json::Value::Null, effective_value: None };
                            let capture = state.with_mut(|state| state.set_setting(&field_id_for_color_reset, next));
                            schedule_editor_preview(state, capture);
                        },
                        "Reset to preset"
                    }
                }
            } else if control == "select" {
                label { r#for: "setting-{id}", "{field.label}" }
                select {
                    id: "setting-{id}",
                    value: "{text_value}",
                    disabled: locked,
                    onchange: move |event| {
                        let next = EditorSettingValue { kind: "select".to_owned(), value: serde_json::json!(event.value()), effective_value: None };
                        let capture = state.with_mut(|state| state.set_setting(&field_id_for_select, next));
                        schedule_editor_preview(state, capture);
                    },
                    for option in field.options.iter() {
                        option { value: "{option.value}", "{option.label}" }
                    }
                }
            } else if control == "toggle" {
                label { class: "editor-toggle-control", r#for: "setting-{id}",
                    input {
                        id: "setting-{id}",
                        r#type: "checkbox",
                        checked: boolean_value,
                        disabled: locked,
                        onchange: move |event| {
                            let next = EditorSettingValue { kind: "toggle".to_owned(), value: serde_json::json!(event.checked()), effective_value: None };
                            let capture = state.with_mut(|state| state.set_setting(&field_id_for_toggle, next));
                            schedule_editor_preview(state, capture);
                        },
                    }
                    "{field.label}"
                }
            } else if control == "image" {
                label { r#for: "setting-{id}-url", "{field.label} URL" }
                input {
                    id: "setting-{id}-url",
                    r#type: "url",
                    value: "{image_url}",
                    placeholder: "https://example.com/image.jpg",
                    disabled: locked,
                    oninput: move |event| {
                        let capture = state.with_mut(|state| state.set_image_property(&field_id_for_image_url, "url", event.value()));
                        schedule_editor_preview(state, capture);
                    },
                }
                label { r#for: "setting-{id}-alt", "Alternative text" }
                input {
                    id: "setting-{id}-alt",
                    r#type: "text",
                    value: "{image_alt}",
                    disabled: locked || image_url.is_empty(),
                    oninput: move |event| {
                        let capture = state.with_mut(|state| state.set_image_property(&field_id_for_image_alt, "alt", event.value()));
                        schedule_editor_preview(state, capture);
                    },
                }
                button {
                    class: "secondary-button editor-remove-image",
                    r#type: "button",
                    disabled: locked || (image_url.is_empty() && image_alt.is_empty()),
                    onclick: move |_| {
                        let capture = state.with_mut(|state| state.set_setting(&field_id_for_remove, EditorSettingValue { kind: "image".to_owned(), value: serde_json::Value::Null, effective_value: None }));
                        schedule_editor_preview(state, capture);
                    },
                    "Remove image"
                }
                p { class: "editor-image-preview-note", "The preview shows a safe placeholder; saved pages display the image." }
            }
        }
    }
}
