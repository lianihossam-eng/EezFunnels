use std::collections::BTreeMap;

use dioxus::prelude::*;
#[cfg(feature = "browser")]
use eez_domain::document_v2::MAX_PORTABLE_TEMPLATE_BYTES;
use serde::Deserialize;
#[cfg(feature = "browser")]
use serde_json::Value;

#[cfg(feature = "browser")]
use super::api::{api_request, error_message};
#[cfg(feature = "browser")]
use super::browser::set_modal_dialog;
use super::idempotency::IdempotencyKeyState;
#[cfg(feature = "browser")]
use super::idempotency::{clear_idempotency_storage, prepare_idempotency_key};

const MAX_FUNNEL_NAME_BYTES: usize = 120;

#[cfg(feature = "browser")]
const TEMPLATE_FILE_READ: &str = r#"
const input = document.getElementById("funnel-template-file");
const file = input?.files?.[0];
if (input) input.value = "";
if (!file) {
    dioxus.send({ok: false, text: null, error: "Choose a template JSON file."});
} else if (file.size > 1400000) {
    dioxus.send({ok: false, text: null, error: "The template file exceeds the 1.4 MB limit."});
} else {
    try {
        dioxus.send({ok: true, text: await file.text(), error: null});
    } catch (_) {
        dioxus.send({ok: false, text: null, error: "The selected file could not be read."});
    }
}
"#;

#[derive(Clone, Debug, Deserialize)]
struct InspectionResponse {
    name: String,
    page_count: usize,
    form_count: usize,
    media: Vec<TemplateMediaAvailability>,
}

#[derive(Clone, Debug, Deserialize)]
struct TemplateMediaAvailability {
    asset: TemplateMediaAsset,
    available: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct TemplateMediaAsset {
    asset_id: String,
    mime: String,
    size_bytes: u64,
}

#[cfg(feature = "browser")]
#[derive(Clone, Debug, Deserialize)]
struct MediaLibraryResponse {
    assets: Vec<MediaLibraryAsset>,
}

#[derive(Clone, Debug, Deserialize)]
struct MediaLibraryAsset {
    id: String,
    mime: String,
    size_bytes: u64,
}

#[cfg(feature = "browser")]
#[derive(Clone, Debug, Deserialize)]
struct ImportResponse {
    funnel_id: String,
    published: bool,
}

#[cfg(feature = "browser")]
#[derive(Clone, Debug, Deserialize)]
struct FileReadResponse {
    ok: bool,
    text: Option<String>,
    error: Option<String>,
}

#[component]
pub(super) fn FunnelTemplateImportDialog(
    on_close: EventHandler<()>,
    on_imported: EventHandler<String>,
) -> Element {
    #[cfg(feature = "browser")]
    let mut package_json = use_signal(|| None::<String>);
    let mut inspection = use_signal(|| None::<InspectionResponse>);
    let mut media_library = use_signal(|| None::<Result<Vec<MediaLibraryAsset>, String>>);
    let mut name = use_signal(String::new);
    let mut replacements = use_signal(BTreeMap::<String, String>::new);
    let mut pending = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut idempotency_key = use_signal(|| None::<IdempotencyKeyState>);

    #[cfg(feature = "browser")]
    use_effect(move || {
        spawn(async move { set_modal_dialog("import-funnel-template-dialog", true).await });
    });

    let on_dismiss = move || {
        if pending() {
            return;
        }
        on_close.call(());
    };

    rsx! {
        dialog {
            id: "import-funnel-template-dialog",
            class: "funnel-dialog template-import-dialog",
            "aria-modal": "true",
            "aria-labelledby": "import-funnel-template-title",
            "aria-describedby": "import-funnel-template-description",
            oncancel: move |event| {
                event.prevent_default();
                on_dismiss();
            },
            div { class: "dialog-heading",
                p { class: "eyebrow", "Portable funnel" }
                h2 { id: "import-funnel-template-title", "Import a funnel template" }
                p { id: "import-funnel-template-description", "Choose a versioned JSON template. It will create a new unpublished draft; the source funnel is never changed." }
            }
            label { class: "template-import-file-label",
                "Template JSON"
                input {
                    id: "funnel-template-file",
                    r#type: "file",
                    accept: ".json,application/json",
                    disabled: pending(),
                    onchange: move |_| {
                        if pending() { return; }
                        clear_retry_key(&mut idempotency_key);
                        pending.set(true);
                        error.set(None);
                        inspection.set(None);
                        media_library.set(None);
                        replacements.set(BTreeMap::new());
                        #[cfg(feature = "browser")]
                        package_json.set(None);
                        #[cfg(feature = "browser")]
                        spawn(async move {
                            let result = read_template_file().await;
                            match result {
                                Ok(raw) => {
                                    match api_request("POST", "/api/funnel-templates/inspect", Some(raw.clone()), None).await {
                                        Ok(response) if response.status == 200 => {
                                            match serde_json::from_str::<InspectionResponse>(&response.body) {
                                                Ok(parsed) => {
                                                    name.set(parsed.name.clone());
                                                    let needs_library =
                                                        parsed.media.iter().any(|item| !item.available);
                                                    inspection.set(Some(parsed));
                                                    package_json.set(Some(raw));
                                                    if needs_library {
                                                        match load_media_library().await {
                                                            Ok(assets) => media_library.set(Some(Ok(assets))),
                                                            Err(message) => media_library.set(Some(Err(message))),
                                                        }
                                                    } else {
                                                        media_library.set(Some(Ok(Vec::new())));
                                                    }
                                                }
                                                Err(_) => error.set(Some("The inspection response could not be read.".to_owned())),
                                            }
                                        }
                                        Ok(response) => error.set(Some(error_message(&response.body))),
                                        Err(()) => error.set(Some("The template could not be checked. Choose it again to retry.".to_owned())),
                                    }
                                }
                                Err(message) => error.set(Some(message)),
                            }
                            pending.set(false);
                        });
                        #[cfg(not(feature = "browser"))]
                        pending.set(false);
                    },
                }
                small { "Maximum file size: 1.4 MB. Template contents remain in memory until this dialog closes." }
            }

            if pending() {
                p { class: "editor-publication-help", role: "status", "Reading and checking the template…" }
            }

            if let Some(checked) = inspection() {
                section { class: "template-import-summary", "aria-label": "Template summary",
                    h3 { "{checked.name}" }
                    p { "{checked.page_count} pages · {checked.form_count} forms · {checked.media.len()} referenced images" }
                }
                label {
                    "Name for the new funnel"
                    input {
                        r#type: "text",
                        maxlength: "120",
                        value: "{name()}",
                        disabled: pending(),
                        oninput: move |event| {
                            clear_retry_key(&mut idempotency_key);
                            name.set(event.value());
                        },
                    }
                }
                if !checked.media.is_empty() {
                    section { class: "template-import-media-list", "aria-label": "Image replacements",
                        h3 { "Images" }
                        p { "Images already in this library can keep their reference. Choose a replacement for each unavailable image." }
                        match media_library() {
                            None => rsx! { p { role: "status", "Loading the media library…" } },
                            Some(Err(message)) => rsx! { p { class: "error-message", role: "alert", "{message}" } },
                            Some(Ok(assets)) => rsx! {
                                for item in checked.media.iter().filter(|item| !item.available) {
                                    {
                                        let source_id = item.asset.asset_id.clone();
                                        let short_id = source_id.chars().take(12).collect::<String>();
                                        let selected = replacements().get(&source_id).cloned().unwrap_or_default();
                                        let select_id = format!("template-replacement-{}", short_id);
                                        rsx! {
                                            div { class: "template-import-media-item",
                                                label { r#for: "{select_id}", "Image {short_id} is unavailable ({item.asset.mime}, {item.asset.size_bytes} bytes). Choose a library image." }
                                                select {
                                                    id: "{select_id}",
                                                    value: "{selected}",
                                                    disabled: pending(),
                                                    onchange: move |event| {
                                                        let target_id = event.value();
                                                        clear_retry_key(&mut idempotency_key);
                                                        replacements.with_mut(|mapping| {
                                                            if target_id.is_empty() { mapping.remove(&source_id); }
                                                            else { mapping.insert(source_id.clone(), target_id); }
                                                        });
                                                    },
                                                    option { value: "", "Choose a replacement" }
                                                    for asset in assets.iter() {
                                                        option { value: "{asset.id}", "{asset.mime} · {asset.size_bytes} bytes · {asset.id.chars().take(12).collect::<String>()}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                        }
                    }
                }
            }

            if let Some(message) = error() {
                p { class: "error-message", role: "alert", "{message}" }
            }
            div { class: "dialog-actions",
                button {
                    class: "secondary-button",
                    r#type: "button",
                    disabled: pending(),
                    onclick: move |_| on_dismiss(),
                    "Cancel"
                }
                button {
                    class: "primary-button",
                    r#type: "button",
                    disabled: pending() || !can_import(inspection().as_ref(), media_library().as_ref(), &replacements(), &name()),
                    onclick: move |_| {
                        if pending() || !can_import(inspection().as_ref(), media_library().as_ref(), &replacements(), &name()) { return; }
                        #[cfg(feature = "browser")]
                        let Some(raw) = package_json() else { return; };
                        #[cfg(feature = "browser")]
                        let Ok(template) = serde_json::from_str::<Value>(&raw) else {
                            error.set(Some("The selected template is not valid JSON.".to_owned()));
                            return;
                        };
                        #[cfg(feature = "browser")]
                        let import_name = name().trim().to_owned();
                        #[cfg(feature = "browser")]
                        let request_body = serde_json::json!({
                            "name": import_name,
                            "template": template,
                            "asset_replacements": replacements(),
                        }).to_string();
                        #[cfg(feature = "browser")]
                        let path = "/api/funnel-templates/import";
                        #[cfg(feature = "browser")]
                        let prepared = match prepare_idempotency_key("POST", path, Some(&request_body), idempotency_key()) {
                            Ok(prepared) => prepared,
                            Err(()) => {
                                error.set(Some("The browser could not prepare a retry-safe import.".to_owned()));
                                return;
                            }
                        };
                        #[cfg(feature = "browser")]
                        idempotency_key.set(Some(prepared.clone()));
                        pending.set(true);
                        error.set(None);
                        #[cfg(feature = "browser")]
                        spawn(async move {
                            match api_request("POST", path, Some(request_body), Some(prepared.clone())).await {
                                Ok(response) if response.status == 201 => {
                                    match serde_json::from_str::<ImportResponse>(&response.body) {
                                        Ok(imported) if !imported.published => {
                                            let _ = clear_idempotency_storage(&prepared);
                                            on_imported.call(imported.funnel_id);
                                        }
                                        _ => {
                                            idempotency_key.set(Some(prepared));
                                            error.set(Some("The import result could not be confirmed. Retry safely or keep the response reference.".to_owned()));
                                        }
                                    }
                                }
                                Ok(response) => {
                                    idempotency_key.set(Some(prepared));
                                    error.set(Some(error_message(&response.body)));
                                }
                                Err(()) => {
                                    idempotency_key.set(Some(prepared));
                                    error.set(Some("The result is uncertain. Retry this import to safely check whether it completed.".to_owned()));
                                }
                            }
                            pending.set(false);
                        });
                        #[cfg(not(feature = "browser"))]
                        pending.set(false);
                    },
                    if idempotency_key().is_some() { "Retry import" } else { "Import as draft" }
                }
            }
        }
    }
}

#[cfg(feature = "browser")]
async fn read_template_file() -> Result<String, String> {
    let mut request = document::eval(TEMPLATE_FILE_READ);
    request
        .send(())
        .map_err(|_| "The selected file could not be read.".to_owned())?;
    let response = request
        .recv::<FileReadResponse>()
        .await
        .map_err(|_| "The selected file could not be read.".to_owned())?;
    if !response.ok {
        return Err(response
            .error
            .unwrap_or_else(|| "The selected file could not be read.".to_owned()));
    }
    response
        .text
        .filter(|text| text.len() <= MAX_PORTABLE_TEMPLATE_BYTES)
        .ok_or_else(|| "The template file exceeds the 1.4 MB limit.".to_owned())
}

#[cfg(feature = "browser")]
async fn load_media_library() -> Result<Vec<MediaLibraryAsset>, String> {
    let response = api_request("GET", "/api/media", None, None)
        .await
        .map_err(|_| "The media library could not be reached.".to_owned())?;
    if response.status != 200 {
        return Err(error_message(&response.body));
    }
    serde_json::from_str::<MediaLibraryResponse>(&response.body)
        .map(|result| result.assets)
        .map_err(|_| "The media library returned an invalid response.".to_owned())
}

fn can_import(
    inspection: Option<&InspectionResponse>,
    library: Option<&Result<Vec<MediaLibraryAsset>, String>>,
    replacements: &BTreeMap<String, String>,
    import_name: &str,
) -> bool {
    let Some(inspection) = inspection else {
        return false;
    };
    if !name_is_valid(import_name, MAX_FUNNEL_NAME_BYTES) {
        return false;
    }
    let Some(Ok(library)) = library else {
        return false;
    };
    let available_ids = library
        .iter()
        .map(|asset| asset.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    inspection.media.iter().all(|asset| {
        asset.available
            || replacements
                .get(&asset.asset.asset_id)
                .is_some_and(|target| available_ids.contains(target.as_str()))
    })
}

fn name_is_valid(name: &str, maximum_bytes: usize) -> bool {
    !name.trim().is_empty()
        && name.trim().len() <= maximum_bytes
        && !name.chars().any(char::is_control)
}

fn clear_retry_key(key: &mut Signal<Option<IdempotencyKeyState>>) {
    #[cfg(feature = "browser")]
    if let Some(previous) = key() {
        let _ = clear_idempotency_storage(&previous);
    }
    key.set(None);
}

#[cfg(test)]
mod tests {
    use super::{
        can_import, name_is_valid, InspectionResponse, MediaLibraryAsset, TemplateMediaAsset,
        TemplateMediaAvailability,
    };
    use std::collections::BTreeMap;

    fn inspection(available: bool) -> InspectionResponse {
        InspectionResponse {
            name: "Service template".to_owned(),
            page_count: 2,
            form_count: 1,
            media: vec![TemplateMediaAvailability {
                asset: TemplateMediaAsset {
                    asset_id: "source".to_owned(),
                    mime: "image/png".to_owned(),
                    size_bytes: 100,
                },
                available,
            }],
        }
    }

    #[test]
    fn an_unavailable_image_requires_an_existing_library_replacement() {
        let inspection = inspection(false);
        let library = Ok(vec![MediaLibraryAsset {
            id: "target".to_owned(),
            mime: "image/webp".to_owned(),
            size_bytes: 200,
        }]);
        assert!(!can_import(
            Some(&inspection),
            Some(&library),
            &BTreeMap::new(),
            "Service template",
        ));
        assert!(can_import(
            Some(&inspection),
            Some(&library),
            &BTreeMap::from([("source".to_owned(), "target".to_owned())]),
            "Service template",
        ));
        assert!(!can_import(
            Some(&inspection),
            Some(&library),
            &BTreeMap::from([("source".to_owned(), "missing".to_owned())]),
            "Service template",
        ));
    }

    #[test]
    fn already_available_media_can_keep_its_original_reference() {
        let inspection = inspection(true);
        let library = Ok(Vec::new());
        assert!(can_import(
            Some(&inspection),
            Some(&library),
            &BTreeMap::new(),
            "Service template",
        ));
    }

    #[test]
    fn imported_names_are_bounded_and_control_free() {
        assert!(name_is_valid("Offer", 120));
        assert!(!name_is_valid("  ", 120));
        assert!(!name_is_valid(&"x".repeat(121), 120));
        assert!(!name_is_valid("Offer\nPage", 120));
    }
}
