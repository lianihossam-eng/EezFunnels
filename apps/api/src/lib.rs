#![forbid(unsafe_code)]

use axum::body::to_bytes;
use axum::extract::Json;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use eez_commands::{command_ids, command_registry, Session};
use serde::Serialize;
use serde_json::Value;
use tower_service::Service;
use worker::{event, Context, Env, HttpRequest};

use crate::errors::ApiError;
use crate::json::{validate_unique_json, JsonValidationError};

pub mod errors;
pub mod host;
mod json;
mod mcp;

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

#[derive(Serialize)]
struct ProductProbeResponse {
    status: &'static str,
    funnels: &'static str,
    revisions: &'static str,
    themes: &'static str,
    publication: &'static str,
    unsupported_schema_version: &'static str,
    cross_funnel_published_revision_insert_accepted: bool,
    null_published_revision_insert_accepted: bool,
    unsupported_schema_publish_accepted: bool,
    unsupported_schema_published_revision_id: Option<String>,
    current_revision_noop_accepted: bool,
    publication_baseline_cas_successes: usize,
    distinct_candidate_cas_successes: usize,
    publication_replay_interleavings_safe: bool,
    publication_race_funnel_id: &'static str,
    publication_race_revision_ids: Vec<String>,
    correlation_id: String,
}

#[derive(Serialize)]
struct PublicationRaceResponse {
    published: bool,
    correlation_id: String,
}

#[derive(Serialize)]
struct PublicationStatusResponse {
    published_revision_id: Option<String>,
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
    } else if request_path == "/mcp" {
        mcp::handle(&env, request, &correlation_id).await
    } else if request_path == "/api/funnels" {
        funnel_collection_response(&env, request, correlation_id.clone()).await
    } else if let Some(funnel_id) = funnel_subroute(&request_path, "/publish") {
        funnel_publication_mutation_response(
            &env,
            request,
            funnel_id,
            command_ids::FUNNEL_PUBLISH,
            correlation_id.clone(),
        )
        .await
    } else if let Some(funnel_id) = funnel_subroute(&request_path, "/unpublish") {
        funnel_publication_mutation_response(
            &env,
            request,
            funnel_id,
            command_ids::FUNNEL_UNPUBLISH,
            correlation_id.clone(),
        )
        .await
    } else if let Some(funnel_id) = funnel_subroute(&request_path, "/preview") {
        funnel_editor_preview_response(&env, request, funnel_id, correlation_id.clone()).await
    } else if let Some(funnel_id) = funnel_subroute(&request_path, "/revisions") {
        funnel_editor_save_response(&env, request, funnel_id, correlation_id.clone()).await
    } else if let Some(slug) = public_slug_from_path(&request_path) {
        public_funnel_response(&env, request, slug, correlation_id.clone()).await
    } else if let Some(funnel_id) = request_path.strip_prefix("/api/funnels/") {
        funnel_item_response(&env, request, funnel_id, correlation_id.clone()).await
    } else if request_path == "/__local/probe" && request.method() == Method::POST {
        local_probe(&env, request.headers(), correlation_id.clone()).await
    } else if request_path == "/__local/product-probe" && request.method() == Method::POST {
        local_product_probe(&env, request.headers(), correlation_id.clone()).await
    } else if request_path == "/__local/publication-race" && request.method() == Method::POST {
        local_publication_race(
            &env,
            request.headers(),
            request.uri().query(),
            correlation_id.clone(),
        )
        .await
    } else if request_path == "/__local/publication-race" && request.method() == Method::GET {
        local_publication_status(&env, request.headers(), correlation_id.clone()).await
    } else if request_path == "/__local/publication-race" && request.method() == Method::DELETE {
        local_publication_race_cleanup(&env, request.headers(), correlation_id.clone()).await
    } else if request_path.starts_with("/assets/")
        && (request.method() == Method::GET || request.method() == Method::HEAD)
    {
        asset_response(&env, request).await?
    } else if request.method() == Method::GET {
        match eez_dashboard_presentation::render_document(&request_path) {
            Some(document) => dashboard_response(document),
            None => {
                let mut router = router();
                router.call(request).await?
            }
        }
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

async fn asset_response(
    env: &Env,
    request: HttpRequest,
) -> worker::Result<Response<axum::body::Body>> {
    let response = env.assets("ASSETS")?.fetch_request(request).await?;
    let (parts, body) = response.into_parts();
    Ok(Response::from_parts(parts, axum::body::Body::new(body)))
}

fn dashboard_response(document: String) -> Response<axum::body::Body> {
    let mut response = Response::new(axum::body::Body::from(document));
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
}

fn request_host_allowed(env: &Env, headers: &HeaderMap<HeaderValue>) -> bool {
    let configured_hosts = env
        .var("ALLOWED_HOSTS")
        .map(|value| value.to_string())
        .unwrap_or_default();
    HostAllowlist::parse(&configured_hosts).allows_headers(headers)
}

const MAX_CREATE_REQUEST_BYTES: usize = 16 * 1024;
const MAX_EDITOR_REQUEST_BYTES: usize = 16 * 1_024;

fn public_slug_from_path(path: &str) -> Option<&str> {
    let slug = path.strip_prefix("/f/")?;
    (!slug.is_empty() && !slug.contains('/')).then_some(slug)
}

fn valid_public_slug(slug: &str) -> bool {
    (1..=128).contains(&slug.len())
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !slug.starts_with('-')
        && !slug.ends_with('-')
}

async fn funnel_publication_mutation_response(
    env: &Env,
    request: HttpRequest,
    funnel_id: &str,
    command_id: &'static str,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if request.method() != Method::POST {
        return method_not_allowed_for(&correlation_id, "POST");
    }
    let valid_route_id = valid_funnel_route_id(funnel_id);
    let idempotency_key = match request_idempotency_key(request.headers()) {
        Err(_) if !valid_route_id => return funnel_route_not_found_response(&correlation_id),
        Ok(key) => key,
        Err(code) => {
            let error = ApiError::from_code(code);
            log_api_error(&error, &correlation_id, error.code());
            return error_response(&error, correlation_id);
        }
    };
    let body = match to_bytes(
        axum::body::Body::new(request.into_body()),
        MAX_EDITOR_REQUEST_BYTES,
    )
    .await
    {
        Ok(body) => body,
        Err(_) => {
            return unreadable_mutation_payload_response(
                env,
                command_id,
                &idempotency_key,
                &correlation_id,
                "validation/invalid_json",
                valid_route_id,
            )
            .await
        }
    };
    let body = match std::str::from_utf8(&body) {
        Ok(body) => body,
        Err(_) => {
            return unreadable_mutation_payload_response(
                env,
                command_id,
                &idempotency_key,
                &correlation_id,
                "validation/invalid_json",
                valid_route_id,
            )
            .await
        }
    };
    let parameters = match publication_command_parameters(funnel_id, command_id, body) {
        Ok(parameters) => parameters,
        Err(
            EditorCommandParametersError::DuplicateObjectKey
            | EditorCommandParametersError::InvalidJson,
        ) => {
            return unreadable_mutation_payload_response(
                env,
                command_id,
                &idempotency_key,
                &correlation_id,
                "validation/invalid_json",
                valid_route_id,
            )
            .await
        }
    };
    match execute_funnel_command(env, command_id, &parameters, Some(&idempotency_key)).await {
        Ok(result) if valid_route_id => {
            json_success_response(result, StatusCode::OK, &correlation_id, None)
        }
        Ok(_) => funnel_route_not_found_response(&correlation_id),
        Err(error)
            if !valid_route_id
                && matches!(
                    error.code(),
                    "validation/invalid_json" | "validation/missing_field"
                ) =>
        {
            funnel_route_not_found_response(&correlation_id)
        }
        Err(error) => funnel_command_error_response(error, &correlation_id),
    }
}

async fn unreadable_mutation_payload_response(
    env: &Env,
    command_id: &str,
    idempotency_key: &str,
    correlation_id: &str,
    fresh_error_code: &'static str,
    valid_route_id: bool,
) -> Response<axum::body::Body> {
    match execute_funnel_command(env, command_id, "null", Some(idempotency_key)).await {
        Err(error)
            if !valid_route_id
                && matches!(
                    error.code(),
                    "validation/invalid_json" | "validation/missing_field"
                ) =>
        {
            funnel_route_not_found_response(correlation_id)
        }
        Err(error) => funnel_command_error_response(error, correlation_id),
        Ok(_) => editor_payload_error_response(correlation_id, fresh_error_code),
    }
}

fn publication_command_parameters(
    funnel_id: &str,
    command_id: &str,
    body: &str,
) -> Result<String, EditorCommandParametersError> {
    validate_unique_json(body).map_err(EditorCommandParametersError::from)?;
    let mut value: Value =
        serde_json::from_str(body).map_err(|_| EditorCommandParametersError::InvalidJson)?;
    let object = value
        .as_object_mut()
        .ok_or(EditorCommandParametersError::InvalidJson)?;
    let allowed_fields: &[&str] = match command_id {
        command_ids::FUNNEL_PUBLISH => &["revision_id", "expected_published_revision_id"],
        command_ids::FUNNEL_UNPUBLISH => &["expected_published_revision_id"],
        _ => return Err(EditorCommandParametersError::InvalidJson),
    };
    if object
        .keys()
        .any(|key| !allowed_fields.contains(&key.as_str()))
    {
        return Err(EditorCommandParametersError::InvalidJson);
    }
    object.insert("funnel_id".to_owned(), Value::String(funnel_id.to_owned()));
    serde_json::to_string(&value).map_err(|_| EditorCommandParametersError::InvalidJson)
}

async fn public_funnel_response(
    env: &Env,
    request: HttpRequest,
    slug: &str,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if request.method() != Method::GET {
        return method_not_allowed_for(&correlation_id, "GET");
    }
    if !valid_public_slug(slug) {
        return funnel_slug_not_found_response(&correlation_id);
    }
    let parameters = serde_json::json!({ "slug": slug }).to_string();
    let result =
        match execute_funnel_command(env, command_ids::FUNNEL_PUBLIC_READ, &parameters, None).await
        {
            Ok(result) => result,
            Err(error) => return funnel_command_error_response(error, &correlation_id),
        };
    let (Some(revision_id), Some(html)) = (
        result.get("revision_id").and_then(Value::as_str),
        result.get("html").and_then(Value::as_str),
    ) else {
        let error = ApiError::from_code("internal/unexpected");
        log_api_error(&error, &correlation_id, error.code());
        return error_response(&error, correlation_id);
    };
    let etag = format!("\"{revision_id}\"");
    let etag_header = match HeaderValue::from_str(&etag) {
        Ok(etag_header) => etag_header,
        Err(_) => {
            let error = ApiError::from_code("internal/unexpected");
            log_api_error(&error, &correlation_id, error.code());
            return error_response(&error, correlation_id);
        }
    };
    let not_modified = request_if_none_match(request.headers(), &etag);
    let mut response = Response::new(axum::body::Body::empty());
    response.headers_mut().insert("etag", etag_header);
    response.headers_mut().insert(
        "cache-control",
        HeaderValue::from_static("public, max-age=0, must-revalidate"),
    );
    if let Ok(value) = HeaderValue::from_str(&correlation_id) {
        response.headers_mut().insert("x-correlation-id", value);
    }
    if not_modified {
        *response.status_mut() = StatusCode::NOT_MODIFIED;
        return response;
    }
    *response.body_mut() = axum::body::Body::from(html.to_owned());
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
}

fn request_if_none_match(headers: &HeaderMap<HeaderValue>, etag: &str) -> bool {
    headers
        .get_all(axum::http::header::IF_NONE_MATCH)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .any(|candidate| {
            candidate == "*" || candidate.strip_prefix("W/").unwrap_or(candidate) == etag
        })
}

fn funnel_slug_not_found_response(correlation_id: &str) -> Response<axum::body::Body> {
    let error = ApiError::from_code("not_found/slug");
    log_api_error(&error, correlation_id, error.code());
    error_response(&error, correlation_id.to_owned())
}

async fn funnel_collection_response(
    env: &Env,
    request: HttpRequest,
    correlation_id: String,
) -> Response<axum::body::Body> {
    match *request.method() {
        Method::GET => {
            match execute_funnel_command(env, command_ids::FUNNEL_LIST, "{}", None).await {
                Ok(result) => json_success_response(result, StatusCode::OK, &correlation_id, None),
                Err(error) => funnel_command_error_response(error, &correlation_id),
            }
        }
        Method::POST => {
            let idempotency_key = match request_idempotency_key(request.headers()) {
                Ok(key) => key,
                Err(code) => {
                    let error = ApiError::from_code(code);
                    log_api_error(&error, &correlation_id, error.code());
                    return error_response(&error, correlation_id);
                }
            };
            let body = match to_bytes(
                axum::body::Body::new(request.into_body()),
                MAX_CREATE_REQUEST_BYTES,
            )
            .await
            {
                Ok(body) => body,
                Err(_) => {
                    return unreadable_create_payload_response(
                        env,
                        &idempotency_key,
                        &correlation_id,
                    )
                    .await;
                }
            };
            let parameters = match std::str::from_utf8(&body) {
                Ok(parameters) => parameters,
                Err(_) => {
                    return unreadable_create_payload_response(
                        env,
                        &idempotency_key,
                        &correlation_id,
                    )
                    .await;
                }
            };
            match execute_funnel_command(
                env,
                command_ids::FUNNEL_CREATE,
                parameters,
                Some(&idempotency_key),
            )
            .await
            {
                Ok(result) => {
                    let location = result
                        .get("funnel")
                        .and_then(|funnel| funnel.get("id"))
                        .and_then(Value::as_str)
                        .map(|id| format!("/funnels/{id}"));
                    json_success_response(result, StatusCode::CREATED, &correlation_id, location)
                }
                Err(error) => funnel_command_error_response(error, &correlation_id),
            }
        }
        _ => method_not_allowed_for(&correlation_id, "GET,POST"),
    }
}

async fn unreadable_create_payload_response(
    env: &Env,
    idempotency_key: &str,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    match execute_funnel_command(
        env,
        command_ids::FUNNEL_CREATE,
        "null",
        Some(idempotency_key),
    )
    .await
    {
        Err(error) => funnel_command_error_response(error, correlation_id),
        Ok(_) => funnel_command_error_response(
            ApiError::from_code("validation/invalid_json"),
            correlation_id,
        ),
    }
}

async fn funnel_editor_preview_response(
    env: &Env,
    request: HttpRequest,
    funnel_id: &str,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if request.method() != Method::POST {
        return method_not_allowed_for(&correlation_id, "POST");
    }
    if !valid_funnel_route_id(funnel_id) {
        return funnel_route_not_found_response(&correlation_id);
    }
    let body = match to_bytes(
        axum::body::Body::new(request.into_body()),
        MAX_EDITOR_REQUEST_BYTES,
    )
    .await
    {
        Ok(body) => body,
        Err(_) => return editor_payload_error_response(&correlation_id, "validation/invalid_json"),
    };
    let body = match std::str::from_utf8(&body) {
        Ok(body) => body,
        Err(_) => return editor_payload_error_response(&correlation_id, "validation/invalid_json"),
    };
    let parameters = match editor_command_parameters(funnel_id, body) {
        Ok(parameters) => parameters,
        Err(_) => return editor_payload_error_response(&correlation_id, "validation/invalid_json"),
    };
    match execute_funnel_command(env, command_ids::FUNNEL_EDITOR_PREVIEW, &parameters, None).await {
        Ok(result) => {
            let mut response = json_success_response(result, StatusCode::OK, &correlation_id, None);
            response
                .headers_mut()
                .insert("cache-control", HeaderValue::from_static("no-store"));
            response
        }
        Err(error) => funnel_command_error_response(error, &correlation_id),
    }
}

async fn funnel_editor_save_response(
    env: &Env,
    request: HttpRequest,
    funnel_id: &str,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if request.method() != Method::PUT {
        return method_not_allowed_for(&correlation_id, "PUT");
    }
    if !valid_funnel_route_id(funnel_id) {
        return funnel_route_not_found_response(&correlation_id);
    }
    let idempotency_key = match request_idempotency_key(request.headers()) {
        Ok(key) => key,
        Err(code) => {
            let error = ApiError::from_code(code);
            log_api_error(&error, &correlation_id, error.code());
            return error_response(&error, correlation_id);
        }
    };
    let body = match to_bytes(
        axum::body::Body::new(request.into_body()),
        MAX_EDITOR_REQUEST_BYTES,
    )
    .await
    {
        Ok(body) => body,
        Err(_) => {
            return unreadable_editor_payload_response(env, &idempotency_key, &correlation_id).await
        }
    };
    let body = match std::str::from_utf8(&body) {
        Ok(body) => body,
        Err(_) => {
            return unreadable_editor_payload_response(env, &idempotency_key, &correlation_id).await
        }
    };
    let parameters = match editor_command_parameters(funnel_id, body) {
        Ok(parameters) => parameters,
        Err(EditorCommandParametersError::DuplicateObjectKey) => {
            return editor_payload_error_response(&correlation_id, "validation/invalid_json")
        }
        Err(EditorCommandParametersError::InvalidJson) => {
            return execute_unreadable_editor_payload(
                env,
                &idempotency_key,
                "validation/invalid_json",
                &correlation_id,
            )
            .await
        }
    };
    match execute_funnel_command(
        env,
        command_ids::FUNNEL_REVISION_SAVE,
        &parameters,
        Some(&idempotency_key),
    )
    .await
    {
        Ok(result) => json_success_response(result, StatusCode::OK, &correlation_id, None),
        Err(error) => funnel_command_error_response(error, &correlation_id),
    }
}

async fn unreadable_editor_payload_response(
    env: &Env,
    idempotency_key: &str,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    execute_unreadable_editor_payload(
        env,
        idempotency_key,
        "validation/invalid_json",
        correlation_id,
    )
    .await
}

async fn execute_unreadable_editor_payload(
    env: &Env,
    idempotency_key: &str,
    fresh_error_code: &'static str,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    match execute_funnel_command(
        env,
        command_ids::FUNNEL_REVISION_SAVE,
        "null",
        Some(idempotency_key),
    )
    .await
    {
        Err(error) => funnel_command_error_response(error, correlation_id),
        Ok(_) => editor_payload_error_response(correlation_id, fresh_error_code),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditorCommandParametersError {
    InvalidJson,
    DuplicateObjectKey,
}

impl From<JsonValidationError> for EditorCommandParametersError {
    fn from(error: JsonValidationError) -> Self {
        match error {
            JsonValidationError::Invalid => Self::InvalidJson,
            JsonValidationError::DuplicateObjectKey => Self::DuplicateObjectKey,
        }
    }
}

fn editor_command_parameters(
    funnel_id: &str,
    body: &str,
) -> Result<String, EditorCommandParametersError> {
    validate_unique_json(body).map_err(EditorCommandParametersError::from)?;

    let mut value: Value =
        serde_json::from_str(body).map_err(|_| EditorCommandParametersError::InvalidJson)?;
    let object = value
        .as_object_mut()
        .ok_or(EditorCommandParametersError::InvalidJson)?;
    if object
        .keys()
        .any(|key| key != "base_revision_id" && key != "section_ids" && key != "settings")
    {
        return Err(EditorCommandParametersError::InvalidJson);
    }
    object.insert("funnel_id".to_owned(), Value::String(funnel_id.to_owned()));
    serde_json::to_string(&value).map_err(|_| EditorCommandParametersError::InvalidJson)
}

fn editor_payload_error_response(
    correlation_id: &str,
    code: &'static str,
) -> Response<axum::body::Body> {
    let error = ApiError::from_code(code);
    log_api_error(&error, correlation_id, error.code());
    error_response(&error, correlation_id.to_owned())
}

async fn funnel_item_response(
    env: &Env,
    request: HttpRequest,
    funnel_id: &str,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if request.method() == Method::GET {
        if !valid_funnel_route_id(funnel_id) {
            return funnel_route_not_found_response(&correlation_id);
        }
        let parameters = serde_json::json!({ "funnel_id": funnel_id }).to_string();
        return match execute_funnel_command(env, command_ids::FUNNEL_EDITOR_READ, &parameters, None)
            .await
        {
            Ok(result) => {
                let mut response =
                    json_success_response(result, StatusCode::OK, &correlation_id, None);
                response
                    .headers_mut()
                    .insert("cache-control", HeaderValue::from_static("no-store"));
                response
            }
            Err(error) => funnel_command_error_response(error, &correlation_id),
        };
    }
    if request.method() != Method::DELETE {
        return method_not_allowed_for(&correlation_id, "GET,DELETE");
    }
    let valid_route_id = valid_funnel_route_id(funnel_id);
    let idempotency_key = match request_idempotency_key(request.headers()) {
        Ok(key) => key,
        Err(_) if !valid_route_id => {
            return funnel_route_not_found_response(&correlation_id);
        }
        Err(code) => {
            let error = ApiError::from_code(code);
            log_api_error(&error, &correlation_id, error.code());
            return error_response(&error, correlation_id);
        }
    };
    let parameters = match serde_json::to_string(&serde_json::json!({ "funnel_id": funnel_id })) {
        Ok(parameters) => parameters,
        Err(_) => {
            let error = ApiError::from_code("internal/unexpected");
            log_api_error(&error, &correlation_id, error.code());
            return error_response(&error, correlation_id);
        }
    };
    match execute_funnel_command(
        env,
        command_ids::FUNNEL_DELETE,
        &parameters,
        Some(&idempotency_key),
    )
    .await
    {
        Ok(result) if valid_route_id => {
            json_success_response(result, StatusCode::OK, &correlation_id, None)
        }
        Ok(_) => funnel_route_not_found_response(&correlation_id),
        Err(error) if !valid_route_id && error.code() == "validation/invalid_json" => {
            funnel_route_not_found_response(&correlation_id)
        }
        Err(error) => funnel_command_error_response(error, &correlation_id),
    }
}

fn funnel_route_not_found_response(correlation_id: &str) -> Response<axum::body::Body> {
    let error = ApiError::from_code("not_found/funnel");
    log_api_error(&error, correlation_id, error.code());
    error_response(&error, correlation_id.to_owned())
}

async fn execute_funnel_command(
    env: &Env,
    command_id: &str,
    parameters: &str,
    idempotency_key: Option<&str>,
) -> Result<Value, ApiError> {
    let actor = command_registry()
        .iter()
        .find(|command| command.id() == command_id)
        .map(|command| command.actor())
        .ok_or_else(|| ApiError::from_code("validation/operation_not_supported"))?;
    let bindings = eez_storage::Bindings::from_env(env)
        .map_err(|_| ApiError::from_code("external/unavailable"))?;
    let repository = bindings.repository();
    Session::new(&repository)
        .execute(actor, command_id, parameters, idempotency_key)
        .await
        .map_err(|error| ApiError::from_code(error.code()))
}

fn request_idempotency_key(headers: &HeaderMap<HeaderValue>) -> Result<String, &'static str> {
    let mut values = headers.get_all("idempotency-key").iter();
    let value = values.next().ok_or("validation/missing_field")?;
    if values.next().is_some() {
        return Err("validation/invalid_json");
    }
    let value = value.to_str().map_err(|_| "validation/invalid_json")?;
    if !(1..=128).contains(&value.len())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err("validation/invalid_json");
    }
    Ok(value.to_owned())
}

fn valid_funnel_route_id(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn funnel_subroute<'a>(path: &'a str, suffix: &str) -> Option<&'a str> {
    let prefix = path.strip_suffix(suffix)?;
    let funnel_id = prefix.strip_prefix("/api/funnels/")?;
    if funnel_id.is_empty() || funnel_id.contains('/') {
        None
    } else {
        Some(funnel_id)
    }
}

fn funnel_command_error_response(
    error: ApiError,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    log_api_error(&error, correlation_id, error.code());
    error_response(&error, correlation_id.to_owned())
}

fn json_success_response(
    mut result: Value,
    status: StatusCode,
    correlation_id: &str,
    location: Option<String>,
) -> Response<axum::body::Body> {
    let Some(object) = result.as_object_mut() else {
        let error = ApiError::from_code("internal/unexpected");
        log_api_error(&error, correlation_id, error.code());
        return error_response(&error, correlation_id.to_owned());
    };
    object.insert(
        "correlation_id".to_owned(),
        Value::String(correlation_id.to_owned()),
    );
    let mut response = Json(result).into_response();
    *response.status_mut() = status;
    if let Ok(value) = HeaderValue::from_str(correlation_id) {
        response.headers_mut().insert("x-correlation-id", value);
    }
    if let Some(location) = location {
        if let Ok(value) = HeaderValue::from_str(&location) {
            response.headers_mut().insert("location", value);
        }
    }
    response
}

fn method_not_allowed_for(correlation_id: &str, allow: &'static str) -> Response<axum::body::Body> {
    let error = ApiError::from_code("validation/operation_not_supported");
    log_api_error(&error, correlation_id, error.code());
    let mut response = error_response(&error, correlation_id.to_owned());
    response
        .headers_mut()
        .insert(axum::http::header::ALLOW, HeaderValue::from_static(allow));
    response
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
    if !local_probe_authorized(env, headers) {
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

async fn local_product_probe(
    env: &Env,
    headers: &HeaderMap<HeaderValue>,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if !local_probe_authorized(env, headers) {
        let error = ApiError::from_code("not_found/route");
        log_api_error(
            &error,
            &correlation_id,
            "local product probe route is not available",
        );
        return error_response(&error, correlation_id);
    }

    let result = async {
        let bindings = eez_storage::Bindings::from_env(env)?;
        let publication_regressions = bindings.verify_product_repository_round_trip().await?;
        let revision_ids = bindings.prepare_publication_race().await?;
        Ok::<_, worker::Error>((publication_regressions, revision_ids))
    }
    .await;

    match result {
        Ok((publication_regressions, publication_race_revision_ids)) => {
            Json(ProductProbeResponse {
                status: "ok",
                funnels: "ok",
                revisions: "ok",
                themes: "ok",
                publication: "ok",
                unsupported_schema_version: "rejected",
                cross_funnel_published_revision_insert_accepted: publication_regressions
                    .cross_funnel_published_revision_insert_accepted,
                null_published_revision_insert_accepted: publication_regressions
                    .null_published_revision_insert_accepted,
                unsupported_schema_publish_accepted: publication_regressions
                    .unsupported_schema_publish_accepted,
                unsupported_schema_published_revision_id: publication_regressions
                    .unsupported_schema_published_revision_id,
                current_revision_noop_accepted: publication_regressions
                    .current_revision_noop_accepted,
                publication_baseline_cas_successes: publication_regressions
                    .publication_baseline_cas_successes,
                distinct_candidate_cas_successes: publication_regressions
                    .distinct_candidate_cas_successes,
                publication_replay_interleavings_safe: publication_regressions
                    .publication_replay_interleavings_safe,
                publication_race_funnel_id: "pr003-race-funnel",
                publication_race_revision_ids,
                correlation_id,
            })
            .into_response()
        }
        Err(source) => {
            let error = ApiError::from_storage_error(&source);
            log_api_error(&error, &correlation_id, &format!("{source:?}"));
            error_response(&error, correlation_id)
        }
    }
}

async fn local_publication_race(
    env: &Env,
    headers: &HeaderMap<HeaderValue>,
    query: Option<&str>,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if !local_probe_authorized(env, headers) {
        let error = ApiError::from_code("not_found/route");
        log_api_error(
            &error,
            &correlation_id,
            "local publication probe is not available",
        );
        return error_response(&error, correlation_id);
    }
    let Some(revision_id) = query_parameter(query, "revision_id") else {
        let error = ApiError::from_code("not_found/route");
        log_api_error(
            &error,
            &correlation_id,
            "local publication candidate is missing",
        );
        return error_response(&error, correlation_id);
    };
    let expected_published_revision_id = query_parameter(query, "expected_published_revision_id");

    let result = async {
        let bindings = eez_storage::Bindings::from_env(env)?;
        bindings
            .publish_race_candidate(revision_id, expected_published_revision_id)
            .await
    }
    .await;
    match result {
        Ok(published) => Json(PublicationRaceResponse {
            published,
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

async fn local_publication_status(
    env: &Env,
    headers: &HeaderMap<HeaderValue>,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if !local_probe_authorized(env, headers) {
        let error = ApiError::from_code("not_found/route");
        log_api_error(
            &error,
            &correlation_id,
            "local publication probe is not available",
        );
        return error_response(&error, correlation_id);
    }

    let result = async {
        let bindings = eez_storage::Bindings::from_env(env)?;
        bindings.publication_race_winner().await
    }
    .await;
    match result {
        Ok(published_revision_id) => Json(PublicationStatusResponse {
            published_revision_id,
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

async fn local_publication_race_cleanup(
    env: &Env,
    headers: &HeaderMap<HeaderValue>,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if !local_probe_authorized(env, headers) {
        let error = ApiError::from_code("not_found/route");
        log_api_error(
            &error,
            &correlation_id,
            "local publication probe is not available",
        );
        return error_response(&error, correlation_id);
    }
    let result = async {
        let bindings = eez_storage::Bindings::from_env(env)?;
        bindings.cleanup_publication_race().await
    }
    .await;
    match result {
        Ok(()) => Json(HealthResponse { status: "ok" }).into_response(),
        Err(source) => {
            let error = ApiError::from_storage_error(&source);
            log_api_error(&error, &correlation_id, &format!("{source:?}"));
            error_response(&error, correlation_id)
        }
    }
}

fn query_parameter<'a>(query: Option<&'a str>, name: &str) -> Option<&'a str> {
    query?.split('&').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key == name && !value.is_empty()).then_some(value)
    })
}

fn local_probe_authorized(env: &Env, headers: &HeaderMap<HeaderValue>) -> bool {
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
    matches!(
        (is_development, expected_token.as_deref(), supplied_token),
        (true, Some(expected), Some(supplied)) if !expected.is_empty() && expected == supplied
    )
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
    let correlation_header = HeaderValue::from_str(&correlation_id).ok();
    let mut response = Json(error.response(correlation_id)).into_response();
    *response.status_mut() = status;
    if let Some(value) = correlation_header {
        response.headers_mut().insert("x-correlation-id", value);
    }
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
    use super::{
        correlation_id, normalize_correlation_id, public_slug_from_path,
        publication_command_parameters, request_if_none_match, valid_public_slug,
        EditorCommandParametersError,
    };
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

    #[test]
    fn publication_adapter_rejects_duplicate_keys_before_normalization() {
        assert_eq!(
            publication_command_parameters(
                "funnel-a",
                "funnel.publish",
                r#"{"revision_id":"revision-a","expected_published_revision_id":null,"expected_published_revision_id":"revision-a"}"#,
            ),
            Err(EditorCommandParametersError::DuplicateObjectKey)
        );
        assert_eq!(
            publication_command_parameters(
                "funnel-a",
                "funnel.unpublish",
                r#"{"expected_published_revision_id":"revision-a","nested":{"x":1,"x":2}}"#,
            ),
            Err(EditorCommandParametersError::DuplicateObjectKey)
        );
    }

    #[test]
    fn publication_adapter_owns_route_identity_and_rejects_extra_fields() {
        assert_eq!(
            publication_command_parameters(
                "funnel-route",
                "funnel.publish",
                r#"{"revision_id":"revision-a","expected_published_revision_id":null}"#,
            )
            .expect("closed publish request"),
            r#"{"expected_published_revision_id":null,"funnel_id":"funnel-route","revision_id":"revision-a"}"#
        );
        assert_eq!(
            publication_command_parameters(
                "funnel-route",
                "funnel.publish",
                r#"{"funnel_id":"client-chosen","revision_id":"revision-a","expected_published_revision_id":null}"#,
            ),
            Err(EditorCommandParametersError::InvalidJson)
        );
        assert_eq!(
            publication_command_parameters(
                "funnel-route",
                "funnel.unpublish",
                r#"{"expected_published_revision_id":"revision-a","html":"unsafe"}"#,
            ),
            Err(EditorCommandParametersError::InvalidJson)
        );
    }

    #[test]
    fn public_slug_routing_and_etag_comparison_are_strict_and_support_weak_get_validators() {
        let mut headers = HeaderMap::new();
        let etag = "\"revision-a\"";
        headers.insert(
            "if-none-match",
            HeaderValue::from_static("\"older\", W/\"revision-a\""),
        );
        assert!(request_if_none_match(&headers, etag));
        headers.insert("if-none-match", HeaderValue::from_static("*"));
        assert!(request_if_none_match(&headers, etag));
        headers.insert("if-none-match", HeaderValue::from_static("\"older\""));
        assert!(!request_if_none_match(&headers, etag));

        assert_eq!(public_slug_from_path("/f/quick-offer"), Some("quick-offer"));
        assert_eq!(public_slug_from_path("/f/quick-offer/extra"), None);
        assert!(valid_public_slug("quick-offer-123"));
        assert!(!valid_public_slug("../private"));
        assert!(!valid_public_slug("quick_offer"));
    }
}
