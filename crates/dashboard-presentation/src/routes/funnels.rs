use dioxus::prelude::*;
use eez_domain::{document_v2::PublicLanguage, FunnelBlueprint};
#[cfg(any(feature = "browser", test))]
use std::future::Future;

use super::api::FunnelCard;
#[cfg(feature = "browser")]
use super::api::{api_request, load_funnels};
#[cfg(any(feature = "browser", test))]
use super::api::{created_summary, error_message, BrowserResponse};
#[cfg(feature = "browser")]
use super::browser::set_modal_dialog;
use super::idempotency::IdempotencyKeyState;
#[cfg(feature = "browser")]
use super::idempotency::{clear_idempotency_storage, prepare_idempotency_key};
use super::RouteView;

const MAX_FUNNEL_NAME_BYTES: usize = 120;
const FUNNEL_NAME_LIMIT_MESSAGE: &str = "Funnel name must be 120 UTF-8 bytes or fewer.";

#[cfg(any(feature = "browser", test))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct FunnelLoadVersion(u64);

#[cfg(any(feature = "browser", test))]
impl FunnelLoadVersion {
    fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    fn is_current(self, latest: Self) -> bool {
        self == latest
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct FunnelListLoad {
    #[cfg(any(feature = "browser", test))]
    latest: FunnelLoadVersion,
    pub(super) response: Option<Result<Vec<FunnelCard>, ()>>,
}

impl FunnelListLoad {
    #[cfg(any(feature = "browser", test))]
    fn begin(&mut self) -> FunnelLoadVersion {
        let request = self.latest.next();
        self.latest = request;
        self.response = None;
        request
    }

    #[cfg(any(feature = "browser", test))]
    fn complete(&mut self, request: FunnelLoadVersion, response: Result<Vec<FunnelCard>, ()>) {
        if request.is_current(self.latest) {
            self.response = Some(response);
        }
    }

    #[cfg(any(feature = "browser", test))]
    fn delete_succeeded(&mut self, funnel_id: &str) {
        self.latest = self.latest.next();
        if let Some(Ok(items)) = self.response.as_mut() {
            items.retain(|item| item.id != funnel_id);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DialogCancelDisposition {
    KeepOpen,
    Dismiss,
}

fn dialog_cancel_disposition(mutation_pending: bool) -> DialogCancelDisposition {
    if mutation_pending {
        DialogCancelDisposition::KeepOpen
    } else {
        DialogCancelDisposition::Dismiss
    }
}

#[component]
pub(super) fn Funnels() -> Element {
    let load_state = use_signal(FunnelListLoad::default);
    let mut create_open = use_signal(|| false);
    let mut funnel_name = use_signal(String::new);
    let mut blueprint = use_signal(|| FunnelBlueprint::Tripwire);
    let mut language = use_signal(|| PublicLanguage::English);
    let mut create_pending = use_signal(|| false);
    let mut create_error = use_signal(|| None::<String>);
    let mut create_idempotency_key = use_signal(|| None::<IdempotencyKeyState>);
    let mut create_success = use_signal(|| None::<String>);
    let mut delete_target = use_signal(|| None::<String>);
    let mut delete_pending = use_signal(|| false);
    let mut delete_error = use_signal(|| None::<String>);
    let mut delete_idempotency_key = use_signal(|| None::<IdempotencyKeyState>);
    let mut delete_idempotency_target = use_signal(|| None::<String>);

    #[cfg(feature = "browser")]
    {
        let page_state = load_state;
        use_effect(move || {
            spawn(async move {
                refresh_funnel_list(page_state).await;
            });
        });

        let create_modal_open = create_open;
        use_effect(move || {
            let open = create_modal_open();
            spawn(async move { set_modal_dialog("create-funnel-dialog", open).await });
        });

        let delete_modal_target = delete_target;
        use_effect(move || {
            let open = delete_modal_target().is_some();
            spawn(async move { set_modal_dialog("delete-funnel-dialog", open).await });
        });
    }

    rsx! {
        section { class: "page-heading",
            p { class: "eyebrow", "Workspace" }
            h1 { "Your funnels" }
            p { class: "page-description", "Review the funnels in your workspace." }
        }
        div { class: "section-heading funnel-list-heading",
            h2 { "Funnels" }
            button {
                class: "primary-button",
                r#type: "button",
                onclick: move |_| {
                    create_error.set(None);
                    create_success.set(None);
                    create_open.set(true);
                },
                "Create funnel"
            }
        }
        if let Some(message) = create_success() {
            p { class: "success-message", role: "status", "{message}" }
        }
        match load_state().response {
            None => rsx! {
                section { class: "empty-state", "aria-label": "Funnel list", role: "status",
                    "Loading your funnels…"
                }
            },
            Some(Err(())) => rsx! {
                section { class: "empty-state", "aria-label": "Funnel list", role: "status",
                    p { "Your funnels could not be loaded." }
                    button {
                        class: "secondary-button",
                        r#type: "button",
                        onclick: move |_| {
                            #[cfg(feature = "browser")]
                            spawn(async move {
                                refresh_funnel_list(load_state).await;
                            });
                        },
                        "Try again"
                    }
                }
            },
            Some(Ok(items)) if items.is_empty() => rsx! {
                section { class: "empty-state", "aria-label": "Funnel list",
                    h3 { "No funnels yet" }
                    p { "Create a funnel to start with a ready-to-edit page." }
                    button {
                        class: "secondary-button",
                        r#type: "button",
                        onclick: move |_| create_open.set(true),
                        "Create your first funnel"
                    }
                }
            },
            Some(Ok(items)) => rsx! {
                section { class: "funnel-list", "aria-label": "Funnel list",
                    for item in items {
                        article { class: "funnel-card",
                            div { class: "funnel-card-main",
                                h3 {
                                    Link {
                                        to: RouteView::FunnelDetail { id: item.id.clone() },
                                        class: "funnel-card-link",
                                        "{item.name}"
                                    }
                                }
                                p { class: "funnel-slug", "/{item.slug}" }
                            }
                            div { class: "funnel-card-meta",
                                span { class: if item.published_revision_id.is_some() { "status-badge published" } else { "status-badge draft" },
                                    if item.published_revision_id.is_some() { "Published" } else { "Draft" }
                                }
                                span { "Updated {item.updated_at}" }
                            }
                            button {
                                class: "text-button danger-button",
                                r#type: "button",
                                disabled: delete_pending(),
                                onclick: move |_| {
                                    if delete_idempotency_target().as_deref() != Some(item.id.as_str()) {
                                        delete_idempotency_key.set(None);
                                        delete_idempotency_target.set(Some(item.id.clone()));
                                    }
                                    delete_target.set(Some(item.id.clone()));
                                    delete_error.set(None);
                                },
                                "Delete"
                            }
                        }
                    }
                }
            },
        }

        dialog {
            id: "create-funnel-dialog",
            class: "funnel-dialog",
            "aria-modal": "true",
            "aria-labelledby": "create-funnel-title",
            "aria-describedby": "create-funnel-description",
            oncancel: move |event| match dialog_cancel_disposition(create_pending()) {
                DialogCancelDisposition::KeepOpen => event.prevent_default(),
                DialogCancelDisposition::Dismiss => create_open.set(false),
            },
            div { class: "dialog-heading",
                p { class: "eyebrow", "New workspace asset" }
                h2 { id: "create-funnel-title", "Create a funnel" }
                p { id: "create-funnel-description", "Choose a page outline or a working service inquiry flow. The other blueprints do not include checkout, registration, or automation." }
            }
            form {
                onsubmit: move |event| {
                    event.prevent_default();
                    if create_pending() || !can_create_funnel(&funnel_name()) { return; }
                    create_pending.set(true);
                    create_error.set(None);
                    #[cfg(feature = "browser")]
                    let request_name = funnel_name().trim().to_owned();
                    #[cfg(feature = "browser")]
                    let request_blueprint = blueprint();
                    #[cfg(feature = "browser")]
                    let request_language = language();
                    #[cfg(feature = "browser")]
                    let request_body = serde_json::json!({
                        "name": request_name.clone(),
                        "blueprint": request_blueprint.id(),
                        "language": request_language.as_str(),
                    }).to_string();
                    #[cfg(feature = "browser")]
                    let prepared_key = match prepare_idempotency_key(
                        "POST",
                        "/api/funnels/v2",
                        Some(&request_body),
                        create_idempotency_key(),
                    ) {
                        Ok(prepared_key) => prepared_key,
                        Err(()) => {
                            create_error.set(Some("The browser could not prepare a retry-safe request.".to_owned()));
                            create_pending.set(false);
                            return;
                        }
                    };
                    #[cfg(feature = "browser")]
                    create_idempotency_key.set(Some(prepared_key.clone()));
                    #[cfg(feature = "browser")]
                    spawn(async move {
                        let response = api_request(
                            "POST",
                            "/api/funnels/v2",
                            Some(request_body),
                            Some(prepared_key.clone()),
                        );
                        let retry_key = prepared_key.clone();
                        finish_create_response(
                            response,
                            request_name,
                            move |message| {
                                let _ = clear_idempotency_storage(&prepared_key);
                                create_success.set(Some(message));
                                create_open.set(false);
                                funnel_name.set(String::new());
                                blueprint.set(FunnelBlueprint::Tripwire);
                                language.set(PublicLanguage::English);
                                create_idempotency_key.set(None);
                            },
                            move |message| {
                                create_idempotency_key.set(Some(retry_key));
                                create_error.set(Some(message));
                            },
                            move || create_pending.set(false),
                            refresh_funnel_list(load_state),
                        ).await;
                    });
                    #[cfg(not(feature = "browser"))]
                    create_pending.set(false);
                },
                CreateFields {
                    name: funnel_name(),
                    blueprint: blueprint(),
                    language: language(),
                    pending: create_pending(),
                    on_name_change: move |value| {
                        if create_pending() { return; }
                        funnel_name.set(value);
                        create_idempotency_key.set(None);
                    },
                    on_blueprint_change: move |value| {
                        if create_pending() { return; }
                        blueprint.set(value);
                        create_idempotency_key.set(None);
                    },
                    on_language_change: move |value| {
                        if create_pending() { return; }
                        language.set(value);
                        create_idempotency_key.set(None);
                    },
                }
                if let Some(message) = create_error() {
                    p { class: "error-message", role: "alert", "{message}" }
                }
                div { class: "dialog-actions",
                    button {
                        class: "secondary-button",
                        r#type: "button",
                        disabled: create_pending(),
                        onclick: move |_| {
                            create_open.set(false);
                        },
                        "Cancel"
                    }
                    button {
                        class: "primary-button",
                        r#type: "submit",
                        disabled: create_pending() || !can_create_funnel(&funnel_name()),
                        if create_pending() { "Creating…" } else { "Create funnel" }
                    }
                }
            }
        }

        if let Some(target_id) = delete_target() {
            if let Some(target) = load_state()
                .response
                .and_then(|result| result.ok())
                .and_then(|items| items.into_iter().find(|item| item.id == target_id))
            {
                dialog {
                    id: "delete-funnel-dialog",
                    class: "funnel-dialog confirmation-dialog",
                    "aria-modal": "true",
                    "aria-labelledby": "delete-funnel-title",
                    "aria-describedby": "delete-funnel-description",
                    oncancel: move |event| match dialog_cancel_disposition(delete_pending()) {
                        DialogCancelDisposition::KeepOpen => event.prevent_default(),
                        DialogCancelDisposition::Dismiss => {
                            delete_target.set(None);
                            delete_error.set(None);
                        }
                    },
                    h2 { id: "delete-funnel-title", "Delete {target.name}?" }
                    p { id: "delete-funnel-description", "This removes the funnel and its saved revisions." }
                    if let Some(message) = delete_error() {
                        p { class: "error-message", role: "alert", "{message}" }
                    }
                    div { class: "dialog-actions",
                        button {
                            class: "secondary-button",
                            r#type: "button",
                            disabled: delete_pending(),
                            onclick: move |_| {
                                delete_target.set(None);
                                delete_error.set(None);
                            },
                            "Cancel"
                        }
                        button {
                            class: "primary-button danger-button",
                            r#type: "button",
                            disabled: delete_pending(),
                            onclick: move |_| {
                                if delete_pending() { return; }
                                delete_pending.set(true);
                                delete_error.set(None);
                                #[cfg(feature = "browser")]
                                let request_path = format!("/api/funnels/{}", target_id);
                                #[cfg(feature = "browser")]
                                let response_funnel_id = target_id.clone();
                                #[cfg(feature = "browser")]
                                let prepared_key = match prepare_idempotency_key(
                                    "DELETE",
                                    &request_path,
                                    None,
                                    delete_idempotency_key(),
                                ) {
                                    Ok(prepared_key) => prepared_key,
                                    Err(()) => {
                                        delete_error.set(Some("The browser could not prepare a retry-safe request.".to_owned()));
                                        delete_pending.set(false);
                                        return;
                                    }
                                };
                                #[cfg(feature = "browser")]
                                delete_idempotency_key.set(Some(prepared_key.clone()));
                                #[cfg(feature = "browser")]
                                spawn(async move {
                                    match api_request(
                                        "DELETE",
                                        &request_path,
                                        None,
                                        Some(prepared_key.clone()),
                                    ).await {
                                        Ok(response) if response.status == 200 => {
                                            let _ = clear_idempotency_storage(&prepared_key);
                                            let mut current_list = load_state;
                                            current_list.with_mut(|state| {
                                                state.delete_succeeded(&response_funnel_id);
                                            });
                                            let refreshed_list = load_state;
                                            spawn(async move { refresh_funnel_list(refreshed_list).await; });
                                            delete_target.set(None);
                                            delete_error.set(None);
                                            delete_idempotency_key.set(None);
                                            delete_idempotency_target.set(None);
                                        }
                                        Ok(response) => {
                                            delete_idempotency_key.set(Some(prepared_key.clone()));
                                            delete_error.set(Some(error_message(&response.body)));
                                        }
                                        Err(()) => delete_error.set(Some("The funnel could not be deleted. Try again.".to_owned())),
                                    }
                                    delete_pending.set(false);
                                });
                                #[cfg(not(feature = "browser"))]
                                delete_pending.set(false);
                            },
                            if delete_pending() { "Deleting…" } else { "Delete funnel" }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(feature = "browser")]
pub(super) async fn refresh_funnel_list(mut load_state: Signal<FunnelListLoad>) {
    let request_version = load_state.with_mut(FunnelListLoad::begin);
    let result = load_funnels().await;
    if load_state.try_peek().is_err() {
        return;
    }
    load_state.with_mut(|state| state.complete(request_version, result));
}

#[cfg(any(feature = "browser", test))]
async fn finish_create_response<Q, S, E, P, R>(
    response: Q,
    request_name: String,
    finish_success: S,
    finish_error: E,
    mut clear_pending: P,
    refresh: R,
) where
    Q: Future<Output = Result<BrowserResponse, ()>>,
    S: FnOnce(String),
    E: FnOnce(String),
    P: FnMut(),
    R: Future<Output = ()>,
{
    match response.await {
        Ok(response) if response.status == 201 => {
            finish_success(created_summary(&response.body, &request_name));
            clear_pending();
            refresh.await;
        }
        Ok(response) => {
            finish_error(error_message(&response.body));
            clear_pending();
        }
        Err(()) => {
            finish_error("The funnel could not be created. Try again.".to_owned());
            clear_pending();
        }
    }
}

fn can_create_funnel(name: &str) -> bool {
    let trimmed = name.trim();
    !trimmed.is_empty() && trimmed.len() <= MAX_FUNNEL_NAME_BYTES
}

#[component]
fn CreateFields(
    name: String,
    blueprint: FunnelBlueprint,
    language: PublicLanguage,
    pending: bool,
    on_name_change: EventHandler<String>,
    on_blueprint_change: EventHandler<FunnelBlueprint>,
    on_language_change: EventHandler<PublicLanguage>,
) -> Element {
    rsx! {
        label { "Funnel name"
            input {
                r#type: "text",
                name: "name",
                required: true,
                maxlength: "120",
                disabled: pending,
                value: "{name}",
                oninput: move |event| on_name_change.call(event.value()),
            }
        }
        label { class: "editor-setting-control",
            "Public language"
            select {
                name: "language",
                value: language.as_str(),
                disabled: pending,
                onchange: move |event| {
                    match event.value().as_str() {
                        "fr" => on_language_change.call(PublicLanguage::French),
                        "en" => on_language_change.call(PublicLanguage::English),
                        _ => {}
                    }
                },
                option { value: "en", "English" }
                option { value: "fr", "Français" }
            }
        }
        if !name.trim().is_empty() && name.trim().len() > MAX_FUNNEL_NAME_BYTES {
            p { class: "error-message", role: "alert", "{FUNNEL_NAME_LIMIT_MESSAGE}" }
        }
        fieldset { class: "blueprint-options",
            legend { "Choose a blueprint" }
            for item in FunnelBlueprint::ALL {
                label { class: "blueprint-option",
                    "data-blueprint": item.id(),
                    input {
                        r#type: "radio",
                        name: "blueprint",
                        value: item.id(),
                        checked: blueprint == item,
                        disabled: pending,
                        onchange: move |_| on_blueprint_change.call(item),
                    }
                    span { class: "blueprint-copy",
                        strong { "{item.label()}" }
                        span { "{item.description()}" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::core::VirtualDom;
    #[test]
    fn create_fields_are_disabled_while_a_request_is_pending() {
        let mut virtual_dom = VirtualDom::new(|| {
            rsx! {
                CreateFields {
                    name: "Offer".to_owned(),
                    blueprint: FunnelBlueprint::Tripwire,
                    language: PublicLanguage::English,
                    pending: true,
                    on_name_change: move |_| {},
                    on_blueprint_change: move |_| {},
                    on_language_change: move |_| {},
                }
            }
        });
        virtual_dom.rebuild_in_place();
        let html = dioxus::ssr::render(&virtual_dom);

        assert_eq!(html.matches("<input").count(), 6, "{html}");
        assert_eq!(html.matches("disabled").count(), 7, "{html}");
        assert!(html.contains("Page outline only; checkout and payment are not included."));
        assert!(html.contains("Working service inquiry form with an offer page and confirmation."));
    }

    #[test]
    fn create_name_limit_matches_the_command_utf8_byte_limit() {
        assert!(can_create_funnel(&"x".repeat(120)));
        assert!(!can_create_funnel(&"x".repeat(121)));
        assert!(can_create_funnel(&"😀".repeat(30)));
        assert!(!can_create_funnel(&"😀".repeat(31)));
    }

    #[test]
    fn create_fields_explain_when_multibyte_name_exceeds_command_byte_limit() {
        let mut virtual_dom = VirtualDom::new(|| {
            rsx! {
                CreateFields {
                    name: "😀".repeat(31),
                    blueprint: FunnelBlueprint::Tripwire,
                    language: PublicLanguage::English,
                    pending: false,
                    on_name_change: move |_| {},
                    on_blueprint_change: move |_| {},
                    on_language_change: move |_| {},
                }
            }
        });
        virtual_dom.rebuild_in_place();
        let html = dioxus::ssr::render(&virtual_dom);

        assert!(html.contains("120 UTF-8 bytes or fewer"), "{html}");
    }

    #[test]
    fn stale_funnel_responses_do_not_replace_the_newer_list_state() {
        let mut list_state = FunnelListLoad::default();
        let initial_load = list_state.begin();
        let create_refresh = list_state.begin();
        let created_funnel = FunnelCard {
            id: "created".to_owned(),
            name: "Created funnel".to_owned(),
            slug: "created-funnel".to_owned(),
            published_revision_id: None,
            created_at: "2026-10-14T00:00:00Z".to_owned(),
            updated_at: "2026-10-14T00:00:00Z".to_owned(),
        };

        list_state.complete(create_refresh, Ok(vec![created_funnel.clone()]));
        list_state.complete(initial_load, Ok(Vec::new()));
        list_state.complete(initial_load, Err(()));

        assert_eq!(list_state.response, Some(Ok(vec![created_funnel])));
    }

    #[test]
    fn a_list_response_started_before_delete_cannot_restore_the_deleted_funnel() {
        let mut list_state = FunnelListLoad::default();
        let pre_delete_refresh = list_state.begin();
        let deleted_funnel = FunnelCard {
            id: "deleted".to_owned(),
            name: "Deleted funnel".to_owned(),
            slug: "deleted-funnel".to_owned(),
            published_revision_id: None,
            created_at: "2026-10-14T00:00:00Z".to_owned(),
            updated_at: "2026-10-14T00:00:00Z".to_owned(),
        };
        let retained_funnel = FunnelCard {
            id: "retained".to_owned(),
            name: "Retained funnel".to_owned(),
            slug: "retained-funnel".to_owned(),
            published_revision_id: None,
            created_at: "2026-10-14T00:00:00Z".to_owned(),
            updated_at: "2026-10-14T00:00:00Z".to_owned(),
        };
        let stale_snapshot = vec![deleted_funnel.clone(), retained_funnel.clone()];

        list_state.response = Some(Ok(stale_snapshot.clone()));
        list_state.delete_succeeded(&deleted_funnel.id);
        list_state.complete(pre_delete_refresh, Ok(stale_snapshot));

        assert_eq!(list_state.response, Some(Ok(vec![retained_funnel])));
    }

    #[test]
    fn successful_create_closes_and_enables_form_before_unresolved_list_refresh() {
        use std::cell::Cell;
        use std::future::Future;
        use std::pin::Pin;
        use std::rc::Rc;
        use std::sync::Arc;
        use std::task::{Context, Poll, Wake, Waker};

        struct NoopWake;
        impl Wake for NoopWake {
            fn wake(self: Arc<Self>) {}
        }

        struct ManualRefresh(Rc<Cell<bool>>);
        impl Future for ManualRefresh {
            type Output = ();

            fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
                if self.0.get() {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            }
        }

        fn poll_once<F: Future>(mut future: Pin<&mut F>) -> Poll<F::Output> {
            let waker = Waker::from(Arc::new(NoopWake));
            let mut context = Context::from_waker(&waker);
            future.as_mut().poll(&mut context)
        }

        let dialog_open = Rc::new(Cell::new(true));
        let mutation_pending = Rc::new(Cell::new(true));
        let refresh_ready = Rc::new(Cell::new(false));
        let finished_dialog = Rc::clone(&dialog_open);
        let finished_pending = Rc::clone(&mutation_pending);
        let response = async {
            Ok(BrowserResponse {
                status: 201,
                body: "{}".to_owned(),
            })
        };
        let refresh = finish_create_response(
            response,
            "Offer".to_owned(),
            move |message| {
                assert_eq!(message, "Created Offer with starter sections.");
                finished_dialog.set(false);
            },
            |_| panic!("a successful creation must not take the error path"),
            move || finished_pending.set(false),
            ManualRefresh(Rc::clone(&refresh_ready)),
        );
        let mut refresh = std::pin::pin!(refresh);

        assert!(poll_once(refresh.as_mut()).is_pending());
        assert!(
            !dialog_open.get(),
            "success closes the create dialog immediately"
        );
        assert!(
            !mutation_pending.get(),
            "list refresh must not keep the create mutation pending"
        );

        dialog_open.set(true);
        mutation_pending.set(true);
        refresh_ready.set(true);
        assert!(poll_once(refresh.as_mut()).is_ready());
        assert!(dialog_open.get(), "a second create can open during refresh");
        assert!(
            mutation_pending.get(),
            "the first refresh completion must not clear the second create's pending state"
        );
    }

    #[test]
    fn unsuccessful_create_responses_clear_pending_without_refreshing() {
        use std::cell::Cell;
        use std::future::Future;
        use std::pin::Pin;
        use std::rc::Rc;
        use std::sync::Arc;
        use std::task::{Context, Poll, Wake, Waker};

        struct NoopWake;
        impl Wake for NoopWake {
            fn wake(self: Arc<Self>) {}
        }

        struct Refresh(Rc<Cell<bool>>);
        impl Future for Refresh {
            type Output = ();

            fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
                self.0.set(true);
                Poll::Ready(())
            }
        }

        let responses = [
            Ok(BrowserResponse {
                status: 400,
                body: "{}".to_owned(),
            }),
            Err(()),
        ];
        for response in responses {
            let mutation_pending = Rc::new(Cell::new(true));
            let error_reported = Rc::new(Cell::new(false));
            let refresh_polled = Rc::new(Cell::new(false));
            let finished_pending = Rc::clone(&mutation_pending);
            let reported_error = Rc::clone(&error_reported);
            let response = async move { response };
            let future = finish_create_response(
                response,
                "Offer".to_owned(),
                |_| panic!("an unsuccessful creation must not take the success path"),
                move |message| reported_error.set(!message.is_empty()),
                move || finished_pending.set(false),
                Refresh(Rc::clone(&refresh_polled)),
            );
            let mut future = std::pin::pin!(future);
            let waker = Waker::from(Arc::new(NoopWake));
            let mut context = Context::from_waker(&waker);

            assert!(future.as_mut().poll(&mut context).is_ready());
            assert!(error_reported.get());
            assert!(!mutation_pending.get());
            assert!(!refresh_polled.get());
        }
    }

    #[test]
    fn native_dialog_cancel_stays_open_until_pending_mutation_finishes() {
        assert_eq!(
            dialog_cancel_disposition(true),
            DialogCancelDisposition::KeepOpen
        );
        assert_eq!(
            dialog_cancel_disposition(false),
            DialogCancelDisposition::Dismiss
        );
    }
}
