#[cfg(feature = "browser")]
use std::collections::BTreeSet;

use dioxus::prelude::*;

#[cfg(any(feature = "browser", test))]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct FunnelRequestState {
    generation: u64,
    funnel_id: Option<String>,
}

#[cfg(any(feature = "browser", test))]
impl FunnelRequestState {
    fn begin(&mut self, funnel_id: Option<String>) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.funnel_id = funnel_id;
        self.generation
    }

    fn is_current(&self, funnel_id: &str, generation: u64) -> bool {
        self.generation == generation && self.funnel_id.as_deref() == Some(funnel_id)
    }
}

#[cfg(feature = "browser")]
use super::api::{
    load_funnels, load_webhook_destination, load_webhook_form_delivery, save_webhook_destination,
    save_webhook_form_delivery, DeliveryControlFailure, FunnelCard, WebhookDestinationResponse,
    WebhookFormDeliveryOption, WebhookFormDeliverySnapshot,
};
#[cfg(feature = "browser")]
use super::idempotency::{
    clear_idempotency_storage, prepare_idempotency_key, PreparedIdempotencyKey,
};

#[cfg(feature = "browser")]
#[component]
pub(super) fn WebhookDeliveryPanel() -> Element {
    let mut destination = use_signal(|| None::<WebhookDestinationResponse>);
    let mut destination_url = use_signal(String::new);
    let mut destination_enabled = use_signal(|| false);
    let mut destination_loading = use_signal(|| true);
    let mut destination_error = use_signal(|| false);
    let destination_pending = use_signal(|| false);
    let destination_message = use_signal(|| None::<String>);
    let destination_key = use_signal(|| None::<PreparedIdempotencyKey>);
    let mut funnels = use_signal(|| None::<Vec<FunnelCard>>);
    let mut selected_funnel = use_signal(|| None::<String>);
    let mut funnel_snapshot = use_signal(|| None::<WebhookFormDeliverySnapshot>);
    let mut funnel_loading = use_signal(|| false);
    let mut funnel_error = use_signal(|| false);
    let funnel_request = use_signal(FunnelRequestState::default);

    use_effect(move || {
        spawn(async move {
            match load_webhook_destination().await {
                Ok(snapshot) => {
                    destination_url.set(snapshot.destination_url.clone().unwrap_or_default());
                    destination_enabled.set(snapshot.enabled);
                    destination.set(Some(snapshot));
                }
                Err(()) => destination_error.set(true),
            }
            destination_loading.set(false);
        });
        spawn(async move {
            match load_funnels().await {
                Ok(items) => {
                    let initial = selected_funnel()
                        .filter(|selected| items.iter().any(|funnel| &funnel.id == selected))
                        .or_else(|| items.first().map(|funnel| funnel.id.clone()));
                    selected_funnel.set(initial);
                    funnels.set(Some(items));
                }
                Err(()) => funnels.set(Some(Vec::new())),
            }
        });
    });

    use_effect(move || {
        let Some(funnel_id) = selected_funnel() else {
            begin_funnel_request(funnel_request, None);
            funnel_snapshot.set(None);
            funnel_loading.set(false);
            return;
        };
        let request_generation = begin_funnel_request(funnel_request, Some(funnel_id.clone()));
        funnel_snapshot.set(None);
        funnel_error.set(false);
        funnel_loading.set(true);
        spawn(async move {
            let result = load_webhook_form_delivery(&funnel_id).await;
            if !funnel_request_is_current(
                funnel_request,
                selected_funnel,
                &funnel_id,
                request_generation,
            ) {
                return;
            }
            match result {
                Ok(snapshot) if snapshot.funnel_id == funnel_id => {
                    funnel_snapshot.set(Some(snapshot))
                }
                Ok(_) | Err(()) => funnel_error.set(true),
            }
            funnel_loading.set(false);
        });
    });

    let saved_destination = destination();
    let current_destination = saved_destination.as_ref();
    let current_version =
        current_destination.and_then(|saved| saved.destination_version_id.clone());
    let current_enabled = current_destination.is_some_and(|saved| saved.enabled);
    let url_value = destination_url();
    let selected_funnel_value = selected_funnel();
    let funnel_list = funnels();
    let snapshot = funnel_snapshot();
    let snapshot_revision = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.published_revision_id.as_deref());
    let destination_version_id = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.destination_version_id.clone());

    rsx! {
        section { class: "webhook-delivery-panel", "aria-labelledby": "webhook-delivery-heading",
            div { class: "webhook-delivery-heading",
                div {
                    p { class: "eyebrow", "Optional integration" }
                    h2 { id: "webhook-delivery-heading", "Signed webhook delivery" }
                    p { class: "webhook-delivery-description",
                        "Keep every response in the inbox. Selected forms can also send a signed event to your HTTPS receiver."
                    }
                }
                span { class: "webhook-delivery-badge", "Off by default" }
            }
            if destination_loading() {
                p { role: "status", "Loading delivery settings…" }
            } else if destination_error() && destination().is_none() {
                div { class: "webhook-delivery-error", role: "alert",
                    p { "Delivery settings could not be loaded." }
                    button {
                        class: "secondary-button",
                        r#type: "button",
                        onclick: move |_| reload_destination(destination, destination_url, destination_enabled, destination_error, destination_loading),
                        "Try again"
                    }
                }
            } else {
                form {
                    class: "webhook-destination-form",
                    onsubmit: move |event| {
                        event.prevent_default();
                        submit_destination(
                            destination,
                            destination_url,
                            destination_enabled,
                            destination_pending,
                            destination_message,
                            destination_key,
                            destination_error,
                            destination_loading,
                            funnel_snapshot,
                            funnel_loading,
                            selected_funnel,
                            funnel_request,
                        );
                    },
                    label { r#for: "webhook-destination-url", "Receiver HTTPS URL" }
                    input {
                        id: "webhook-destination-url",
                        r#type: "url",
                        required: true,
                        maxlength: 2048,
                        autocomplete: "url",
                        placeholder: "https://receiver.your-domain.tld/events",
                        value: "{url_value}",
                        disabled: destination_pending(),
                        oninput: move |event| destination_url.set(event.value()),
                    }
                    p { class: "webhook-delivery-help",
                        "The receiver URL is checked for HTTPS and safe public routing when saved. The signing secret stays in Worker configuration and is never sent to this page."
                    }
                    label { class: "webhook-enabled-toggle", r#for: "webhook-installation-enabled",
                        input {
                            id: "webhook-installation-enabled",
                            r#type: "checkbox",
                            checked: destination_enabled(),
                            disabled: destination_pending() || current_version.is_none(),
                            onchange: move |event| destination_enabled.set(event.checked()),
                        }
                        span { "Allow future submissions to use this receiver" }
                    }
                    if destination_message().is_some() {
                        p { class: if destination_error() { "webhook-delivery-error" } else { "webhook-delivery-feedback" }, role: if destination_error() { "alert" } else { "status" },
                            "{destination_message().unwrap_or_default()}"
                        }
                    }
                    button {
                        class: "primary-button",
                        r#type: "submit",
                        disabled: destination_pending() || url_value.trim().is_empty(),
                        if destination_pending() { "Validating and saving…" } else if destination_key().is_some() { "Retry destination save" } else { "Validate and save destination" }
                    }
                    if current_version.is_some() && (current_version != destination_version_id || current_enabled != destination_enabled()) {
                        p { class: "webhook-delivery-help", role: "status",
                            "Save this change before enabling form delivery. Existing form mappings are tied to the saved destination version."
                        }
                    }
                }
                section { class: "webhook-form-delivery-settings", "aria-labelledby": "webhook-form-settings-heading",
                    div { class: "webhook-form-settings-heading",
                        h3 { id: "webhook-form-settings-heading", "Choose which forms can send" }
                        p { "Each configuration applies to future submissions that match the currently published form. Older tasks keep their original destination." }
                    }
                    match funnel_list {
                        None => rsx! { p { role: "status", "Loading funnels…" } },
                        Some(items) if items.is_empty() => rsx! { p { class: "webhook-delivery-help", "Create a funnel to configure form delivery." } },
                        Some(items) => rsx! {
                            label { r#for: "webhook-funnel-select", "Funnel" }
                            select {
                                id: "webhook-funnel-select",
                                value: selected_funnel_value.clone().unwrap_or_default(),
                                onchange: move |event| {
                                    let funnel_id = event.value();
                                    begin_funnel_request(funnel_request, Some(funnel_id.clone()));
                                    funnel_snapshot.set(None);
                                    funnel_loading.set(true);
                                    funnel_error.set(false);
                                    selected_funnel.set(Some(funnel_id));
                                },
                                for funnel in items.iter() {
                                    option { value: "{funnel.id}", "{funnel.name}" }
                                }
                            }
                            if funnel_loading() {
                                p { role: "status", "Loading published forms…" }
                            } else if funnel_error() {
                                div { class: "webhook-delivery-error", role: "alert",
                                    p { "Published form settings could not be loaded." }
                                    button {
                                        class: "secondary-button",
                                        r#type: "button",
                                        onclick: move |_| reload_funnel(selected_funnel, funnel_request, funnel_snapshot, funnel_loading, funnel_error),
                                        "Try again"
                                    }
                                }
                            } else if let Some(snapshot) = snapshot.as_ref() {
                                if snapshot.published_revision_id.is_none() {
                                    p { class: "webhook-delivery-help", "Publish this funnel before configuring form delivery." }
                                } else if snapshot.forms.is_empty() {
                                    p { class: "webhook-delivery-help", "The published funnel has no visitor-facing forms." }
                                } else {
                                    for form in snapshot.forms.iter() {
                                        WebhookFormDeliveryPanel {
                                            key: "{form.form_id}:{form.mapping_version_id.clone().unwrap_or_default()}:{snapshot_revision.unwrap_or_default()}:{destination_version_id.clone().unwrap_or_default()}",
                                            funnel_id: snapshot.funnel_id.clone(),
                                            published_revision_id: snapshot_revision.unwrap_or_default().to_owned(),
                                            destination_version_id: destination_version_id.clone(),
                                            form: form.clone(),
                                            funnel_request,
                                            on_saved: move |saved_funnel_id: String| {
                                                if should_reload_after_save(selected_funnel().as_deref(), &saved_funnel_id) {
                                                    reload_funnel(selected_funnel, funnel_request, funnel_snapshot, funnel_loading, funnel_error);
                                                }
                                            },
                                            on_reload: move |_| reload_funnel(selected_funnel, funnel_request, funnel_snapshot, funnel_loading, funnel_error),
                                        }
                                    }
                                }
                            }
                        }
                    }
                    p { class: "webhook-delivery-help",
                        "Inbox-only submissions remain available even when delivery is off, the receiver fails, or a form mapping needs revalidation."
                    }
                }
            }
        }
    }
}

#[cfg(not(feature = "browser"))]
#[component]
pub(super) fn WebhookDeliveryPanel() -> Element {
    rsx! {
        section { class: "webhook-delivery-panel", "aria-label": "Webhook delivery settings",
            p { "Open this page in the dashboard to configure signed webhook delivery." }
        }
    }
}

#[cfg(feature = "browser")]
#[component]
fn WebhookFormDeliveryPanel(
    funnel_id: String,
    published_revision_id: String,
    destination_version_id: Option<String>,
    form: WebhookFormDeliveryOption,
    funnel_request: Signal<FunnelRequestState>,
    on_saved: EventHandler<String>,
    on_reload: EventHandler<()>,
) -> Element {
    let compatible_ids = form
        .fields
        .iter()
        .map(|field| field.field_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut selected_field_ids = use_signal(|| {
        form.selected_field_ids
            .iter()
            .filter(|field_id| compatible_ids.contains(field_id.as_str()))
            .cloned()
            .collect::<BTreeSet<_>>()
    });
    let mut enabled = use_signal(|| form.enabled);
    let pending = use_signal(|| false);
    let error = use_signal(|| None::<String>);
    let idempotency_key = use_signal(|| None::<PreparedIdempotencyKey>);
    let selected = selected_field_ids();
    let form_id = form.form_id.clone();
    let form_name = format!("webhook-form-{}", form.form_id);
    let can_save = !pending() && !selected.is_empty() && destination_version_id.is_some();

    rsx! {
        fieldset { class: "webhook-form-delivery-card",
            legend { "{form.purpose}" }
            label { class: "webhook-enabled-toggle", r#for: "{form_name}-enabled",
                input {
                    id: "{form_name}-enabled",
                    r#type: "checkbox",
                    checked: enabled(),
                    disabled: pending() || destination_version_id.is_none(),
                    onchange: move |event| enabled.set(event.checked()),
                }
                span { "Send new submissions from this form" }
            }
            if form.needs_revalidation {
                p { class: "webhook-delivery-warning", role: "status",
                    "This mapping uses an older destination or contains fields that are no longer in the published form. Review the fields and save to revalidate it."
                }
            } else if form.mapping_version_id.is_none() {
                p { class: "webhook-delivery-help", "No delivery mapping is saved for this form. It will stay inbox-only until you save one." }
            }
            if form.fields.is_empty() {
                p { class: "webhook-delivery-help", "This form has no fields that can be sent." }
            } else {
                div { class: "webhook-field-options",
                    p { "Fields included in the webhook" }
                    for field in form.fields.iter() {
                        label { class: "webhook-field-option", r#for: "{form_name}-field-{field.field_id}",
                            input {
                                id: "{form_name}-field-{field.field_id}",
                                r#type: "checkbox",
                                checked: selected.contains(&field.field_id),
                                disabled: pending(),
                                onchange: {
                                    let field_id = field.field_id.clone();
                                    move |event| {
                                        selected_field_ids.with_mut(|selected| {
                                            if event.checked() {
                                                selected.insert(field_id.clone());
                                            } else {
                                                selected.remove(&field_id);
                                            }
                                        });
                                    }
                                },
                            }
                            span { "{field.label}" }
                        }
                    }
                }
            }
            if selected.is_empty() {
                p { class: "webhook-delivery-warning", role: "status", "Choose at least one field before saving a mapping." }
            }
            if let Some(message) = error() {
                p { class: "webhook-delivery-error", role: "alert", "{message}" }
            }
            div { class: "webhook-form-delivery-actions",
                button {
                    class: "secondary-button",
                    r#type: "button",
                    disabled: !can_save,
                    onclick: move |_| save_form_delivery(
                        funnel_id.clone(),
                        published_revision_id.clone(),
                        form.clone(),
                        destination_version_id.clone(),
                        selected_field_ids,
                        enabled,
                        funnel_request,
                        pending,
                        error,
                        idempotency_key,
                        on_saved,
                        on_reload,
                    ),
                    if pending() { "Saving form delivery…" } else if idempotency_key().is_some() { "Retry form save" } else { "Save form delivery" }
                }
                if form.enabled && form.needs_revalidation {
                    span { class: "webhook-delivery-help", "New submissions stay in the inbox while this mapping is stale." }
                }
            }
            code { class: "visually-hidden", "Form: {form_id}" }
        }
    }
}

#[cfg(feature = "browser")]
fn submit_destination(
    mut destination: Signal<Option<WebhookDestinationResponse>>,
    mut destination_url: Signal<String>,
    mut destination_enabled: Signal<bool>,
    mut pending: Signal<bool>,
    mut message: Signal<Option<String>>,
    mut idempotency_key: Signal<Option<PreparedIdempotencyKey>>,
    mut load_error: Signal<bool>,
    destination_loading: Signal<bool>,
    mut funnel_snapshot: Signal<Option<WebhookFormDeliverySnapshot>>,
    mut funnel_loading: Signal<bool>,
    selected_funnel: Signal<Option<String>>,
    funnel_request: Signal<FunnelRequestState>,
) {
    if pending() {
        return;
    }
    let Some(current) = destination() else {
        return;
    };
    let url = destination_url().trim().to_owned();
    if url.is_empty() {
        message.set(Some("Enter a destination URL before saving.".to_owned()));
        load_error.set(true);
        return;
    }
    let enabled = destination_enabled();
    let body = serde_json::json!({
        "expected_version_id": current.destination_version_id,
        "expected_enabled": current.enabled,
        "destination_url": url,
        "enabled": enabled,
    })
    .to_string();
    let key = match prepare_idempotency_key(
        "PUT",
        "/api/webhook-delivery",
        Some(&body),
        idempotency_key(),
    ) {
        Ok(key) => key,
        Err(()) => {
            message.set(Some(
                "This browser could not prepare a retry-safe save.".to_owned(),
            ));
            load_error.set(true);
            return;
        }
    };
    idempotency_key.set(Some(key.clone()));
    pending.set(true);
    message.set(Some("Validating the HTTPS destination…".to_owned()));
    load_error.set(false);
    spawn(async move {
        let result = save_webhook_destination(
            current.destination_version_id.as_deref(),
            current.enabled,
            &url,
            enabled,
            key.clone(),
        )
        .await;
        match result {
            Ok(saved) => {
                let _ = clear_idempotency_storage(&key);
                idempotency_key.set(None);
                destination_url.set(saved.destination_url.clone().unwrap_or_default());
                destination_enabled.set(saved.enabled);
                destination.set(Some(saved));
                message.set(Some(
                    "Destination saved. Recheck each form mapping after a destination change."
                        .to_owned(),
                ));
                if let Some(funnel_id) = selected_funnel() {
                    let request_generation =
                        begin_funnel_request(funnel_request, Some(funnel_id.clone()));
                    funnel_loading.set(true);
                    let result = load_webhook_form_delivery(&funnel_id).await;
                    if funnel_request_is_current(
                        funnel_request,
                        selected_funnel,
                        &funnel_id,
                        request_generation,
                    ) {
                        match result {
                            Ok(snapshot) if snapshot.funnel_id == funnel_id => {
                                funnel_snapshot.set(Some(snapshot))
                            }
                            Ok(_) | Err(()) => funnel_snapshot.set(None),
                        }
                        funnel_loading.set(false);
                    }
                }
            }
            Err(DeliveryControlFailure::Conflict) => {
                message.set(Some("The destination changed elsewhere. Reload the latest settings before saving again.".to_owned()));
                load_error.set(true);
                reload_destination(
                    destination,
                    destination_url,
                    destination_enabled,
                    load_error,
                    destination_loading,
                );
            }
            Err(DeliveryControlFailure::Uncertain) => {
                message.set(Some("The server may have saved this destination. Retry with the same values to confirm safely.".to_owned()));
                load_error.set(true);
            }
            Err(DeliveryControlFailure::Rejected) => {
                message.set(Some("This URL was rejected. Use a public HTTPS receiver without credentials or a fragment.".to_owned()));
                load_error.set(true);
            }
        }
        pending.set(false);
    });
}

#[cfg(feature = "browser")]
fn save_form_delivery(
    funnel_id: String,
    published_revision_id: String,
    form: WebhookFormDeliveryOption,
    destination_version_id: Option<String>,
    selected_field_ids: Signal<BTreeSet<String>>,
    enabled: Signal<bool>,
    funnel_request: Signal<FunnelRequestState>,
    mut pending: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut idempotency_key: Signal<Option<PreparedIdempotencyKey>>,
    on_saved: EventHandler<String>,
    on_reload: EventHandler<()>,
) {
    if pending() {
        return;
    }
    let Some(destination_version_id) = destination_version_id else {
        error.set(Some(
            "Save and validate a destination before configuring forms.".to_owned(),
        ));
        return;
    };
    let selected = selected_field_ids().into_iter().collect::<Vec<_>>();
    if selected.is_empty() {
        error.set(Some(
            "Choose at least one field to include in the webhook.".to_owned(),
        ));
        return;
    }
    let path = format!("/api/funnels/{funnel_id}/webhook-delivery");
    let body = serde_json::json!({
        "form_id": form.form_id,
        "expected_published_revision_id": published_revision_id,
        "destination_version_id": destination_version_id,
        "expected_mapping_version_id": form.mapping_version_id,
        "enabled": enabled(),
        "selected_field_ids": selected,
    })
    .to_string();
    let key = match prepare_idempotency_key("PUT", &path, Some(&body), idempotency_key()) {
        Ok(key) => key,
        Err(()) => {
            error.set(Some(
                "This browser could not prepare a retry-safe save.".to_owned(),
            ));
            return;
        }
    };
    idempotency_key.set(Some(key.clone()));
    pending.set(true);
    error.set(None);
    begin_funnel_request(funnel_request, Some(funnel_id.clone()));
    spawn(async move {
        match save_webhook_form_delivery(
            &funnel_id,
            &published_revision_id,
            &form.form_id,
            &destination_version_id,
            form.mapping_version_id.as_deref(),
            enabled(),
            &selected,
            key.clone(),
        )
        .await
        {
            Ok(_) => {
                let _ = clear_idempotency_storage(&key);
                idempotency_key.set(None);
                on_saved.call(funnel_id.clone());
            }
            Err(DeliveryControlFailure::Conflict) => {
                error.set(Some(
                    "Published form or delivery settings changed. Refreshing the latest version."
                        .to_owned(),
                ));
                on_reload.call(());
            }
            Err(DeliveryControlFailure::Uncertain) => {
                error.set(Some("The server may have saved this mapping. Retry with the same settings to confirm safely.".to_owned()));
            }
            Err(DeliveryControlFailure::Rejected) => {
                error.set(Some("The mapping was rejected. Check that at least one current form field is selected.".to_owned()));
            }
        }
        pending.set(false);
    });
}

#[cfg(feature = "browser")]
fn reload_destination(
    mut destination: Signal<Option<WebhookDestinationResponse>>,
    mut destination_url: Signal<String>,
    mut destination_enabled: Signal<bool>,
    mut error: Signal<bool>,
    mut loading: Signal<bool>,
) {
    loading.set(true);
    error.set(false);
    spawn(async move {
        match load_webhook_destination().await {
            Ok(snapshot) => {
                destination_url.set(snapshot.destination_url.clone().unwrap_or_default());
                destination_enabled.set(snapshot.enabled);
                destination.set(Some(snapshot));
            }
            Err(()) => error.set(true),
        }
        loading.set(false);
    });
}

#[cfg(feature = "browser")]
fn reload_funnel(
    selected_funnel: Signal<Option<String>>,
    funnel_request: Signal<FunnelRequestState>,
    mut snapshot: Signal<Option<WebhookFormDeliverySnapshot>>,
    mut loading: Signal<bool>,
    mut error: Signal<bool>,
) {
    let Some(funnel_id) = selected_funnel() else {
        return;
    };
    let request_generation = begin_funnel_request(funnel_request, Some(funnel_id.clone()));
    snapshot.set(None);
    loading.set(true);
    error.set(false);
    spawn(async move {
        let result = load_webhook_form_delivery(&funnel_id).await;
        if !funnel_request_is_current(
            funnel_request,
            selected_funnel,
            &funnel_id,
            request_generation,
        ) {
            return;
        }
        match result {
            Ok(loaded) if loaded.funnel_id == funnel_id => snapshot.set(Some(loaded)),
            Ok(_) | Err(()) => error.set(true),
        }
        loading.set(false);
    });
}

#[cfg(feature = "browser")]
fn begin_funnel_request(
    mut request_state: Signal<FunnelRequestState>,
    funnel_id: Option<String>,
) -> u64 {
    request_state.with_mut(|state| state.begin(funnel_id))
}

#[cfg(feature = "browser")]
fn funnel_request_is_current(
    request_state: Signal<FunnelRequestState>,
    selected_funnel: Signal<Option<String>>,
    funnel_id: &str,
    generation: u64,
) -> bool {
    selected_funnel().as_deref() == Some(funnel_id)
        && request_state().is_current(funnel_id, generation)
}

#[cfg(any(feature = "browser", test))]
fn should_reload_after_save(selected_funnel: Option<&str>, saved_funnel_id: &str) -> bool {
    selected_funnel == Some(saved_funnel_id)
}

#[cfg(test)]
mod tests {
    use super::{should_reload_after_save, FunnelRequestState, WebhookStatusCopy};

    #[test]
    fn late_funnel_response_cannot_replace_the_current_selection() {
        let mut requests = FunnelRequestState::default();
        let funnel_a_request = requests.begin(Some("funnel-a".to_owned()));
        let funnel_b_request = requests.begin(Some("funnel-b".to_owned()));

        assert!(!requests.is_current("funnel-a", funnel_a_request));
        assert!(requests.is_current("funnel-b", funnel_b_request));
    }

    #[test]
    fn every_successful_save_for_the_selected_funnel_can_trigger_a_converging_reload() {
        let mut requests = FunnelRequestState::default();
        let first_save = requests.begin(Some("funnel-a".to_owned()));
        let second_save = requests.begin(Some("funnel-a".to_owned()));
        let second_save_reload = requests.begin(Some("funnel-a".to_owned()));

        assert!(!requests.is_current("funnel-a", first_save));
        assert!(should_reload_after_save(Some("funnel-a"), "funnel-a"));
        let first_save_reload = requests.begin(Some("funnel-a".to_owned()));
        assert!(!requests.is_current("funnel-a", second_save_reload));
        assert!(requests.is_current("funnel-a", first_save_reload));
        assert!(!should_reload_after_save(Some("funnel-b"), "funnel-a"));
        assert!(!requests.is_current("funnel-a", second_save));
    }

    #[test]
    fn webhook_status_copy_keeps_inbox_only_distinct_from_delivery_failure() {
        assert_eq!(
            WebhookStatusCopy::from_wire("inbox_only").label,
            "Inbox only"
        );
        assert_eq!(
            WebhookStatusCopy::from_wire("failed").label,
            "Delivery failed"
        );
        assert_ne!(
            WebhookStatusCopy::from_wire("inbox_only").description,
            WebhookStatusCopy::from_wire("failed").description
        );
    }
}

#[derive(Clone, Copy)]
pub(super) struct WebhookStatusCopy {
    pub(super) label: &'static str,
    pub(super) description: &'static str,
}

impl WebhookStatusCopy {
    fn from_wire(status: &str) -> Self {
        match status {
            "pending" => Self { label: "Queued", description: "Waiting for the next delivery attempt." },
            "retry_scheduled" => Self { label: "Retry scheduled", description: "A temporary receiver or network issue scheduled another attempt." },
            "sending" => Self { label: "Sending", description: "A delivery attempt is in progress." },
            "accepted" => Self { label: "Accepted by receiver", description: "The receiver returned a success response; this does not confirm human follow-up." },
            "failed" => Self { label: "Delivery failed", description: "The webhook could not be delivered. The response remains in the inbox." },
            "cancelled" => Self { label: "Cancelled", description: "No more webhook attempts will be made." },
            _ => Self { label: "Inbox only", description: "No delivery task was created. The response remains available in the inbox." },
        }
    }
}

pub(super) fn webhook_status_copy(status: &str) -> WebhookStatusCopy {
    WebhookStatusCopy::from_wire(status)
}
