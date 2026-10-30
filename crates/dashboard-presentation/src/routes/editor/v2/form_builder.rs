use super::orchestration::dispatch_v2_edit;
use super::state::{DeterministicIds, V2EditorState};
use dioxus::prelude::*;
use eez_domain::document_v2::{
    DocumentV2, Edit, FormField, FormPresentation, FormResult, FormV2, NewFormField, PageId,
    PageV2, PublicLanguage, TextPresentation, ValidationRange,
};

#[cfg(feature = "browser")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum V2NewFormFieldKind {
    #[default]
    Text,
    Email,
    Phone,
    Checkbox,
    Select,
}

#[cfg(feature = "browser")]
impl V2NewFormFieldKind {
    const ALL: [Self; 5] = [
        Self::Text,
        Self::Email,
        Self::Phone,
        Self::Checkbox,
        Self::Select,
    ];

    const fn id(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Email => "email",
            Self::Phone => "phone",
            Self::Checkbox => "checkbox",
            Self::Select => "select",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Email => "Email",
            Self::Phone => "Phone",
            Self::Checkbox => "Checkbox",
            Self::Select => "Dropdown",
        }
    }

    fn from_id(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|item| item.id() == value)
    }

    fn create(self, form: &FormV2, language: PublicLanguage) -> NewFormField {
        let mut suffix = form.fields().len() + 1;
        let name = loop {
            let candidate = format!("field_{suffix}");
            if !form
                .fields()
                .iter()
                .any(|field| form_field_name(field) == candidate)
            {
                break candidate;
            }
            suffix += 1;
        };
        let label = match self {
            Self::Text => {
                super::public_default_text(language, "New text field", "Nouveau champ texte")
            }
            Self::Email => super::public_default_text(language, "Email address", "Adresse e-mail"),
            Self::Phone => {
                super::public_default_text(language, "Phone number", "Numéro de téléphone")
            }
            Self::Checkbox => super::public_default_text(language, "Checkbox", "Case à cocher"),
            Self::Select => {
                super::public_default_text(language, "Choose an option", "Choisissez une option")
            }
        };
        match self {
            Self::Text => NewFormField::Text {
                name,
                label,
                help: None,
                required: false,
                validation: None,
                presentation: TextPresentation::SingleLine,
            },
            Self::Email => NewFormField::Email {
                name,
                label,
                help: None,
                required: false,
                validation: None,
            },
            Self::Phone => NewFormField::Phone {
                name,
                label,
                help: None,
                required: false,
                validation: None,
            },
            Self::Checkbox => NewFormField::Checkbox {
                name,
                label,
                help: None,
                required: false,
            },
            Self::Select => NewFormField::Select {
                name,
                label,
                help: None,
                required: false,
                options: vec!["Option 1".to_owned()],
            },
        }
    }
}

#[cfg(feature = "browser")]
fn form_field_name(field: &FormField) -> &str {
    match field {
        FormField::Text { name, .. }
        | FormField::Email { name, .. }
        | FormField::Phone { name, .. }
        | FormField::Checkbox { name, .. }
        | FormField::Select { name, .. } => name,
    }
}

#[cfg(feature = "browser")]
fn form_field_label(field: &FormField) -> &str {
    match field {
        FormField::Text { label, .. }
        | FormField::Email { label, .. }
        | FormField::Phone { label, .. }
        | FormField::Checkbox { label, .. }
        | FormField::Select { label, .. } => label,
    }
}

#[cfg(feature = "browser")]
fn form_field_help(field: &FormField) -> Option<&str> {
    match field {
        FormField::Text { help, .. }
        | FormField::Email { help, .. }
        | FormField::Phone { help, .. }
        | FormField::Checkbox { help, .. }
        | FormField::Select { help, .. } => help.as_deref(),
    }
}

#[cfg(feature = "browser")]
fn form_field_required(field: &FormField) -> bool {
    match field {
        FormField::Text { required, .. }
        | FormField::Email { required, .. }
        | FormField::Phone { required, .. }
        | FormField::Checkbox { required, .. }
        | FormField::Select { required, .. } => *required,
    }
}

#[cfg(feature = "browser")]
fn form_field_kind_id(field: &FormField) -> &'static str {
    match field {
        FormField::Text { .. } => "text",
        FormField::Email { .. } => "email",
        FormField::Phone { .. } => "phone",
        FormField::Checkbox { .. } => "checkbox",
        FormField::Select { .. } => "select",
    }
}

#[cfg(feature = "browser")]
fn form_field_options(field: &FormField) -> Option<&[String]> {
    match field {
        FormField::Select { options, .. } => Some(options),
        _ => None,
    }
}

#[cfg(feature = "browser")]
fn form_field_validation(field: &FormField) -> Option<&ValidationRange> {
    match field {
        FormField::Text { validation, .. }
        | FormField::Email { validation, .. }
        | FormField::Phone { validation, .. } => validation.as_ref(),
        FormField::Checkbox { .. } | FormField::Select { .. } => None,
    }
}

#[cfg(feature = "browser")]
fn form_field_presentation(field: &FormField) -> Option<TextPresentation> {
    match field {
        FormField::Text { presentation, .. } => Some(*presentation),
        _ => None,
    }
}

#[cfg(feature = "browser")]
fn form_field_validation_cap(field: &FormField) -> Option<u32> {
    match field {
        FormField::Text { .. } => Some(8192),
        FormField::Email { .. } => Some(320),
        FormField::Phone { .. } => Some(64),
        FormField::Checkbox { .. } | FormField::Select { .. } => None,
    }
}

#[cfg(feature = "browser")]
fn form_setting_key(form_id: &str, setting: &str) -> String {
    format!("form:{form_id}:setting:{setting}")
}

#[cfg(feature = "browser")]
fn optional_form_presentation(
    submit_label: Option<String>,
    information_url: Option<String>,
) -> Option<FormPresentation> {
    (submit_label.is_some() || information_url.is_some()).then_some(FormPresentation {
        submit_label,
        information_url,
    })
}

#[cfg(feature = "browser")]
fn form_field_key(form_id: &str, field_id: &str, setting: &str) -> String {
    format!("form:{form_id}:field:{field_id}:{setting}")
}

#[cfg(feature = "browser")]
fn input_value(state: Signal<V2EditorState>, key: &str, fallback: &str) -> String {
    state.with(|state| {
        state
            .form_input_drafts
            .get(key)
            .cloned()
            .unwrap_or_else(|| fallback.to_owned())
    })
}

#[cfg(feature = "browser")]
fn input_error(state: Signal<V2EditorState>, key: &str) -> Option<String> {
    state.with(|state| state.form_input_errors.get(key).cloned())
}

#[cfg(feature = "browser")]
fn update_form_input(mut state: Signal<V2EditorState>, key: String, value: String) {
    state.with_mut(|state| {
        state.form_input_errors.remove(&key);
        state.form_input_drafts.insert(key, value);
    });
}

#[cfg(feature = "browser")]
fn reject_form_input(
    mut state: Signal<V2EditorState>,
    key: String,
    value: String,
    error: impl Into<String>,
) {
    state.with_mut(|state| {
        state.form_input_drafts.insert(key.clone(), value);
        state.form_input_errors.insert(key, error.into());
    });
}

#[cfg(feature = "browser")]
fn accept_form_input(mut state: Signal<V2EditorState>, key: String) {
    state.with_mut(|state| {
        state.form_input_drafts.remove(&key);
        state.form_input_errors.remove(&key);
    });
}

#[cfg(feature = "browser")]
fn pending_field_type(state: Signal<V2EditorState>, key: &str) -> Option<String> {
    state.with(|state| state.form_pending_field_types.get(key).cloned())
}

#[cfg(feature = "browser")]
fn set_pending_field_type(mut state: Signal<V2EditorState>, key: String, kind: String) {
    state.with_mut(|state| {
        state.form_pending_field_types.insert(key, kind);
    });
}

#[cfg(feature = "browser")]
fn clear_pending_field_type(mut state: Signal<V2EditorState>, key: String) {
    state.with_mut(|state| {
        state.form_pending_field_types.remove(&key);
    });
}

#[cfg(feature = "browser")]
fn form_success_pages(document: &DocumentV2, form: &FormV2) -> Vec<PageV2> {
    document
        .pages()
        .iter()
        .filter(|page| {
            let mut ids = DeterministicIds {
                operation_id: "form-success-page-option".to_owned(),
                next: 0,
            };
            document
                .apply(
                    Edit::SetFormResult {
                        form: form.id().clone(),
                        result: FormResult::SuccessPage {
                            page_id: page.id().clone(),
                        },
                    },
                    &mut ids,
                )
                .is_ok()
        })
        .cloned()
        .collect()
}

#[cfg(feature = "browser")]
fn validation_range_update(
    field: &FormField,
    min: &str,
    max: &str,
) -> Result<NewFormField, &'static str> {
    let parse = |value: &str| {
        if value.is_empty() {
            Some(None)
        } else {
            value.parse::<u32>().ok().map(Some)
        }
    };
    let min_length = parse(min).ok_or("Enter a whole number for the minimum length.")?;
    let max_length = parse(max).ok_or("Enter a whole number for the maximum length.")?;
    let cap =
        form_field_validation_cap(field).ok_or("Length rules are unavailable for this field.")?;
    if min_length.is_some_and(|value| value > cap)
        || max_length.is_some_and(|value| value == 0 || value > cap)
    {
        return Err("Length rules exceed the supported range for this field.");
    }
    if min_length
        .zip(max_length)
        .is_some_and(|(min, max)| min > max)
    {
        return Err("Minimum length must not exceed maximum length.");
    }
    let validation = (min_length.is_some() || max_length.is_some()).then_some(ValidationRange {
        min_length,
        max_length,
    });
    let name = form_field_name(field).to_owned();
    let label = form_field_label(field).to_owned();
    let help = form_field_help(field).map(str::to_owned);
    let required = form_field_required(field);
    Ok(match field {
        FormField::Text { .. } => NewFormField::Text {
            name,
            label,
            help,
            required,
            validation,
            presentation: form_field_presentation(field).unwrap_or_default(),
        },
        FormField::Email { .. } => NewFormField::Email {
            name,
            label,
            help,
            required,
            validation,
        },
        FormField::Phone { .. } => NewFormField::Phone {
            name,
            label,
            help,
            required,
            validation,
        },
        FormField::Checkbox { .. } | FormField::Select { .. } => {
            return Err("Length rules are unavailable for this field.");
        }
    })
}

#[cfg(feature = "browser")]
fn new_form_field_with_common(
    field: &FormField,
    name: String,
    label: String,
    help: Option<String>,
    required: bool,
) -> NewFormField {
    match field {
        FormField::Text { validation, .. } => NewFormField::Text {
            name,
            label,
            help,
            required,
            validation: validation.clone(),
            presentation: form_field_presentation(field).unwrap_or_default(),
        },
        FormField::Email { validation, .. } => NewFormField::Email {
            name,
            label,
            help,
            required,
            validation: validation.clone(),
        },
        FormField::Phone { validation, .. } => NewFormField::Phone {
            name,
            label,
            help,
            required,
            validation: validation.clone(),
        },
        FormField::Checkbox { .. } => NewFormField::Checkbox {
            name,
            label,
            help,
            required,
        },
        FormField::Select { options, .. } => NewFormField::Select {
            name,
            label,
            help,
            required,
            options: options.clone(),
        },
    }
}

#[cfg(feature = "browser")]
fn new_form_field_with_text_presentation(
    field: &FormField,
    presentation: TextPresentation,
) -> Option<NewFormField> {
    let FormField::Text {
        name,
        label,
        help,
        required,
        validation,
        ..
    } = field
    else {
        return None;
    };
    Some(NewFormField::Text {
        name: name.clone(),
        label: label.clone(),
        help: help.clone(),
        required: *required,
        validation: validation.clone(),
        presentation,
    })
}

#[cfg(feature = "browser")]
fn new_form_field_as_kind(field: &FormField, kind: V2NewFormFieldKind) -> NewFormField {
    let name = form_field_name(field).to_owned();
    let label = form_field_label(field).to_owned();
    let help = form_field_help(field).map(str::to_owned);
    let required = form_field_required(field);
    match kind {
        V2NewFormFieldKind::Text => NewFormField::Text {
            name,
            label,
            help,
            required,
            validation: None,
            presentation: form_field_presentation(field).unwrap_or_default(),
        },
        V2NewFormFieldKind::Email => NewFormField::Email {
            name,
            label,
            help,
            required,
            validation: None,
        },
        V2NewFormFieldKind::Phone => NewFormField::Phone {
            name,
            label,
            help,
            required,
            validation: None,
        },
        V2NewFormFieldKind::Checkbox => NewFormField::Checkbox {
            name,
            label,
            help,
            required,
        },
        V2NewFormFieldKind::Select => NewFormField::Select {
            name,
            label,
            help,
            required,
            options: form_field_options(field)
                .map(<[String]>::to_vec)
                .unwrap_or_else(|| vec!["Option 1".to_owned()]),
        },
    }
}

#[cfg(feature = "browser")]
fn form_field_with_options(field: &FormField, options: Vec<String>) -> Option<NewFormField> {
    let FormField::Select {
        name,
        label,
        help,
        required,
        ..
    } = field
    else {
        return None;
    };
    Some(NewFormField::Select {
        name: name.clone(),
        label: label.clone(),
        help: help.clone(),
        required: *required,
        options,
    })
}

#[cfg(feature = "browser")]
fn form_field_name_error(value: &str, form: &FormV2, field_id: &str) -> Result<(), &'static str> {
    let mut bytes = value.bytes();
    if !matches!(bytes.next(), Some(b'a'..=b'z'))
        || value.len() > 64
        || !bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err("Use 1–64 lowercase letters, numbers, or underscores; start with a letter.");
    }
    if matches!(value, "request_consent" | "marketing_opt_in") {
        return Err("This submission key is reserved for consent fields.");
    }
    if form
        .fields()
        .iter()
        .any(|field| field.id().as_str() != field_id && form_field_name(field) == value)
    {
        return Err("Each submission key must be unique within this form.");
    }
    Ok(())
}

#[cfg(feature = "browser")]
fn parse_form_options(value: &str) -> Result<Vec<String>, &'static str> {
    let options = value
        .lines()
        .map(str::trim)
        .filter(|option| !option.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if !(1..=50).contains(&options.len()) {
        return Err("Add between 1 and 50 non-empty dropdown options.");
    }
    if options.iter().any(|option| option.len() > 1024) {
        return Err("Each dropdown option must be 1,024 characters or fewer.");
    }
    if options
        .iter()
        .enumerate()
        .any(|(index, option)| options[..index].contains(option))
    {
        return Err("Dropdown options must be unique.");
    }
    Ok(options)
}

#[cfg(feature = "browser")]
#[component]
pub(super) fn V2FormEditor(state: Signal<V2EditorState>, form: FormV2, locked: bool) -> Element {
    let form_id = form.id().clone();
    let form_id_text = form_id.as_str().to_owned();
    let purpose = form.purpose().to_owned();
    let request_consent_copy = form.request_consent().copy().to_owned();
    let marketing_copy = form.marketing_opt_in().map(|copy| copy.copy().to_owned());
    let marketing_enabled = marketing_copy.is_some();
    let marketing_copy_for_toggle = marketing_copy.clone();
    let document = state.with(|state| state.document.clone());
    let language = document
        .as_ref()
        .map(DocumentV2::public_language)
        .unwrap_or(PublicLanguage::English);
    let pages = document
        .as_ref()
        .map(|document| form_success_pages(document, &form))
        .unwrap_or_default();
    let result_value = match form.result() {
        FormResult::Inline { .. } => "inline".to_owned(),
        FormResult::SuccessPage { page_id } => page_id.as_str().to_owned(),
    };
    let result_message = match form.result() {
        FormResult::Inline { message } => Some(message.clone()),
        FormResult::SuccessPage { .. } => None,
    };
    let purpose_key = form_setting_key(form_id.as_str(), "purpose");
    let purpose_value = input_value(state, &purpose_key, &purpose);
    let purpose_error = input_error(state, &purpose_key);
    let purpose_error_id = format!("{}-purpose-error", form_id.as_str());
    let request_consent_key = form_setting_key(form_id.as_str(), "request-consent");
    let request_consent_value = input_value(state, &request_consent_key, &request_consent_copy);
    let request_consent_error = input_error(state, &request_consent_key);
    let request_consent_error_id = format!("{}-request-consent-error", form_id.as_str());
    let result_key = form_setting_key(form_id.as_str(), "result-message");
    let result_error = input_error(state, &result_key);
    let result_error_id = format!("{}-result-error", form_id.as_str());
    let result_message_value = input_value(
        state,
        &result_key,
        result_message.as_deref().unwrap_or_default(),
    );
    let marketing_copy_key = form_setting_key(form_id.as_str(), "marketing-copy");
    let marketing_copy_value = marketing_copy
        .as_ref()
        .map(|copy| input_value(state, &marketing_copy_key, copy));
    let marketing_copy_error = input_error(state, &marketing_copy_key);
    let purpose_invalid = purpose_error.is_some();
    let purpose_error_message = purpose_error.unwrap_or_default();
    let request_consent_invalid = request_consent_error.is_some();
    let request_consent_error_message = request_consent_error.unwrap_or_default();
    let result_invalid = result_error.is_some();
    let result_error_message = result_error.unwrap_or_default();
    let marketing_copy_invalid = marketing_copy_error.is_some();
    let marketing_copy_error_message = marketing_copy_error.unwrap_or_default();
    let marketing_copy_error_id = format!("{}-marketing-copy-error", form_id.as_str());
    let form_id_for_purpose = form_id.clone();
    let form_id_for_result_mode = form_id.clone();
    let form_id_for_result_message = form_id.clone();
    let form_id_for_consent = form_id.clone();
    let form_id_for_marketing_toggle = form_id.clone();
    let form_id_for_marketing_copy = form_id.clone();
    let purpose_key_for_input = purpose_key.clone();
    let purpose_key_for_change = purpose_key.clone();
    let result_key_for_input = result_key.clone();
    let result_key_for_change = result_key.clone();
    let request_consent_key_for_input = request_consent_key.clone();
    let request_consent_key_for_change = request_consent_key.clone();
    let marketing_copy_key_for_input = marketing_copy_key.clone();
    let marketing_copy_key_for_change = marketing_copy_key.clone();
    let fields = form.fields().to_vec();
    let current_presentation = form.presentation().cloned().unwrap_or(FormPresentation {
        submit_label: None,
        information_url: None,
    });
    let submit_label_key = form_setting_key(form_id.as_str(), "submit-label");
    let submit_label_value = input_value(
        state,
        &submit_label_key,
        current_presentation
            .submit_label
            .as_deref()
            .unwrap_or_default(),
    );
    let submit_label_error = input_error(state, &submit_label_key);
    let submit_label_error_message = submit_label_error.unwrap_or_default();
    let submit_label_invalid = !submit_label_error_message.is_empty();
    let information_url_key = form_setting_key(form_id.as_str(), "information-url");
    let information_url_value = input_value(
        state,
        &information_url_key,
        current_presentation
            .information_url
            .as_deref()
            .unwrap_or_default(),
    );
    let information_url_error = input_error(state, &information_url_key);
    let information_url_error_message = information_url_error.unwrap_or_default();
    let information_url_invalid = !information_url_error_message.is_empty();
    let submit_label_key_for_input = submit_label_key.clone();
    let submit_label_key_for_change = submit_label_key.clone();
    let information_url_key_for_input = information_url_key.clone();
    let information_url_key_for_change = information_url_key.clone();
    let form_for_submit_label = form_id.clone();
    let form_for_information_url = form_id.clone();
    let information_url_for_submit_label = current_presentation.information_url.clone();
    let submit_label_for_information_url = current_presentation.submit_label.clone();

    rsx! {
        fieldset { class: "editor-form-builder", "aria-label": "Form settings",
            legend { "Form settings" }
            p { class: "editor-keyboard-help", "Submission purpose, required request consent, and optional marketing consent are stored separately." }
            label { class: "editor-setting-control",
                "Submit button label (optional)"
                input {
                    id: "{form_id_text}-submit-label",
                    value: "{submit_label_value}",
                    maxlength: "120",
                    disabled: locked,
                    "aria-invalid": "{submit_label_invalid}",
                    "aria-describedby": "{form_id_text}-submit-label-error",
                    oninput: move |event| {
                        let value = event.value();
                        if value.len() <= 120 {
                            update_form_input(state, submit_label_key_for_input.clone(), value);
                        } else {
                            reject_form_input(state, submit_label_key_for_input.clone(), value, "Use 120 characters or fewer.");
                        }
                    },
                    onchange: move |event| {
                        let value = event.value();
                        if value.len() <= 120 && (value.is_empty() || !value.trim().is_empty()) {
                            accept_form_input(state, submit_label_key_for_change.clone());
                            dispatch_v2_edit(state, Edit::SetFormPresentation {
                                form: form_for_submit_label.clone(),
                                presentation: optional_form_presentation(
                                    (!value.is_empty()).then_some(value),
                                    information_url_for_submit_label.clone(),
                                ),
                            });
                        } else if value.len() > 120 {
                            reject_form_input(state, submit_label_key_for_change.clone(), value, "Use 120 characters or fewer.");
                        } else {
                            reject_form_input(state, submit_label_key_for_change.clone(), value, "Enter visible button text, or clear the optional label.");
                        }
                    },
                }
                span { id: "{form_id_text}-submit-label-error", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{submit_label_error_message}" }
            }
            label { class: "editor-setting-control",
                "Information link URL (optional)"
                input {
                    id: "{form_id_text}-information-url",
                    r#type: "url",
                    value: "{information_url_value}",
                    maxlength: "2048",
                    disabled: locked,
                    "aria-invalid": "{information_url_invalid}",
                    "aria-describedby": "{form_id_text}-information-url-error",
                    oninput: move |event| {
                        let value = event.value();
                        if value.len() <= 2048 {
                            update_form_input(state, information_url_key_for_input.clone(), value);
                        } else {
                            reject_form_input(state, information_url_key_for_input.clone(), value, "Use an HTTPS URL up to 2,048 characters.");
                        }
                    },
                    onchange: move |event| {
                        let value = event.value();
                        let normalized_url = if value.is_empty() {
                            Some(String::new())
                        } else if value.len() <= 2048 {
                            eez_domain::document_v2::canonical_information_url(&value).ok()
                        } else {
                            None
                        };
                        if let Some(normalized_url) = normalized_url {
                            accept_form_input(state, information_url_key_for_change.clone());
                            dispatch_v2_edit(state, Edit::SetFormPresentation {
                                form: form_for_information_url.clone(),
                                presentation: optional_form_presentation(
                                    submit_label_for_information_url.clone(),
                                    (!normalized_url.is_empty()).then_some(normalized_url),
                                ),
                            });
                        } else {
                            reject_form_input(state, information_url_key_for_change.clone(), value, "Enter an HTTPS URL up to 2,048 characters.");
                        }
                    },
                }
                span { id: "{form_id_text}-information-url-error", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{information_url_error_message}" }
            }
            label { class: "editor-setting-control",
                "Submission purpose"
                textarea {
                    id: "{form_id_text}-purpose",
                    rows: "2",
                    value: "{purpose_value}",
                    disabled: locked,
                    "aria-invalid": "{purpose_invalid}",
                    "aria-describedby": "{purpose_error_id}",
                    oninput: move |event| {
                        let value = event.value();
                        if !value.trim().is_empty() && value.len() <= 1024 {
                            update_form_input(state, purpose_key_for_input.clone(), value);
                        } else {
                            reject_form_input(state, purpose_key_for_input.clone(), value, "Enter a purpose between 1 and 1,024 characters.");
                        }
                    },
                    onchange: move |event| {
                        let value = event.value();
                        if !value.trim().is_empty() && value.len() <= 1024 {
                            accept_form_input(state, purpose_key_for_change.clone());
                            dispatch_v2_edit(state, Edit::SetFormPurpose { form: form_id_for_purpose.clone(), purpose: value });
                        } else {
                            reject_form_input(
                                state,
                                purpose_key_for_change.clone(),
                                value,
                                "Enter a purpose between 1 and 1,024 characters.",
                            );
                        }
                    },
                }
                span { id: "{purpose_error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{purpose_error_message}" }
            }
            label { class: "editor-setting-control",
                "After submission"
                select {
                    value: "{result_value}",
                    disabled: locked,
                    onchange: move |event| {
                        let selected = event.value();
                        if selected == "inline" {
                            dispatch_v2_edit(state, Edit::SetFormResult {
                                form: form_id_for_result_mode.clone(),
                                result: FormResult::Inline {
                                    message: super::public_default_text(
                                        language,
                                        "Thanks for reaching out. We’ll be in touch soon.",
                                        "Merci, votre demande a bien été reçue.",
                                    ),
                                },
                            });
                        } else if let Ok(page_id) = PageId::parse(selected) {
                            dispatch_v2_edit(state, Edit::SetFormResult {
                                form: form_id_for_result_mode.clone(),
                                result: FormResult::SuccessPage { page_id },
                            });
                        }
                    },
                    option { value: "inline", selected: result_value == "inline", "Inline confirmation" }
                    for page in pages.iter() {
                        option {
                            value: "{page.id().as_str()}",
                            selected: result_value == page.id().as_str(),
                            "Success page: {page.title()}"
                        }
                    }
                }
            }
            if pages.is_empty() {
                p { class: "editor-keyboard-help", "No valid success page is available yet. Add another page or remove the path back to this form page." }
            }
            if result_message.is_some() {
                label { class: "editor-setting-control",
                    "Confirmation message"
                    textarea {
                        id: "{form_id_text}-result-message",
                        rows: "2",
                        value: "{result_message_value}",
                        disabled: locked,
                        "aria-invalid": "{result_invalid}",
                        "aria-describedby": "{result_error_id}",
                        oninput: move |event| {
                            let value = event.value();
                            if !value.trim().is_empty() && value.len() <= 1024 {
                                update_form_input(state, result_key_for_input.clone(), value);
                            } else {
                                reject_form_input(state, result_key_for_input.clone(), value, "Enter a confirmation message between 1 and 1,024 characters.");
                            }
                        },
                        onchange: move |event| {
                            let value = event.value();
                            if !value.trim().is_empty() && value.len() <= 1024 {
                                accept_form_input(state, result_key_for_change.clone());
                                dispatch_v2_edit(state, Edit::SetFormResult {
                                    form: form_id_for_result_message.clone(),
                                    result: FormResult::Inline { message: value },
                                });
                            } else {
                                reject_form_input(
                                    state,
                                    result_key_for_change.clone(),
                                    value,
                                    "Enter a confirmation message between 1 and 1,024 characters.",
                                );
                            }
                        },
                    }
                    span { id: "{result_error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{result_error_message}" }
                }
            }
            label { class: "editor-setting-control",
                "Required request consent"
                textarea {
                    id: "{form_id_text}-request-consent",
                    rows: "2",
                    value: "{request_consent_value}",
                    disabled: locked,
                    "aria-invalid": "{request_consent_invalid}",
                    "aria-describedby": "{request_consent_error_id}",
                    oninput: move |event| {
                        let value = event.value();
                        if !value.trim().is_empty() && value.len() <= 1024 {
                            update_form_input(state, request_consent_key_for_input.clone(), value);
                        } else {
                            reject_form_input(state, request_consent_key_for_input.clone(), value, "Enter request consent wording between 1 and 1,024 characters.");
                        }
                    },
                    onchange: move |event| {
                        let value = event.value();
                        if !value.trim().is_empty() && value.len() <= 1024 {
                            accept_form_input(state, request_consent_key_for_change.clone());
                            dispatch_v2_edit(state, Edit::SetFormRequestConsent {
                                form: form_id_for_consent.clone(),
                                copy: value,
                            });
                        } else {
                            reject_form_input(
                                state,
                                request_consent_key_for_change.clone(),
                                value,
                                "Enter request consent wording between 1 and 1,024 characters.",
                            );
                        }
                    },
                }
                span { id: "{request_consent_error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{request_consent_error_message}" }
            }
            label { class: "editor-form-checkbox",
                input {
                    r#type: "checkbox",
                    checked: marketing_enabled,
                    disabled: locked,
                    onchange: move |event| {
                        let value = if event.checked() {
                            Some(marketing_copy_for_toggle.clone().unwrap_or_else(|| {
                                super::public_default_text(
                                    language,
                                    "Send me occasional product updates.",
                                    "Je souhaite recevoir ponctuellement des nouvelles du produit.",
                                )
                            }))
                        } else {
                            None
                        };
                        dispatch_v2_edit(state, Edit::SetFormMarketingOptIn {
                            form: form_id_for_marketing_toggle.clone(),
                            copy: value,
                        });
                    },
                }
                "Offer separate, optional marketing consent"
            }
            if let Some(copy) = marketing_copy_value {
                label { class: "editor-setting-control",
                    "Optional marketing consent wording"
                    textarea {
                        id: "{form_id_text}-marketing-copy",
                        rows: "2",
                        value: "{copy}",
                        disabled: locked,
                        "aria-invalid": "{marketing_copy_invalid}",
                        "aria-describedby": "{marketing_copy_error_id}",
                        oninput: move |event| {
                            let value = event.value();
                            if !value.trim().is_empty() && value.len() <= 1024 {
                                update_form_input(state, marketing_copy_key_for_input.clone(), value);
                            } else {
                                reject_form_input(state, marketing_copy_key_for_input.clone(), value, "Enter marketing consent wording between 1 and 1,024 characters.");
                            }
                        },
                        onchange: move |event| {
                            let value = event.value();
                            if !value.trim().is_empty() && value.len() <= 1024 {
                                accept_form_input(state, marketing_copy_key_for_change.clone());
                                dispatch_v2_edit(state, Edit::SetFormMarketingOptIn {
                                    form: form_id_for_marketing_copy.clone(),
                                    copy: Some(value),
                                });
                            } else {
                                reject_form_input(
                                    state,
                                    marketing_copy_key_for_change.clone(),
                                    value,
                                    "Enter marketing consent wording between 1 and 1,024 characters.",
                                );
                            }
                        },
                    }
                    span { id: "{marketing_copy_error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{marketing_copy_error_message}" }
                }
            }
        }
        fieldset { class: "editor-form-builder", "aria-label": "Form fields",
            legend { "Fields ({fields.len()}/50)" }
            div { class: "editor-form-field-list", role: "list", "aria-label": "Form fields",
                for (index, field) in fields.iter().enumerate() {
                    {
                        let field_id = field.id().clone();
                        let field_name_id = field.id().as_str().to_owned();
                        let type_id = format!("form-{}-field-{}-type", form_id.as_str(), field_name_id);
                        let name_id = format!("form-{}-field-{}-name", form_id.as_str(), field_name_id);
                        let label_id = format!("form-{}-field-{}-label", form_id.as_str(), field_name_id);
                        let help_id = format!("form-{}-field-{}-help", form_id.as_str(), field_name_id);
                        let options_id = format!("form-{}-field-{}-options", form_id.as_str(), field_name_id);
                        let min_id = format!("form-{}-field-{}-min", form_id.as_str(), field_name_id);
                        let max_id = format!("form-{}-field-{}-max", form_id.as_str(), field_name_id);
                        let name_key = form_field_key(form_id.as_str(), &field_name_id, "name");
                        let label_key = form_field_key(form_id.as_str(), &field_name_id, "label");
                        let help_key = form_field_key(form_id.as_str(), &field_name_id, "help");
                        let options_key = form_field_key(form_id.as_str(), &field_name_id, "options");
                        let min_key = form_field_key(form_id.as_str(), &field_name_id, "min");
                        let max_key = form_field_key(form_id.as_str(), &field_name_id, "max");
                        let type_key = form_field_key(form_id.as_str(), &field_name_id, "type");
                        let base_name = form_field_name(field).to_owned();
                        let base_label = form_field_label(field).to_owned();
                        let base_help = form_field_help(field).unwrap_or_default().to_owned();
                        let current_name = input_value(state, &name_key, &base_name);
                        let current_label = input_value(state, &label_key, &base_label);
                        let current_help = input_value(state, &help_key, &base_help);
                        let name_error = input_error(state, &name_key);
                        let label_error = input_error(state, &label_key);
                        let help_error = input_error(state, &help_key);
                        let options_error = input_error(state, &options_key);
                        let min_error = input_error(state, &min_key);
                        let max_error = input_error(state, &max_key);
                        let name_invalid = name_error.is_some();
                        let name_error_message = name_error.unwrap_or_default();
                        let label_invalid = label_error.is_some();
                        let label_error_message = label_error.unwrap_or_default();
                        let help_invalid = help_error.is_some();
                        let help_error_message = help_error.unwrap_or_default();
                        let options_invalid = options_error.is_some();
                        let options_error_message = options_error.unwrap_or_default();
                        let min_invalid = min_error.is_some();
                        let min_error_message = min_error.unwrap_or_default();
                        let max_invalid = max_error.is_some();
                        let max_error_message = max_error.unwrap_or_default();
                        let name_error_id = format!("{name_id}-error");
                        let label_error_id = format!("{label_id}-error");
                        let help_error_id = format!("{help_id}-error");
                        let options_error_id = format!("{options_id}-error");
                        let min_error_id = format!("{min_id}-error");
                        let max_error_id = format!("{max_id}-error");
                        let current_required = form_field_required(field);
                        let current_text_presentation = form_field_presentation(field);
                        let text_presentation_id = format!(
                            "form-{}-field-{}-presentation",
                            form_id.as_str(),
                            field_name_id
                        );
                        let base_kind = form_field_kind_id(field).to_owned();
                        let base_kind_for_type = base_kind.clone();
                        let current_kind = pending_field_type(state, &type_key).unwrap_or(base_kind);
                        let current_options_default = form_field_options(field)
                            .map(|values| values.join("\n"))
                            .unwrap_or_default();
                        let current_options = input_value(state, &options_key, &current_options_default);
                        let current_validation = form_field_validation(field).cloned();
                        let current_min_length_default = current_validation
                            .as_ref()
                            .and_then(|value| value.min_length)
                            .map(|value| value.to_string())
                            .unwrap_or_default();
                        let current_max_length_default = current_validation
                            .as_ref()
                            .and_then(|value| value.max_length)
                            .map(|value| value.to_string())
                            .unwrap_or_default();
                        let current_min_length = input_value(state, &min_key, &current_min_length_default);
                        let current_max_length = input_value(state, &max_key, &current_max_length_default);
                        let current_min_length_for_update = current_min_length.clone();
                        let current_max_length_for_update = current_max_length.clone();
                        let current_min_length_for_input = current_min_length.clone();
                        let current_max_length_for_input = current_max_length.clone();
                        let field_for_name = field.clone();
                        let field_for_label = field.clone();
                        let field_for_help = field.clone();
                        let field_for_required = field.clone();
                        let field_for_presentation = field.clone();
                        let field_for_options = field.clone();
                        let field_for_min = field.clone();
                        let field_for_min_input = field.clone();
                        let field_for_max = field.clone();
                        let field_for_max_input = field.clone();
                        let form_for_type_apply = form_id.clone();
                        let form_for_name = form_id.clone();
                        let form_for_label = form_id.clone();
                        let form_for_help = form_id.clone();
                        let form_for_required = form_id.clone();
                        let form_for_presentation = form_id.clone();
                        let form_for_options = form_id.clone();
                        let form_for_min = form_id.clone();
                        let form_for_max = form_id.clone();
                        let id_for_name = field_id.clone();
                        let id_for_label = field_id.clone();
                        let id_for_help = field_id.clone();
                        let id_for_required = field_id.clone();
                        let id_for_presentation = field_id.clone();
                        let id_for_type_apply = field_id.clone();
                        let id_for_options = field_id.clone();
                        let id_for_min = field_id.clone();
                        let id_for_max = field_id.clone();
                        let field_ids = fields.iter().map(|item| item.id().clone()).collect::<Vec<_>>();
                        let field_ids_up = field_ids.clone();
                        let field_ids_down = field_ids;
                        let form_for_up = form_id.clone();
                        let form_for_down = form_id.clone();
                        let form_for_delete = form_id.clone();
                        let field_for_delete = field_id.clone();
                        let count = fields.len();
                        let help_value = current_help.clone();
                        let name_key_for_input = name_key.clone();
                        let name_key_for_change = name_key.clone();
                        let label_key_for_input = label_key.clone();
                        let label_key_for_change = label_key.clone();
                        let help_key_for_input = help_key.clone();
                        let help_key_for_change = help_key.clone();
                        let options_key_for_input = options_key.clone();
                        let options_key_for_change = options_key.clone();
                        let min_key_for_input = min_key.clone();
                        let min_key_for_change = min_key.clone();
                        let max_key_for_input = max_key.clone();
                        let max_key_for_change = max_key.clone();
                        let type_key_for_select = type_key.clone();
                        let type_key_for_apply = type_key.clone();
                        let type_key_for_cancel = type_key.clone();
                        let form_for_name_validation = form.clone();
                        let form_for_name_input_validation = form.clone();
                        let field_id_for_name_validation = field_id.clone();
                        let field_id_for_name_input_validation = field_id.clone();
                        let field_for_type_apply = field.clone();
                        let pending_kind = pending_field_type(state, &type_key);
                        let pending_kind_value = pending_kind
                            .as_deref()
                            .and_then(V2NewFormFieldKind::from_id);
                        let loses_length_rules = pending_kind_value.is_some()
                            && form_field_validation(field).is_some();
                        let loses_dropdown_options = pending_kind_value.is_some()
                            && form_field_options(field).is_some()
                            && pending_kind_value != Some(V2NewFormFieldKind::Select);
                        let loses_multiline = current_text_presentation
                            == Some(TextPresentation::Multiline)
                            && pending_kind_value.is_some_and(|kind| kind != V2NewFormFieldKind::Text);
                        let conversion_warning = match (
                            loses_length_rules,
                            loses_dropdown_options,
                            loses_multiline,
                        ) {
                            (true, true, true) => Some("Applying this change clears the saved length rules, dropdown options, and multiline setting."),
                            (true, true, false) => Some("Applying this change clears the saved length rules and dropdown options."),
                            (true, false, true) => Some("Applying this change clears the saved length rules and multiline setting."),
                            (false, true, true) => Some("Applying this change clears the saved dropdown options and multiline setting."),
                            (true, false, false) => Some("Applying this change clears the saved length rules."),
                            (false, true, false) => Some("Applying this change clears the saved dropdown options."),
                            (false, false, true) => Some("Applying this change clears the saved multiline setting."),
                            (false, false, false) => None,
                        };
                        rsx! {
                            div { class: "editor-form-field-row", role: "listitem",
                                fieldset { class: "editor-form-field",
                                    legend { "Field {index + 1}" }
                                label { class: "editor-setting-control", r#for: "{type_id}",
                                    "Field type"
                                    select {
                                        id: "{type_id}",
                                        value: "{current_kind}",
                                        disabled: locked,
                                        onchange: move |event| {
                                            let selected = event.value();
                                            if selected == base_kind_for_type {
                                                clear_pending_field_type(state, type_key_for_select.clone());
                                            } else if V2NewFormFieldKind::from_id(&selected).is_some() {
                                                set_pending_field_type(state, type_key_for_select.clone(), selected);
                                            }
                                        },
                                        for kind in V2NewFormFieldKind::ALL {
                                            option { value: "{kind.id()}", "{kind.label()}" }
                                        }
                                    }
                                }
                                if let Some(kind) = pending_kind_value {
                                    div { class: "editor-form-conversion-confirmation", role: "group", "aria-label": "Confirm field type change",
                                        p { role: "status", aria_live: "polite",
                                            if let Some(warning) = conversion_warning {
                                                "{warning} The submission key, label, help text, and required setting are preserved."
                                            } else {
                                                "The submission key, label, help text, and required setting are preserved."
                                            }
                                        }
                                        button {
                                            class: "secondary-button",
                                            r#type: "button",
                                            disabled: locked,
                                            onclick: move |_| {
                                                clear_pending_field_type(state, type_key_for_apply.clone());
                                                dispatch_v2_edit(state, Edit::SetFormField {
                                                    form: form_for_type_apply.clone(),
                                                    field: id_for_type_apply.clone(),
                                                    value: new_form_field_as_kind(&field_for_type_apply, kind),
                                                });
                                            },
                                            "Apply type change"
                                        }
                                        button {
                                            class: "text-button",
                                            r#type: "button",
                                            disabled: locked,
                                            onclick: move |_| clear_pending_field_type(state, type_key_for_cancel.clone()),
                                            "Cancel"
                                        }
                                    }
                                }
                                label { class: "editor-setting-control", r#for: "{name_id}",
                                    "Submission key"
                                    input {
                                        id: "{name_id}",
                                        value: "{current_name}",
                                        disabled: locked,
                                        "aria-invalid": "{name_invalid}",
                                        "aria-describedby": "{name_error_id}",
                                        oninput: move |event| {
                                            let value = event.value();
                                            match form_field_name_error(&value, &form_for_name_input_validation, field_id_for_name_input_validation.as_str()) {
                                                Ok(()) => update_form_input(state, name_key_for_input.clone(), value),
                                                Err(error) => reject_form_input(state, name_key_for_input.clone(), value, error),
                                            }
                                        },
                                        onchange: move |event| {
                                            let value = event.value();
                                            if form_field_name_error(&value, &form_for_name_validation, field_id_for_name_validation.as_str()).is_ok() {
                                                accept_form_input(state, name_key_for_change.clone());
                                                dispatch_v2_edit(state, Edit::SetFormField {
                                                    form: form_for_name.clone(),
                                                    field: id_for_name.clone(),
                                                    value: new_form_field_with_common(
                                                        &field_for_name,
                                                        value,
                                                        form_field_label(&field_for_name).to_owned(),
                                                        form_field_help(&field_for_name).map(str::to_owned),
                                                        form_field_required(&field_for_name),
                                                    ),
                                                });
                                            } else if let Err(error) = form_field_name_error(&value, &form_for_name_validation, field_id_for_name_validation.as_str()) {
                                                reject_form_input(state, name_key_for_change.clone(), value, error);
                                            }
                                        },
                                    }
                                    span { id: "{name_error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{name_error_message}" }
                                }
                                label { class: "editor-setting-control", r#for: "{label_id}",
                                    "Visible label"
                                    input {
                                        id: "{label_id}",
                                        value: "{current_label}",
                                        disabled: locked,
                                        "aria-invalid": "{label_invalid}",
                                        "aria-describedby": "{label_error_id}",
                                        oninput: move |event| {
                                            let value = event.value();
                                            if !value.trim().is_empty() && value.len() <= 1024 {
                                                update_form_input(state, label_key_for_input.clone(), value);
                                            } else {
                                                reject_form_input(state, label_key_for_input.clone(), value, "Enter a visible label between 1 and 1,024 characters.");
                                            }
                                        },
                                        onchange: move |event| {
                                            let value = event.value();
                                            if !value.trim().is_empty() && value.len() <= 1024 {
                                                accept_form_input(state, label_key_for_change.clone());
                                                dispatch_v2_edit(state, Edit::SetFormField {
                                                    form: form_for_label.clone(),
                                                    field: id_for_label.clone(),
                                                    value: new_form_field_with_common(
                                                        &field_for_label,
                                                        form_field_name(&field_for_label).to_owned(),
                                                        value,
                                                        form_field_help(&field_for_label).map(str::to_owned),
                                                        form_field_required(&field_for_label),
                                                    ),
                                                });
                                            } else {
                                                reject_form_input(state, label_key_for_change.clone(), value, "Enter a visible label between 1 and 1,024 characters.");
                                            }
                                        },
                                    }
                                    span { id: "{label_error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{label_error_message}" }
                                }
                                label { class: "editor-setting-control", r#for: "{help_id}",
                                    "Help text (optional)"
                                    input {
                                        id: "{help_id}",
                                        value: "{help_value}",
                                        disabled: locked,
                                        "aria-invalid": "{help_invalid}",
                                        "aria-describedby": "{help_error_id}",
                                        oninput: move |event| {
                                            let value = event.value();
                                            if value.len() <= 1024 {
                                                update_form_input(state, help_key_for_input.clone(), value);
                                            } else {
                                                reject_form_input(state, help_key_for_input.clone(), value, "Help text must be 1,024 characters or fewer.");
                                            }
                                        },
                                        onchange: move |event| {
                                            let value = event.value();
                                            if value.len() <= 1024 {
                                                accept_form_input(state, help_key_for_change.clone());
                                                dispatch_v2_edit(state, Edit::SetFormField {
                                                    form: form_for_help.clone(),
                                                    field: id_for_help.clone(),
                                                    value: new_form_field_with_common(
                                                        &field_for_help,
                                                        form_field_name(&field_for_help).to_owned(),
                                                        form_field_label(&field_for_help).to_owned(),
                                                        (!value.is_empty()).then_some(value),
                                                        form_field_required(&field_for_help),
                                                    ),
                                                });
                                            } else {
                                                reject_form_input(state, help_key_for_change.clone(), value, "Help text must be 1,024 characters or fewer.");
                                            }
                                        },
                                    }
                                    span { id: "{help_error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{help_error_message}" }
                                }
                                label { class: "editor-form-checkbox",
                                    input {
                                        r#type: "checkbox",
                                        checked: current_required,
                                        disabled: locked,
                                        onchange: move |event| {
                                            dispatch_v2_edit(state, Edit::SetFormField {
                                                form: form_for_required.clone(),
                                                field: id_for_required.clone(),
                                                value: new_form_field_with_common(
                                                    &field_for_required,
                                                    form_field_name(&field_for_required).to_owned(),
                                                    form_field_label(&field_for_required).to_owned(),
                                                    form_field_help(&field_for_required).map(str::to_owned),
                                                    event.checked(),
                                                ),
                                            });
                                        },
                                    }
                                    "Required"
                                }
                                if let Some(current_presentation) = current_text_presentation {
                                    label { class: "editor-setting-control", r#for: "{text_presentation_id}",
                                        "Text presentation"
                                        select {
                                            id: "{text_presentation_id}",
                                            value: if current_presentation == TextPresentation::Multiline { "multiline" } else { "single_line" },
                                            disabled: locked,
                                            onchange: move |event| {
                                                let next = match event.value().as_str() {
                                                    "multiline" => Some(TextPresentation::Multiline),
                                                    "single_line" => Some(TextPresentation::SingleLine),
                                                    _ => None,
                                                };
                                                if let Some(next) = next {
                                                    if let Some(value) = new_form_field_with_text_presentation(&field_for_presentation, next) {
                                                        dispatch_v2_edit(state, Edit::SetFormField {
                                                            form: form_for_presentation.clone(),
                                                            field: id_for_presentation.clone(),
                                                            value,
                                                        });
                                                    }
                                                }
                                            },
                                            option { value: "single_line", "Single line" }
                                            option { value: "multiline", "Multiple lines" }
                                        }
                                    }
                                }
                                if form_field_validation_cap(field).is_some() {
                                    div { class: "editor-form-length-row",
                                        label { class: "editor-setting-control",
                                            r#for: "{min_id}",
                                            "Minimum characters"
                                            input {
                                                id: "{min_id}",
                                                r#type: "number",
                                                min: "0",
                                                max: "{form_field_validation_cap(field).unwrap_or_default()}",
                                                value: "{current_min_length}",
                                                disabled: locked,
                                                "aria-invalid": "{min_invalid}",
                                                "aria-describedby": "{min_error_id}",
                                                oninput: move |event| {
                                                    let input = event.value();
                                                    match validation_range_update(&field_for_min_input, &input, &current_max_length_for_input) {
                                                        Ok(_) => update_form_input(state, min_key_for_input.clone(), input),
                                                        Err(error) => reject_form_input(state, min_key_for_input.clone(), input, error),
                                                    }
                                                },
                                                onchange: move |event| {
                                                    let input = event.value();
                                                    match validation_range_update(
                                                        &field_for_min,
                                                        &input,
                                                        &current_max_length_for_update,
                                                    ) {
                                                        Ok(value) => {
                                                            accept_form_input(state, min_key_for_change.clone());
                                                            dispatch_v2_edit(state, Edit::SetFormField {
                                                                form: form_for_min.clone(),
                                                                field: id_for_min.clone(),
                                                                value,
                                                            });
                                                        }
                                                        Err(error) => reject_form_input(state, min_key_for_change.clone(), input, error),
                                                    }
                                                },
                                            }
                                            span { id: "{min_error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{min_error_message}" }
                                        }
                                        label { class: "editor-setting-control",
                                            r#for: "{max_id}",
                                            "Maximum characters"
                                            input {
                                                id: "{max_id}",
                                                r#type: "number",
                                                min: "1",
                                                max: "{form_field_validation_cap(field).unwrap_or_default()}",
                                                value: "{current_max_length}",
                                                disabled: locked,
                                                "aria-invalid": "{max_invalid}",
                                                "aria-describedby": "{max_error_id}",
                                                oninput: move |event| {
                                                    let input = event.value();
                                                    match validation_range_update(&field_for_max_input, &current_min_length_for_input, &input) {
                                                        Ok(_) => update_form_input(state, max_key_for_input.clone(), input),
                                                        Err(error) => reject_form_input(state, max_key_for_input.clone(), input, error),
                                                    }
                                                },
                                                onchange: move |event| {
                                                    let input = event.value();
                                                    match validation_range_update(
                                                        &field_for_max,
                                                        &current_min_length_for_update,
                                                        &input,
                                                    ) {
                                                        Ok(value) => {
                                                            accept_form_input(state, max_key_for_change.clone());
                                                            dispatch_v2_edit(state, Edit::SetFormField {
                                                                form: form_for_max.clone(),
                                                                field: id_for_max.clone(),
                                                                value,
                                                            });
                                                        }
                                                        Err(error) => reject_form_input(state, max_key_for_change.clone(), input, error),
                                                    }
                                                },
                                            }
                                            span { id: "{max_error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{max_error_message}" }
                                        }
                                    }
                                }
                                if matches!(field, FormField::Select { .. }) {
                                    label { class: "editor-setting-control", r#for: "{options_id}",
                                        "Dropdown options (one per line)"
                                        textarea {
                                            id: "{options_id}",
                                            rows: "3",
                                            value: "{current_options}",
                                            disabled: locked,
                                            "aria-invalid": "{options_invalid}",
                                            "aria-describedby": "{options_error_id}",
                                            oninput: move |event| {
                                                let input = event.value();
                                                match parse_form_options(&input) {
                                                    Ok(_) => update_form_input(state, options_key_for_input.clone(), input),
                                                    Err(error) => reject_form_input(state, options_key_for_input.clone(), input, error),
                                                }
                                            },
                                            onchange: move |event| {
                                                let input = event.value();
                                                match parse_form_options(&input) {
                                                    Ok(options) => {
                                                        if let Some(updated) = form_field_with_options(&field_for_options, options) {
                                                            accept_form_input(state, options_key_for_change.clone());
                                                            dispatch_v2_edit(state, Edit::SetFormField {
                                                                form: form_for_options.clone(),
                                                                field: id_for_options.clone(),
                                                                value: updated,
                                                            });
                                                        }
                                                    }
                                                    Err(error) => reject_form_input(state, options_key_for_change.clone(), input, error),
                                                }
                                            },
                                        }
                                        span { id: "{options_error_id}", class: "editor-form-input-error", role: "alert", aria_live: "polite", "{options_error_message}" }
                                    }
                                }
                                div { class: "editor-section-actions",
                                    button {
                                        class: "editor-move-button",
                                        r#type: "button",
                                        disabled: locked || index == 0,
                                        "aria-label": "Move form field {index + 1} up",
                                        onclick: move |_| {
                                            let mut ordered = field_ids_up.clone();
                                            if index > 0 { ordered.swap(index, index - 1); }
                                            dispatch_v2_edit(state, Edit::ReorderFormFields { form: form_for_up.clone(), ordered });
                                        },
                                        "Up"
                                    }
                                    button {
                                        class: "editor-move-button",
                                        r#type: "button",
                                        disabled: locked || index + 1 >= count,
                                        "aria-label": "Move form field {index + 1} down",
                                        onclick: move |_| {
                                            let mut ordered = field_ids_down.clone();
                                            if index + 1 < ordered.len() { ordered.swap(index, index + 1); }
                                            dispatch_v2_edit(state, Edit::ReorderFormFields { form: form_for_down.clone(), ordered });
                                        },
                                        "Down"
                                    }
                                    button {
                                        class: "editor-move-button",
                                        r#type: "button",
                                        disabled: locked || count <= 1,
                                        "aria-label": "Delete form field {index + 1}",
                                        onclick: move |_| dispatch_v2_edit(state, Edit::DeleteFormField {
                                            form: form_for_delete.clone(),
                                            field: field_for_delete.clone(),
                                        }),
                                        "Delete"
                                    }
                                }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "editor-form-add-fields", "aria-label": "Add a form field",
                for kind in V2NewFormFieldKind::ALL {
                    {
                        let form_for_add = form_id.clone();
                        let field = fields.last().map(|field| field.id().clone());
                        let form_for_field = form.clone();
                        rsx! {
                            button {
                                class: "secondary-button",
                                r#type: "button",
                                disabled: locked || fields.len() >= 50,
                                onclick: move |_| dispatch_v2_edit(state, Edit::AddFormField {
                                    form: form_for_add.clone(),
                                    after: field.clone(),
                                            field: kind.create(&form_for_field, language),
                                }),
                                "Add {kind.label().to_lowercase()} field"
                            }
                        }
                    }
                }
            }
        }
    }
}
