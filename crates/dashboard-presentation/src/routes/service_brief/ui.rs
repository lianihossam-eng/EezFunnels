use dioxus::prelude::*;
#[cfg(feature = "browser")]
use eez_domain::service_brief::{
    ServiceBriefDraft, ServiceIntention, SERVICE_BRIEF_FIELD_MAX_BYTES,
};

#[cfg(feature = "browser")]
use super::state::{
    SaveCapture, ServiceBriefEditorView, ServiceBriefField, ServiceBriefPageState,
    ServiceBriefPageView, ServiceBriefSavePhaseView,
};
use crate::routes::RouteView;

#[cfg(feature = "browser")]
use crate::routes::api::{load_service_brief, save_service_brief};

#[cfg(feature = "browser")]
#[component]
pub(crate) fn ServiceBriefPage(id: String) -> Element {
    let mut state = use_signal(|| ServiceBriefPageState::new(id.clone()));
    use_effect(use_reactive!(|(id)| {
        let funnel_id = id.clone();
        let route_epoch = state.with_mut(|state| state.begin_route(funnel_id.clone()));
        spawn(async move {
            let result = load_service_brief(&funnel_id).await;
            state.with_mut(|state| state.complete_load(&funnel_id, route_epoch, result));
        });
    }));

    let view = state.with(|state| state.view());
    match view {
        ServiceBriefPageView::Loading => rsx! {
            section { class: "service-brief-page",
                PageHeading { id: id.clone() }
                section { class: "service-brief-loading", role: "status", "aria-label": "Loading service brief",
                    div { class: "service-brief-skeleton service-brief-skeleton-title" }
                    div { class: "service-brief-skeleton" }
                    div { class: "service-brief-skeleton service-brief-skeleton-short" }
                    div { class: "service-brief-skeleton service-brief-skeleton-field" }
                    div { class: "service-brief-skeleton service-brief-skeleton-field" }
                    span { class: "visually-hidden", "Loading service brief…" }
                }
            }
        },
        ServiceBriefPageView::LoadError => rsx! {
            section { class: "service-brief-page",
                PageHeading { id: id.clone() }
                section { class: "service-brief-message service-brief-error", role: "alert",
                    h2 { "The service brief could not be loaded" }
                    p { "Your saved notes have not been changed. Check the connection and try again." }
                    button {
                        class: "secondary-button",
                        r#type: "button",
                        onclick: move |_| {
                            let (funnel_id, route_epoch) = state.with_mut(ServiceBriefPageState::retry_load);
                            spawn(async move {
                                let result = load_service_brief(&funnel_id).await;
                                state.with_mut(|state| state.complete_load(&funnel_id, route_epoch, result));
                            });
                        },
                        "Try again"
                    }
                }
            }
        },
        ServiceBriefPageView::Ready(view) => rsx! {
            section { class: "service-brief-page",
                PageHeading { id: id.clone() }
                ServiceBriefForm { state, id, view }
            }
        },
    }
}

#[cfg(not(feature = "browser"))]
#[component]
pub(crate) fn ServiceBriefPage(id: String) -> Element {
    rsx! {
        section { class: "service-brief-page",
            PageHeading { id }
            section { class: "service-brief-message", role: "status",
                "Open this page in the dashboard to edit its private service brief."
            }
        }
    }
}

#[component]
fn PageHeading(id: String) -> Element {
    rsx! {
        header { class: "page-heading service-brief-heading",
            h1 { "Service brief" }
            p { class: "page-description",
                "Private planning notes for this funnel. They are not shown on public pages, previews, or reports."
            }
            Link {
                to: RouteView::FunnelDetail { id },
                class: "secondary-button service-brief-back-link",
                "Back to funnel editor"
            }
        }
    }
}

#[cfg(feature = "browser")]
#[component]
fn ServiceBriefForm(
    state: Signal<ServiceBriefPageState>,
    id: String,
    view: Box<ServiceBriefEditorView>,
) -> Element {
    let status = save_status(&view);
    let mut save_state = state;
    let funnel_id = id.clone();
    rsx! {
        form {
            class: "service-brief-form",
            onsubmit: move |event| {
                event.prevent_default();
                let attempt = save_state.with_mut(ServiceBriefPageState::begin_save);
                if let Ok(Some(attempt)) = attempt {
                    submit_brief_save(save_state, attempt);
                }
            },
            section { class: "service-brief-intention-section",
                fieldset { class: "service-brief-intentions",
                    legend { "What should this funnel invite visitors to request?" }
                    p { class: "service-brief-help",
                        "Choose the purpose that best fits the offer. You can leave it undecided while drafting."
                    }
                    IntentionOption {
                        checked: view.draft.intention.is_none(),
                        enabled: view.can_edit,
                        value: "undecided",
                        title: "Decide later",
                        description: "Keep the brief open while you shape the offer.",
                        intention: None,
                        state: save_state,
                    }
                    IntentionOption {
                        checked: view.draft.intention == Some(ServiceIntention::Contact),
                        enabled: view.can_edit,
                        value: "contact",
                        title: "Contact",
                        description: "Invite a direct question or conversation about the service.",
                        intention: Some(ServiceIntention::Contact),
                        state: save_state,
                    }
                    IntentionOption {
                        checked: view.draft.intention == Some(ServiceIntention::QuoteRequest),
                        enabled: view.can_edit,
                        value: "quote_request",
                        title: "Quote request",
                        description: "Collect the details needed to prepare a scoped estimate.",
                        intention: Some(ServiceIntention::QuoteRequest),
                        state: save_state,
                    }
                    IntentionOption {
                        checked: view.draft.intention == Some(ServiceIntention::DiagnosticApplication),
                        enabled: view.can_edit,
                        value: "diagnostic_application",
                        title: "Diagnostic candidacy",
                        description: "Invite an initial fit-assessment request; no session is booked.",
                        intention: Some(ServiceIntention::DiagnosticApplication),
                        state: save_state,
                    }
                }
                p { class: "service-brief-boundary-note",
                    "These choices do not add booking, payment, scoring, or automated follow-up."
                }
            }
            section { class: "service-brief-fields-section",
                h2 { "Service context" }
                p { class: "service-brief-help",
                    "You can save a partial brief and return to it later. None of these notes are published."
                }
                div { class: "service-brief-fields",
                    BriefTextField { id: "service-brief-audience", label: "Audience", value: view.draft.audience.clone(), enabled: view.can_edit, field: ServiceBriefField::Audience, state: save_state }
                    BriefTextField { id: "service-brief-need", label: "Need", value: view.draft.need.clone(), enabled: view.can_edit, field: ServiceBriefField::Need, state: save_state }
                    BriefTextField { id: "service-brief-offer", label: "Offer", value: view.draft.offer.clone(), enabled: view.can_edit, field: ServiceBriefField::Offer, state: save_state }
                    BriefTextField { id: "service-brief-scope", label: "Scope", value: view.draft.scope.clone(), enabled: view.can_edit, field: ServiceBriefField::Scope, state: save_state }
                    BriefTextField { id: "service-brief-terms", label: "Conditions and terms", value: view.draft.terms.clone(), enabled: view.can_edit, field: ServiceBriefField::Terms, state: save_state }
                    BriefTextField { id: "service-brief-action", label: "Intended visitor action", value: view.draft.action.clone(), enabled: view.can_edit, field: ServiceBriefField::Action, state: save_state }
                    BriefTextField { id: "service-brief-next-step", label: "Promised next step", value: view.draft.promised_next_step.clone(), enabled: view.can_edit, field: ServiceBriefField::PromisedNextStep, state: save_state }
                    BriefTextField { id: "service-brief-evidence", label: "Evidence available", value: view.draft.available_evidence.clone(), enabled: view.can_edit, field: ServiceBriefField::AvailableEvidence, state: save_state }
                }
            }
            footer { class: "service-brief-footer",
                div { class: "service-brief-save-state", role: status.0, "aria-live": "polite",
                    p { "{status.1}" }
                    if let ServiceBriefSavePhaseView::Conflict { latest } = &view.phase {
                        p { "Latest saved version: {latest.version.value()}. Your draft is still here." }
                        LatestBriefSummary {
                            version: latest.version.value(),
                            brief: latest.brief.draft(),
                        }
                    }
                }
                div { class: "service-brief-actions",
                    if matches!(&view.phase, ServiceBriefSavePhaseView::Uncertain) {
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            onclick: move |_| {
                                if let Some(attempt) = save_state.with_mut(ServiceBriefPageState::retry_uncertain_save) {
                                    submit_brief_save(save_state, attempt);
                                }
                            },
                            "Retry save"
                        }
                    }
                    if matches!(&view.phase, ServiceBriefSavePhaseView::Conflict { .. }) {
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            onclick: move |_| save_state.with_mut(ServiceBriefPageState::load_latest_after_conflict),
                            "Replace my draft with latest"
                        }
                        button {
                            class: "primary-button",
                            r#type: "button",
                            onclick: move |_| {
                                let attempt = save_state.with_mut(ServiceBriefPageState::save_my_draft_over_latest);
                                if let Ok(Some(attempt)) = attempt {
                                    submit_brief_save(save_state, attempt);
                                }
                            },
                            "Save my draft over latest"
                        }
                    }
                    if matches!(&view.phase, ServiceBriefSavePhaseView::ConflictLoadFailed) {
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            onclick: move |_| {
                                if let Some(attempt) = save_state.with_mut(ServiceBriefPageState::retry_conflict_load) {
                                    refresh_conflict(save_state, attempt);
                                }
                            },
                            "Check latest version"
                        }
                    }
                    if !matches!(&view.phase, ServiceBriefSavePhaseView::Conflict { .. } | ServiceBriefSavePhaseView::ConflictLoadFailed) {
                        button {
                            class: "primary-button",
                            r#type: "submit",
                            disabled: !view.can_edit || !view.dirty,
                            "Save private brief"
                        }
                    }
                }
            }
            code { class: "visually-hidden", "Funnel: {funnel_id}" }
        }
    }
}

#[cfg(feature = "browser")]
#[component]
fn IntentionOption(
    checked: bool,
    enabled: bool,
    value: &'static str,
    title: &'static str,
    description: &'static str,
    intention: Option<ServiceIntention>,
    mut state: Signal<ServiceBriefPageState>,
) -> Element {
    rsx! {
        label { class: "service-brief-intention-option", r#for: "service-intention-{value}",
            input {
                id: "service-intention-{value}",
                r#type: "radio",
                name: "service-intention",
                value,
                checked,
                disabled: !enabled,
                onchange: move |_| state.with_mut(|state| state.set_intention(intention)),
            }
            span { class: "service-brief-intention-copy",
                strong { "{title}" }
                span { "{description}" }
            }
        }
    }
}

#[cfg(feature = "browser")]
#[component]
fn BriefTextField(
    id: &'static str,
    label: &'static str,
    value: Option<String>,
    enabled: bool,
    field: ServiceBriefField,
    mut state: Signal<ServiceBriefPageState>,
) -> Element {
    rsx! {
        label { class: "editor-setting-control service-brief-field", r#for: "{id}",
            "{label}"
            textarea {
                id: "{id}",
                rows: 4,
                maxlength: SERVICE_BRIEF_FIELD_MAX_BYTES,
                disabled: !enabled,
                value: value.unwrap_or_default(),
                oninput: move |event| state.with_mut(|state| state.edit_text(field, event.value())),
            }
        }
    }
}

#[cfg(feature = "browser")]
#[component]
fn LatestBriefSummary(version: i32, brief: ServiceBriefDraft) -> Element {
    let intention = brief
        .intention
        .map(|intention| intention.as_str().to_owned())
        .unwrap_or_else(|| "Not decided".to_owned());
    let fields = [
        ("Intention", intention),
        ("Audience", summary_value(brief.audience)),
        ("Need", summary_value(brief.need)),
        ("Offer", summary_value(brief.offer)),
        ("Scope", summary_value(brief.scope)),
        ("Conditions and terms", summary_value(brief.terms)),
        ("Intended visitor action", summary_value(brief.action)),
        (
            "Promised next step",
            summary_value(brief.promised_next_step),
        ),
        (
            "Evidence available",
            summary_value(brief.available_evidence),
        ),
    ];
    rsx! {
        section { class: "service-brief-conflict-summary", "aria-labelledby": "service-brief-latest-heading",
            h3 { id: "service-brief-latest-heading", "Latest saved brief (version {version})" }
            dl {
                for (label, value) in fields {
                    div {
                        dt { "{label}" }
                        dd { "{value}" }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "browser")]
fn summary_value(value: Option<String>) -> String {
    value.unwrap_or_else(|| "Not set".to_owned())
}

#[cfg(feature = "browser")]
fn save_status(view: &ServiceBriefEditorView) -> (&'static str, String) {
    match &view.phase {
        ServiceBriefSavePhaseView::Editing if view.dirty => (
            "status",
            "Unsaved changes. Save to keep these notes with the funnel.".to_owned(),
        ),
        ServiceBriefSavePhaseView::Editing => (
            "status",
            "All changes saved. The current brief is versioned independently of the public page."
                .to_owned(),
        ),
        ServiceBriefSavePhaseView::Invalid => (
            "alert",
            "Some notes exceed the allowed size or contain unsupported control characters."
                .to_owned(),
        ),
        ServiceBriefSavePhaseView::Saving => ("status", "Saving the private brief…".to_owned()),
        ServiceBriefSavePhaseView::Uncertain => (
            "alert",
            "The save response was uncertain. Retry the same save to check whether it was stored."
                .to_owned(),
        ),
        ServiceBriefSavePhaseView::RefreshingConflict => {
            ("status", "Checking the latest saved version…".to_owned())
        }
        ServiceBriefSavePhaseView::Conflict { .. } => (
            "alert",
            "A newer version was saved elsewhere. Your draft has been preserved.".to_owned(),
        ),
        ServiceBriefSavePhaseView::ConflictLoadFailed => (
            "alert",
            "The latest version could not be loaded. Your draft has been preserved.".to_owned(),
        ),
        ServiceBriefSavePhaseView::Saved => (
            "status",
            format!("Service brief saved as version {}.", view.version.value()),
        ),
        ServiceBriefSavePhaseView::Rejected => (
            "alert",
            "The brief was not saved. Check your connection and try again.".to_owned(),
        ),
    }
}

#[cfg(feature = "browser")]
fn submit_brief_save(mut state: Signal<ServiceBriefPageState>, attempt: SaveCapture) {
    spawn(async move {
        let result =
            save_service_brief(&attempt.funnel_id, attempt.expected_version, &attempt.brief).await;
        let refresh = state.with_mut(|state| state.complete_save(&attempt, result));
        if refresh {
            refresh_conflict(state, attempt);
        }
    });
}

#[cfg(feature = "browser")]
fn refresh_conflict(mut state: Signal<ServiceBriefPageState>, attempt: SaveCapture) {
    spawn(async move {
        let latest = load_service_brief(&attempt.funnel_id).await;
        state.with_mut(|state| state.complete_conflict_load(&attempt, latest));
    });
}
