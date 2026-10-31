use std::collections::BTreeMap;

use axum::body::to_bytes;
use axum::http::{HeaderValue, Method, StatusCode};
use axum::response::Response;
use eez_commands::{PortableTemplateImportResult, PortableTemplateInspection, Session};
use eez_domain::document_v2::MAX_PORTABLE_TEMPLATE_BYTES;
use serde::Deserialize;
use serde_json::Value;
use worker::{Env, HttpRequest};

const MAX_TEMPLATE_IMPORT_REQUEST_BYTES: usize = MAX_PORTABLE_TEMPLATE_BYTES + 512 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateImportRequest {
    name: String,
    template: Value,
    #[serde(default)]
    asset_replacements: BTreeMap<String, String>,
}

struct ParsedTemplateImport {
    name: String,
    template: Vec<u8>,
    asset_replacements: BTreeMap<String, String>,
}

pub(super) async fn export_response(
    env: &Env,
    request: HttpRequest,
    funnel_id: &str,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    if request.method() != Method::GET {
        return crate::method_not_allowed_for(correlation_id, "GET");
    }
    if !crate::valid_funnel_route_id(funnel_id) {
        return crate::funnel_route_not_found_response(correlation_id);
    }
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => {
            return crate::editor_payload_error_response(correlation_id, "external/unavailable")
        }
    };
    let repository = bindings.repository();
    let stages = bindings.document_stage_repository();
    let media = bindings.media_asset_repository();
    let session = Session::with_document_stages_and_media_assets(&repository, &stages, &media);
    match session.export_portable_template(funnel_id).await {
        Ok(bytes) => {
            let mut response = Response::new(axum::body::Body::from(bytes));
            response.headers_mut().insert(
                "content-type",
                HeaderValue::from_static("application/json; charset=utf-8"),
            );
            response.headers_mut().insert(
                "content-disposition",
                HeaderValue::from_static("attachment; filename=\"funnel-template.json\""),
            );
            response.headers_mut().insert(
                "cache-control",
                HeaderValue::from_static("private, no-store"),
            );
            if let Ok(value) = HeaderValue::from_str(correlation_id) {
                response.headers_mut().insert("x-correlation-id", value);
            }
            response
        }
        Err(error) => crate::funnel_command_error_response(
            crate::errors::ApiError::from_code(error.code()),
            correlation_id,
        ),
    }
}

pub(super) async fn inspect_response(
    env: &Env,
    request: HttpRequest,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    if request.method() != Method::POST {
        return crate::method_not_allowed_for(correlation_id, "POST");
    }
    if let Some(error) = validate_mutation_request(&request, correlation_id) {
        return error;
    }
    let body = match to_bytes(
        axum::body::Body::new(request.into_body()),
        MAX_PORTABLE_TEMPLATE_BYTES,
    )
    .await
    {
        Ok(body) => body,
        Err(_) => return template_error_response("validation/template_too_large", correlation_id),
    };
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => {
            return crate::editor_payload_error_response(correlation_id, "external/unavailable")
        }
    };
    let repository = bindings.repository();
    let stages = bindings.document_stage_repository();
    let media = bindings.media_asset_repository();
    let session = Session::with_document_stages_and_media_assets(&repository, &stages, &media);
    match session.inspect_portable_template(&body).await {
        Ok(inspection) => inspection_response(inspection, correlation_id),
        Err(error) => crate::funnel_command_error_response(
            crate::errors::ApiError::from_code(error.code()),
            correlation_id,
        ),
    }
}

pub(super) async fn import_response(
    env: &Env,
    request: HttpRequest,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    if request.method() != Method::POST {
        return crate::method_not_allowed_for(correlation_id, "POST");
    }
    let idempotency_key = match crate::request_idempotency_key(request.headers()) {
        Ok(key) => key,
        Err(code) => return template_error_response(code, correlation_id),
    };
    if let Some(error) = validate_mutation_request(&request, correlation_id) {
        return error;
    }
    let body = match to_bytes(
        axum::body::Body::new(request.into_body()),
        MAX_TEMPLATE_IMPORT_REQUEST_BYTES,
    )
    .await
    {
        Ok(body) => body,
        Err(_) => return template_error_response("validation/template_too_large", correlation_id),
    };
    let parsed = match parse_import_request(&body) {
        Ok(request) => request,
        Err(code) => return template_error_response(code, correlation_id),
    };
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => {
            return crate::editor_payload_error_response(correlation_id, "external/unavailable")
        }
    };
    let repository = bindings.repository();
    let stages = bindings.document_stage_repository();
    let media = bindings.media_asset_repository();
    let session = Session::with_document_stages_and_media_assets(&repository, &stages, &media);
    match session
        .import_portable_template(
            &parsed.name,
            &parsed.template,
            &parsed.asset_replacements,
            &idempotency_key,
        )
        .await
    {
        Ok(result) => import_success_response(result, correlation_id),
        Err(error) => crate::funnel_command_error_response(
            crate::errors::ApiError::from_code(error.code()),
            correlation_id,
        ),
    }
}

fn validate_mutation_request(
    request: &HttpRequest,
    correlation_id: &str,
) -> Option<Response<axum::body::Body>> {
    if !crate::origin::matches_request_host(request.headers(), request.uri().scheme_str()) {
        return Some(template_error_response(
            "request/invalid_origin",
            correlation_id,
        ));
    }
    if !crate::is_json_content_type(request.headers()) {
        return Some(template_error_response(
            "validation/invalid_json",
            correlation_id,
        ));
    }
    None
}

fn parse_import_request(raw: &[u8]) -> Result<ParsedTemplateImport, &'static str> {
    if raw.len() > MAX_TEMPLATE_IMPORT_REQUEST_BYTES {
        return Err("validation/template_too_large");
    }
    let raw = std::str::from_utf8(raw).map_err(|_| "validation/invalid_json")?;
    crate::json::validate_unique_json(raw).map_err(|_| "validation/invalid_json")?;
    let request: TemplateImportRequest =
        serde_json::from_str(raw).map_err(|_| "validation/invalid_json")?;
    let template = serde_json::to_vec(&request.template).map_err(|_| "validation/invalid_json")?;
    if template.len() > MAX_PORTABLE_TEMPLATE_BYTES {
        return Err("validation/template_too_large");
    }
    Ok(ParsedTemplateImport {
        name: request.name,
        template,
        asset_replacements: request.asset_replacements,
    })
}

fn inspection_response(
    inspection: PortableTemplateInspection,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    let mut response = crate::json_success_response(
        serde_json::to_value(inspection).unwrap_or(Value::Null),
        StatusCode::OK,
        correlation_id,
        None,
    );
    response.headers_mut().insert(
        "cache-control",
        HeaderValue::from_static("private, no-store"),
    );
    response
}

fn import_success_response(
    result: PortableTemplateImportResult,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    let location = format!("/funnels/{}", result.funnel_id);
    let mut response = crate::json_success_response(
        serde_json::to_value(result).unwrap_or(Value::Null),
        StatusCode::CREATED,
        correlation_id,
        Some(location),
    );
    response.headers_mut().insert(
        "cache-control",
        HeaderValue::from_static("private, no-store"),
    );
    response
}

fn template_error_response(code: &str, correlation_id: &str) -> Response<axum::body::Body> {
    let error = crate::errors::ApiError::from_code(code);
    crate::funnel_command_error_response(error, correlation_id)
}

#[cfg(test)]
mod tests {
    use super::parse_import_request;
    use eez_domain::document_v2::{BlueprintId, DocumentV2, PortableTemplate};
    use eez_domain::ThemePreset;
    use serde_json::json;

    fn valid_package() -> Value {
        let document =
            DocumentV2::starter("source-funnel", BlueprintId::LeadGen, ThemePreset::Minimal)
                .expect("starter document");
        let template = PortableTemplate::from_parts("Service", document, Vec::new())
            .expect("portable template");
        serde_json::from_slice(&template.encode().expect("encoded template"))
            .expect("template JSON")
    }

    use serde_json::Value;

    #[test]
    fn import_transport_accepts_versioned_json_and_replacements() {
        let body = json!({
            "name": "Imported service",
            "template": valid_package(),
            "asset_replacements": {}
        });
        let parsed = parse_import_request(body.to_string().as_bytes()).expect("import request");
        assert_eq!(parsed.name, "Imported service");
        assert!(parsed.asset_replacements.is_empty());
        assert!(PortableTemplate::decode(&parsed.template).is_ok());
    }

    #[test]
    fn import_transport_rejects_duplicate_and_unknown_fields() {
        assert!(parse_import_request(
            br#"{"name":"A","name":"B","template":{},"asset_replacements":{}}"#
        )
        .is_err());
        let unknown =
            json!({"name":"A","template":valid_package(),"asset_replacements":{},"extra":true});
        assert!(matches!(
            parse_import_request(unknown.to_string().as_bytes()),
            Err("validation/invalid_json")
        ));
    }

    #[test]
    fn import_transport_parser_handles_deterministic_fuzz_inputs() {
        let mut state = 0x8f3c_71a5_u32;
        for _ in 0..10_000 {
            let length = (state as usize % 256) + 1;
            let mut input = vec![0_u8; length];
            for byte in &mut input {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                *byte = state as u8;
            }
            let _ = parse_import_request(&input);
        }
    }
}
