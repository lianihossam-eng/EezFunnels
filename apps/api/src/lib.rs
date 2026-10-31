#![forbid(unsafe_code)]

use axum::body::to_bytes;
use axum::extract::Json;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use eez_commands::public_forms::{
    submit_public_form, SubmitPublicFormError, SubmitPublicFormInput, SubmitPublicFormOutcome,
};
use eez_commands::public_interactions::{
    record_interaction, RecordInteractionError, RecordInteractionInput, RecordInteractionOutcome,
};
use eez_commands::{command_ids, command_registry, Session};
use eez_domain::document_v2::{DocumentV2, FormId, PageId};
use eez_domain::interaction_event::{
    InteractionConsent, InteractionEventPayload, DEFAULT_INTERACTION_RETENTION_DAYS,
    MAX_INTERACTION_RETENTION_DAYS,
};
use eez_domain::{
    FormCompletionPeriod, FormSubmissionChannel, FormSubmissionStatus, InteractionReportPeriod,
};
use eez_storage::repository::{PublicFormSubmissionCursor, PublishedRevisionRecord};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tower_service::Service;
use worker::{event, Context, Env, HttpRequest};

use crate::errors::ApiError;
use crate::json::{validate_unique_json, JsonValidationError};

mod campaign_registry;
mod canonical;
mod custom_hosts;
mod delivery_controls;
pub mod errors;
pub mod host;
mod json;
mod mcp;
mod origin;
mod service_brief;
mod sitemap;
mod submission_follow_up;
mod webhook_transport;

const INTERACTION_SCRIPT: &str = include_str!("interaction.js");

pub use host::HostAllowlist;

use custom_hosts::{platform_hostname_conflicts, CustomHostMapping, CustomHostMappings};

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
    let (platform_hosts, custom_hosts) = request_host_configuration(&env);
    let custom_host = custom_hosts.for_headers(request.headers());
    let host_is_allowed = platform_hosts.allows_headers(request.headers()) || custom_host.is_some();
    let recovery_route = request_path == "/api/submissions/recovery";
    let submission_gate_open =
        if host_is_allowed && !recovery_route && submission_recovery_gated_path(&request_path) {
            match eez_storage::Bindings::from_env(&env) {
                Ok(bindings) => bindings
                    .submission_lifecycle_repository()
                    .require_recovery_ready()
                    .await
                    .is_ok(),
                Err(_) => false,
            }
        } else {
            true
        };

    let mut response = if !host_is_allowed {
        let error = ApiError::from_code("authorization/host_not_allowed");
        log_api_error(&error, &correlation_id, "request host is not allowlisted");
        error_response(&error, correlation_id.clone())
    } else if !submission_gate_open {
        lifecycle_unavailable_response(&correlation_id)
    } else if let Some(mapping) = custom_host.as_ref() {
        custom_host_response(
            &env,
            request,
            mapping,
            &custom_hosts,
            correlation_id.clone(),
        )
        .await
    } else if request_path == "/robots.txt" {
        robots_response(&env, request, correlation_id.clone())
    } else if request_path == "/sitemap.xml" {
        sitemap_response(&env, request, correlation_id.clone(), &custom_hosts, None).await
    } else if request_path == "/_eez/interaction.js" {
        interaction_script_response(request, correlation_id.clone())
    } else if request_path == "/mcp" {
        mcp::handle(&env, request, &correlation_id).await
    } else if request_path == "/api/media" {
        media_collection_response(&env, request, correlation_id.clone()).await
    } else if let Some(asset_id) = request_path.strip_prefix("/media/") {
        media_asset_response(&env, request, asset_id, correlation_id.clone()).await
    } else if request_path == "/api/v2/stage-capacity" {
        v2_stage_capacity_response(&env, request, correlation_id.clone()).await
    } else if request_path == "/api/submissions/recovery" {
        submission_recovery_response(&env, request, correlation_id.clone()).await
    } else if request_path == "/api/webhook-delivery" {
        delivery_controls::installation_response(&env, request, &correlation_id).await
    } else if let Some((submission_id, action)) =
        delivery_controls::submission_action_from_path(&request_path)
    {
        delivery_controls::submission_action_response(
            &env,
            request,
            submission_id,
            action,
            &correlation_id,
        )
        .await
    } else if let Some(submission_id) = form_submission_export_id_from_path(&request_path) {
        form_submission_export_response(&env, request, submission_id, correlation_id.clone()).await
    } else if let Some(submission_id) = submission_follow_up::submission_id_from_path(&request_path)
    {
        submission_follow_up::response(&env, request, submission_id, &correlation_id).await
    } else if let Some(submission_id) = form_submission_delete_id_from_path(&request_path) {
        form_submission_delete_response(&env, request, submission_id, correlation_id.clone()).await
    } else if let Some(submission_id) = form_submission_status_id_from_path(&request_path) {
        form_submission_status_response(&env, request, submission_id, correlation_id.clone()).await
    } else if request_path == "/api/submissions" {
        form_submission_list_response(&env, request, correlation_id.clone()).await
    } else if request_path == "/api/funnels" {
        funnel_collection_response(&env, request, correlation_id.clone()).await
    } else if request_path == "/api/funnels/v2" {
        funnel_create_response(
            &env,
            request,
            command_ids::FUNNEL_CREATE_V2,
            correlation_id.clone(),
        )
        .await
    } else if let Some(funnel_id) = funnel_subroute(&request_path, "/completions") {
        funnel_completion_report_response(&env, request, funnel_id, correlation_id.clone()).await
    } else if let Some(funnel_id) = funnel_subroute(&request_path, "/interaction-report") {
        funnel_interaction_report_response(&env, request, funnel_id, correlation_id.clone()).await
    } else if let Some(route) = campaign_registry::item_from_path(&request_path) {
        campaign_registry::item_response(&env, request, route, &correlation_id).await
    } else if let Some(funnel_id) = funnel_subroute(&request_path, "/campaigns") {
        campaign_registry::collection_response(&env, request, funnel_id, &correlation_id).await
    } else if let Some(funnel_id) = funnel_subroute(&request_path, "/service-brief") {
        service_brief::response(&env, request, funnel_id, correlation_id.clone()).await
    } else if let Some(funnel_id) = funnel_subroute(&request_path, "/webhook-delivery") {
        delivery_controls::funnel_response(&env, request, funnel_id, &correlation_id).await
    } else if let Some(route) = public_interaction_events_route_from_path(&request_path) {
        public_interaction_events_response(&env, request, route, correlation_id.clone(), None).await
    } else if let Some(route) = public_form_submission_route_from_path(&request_path) {
        public_form_submission_response(&env, request, route, correlation_id.clone()).await
    } else if let Some(funnel_id) = funnel_subroute(&request_path, "/document") {
        funnel_document_response(&env, request, funnel_id, correlation_id.clone()).await
    } else if let Some(funnel_id) = funnel_subroute(&request_path, "/convert-v2/preview") {
        funnel_v2_conversion_response(&env, request, funnel_id, correlation_id.clone(), false).await
    } else if let Some(funnel_id) = funnel_subroute(&request_path, "/convert-v2") {
        funnel_v2_conversion_response(&env, request, funnel_id, correlation_id.clone(), true).await
    } else if let Some(funnel_id) = funnel_subroute(&request_path, "/preview-v2") {
        funnel_v2_preview_response(&env, request, funnel_id, correlation_id.clone()).await
    } else if let Some(funnel_id) = funnel_subroute(&request_path, "/edit-v2") {
        funnel_v2_edit_response(&env, request, funnel_id, correlation_id.clone()).await
    } else if let Some(funnel_id) = funnel_subroute(&request_path, "/stages-v2") {
        funnel_v2_stage_mint_response(&env, request, funnel_id, correlation_id.clone()).await
    } else if let Some(funnel_id) = funnel_subroute(&request_path, "/revisions-v2") {
        funnel_v2_save_response(&env, request, funnel_id, correlation_id.clone()).await
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
    } else if let Some(route) = public_route_from_path(&request_path) {
        let mapped_origin = custom_hosts
            .mapping_for_funnel(route.funnel_slug)
            .map(|mapping| CustomHostMappings::origin(&mapping));
        public_funnel_response(
            &env,
            request,
            route,
            correlation_id.clone(),
            mapped_origin.as_deref(),
        )
        .await
    } else if let Some(funnel_id) = request_path.strip_prefix("/api/funnels/") {
        funnel_item_response(&env, request, funnel_id, correlation_id.clone()).await
    } else if request_path == "/__local/probe" && request.method() == Method::POST {
        local_probe(&env, request.headers(), correlation_id.clone()).await
    } else if request_path == "/__local/stage-control/bootstrap" && request.method() == Method::POST
    {
        local_stage_control_bootstrap(&env, &correlation_id).await
    } else if request_path == "/__local/stage-sweep" && request.method() == Method::POST {
        local_stage_sweep(&env, &correlation_id).await
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

    if funnel_subroute(&request_path, "/completions").is_some()
        || funnel_subroute(&request_path, "/interaction-report").is_some()
        || funnel_subroute(&request_path, "/service-brief").is_some()
        || funnel_subroute(&request_path, "/webhook-delivery").is_some()
        || request_path == "/api/webhook-delivery"
        || request_path == "/api/submissions"
        || request_path.starts_with("/api/submissions/")
    {
        response.headers_mut().insert(
            "cache-control",
            HeaderValue::from_static("private, no-store"),
        );
    }

    log_request(
        &method,
        &request_path,
        response.status().as_u16(),
        &correlation_id,
    );

    Ok(response)
}

#[event(scheduled)]
pub async fn scheduled(event: worker::ScheduledEvent, env: Env, _context: worker::ScheduleContext) {
    match event.cron().as_str() {
        "* * * * *" => run_webhook_dispatch(&env).await,
        "*/15 * * * *" => run_periodic_cleanup(&env).await,
        _ => {}
    }
}

async fn run_webhook_dispatch(env: &Env) {
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => {
            worker::console_error!("Scheduled webhook dispatch could not load bindings");
            return;
        }
    };
    let lifecycle = bindings.submission_lifecycle_repository();
    let deliveries = bindings.submission_delivery_repository();
    match deliveries.has_due_tasks().await {
        Ok(true) => {}
        Ok(false) => return,
        Err(_) => {
            worker::console_error!("Scheduled webhook dispatch could not check due tasks");
            return;
        }
    }
    if lifecycle.require_recovery_ready().await.is_err() {
        worker::console_warn!("Scheduled webhook dispatch paused by submission recovery gate");
        return;
    }
    if deliveries
        .terminalize_exhausted_and_expired()
        .await
        .is_err()
    {
        worker::console_error!("Scheduled webhook dispatch could not terminalize expired tasks");
        return;
    }
    match deliveries.has_dispatchable_tasks().await {
        Ok(true) => {}
        Ok(false) => return,
        Err(_) => {
            worker::console_error!("Scheduled webhook dispatch could not check dispatchable tasks");
            return;
        }
    }
    let secret = match env.secret("WEBHOOK_SIGNING_SECRET") {
        Ok(secret) => secret.to_string(),
        Err(_) => {
            worker::console_warn!(
                "Scheduled webhook dispatch paused: signing secret is unavailable"
            );
            return;
        }
    };
    let signing_key = match eez_domain::webhook_delivery::WebhookSigningKey::parse(&secret) {
        Ok(key) => key,
        Err(_) => {
            worker::console_error!("Scheduled webhook dispatch paused: signing secret is invalid");
            return;
        }
    };
    match eez_commands::public_delivery::dispatch_due(
        &lifecycle,
        &deliveries,
        &signing_key,
        &webhook_transport::CloudflareWebhookTransport,
    )
    .await
    {
        Ok(summary) if summary.paused => {
            worker::console_warn!("Scheduled webhook dispatch paused by submission recovery gate");
        }
        Ok(summary) if summary.attempted > 0 || summary.terminal > 0 => worker::console_log!(
            "Scheduled webhook dispatch completed: attempted={}, accepted={}, retryable={}, terminal={}",
            summary.attempted,
            summary.accepted,
            summary.retryable,
            summary.terminal
        ),
        Ok(_) => {}
        Err(_) => worker::console_error!("Scheduled webhook dispatch failed; durable tasks remain available for retry"),
    }
}

async fn run_periodic_cleanup(env: &Env) {
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(error) => {
            worker::console_error!("Scheduled cleanup could not load bindings: {error:?}");
            return;
        }
    };
    let lifecycle = bindings.submission_lifecycle_repository();
    match lifecycle.recovery_status().await {
        Ok(status) if status.phase == eez_storage::submission_lifecycle::RecoveryPhase::Ready => {
            if let Err(error) =
                eez_commands::submission_lifecycle::maintain_pending(&lifecycle, 16).await
            {
                worker::console_error!("Pending submission erasure maintenance failed: {error:?}");
            }
            if let Err(error) = bindings
                .repository()
                .cleanup_expired_public_form_submissions()
                .await
            {
                worker::console_error!("Expired public form submission cleanup failed: {error:?}");
            }
            if lifecycle.cleanup_expired_markers().await.is_err() {
                worker::console_error!("Expired submission erasure marker cleanup failed");
            }
            if lifecycle.cleanup_journal_page().await.is_err() {
                worker::console_error!("Submission erasure journal cleanup failed");
            }
        }
        Ok(status)
            if status.phase == eez_storage::submission_lifecycle::RecoveryPhase::Preparing =>
        {
            if let Err(error) =
                eez_commands::submission_lifecycle::maintain_pending(&lifecycle, 16).await
            {
                worker::console_error!("Pending submission erasure maintenance failed: {error:?}");
            }
        }
        Ok(_) => {}
        Err(_) => worker::console_error!(
            "Submission recovery gate unavailable; submission cleanup paused"
        ),
    }
    if let Err(error) = bindings
        .repository()
        .cleanup_expired_public_interaction_events()
        .await
    {
        worker::console_error!("Expired public interaction event cleanup failed: {error:?}");
    }

    if eez_storage::stage::StageCapacity::from_env(env).is_none() {
        worker::console_warn!(
            "V2 document stage cleanup skipped because orphan budgets are not configured"
        );
        return;
    }
    let repository = bindings.document_stage_repository();
    match repository.cleanup_one_page().await {
        Ok(progress) => worker::console_log!(
            "V2 document stage cleanup completed: listed={}, deleted={}, pass_completed={}",
            progress.listed,
            progress.deleted,
            progress.pass_completed
        ),
        Err(error) => worker::console_error!("V2 document stage cleanup failed: {error:?}"),
    }
}

async fn asset_response(
    env: &Env,
    request: HttpRequest,
) -> worker::Result<Response<axum::body::Body>> {
    let response = env.assets("ASSETS")?.fetch_request(request).await?;
    let (parts, body) = response.into_parts();
    Ok(Response::from_parts(parts, axum::body::Body::new(body)))
}

async fn media_collection_response(
    env: &Env,
    request: HttpRequest,
    correlation_id: String,
) -> Response<axum::body::Body> {
    match request.method().as_str() {
        "GET" => {
            let bindings = match eez_storage::Bindings::from_env(env) {
                Ok(bindings) => bindings,
                Err(_) => return media_error_response("external/unavailable", &correlation_id),
            };
            match bindings.media_asset_repository().list().await {
                Ok(assets) => {
                    let mut response = json_success_response(
                        serde_json::json!({ "assets": assets }),
                        StatusCode::OK,
                        &correlation_id,
                        None,
                    );
                    response
                        .headers_mut()
                        .insert("cache-control", HeaderValue::from_static("no-store"));
                    response
                }
                Err(error) => media_error_response(error.code(), &correlation_id),
            }
        }
        "POST" => {
            let content_types = request.headers().get_all("content-type");
            let content_type = content_types
                .iter()
                .next()
                .and_then(|value| value.to_str().ok());
            if content_types.iter().count() != 1
                || !matches!(
                    content_type,
                    Some("image/png" | "image/jpeg" | "image/webp")
                )
            {
                return media_error_response("validation/media_asset_invalid", &correlation_id);
            }
            let content_type = content_type.unwrap_or_default().to_owned();
            let body = match to_bytes(
                axum::body::Body::new(request.into_body()),
                MAX_MEDIA_UPLOAD_BYTES,
            )
            .await
            {
                Ok(body) => body,
                Err(_) => {
                    return media_error_response(
                        "validation/media_asset_too_large",
                        &correlation_id,
                    )
                }
            };
            let bindings = match eez_storage::Bindings::from_env(env) {
                Ok(bindings) => bindings,
                Err(_) => return media_error_response("external/unavailable", &correlation_id),
            };
            match bindings
                .media_asset_repository()
                .upload(&content_type, &body)
                .await
            {
                Ok(asset) => {
                    let mut response = json_success_response(
                        serde_json::json!({ "asset": asset }),
                        StatusCode::CREATED,
                        &correlation_id,
                        None,
                    );
                    response
                        .headers_mut()
                        .insert("cache-control", HeaderValue::from_static("no-store"));
                    response
                }
                Err(error) => media_error_response(error.code(), &correlation_id),
            }
        }
        _ => method_not_allowed_for(&correlation_id, "GET, POST"),
    }
}

async fn media_asset_response(
    env: &Env,
    request: HttpRequest,
    asset_id: &str,
    correlation_id: String,
) -> Response<axum::body::Body> {
    let is_head = request.method() == Method::HEAD;
    if request.method() != Method::GET && !is_head {
        return method_not_allowed_for(&correlation_id, "GET, HEAD");
    }
    if !eez_storage::media::is_lower_sha256(asset_id) {
        return media_error_response("not_found/asset", &correlation_id);
    }
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return media_error_response("external/unavailable", &correlation_id),
    };
    let content = match bindings.media_asset_repository().read(asset_id).await {
        Ok(content) => content,
        Err(error) => return media_error_response(error.code(), &correlation_id),
    };
    let content_type = match content.record.mime.as_str() {
        "image/png" => HeaderValue::from_static("image/png"),
        "image/jpeg" => HeaderValue::from_static("image/jpeg"),
        "image/webp" => HeaderValue::from_static("image/webp"),
        _ => return media_error_response("external/unavailable", &correlation_id),
    };
    let Ok(etag) = HeaderValue::from_str(&format!("\"{}\"", content.record.sha256)) else {
        return media_error_response("external/unavailable", &correlation_id);
    };
    let Ok(content_length) = HeaderValue::from_str(&content.record.size_bytes.to_string()) else {
        return media_error_response("external/unavailable", &correlation_id);
    };
    let mut response = Response::new(if is_head {
        axum::body::Body::empty()
    } else {
        axum::body::Body::from(content.bytes)
    });
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert(axum::http::header::CONTENT_TYPE, content_type);
    response
        .headers_mut()
        .insert(axum::http::header::CONTENT_LENGTH, content_length);
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    response
        .headers_mut()
        .insert(axum::http::header::ETAG, etag);
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    if let Ok(value) = HeaderValue::from_str(&correlation_id) {
        response.headers_mut().insert("x-correlation-id", value);
    }
    response
}

fn media_error_response(code: &str, correlation_id: &str) -> Response<axum::body::Body> {
    let error = ApiError::from_code(code);
    log_api_error(&error, correlation_id, error.code());
    error_response(&error, correlation_id.to_owned())
}

fn robots_response(
    env: &Env,
    request: HttpRequest,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return method_not_allowed_for(&correlation_id, "GET, HEAD");
    }
    let configured_origin = env.var("PUBLIC_BASE_URL").ok();
    let configured_origin = configured_origin.as_ref().map(|value| value.to_string());
    let origin = crate::canonical::public_origin_url(configured_origin.as_deref());
    let body = sitemap::build_robots(origin.as_ref());
    let is_head = request.method() == Method::HEAD;
    let mut response = Response::new(if is_head {
        axum::body::Body::empty()
    } else {
        axum::body::Body::from(body)
    });
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
    );
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    if let Ok(value) = HeaderValue::from_str(&correlation_id) {
        response.headers_mut().insert("x-correlation-id", value);
    }
    response
}

async fn custom_host_response(
    env: &Env,
    request: HttpRequest,
    mapping: &CustomHostMapping,
    custom_hosts: &CustomHostMappings,
    correlation_id: String,
) -> Response<axum::body::Body> {
    let path = request.uri().path().to_owned();
    let Some(route) = custom_host_route_from_path(&path, &mapping.funnel_slug) else {
        return funnel_route_not_found_response(&correlation_id);
    };
    match route {
        CustomHostRequestRoute::Page(route) => {
            let origin = CustomHostMappings::origin(mapping);
            public_funnel_response(env, request, route, correlation_id, Some(&origin)).await
        }
        CustomHostRequestRoute::FormSubmission(route) => {
            public_form_submission_response(env, request, route, correlation_id).await
        }
        CustomHostRequestRoute::InteractionEvents(route) => {
            public_interaction_events_response(env, request, route, correlation_id, Some(mapping))
                .await
        }
        CustomHostRequestRoute::InteractionScript => {
            interaction_script_response(request, correlation_id)
        }
        CustomHostRequestRoute::Media(asset_id) => {
            if request.method() != Method::GET && request.method() != Method::HEAD {
                return method_not_allowed_for(&correlation_id, "GET, HEAD");
            }
            if !mapped_funnel_references_asset(env, &mapping.funnel_slug, asset_id).await {
                return funnel_route_not_found_response(&correlation_id);
            }
            media_asset_response(env, request, asset_id, correlation_id).await
        }
        CustomHostRequestRoute::Robots => {
            if request.method() != Method::GET && request.method() != Method::HEAD {
                return method_not_allowed_for(&correlation_id, "GET, HEAD");
            }
            custom_robots_response(mapping, &correlation_id, request.method() == Method::HEAD)
        }
        CustomHostRequestRoute::Sitemap => {
            sitemap_response(env, request, correlation_id, custom_hosts, Some(mapping)).await
        }
    }
}

async fn mapped_funnel_references_asset(env: &Env, funnel_slug: &str, asset_id: &str) -> bool {
    let Ok(bindings) = eez_storage::Bindings::from_env(env) else {
        return false;
    };
    let Ok(Some(published)) = bindings
        .repository()
        .get_published_revision_by_slug(funnel_slug)
        .await
    else {
        return false;
    };
    published_document_references_asset(
        published.schema_version,
        &published.data_json,
        &published.funnel_id,
        asset_id,
    )
}

fn published_document_references_asset(
    schema_version: i64,
    data_json: &str,
    funnel_id: &str,
    asset_id: &str,
) -> bool {
    if schema_version != 2 || !eez_storage::media::is_lower_sha256(asset_id) {
        return false;
    }
    let Ok(document) = DocumentV2::decode(data_json.as_bytes(), funnel_id) else {
        return false;
    };
    eez_render::document_v2::referenced_asset_ids(&document)
        .iter()
        .any(|referenced| referenced == asset_id)
}

fn custom_robots_response(
    mapping: &CustomHostMapping,
    correlation_id: &str,
    is_head: bool,
) -> Response<axum::body::Body> {
    let origin = CustomHostMappings::origin(mapping);
    let origin = crate::canonical::public_origin_url(Some(&origin));
    let body = sitemap::build_custom_robots(origin.as_ref());
    let mut response = Response::new(if is_head {
        axum::body::Body::empty()
    } else {
        axum::body::Body::from(body)
    });
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
    );
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    if let Ok(value) = HeaderValue::from_str(correlation_id) {
        response.headers_mut().insert("x-correlation-id", value);
    }
    response
}

async fn sitemap_response(
    env: &Env,
    request: HttpRequest,
    correlation_id: String,
    custom_hosts: &CustomHostMappings,
    custom_host: Option<&CustomHostMapping>,
) -> Response<axum::body::Body> {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return method_not_allowed_for(&correlation_id, "GET, HEAD");
    }
    let custom_origin = custom_host.map(CustomHostMappings::origin);
    let configured_origin = if custom_origin.is_some() {
        custom_origin
    } else {
        env.var("PUBLIC_BASE_URL")
            .ok()
            .map(|value| value.to_string())
    };
    let origin = crate::canonical::public_origin_url(configured_origin.as_deref());
    let body = if let Some(origin) = origin.as_ref() {
        let bindings = match eez_storage::Bindings::from_env(env) {
            Ok(bindings) => bindings,
            Err(_) => {
                let error = ApiError::from_code("external/unavailable");
                log_api_error(
                    &error,
                    &correlation_id,
                    "sitemap storage bindings unavailable",
                );
                return error_response(&error, correlation_id);
            }
        };
        let published_revisions = match bindings.repository().list_published_revisions().await {
            Ok(revisions) => revisions,
            Err(_) => {
                let error = ApiError::from_code("external/unavailable");
                log_api_error(
                    &error,
                    &correlation_id,
                    "published sitemap rows unavailable",
                );
                return error_response(&error, correlation_id);
            }
        };
        let published_revisions = sitemap_revisions(published_revisions, custom_hosts, custom_host);
        match sitemap::build_sitemap(origin, &published_revisions) {
            Ok(sitemap) => sitemap,
            Err(()) => {
                let error = ApiError::from_code("internal/unexpected");
                log_api_error(&error, &correlation_id, "published sitemap data is invalid");
                return error_response(&error, correlation_id);
            }
        }
    } else {
        sitemap::empty_sitemap()
    };

    let is_head = request.method() == Method::HEAD;
    let mut response = Response::new(if is_head {
        axum::body::Body::empty()
    } else {
        axum::body::Body::from(body)
    });
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/xml; charset=utf-8"),
    );
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
    );
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    if let Ok(value) = HeaderValue::from_str(&correlation_id) {
        response.headers_mut().insert("x-correlation-id", value);
    }
    response
}

fn dashboard_response(document: String) -> Response<axum::body::Body> {
    let mut response = Response::new(axum::body::Body::from(document));
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
}

fn request_host_configuration(env: &Env) -> (HostAllowlist, CustomHostMappings) {
    let configured_hosts = env
        .var("ALLOWED_HOSTS")
        .map(|value| value.to_string())
        .unwrap_or_default();
    let mut platform_hosts = HostAllowlist::parse(&configured_hosts);
    let configured_mappings = env
        .var("PUBLIC_FUNNEL_HOSTS")
        .map(|value| value.to_string())
        .unwrap_or_default();
    let conflicting_platform_hosts =
        platform_hostname_conflicts(&configured_mappings, &platform_hosts);
    let public_base_hostname = env
        .var("PUBLIC_BASE_URL")
        .ok()
        .and_then(|value| crate::canonical::public_origin_url(Some(&value.to_string())))
        .and_then(|origin| origin.host_str().map(str::to_owned));
    let custom_hosts = match CustomHostMappings::parse(
        &configured_mappings,
        &platform_hosts,
        public_base_hostname.as_deref(),
    ) {
        Ok(mappings) => mappings,
        Err(()) => {
            for hostname in conflicting_platform_hosts {
                platform_hosts.block_hostname(&hostname);
            }
            worker::console_warn!(
                "PUBLIC_FUNNEL_HOSTS is invalid; custom funnel host routing is disabled"
            );
            CustomHostMappings::default()
        }
    };
    (platform_hosts, custom_hosts)
}

fn sitemap_revisions(
    published_revisions: Vec<PublishedRevisionRecord>,
    custom_hosts: &CustomHostMappings,
    custom_host: Option<&CustomHostMapping>,
) -> Vec<PublishedRevisionRecord> {
    published_revisions
        .into_iter()
        .filter(|published| match custom_host {
            Some(mapping) => published.slug == mapping.funnel_slug,
            None => !custom_hosts.has_funnel(&published.slug),
        })
        .collect()
}

const MAX_CREATE_REQUEST_BYTES: usize = 16 * 1024;
const MAX_EDITOR_REQUEST_BYTES: usize = 16 * 1_024;
const MAX_V2_DOCUMENT_BYTES: usize = 1_048_576;
const MAX_V2_STAGE_REQUEST_BYTES: usize = 16_384;
const MAX_V2_STAGE_UPLOAD_BYTES: usize = eez_storage::stage::V2_DOCUMENT_STAGE_MAX_BYTES;
const MAX_MEDIA_UPLOAD_BYTES: usize = eez_storage::media::MAX_MEDIA_ASSET_BYTES;
const MAX_V2_PREVIEW_ENVELOPE_BYTES: usize = MAX_V2_DOCUMENT_BYTES * 6 + 4 * 1_024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PublicFunnelRoute<'a> {
    funnel_slug: &'a str,
    page_slug: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PublicFormSubmissionRoute<'a> {
    funnel_slug: &'a str,
    page_id: &'a str,
    form_id: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PublicInteractionEventsRoute<'a> {
    funnel_slug: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CustomHostRequestRoute<'a> {
    Page(PublicFunnelRoute<'a>),
    FormSubmission(PublicFormSubmissionRoute<'a>),
    InteractionEvents(PublicInteractionEventsRoute<'a>),
    InteractionScript,
    Media(&'a str),
    Robots,
    Sitemap,
}

fn public_route_from_path(path: &str) -> Option<PublicFunnelRoute<'_>> {
    let mut segments = path.split('/');
    if !segments.next()?.is_empty() || segments.next()? != "f" {
        return None;
    }
    let funnel_slug = segments.next()?;
    if funnel_slug.is_empty() {
        return None;
    }
    match (segments.next(), segments.next()) {
        (None, None) => Some(PublicFunnelRoute {
            funnel_slug,
            page_slug: None,
        }),
        (Some(page_slug), None) if !page_slug.is_empty() => Some(PublicFunnelRoute {
            funnel_slug,
            page_slug: Some(page_slug),
        }),
        _ => None,
    }
}

fn public_form_submission_route_from_path(path: &str) -> Option<PublicFormSubmissionRoute<'_>> {
    let segments = path
        .strip_prefix("/api/public/funnels/")?
        .split('/')
        .collect::<Vec<_>>();
    if segments.len() != 6
        || segments[1] != "pages"
        || segments[3] != "forms"
        || segments[5] != "submissions"
        || segments[0].is_empty()
        || segments[2].is_empty()
        || segments[4].is_empty()
    {
        return None;
    }
    Some(PublicFormSubmissionRoute {
        funnel_slug: segments[0],
        page_id: segments[2],
        form_id: segments[4],
    })
}

fn public_interaction_events_route_from_path(
    path: &str,
) -> Option<PublicInteractionEventsRoute<'_>> {
    let segments = path
        .strip_prefix("/api/public/funnels/")?
        .split('/')
        .collect::<Vec<_>>();
    if segments.len() == 2 && !segments[0].is_empty() && segments[1] == "interaction-events" {
        Some(PublicInteractionEventsRoute {
            funnel_slug: segments[0],
        })
    } else {
        None
    }
}

fn custom_host_route_from_path<'a>(
    path: &'a str,
    mapped_funnel_slug: &'a str,
) -> Option<CustomHostRequestRoute<'a>> {
    if path == "/_eez/interaction.js" {
        return Some(CustomHostRequestRoute::InteractionScript);
    }
    if path == "/robots.txt" {
        return Some(CustomHostRequestRoute::Robots);
    }
    if path == "/sitemap.xml" {
        return Some(CustomHostRequestRoute::Sitemap);
    }
    if let Some(route) = public_form_submission_route_from_path(path) {
        return (route.funnel_slug == mapped_funnel_slug)
            .then_some(CustomHostRequestRoute::FormSubmission(route));
    }
    if let Some(route) = public_interaction_events_route_from_path(path) {
        return (route.funnel_slug == mapped_funnel_slug)
            .then_some(CustomHostRequestRoute::InteractionEvents(route));
    }
    if let Some(asset_id) = path.strip_prefix("/media/") {
        if eez_storage::media::is_lower_sha256(asset_id) {
            return Some(CustomHostRequestRoute::Media(asset_id));
        }
        return None;
    }
    if path == "/" {
        return Some(CustomHostRequestRoute::Page(PublicFunnelRoute {
            funnel_slug: mapped_funnel_slug,
            page_slug: None,
        }));
    }
    if let Some(route) = public_route_from_path(path) {
        return (route.funnel_slug == mapped_funnel_slug)
            .then_some(CustomHostRequestRoute::Page(route));
    }
    let page_slug = path.strip_prefix('/')?;
    if page_slug.contains('/')
        || !valid_public_slug(page_slug)
        || reserved_custom_host_path(page_slug)
    {
        return None;
    }
    Some(CustomHostRequestRoute::Page(PublicFunnelRoute {
        funnel_slug: mapped_funnel_slug,
        page_slug: Some(page_slug),
    }))
}

fn reserved_custom_host_path(path_segment: &str) -> bool {
    matches!(
        path_segment,
        "admin"
            | "api"
            | "assets"
            | "dashboard"
            | "f"
            | "funnels"
            | "health"
            | "mcp"
            | "media"
            | "submissions"
    ) || path_segment.starts_with("__local")
}

pub(crate) fn valid_public_slug(slug: &str) -> bool {
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

async fn funnel_v2_stage_upload_response(
    env: &Env,
    request: HttpRequest,
    funnel_id: &str,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if !valid_funnel_route_id(funnel_id) {
        return funnel_route_not_found_response(&correlation_id);
    }
    let Some(stage_id) = stage_upload_token(request.headers()) else {
        let error = ApiError::from_code("authorization/token_invalid");
        log_api_error(&error, &correlation_id, error.code());
        return error_response(&error, correlation_id);
    };
    let content_types = request.headers().get_all("content-type");
    if content_types.iter().count() != 1
        || content_types
            .iter()
            .next()
            .and_then(|value| value.to_str().ok())
            != Some("application/octet-stream")
    {
        return editor_payload_error_response(&correlation_id, "validation/invalid_json");
    }
    let body = match to_bytes(
        axum::body::Body::new(request.into_body()),
        MAX_V2_STAGE_UPLOAD_BYTES,
    )
    .await
    {
        Ok(body) => body,
        Err(_) => return editor_payload_error_response(&correlation_id, "validation/invalid_json"),
    };
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return editor_payload_error_response(&correlation_id, "external/unavailable"),
    };
    match bindings
        .document_stage_repository()
        .upload(funnel_id, &stage_id, &body)
        .await
    {
        Ok(()) => {
            let mut response = Response::new(axum::body::Body::empty());
            *response.status_mut() = StatusCode::NO_CONTENT;
            response
                .headers_mut()
                .insert("cache-control", HeaderValue::from_static("no-store"));
            if let Ok(value) = HeaderValue::from_str(&correlation_id) {
                response.headers_mut().insert("x-correlation-id", value);
            }
            response
        }
        Err(error) => {
            let error = ApiError::from_code(error.code());
            log_api_error(&error, &correlation_id, error.code());
            error_response(&error, correlation_id)
        }
    }
}

fn stage_upload_token(headers: &HeaderMap<HeaderValue>) -> Option<String> {
    let mut values = headers.get_all("authorization").iter();
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    let value = value.to_str().ok()?.strip_prefix("Bearer ")?;
    (value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    .then(|| value.to_owned())
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
    route: PublicFunnelRoute<'_>,
    correlation_id: String,
    canonical_origin: Option<&str>,
) -> Response<axum::body::Body> {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return method_not_allowed_for(&correlation_id, "GET, HEAD");
    }
    let is_head = request.method() == Method::HEAD;
    if !valid_public_slug(route.funnel_slug) {
        return funnel_slug_not_found_response(&correlation_id);
    }
    let submission_channel = public_form_submission_channel_from_page_query(request.uri().query());
    let parameters = match route.page_slug {
        Some(page_slug) => {
            serde_json::json!({ "slug": route.funnel_slug, "page_slug": page_slug }).to_string()
        }
        None => serde_json::json!({ "slug": route.funnel_slug }).to_string(),
    };
    let result =
        match execute_funnel_command(env, command_ids::FUNNEL_PUBLIC_READ, &parameters, None).await
        {
            Ok(result) => result,
            Err(error) => return funnel_command_error_response(error, &correlation_id),
        };
    let Some(mut html) = result
        .get("html")
        .and_then(Value::as_str)
        .map(str::to_owned)
    else {
        let error = ApiError::from_code("internal/unexpected");
        log_api_error(&error, &correlation_id, error.code());
        return error_response(&error, correlation_id);
    };
    let configured_origin = env.var("PUBLIC_BASE_URL").ok();
    let configured_origin = configured_origin.as_ref().map(|value| value.to_string());
    if let Some(canonical_url) = crate::canonical::public_page_url(
        canonical_origin.or(configured_origin.as_deref()),
        route.funnel_slug,
        route.page_slug,
    ) {
        if let Some(canonical_html) =
            crate::canonical::insert_public_canonical(&html, &canonical_url)
        {
            html = canonical_html;
        }
    }
    let Some(funnel_id) = result.get("funnel_id").and_then(Value::as_str) else {
        let error = ApiError::from_code("internal/unexpected");
        log_api_error(&error, &correlation_id, error.code());
        return error_response(&error, correlation_id);
    };
    let Some(revision_id) = result.get("revision_id").and_then(Value::as_str) else {
        let error = ApiError::from_code("internal/unexpected");
        log_api_error(&error, &correlation_id, error.code());
        return error_response(&error, correlation_id);
    };
    let Some(schema_version) = result.get("schema_version").and_then(Value::as_i64) else {
        let error = ApiError::from_code("internal/unexpected");
        log_api_error(&error, &correlation_id, error.code());
        return error_response(&error, correlation_id);
    };
    if interaction_collection_enabled(env) {
        let Some(interaction_html) = append_interaction_script(
            &html,
            funnel_id,
            revision_id,
            route.funnel_slug,
            schema_version,
        ) else {
            let error = ApiError::from_code("internal/unexpected");
            log_api_error(&error, &correlation_id, error.code());
            return error_response(&error, correlation_id);
        };
        html = interaction_html;
    }
    let submission_key_placeholder = format!(
        "?key={}",
        eez_render::document_v2::PUBLIC_FORM_SUBMISSION_KEY_MARKER
    );
    if html.contains(&submission_key_placeholder) {
        let Some(revision_id) = result.get("revision_id").and_then(Value::as_str) else {
            let error = ApiError::from_code("internal/unexpected");
            log_api_error(&error, &correlation_id, error.code());
            return error_response(&error, correlation_id);
        };
        if !valid_funnel_route_id(revision_id) {
            let error = ApiError::from_code("internal/unexpected");
            log_api_error(&error, &correlation_id, error.code());
            return error_response(&error, correlation_id);
        }
        let submission_key = uuid::Uuid::new_v4().to_string();
        let html = bind_public_form_submission_identity(
            &html,
            &submission_key,
            revision_id,
            submission_channel,
        );
        let html_length = html.len();
        let mut response = Response::new(if is_head {
            axum::body::Body::empty()
        } else {
            axum::body::Body::from(html)
        });
        if is_head {
            if let Ok(value) = HeaderValue::from_str(&html_length.to_string()) {
                response
                    .headers_mut()
                    .insert(axum::http::header::CONTENT_LENGTH, value);
            }
        }
        response
            .headers_mut()
            .insert("cache-control", HeaderValue::from_static("no-store"));
        response.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
        if let Ok(value) = HeaderValue::from_str(&correlation_id) {
            response.headers_mut().insert("x-correlation-id", value);
        }
        return response;
    }
    let etag = public_html_etag(&html);
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
    let html_length = html.len();
    if is_head {
        if let Ok(value) = HeaderValue::from_str(&html_length.to_string()) {
            response
                .headers_mut()
                .insert(axum::http::header::CONTENT_LENGTH, value);
        }
    } else {
        *response.body_mut() = axum::body::Body::from(html);
    }
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
}

fn append_interaction_script(
    html: &str,
    funnel_id: &str,
    revision_id: &str,
    funnel_slug: &str,
    schema_version: i64,
) -> Option<String> {
    if !valid_funnel_route_id(funnel_id)
        || !valid_funnel_route_id(revision_id)
        || !valid_public_slug(funnel_slug)
        || !matches!(schema_version, 1 | 2)
    {
        return None;
    }
    let legacy_page_attribute = if schema_version == 1 {
        " data-legacy-page-id=\"legacy-entry\""
    } else {
        ""
    };
    let script = format!(
        "<script defer src=\"/_eez/interaction.js\" data-funnel-id=\"{funnel_id}\" data-revision-id=\"{revision_id}\" data-endpoint=\"/api/public/funnels/{funnel_slug}/interaction-events\"{legacy_page_attribute}></script>"
    );
    let closing_body = html.rfind("</body>")?;
    let mut output = String::with_capacity(html.len() + script.len());
    output.push_str(&html[..closing_body]);
    output.push_str(&script);
    output.push_str(&html[closing_body..]);
    Some(output)
}

fn interaction_script_response(
    request: HttpRequest,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return method_not_allowed_for(&correlation_id, "GET, HEAD");
    }
    let is_head = request.method() == Method::HEAD;
    let mut response = Response::new(if is_head {
        axum::body::Body::empty()
    } else {
        axum::body::Body::from(INTERACTION_SCRIPT)
    });
    if is_head {
        if let Ok(value) = HeaderValue::from_str(&INTERACTION_SCRIPT.len().to_string()) {
            response
                .headers_mut()
                .insert(axum::http::header::CONTENT_LENGTH, value);
        }
    }
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/javascript; charset=utf-8"),
    );
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
    );
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response.headers_mut().insert(
        "cross-origin-resource-policy",
        HeaderValue::from_static("same-origin"),
    );
    if let Ok(value) = HeaderValue::from_str(&correlation_id) {
        response.headers_mut().insert("x-correlation-id", value);
    }
    response
}

fn bind_public_form_submission_identity(
    html: &str,
    submission_key: &str,
    revision_id: &str,
    submission_channel: FormSubmissionChannel,
) -> String {
    let placeholder = format!(
        "?key={}",
        eez_render::document_v2::PUBLIC_FORM_SUBMISSION_KEY_MARKER
    );
    let bound_query = format!(
        "?key={submission_key}&revision_id={revision_id}&channel={}",
        submission_channel.as_str()
    );
    let mut output = String::with_capacity(html.len() + bound_query.len());
    let mut remaining = html;

    while let Some(form_start) = remaining.find("<form ") {
        let (before_form, form_and_rest) = remaining.split_at(form_start);
        output.push_str(before_form);
        let Some(tag_end) = form_and_rest.find('>') else {
            output.push_str(form_and_rest);
            return output;
        };
        let (form_tag, after_form_tag) = form_and_rest.split_at(tag_end + 1);
        let action_attribute = " action=\"";
        if let Some(action_attribute_start) = form_tag.find(action_attribute) {
            let action_start = action_attribute_start + action_attribute.len();
            if let Some(action_end_offset) = form_tag[action_start..].find('"') {
                let action_end = action_start + action_end_offset;
                let action = &form_tag[action_start..action_end];
                if let Some(action_prefix) = action.strip_suffix(&placeholder) {
                    output.push_str(&form_tag[..action_start]);
                    output.push_str(action_prefix);
                    output.push_str(&bound_query);
                    output.push_str(&form_tag[action_end..]);
                } else {
                    output.push_str(form_tag);
                }
            } else {
                output.push_str(form_tag);
            }
        } else {
            output.push_str(form_tag);
        }
        remaining = after_form_tag;
    }

    output.push_str(remaining);
    output
}

struct ParsedPublicFormBody {
    fields: std::collections::BTreeMap<String, String>,
    request_consent_accepted: bool,
    marketing_opt_in: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PublicFormSubmissionIdentity {
    submission_key: String,
    revision_id: String,
    submission_channel: FormSubmissionChannel,
}

fn public_form_submission_channel_from_page_query(query: Option<&str>) -> FormSubmissionChannel {
    let Some(query) = query else {
        return FormSubmissionChannel::Unattributed;
    };
    if query.len() > 2_048 {
        return FormSubmissionChannel::Other;
    }
    let mut medium = None;
    for (name, value) in url::form_urlencoded::parse(query.as_bytes()) {
        if name == "utm_medium" {
            if medium.is_some() {
                return FormSubmissionChannel::Other;
            }
            if value.len() > 64 {
                return FormSubmissionChannel::Other;
            }
            medium = Some(value.into_owned());
        }
    }
    medium
        .as_deref()
        .map(FormSubmissionChannel::from_utm_medium)
        .unwrap_or(FormSubmissionChannel::Unattributed)
}

async fn public_form_submission_response(
    env: &Env,
    request: HttpRequest,
    route: PublicFormSubmissionRoute<'_>,
    correlation_id: String,
) -> Response<axum::body::Body> {
    let prefers_html = prefers_public_form_html(request.headers());
    if request.method() != Method::POST {
        return method_not_allowed_for(&correlation_id, "POST");
    }
    if !origin::matches_request_host(request.headers(), request.uri().scheme_str()) {
        return form_submission_origin_error(&correlation_id, prefers_html);
    }
    if !valid_public_slug(route.funnel_slug) {
        if prefers_html {
            return form_submission_html_error(
                StatusCode::NOT_FOUND,
                "Form not found",
                "This form is no longer available.",
                None,
                &correlation_id,
                None,
            );
        }
        return funnel_slug_not_found_response(&correlation_id);
    }
    let page_id = match PageId::parse(route.page_id.to_owned()) {
        Ok(page_id) => page_id,
        Err(_) => {
            if prefers_html {
                return form_submission_html_error(
                    StatusCode::NOT_FOUND,
                    "Form not found",
                    "This form is no longer available.",
                    None,
                    &correlation_id,
                    None,
                );
            }
            return funnel_slug_not_found_response(&correlation_id);
        }
    };
    let form_id = match FormId::parse(route.form_id.to_owned()) {
        Ok(form_id) => form_id,
        Err(_) => {
            if prefers_html {
                return form_submission_html_error(
                    StatusCode::NOT_FOUND,
                    "Form not found",
                    "This form is no longer available.",
                    None,
                    &correlation_id,
                    None,
                );
            }
            return funnel_slug_not_found_response(&correlation_id);
        }
    };
    if !is_urlencoded_form(request.headers()) {
        return form_submission_payload_error(&correlation_id, prefers_html);
    }
    let Some(submission_identity) = public_form_submission_identity(request.uri().query()) else {
        return form_submission_payload_error(&correlation_id, prefers_html);
    };
    let body = match to_bytes(
        axum::body::Body::new(request.into_body()),
        eez_storage::MAX_PUBLIC_FORM_SUBMISSION_BYTES,
    )
    .await
    {
        Ok(body) => body,
        Err(_) => return form_submission_payload_error(&correlation_id, prefers_html),
    };
    let Some(parsed_body) = parse_public_form_body(&body) else {
        return form_submission_payload_error(&correlation_id, prefers_html);
    };

    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => {
            if prefers_html {
                return form_submission_html_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Submission unavailable",
                    "We could not confirm your request. Return to the form and try again with the same answers.",
                    None,
                    &correlation_id,
                    None,
                );
            }
            return form_submission_storage_error(&correlation_id);
        }
    };
    let repository = bindings.repository();
    let media_assets = bindings.media_asset_repository();
    let result = submit_public_form(
        &repository,
        &media_assets,
        SubmitPublicFormInput {
            funnel_slug: route.funnel_slug,
            page_id,
            form_id,
            revision_id: &submission_identity.revision_id,
            submission_key: &submission_identity.submission_key,
            submission_channel: submission_identity.submission_channel,
            fields: &parsed_body.fields,
            request_consent_accepted: parsed_body.request_consent_accepted,
            marketing_opt_in: parsed_body.marketing_opt_in,
            retention_seconds: public_form_retention_seconds(env),
            limit_per_minute: public_form_submission_limit(env),
        },
    )
    .await;
    let success_location = match result {
        Ok(SubmitPublicFormOutcome::Accepted { success_location }) => success_location,
        Ok(SubmitPublicFormOutcome::KeyReuseMismatch {
            current_page_slug,
            language,
        }) => {
            if prefers_html {
                let recovery_path =
                    public_form_recovery_path(route.funnel_slug, current_page_slug.as_deref());
                return form_submission_html_response(
                    StatusCode::CONFLICT,
                    eez_render::document_v2::render_form_error_page(
                        language,
                        eez_render::document_v2::FormErrorPageKind::KeyReuseMismatch,
                        Some(&recovery_path),
                    ),
                    &correlation_id,
                    None,
                );
            }
            return editor_payload_error_response(
                &correlation_id,
                "conflict/idempotency_payload_mismatch",
            );
        }
        Ok(SubmitPublicFormOutcome::RateLimited { recovery_html }) => {
            if prefers_html {
                if let Some(html) = recovery_html {
                    let html = bind_public_form_submission_identity(
                        &html,
                        &submission_identity.submission_key,
                        &submission_identity.revision_id,
                        submission_identity.submission_channel,
                    );
                    return form_submission_html_response(
                        StatusCode::TOO_MANY_REQUESTS,
                        html,
                        &correlation_id,
                        Some("60"),
                    );
                }
                return form_submission_html_error(
                    StatusCode::TOO_MANY_REQUESTS,
                    "Please wait before trying again",
                    "Your answers could not be accepted yet. Wait a moment, then retry the same form.",
                    None,
                    &correlation_id,
                    Some("60"),
                );
            }
            let mut response =
                editor_payload_error_response(&correlation_id, "rate_limit/exceeded");
            response
                .headers_mut()
                .insert("retry-after", HeaderValue::from_static("60"));
            response
                .headers_mut()
                .insert("cache-control", HeaderValue::from_static("no-store"));
            return response;
        }
        Err(SubmitPublicFormError::NotFound) => {
            if prefers_html {
                return form_submission_html_error(
                    StatusCode::NOT_FOUND,
                    "Form not found",
                    "This form is no longer available.",
                    None,
                    &correlation_id,
                    None,
                );
            }
            return funnel_slug_not_found_response(&correlation_id);
        }
        Err(SubmitPublicFormError::StaleRevision {
            current_page_slug,
            language,
        }) => {
            if prefers_html {
                let recovery_path =
                    public_form_recovery_path(route.funnel_slug, current_page_slug.as_deref());
                return form_submission_html_response(
                    StatusCode::CONFLICT,
                    eez_render::document_v2::render_form_error_page(
                        language,
                        eez_render::document_v2::FormErrorPageKind::StaleRevision,
                        Some(&recovery_path),
                    ),
                    &correlation_id,
                    None,
                );
            }
            return form_submission_stale_revision_error(&correlation_id);
        }
        Err(SubmitPublicFormError::InvalidPayload { recovery_html }) => {
            if prefers_html {
                if let Some(html) = recovery_html {
                    let html = bind_public_form_submission_identity(
                        &html,
                        &submission_identity.submission_key,
                        &submission_identity.revision_id,
                        submission_identity.submission_channel,
                    );
                    return form_submission_html_response(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        html,
                        &correlation_id,
                        None,
                    );
                }
                return form_submission_html_error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Check your form",
                    "Some fields need attention. Return to the form and review your answers.",
                    None,
                    &correlation_id,
                    None,
                );
            }
            return form_submission_payload_error(&correlation_id, false);
        }
        Err(SubmitPublicFormError::SerializationFailure { recovery_html }) => {
            if prefers_html {
                if let Some(html) = recovery_html {
                    let html = bind_public_form_submission_identity(
                        &html,
                        &submission_identity.submission_key,
                        &submission_identity.revision_id,
                        submission_identity.submission_channel,
                    );
                    return form_submission_html_response(
                        StatusCode::SERVICE_UNAVAILABLE,
                        html,
                        &correlation_id,
                        None,
                    );
                }
                return form_submission_html_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Submission unavailable",
                    "We could not confirm your request. You can safely try again using the same form.",
                    None,
                    &correlation_id,
                    None,
                );
            }
            return form_submission_storage_error(&correlation_id);
        }
        Err(SubmitPublicFormError::Storage {
            code,
            recovery_html,
        }) => {
            let api_error = ApiError::from_code(code);
            log_api_error(&api_error, &correlation_id, api_error.code());
            if prefers_html {
                if let Some(html) = recovery_html {
                    let html = bind_public_form_submission_identity(
                        &html,
                        &submission_identity.submission_key,
                        &submission_identity.revision_id,
                        submission_identity.submission_channel,
                    );
                    return form_submission_html_response(
                        StatusCode::SERVICE_UNAVAILABLE,
                        html,
                        &correlation_id,
                        None,
                    );
                }
                return form_submission_html_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Submission unavailable",
                    "We could not confirm your request. You can safely try again using the same form.",
                    None,
                    &correlation_id,
                    None,
                );
            }
            return error_response(&api_error, correlation_id);
        }
    };

    let Ok(location) = HeaderValue::from_str(&success_location) else {
        return form_submission_storage_error(&correlation_id);
    };
    let mut response = Response::new(axum::body::Body::empty());
    *response.status_mut() = StatusCode::SEE_OTHER;
    response.headers_mut().insert("location", location);
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    if let Ok(value) = HeaderValue::from_str(&correlation_id) {
        response.headers_mut().insert("x-correlation-id", value);
    }
    response
}

async fn public_interaction_events_response(
    env: &Env,
    request: HttpRequest,
    route: PublicInteractionEventsRoute<'_>,
    correlation_id: String,
    custom_host: Option<&CustomHostMapping>,
) -> Response<axum::body::Body> {
    if request.method() != Method::POST {
        return method_not_allowed_for(&correlation_id, "POST");
    }
    if request.uri().query().is_some() {
        return interaction_empty_response(StatusCode::BAD_REQUEST, &correlation_id);
    }
    if !interaction_request_origin_matches(&request, custom_host) {
        return interaction_empty_response(StatusCode::FORBIDDEN, &correlation_id);
    }
    if interaction_consent(request.headers()) != InteractionConsent::Granted {
        return interaction_empty_response(StatusCode::NO_CONTENT, &correlation_id);
    }
    if !valid_public_slug(route.funnel_slug) || !is_json_content_type(request.headers()) {
        return interaction_empty_response(StatusCode::BAD_REQUEST, &correlation_id);
    }
    let Some(replay_key) = interaction_replay_key(env) else {
        return interaction_empty_response(StatusCode::SERVICE_UNAVAILABLE, &correlation_id);
    };
    let Some(retention_days) = interaction_retention_days(env) else {
        return interaction_empty_response(StatusCode::SERVICE_UNAVAILABLE, &correlation_id);
    };
    let Some(limit_per_minute) = interaction_event_limit(env) else {
        return interaction_empty_response(StatusCode::SERVICE_UNAVAILABLE, &correlation_id);
    };
    let body = match to_bytes(
        axum::body::Body::new(request.into_body()),
        eez_domain::interaction_event::MAX_INTERACTION_EVENT_BYTES,
    )
    .await
    {
        Ok(body) => body,
        Err(_) => return interaction_empty_response(StatusCode::BAD_REQUEST, &correlation_id),
    };
    let payload = match serde_json::from_slice::<InteractionEventPayload>(&body) {
        Ok(payload) => payload,
        Err(_) => return interaction_empty_response(StatusCode::BAD_REQUEST, &correlation_id),
    };
    let bindings = eez_storage::Bindings::from_env(env).ok();
    let repository = bindings.as_ref().map(eez_storage::Bindings::repository);
    let campaigns = bindings
        .as_ref()
        .map(eez_storage::Bindings::campaign_registry_repository);
    let result = record_interaction(
        repository.as_ref(),
        campaigns.as_ref(),
        RecordInteractionInput {
            funnel_slug: route.funnel_slug,
            payload,
            replay_key: replay_key.as_bytes(),
            retention_days,
            limit_per_minute,
        },
    )
    .await;
    match result {
        Ok(
            RecordInteractionOutcome::Ignored
            | RecordInteractionOutcome::Recorded
            | RecordInteractionOutcome::Duplicate,
        ) => interaction_empty_response(StatusCode::NO_CONTENT, &correlation_id),
        Ok(RecordInteractionOutcome::Conflict) => {
            interaction_empty_response(StatusCode::CONFLICT, &correlation_id)
        }
        Ok(RecordInteractionOutcome::RateLimited) => {
            let mut response =
                interaction_empty_response(StatusCode::TOO_MANY_REQUESTS, &correlation_id);
            response
                .headers_mut()
                .insert("retry-after", HeaderValue::from_static("60"));
            response
        }
        Err(RecordInteractionError::InvalidEvent) => {
            interaction_empty_response(StatusCode::BAD_REQUEST, &correlation_id)
        }
        Err(RecordInteractionError::NotFound) => {
            interaction_empty_response(StatusCode::NOT_FOUND, &correlation_id)
        }
        Err(RecordInteractionError::RepositoryUnavailable) => {
            interaction_empty_response(StatusCode::SERVICE_UNAVAILABLE, &correlation_id)
        }
        Err(RecordInteractionError::Storage { code }) => {
            let api_error = ApiError::from_code(code);
            log_api_error(&api_error, &correlation_id, api_error.code());
            interaction_empty_response(StatusCode::SERVICE_UNAVAILABLE, &correlation_id)
        }
    }
}

fn interaction_request_origin_matches(
    request: &HttpRequest,
    custom_host: Option<&CustomHostMapping>,
) -> bool {
    interaction_origin_matches(request.headers(), request.uri().scheme_str(), custom_host)
}

fn interaction_origin_matches(
    headers: &HeaderMap<HeaderValue>,
    request_scheme: Option<&str>,
    custom_host: Option<&CustomHostMapping>,
) -> bool {
    let values = headers.get_all("origin");
    if values.iter().count() != 1 {
        return false;
    }
    if let Some(mapping) = custom_host {
        let Ok(origin) = values
            .iter()
            .next()
            .and_then(|value| value.to_str().ok())
            .ok_or(())
        else {
            return false;
        };
        let expected = CustomHostMappings::origin(mapping);
        origin.eq_ignore_ascii_case(&expected)
    } else {
        origin::matches_request_host(headers, request_scheme)
    }
}

fn interaction_consent(headers: &HeaderMap<HeaderValue>) -> InteractionConsent {
    let values = headers.get_all("x-eezfunnel-interaction-consent");
    if values.iter().count() != 1 {
        return InteractionConsent::Missing;
    }
    match values.iter().next().and_then(|value| value.to_str().ok()) {
        Some("granted") => InteractionConsent::Granted,
        Some("refused") => InteractionConsent::Refused,
        _ => InteractionConsent::Missing,
    }
}

fn interaction_empty_response(
    status: StatusCode,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    let mut response = Response::new(axum::body::Body::empty());
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    if let Ok(value) = HeaderValue::from_str(correlation_id) {
        response.headers_mut().insert("x-correlation-id", value);
    }
    response
}

fn is_urlencoded_form(headers: &HeaderMap<HeaderValue>) -> bool {
    let values = headers.get_all(axum::http::header::CONTENT_TYPE);
    if values.iter().count() != 1 {
        return false;
    }
    let Some(value) = values.iter().next().and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let mut parts = value.split(';');
    if !parts.next().is_some_and(|media_type| {
        media_type
            .trim()
            .eq_ignore_ascii_case("application/x-www-form-urlencoded")
    }) {
        return false;
    }
    parts.all(|parameter| parameter.trim().eq_ignore_ascii_case("charset=utf-8"))
}

fn is_json_content_type(headers: &HeaderMap<HeaderValue>) -> bool {
    let values = headers.get_all(axum::http::header::CONTENT_TYPE);
    if values.iter().count() != 1 {
        return false;
    }
    let Some(value) = values.iter().next().and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let mut parts = value.split(';');
    if !parts
        .next()
        .is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case("application/json"))
    {
        return false;
    }
    parts.all(|parameter| parameter.trim().eq_ignore_ascii_case("charset=utf-8"))
}

fn prefers_public_form_html(headers: &HeaderMap<HeaderValue>) -> bool {
    prefers_public_form_html_accept_values(
        headers
            .get_all(axum::http::header::ACCEPT)
            .iter()
            .filter_map(|value| value.to_str().ok()),
    )
}

fn prefers_public_form_html_accept_values<'a>(values: impl IntoIterator<Item = &'a str>) -> bool {
    let mut html = None;
    let mut json = None;
    let mut application_wildcard = None;
    let mut any_wildcard = None;

    for value in values {
        for range in value.split(',') {
            let mut parts = range.split(';');
            let media_type = parts.next().unwrap_or_default().trim();
            let Some(quality) = accept_range_quality(parts) else {
                continue;
            };
            let target = if media_type.eq_ignore_ascii_case("text/html") {
                &mut html
            } else if media_type.eq_ignore_ascii_case("application/json") {
                &mut json
            } else if media_type.eq_ignore_ascii_case("application/*") {
                &mut application_wildcard
            } else if media_type.eq_ignore_ascii_case("*/*") {
                &mut any_wildcard
            } else {
                continue;
            };
            merge_accept_quality(target, quality);
        }
    }

    let Some(html_quality) = html else {
        return false;
    };
    let json_quality = json.or(application_wildcard).or(any_wildcard).unwrap_or(0);
    html_quality > 0 && html_quality > json_quality
}

fn accept_range_quality<'a>(parameters: impl Iterator<Item = &'a str>) -> Option<u16> {
    let mut quality = 1000;
    let mut quality_seen = false;
    for parameter in parameters {
        let parameter = parameter.trim();
        let Some((name, value)) = parameter.split_once('=') else {
            if parameter.eq_ignore_ascii_case("q") {
                return None;
            }
            continue;
        };
        if name.trim().eq_ignore_ascii_case("q") {
            if quality_seen {
                return None;
            }
            quality = parse_accept_quality(value.trim())?;
            quality_seen = true;
        }
    }
    Some(quality)
}

fn parse_accept_quality(value: &str) -> Option<u16> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if fraction.len() > 3 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    match whole {
        "0" => {
            let mut thousandths = if fraction.is_empty() {
                0
            } else {
                fraction.parse::<u16>().ok()?
            };
            for _ in fraction.len()..3 {
                thousandths *= 10;
            }
            Some(thousandths)
        }
        "1" if fraction.bytes().all(|byte| byte == b'0') => Some(1000),
        _ => None,
    }
}

fn merge_accept_quality(target: &mut Option<u16>, quality: u16) {
    *target = Some(target.map_or(quality, |current| current.min(quality)));
}

fn form_completion_period(query: Option<&str>) -> Result<FormCompletionPeriod, ()> {
    let Some(query) = query.filter(|query| !query.is_empty()) else {
        return Err(());
    };
    let mut period = None;
    for (name, value) in url::form_urlencoded::parse(query.as_bytes()) {
        if name != "period" || period.is_some() {
            return Err(());
        }
        period = match value.as_ref() {
            "7" => Some(FormCompletionPeriod::Days7),
            "30" => Some(FormCompletionPeriod::Days30),
            "90" => Some(FormCompletionPeriod::Days90),
            _ => return Err(()),
        };
    }
    period.ok_or(())
}

fn interaction_report_period(query: Option<&str>) -> Result<InteractionReportPeriod, ()> {
    let Some(query) = query.filter(|query| !query.is_empty()) else {
        return Err(());
    };
    let mut period = None;
    for (name, value) in url::form_urlencoded::parse(query.as_bytes()) {
        if name != "period" || period.is_some() {
            return Err(());
        }
        period = match value.as_ref() {
            "7" => Some(InteractionReportPeriod::Days7),
            "30" => Some(InteractionReportPeriod::Days30),
            "90" => Some(InteractionReportPeriod::Days90),
            _ => return Err(()),
        };
    }
    period.ok_or(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PublicFormSubmissionQuery {
    cursor: Option<PublicFormSubmissionCursor>,
    status: Option<FormSubmissionStatus>,
    funnel_id: Option<String>,
}

fn public_form_submission_cursor(value: &str) -> Result<PublicFormSubmissionCursor, ()> {
    let mut parts = value.split('.');
    let submitted_at_text = parts.next().ok_or(())?;
    let submission_id = parts.next().ok_or(())?;
    let submitted_at = submitted_at_text.parse::<i64>().map_err(|_| ())?;
    let parsed_id = uuid::Uuid::parse_str(submission_id).map_err(|_| ())?;
    if submitted_at <= 0
        || submitted_at > 9_007_199_254_740_991
        || submitted_at.to_string() != submitted_at_text
        || parsed_id.to_string() != submission_id
    {
        return Err(());
    }

    let (status, funnel_id) = match parts.next() {
        None => (None, None), // Accept legacy cursors only for an unfiltered inbox.
        Some(status_filter) => {
            let status = if status_filter == "all" {
                None
            } else {
                Some(FormSubmissionStatus::from_wire_value(status_filter).ok_or(())?)
            };
            let funnel_filter = parts.next().ok_or(())?;
            let funnel_id = if funnel_filter == "all" {
                None
            } else {
                let parsed_funnel_id = uuid::Uuid::parse_str(funnel_filter).map_err(|_| ())?;
                if parsed_funnel_id.to_string() != funnel_filter {
                    return Err(());
                }
                Some(funnel_filter.to_owned())
            };
            if parts.next().is_some() {
                return Err(());
            }
            (status, funnel_id)
        }
    };

    Ok(PublicFormSubmissionCursor {
        submitted_at,
        submission_id: submission_id.to_owned(),
        status,
        funnel_id,
    })
}

fn public_form_submission_query(query: Option<&str>) -> Result<PublicFormSubmissionQuery, ()> {
    let Some(query) = query.filter(|query| !query.is_empty()) else {
        return Ok(PublicFormSubmissionQuery {
            cursor: None,
            status: None,
            funnel_id: None,
        });
    };
    let mut cursor = None;
    let mut status = None;
    let mut funnel_id = None;
    for (name, value) in url::form_urlencoded::parse(query.as_bytes()) {
        match name.as_ref() {
            "cursor" if cursor.is_none() => {
                cursor = Some(public_form_submission_cursor(&value)?);
            }
            "status" if status.is_none() => {
                status = Some(FormSubmissionStatus::from_wire_value(&value).ok_or(())?);
            }
            "funnel_id" if funnel_id.is_none() => {
                let parsed_id = uuid::Uuid::parse_str(&value).map_err(|_| ())?;
                if parsed_id.to_string() != value {
                    return Err(());
                }
                funnel_id = Some(value.into_owned());
            }
            _ => return Err(()),
        }
    }
    if cursor.as_ref().is_some_and(|cursor| {
        cursor.status != status || cursor.funnel_id.as_deref() != funnel_id.as_deref()
    }) {
        return Err(());
    }
    Ok(PublicFormSubmissionQuery {
        cursor,
        status,
        funnel_id,
    })
}

fn form_submission_status_id_from_path(path: &str) -> Option<&str> {
    let route = path.strip_prefix("/api/submissions/")?;
    let (submission_id, suffix) = route.split_once('/')?;
    (suffix == "status" && !submission_id.is_empty()).then_some(submission_id)
}

fn form_submission_export_id_from_path(path: &str) -> Option<&str> {
    let route = path.strip_prefix("/api/submissions/")?;
    let (submission_id, suffix) = route.split_once('/')?;
    (suffix == "export" && canonical_submission_id(submission_id)).then_some(submission_id)
}

fn form_submission_delete_id_from_path(path: &str) -> Option<&str> {
    let submission_id = path.strip_prefix("/api/submissions/")?;
    (!submission_id.contains('/') && canonical_submission_id(submission_id))
        .then_some(submission_id)
}

fn canonical_submission_id(value: &str) -> bool {
    uuid::Uuid::parse_str(value)
        .map(|id| id.hyphenated().to_string() == value)
        .unwrap_or(false)
}

fn submission_recovery_gated_path(path: &str) -> bool {
    path == "/api/submissions"
        || (path.starts_with("/api/submissions/") && path != "/api/submissions/recovery")
        || funnel_subroute(path, "/completions").is_some()
        || public_form_submission_route_from_path(path).is_some()
}

fn form_submission_status_command_parameters(
    submission_id: &str,
    body: &str,
) -> Result<String, ()> {
    let parsed_id = uuid::Uuid::parse_str(submission_id).map_err(|_| ())?;
    if parsed_id.to_string() != submission_id {
        return Err(());
    }
    validate_unique_json(body).map_err(|_| ())?;
    let value: Value = serde_json::from_str(body).map_err(|_| ())?;
    let object = value.as_object().ok_or(())?;
    if object.len() != 1 || object.keys().any(|key| key != "status") {
        return Err(());
    }
    let status = object.get("status").and_then(Value::as_str).ok_or(())?;
    FormSubmissionStatus::from_wire_value(status).ok_or(())?;
    serde_json::to_string(&serde_json::json!({
        "submission_id": submission_id,
        "status": status,
    }))
    .map_err(|_| ())
}

fn public_form_submission_identity(query: Option<&str>) -> Option<PublicFormSubmissionIdentity> {
    let query = query?;
    if query.len() > 1_024 {
        return None;
    }
    let pairs = url::form_urlencoded::parse(query.as_bytes()).collect::<Vec<_>>();
    if !(2..=3).contains(&pairs.len()) {
        return None;
    }
    let mut submission_key = None;
    let mut revision_id = None;
    let mut submission_channel = FormSubmissionChannel::Unattributed;
    let mut channel_seen = false;
    for (name, value) in pairs {
        match name.as_ref() {
            "key" if submission_key.is_none() => {
                let parsed = uuid::Uuid::parse_str(&value).ok()?;
                if parsed.to_string() != value {
                    return None;
                }
                submission_key = Some(value.into_owned());
            }
            "revision_id" if revision_id.is_none() && valid_funnel_route_id(&value) => {
                revision_id = Some(value.into_owned());
            }
            "channel" if !channel_seen => {
                channel_seen = true;
                submission_channel = FormSubmissionChannel::from_wire_value(&value)
                    .unwrap_or(FormSubmissionChannel::Other);
            }
            _ => return None,
        }
    }
    Some(PublicFormSubmissionIdentity {
        submission_key: submission_key?,
        revision_id: revision_id?,
        submission_channel,
    })
}

fn parse_public_form_body(body: &[u8]) -> Option<ParsedPublicFormBody> {
    let raw = std::str::from_utf8(body).ok()?;
    let mut fields = std::collections::BTreeMap::new();
    let mut request_consent_accepted = false;
    let mut marketing_opt_in = false;
    let mut request_consent_seen = false;
    let mut marketing_opt_in_seen = false;
    let mut pair_count = 0usize;

    for (key, value) in url::form_urlencoded::parse(raw.as_bytes()) {
        pair_count += 1;
        if pair_count > 52 || key.is_empty() || key.len() > 64 || value.len() > 32_768 {
            return None;
        }
        let key = key.into_owned();
        let value = value.into_owned();
        if key == "_request_consent" {
            if request_consent_seen || value != "on" {
                return None;
            }
            request_consent_seen = true;
            request_consent_accepted = true;
        } else if key == "_marketing_opt_in" {
            if marketing_opt_in_seen || value != "on" {
                return None;
            }
            marketing_opt_in_seen = true;
            marketing_opt_in = true;
        } else if fields.insert(key, value).is_some() {
            return None;
        }
    }

    Some(ParsedPublicFormBody {
        fields,
        request_consent_accepted,
        marketing_opt_in,
    })
}

fn interaction_replay_key(env: &Env) -> Option<String> {
    let key = env.secret("INTERACTION_REPLAY_KEY").ok()?.to_string();
    (key.len() >= 32).then_some(key)
}

fn interaction_retention_days(env: &Env) -> Option<u8> {
    let days = match env.var("PUBLIC_INTERACTION_RETENTION_DAYS") {
        Ok(value) => value.to_string().parse::<u8>().ok()?,
        Err(_) => DEFAULT_INTERACTION_RETENTION_DAYS,
    };
    (1..=MAX_INTERACTION_RETENTION_DAYS)
        .contains(&days)
        .then_some(days)
}

fn interaction_event_limit(env: &Env) -> Option<i32> {
    let limit = match env.var("PUBLIC_INTERACTION_EVENTS_PER_MINUTE") {
        Ok(value) => value.to_string().parse::<i32>().ok()?,
        Err(_) => 100,
    };
    (1..=10_000).contains(&limit).then_some(limit)
}

fn interaction_collection_enabled(env: &Env) -> bool {
    interaction_replay_key(env).is_some()
        && interaction_retention_days(env).is_some()
        && interaction_event_limit(env).is_some()
}

fn public_form_retention_seconds(env: &Env) -> i32 {
    env.var("PUBLIC_FORM_RETENTION_DAYS")
        .ok()
        .and_then(|value| value.to_string().parse::<i32>().ok())
        .filter(|days| (1..=3650).contains(days))
        .unwrap_or(90)
        * 86_400
}

fn public_form_submission_limit(env: &Env) -> i32 {
    env.var("PUBLIC_FORM_SUBMISSIONS_PER_MINUTE")
        .ok()
        .and_then(|value| value.to_string().parse::<i32>().ok())
        .filter(|limit| (1..=100).contains(limit))
        .unwrap_or(100)
}

fn form_submission_payload_error(
    correlation_id: &str,
    prefers_html: bool,
) -> Response<axum::body::Body> {
    if prefers_html {
        return form_submission_html_error(
            StatusCode::BAD_REQUEST,
            "We could not process this form",
            "The request could not be processed. Return to the form and try again.",
            None,
            correlation_id,
            None,
        );
    }
    let mut response = editor_payload_error_response(correlation_id, "validation/invalid_json");
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
}

fn form_submission_origin_error(
    correlation_id: &str,
    prefers_html: bool,
) -> Response<axum::body::Body> {
    if prefers_html {
        return form_submission_html_error(
            StatusCode::FORBIDDEN,
            "Form unavailable",
            "This request could not be accepted. Return to the page and try again.",
            None,
            correlation_id,
            None,
        );
    }
    let error = ApiError::from_code("request/invalid_origin");
    let mut response = error_response(&error, correlation_id.to_owned());
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
}

fn public_form_recovery_path(funnel_slug: &str, page_slug: Option<&str>) -> String {
    if !valid_public_slug(funnel_slug)
        || page_slug.is_some_and(|slug| !eez_domain::document_v2::is_valid_page_slug(slug))
    {
        return "/".to_owned();
    }
    match page_slug {
        Some(page_slug) => format!("/f/{funnel_slug}/{page_slug}"),
        None => format!("/f/{funnel_slug}"),
    }
}

fn form_submission_html_error(
    status: StatusCode,
    title: &str,
    message: &str,
    recovery_path: Option<&str>,
    correlation_id: &str,
    retry_after: Option<&str>,
) -> Response<axum::body::Body> {
    let mut html = String::from(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><meta name=\"robots\" content=\"noindex\"><title>",
    );
    html.push_str(title);
    html.push_str("</title></head><body><main><h1>");
    html.push_str(title);
    html.push_str("</h1><p>");
    html.push_str(message);
    html.push_str("</p>");
    if let Some(recovery_path) = recovery_path {
        html.push_str("<p><a href=\"");
        html.push_str(recovery_path);
        html.push_str("\">Open the current form</a></p>");
    }
    html.push_str("</main></body></html>");
    form_submission_html_response(status, html, correlation_id, retry_after)
}

fn form_submission_html_response(
    status: StatusCode,
    html: String,
    correlation_id: &str,
    retry_after: Option<&str>,
) -> Response<axum::body::Body> {
    let mut response = Response::new(axum::body::Body::from(html));
    *response.status_mut() = status;
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
        .headers_mut()
        .insert(axum::http::header::VARY, HeaderValue::from_static("Accept"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    if let Ok(value) = HeaderValue::from_str(correlation_id) {
        response.headers_mut().insert("x-correlation-id", value);
    }
    if let Some(retry_after) = retry_after {
        if let Ok(value) = HeaderValue::from_str(retry_after) {
            response.headers_mut().insert("retry-after", value);
        }
    }
    response
}

fn form_submission_stale_revision_error(correlation_id: &str) -> Response<axum::body::Body> {
    let error = ApiError::from_code("conflict/public_form_stale_revision");
    let mut response = error_response(&error, correlation_id.to_owned());
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
}

fn form_submission_storage_error(correlation_id: &str) -> Response<axum::body::Body> {
    let error = ApiError::from_code("external/unavailable");
    log_api_error(&error, correlation_id, error.code());
    let mut response = error_response(&error, correlation_id.to_owned());
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
}

fn public_html_etag(html: &str) -> String {
    format!("\"{:x}\"", Sha256::digest(html.as_bytes()))
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

async fn form_submission_list_response(
    env: &Env,
    request: HttpRequest,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if request.method() != Method::GET {
        let mut response = method_not_allowed_for(&correlation_id, "GET");
        response.headers_mut().insert(
            "cache-control",
            HeaderValue::from_static("private, no-store"),
        );
        return response;
    }
    let submission_query = match public_form_submission_query(request.uri().query()) {
        Ok(query) => query,
        Err(()) => {
            let error = ApiError::from_code("validation/invalid_cursor");
            let mut response = error_response(&error, correlation_id.clone());
            response.headers_mut().insert(
                "cache-control",
                HeaderValue::from_static("private, no-store"),
            );
            return response;
        }
    };
    let parameters = serde_json::json!({
        "cursor": submission_query.cursor,
        "status": submission_query.status,
        "funnel_id": submission_query.funnel_id,
    })
    .to_string();
    match execute_funnel_command(env, command_ids::FORM_SUBMISSION_LIST, &parameters, None).await {
        Ok(result) => {
            let mut response = json_success_response(result, StatusCode::OK, &correlation_id, None);
            response.headers_mut().insert(
                "cache-control",
                HeaderValue::from_static("private, no-store"),
            );
            response
        }
        Err(error) => {
            let error = ApiError::from_code(error.code());
            log_api_error(&error, &correlation_id, error.code());
            let mut response = error_response(&error, correlation_id);
            response.headers_mut().insert(
                "cache-control",
                HeaderValue::from_static("private, no-store"),
            );
            response
        }
    }
}

async fn form_submission_status_response(
    env: &Env,
    request: HttpRequest,
    submission_id: &str,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if request.method() != Method::POST {
        return method_not_allowed_for(&correlation_id, "POST");
    }
    if !is_json_content_type(request.headers()) {
        return error_response(
            &ApiError::from_code("validation/invalid_json"),
            correlation_id,
        );
    }
    let body = match to_bytes(axum::body::Body::new(request.into_body()), 1_024).await {
        Ok(body) => body,
        Err(_) => {
            return error_response(
                &ApiError::from_code("validation/invalid_json"),
                correlation_id,
            )
        }
    };
    let body = match std::str::from_utf8(&body) {
        Ok(body) => body,
        Err(_) => {
            return error_response(
                &ApiError::from_code("validation/invalid_json"),
                correlation_id,
            )
        }
    };
    let parameters = match form_submission_status_command_parameters(submission_id, body) {
        Ok(parameters) => parameters,
        Err(()) => {
            return error_response(
                &ApiError::from_code("validation/invalid_json"),
                correlation_id,
            )
        }
    };
    match execute_funnel_command(
        env,
        command_ids::FORM_SUBMISSION_STATUS_SET,
        &parameters,
        None,
    )
    .await
    {
        Ok(result) => json_success_response(result, StatusCode::OK, &correlation_id, None),
        Err(error) => {
            let error = ApiError::from_code(error.code());
            log_api_error(&error, &correlation_id, error.code());
            error_response(&error, correlation_id)
        }
    }
}

async fn form_submission_export_response(
    env: &Env,
    request: HttpRequest,
    submission_id: &str,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if request.method() != Method::GET {
        return private_lifecycle_response(method_not_allowed_for(&correlation_id, "GET"));
    }
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return lifecycle_unavailable_response(&correlation_id),
    };
    match eez_commands::submission_lifecycle::export_one(
        &bindings.submission_lifecycle_repository(),
        submission_id,
    )
    .await
    {
        Ok(export) => {
            let Ok(body) = serde_json::to_vec(&export) else {
                return lifecycle_unavailable_response(&correlation_id);
            };
            let mut response = Response::new(axum::body::Body::from(body));
            *response.status_mut() = StatusCode::OK;
            response.headers_mut().insert(
                axum::http::header::CONTENT_TYPE,
                HeaderValue::from_static("application/json; charset=utf-8"),
            );
            let disposition = format!("attachment; filename=\"submission-{submission_id}.json\"");
            if let Ok(value) = HeaderValue::from_str(&disposition) {
                response
                    .headers_mut()
                    .insert(axum::http::header::CONTENT_DISPOSITION, value);
            }
            if let Ok(value) = HeaderValue::from_str(&correlation_id) {
                response.headers_mut().insert("x-correlation-id", value);
            }
            private_lifecycle_response(response)
        }
        Err(error) => lifecycle_error_response(error, &correlation_id, false),
    }
}

async fn form_submission_delete_response(
    env: &Env,
    request: HttpRequest,
    submission_id: &str,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if request.method() != Method::DELETE {
        return private_lifecycle_response(method_not_allowed_for(&correlation_id, "DELETE"));
    }
    if !owner_mutation_origin_matches(&request) {
        return private_lifecycle_response(error_response(
            &ApiError::from_code("authorization/host_not_allowed"),
            correlation_id,
        ));
    }
    let Some(horizon_days) = submission_erasure_recovery_horizon_days(env) else {
        return lifecycle_unavailable_response(&correlation_id);
    };
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return lifecycle_unavailable_response(&correlation_id),
    };
    match eez_commands::submission_lifecycle::erase_one(
        &bindings.submission_lifecycle_repository(),
        submission_id,
        horizon_days,
    )
    .await
    {
        Ok(_) => {
            let mut response = Response::new(axum::body::Body::empty());
            *response.status_mut() = StatusCode::NO_CONTENT;
            if let Ok(value) = HeaderValue::from_str(&correlation_id) {
                response.headers_mut().insert("x-correlation-id", value);
            }
            private_lifecycle_response(response)
        }
        Err(error) => lifecycle_error_response(error, &correlation_id, true),
    }
}

async fn submission_recovery_response(
    env: &Env,
    request: HttpRequest,
    correlation_id: String,
) -> Response<axum::body::Body> {
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return lifecycle_unavailable_response(&correlation_id),
    };
    let repository = bindings.submission_lifecycle_repository();
    if request.method() == Method::GET {
        return match repository.recovery_status().await {
            Ok(status) => recovery_status_response(status, None, None, &correlation_id),
            Err(_) => lifecycle_unavailable_response(&correlation_id),
        };
    }
    if request.method() != Method::POST {
        return private_lifecycle_response(method_not_allowed_for(&correlation_id, "GET, POST"));
    }
    if !owner_mutation_origin_matches(&request) {
        return private_lifecycle_response(error_response(
            &ApiError::from_code("authorization/host_not_allowed"),
            correlation_id,
        ));
    }
    if !is_json_content_type(request.headers()) {
        return lifecycle_error_response(
            eez_commands::submission_lifecycle::SubmissionLifecycleError::InvalidInput,
            &correlation_id,
            false,
        );
    }
    let body = match to_bytes(axum::body::Body::new(request.into_body()), 1_024).await {
        Ok(body) => body,
        Err(_) => {
            return lifecycle_error_response(
                eez_commands::submission_lifecycle::SubmissionLifecycleError::InvalidInput,
                &correlation_id,
                false,
            )
        }
    };
    let Ok(body) = std::str::from_utf8(&body) else {
        return lifecycle_error_response(
            eez_commands::submission_lifecycle::SubmissionLifecycleError::InvalidInput,
            &correlation_id,
            false,
        );
    };
    let action = match submission_recovery_action(body) {
        Ok(action) => action,
        Err(()) => {
            return lifecycle_error_response(
                eez_commands::submission_lifecycle::SubmissionLifecycleError::InvalidInput,
                &correlation_id,
                false,
            )
        }
    };
    match action {
        SubmissionRecoveryAction::Close => {
            match eez_commands::submission_lifecycle::begin_recovery(&repository).await {
                Ok(status) => recovery_status_response(status, None, None, &correlation_id),
                Err(error) => lifecycle_error_response(error, &correlation_id, false),
            }
        }
        SubmissionRecoveryAction::Prepare { generation } => {
            match eez_commands::submission_lifecycle::prepare_recovery_step(
                &repository,
                &generation,
            )
            .await
            {
                Ok((status, attempted, pending)) => recovery_status_response(
                    status,
                    Some(attempted),
                    Some(pending),
                    &correlation_id,
                ),
                Err(error) => lifecycle_error_response(error, &correlation_id, false),
            }
        }
        SubmissionRecoveryAction::Reconcile { generation } => {
            match eez_commands::submission_lifecycle::reconcile_after_restore(
                &repository,
                &generation,
            )
            .await
            {
                Ok((status, processed)) => {
                    recovery_status_response(status, Some(processed), None, &correlation_id)
                }
                Err(error) => lifecycle_error_response(error, &correlation_id, false),
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SubmissionRecoveryAction {
    Close,
    Prepare { generation: String },
    Reconcile { generation: String },
}

fn submission_recovery_action(body: &str) -> Result<SubmissionRecoveryAction, ()> {
    validate_unique_json(body).map_err(|_| ())?;
    let value: Value = serde_json::from_str(body).map_err(|_| ())?;
    let object = value.as_object().ok_or(())?;
    let action = object.get("action").and_then(Value::as_str).ok_or(())?;
    match action {
        "close" if object.len() == 1 => Ok(SubmissionRecoveryAction::Close),
        "prepare" | "reconcile" if object.len() == 2 => {
            let generation = object.get("generation").and_then(Value::as_str).ok_or(())?;
            if !canonical_submission_id(generation) {
                return Err(());
            }
            if action == "prepare" {
                Ok(SubmissionRecoveryAction::Prepare {
                    generation: generation.to_owned(),
                })
            } else {
                Ok(SubmissionRecoveryAction::Reconcile {
                    generation: generation.to_owned(),
                })
            }
        }
        _ => Err(()),
    }
}

fn recovery_status_response(
    status: eez_storage::submission_lifecycle::RecoveryStatus,
    processed: Option<u32>,
    pending: Option<u64>,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    let mut value = serde_json::json!({
        "generation": status.generation,
        "phase": status.phase,
    });
    if let Some(processed) = processed {
        value["processed"] = serde_json::json!(processed);
    }
    if let Some(pending) = pending {
        value["pending"] = serde_json::json!(pending);
    }
    private_lifecycle_response(json_success_response(
        value,
        StatusCode::OK,
        correlation_id,
        None,
    ))
}

fn owner_mutation_origin_matches(request: &HttpRequest) -> bool {
    request.headers().get_all("origin").iter().count() == 1
        && origin::matches_request_host(request.headers(), request.uri().scheme_str())
}

fn submission_erasure_recovery_horizon_days(env: &Env) -> Option<u16> {
    let value = match env.var("SUBMISSION_ERASURE_RECOVERY_HORIZON_DAYS") {
        Ok(value) => value.to_string(),
        Err(_) => return Some(30),
    };
    let days = value.parse::<u16>().ok()?;
    (1..=3_650).contains(&days).then_some(days)
}

fn lifecycle_error_response(
    error: eez_commands::submission_lifecycle::SubmissionLifecycleError,
    correlation_id: &str,
    erase_operation: bool,
) -> Response<axum::body::Body> {
    use eez_commands::submission_lifecycle::SubmissionLifecycleError;
    match error {
        SubmissionLifecycleError::NotFound => private_lifecycle_response(error_response(
            &ApiError::from_code("not_found/route"),
            correlation_id.to_owned(),
        )),
        SubmissionLifecycleError::Unavailable => lifecycle_unavailable_response(correlation_id),
        SubmissionLifecycleError::InvalidInput => private_lifecycle_response(error_response(
            &ApiError::from_code("validation/invalid_json"),
            correlation_id.to_owned(),
        )),
        SubmissionLifecycleError::Conflict if erase_operation => {
            lifecycle_unavailable_response(correlation_id)
        }
        SubmissionLifecycleError::Conflict => private_lifecycle_response(error_response(
            &ApiError::from_code("conflict/idempotency_payload_mismatch"),
            correlation_id.to_owned(),
        )),
    }
}

fn lifecycle_unavailable_response(correlation_id: &str) -> Response<axum::body::Body> {
    private_lifecycle_response(error_response(
        &ApiError::from_code("external/unavailable"),
        correlation_id.to_owned(),
    ))
}

fn private_lifecycle_response(
    mut response: Response<axum::body::Body>,
) -> Response<axum::body::Body> {
    response.headers_mut().insert(
        "cache-control",
        HeaderValue::from_static("private, no-store"),
    );
    response
}

async fn funnel_completion_report_response(
    env: &Env,
    request: HttpRequest,
    funnel_id: &str,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if request.method() != Method::GET {
        return method_not_allowed_for(&correlation_id, "GET");
    }
    let period = match form_completion_period(request.uri().query()) {
        Ok(period) => period,
        Err(()) => {
            let error = ApiError::from_code("validation/invalid_json");
            return error_response(&error, correlation_id);
        }
    };
    let parameters = serde_json::json!({
        "funnel_id": funnel_id,
        "period_days": period.days(),
    })
    .to_string();
    match execute_funnel_command(env, command_ids::FORM_COMPLETION_REPORT, &parameters, None).await
    {
        Ok(result) => {
            let retention_days = u16::try_from(public_form_retention_seconds(env) / 86_400).ok();
            let result = attach_report_context(result, true, retention_days);
            json_success_response(result, StatusCode::OK, &correlation_id, None)
        }
        Err(error) => {
            let error = ApiError::from_code(error.code());
            log_api_error(&error, &correlation_id, error.code());
            error_response(&error, correlation_id)
        }
    }
}

async fn funnel_interaction_report_response(
    env: &Env,
    request: HttpRequest,
    funnel_id: &str,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if request.method() != Method::GET {
        return method_not_allowed_for(&correlation_id, "GET");
    }
    let period = match interaction_report_period(request.uri().query()) {
        Ok(period) => period,
        Err(()) => {
            let error = ApiError::from_code("validation/invalid_json");
            return error_response(&error, correlation_id);
        }
    };
    let parameters = serde_json::json!({
        "funnel_id": funnel_id,
        "period_days": period.days(),
    })
    .to_string();
    match execute_funnel_command(env, command_ids::INTERACTION_REPORT_READ, &parameters, None).await
    {
        Ok(result) => {
            let retention_days = interaction_retention_days(env).map(u16::from);
            let result =
                attach_report_context(result, interaction_collection_enabled(env), retention_days);
            json_success_response(result, StatusCode::OK, &correlation_id, None)
        }
        Err(error) => {
            let error = ApiError::from_code(error.code());
            log_api_error(&error, &correlation_id, error.code());
            error_response(&error, correlation_id)
        }
    }
}

fn attach_report_context(
    mut result: Value,
    collection_enabled: bool,
    effective_retention_days: Option<u16>,
) -> Value {
    if let Some(object) = result.as_object_mut() {
        object.insert(
            "report_context".to_owned(),
            serde_json::json!({
                "collection_state": if collection_enabled { "enabled" } else { "disabled" },
                "effective_retention_days": effective_retention_days,
                "historical_coverage": "unknown",
            }),
        );
    }
    result
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
            funnel_create_response(env, request, command_ids::FUNNEL_CREATE, correlation_id).await
        }
        _ => method_not_allowed_for(&correlation_id, "GET,POST"),
    }
}

async fn funnel_create_response(
    env: &Env,
    request: HttpRequest,
    command_id: &'static str,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if request.method() != Method::POST {
        return method_not_allowed_for(&correlation_id, "POST");
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
        MAX_CREATE_REQUEST_BYTES,
    )
    .await
    {
        Ok(body) => body,
        Err(_) => {
            return unreadable_create_payload_response(
                env,
                command_id,
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
                command_id,
                &idempotency_key,
                &correlation_id,
            )
            .await;
        }
    };
    match execute_funnel_command(env, command_id, parameters, Some(&idempotency_key)).await {
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

async fn unreadable_create_payload_response(
    env: &Env,
    command_id: &'static str,
    idempotency_key: &str,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    match execute_funnel_command(env, command_id, "null", Some(idempotency_key)).await {
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

fn v1_conversion_command_parameters(
    funnel_id: &str,
    body: &str,
) -> Result<String, EditorCommandParametersError> {
    validate_unique_json(body).map_err(EditorCommandParametersError::from)?;
    let mut value: Value =
        serde_json::from_str(body).map_err(|_| EditorCommandParametersError::InvalidJson)?;
    let object = value
        .as_object_mut()
        .ok_or(EditorCommandParametersError::InvalidJson)?;
    if object.keys().any(|key| key != "base_revision_id")
        || object.len() != 1
        || object
            .get("base_revision_id")
            .and_then(Value::as_str)
            .is_none_or(|revision_id| !valid_funnel_route_id(revision_id))
    {
        return Err(EditorCommandParametersError::InvalidJson);
    }
    object.insert("funnel_id".to_owned(), Value::String(funnel_id.to_owned()));
    serde_json::to_string(&value).map_err(|_| EditorCommandParametersError::InvalidJson)
}

async fn funnel_v2_conversion_response(
    env: &Env,
    request: HttpRequest,
    funnel_id: &str,
    correlation_id: String,
    commit: bool,
) -> Response<axum::body::Body> {
    if request.method() != Method::POST {
        return method_not_allowed_for(&correlation_id, "POST");
    }
    let valid_route_id = valid_funnel_route_id(funnel_id);
    if !valid_route_id {
        return funnel_route_not_found_response(&correlation_id);
    }
    let command_id = if commit {
        command_ids::FUNNEL_DOCUMENT_CONVERSION_COMMIT_V2
    } else {
        command_ids::FUNNEL_DOCUMENT_CONVERSION_PREVIEW_V2
    };
    let idempotency_key = if commit {
        match request_idempotency_key(request.headers()) {
            Ok(key) => Some(key),
            Err(code) => {
                let error = ApiError::from_code(code);
                log_api_error(&error, &correlation_id, error.code());
                return error_response(&error, correlation_id);
            }
        }
    } else {
        None
    };
    let body = match to_bytes(
        axum::body::Body::new(request.into_body()),
        MAX_EDITOR_REQUEST_BYTES,
    )
    .await
    {
        Ok(body) => body,
        Err(_) if commit => {
            return unreadable_mutation_payload_response(
                env,
                command_id,
                idempotency_key.as_deref().unwrap_or_default(),
                &correlation_id,
                "validation/invalid_json",
                valid_route_id,
            )
            .await
        }
        Err(_) => return editor_payload_error_response(&correlation_id, "validation/invalid_json"),
    };
    let body = match std::str::from_utf8(&body) {
        Ok(body) => body,
        Err(_) if commit => {
            return unreadable_mutation_payload_response(
                env,
                command_id,
                idempotency_key.as_deref().unwrap_or_default(),
                &correlation_id,
                "validation/invalid_json",
                valid_route_id,
            )
            .await
        }
        Err(_) => return editor_payload_error_response(&correlation_id, "validation/invalid_json"),
    };
    let parameters = match v1_conversion_command_parameters(funnel_id, body) {
        Ok(parameters) => parameters,
        Err(_) if commit => {
            return unreadable_mutation_payload_response(
                env,
                command_id,
                idempotency_key.as_deref().unwrap_or_default(),
                &correlation_id,
                "validation/invalid_json",
                valid_route_id,
            )
            .await
        }
        Err(_) => return editor_payload_error_response(&correlation_id, "validation/invalid_json"),
    };
    match execute_funnel_command(env, command_id, &parameters, idempotency_key.as_deref()).await {
        Ok(result) => {
            let status = if commit {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            };
            let mut response = json_success_response(result, status, &correlation_id, None);
            response
                .headers_mut()
                .insert("cache-control", HeaderValue::from_static("no-store"));
            response
        }
        Err(error) => funnel_command_error_response(error, &correlation_id),
    }
}

async fn funnel_document_response(
    env: &Env,
    request: HttpRequest,
    funnel_id: &str,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if request.method() != Method::GET {
        return method_not_allowed_for(&correlation_id, "GET");
    }
    if !valid_funnel_route_id(funnel_id) {
        return funnel_route_not_found_response(&correlation_id);
    }
    let revision_id = match v2_document_revision_query(request.uri().query()) {
        Ok(revision_id) => revision_id,
        Err(()) => {
            return editor_payload_error_response(&correlation_id, "validation/invalid_json")
        }
    };
    let parameters = serde_json::json!({
        "funnel_id": funnel_id,
        "revision_id": revision_id,
    })
    .to_string();
    let result =
        match execute_funnel_command(env, command_ids::FUNNEL_DOCUMENT_READ_V2, &parameters, None)
            .await
        {
            Ok(result) => result,
            Err(error) => return funnel_command_error_response(error, &correlation_id),
        };
    let (Some(document_json), Some(revision_id)) = (
        result.get("document_json").and_then(Value::as_str),
        result
            .get("document_revision_id")
            .and_then(Value::as_str)
            .or_else(|| result.get("base_revision_id").and_then(Value::as_str)),
    ) else {
        let error = ApiError::from_code("internal/unexpected");
        log_api_error(&error, &correlation_id, error.code());
        return error_response(&error, correlation_id);
    };
    let mut response = Response::new(axum::body::Body::from(document_json.to_owned()));
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    if let Ok(value) = HeaderValue::from_str(revision_id) {
        response.headers_mut().insert("x-funnel-revision-id", value);
    } else {
        let error = ApiError::from_code("internal/unexpected");
        log_api_error(&error, &correlation_id, error.code());
        return error_response(&error, correlation_id);
    }
    if let Ok(value) = HeaderValue::from_str(&correlation_id) {
        response.headers_mut().insert("x-correlation-id", value);
    }
    response
}

async fn funnel_v2_preview_response(
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
        MAX_V2_PREVIEW_ENVELOPE_BYTES,
    )
    .await
    {
        Ok(body) => body,
        Err(_) => return editor_payload_error_response(&correlation_id, "validation/invalid_json"),
    };
    let document_json = match std::str::from_utf8(&body) {
        Ok(document_json) => document_json,
        Err(_) => return editor_payload_error_response(&correlation_id, "validation/invalid_json"),
    };
    let parameters = match v2_preview_command_parameters(funnel_id, document_json) {
        Ok(parameters) => parameters,
        Err(_) => return editor_payload_error_response(&correlation_id, "validation/invalid_json"),
    };
    match execute_funnel_command(
        env,
        command_ids::FUNNEL_DOCUMENT_PREVIEW_V2,
        &parameters,
        None,
    )
    .await
    {
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

async fn funnel_v2_edit_response(
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
        MAX_V2_PREVIEW_ENVELOPE_BYTES,
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
    let parameters = match v2_edit_command_parameters(funnel_id, body) {
        Ok(parameters) => parameters,
        Err(_) => return editor_payload_error_response(&correlation_id, "validation/invalid_json"),
    };
    match execute_funnel_command(env, command_ids::FUNNEL_DOCUMENT_EDIT_V2, &parameters, None).await
    {
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

async fn funnel_v2_stage_mint_response(
    env: &Env,
    request: HttpRequest,
    funnel_id: &str,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if request.method() != Method::POST {
        if request.method() == Method::PUT {
            return funnel_v2_stage_upload_response(env, request, funnel_id, correlation_id).await;
        }
        return method_not_allowed_for(&correlation_id, "POST, PUT");
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
        MAX_V2_STAGE_REQUEST_BYTES,
    )
    .await
    {
        Ok(body) => body,
        Err(_) => {
            return unreadable_mutation_payload_response(
                env,
                command_ids::FUNNEL_DOCUMENT_STAGE_V2,
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
                command_ids::FUNNEL_DOCUMENT_STAGE_V2,
                &idempotency_key,
                &correlation_id,
                "validation/invalid_json",
                valid_route_id,
            )
            .await
        }
    };
    let parameters = match v2_stage_command_parameters(funnel_id, body) {
        Ok(parameters) => parameters,
        Err(_) => {
            return unreadable_mutation_payload_response(
                env,
                command_ids::FUNNEL_DOCUMENT_STAGE_V2,
                &idempotency_key,
                &correlation_id,
                "validation/invalid_json",
                valid_route_id,
            )
            .await
        }
    };
    match execute_funnel_command(
        env,
        command_ids::FUNNEL_DOCUMENT_STAGE_V2,
        &parameters,
        Some(&idempotency_key),
    )
    .await
    {
        Ok(result) if valid_route_id => {
            json_success_response(result, StatusCode::CREATED, &correlation_id, None)
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

fn v2_preview_command_parameters(
    funnel_id: &str,
    body: &str,
) -> Result<String, EditorCommandParametersError> {
    validate_unique_json(body).map_err(EditorCommandParametersError::from)?;
    let mut value: Value =
        serde_json::from_str(body).map_err(|_| EditorCommandParametersError::InvalidJson)?;
    let object = value
        .as_object_mut()
        .ok_or(EditorCommandParametersError::InvalidJson)?;
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "base_revision_id" | "page_id" | "document_json"
        )
    }) || ["base_revision_id", "page_id", "document_json"]
        .iter()
        .any(|key| !object.contains_key(*key))
        || object
            .get("document_json")
            .and_then(Value::as_str)
            .is_none_or(|document| document.len() > MAX_V2_DOCUMENT_BYTES)
    {
        return Err(EditorCommandParametersError::InvalidJson);
    }
    object.insert("funnel_id".to_owned(), Value::String(funnel_id.to_owned()));
    serde_json::to_string(&value).map_err(|_| EditorCommandParametersError::InvalidJson)
}

fn v2_edit_command_parameters(
    funnel_id: &str,
    body: &str,
) -> Result<String, EditorCommandParametersError> {
    validate_unique_json(body).map_err(EditorCommandParametersError::from)?;
    let mut value: Value =
        serde_json::from_str(body).map_err(|_| EditorCommandParametersError::InvalidJson)?;
    let object = value
        .as_object_mut()
        .ok_or(EditorCommandParametersError::InvalidJson)?;
    let operation_id = object
        .get("operation_id")
        .and_then(Value::as_str)
        .ok_or(EditorCommandParametersError::InvalidJson)?;
    let is_valid_operation_id = operation_id.len() == 32
        && operation_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "base_revision_id" | "page_id" | "document_json" | "operation_id" | "edit"
        )
    }) || [
        "base_revision_id",
        "page_id",
        "document_json",
        "operation_id",
        "edit",
    ]
    .iter()
    .any(|key| !object.contains_key(*key))
        || object
            .get("base_revision_id")
            .and_then(Value::as_str)
            .is_none_or(|revision_id| !valid_funnel_route_id(revision_id))
        || object
            .get("page_id")
            .and_then(Value::as_str)
            .is_none_or(|page_id| !valid_funnel_route_id(page_id))
        || object
            .get("document_json")
            .and_then(Value::as_str)
            .is_none_or(|document| document.len() > MAX_V2_DOCUMENT_BYTES)
        || !is_valid_operation_id
        || !object.get("edit").is_some_and(Value::is_object)
    {
        return Err(EditorCommandParametersError::InvalidJson);
    }
    object.insert("funnel_id".to_owned(), Value::String(funnel_id.to_owned()));
    serde_json::to_string(&value).map_err(|_| EditorCommandParametersError::InvalidJson)
}

fn v2_stage_command_parameters(
    funnel_id: &str,
    body: &str,
) -> Result<String, EditorCommandParametersError> {
    validate_unique_json(body).map_err(EditorCommandParametersError::from)?;
    let mut value: Value =
        serde_json::from_str(body).map_err(|_| EditorCommandParametersError::InvalidJson)?;
    let object = value
        .as_object_mut()
        .ok_or(EditorCommandParametersError::InvalidJson)?;
    let expected_length = object
        .get("expected_length")
        .and_then(Value::as_u64)
        .ok_or(EditorCommandParametersError::InvalidJson)?;
    let expected_sha256 = object
        .get("expected_sha256")
        .and_then(Value::as_str)
        .ok_or(EditorCommandParametersError::InvalidJson)?;
    let digest_is_valid = expected_sha256.len() == 64
        && expected_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "base_revision_id" | "expected_length" | "expected_sha256"
        )
    }) || ["base_revision_id", "expected_length", "expected_sha256"]
        .iter()
        .any(|key| !object.contains_key(*key))
        || object
            .get("base_revision_id")
            .and_then(Value::as_str)
            .is_none_or(|revision_id| !valid_funnel_route_id(revision_id))
        || !(1..=MAX_V2_DOCUMENT_BYTES as u64).contains(&expected_length)
        || !digest_is_valid
    {
        return Err(EditorCommandParametersError::InvalidJson);
    }
    object.insert("funnel_id".to_owned(), Value::String(funnel_id.to_owned()));
    serde_json::to_string(&value).map_err(|_| EditorCommandParametersError::InvalidJson)
}

fn v2_save_command_parameters(
    funnel_id: &str,
    body: &str,
) -> Result<String, EditorCommandParametersError> {
    validate_unique_json(body).map_err(EditorCommandParametersError::from)?;
    let mut value: Value =
        serde_json::from_str(body).map_err(|_| EditorCommandParametersError::InvalidJson)?;
    let object = value
        .as_object_mut()
        .ok_or(EditorCommandParametersError::InvalidJson)?;
    let is_lower_hex = |value: &str| {
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    };
    let expected_length = object
        .get("expected_length")
        .and_then(Value::as_u64)
        .ok_or(EditorCommandParametersError::InvalidJson)?;
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "base_revision_id" | "stage_id" | "expected_length" | "expected_sha256"
        )
    }) || [
        "base_revision_id",
        "stage_id",
        "expected_length",
        "expected_sha256",
    ]
    .iter()
    .any(|key| !object.contains_key(*key))
        || object
            .get("base_revision_id")
            .and_then(Value::as_str)
            .is_none_or(|revision_id| !valid_funnel_route_id(revision_id))
        || object
            .get("stage_id")
            .and_then(Value::as_str)
            .is_none_or(|stage_id| !is_lower_hex(stage_id))
        || object
            .get("expected_sha256")
            .and_then(Value::as_str)
            .is_none_or(|digest| !is_lower_hex(digest))
        || !(1..=MAX_V2_DOCUMENT_BYTES as u64).contains(&expected_length)
    {
        return Err(EditorCommandParametersError::InvalidJson);
    }
    object.insert("funnel_id".to_owned(), Value::String(funnel_id.to_owned()));
    object.insert("document_version".to_owned(), Value::from(2));
    object.insert("schema_version".to_owned(), Value::from(2));
    object.insert("manifest_version".to_owned(), Value::from(2));
    serde_json::to_string(&value).map_err(|_| EditorCommandParametersError::InvalidJson)
}

async fn funnel_v2_save_response(
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
        return match execute_funnel_command(
            env,
            command_ids::FUNNEL_REVISION_HISTORY_READ_V2,
            &parameters,
            None,
        )
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
    if request.method() != Method::POST {
        return method_not_allowed_for(&correlation_id, "GET,POST");
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
        MAX_V2_STAGE_REQUEST_BYTES,
    )
    .await
    {
        Ok(body) => body,
        Err(_) => {
            return unreadable_mutation_payload_response(
                env,
                command_ids::FUNNEL_REVISION_SAVE_V2,
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
                command_ids::FUNNEL_REVISION_SAVE_V2,
                &idempotency_key,
                &correlation_id,
                "validation/invalid_json",
                valid_route_id,
            )
            .await
        }
    };
    let parameters = match v2_save_command_parameters(funnel_id, body) {
        Ok(parameters) => parameters,
        Err(_) => {
            return unreadable_mutation_payload_response(
                env,
                command_ids::FUNNEL_REVISION_SAVE_V2,
                &idempotency_key,
                &correlation_id,
                "validation/invalid_json",
                valid_route_id,
            )
            .await
        }
    };
    match execute_funnel_command(
        env,
        command_ids::FUNNEL_REVISION_SAVE_V2,
        &parameters,
        Some(&idempotency_key),
    )
    .await
    {
        Ok(result) if valid_route_id => {
            let mut response =
                json_success_response(result, StatusCode::CREATED, &correlation_id, None);
            response
                .headers_mut()
                .insert("cache-control", HeaderValue::from_static("no-store"));
            response
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
    let document_stages = bindings.document_stage_repository();
    // Stage control is initialized by stage endpoints; V2 creation and its receipt replays do not
    // depend on it, so a temporary R2 sequence-control outage must not block funnel creation.
    let media_assets = bindings.media_asset_repository();
    Session::with_document_stages_and_media_assets(&repository, &document_stages, &media_assets)
        .execute(actor, command_id, parameters, idempotency_key)
        .await
        .map_err(|error| ApiError::from_code(error.code()))
}

async fn local_stage_control_bootstrap(
    env: &Env,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    if env
        .var("ENVIRONMENT")
        .map(|value| value.to_string())
        .unwrap_or_default()
        != "development"
    {
        return funnel_route_not_found_response(correlation_id);
    }
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return editor_payload_error_response(correlation_id, "external/unavailable"),
    };
    match bindings
        .document_stage_repository()
        .ensure_sequence_control()
        .await
    {
        Ok(()) => {
            let mut response = Response::new(axum::body::Body::empty());
            *response.status_mut() = StatusCode::NO_CONTENT;
            response
                .headers_mut()
                .insert("cache-control", HeaderValue::from_static("no-store"));
            if let Ok(value) = HeaderValue::from_str(correlation_id) {
                response.headers_mut().insert("x-correlation-id", value);
            }
            response
        }
        Err(error) => {
            let error = ApiError::from_code(error.code());
            log_api_error(&error, correlation_id, error.code());
            error_response(&error, correlation_id.to_owned())
        }
    }
}

async fn v2_stage_capacity_response(
    env: &Env,
    request: HttpRequest,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if request.method() != Method::GET {
        return method_not_allowed_for(&correlation_id, "GET");
    }
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return editor_payload_error_response(&correlation_id, "external/unavailable"),
    };
    match bindings
        .document_stage_repository()
        .capacity_snapshot()
        .await
    {
        Ok(capacity) => {
            let mut response = json_success_response(
                serde_json::json!({
                    "orphan_byte_budget": capacity.orphan_byte_budget,
                    "orphan_bytes_held": capacity.orphan_bytes_held,
                    "orphan_key_budget": capacity.orphan_key_budget,
                    "orphan_keys_held": capacity.orphan_keys_held,
                    "worst_case_orphan_bytes": capacity.worst_case_orphan_bytes,
                    "worst_case_orphan_keys": capacity.worst_case_orphan_keys,
                }),
                StatusCode::OK,
                &correlation_id,
                None,
            );
            response
                .headers_mut()
                .insert("cache-control", HeaderValue::from_static("no-store"));
            response
        }
        Err(error) => {
            funnel_command_error_response(ApiError::from_code(error.code()), &correlation_id)
        }
    }
}

async fn local_stage_sweep(env: &Env, correlation_id: &str) -> Response<axum::body::Body> {
    if env
        .var("ENVIRONMENT")
        .map(|value| value.to_string())
        .unwrap_or_default()
        != "development"
    {
        return funnel_route_not_found_response(correlation_id);
    }
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return editor_payload_error_response(correlation_id, "external/unavailable"),
    };
    match bindings
        .document_stage_repository()
        .cleanup_one_page()
        .await
    {
        Ok(progress) => json_success_response(
            serde_json::json!({
                "listed": progress.listed,
                "deleted": progress.deleted,
                "pass_completed": progress.pass_completed,
            }),
            StatusCode::OK,
            correlation_id,
            None,
        ),
        Err(error) => {
            let error = ApiError::from_code(error.code());
            log_api_error(&error, correlation_id, error.code());
            error_response(&error, correlation_id.to_owned())
        }
    }
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

fn v2_document_revision_query(query: Option<&str>) -> Result<Option<&str>, ()> {
    let Some(query) = query else {
        return Ok(None);
    };
    let mut parts = query.split('&');
    let Some(part) = parts.next() else {
        return Err(());
    };
    if parts.next().is_some() {
        return Err(());
    }
    let Some((key, value)) = part.split_once('=') else {
        return Err(());
    };
    if key != "revision_id" || !valid_funnel_route_id(value) {
        return Err(());
    }
    Ok(Some(value))
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
        append_interaction_script, attach_report_context, bind_public_form_submission_identity,
        correlation_id, custom_host_route_from_path, editor_command_parameters,
        form_completion_period, form_submission_delete_id_from_path,
        form_submission_export_id_from_path, form_submission_status_command_parameters,
        form_submission_status_id_from_path, funnel_subroute, interaction_consent,
        interaction_origin_matches, interaction_report_period, is_json_content_type,
        normalize_correlation_id, parse_public_form_body, prefers_public_form_html_accept_values,
        public_form_submission_channel_from_page_query, public_form_submission_identity,
        public_form_submission_query, public_form_submission_route_from_path, public_html_etag,
        public_interaction_events_route_from_path, public_route_from_path,
        publication_command_parameters, published_document_references_asset, request_if_none_match,
        sitemap_revisions, stage_upload_token, submission_recovery_action,
        submission_recovery_gated_path, v1_conversion_command_parameters,
        v2_document_revision_query, v2_edit_command_parameters, v2_preview_command_parameters,
        v2_save_command_parameters, v2_stage_command_parameters, valid_public_slug,
        CustomHostRequestRoute, EditorCommandParametersError, InteractionReportPeriod,
        PublicFormSubmissionCursor, PublicFormSubmissionIdentity, PublicInteractionEventsRoute,
    };
    use axum::http::{HeaderMap, HeaderValue};
    use eez_domain::interaction_event::InteractionConsent;
    use eez_domain::{FormCompletionPeriod, FormSubmissionChannel, FormSubmissionStatus};
    use eez_storage::repository::{PublishedRevisionRecord, RevisionRecord};
    use serde_json::Value;

    #[test]
    fn report_context_explains_disabled_collection_without_rewriting_aggregates() {
        let result = attach_report_context(
            serde_json::json!({"total_events": 4, "as_of": "2026-10-17T12:00:00Z"}),
            false,
            None,
        );
        assert_eq!(result["total_events"], 4);
        assert_eq!(result["as_of"], "2026-10-17T12:00:00Z");
        assert_eq!(
            result["report_context"],
            serde_json::json!({
                "collection_state": "disabled",
                "effective_retention_days": null,
                "historical_coverage": "unknown",
            })
        );
    }

    #[test]
    fn deterministic_request_json_fuzzes_ten_thousand_cases_without_panicking() {
        let inputs = super::json::tests::deterministic_json_fuzz_inputs();
        assert_eq!(inputs.len(), 10_000);

        for (case_number, input) in inputs.iter().enumerate() {
            let result = std::panic::catch_unwind(|| {
                let _ = editor_command_parameters("fuzz-funnel", input);
                let _ = v1_conversion_command_parameters("fuzz-funnel", input);
                let _ = publication_command_parameters("fuzz-funnel", "funnel.publish", input);
                let _ = public_form_submission_query(Some(input));
                let _ = form_submission_status_command_parameters(
                    "11111111-1111-4111-8111-111111111111",
                    input,
                );
                let _ = submission_recovery_action(input);
            });
            assert!(
                result.is_ok(),
                "API JSON adapter panicked for case {case_number}: {input:?}"
            );
        }
    }

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

        assert_eq!(
            public_route_from_path("/f/quick-offer"),
            Some(super::PublicFunnelRoute {
                funnel_slug: "quick-offer",
                page_slug: None,
            })
        );
        assert_eq!(
            public_route_from_path("/f/quick-offer/checkout"),
            Some(super::PublicFunnelRoute {
                funnel_slug: "quick-offer",
                page_slug: Some("checkout"),
            })
        );
        assert_eq!(public_route_from_path("/f/quick-offer/extra/depth"), None);
        assert_eq!(public_route_from_path("/f/quick-offer/"), None);
        assert_eq!(public_route_from_path("/f//checkout"), None);
        assert_eq!(public_route_from_path("/f/quick-offer/checkout/"), None);
        assert_ne!(
            public_html_etag("<main>entry</main>"),
            public_html_etag("<main>checkout</main>")
        );
        assert!(valid_public_slug("quick-offer-123"));
        assert!(!valid_public_slug("../private"));
        assert!(!valid_public_slug("quick_offer"));
    }

    #[test]
    fn interaction_routes_and_script_injection_are_exact_and_closed() {
        assert_eq!(
            public_interaction_events_route_from_path(
                "/api/public/funnels/lead-gen/interaction-events"
            ),
            Some(PublicInteractionEventsRoute {
                funnel_slug: "lead-gen",
            })
        );
        for path in [
            "/api/public/funnels/lead-gen/interaction-events/extra",
            "/api/public/funnels//interaction-events",
            "/api/public/funnels/lead-gen/events",
        ] {
            assert_eq!(
                public_interaction_events_route_from_path(path),
                None,
                "{path}"
            );
        }
        assert_eq!(
            custom_host_route_from_path(
                "/api/public/funnels/lead-gen/interaction-events",
                "lead-gen"
            ),
            Some(CustomHostRequestRoute::InteractionEvents(
                PublicInteractionEventsRoute {
                    funnel_slug: "lead-gen",
                }
            ))
        );
        assert_eq!(
            custom_host_route_from_path(
                "/api/public/funnels/another/interaction-events",
                "lead-gen"
            ),
            None
        );
        assert_eq!(
            custom_host_route_from_path("/_eez/interaction.js", "lead-gen"),
            Some(CustomHostRequestRoute::InteractionScript)
        );
        assert_eq!(
            custom_host_route_from_path("/api/funnels", "lead-gen"),
            None
        );
        assert_eq!(
            custom_host_route_from_path(
                "/api/funnels/funnel-a/interaction-report?period=7",
                "lead-gen"
            ),
            None
        );

        let html = "<!doctype html><html><body><main id=\"funnel\"></main></body></html>";
        let injected = append_interaction_script(html, "funnel-01", "revision-a", "lead-gen", 2)
            .expect("valid published page metadata");
        assert!(injected.contains("src=\"/_eez/interaction.js\""));
        assert!(
            injected.contains("data-endpoint=\"/api/public/funnels/lead-gen/interaction-events\"")
        );
        assert!(!injected.contains("data-legacy-page-id"));
        let legacy = append_interaction_script(html, "funnel-01", "revision-a", "lead-gen", 1)
            .expect("valid V1 published page metadata");
        assert!(legacy.contains("data-legacy-page-id=\"legacy-entry\""));
        assert!(
            append_interaction_script(html, "../funnel", "revision-a", "lead-gen", 2).is_none()
        );
        assert!(append_interaction_script(
            "<html><body>",
            "funnel-01",
            "revision-a",
            "lead-gen",
            2
        )
        .is_none());
    }

    #[test]
    fn interaction_consent_requires_one_exact_grant_header() {
        let mut headers = HeaderMap::new();
        assert_eq!(interaction_consent(&headers), InteractionConsent::Missing);
        headers.insert(
            "x-eezfunnel-interaction-consent",
            HeaderValue::from_static("granted"),
        );
        assert_eq!(interaction_consent(&headers), InteractionConsent::Granted);
        headers.insert(
            "x-eezfunnel-interaction-consent",
            HeaderValue::from_static("refused"),
        );
        assert_eq!(interaction_consent(&headers), InteractionConsent::Refused);
        headers.insert(
            "x-eezfunnel-interaction-consent",
            HeaderValue::from_static("Granted"),
        );
        assert_eq!(interaction_consent(&headers), InteractionConsent::Missing);
        headers.insert(
            "x-eezfunnel-interaction-consent",
            HeaderValue::from_static("granted"),
        );
        headers.append(
            "x-eezfunnel-interaction-consent",
            HeaderValue::from_static("refused"),
        );
        assert_eq!(interaction_consent(&headers), InteractionConsent::Missing);
    }

    #[test]
    fn interaction_origins_match_the_platform_or_exact_custom_host() {
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("localhost:8787"));
        headers.insert("origin", HeaderValue::from_static("http://localhost:8787"));
        assert!(interaction_origin_matches(&headers, Some("http"), None));
        headers.insert(
            "origin",
            HeaderValue::from_static("http://attacker.example"),
        );
        assert!(!interaction_origin_matches(&headers, Some("http"), None));
        headers.remove("origin");
        assert!(!interaction_origin_matches(&headers, Some("http"), None));

        let platform = super::HostAllowlist::parse("localhost:8787");
        let custom_hosts =
            super::CustomHostMappings::parse("funnel.example=lead-gen", &platform, None)
                .expect("valid test mapping");
        let mapping = custom_hosts
            .mapping_for_funnel("lead-gen")
            .expect("mapped custom host");
        headers.insert("origin", HeaderValue::from_static("https://funnel.example"));
        assert!(interaction_origin_matches(
            &headers,
            Some("https"),
            Some(&mapping)
        ));
        headers.insert(
            "origin",
            HeaderValue::from_static("https://attacker.example"),
        );
        assert!(!interaction_origin_matches(
            &headers,
            Some("https"),
            Some(&mapping)
        ));
    }

    #[test]
    fn custom_hosts_only_resolve_the_mapped_funnels_public_routes() {
        assert_eq!(
            custom_host_route_from_path("/", "lead-gen"),
            Some(CustomHostRequestRoute::Page(super::PublicFunnelRoute {
                funnel_slug: "lead-gen",
                page_slug: None,
            }))
        );
        assert_eq!(
            custom_host_route_from_path("/checkout", "lead-gen"),
            Some(CustomHostRequestRoute::Page(super::PublicFunnelRoute {
                funnel_slug: "lead-gen",
                page_slug: Some("checkout"),
            }))
        );
        assert_eq!(
            custom_host_route_from_path("/f/lead-gen/checkout", "lead-gen"),
            Some(CustomHostRequestRoute::Page(super::PublicFunnelRoute {
                funnel_slug: "lead-gen",
                page_slug: Some("checkout"),
            }))
        );
        assert_eq!(
            custom_host_route_from_path("/f/another-funnel", "lead-gen"),
            None
        );
        assert_eq!(
            custom_host_route_from_path(
                "/api/public/funnels/lead-gen/pages/page-home/forms/form-contact/submissions",
                "lead-gen"
            ),
            Some(CustomHostRequestRoute::FormSubmission(
                super::PublicFormSubmissionRoute {
                    funnel_slug: "lead-gen",
                    page_id: "page-home",
                    form_id: "form-contact",
                }
            ))
        );
        let asset_id = "a".repeat(64);
        let media_path = format!("/media/{asset_id}");
        assert_eq!(
            custom_host_route_from_path(&media_path, "lead-gen"),
            Some(CustomHostRequestRoute::Media(&asset_id))
        );
        assert_eq!(
            custom_host_route_from_path("/media/aaaaaaaaaaaaaaaa", "lead-gen"),
            None
        );
        assert_eq!(
            custom_host_route_from_path("/robots.txt", "lead-gen"),
            Some(CustomHostRequestRoute::Robots)
        );
        assert_eq!(
            custom_host_route_from_path("/sitemap.xml", "lead-gen"),
            Some(CustomHostRequestRoute::Sitemap)
        );
        for blocked in [
            "/api/funnels",
            "/api/funnels/funnel-a/interaction-report",
            "/api/media",
            "/api/public/funnels/another-funnel/pages/page-home/forms/form-contact/submissions",
            "/mcp",
            "/admin",
            "/dashboard",
            "/health",
            "/assets/index.js",
            "/__local/probe",
            "/unknown/nested/path",
            "/f/lead-gen/",
        ] {
            assert_eq!(
                custom_host_route_from_path(blocked, "lead-gen"),
                None,
                "{blocked}"
            );
        }
    }

    #[test]
    fn sitemap_scope_follows_custom_host_mappings_and_published_rows() {
        let hosts = super::CustomHostMappings::parse(
            "funnel.example=lead-gen",
            &super::HostAllowlist::parse("eezfunnel.workers.dev"),
            Some("eezfunnel.workers.dev"),
        )
        .expect("valid mapping");
        let mapping = hosts.mapping_for_funnel("lead-gen").expect("mapped funnel");
        let published_revisions = vec![
            PublishedRevisionRecord {
                slug: "lead-gen".to_owned(),
                revision: RevisionRecord {
                    id: "revision-lead".to_owned(),
                    funnel_id: "funnel-lead".to_owned(),
                    schema_version: 1,
                    data_json: "{}".to_owned(),
                    render_manifest_json: String::new(),
                    created_at: String::new(),
                },
            },
            PublishedRevisionRecord {
                slug: "other".to_owned(),
                revision: RevisionRecord {
                    id: "revision-other".to_owned(),
                    funnel_id: "funnel-other".to_owned(),
                    schema_version: 1,
                    data_json: "{}".to_owned(),
                    render_manifest_json: String::new(),
                    created_at: String::new(),
                },
            },
        ];

        let custom_sitemap_rows =
            sitemap_revisions(published_revisions.clone(), &hosts, Some(&mapping));
        assert_eq!(
            custom_sitemap_rows
                .iter()
                .map(|published| published.slug.as_str())
                .collect::<Vec<_>>(),
            ["lead-gen"]
        );
        let custom_sitemap = super::sitemap::build_sitemap(
            &url::Url::parse("https://funnel.example").expect("valid origin"),
            &custom_sitemap_rows,
        )
        .expect("valid custom-host sitemap");
        assert!(custom_sitemap.contains("https://funnel.example/f/lead-gen"));
        assert!(!custom_sitemap.contains("/f/other"));

        let platform_sitemap_rows = sitemap_revisions(published_revisions, &hosts, None);
        assert_eq!(
            platform_sitemap_rows
                .iter()
                .map(|published| published.slug.as_str())
                .collect::<Vec<_>>(),
            ["other"]
        );
        assert!(sitemap_revisions(Vec::new(), &hosts, Some(&mapping)).is_empty());
    }

    #[test]
    fn custom_host_media_requires_a_valid_asset_in_the_published_v2_document() {
        let asset_id = "a".repeat(64);
        let mut document: Value = serde_json::from_str(include_str!(
            "../../../crates/domain/tests/fixtures/document_v2_minimal.json"
        ))
        .expect("minimal V2 fixture is valid JSON");
        document["pages"][0]["sections"][0]["blocks"] = serde_json::json!([{
            "id": "image-block",
            "kind": "image",
            "asset_id": asset_id.clone(),
            "alt": {"kind": "decorative"}
        }]);
        let data_json = document.to_string();

        assert!(published_document_references_asset(
            2,
            &data_json,
            "funnel-01",
            &asset_id,
        ));
        assert!(!published_document_references_asset(
            2,
            &data_json,
            "funnel-01",
            &"b".repeat(64),
        ));
        assert!(!published_document_references_asset(
            1,
            &data_json,
            "funnel-01",
            &asset_id,
        ));
        assert!(!published_document_references_asset(
            2,
            "not-json",
            "funnel-01",
            &asset_id,
        ));
    }

    #[test]
    fn v2_document_routes_and_preview_parameters_are_closed() {
        assert_eq!(
            funnel_subroute("/api/funnels/funnel-a/document", "/document"),
            Some("funnel-a")
        );
        assert_eq!(
            funnel_subroute("/api/funnels/funnel-a/preview-v2", "/preview-v2"),
            Some("funnel-a")
        );
        assert_eq!(
            funnel_subroute("/api/funnels/funnel-a/stages-v2", "/stages-v2"),
            Some("funnel-a")
        );
        assert_eq!(
            funnel_subroute(
                "/api/funnels/funnel-a/convert-v2/preview",
                "/convert-v2/preview"
            ),
            Some("funnel-a")
        );
        assert_eq!(
            funnel_subroute("/api/funnels/funnel-a/convert-v2", "/convert-v2"),
            Some("funnel-a")
        );

        assert_eq!(
            v2_preview_command_parameters(
                "route-funnel",
                r#"{"base_revision_id":"revision-a","page_id":"page-a","document_json":"{\"schema_version\":2}"}"#
            )
            .expect("route provides funnel identity"),
            r#"{"base_revision_id":"revision-a","document_json":"{\"schema_version\":2}","funnel_id":"route-funnel","page_id":"page-a"}"#
        );
        assert_eq!(
            v2_preview_command_parameters(
                "route-funnel",
                r#"{"base_revision_id":"revision-a","base_revision_id":"revision-b","page_id":"page-a","document_json":"{}"}"#
            ),
            Err(EditorCommandParametersError::DuplicateObjectKey)
        );
        assert_eq!(
            v2_preview_command_parameters(
                "route-funnel",
                r#"{"base_revision_id":"revision-a","page_id":"page-a","document_json":"{}","funnel_id":"client-funnel"}"#
            ),
            Err(EditorCommandParametersError::InvalidJson)
        );
    }

    #[test]
    fn conversion_request_is_closed_and_uses_route_identity() {
        assert_eq!(
            v1_conversion_command_parameters(
                "route-funnel",
                r#"{"base_revision_id":"revision-a"}"#
            )
            .expect("valid conversion request"),
            r#"{"base_revision_id":"revision-a","funnel_id":"route-funnel"}"#
        );
        assert_eq!(
            v1_conversion_command_parameters(
                "route-funnel",
                r#"{"base_revision_id":"revision-a","funnel_id":"client-funnel"}"#
            ),
            Err(EditorCommandParametersError::InvalidJson)
        );
        assert_eq!(
            v1_conversion_command_parameters(
                "route-funnel",
                r#"{"base_revision_id":"revision-a","base_revision_id":"revision-b"}"#
            ),
            Err(EditorCommandParametersError::DuplicateObjectKey)
        );
        assert_eq!(
            v1_conversion_command_parameters(
                "route-funnel",
                r#"{"base_revision_id":"revision-a","html":"unsafe"}"#
            ),
            Err(EditorCommandParametersError::InvalidJson)
        );
    }

    #[test]
    fn v2_stage_parameters_are_bounded_closed_and_use_route_identity() {
        let digest = "a".repeat(64);
        let body = format!(
            r#"{{"base_revision_id":"revision-a","expected_length":1048576,"expected_sha256":"{digest}"}}"#
        );
        assert_eq!(
            v2_stage_command_parameters("route-funnel", &body).expect("valid stage request"),
            format!(
                r#"{{"base_revision_id":"revision-a","expected_length":1048576,"expected_sha256":"{digest}","funnel_id":"route-funnel"}}"#
            )
        );
        assert_eq!(
            v2_stage_command_parameters(
                "route-funnel",
                &format!(
                    r#"{{"base_revision_id":"revision-a","expected_length":0,"expected_sha256":"{digest}"}}"#
                )
            ),
            Err(EditorCommandParametersError::InvalidJson)
        );
        assert_eq!(
            v2_stage_command_parameters(
                "route-funnel",
                &format!(
                    r#"{{"base_revision_id":"revision-a","expected_length":1,"expected_sha256":"{}"}}"#,
                    "G".repeat(64)
                )
            ),
            Err(EditorCommandParametersError::InvalidJson)
        );
        assert_eq!(
            v2_stage_command_parameters(
                "route-funnel",
                &format!(
                    r#"{{"base_revision_id":"revision-a","expected_length":1048577,"expected_sha256":"{digest}"}}"#
                )
            ),
            Err(EditorCommandParametersError::InvalidJson)
        );
        assert_eq!(
            v2_stage_command_parameters(
                "route-funnel",
                &format!(
                    r#"{{"base_revision_id":"revision-a","base_revision_id":"revision-b","expected_length":1,"expected_sha256":"{digest}"}}"#
                )
            ),
            Err(EditorCommandParametersError::DuplicateObjectKey)
        );
        assert_eq!(
            v2_stage_command_parameters(
                "route-funnel",
                &format!(
                    r#"{{"base_revision_id":"revision-a","expected_length":1,"expected_sha256":"{digest}","funnel_id":"client-funnel"}}"#
                )
            ),
            Err(EditorCommandParametersError::InvalidJson)
        );
    }

    #[test]
    fn v2_save_parameters_fix_versions_and_route_identity() {
        let body = format!(
            r#"{{"base_revision_id":"revision-a","stage_id":"{}","expected_length":1,"expected_sha256":"{}"}}"#,
            "ab".repeat(32),
            "cd".repeat(32)
        );
        let value: serde_json::Value = serde_json::from_str(
            &v2_save_command_parameters("route-funnel", &body).expect("valid save request"),
        )
        .expect("command parameters are JSON");
        assert_eq!(value["funnel_id"], "route-funnel");
        assert_eq!(value["document_version"], 2);
        assert_eq!(value["schema_version"], 2);
        assert_eq!(value["manifest_version"], 2);

        assert_eq!(
            v2_save_command_parameters(
                "route-funnel",
                &body.replace("revision-a", "revision-a\",\"funnel_id\":\"client-funnel")
            ),
            Err(EditorCommandParametersError::InvalidJson)
        );
        assert_eq!(
            v2_save_command_parameters(
                "route-funnel",
                &body.replace("expected_length", "document_version\":3,\"expected_length")
            ),
            Err(EditorCommandParametersError::InvalidJson)
        );
        assert_eq!(
            v2_save_command_parameters(
                "route-funnel",
                &body.replace(
                    "base_revision_id",
                    "base_revision_id\":\"revision-b\",\"base_revision_id"
                )
            ),
            Err(EditorCommandParametersError::DuplicateObjectKey)
        );
    }

    #[test]
    fn v2_edit_parameters_are_closed_and_fix_route_identity() {
        let body = serde_json::json!({
            "base_revision_id": "revision-a",
            "page_id": "page-a",
            "document_json": "{}",
            "operation_id": "ab".repeat(16),
            "edit": { "operation": "delete_section", "page": "page-home", "section": "section-hero" },
        })
        .to_string();
        let parameters =
            v2_edit_command_parameters("route-funnel", &body).expect("valid typed edit body");
        let parsed: serde_json::Value = serde_json::from_str(&parameters).expect("JSON parameters");
        assert_eq!(parsed["funnel_id"], "route-funnel");
        assert_eq!(parsed["operation_id"], "ab".repeat(16));
        assert_eq!(parsed["edit"]["operation"], "delete_section");

        let mut client_identity: serde_json::Value = serde_json::from_str(&body).expect("body");
        client_identity["funnel_id"] = serde_json::json!("client-funnel");
        assert_eq!(
            v2_edit_command_parameters("route-funnel", &client_identity.to_string()),
            Err(EditorCommandParametersError::InvalidJson)
        );
        let invalid_operation = body.replace("ab", "AB");
        assert_eq!(
            v2_edit_command_parameters("route-funnel", &invalid_operation),
            Err(EditorCommandParametersError::InvalidJson)
        );
    }

    #[test]
    fn v2_document_revision_query_accepts_only_one_valid_revision_id() {
        assert_eq!(v2_document_revision_query(None), Ok(None));
        assert_eq!(
            v2_document_revision_query(Some("revision_id=revision-a")),
            Ok(Some("revision-a"))
        );
        for query in [
            "revision_id=revision-a&revision_id=revision-b",
            "other=revision-a",
            "revision_id=",
            "revision_id=../private",
        ] {
            assert_eq!(v2_document_revision_query(Some(query)), Err(()));
        }
    }

    #[test]
    fn stage_upload_capability_is_taken_only_from_a_single_bearer_header() {
        let token = "ab".repeat(32);
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {token}")).expect("valid header"),
        );
        assert_eq!(
            stage_upload_token(&headers).as_deref(),
            Some(token.as_str())
        );

        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer short-token"),
        );
        assert_eq!(stage_upload_token(&headers), None);

        headers.insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {token}")).expect("valid header"),
        );
        headers.append(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {token}")).expect("valid header"),
        );
        assert_eq!(stage_upload_token(&headers), None);
    }

    #[test]
    fn public_form_submission_route_matches_only_the_exact_resource_shape() {
        assert_eq!(
            public_form_submission_route_from_path(
                "/api/public/funnels/lead-gen/pages/page-home/forms/form-contact/submissions"
            ),
            Some(super::PublicFormSubmissionRoute {
                funnel_slug: "lead-gen",
                page_id: "page-home",
                form_id: "form-contact",
            })
        );
        for path in [
            "/api/public/funnels/lead-gen/pages/page-home/forms/form-contact/submissions/extra",
            "/api/public/funnels/lead-gen/page/page-home/forms/form-contact/submissions",
            "/api/public/funnels/lead-gen/pages/page-home/forms//submissions",
        ] {
            assert_eq!(public_form_submission_route_from_path(path), None);
        }
    }

    #[test]
    fn public_form_submission_identity_requires_one_key_and_one_revision() {
        let key = uuid::Uuid::new_v4().to_string();
        let expected = PublicFormSubmissionIdentity {
            submission_key: key.clone(),
            revision_id: "revision-a".to_owned(),
            submission_channel: FormSubmissionChannel::Unattributed,
        };
        assert_eq!(
            public_form_submission_identity(Some(&format!("key={key}&revision_id=revision-a"))),
            Some(expected.clone())
        );
        assert_eq!(
            public_form_submission_identity(Some(&format!("revision_id=revision-a&key={key}"))),
            Some(expected)
        );
        assert_eq!(
            public_form_submission_identity(Some(&format!(
                "key={key}&revision_id=revision-a&channel=social"
            )))
            .map(|identity| identity.submission_channel),
            Some(FormSubmissionChannel::Social)
        );
        assert_eq!(
            public_form_submission_identity(Some(&format!(
                "key={key}&revision_id=revision-a&channel=raw-utm"
            )))
            .map(|identity| identity.submission_channel),
            Some(FormSubmissionChannel::Other)
        );
        for query in [
            None,
            Some("key="),
            Some("key=not-a-uuid&revision_id=revision-a"),
            Some("key=00000000-0000-0000-0000-000000000000"),
            Some("revision_id=revision-a"),
            Some("key=00000000-0000-0000-0000-000000000000&revision_id="),
            Some("key=00000000-0000-0000-0000-000000000000&revision_id=../private"),
            Some("key=00000000-0000-0000-0000-000000000000&revision_id=revision-a&extra=x"),
            Some("key=00000000-0000-0000-0000-000000000000&key=00000000-0000-0000-0000-000000000000&revision_id=revision-a"),
        ] {
            assert_eq!(public_form_submission_identity(query), None, "{query:?}");
        }
    }

    #[test]
    fn page_utm_medium_is_reduced_to_one_bounded_channel_category() {
        for (query, expected) in [
            (None, FormSubmissionChannel::Unattributed),
            (
                Some("utm_source=fixture-source"),
                FormSubmissionChannel::Unattributed,
            ),
            (Some("utm_medium=organic"), FormSubmissionChannel::Search),
            (
                Some("utm_medium=social&utm_campaign=campaign-fixture"),
                FormSubmissionChannel::Social,
            ),
            (Some("utm_medium=unknown"), FormSubmissionChannel::Other),
            (
                Some("utm_medium=social&utm_medium=email"),
                FormSubmissionChannel::Other,
            ),
            (Some("utm_medium="), FormSubmissionChannel::Other),
        ] {
            assert_eq!(
                public_form_submission_channel_from_page_query(query),
                expected,
                "{query:?}"
            );
        }
        assert_eq!(
            public_form_submission_channel_from_page_query(Some(&format!(
                "utm_medium={}",
                "x".repeat(65)
            ))),
            FormSubmissionChannel::Other
        );
        assert_eq!(
            public_form_submission_channel_from_page_query(Some(&"x".repeat(2_049))),
            FormSubmissionChannel::Other
        );
    }

    #[test]
    fn public_form_submission_query_accepts_one_canonical_cursor_status_and_funnel() {
        let submission_id = "11111111-1111-4111-8111-111111111111";
        let funnel_id = "22222222-2222-4222-8222-222222222222";
        assert_eq!(
            public_form_submission_query(None),
            Ok(super::PublicFormSubmissionQuery {
                cursor: None,
                status: None,
                funnel_id: None,
            })
        );
        assert_eq!(
            public_form_submission_query(Some("")),
            Ok(super::PublicFormSubmissionQuery {
                cursor: None,
                status: None,
                funnel_id: None,
            })
        );
        assert_eq!(
            public_form_submission_query(Some(&format!("cursor=1234567890.{submission_id}"))),
            Ok(super::PublicFormSubmissionQuery {
                cursor: Some(PublicFormSubmissionCursor {
                    submitted_at: 1_234_567_890,
                    submission_id: submission_id.to_owned(),
                    status: None,
                    funnel_id: None,
                }),
                status: None,
                funnel_id: None,
            })
        );
        assert_eq!(
            public_form_submission_query(Some(&format!(
                "cursor=1234567890.{submission_id}.contacted.{funnel_id}&status=contacted&funnel_id={funnel_id}"
            ))),
            Ok(super::PublicFormSubmissionQuery {
                cursor: Some(PublicFormSubmissionCursor {
                    submitted_at: 1_234_567_890,
                    submission_id: submission_id.to_owned(),
                    status: Some(FormSubmissionStatus::Contacted),
                    funnel_id: Some(funnel_id.to_owned()),
                }),
                status: Some(FormSubmissionStatus::Contacted),
                funnel_id: Some(funnel_id.to_owned()),
            })
        );
        for query in [
            "other=1234567890.11111111-1111-4111-8111-111111111111",
            "cursor=1234567890.11111111-1111-4111-8111-111111111111&cursor=1234567890.11111111-1111-4111-8111-111111111112",
            "cursor=0.11111111-1111-4111-8111-111111111111",
            "cursor=9007199254740992.11111111-1111-4111-8111-111111111111",
            "cursor=1234567890.not-a-uuid",
            "cursor=01234567890.11111111-1111-4111-8111-111111111111",
            "status=unknown",
            "status=new&status=closed",
            "status=NEW",
            "status=new&other=value",
            "funnel_id=",
            "funnel_id=not-a-uuid",
            "funnel_id=22222222-2222-4222-8222-22222222222A",
            "funnel_id=22222222-2222-4222-8222-222222222222&funnel_id=22222222-2222-4222-8222-222222222222",
            &format!("cursor=1234567890.{submission_id}&status=contacted"),
            &format!("cursor=1234567890.{submission_id}.new.{funnel_id}&status=contacted&funnel_id={funnel_id}"),
            &format!("cursor=1234567890.{submission_id}.contacted.all&status=contacted&funnel_id={funnel_id}"),
            &format!("cursor=1234567890.{submission_id}.contacted.{funnel_id}.extra&status=contacted&funnel_id={funnel_id}"),
        ] {
            assert_eq!(public_form_submission_query(Some(query)), Err(()), "{query}");
        }
    }

    #[test]
    fn form_submission_status_route_requires_one_exact_id_and_suffix() {
        let submission_id = "11111111-1111-4111-8111-111111111111";
        assert_eq!(
            form_submission_status_id_from_path(&format!(
                "/api/submissions/{submission_id}/status"
            )),
            Some(submission_id)
        );
        for path in [
            "/api/submissions/id/status/extra",
            "/api/submissions/id/other",
            "/api/submissions//status",
            "/api/submissions/id/status/",
            "/api/submissions/id",
        ] {
            assert_eq!(form_submission_status_id_from_path(path), None, "{path}");
        }
    }

    #[test]
    fn submission_export_and_delete_routes_require_a_canonical_id() {
        let submission_id = "11111111-1111-4111-8111-111111111111";
        assert_eq!(
            form_submission_export_id_from_path(&format!(
                "/api/submissions/{submission_id}/export"
            )),
            Some(submission_id)
        );
        assert_eq!(
            form_submission_delete_id_from_path(&format!("/api/submissions/{submission_id}")),
            Some(submission_id)
        );
        for path in [
            "/api/submissions/not-a-uuid/export",
            "/api/submissions/11111111-1111-4111-8111-11111111111A/export",
            "/api/submissions/11111111-1111-4111-8111-111111111111/export/extra",
            "/api/submissions/not-a-uuid",
            "/api/submissions/11111111-1111-4111-8111-111111111111/",
        ] {
            assert_eq!(form_submission_export_id_from_path(path), None, "{path}");
            assert_eq!(form_submission_delete_id_from_path(path), None, "{path}");
        }
    }

    #[test]
    fn submission_recovery_actions_are_closed_and_generation_bound() {
        let generation = "11111111-1111-4111-8111-111111111111";
        assert_eq!(
            submission_recovery_action(r#"{"action":"close"}"#),
            Ok(super::SubmissionRecoveryAction::Close)
        );
        assert_eq!(
            submission_recovery_action(&format!(
                r#"{{"action":"prepare","generation":"{generation}"}}"#
            )),
            Ok(super::SubmissionRecoveryAction::Prepare {
                generation: generation.to_owned()
            })
        );
        assert_eq!(
            submission_recovery_action(&format!(
                r#"{{"action":"reconcile","generation":"{generation}"}}"#
            )),
            Ok(super::SubmissionRecoveryAction::Reconcile {
                generation: generation.to_owned()
            })
        );
        for body in [
            r#"{"action":"open"}"#,
            r#"{"action":"close","generation":"11111111-1111-4111-8111-111111111111"}"#,
            r#"{"action":"prepare","generation":"not-a-uuid"}"#,
            r#"{"action":"reconcile","generation":"11111111-1111-4111-8111-111111111111","complete":true}"#,
            r#"{"action":"close","action":"reconcile"}"#,
        ] {
            assert_eq!(submission_recovery_action(body), Err(()), "{body}");
        }
    }

    #[test]
    fn submission_data_routes_share_the_recovery_gate() {
        assert!(submission_recovery_gated_path("/api/submissions"));
        assert!(submission_recovery_gated_path(
            "/api/submissions/11111111-1111-4111-8111-111111111111/export"
        ));
        assert!(submission_recovery_gated_path(
            "/api/submissions/11111111-1111-4111-8111-111111111111/delivery/retry"
        ));
        assert!(submission_recovery_gated_path(
            "/api/submissions/11111111-1111-4111-8111-111111111111/delivery/cancel"
        ));
        assert!(submission_recovery_gated_path(
            "/api/public/funnels/service/pages/home/forms/contact/submissions"
        ));
        assert!(submission_recovery_gated_path(
            "/api/funnels/11111111-1111-4111-8111-111111111111/completions"
        ));
        assert!(!submission_recovery_gated_path("/api/submissions/recovery"));
        assert!(!submission_recovery_gated_path(
            "/api/public/funnels/service/interaction-events"
        ));
    }

    #[test]
    fn status_mutation_requires_one_json_content_type() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        assert!(is_json_content_type(&headers));

        headers.insert(
            "content-type",
            HeaderValue::from_static("application/json; charset=utf-8"),
        );
        assert!(is_json_content_type(&headers));

        headers.insert("content-type", HeaderValue::from_static("text/plain"));
        assert!(!is_json_content_type(&headers));
        headers.append("content-type", HeaderValue::from_static("application/json"));
        assert!(!is_json_content_type(&headers));
    }

    #[test]
    fn status_mutation_adapter_accepts_only_one_closed_status_field() {
        let submission_id = "11111111-1111-4111-8111-111111111111";
        assert_eq!(
            form_submission_status_command_parameters(submission_id, r#"{"status":"qualified"}"#),
            Ok(format!(
                r#"{{"status":"qualified","submission_id":"{submission_id}"}}"#
            ))
        );
        for body in [
            r#"{"status":"unknown"}"#,
            r#"{"status":"new","status":"closed"}"#,
            r#"{"status":"new","extra":true}"#,
            r#"{"status":null}"#,
            "null",
        ] {
            assert_eq!(
                form_submission_status_command_parameters(submission_id, body),
                Err(()),
                "{body}"
            );
        }
        assert_eq!(
            form_submission_status_command_parameters("not-a-uuid", r#"{"status":"new"}"#),
            Err(())
        );
    }

    #[test]
    fn form_completion_period_requires_one_supported_value() {
        assert_eq!(
            form_completion_period(Some("period=7")),
            Ok(FormCompletionPeriod::Days7)
        );
        assert_eq!(
            form_completion_period(Some("period=30")),
            Ok(FormCompletionPeriod::Days30)
        );
        assert_eq!(
            form_completion_period(Some("period=90")),
            Ok(FormCompletionPeriod::Days90)
        );
        for query in [
            None,
            Some(""),
            Some("period=1"),
            Some("period=07"),
            Some("period=30&period=90"),
            Some("period=30&other=value"),
            Some("other=value"),
        ] {
            assert_eq!(form_completion_period(query), Err(()), "{query:?}");
        }
    }

    #[test]
    fn interaction_report_period_requires_one_supported_value() {
        assert_eq!(
            interaction_report_period(Some("period=7")),
            Ok(InteractionReportPeriod::Days7)
        );
        assert_eq!(
            interaction_report_period(Some("period=30")),
            Ok(InteractionReportPeriod::Days30)
        );
        assert_eq!(
            interaction_report_period(Some("period=90")),
            Ok(InteractionReportPeriod::Days90)
        );
        for query in [
            None,
            Some(""),
            Some("period=1"),
            Some("period=07"),
            Some("period=30&period=90"),
            Some("period=30&other=value"),
            Some("other=value"),
        ] {
            assert_eq!(interaction_report_period(query), Err(()), "{query:?}");
        }
    }

    #[test]
    fn submission_identity_binding_changes_only_the_form_action_query() {
        let marker = eez_render::document_v2::PUBLIC_FORM_SUBMISSION_KEY_MARKER;
        let html = format!(
            "<p>{marker}</p><form action=\"/api/public/funnels/test/pages/home/forms/{marker}/submissions?key={marker}\"><input value=\"{marker}\"></form><a href=\"https://example.test/?key={marker}\">copy</a>"
        );
        let key = "e8ac58a6-a7cf-4f65-9a3e-13e9b2e7d59d";

        let bound = bind_public_form_submission_identity(
            &html,
            key,
            "revision-a",
            FormSubmissionChannel::Social,
        );

        assert!(bound.contains(&format!(
            "forms/{marker}/submissions?key={key}&revision_id=revision-a&channel=social"
        )));
        assert!(bound.contains(&format!("<input value=\"{marker}\">")));
        assert!(bound.contains(&format!("<p>{marker}</p>")));
        assert!(bound.contains(&format!("href=\"https://example.test/?key={marker}\"")));
    }

    #[test]
    fn public_form_body_keeps_consent_choices_separate_and_rejects_duplicate_pairs() {
        let request_only = parse_public_form_body(b"email=ada%40example.test&_request_consent=on")
            .expect("valid browser form body");
        assert!(request_only.request_consent_accepted);
        assert!(!request_only.marketing_opt_in);
        assert_eq!(
            request_only.fields.get("email").map(String::as_str),
            Some("ada@example.test")
        );

        let both = parse_public_form_body(
            b"email=ada%40example.test&_request_consent=on&_marketing_opt_in=on",
        )
        .expect("valid explicit marketing choice");
        assert!(both.request_consent_accepted);
        assert!(both.marketing_opt_in);

        assert!(parse_public_form_body(b"email=first&email=second&_request_consent=on").is_none());
        assert!(parse_public_form_body(b"email=ada&_request_consent=yes").is_none());
        assert!(
            parse_public_form_body(b"email=ada&_request_consent=on&_request_consent=on").is_none()
        );
    }

    #[test]
    fn public_form_html_negotiation_requires_an_explicit_preference() {
        assert!(!prefers_public_form_html_accept_values(std::iter::empty::<
            &str,
        >()));
        assert!(!prefers_public_form_html_accept_values(["*/*"]));
        assert!(!prefers_public_form_html_accept_values(["text/*"]));
        assert!(!prefers_public_form_html_accept_values([
            "application/json"
        ]));
        assert!(!prefers_public_form_html_accept_values(["text/html;q=0"]));
        assert!(!prefers_public_form_html_accept_values([
            "text/html;q=0.8, application/json;q=0.9"
        ]));
        assert!(!prefers_public_form_html_accept_values([
            "text/html;q=1, application/json;q=1"
        ]));
        assert!(!prefers_public_form_html_accept_values([
            "text/html;q=0.9, */*;q=1"
        ]));
        assert!(prefers_public_form_html_accept_values(["text/html"]));
        assert!(prefers_public_form_html_accept_values([
            "text/html;q=0.9, application/json;q=0.8"
        ]));
        assert!(prefers_public_form_html_accept_values([
            "text/html;q=0.5, application/json;q=0"
        ]));
        assert!(prefers_public_form_html_accept_values([
            "text/html;q=0.8, application/json;q=0, */*;q=1"
        ]));
        assert!(prefers_public_form_html_accept_values([
            "text/html, */*;q=0.5"
        ]));
        assert!(prefers_public_form_html_accept_values([
            "text/html;q=0.6",
            "application/json;q=0.4"
        ]));
    }

    #[test]
    fn public_form_accept_parser_fuzzes_malformed_inputs_without_panicking() {
        let alphabet = b"text/htmlapplicationjsonq=;,* /0123456789\r\n\xff\x80";
        let mut state = 0x41c6_4e6d_u64;
        for _ in 0..10_000 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let length = (state as usize) % 160;
            let mut bytes = Vec::with_capacity(length);
            for _ in 0..length {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                bytes.push(alphabet[(state as usize) % alphabet.len()]);
            }
            let value = String::from_utf8_lossy(&bytes);
            let _ = prefers_public_form_html_accept_values([value.as_ref()]);
        }
    }
}
