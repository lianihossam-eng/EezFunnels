use dioxus::prelude::*;

use super::RouteView;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(not(feature = "browser"), allow(dead_code))]
struct CampaignRequestState {
    funnel_id: Option<String>,
    route_epoch: u64,
    load_epoch: u64,
}

#[cfg_attr(not(feature = "browser"), allow(dead_code))]
impl CampaignRequestState {
    fn begin_route(&mut self, funnel_id: String) -> (u64, u64) {
        self.route_epoch = self.route_epoch.wrapping_add(1);
        self.funnel_id = Some(funnel_id);
        self.begin_load()
    }

    fn begin_load(&mut self) -> (u64, u64) {
        self.load_epoch = self.load_epoch.wrapping_add(1);
        (self.route_epoch, self.load_epoch)
    }

    fn is_current_route(&self, funnel_id: &str, route_epoch: u64) -> bool {
        self.route_epoch == route_epoch && self.funnel_id.as_deref() == Some(funnel_id)
    }

    fn is_current_load(&self, funnel_id: &str, route_epoch: u64, load_epoch: u64) -> bool {
        self.is_current_route(funnel_id, route_epoch) && self.load_epoch == load_epoch
    }
}

#[cfg_attr(not(feature = "browser"), allow(dead_code))]
#[derive(Clone, Debug, Eq, PartialEq)]
enum CampaignOperation {
    Add(String),
    Remove(String),
}

#[cfg(feature = "browser")]
impl CampaignOperation {
    fn campaign_id(&self) -> &str {
        match self {
            Self::Add(campaign_id) | Self::Remove(campaign_id) => campaign_id,
        }
    }

    fn is_applied(&self, snapshot: &super::api::CampaignRegistryResponse) -> bool {
        let contains = snapshot
            .campaign_ids
            .iter()
            .any(|campaign_id| campaign_id == self.campaign_id());
        match self {
            Self::Add(_) => contains,
            Self::Remove(_) => !contains,
        }
    }
}

#[component]
#[cfg_attr(not(feature = "browser"), allow(unused_mut, unused_variables))]
pub(crate) fn CampaignLinksPage(id: String) -> Element {
    use super::api::CampaignRegistryResponse;
    use eez_domain::interaction_event::CampaignId;

    let mut request_state = use_signal(CampaignRequestState::default);
    let mut active_funnel_id = use_signal(|| id.clone());
    let mut snapshot = use_signal(|| None::<CampaignRegistryResponse>);
    let mut campaign_input = use_signal(String::new);
    let mut loading = use_signal(|| true);
    let mut load_error = use_signal(|| false);
    let mut pending = use_signal(|| None::<CampaignOperation>);
    let mut message = use_signal(|| None::<String>);
    let message_is_error = use_signal(|| false);

    #[cfg(feature = "browser")]
    use_effect(use_reactive!(|(id)| {
        let (route_epoch, load_epoch) =
            request_state.with_mut(|state| state.begin_route(id.clone()));
        active_funnel_id.set(id.clone());
        snapshot.set(None);
        campaign_input.set(String::new());
        pending.set(None);
        load_error.set(false);
        message.set(None);
        loading.set(true);
        let funnel_id = id.clone();
        spawn(async move {
            let result = super::api::load_campaign_registry(&funnel_id).await;
            if !request_state
                .with(|state| state.is_current_load(&funnel_id, route_epoch, load_epoch))
            {
                return;
            }
            match result {
                Ok(loaded) => snapshot.set(Some(loaded)),
                Err(()) => load_error.set(true),
            }
            loading.set(false);
        });
    }));

    let current_snapshot = snapshot();
    let campaign_ids = current_snapshot
        .as_ref()
        .map(|current| current.campaign_ids.clone())
        .unwrap_or_default();
    let max_campaign_ids = current_snapshot
        .as_ref()
        .map(|current| current.max_campaign_ids)
        .unwrap_or(0);
    let value = campaign_input();
    let is_valid_id = CampaignId::parse(value.clone()).is_ok();
    let is_registered = campaign_ids.iter().any(|campaign_id| campaign_id == &value);
    let is_pending = pending().is_some();
    let can_add = !loading()
        && !load_error()
        && !is_pending
        && is_valid_id
        && !is_registered
        && campaign_ids.len() < max_campaign_ids;
    let campaign_links = current_snapshot
        .as_ref()
        .map(|current| current.campaign_links.clone())
        .unwrap_or_default();

    rsx! {
        section { class: "campaign-page",
            header { class: "page-heading campaign-heading",
                h1 { "Campaign links" }
                p { class: "page-description",
                    "Create links that label consented interactions on this funnel’s entry page. Campaign IDs never identify a visitor or follow them to other pages."
                }
                Link {
                    to: RouteView::FunnelDetail { id: id.clone() },
                    class: "secondary-button campaign-back-link",
                    "Back to funnel editor"
                }
            }
            section { class: "campaign-registry-panel", "aria-labelledby": "campaign-registry-heading",
                h2 { id: "campaign-registry-heading", "Campaign IDs" }
                p { class: "campaign-registry-intro",
                    "An ID is matched exactly and case-sensitively. Measurement consent is still required. Registering or removing an ID affects future events only; retained events keep their recorded attribution."
                }
                if loading() {
                    div { class: "campaign-loading", role: "status", "aria-label": "Loading campaign IDs",
                        div { class: "campaign-skeleton campaign-skeleton-title" }
                        div { class: "campaign-skeleton" }
                        span { class: "visually-hidden", "Loading campaign IDs…" }
                    }
                } else if load_error() {
                    div { class: "campaign-state campaign-state-error", role: "alert",
                        if let Some(error_message) = message() {
                            p { "{error_message}" }
                        } else {
                            p { "Campaign IDs could not be loaded. Reload before trying another change." }
                        }
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            onclick: move |_| {
                                #[cfg(feature = "browser")]
                                load_campaigns(
                                    active_funnel_id(), request_state, snapshot, loading, load_error, message,
                                );
                            },
                            "Reload campaign IDs"
                        }
                    }
                } else if let Some(current) = current_snapshot.as_ref() {
                    form {
                        class: "campaign-add-form",
                        onsubmit: move |event| {
                            event.prevent_default();
                            #[cfg(feature = "browser")]
                            if can_add {
                                apply_campaign_operation(
                                    active_funnel_id(),
                                    CampaignOperation::Add(campaign_input()),
                                    request_state,
                                    snapshot,
                                    campaign_input,
                                    loading,
                                    load_error,
                                    pending,
                                    message,
                                    message_is_error,
                                );
                            }
                        },
                        label { r#for: "campaign-id", "Campaign ID" }
                        div { class: "campaign-add-row",
                            input {
                                id: "campaign-id",
                                name: "campaign_id",
                                r#type: "text",
                                value: "{value}",
                                maxlength: "64",
                                pattern: "[A-Za-z0-9][A-Za-z0-9_-]{{0,63}}",
                                required: true,
                                disabled: is_pending || campaign_ids.len() >= current.max_campaign_ids,
                                "aria-describedby": "campaign-id-help",
                                "aria-invalid": !value.is_empty() && !is_valid_id,
                                oninput: move |event| {
                                    #[cfg(feature = "browser")]
                                    campaign_input.set(event.value());
                                },
                            }
                            button {
                                class: "primary-button",
                                r#type: "submit",
                                disabled: !can_add,
                                if matches!(pending(), Some(CampaignOperation::Add(_))) { "Adding…" } else { "Add campaign ID" }
                            }
                        }
                        p { id: "campaign-id-help", class: "campaign-field-help",
                            "Use 1–64 letters, numbers, hyphens or underscores. Start with a letter or number."
                        }
                    }
                    if !value.is_empty() && !is_valid_id {
                        p { class: "campaign-state campaign-state-error", role: "alert",
                            "This ID has unsupported characters or an invalid length."
                        }
                    }
                    if campaign_ids.len() >= current.max_campaign_ids {
                        p { class: "campaign-capacity", role: "status",
                            "The registry is full ({campaign_ids.len()} of {current.max_campaign_ids}). Remove an ID before adding another."
                        }
                    } else {
                        p { class: "campaign-capacity", role: "status",
                            "{campaign_ids.len()} of {current.max_campaign_ids} campaign IDs used."
                        }
                    }
                    if message().is_some() {
                        p {
                            class: if message_is_error() { "campaign-state campaign-state-error" } else { "campaign-state campaign-state-success" },
                            role: if message_is_error() { "alert" } else { "status" },
                            "{message().unwrap_or_default()}"
                        }
                    }
                    if current.campaign_ids.is_empty() {
                        section { class: "campaign-empty-state",
                            h3 { "No campaign IDs yet" }
                            p { "Add an ID to create its entry-page link. Unregistered values remain unattributed in interaction reports." }
                        }
                    } else {
                        ul { class: "campaign-list", "aria-label": "Registered campaign IDs",
                            for link in campaign_links.iter() {
                                CampaignRow {
                                    link: link.clone(),
                                    pending: is_pending,
                                    on_remove: move |campaign_id: String| {
                                        #[cfg(feature = "browser")]
                                        apply_campaign_operation(
                                            active_funnel_id(),
                                            CampaignOperation::Remove(campaign_id),
                                            request_state,
                                            snapshot,
                                            campaign_input,
                                            loading,
                                            load_error,
                                            pending,
                                            message,
                                            message_is_error,
                                        );
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn CampaignRow(
    link: super::api::CampaignLinkResponse,
    pending: bool,
    on_remove: EventHandler<String>,
) -> Element {
    let campaign_id = link.campaign_id.clone();
    let field_id = format!("campaign-link-{}", link.campaign_id);
    let visible_link = link.url.as_deref().or(link.relative_path.as_deref());
    rsx! {
        li { class: "campaign-row",
            div { class: "campaign-row-header",
                h3 { code { "{link.campaign_id}" } }
                button {
                    class: "secondary-button campaign-remove-button",
                    r#type: "button",
                    disabled: pending,
                    "aria-label": "Remove campaign {campaign_id}",
                    onclick: move |_| on_remove.call(campaign_id.clone()),
                    "Remove"
                }
            }
            if let Some(value) = visible_link {
                label { r#for: "{field_id}", if link.url.is_some() { "Generated link" } else { "Public link path" } }
                div { class: "campaign-link-row",
                    input {
                        id: "{field_id}",
                        r#type: "text",
                        value: "{value}",
                        readonly: true,
                        "aria-label": "Generated link for {link.campaign_id}",
                    }
                    if let Some(url) = link.url.as_ref() {
                        a {
                            class: "secondary-button campaign-open-link",
                            href: "{url}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            "Open link"
                        }
                    }
                }
                if link.url.is_none() {
                    p { class: "campaign-field-help",
                        "Set PUBLIC_BASE_URL or a custom funnel host to generate an absolute public link."
                    }
                }
            } else {
                p { class: "campaign-field-help", "Publish this funnel to generate a public campaign link." }
            }
        }
    }
}

#[cfg(feature = "browser")]
fn apply_campaign_operation(
    funnel_id: String,
    operation: CampaignOperation,
    request_state: Signal<CampaignRequestState>,
    mut snapshot: Signal<Option<super::api::CampaignRegistryResponse>>,
    mut campaign_input: Signal<String>,
    mut loading: Signal<bool>,
    mut load_error: Signal<bool>,
    mut pending: Signal<Option<CampaignOperation>>,
    mut message: Signal<Option<String>>,
    mut message_is_error: Signal<bool>,
) {
    use super::api::{
        add_campaign_id, load_campaign_registry, remove_campaign_id, CampaignMutationFailure,
    };

    if pending().is_some() {
        return;
    }
    let route_epoch = request_state.with(|state| state.route_epoch);
    let captured_id = operation.campaign_id().to_owned();
    let captured_operation = operation.clone();
    pending.set(Some(operation));
    loading.set(true);
    load_error.set(false);
    message.set(None);
    spawn(async move {
        let result = match &captured_operation {
            CampaignOperation::Add(_) => add_campaign_id(&funnel_id, &captured_id).await,
            CampaignOperation::Remove(_) => remove_campaign_id(&funnel_id, &captured_id).await,
        };
        let (resolved, was_applied, failure_message) = match result {
            Ok(updated) => {
                let applied = captured_operation.is_applied(&updated);
                let failure_message = (!applied).then_some(
                    "The response did not confirm the requested change. Review the refreshed registry before retrying.",
                );
                (Ok(updated), applied, failure_message)
            }
            Err(failure) => match load_campaign_registry(&funnel_id).await {
                Ok(latest) => {
                    let applied = captured_operation.is_applied(&latest);
                    let failure_message = if applied {
                        None
                    } else {
                        Some(match failure {
                            CampaignMutationFailure::Uncertain => "The response was not confirmed. The registry was refreshed; review it before trying again.",
                            CampaignMutationFailure::Rejected => "The change was rejected. The registry was refreshed; check its current state and try again.",
                        })
                    };
                    (Ok(latest), applied, failure_message)
                }
                Err(()) => (
                    Err("The change could not be confirmed, and the registry could not be refreshed. Try reloading the page."),
                    false,
                    None,
                ),
            },
        };
        if !request_state.with(|state| state.is_current_route(&funnel_id, route_epoch)) {
            return;
        }
        pending.set(None);
        loading.set(false);
        match resolved {
            Ok(updated) => {
                snapshot.set(Some(updated));
                if was_applied {
                    if matches!(captured_operation, CampaignOperation::Add(_))
                        && campaign_input() == captured_id
                    {
                        campaign_input.set(String::new());
                    }
                    message.set(Some(operation_success_copy(&captured_operation)));
                    message_is_error.set(false);
                } else {
                    message.set(failure_message.map(str::to_owned));
                    message_is_error.set(true);
                }
            }
            Err(error) => {
                load_error.set(true);
                message.set(Some(error.to_owned()));
                message_is_error.set(true);
            }
        }
    });
}

#[cfg(feature = "browser")]
fn operation_success_copy(operation: &CampaignOperation) -> String {
    match operation {
        CampaignOperation::Add(campaign_id) => format!("Campaign {campaign_id} is registered."),
        CampaignOperation::Remove(campaign_id) => format!("Campaign {campaign_id} was removed."),
    }
}

#[cfg(feature = "browser")]
fn load_campaigns(
    funnel_id: String,
    mut request_state: Signal<CampaignRequestState>,
    mut snapshot: Signal<Option<super::api::CampaignRegistryResponse>>,
    mut loading: Signal<bool>,
    mut load_error: Signal<bool>,
    mut message: Signal<Option<String>>,
) {
    let (route_epoch, load_epoch) = request_state.with_mut(CampaignRequestState::begin_load);
    loading.set(true);
    load_error.set(false);
    message.set(None);
    spawn(async move {
        let result = super::api::load_campaign_registry(&funnel_id).await;
        if !request_state.with(|state| state.is_current_load(&funnel_id, route_epoch, load_epoch)) {
            return;
        }
        match result {
            Ok(loaded) => snapshot.set(Some(loaded)),
            Err(()) => load_error.set(true),
        }
        loading.set(false);
    });
}

#[cfg(test)]
mod tests {
    use super::CampaignRequestState;

    #[test]
    fn campaign_loads_from_an_old_funnel_or_generation_cannot_overwrite_current_state() {
        let mut requests = CampaignRequestState::default();
        let (first_route, first_load) = requests.begin_route("funnel-01".to_owned());
        let (second_route, second_load) = requests.begin_route("funnel-02".to_owned());

        assert!(!requests.is_current_load("funnel-01", first_route, first_load));
        assert!(requests.is_current_load("funnel-02", second_route, second_load));

        let (refresh_route, refresh_load) = requests.begin_load();
        assert!(!requests.is_current_load("funnel-02", second_route, second_load));
        assert!(requests.is_current_load("funnel-02", refresh_route, refresh_load));
    }
}
