use dioxus::core::VirtualDom;
use dioxus_fullstack_core::HydrationContext;
use serde_json::Value;

use crate::app::{WebApp, WebAppProps};

const INITIALIZE_DIOXUS_STREAMING: &str = "window.hydrate_queue=[];window.dx_hydrate=(id,data,debug_types,debug_locations)=>{const decoded=atob(data);const bytes=Uint8Array.from(decoded,(character)=>character.charCodeAt(0));if(window.hydration_callback)window.hydration_callback(id,bytes,debug_types,debug_locations);else window.hydrate_queue.push([id,bytes,debug_types,debug_locations])};";

pub fn render_document(path: &str) -> Option<String> {
    if !is_dashboard_path(path) {
        return None;
    }

    let mut virtual_dom = VirtualDom::new_with_props(
        WebApp,
        WebAppProps {
            initial_path: path.to_owned(),
        },
    );
    virtual_dom.rebuild_in_place();

    let rendered_app = dioxus::ssr::pre_render(&virtual_dom);
    let hydration = HydrationContext::default().serialized();
    let payload = hydration_payload(&hydration);
    let payload = serde_json::to_string(&payload)
        .ok()?
        .replace('<', "\\u003c");

    Some(format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><meta name=\"theme-color\" content=\"#f5f7fb\"><title>Eez Funnel</title><link rel=\"stylesheet\" href=\"/assets/dashboard.css\"><script>{INITIALIZE_DIOXUS_STREAMING}</script></head><body><div id=\"main\">{rendered_app}</div><script id=\"dioxus-hydration-data\" type=\"application/json\">{payload}</script><script type=\"module\" src=\"/assets/bootstrap.js\"></script></body></html>"
    ))
}

fn is_dashboard_path(path: &str) -> bool {
    match path {
        "/" | "/funnels" | "/media" => true,
        _ => {
            let Some(path) = path.strip_prefix("/funnels/") else {
                return false;
            };
            let segments = path.split('/').collect::<Vec<_>>();
            match segments.as_slice() {
                [funnel_id] => is_valid_funnel_id(funnel_id),
                [funnel_id, "leads"] => is_valid_funnel_id(funnel_id),
                [funnel_id, "leads", submission_id] => {
                    is_valid_funnel_id(funnel_id) && is_valid_submission_id(submission_id)
                }
                _ => false,
            }
        }
    }
}

fn is_valid_funnel_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn is_valid_submission_id(id: &str) -> bool {
    uuid::Uuid::parse_str(id)
        .is_ok_and(|submission_id| submission_id.hyphenated().to_string() == id)
}

fn hydration_payload(hydration: &dioxus_fullstack_core::SerializedHydrationData) -> Value {
    #[cfg(debug_assertions)]
    {
        serde_json::json!({
            "data": hydration.data,
            "debug_types": debug_values(&hydration.debug_types),
            "debug_locations": debug_values(&hydration.debug_locations),
        })
    }
    #[cfg(not(debug_assertions))]
    {
        serde_json::json!({ "data": hydration.data })
    }
}

#[cfg(debug_assertions)]
fn debug_values(serialized: &str) -> Value {
    match serde_json::from_str::<Vec<String>>(serialized) {
        Ok(values) => Value::Array(values.into_iter().map(Value::String).collect()),
        Err(_) => Value::Array(Vec::new()),
    }
}
