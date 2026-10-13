#![forbid(unsafe_code)]

use axum::extract::Json;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::Serialize;
use tower_service::Service;
use worker::{event, Context, Env, HttpRequest};

use crate::errors::ApiError;

pub mod errors;
pub mod host;

pub use host::HostAllowlist;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

#[derive(Serialize)]
struct ProbeResponse {
    status: &'static str,
    d1: &'static str,
    r2: &'static str,
    kv: &'static str,
    correlation_id: String,
}

fn router() -> Router {
    Router::new()
        .route("/health", get(health))
        .fallback(route_not_found)
        .method_not_allowed_fallback(method_not_allowed)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

#[event(fetch)]
pub async fn fetch(
    mut request: HttpRequest,
    env: Env,
    _context: Context,
) -> worker::Result<Response<axum::body::Body>> {
    let request_path = request.uri().path().to_owned();
    let method = request.method().as_str().to_owned();
    let correlation_id = normalize_correlation_id(request.headers_mut());

    let response = if !request_host_allowed(&env, request.headers()) {
        let error = ApiError::from_code("authorization/host_not_allowed");
        log_api_error(&error, &correlation_id, "request host is not allowlisted");
        error_response(&error, correlation_id.clone())
    } else if request_path == "/__local/probe" && request.method() == Method::POST {
        local_probe(&env, request.headers(), correlation_id.clone()).await
    } else {
        let mut router = router();
        router.call(request).await?
    };

    log_request(
        &method,
        &request_path,
        response.status().as_u16(),
        &correlation_id,
    );

    Ok(response)
}

fn request_host_allowed(env: &Env, headers: &HeaderMap<HeaderValue>) -> bool {
    let configured_hosts = env
        .var("ALLOWED_HOSTS")
        .map(|value| value.to_string())
        .unwrap_or_default();
    let request_host = headers
        .get("host")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    HostAllowlist::parse(&configured_hosts).allows(request_host)
}

async fn route_not_found(headers: HeaderMap<HeaderValue>) -> Response<axum::body::Body> {
    let correlation_id = correlation_id(&headers);
    let error = ApiError::from_code("not_found/route");
    log_api_error(&error, &correlation_id, "request route was not found");
    error_response(&error, correlation_id)
}

async fn method_not_allowed(headers: HeaderMap<HeaderValue>) -> Response<axum::body::Body> {
    let correlation_id = correlation_id(&headers);
    let error = ApiError::from_code("validation/operation_not_supported");
    log_api_error(
        &error,
        &correlation_id,
        "request method is not supported for route",
    );
    error_response(&error, correlation_id)
}

async fn local_probe(
    env: &Env,
    headers: &HeaderMap<HeaderValue>,
    correlation_id: String,
) -> Response<axum::body::Body> {
    let is_development = env
        .var("ENVIRONMENT")
        .map(|value| value.to_string() == "development")
        .unwrap_or(false);
    let expected_token = env
        .var("LOCAL_PROBE_TOKEN")
        .ok()
        .map(|value| value.to_string());
    let supplied_token = headers
        .get("x-local-probe-token")
        .and_then(|value| value.to_str().ok());
    let has_valid_token = matches!(
        (expected_token.as_deref(), supplied_token),
        (Some(expected), Some(supplied)) if !expected.is_empty() && expected == supplied
    );
    if !is_development || !has_valid_token {
        let error = ApiError::from_code("not_found/route");
        log_api_error(
            &error,
            &correlation_id,
            "local probe route is not available",
        );
        return error_response(&error, correlation_id);
    }

    let result = async {
        let bindings = eez_storage::Bindings::from_env(env)?;
        bindings
            .verify_local_round_trip("pr002-local-roundtrip", "round-trip-ok")
            .await
    }
    .await;

    match result {
        Ok(()) => Json(ProbeResponse {
            status: "ok",
            d1: "ok",
            r2: "ok",
            kv: "ok",
            correlation_id,
        })
        .into_response(),
        Err(source) => {
            let error = ApiError::from_storage_error(&source);
            log_api_error(&error, &correlation_id, &format!("{source:?}"));
            error_response(&error, correlation_id)
        }
    }
}

fn correlation_id(headers: &HeaderMap<HeaderValue>) -> String {
    headers
        .get("x-correlation-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .unwrap_or_default()
}

fn normalize_correlation_id(headers: &mut HeaderMap<HeaderValue>) -> String {
    let mut correlation_id = headers
        .get("x-correlation-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let header_value = HeaderValue::from_bytes(correlation_id.as_bytes()).unwrap_or_else(|_| {
        correlation_id = "correlation-id-unavailable".to_owned();
        HeaderValue::from_static("correlation-id-unavailable")
    });
    headers.insert("x-correlation-id", header_value);
    correlation_id
}

fn error_response(error: &ApiError, correlation_id: String) -> Response<axum::body::Body> {
    let status = StatusCode::from_u16(error.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut response = Json(error.response(correlation_id)).into_response();
    *response.status_mut() = status;
    response
}

fn log_api_error(error: &ApiError, correlation_id: &str, detail: &str) {
    let record = serde_json::json!({
        "event": "api_error",
        "code": error.code(),
        "correlation_id": correlation_id,
        "detail": detail,
        "alert": error.requires_alert(),
    });
    if error.requires_alert() {
        worker::console_error!("ALERT {}", record);
    } else {
        worker::console_error!("{}", record);
    }
}

fn log_request(method: &str, path: &str, status: u16, correlation_id: &str) {
    worker::console_log!(
        "{}",
        serde_json::json!({
            "event": "request",
            "method": method,
            "path": path,
            "status": status,
            "correlation_id": correlation_id,
        })
    );
}

#[cfg(test)]
mod tests {
    use super::{correlation_id, normalize_correlation_id};
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn missing_correlation_id_is_generated_and_written_to_headers() {
        let mut headers = HeaderMap::new();

        let id = normalize_correlation_id(&mut headers);

        assert!(uuid::Uuid::parse_str(&id).is_ok());
        assert_eq!(correlation_id(&headers), id);
    }

    #[test]
    fn empty_correlation_id_is_replaced_and_written_to_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-correlation-id", HeaderValue::from_static(""));

        let id = normalize_correlation_id(&mut headers);

        assert!(uuid::Uuid::parse_str(&id).is_ok());
        assert_eq!(correlation_id(&headers), id);
    }

    #[test]
    fn overlength_correlation_id_is_replaced_and_written_to_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-correlation-id",
            HeaderValue::from_bytes(&[b'x'; 129]).unwrap(),
        );

        let id = normalize_correlation_id(&mut headers);

        assert!(uuid::Uuid::parse_str(&id).is_ok());
        assert_eq!(correlation_id(&headers), id);
    }

    #[test]
    fn nonempty_correlation_id_is_preserved_in_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-correlation-id", HeaderValue::from_static("request-123"));

        assert_eq!(normalize_correlation_id(&mut headers), "request-123");
        assert_eq!(correlation_id(&headers), "request-123");
    }
}
