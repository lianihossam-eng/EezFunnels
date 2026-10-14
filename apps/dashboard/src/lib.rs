#![forbid(unsafe_code)]

mod app;
mod routes;

#[cfg(feature = "browser")]
mod browser;

#[cfg(feature = "ssr")]
mod ssr;

#[cfg(feature = "ssr")]
pub use ssr::render_document;

#[cfg(test)]
mod tests {
    use super::render_document;
    use serde_json::Value;

    const BOOTSTRAP: &str = include_str!("../assets/bootstrap.js");

    fn hydration_payload(html: &str) -> Value {
        let start = html
            .find("<script id=\"dioxus-hydration-data\" type=\"application/json\">")
            .expect("hydration payload element");
        let payload_start =
            start + "<script id=\"dioxus-hydration-data\" type=\"application/json\">".len();
        let payload_end = html[payload_start..]
            .find("</script>")
            .map(|offset| payload_start + offset)
            .expect("hydration payload closing tag");

        serde_json::from_str(&html[payload_start..payload_end]).expect("valid hydration JSON")
    }

    fn assert_valid_hydration_data(html: &str) {
        let payload = hydration_payload(html);
        let data = payload
            .get("data")
            .and_then(Value::as_str)
            .expect("base64 hydration data");
        let bytes = data.as_bytes();
        assert!(!bytes.is_empty());
        assert_eq!(bytes.len() % 4, 0);
        let padding = bytes.iter().rev().take_while(|byte| **byte == b'=').count();
        assert!(padding <= 2);
        let unpadded_length = bytes.len().saturating_sub(padding);
        assert!(bytes[..unpadded_length]
            .iter()
            .all(|byte| { byte.is_ascii_alphanumeric() || *byte == b'+' || *byte == b'/' }));
        assert!(bytes[unpadded_length..].iter().all(|byte| *byte == b'='));
        assert!(BOOTSTRAP.contains("window.initial_dioxus_hydration_data = payload.data"));

        #[cfg(debug_assertions)]
        {
            assert!(payload.get("debug_types").is_some_and(Value::is_array));
            assert!(payload.get("debug_locations").is_some_and(Value::is_array));
            assert!(BOOTSTRAP
                .contains("window.initial_dioxus_hydration_debug_types = payload.debug_types"));
            assert!(BOOTSTRAP.contains(
                "window.initial_dioxus_hydration_debug_locations = payload.debug_locations"
            ));
        }
    }

    #[test]
    fn dashboard_root_renders_a_hydratable_shell() {
        let html = render_document("/").expect("root dashboard route");

        assert!(html.contains("id=\"main\""));
        assert!(html.contains("/assets/dashboard.css"));
        assert!(html.contains("/assets/bootstrap.js"));
        assert!(html.contains("Dashboard"));
        assert!(html.contains("Funnels"));
        assert_valid_hydration_data(&html);
    }

    #[test]
    fn hydration_mount_wraps_the_server_rendered_app_root() {
        let html = render_document("/").expect("root dashboard route");

        assert!(html.contains("<body><div id=\"main\"><div class=\"app-shell\""));
        assert_eq!(html.matches("id=\"main\"").count(), 1);
    }

    #[test]
    fn funnels_route_renders_its_page_inside_the_shared_shell() {
        let html = render_document("/funnels").expect("funnel list route");

        assert!(html.contains("id=\"main\""));
        assert!(html.contains("Your funnels"));
        assert!(html.contains("Dashboard"));
    }

    #[test]
    fn funnel_detail_route_renders_the_validated_identifier() {
        let html = render_document("/funnels/demo-1").expect("funnel detail route");

        assert!(html.contains("Funnel overview"));
        assert!(html.contains("demo-1"));
        assert!(html.contains("id=\"main\""));
    }

    #[test]
    fn funnel_detail_route_rejects_invalid_identifiers() {
        for path in [
            "/funnels/",
            "/funnels/has.dot",
            "/funnels/has space",
            "/funnels/é",
            "/funnels/too-long-identifier-0123456789-0123456789-0123456789-0123456789-0",
        ] {
            assert!(render_document(path).is_none(), "accepted {path:?}");
        }
    }

    #[test]
    fn dashboard_renderer_rejects_unowned_and_extra_paths() {
        for path in [
            "/api/funnels",
            "/health",
            "/f/demo",
            "/mcp",
            "/__local/probe",
            "/funnels/demo/extra",
            "/unknown",
        ] {
            assert!(render_document(path).is_none(), "accepted {path:?}");
        }
    }
}
