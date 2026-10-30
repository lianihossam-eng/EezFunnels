use axum::body::to_bytes;
use axum::http::{Method, StatusCode};
use axum::response::Response;
use eez_commands::service_brief::{
    read_service_brief, save_service_brief, ServiceBriefCommandError, ServiceBriefSnapshot,
};
use eez_domain::service_brief::{
    BriefVersion, ServiceBrief, ServiceBriefDraft, SERVICE_BRIEF_FORMAT_VERSION,
    SERVICE_BRIEF_MAX_REQUEST_BYTES,
};
use serde::Deserialize;
use worker::{Env, HttpRequest};

use crate::errors::ApiError;
use crate::json::validate_unique_json;

#[derive(Debug, Eq, PartialEq)]
struct ParsedServiceBriefUpdate {
    expected_version: i32,
    brief: ServiceBrief,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ServiceBriefUpdateBody {
    expected_version: i32,
    format_version: u8,
    brief: ServiceBriefDraft,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ServiceBriefRequestError {
    InvalidJson,
    InvalidBrief,
    UnsupportedFormatVersion,
}

pub(super) async fn response(
    env: &Env,
    request: HttpRequest,
    funnel_id: &str,
    correlation_id: String,
) -> Response<axum::body::Body> {
    if !crate::valid_funnel_route_id(funnel_id) {
        return crate::funnel_route_not_found_response(&correlation_id);
    }
    match *request.method() {
        Method::GET => read_response(env, funnel_id, &correlation_id).await,
        Method::PUT => write_response(env, request, funnel_id, &correlation_id).await,
        _ => crate::method_not_allowed_for(&correlation_id, "GET, PUT"),
    }
}

async fn read_response(
    env: &Env,
    funnel_id: &str,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return service_brief_error_response("external/unavailable", correlation_id),
    };
    match read_service_brief(&bindings.service_brief_repository(), funnel_id).await {
        Ok(snapshot) => success_response(snapshot, correlation_id),
        Err(error) => command_error_response(error, correlation_id),
    }
}

async fn write_response(
    env: &Env,
    request: HttpRequest,
    funnel_id: &str,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    if !crate::origin::matches_request_host(request.headers(), request.uri().scheme_str()) {
        return service_brief_error_response("request/invalid_origin", correlation_id);
    }
    if !crate::is_json_content_type(request.headers()) {
        return service_brief_error_response("validation/invalid_json", correlation_id);
    }
    let body = match to_bytes(
        axum::body::Body::new(request.into_body()),
        SERVICE_BRIEF_MAX_REQUEST_BYTES,
    )
    .await
    {
        Ok(body) => body,
        Err(_) => {
            return service_brief_error_response(
                "validation/service_brief_too_large",
                correlation_id,
            )
        }
    };
    let body = match std::str::from_utf8(&body) {
        Ok(body) => body,
        Err(_) => return service_brief_error_response("validation/invalid_json", correlation_id),
    };
    let update = match parse_update(body) {
        Ok(update) => update,
        Err(ServiceBriefRequestError::InvalidJson) => {
            return service_brief_error_response("validation/invalid_json", correlation_id)
        }
        Err(ServiceBriefRequestError::InvalidBrief) => {
            return service_brief_error_response("validation/service_brief_invalid", correlation_id)
        }
        Err(ServiceBriefRequestError::UnsupportedFormatVersion) => {
            return service_brief_error_response(
                "validation/service_brief_format_unsupported",
                correlation_id,
            )
        }
    };
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return service_brief_error_response("external/unavailable", correlation_id),
    };
    match save_service_brief(
        &bindings.service_brief_repository(),
        funnel_id,
        update.expected_version,
        update.brief,
    )
    .await
    {
        Ok(snapshot) => success_response(snapshot, correlation_id),
        Err(error) => command_error_response(error, correlation_id),
    }
}

fn parse_update(body: &str) -> Result<ParsedServiceBriefUpdate, ServiceBriefRequestError> {
    validate_unique_json(body).map_err(|_| ServiceBriefRequestError::InvalidJson)?;
    let body: ServiceBriefUpdateBody =
        serde_json::from_str(body).map_err(|_| ServiceBriefRequestError::InvalidJson)?;
    if body.format_version != SERVICE_BRIEF_FORMAT_VERSION {
        return Err(ServiceBriefRequestError::UnsupportedFormatVersion);
    }
    BriefVersion::try_from(body.expected_version)
        .map_err(|_| ServiceBriefRequestError::InvalidJson)?;
    let brief =
        ServiceBrief::from_draft(body.brief).map_err(|_| ServiceBriefRequestError::InvalidBrief)?;
    Ok(ParsedServiceBriefUpdate {
        expected_version: body.expected_version,
        brief,
    })
}

fn success_response(
    snapshot: ServiceBriefSnapshot,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    match serde_json::to_value(snapshot) {
        Ok(result) => crate::json_success_response(result, StatusCode::OK, correlation_id, None),
        Err(_) => service_brief_error_response("internal/unexpected", correlation_id),
    }
}

fn command_error_response(
    error: ServiceBriefCommandError,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    service_brief_error_response(error.code(), correlation_id)
}

fn service_brief_error_response(code: &str, correlation_id: &str) -> Response<axum::body::Body> {
    let error = ApiError::from_code(code);
    crate::log_api_error(&error, correlation_id, error.code());
    crate::error_response(&error, correlation_id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{parse_update, ServiceBriefRequestError};
    use crate::json::tests::deterministic_json_fuzz_inputs;
    use std::panic::catch_unwind;

    #[test]
    fn service_brief_update_parser_accepts_partial_notes_and_three_intentions() {
        let request = parse_update(
            r#"{"expected_version":0,"format_version":1,"brief":{"intention":"quote_request","audience":"Local studios"}}"#,
        )
        .expect("partial quote brief");

        assert_eq!(request.expected_version, 0);
        assert_eq!(
            request.brief.intention(),
            Some(eez_domain::service_brief::ServiceIntention::QuoteRequest)
        );
        assert_eq!(request.brief.audience(), Some("Local studios"));
        for intention in ["contact", "quote_request", "diagnostic_application"] {
            let input = format!(
                r#"{{"expected_version":0,"format_version":1,"brief":{{"intention":"{intention}"}}}}"#
            );
            assert!(parse_update(&input).is_ok(), "{intention}");
        }
    }

    #[test]
    fn service_brief_update_parser_rejects_duplicates_unknown_fields_and_bad_versions() {
        assert_eq!(
            parse_update(
                r#"{"expected_version":0,"expected_version":1,"format_version":1,"brief":{}}"#
            ),
            Err(ServiceBriefRequestError::InvalidJson)
        );
        assert_eq!(
            parse_update(r#"{"expected_version":0,"format_version":1,"brief":{"unexpected":"x"}}"#),
            Err(ServiceBriefRequestError::InvalidJson)
        );
        assert_eq!(
            parse_update(r#"{"expected_version":0,"format_version":2,"brief":{}}"#),
            Err(ServiceBriefRequestError::UnsupportedFormatVersion)
        );
        assert_eq!(
            parse_update(r#"{"expected_version":-1,"format_version":1,"brief":{}}"#),
            Err(ServiceBriefRequestError::InvalidJson)
        );
        assert_eq!(
            parse_update(
                r#"{"expected_version":0,"format_version":1,"brief":{"offer":"private\u0007note"}}"#
            ),
            Err(ServiceBriefRequestError::InvalidBrief)
        );
    }

    #[test]
    fn deterministic_service_brief_json_fuzzes_ten_thousand_inputs_without_panicking() {
        let inputs = deterministic_json_fuzz_inputs();
        assert_eq!(inputs.len(), 10_000);
        for (case_number, input) in inputs.iter().enumerate() {
            let result = catch_unwind(|| parse_update(input));
            assert!(
                result.is_ok(),
                "service brief parser panicked for case {case_number}: {input:?}"
            );
        }
    }
}
