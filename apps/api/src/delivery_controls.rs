use axum::body::to_bytes;
use axum::http::{Method, StatusCode};
use axum::response::Response;
use eez_commands::public_delivery_controls::{
    self, FunnelWebhookDeliverySnapshot, SaveWebhookFormDelivery, WebhookDestinationSnapshot,
};
use serde::Deserialize;
use serde_json::json;
use worker::{Env, HttpRequest};

use crate::errors::ApiError;
use crate::json::validate_unique_json;

const MAX_DELIVERY_CONTROL_REQUEST_BYTES: usize = 16 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DestinationUpdateBody {
    expected_version_id: Option<String>,
    expected_enabled: bool,
    destination_url: String,
    enabled: bool,
}

pub(super) async fn installation_response(
    env: &Env,
    request: HttpRequest,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    match *request.method() {
        Method::GET => read_destination(env, correlation_id).await,
        Method::PUT => save_destination(env, request, correlation_id).await,
        _ => crate::private_lifecycle_response(crate::method_not_allowed_for(
            correlation_id,
            "GET, PUT",
        )),
    }
}

pub(super) async fn funnel_response(
    env: &Env,
    request: HttpRequest,
    funnel_id: &str,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    if !crate::valid_funnel_route_id(funnel_id) {
        return private_error_response("not_found/funnel", correlation_id);
    }
    match *request.method() {
        Method::GET => read_funnel(env, funnel_id, correlation_id).await,
        Method::PUT => save_funnel(env, request, funnel_id, correlation_id).await,
        _ => crate::private_lifecycle_response(crate::method_not_allowed_for(
            correlation_id,
            "GET, PUT",
        )),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SubmissionDeliveryAction {
    Retry,
    Cancel,
}

pub(super) fn submission_action_from_path(path: &str) -> Option<(&str, SubmissionDeliveryAction)> {
    let route = path.strip_prefix("/api/submissions/")?;
    let (submission_id, action) = route.split_once("/delivery/")?;
    let action = match action {
        "retry" => SubmissionDeliveryAction::Retry,
        "cancel" => SubmissionDeliveryAction::Cancel,
        _ => return None,
    };
    let id = uuid::Uuid::parse_str(submission_id).ok()?;
    (id.to_string() == submission_id).then_some((submission_id, action))
}

pub(super) async fn submission_action_response(
    env: &Env,
    request: HttpRequest,
    submission_id: &str,
    action: SubmissionDeliveryAction,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    let method = match action {
        SubmissionDeliveryAction::Retry | SubmissionDeliveryAction::Cancel => "POST",
    };
    if request.method() != Method::POST {
        return crate::private_lifecycle_response(crate::method_not_allowed_for(
            correlation_id,
            method,
        ));
    }
    if !crate::origin::matches_request_host(request.headers(), request.uri().scheme_str()) {
        return private_error_response("request/invalid_origin", correlation_id);
    }
    let idempotency_key = match crate::request_idempotency_key(request.headers()) {
        Ok(key) => key,
        Err(code) => return private_error_response(code, correlation_id),
    };
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return private_error_response("external/unavailable", correlation_id),
    };
    let result = match action {
        SubmissionDeliveryAction::Retry => {
            public_delivery_controls::retry_delivery(
                &bindings.delivery_control_repository(),
                submission_id,
                &idempotency_key,
            )
            .await
        }
        SubmissionDeliveryAction::Cancel => {
            public_delivery_controls::cancel_delivery(
                &bindings.delivery_control_repository(),
                submission_id,
                &idempotency_key,
            )
            .await
        }
    };
    match result {
        Ok(()) => private_lifecycle(crate::json_success_response(
            json!({"submission_id": submission_id, "accepted": true}),
            StatusCode::OK,
            correlation_id,
            None,
        )),
        Err(error) => private_error_response(error.code(), correlation_id),
    }
}

async fn read_destination(env: &Env, correlation_id: &str) -> Response<axum::body::Body> {
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return private_error_response("external/unavailable", correlation_id),
    };
    match public_delivery_controls::read_destination(&bindings.delivery_control_repository()).await
    {
        Ok(snapshot) => destination_success(snapshot, correlation_id),
        Err(error) => private_error_response(error.code(), correlation_id),
    }
}

async fn save_destination(
    env: &Env,
    request: HttpRequest,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    if !crate::origin::matches_request_host(request.headers(), request.uri().scheme_str()) {
        return private_error_response("request/invalid_origin", correlation_id);
    }
    if !crate::is_json_content_type(request.headers()) {
        return private_error_response("validation/invalid_json", correlation_id);
    }
    let idempotency_key = match crate::request_idempotency_key(request.headers()) {
        Ok(key) => key,
        Err(code) => return private_error_response(code, correlation_id),
    };
    let body = match request_json_body(request).await {
        Ok(body) => body,
        Err(code) => return private_error_response(code, correlation_id),
    };
    let body = match parse_unique_json::<DestinationUpdateBody>(&body) {
        Ok(body) => body,
        Err(()) => return private_error_response("validation/invalid_json", correlation_id),
    };
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return private_error_response("external/unavailable", correlation_id),
    };
    match public_delivery_controls::save_destination(
        &bindings.delivery_control_repository(),
        &idempotency_key,
        body.expected_version_id.as_deref(),
        body.expected_enabled,
        &body.destination_url,
        body.enabled,
    )
    .await
    {
        Ok(snapshot) => destination_success(snapshot, correlation_id),
        Err(error) => private_error_response(error.code(), correlation_id),
    }
}

async fn read_funnel(
    env: &Env,
    funnel_id: &str,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return private_error_response("external/unavailable", correlation_id),
    };
    match public_delivery_controls::read_funnel_delivery(
        &bindings.repository(),
        &bindings.delivery_control_repository(),
        funnel_id,
    )
    .await
    {
        Ok(snapshot) => funnel_success(snapshot, correlation_id),
        Err(error) => private_error_response(error.code(), correlation_id),
    }
}

async fn save_funnel(
    env: &Env,
    request: HttpRequest,
    funnel_id: &str,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    if !crate::origin::matches_request_host(request.headers(), request.uri().scheme_str()) {
        return private_error_response("request/invalid_origin", correlation_id);
    }
    if !crate::is_json_content_type(request.headers()) {
        return private_error_response("validation/invalid_json", correlation_id);
    }
    let idempotency_key = match crate::request_idempotency_key(request.headers()) {
        Ok(key) => key,
        Err(code) => return private_error_response(code, correlation_id),
    };
    let body = match request_json_body(request).await {
        Ok(body) => body,
        Err(code) => return private_error_response(code, correlation_id),
    };
    let body = match parse_unique_json::<SaveWebhookFormDelivery>(&body) {
        Ok(body) => body,
        Err(()) => return private_error_response("validation/invalid_json", correlation_id),
    };
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return private_error_response("external/unavailable", correlation_id),
    };
    match public_delivery_controls::save_form_delivery(
        &bindings.repository(),
        &bindings.delivery_control_repository(),
        funnel_id,
        &idempotency_key,
        body,
    )
    .await
    {
        Ok(snapshot) => funnel_success(snapshot, correlation_id),
        Err(error) => private_error_response(error.code(), correlation_id),
    }
}

async fn request_json_body(request: HttpRequest) -> Result<String, &'static str> {
    let bytes = to_bytes(
        axum::body::Body::new(request.into_body()),
        MAX_DELIVERY_CONTROL_REQUEST_BYTES,
    )
    .await
    .map_err(|_| "validation/invalid_json")?;
    let body = std::str::from_utf8(&bytes).map_err(|_| "validation/invalid_json")?;
    validate_unique_json(body).map_err(|_| "validation/invalid_json")?;
    Ok(body.to_owned())
}

fn parse_unique_json<T: for<'de> Deserialize<'de>>(body: &str) -> Result<T, ()> {
    validate_unique_json(body).map_err(|_| ())?;
    serde_json::from_str(body).map_err(|_| ())
}

fn destination_success(
    snapshot: WebhookDestinationSnapshot,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    match serde_json::to_value(snapshot) {
        Ok(value) => private_lifecycle(crate::json_success_response(
            value,
            StatusCode::OK,
            correlation_id,
            None,
        )),
        Err(_) => private_error_response("internal/unexpected", correlation_id),
    }
}

fn funnel_success(
    snapshot: FunnelWebhookDeliverySnapshot,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    match serde_json::to_value(snapshot) {
        Ok(value) => private_lifecycle(crate::json_success_response(
            value,
            StatusCode::OK,
            correlation_id,
            None,
        )),
        Err(_) => private_error_response("internal/unexpected", correlation_id),
    }
}

fn private_error_response(code: &str, correlation_id: &str) -> Response<axum::body::Body> {
    let error = ApiError::from_code(code);
    crate::log_api_error(&error, correlation_id, error.code());
    private_lifecycle(crate::error_response(&error, correlation_id.to_owned()))
}

fn private_lifecycle(response: Response<axum::body::Body>) -> Response<axum::body::Body> {
    crate::private_lifecycle_response(response)
}

#[cfg(test)]
mod tests {
    use super::{
        parse_unique_json, submission_action_from_path, DestinationUpdateBody,
        SaveWebhookFormDelivery, SubmissionDeliveryAction,
    };

    #[test]
    fn delivery_action_paths_require_a_canonical_id_and_exact_action() {
        let id = "11111111-1111-4111-8111-111111111111";
        assert_eq!(
            submission_action_from_path(&format!("/api/submissions/{id}/delivery/retry")),
            Some((id, SubmissionDeliveryAction::Retry))
        );
        assert_eq!(
            submission_action_from_path(&format!("/api/submissions/{id}/delivery/cancel")),
            Some((id, SubmissionDeliveryAction::Cancel))
        );
        for path in [
            "/api/submissions/not-a-uuid/delivery/retry",
            "/api/submissions/11111111-1111-4111-8111-111111111111/delivery/other",
            "/api/submissions/11111111-1111-4111-8111-111111111111/delivery/retry/extra",
        ] {
            assert_eq!(submission_action_from_path(path), None, "{path}");
        }
        for (case_number, input) in crate::json::tests::deterministic_json_fuzz_inputs()
            .iter()
            .enumerate()
        {
            let path = format!("/api/submissions/{input}/delivery/retry");
            let result = std::panic::catch_unwind(|| submission_action_from_path(&path));
            assert!(
                result.is_ok(),
                "action path parser panicked for case {case_number}"
            );
        }
    }

    #[test]
    fn delivery_control_bodies_are_strict_and_preserve_optional_cas_versions() {
        let destination = parse_unique_json::<DestinationUpdateBody>(
            r#"{"expected_version_id":null,"expected_enabled":false,"destination_url":"https://hooks.example.dev/events","enabled":false}"#,
        )
        .expect("valid destination request");
        assert_eq!(destination.expected_version_id, None);
        assert!(!destination.expected_enabled);
        assert!(!destination.enabled);

        let mapping = parse_unique_json::<SaveWebhookFormDelivery>(
            r#"{"form_id":"form-contact","expected_published_revision_id":"22222222-2222-4222-8222-222222222222","destination_version_id":"11111111-1111-4111-8111-111111111111","expected_mapping_version_id":null,"enabled":true,"selected_field_ids":["field-email"]}"#,
        )
        .expect("valid form mapping request");
        assert_eq!(mapping.form_id, "form-contact");
        assert_eq!(
            mapping.expected_published_revision_id,
            "22222222-2222-4222-8222-222222222222"
        );
        assert_eq!(mapping.selected_field_ids, ["field-email"]);
        assert!(mapping.enabled);

        for input in [
            r#"{"expected_version_id":null,"expected_version_id":null,"expected_enabled":false,"destination_url":"https://hooks.example.dev/events","enabled":false}"#,
            r#"{"expected_version_id":null,"expected_enabled":false,"destination_url":"https://hooks.example.dev/events","enabled":false,"secret":"leak"}"#,
        ] {
            assert!(parse_unique_json::<DestinationUpdateBody>(input).is_err());
        }
        assert!(parse_unique_json::<SaveWebhookFormDelivery>(
            r#"{"form_id":"form-contact","destination_version_id":"11111111-1111-4111-8111-111111111111","enabled":true,"selected_field_ids":["field-email"],"secret":"leak"}"#,
        )
        .is_err());
    }

    #[test]
    fn delivery_control_json_parsers_do_not_panic_on_external_inputs() {
        use std::panic::catch_unwind;

        let inputs = crate::json::tests::deterministic_json_fuzz_inputs();
        for (case_number, input) in inputs.iter().enumerate() {
            assert!(
                catch_unwind(|| parse_unique_json::<DestinationUpdateBody>(input)).is_ok(),
                "destination parser panicked for case {case_number}"
            );
            assert!(
                catch_unwind(|| parse_unique_json::<SaveWebhookFormDelivery>(input)).is_ok(),
                "mapping parser panicked for case {case_number}"
            );
        }
    }
}
