use axum::body::to_bytes;
use axum::http::{Method, StatusCode};
use axum::response::Response;
use eez_commands::submission_follow_up::{
    read_submission_follow_up, update_submission_follow_up, SubmissionFollowUpCommandError,
};
use eez_domain::{FollowUpValidationError, FormSubmissionStatus, SubmissionFollowUp};
use serde::Deserialize;
use worker::{Env, HttpRequest};

use crate::errors::ApiError;
use crate::json::validate_unique_json;

const MAX_REQUEST_BYTES: usize = 4 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateBody {
    expected_version: i32,
    status: FormSubmissionStatus,
    #[serde(default)]
    corrects_version: Option<i32>,
    #[serde(deserialize_with = "deserialize_required_nullable_string")]
    next_action: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable_string")]
    due_on: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable_string")]
    close_reason: Option<String>,
}

fn deserialize_required_nullable_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)
}

pub(super) fn submission_id_from_path(path: &str) -> Option<&str> {
    let route = path.strip_prefix("/api/submissions/")?;
    let (submission_id, suffix) = route.split_once('/')?;
    (suffix == "follow-up" && crate::canonical_submission_id(submission_id))
        .then_some(submission_id)
}

pub(super) async fn response(
    env: &Env,
    request: HttpRequest,
    submission_id: &str,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    match *request.method() {
        Method::GET => read_response(env, submission_id, correlation_id).await,
        Method::PUT => update_response(env, request, submission_id, correlation_id).await,
        _ => private(crate::method_not_allowed_for(correlation_id, "GET, PUT")),
    }
}

async fn read_response(
    env: &Env,
    submission_id: &str,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return private_error_response("external/unavailable", correlation_id),
    };
    match read_submission_follow_up(&bindings.submission_follow_up_repository(), submission_id)
        .await
    {
        Ok(history) => match serde_json::to_value(history) {
            Ok(value) => private(crate::json_success_response(
                value,
                StatusCode::OK,
                correlation_id,
                None,
            )),
            Err(_) => private_error_response("internal/unexpected", correlation_id),
        },
        Err(error) => command_error_response(error, correlation_id),
    }
}

async fn update_response(
    env: &Env,
    request: HttpRequest,
    submission_id: &str,
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
    let body = match to_bytes(
        axum::body::Body::new(request.into_body()),
        MAX_REQUEST_BYTES,
    )
    .await
    {
        Ok(body) => body,
        Err(_) => return private_error_response("validation/invalid_json", correlation_id),
    };
    let body = match std::str::from_utf8(&body) {
        Ok(body) => body,
        Err(_) => return private_error_response("validation/invalid_json", correlation_id),
    };
    let update = match parse_update(body) {
        Ok(update) => update,
        Err(UpdateRequestError::InvalidJson) => {
            return private_error_response("validation/invalid_json", correlation_id)
        }
        Err(UpdateRequestError::InvalidFollowUp) => {
            return private_error_response(
                "validation/submission_follow_up_invalid",
                correlation_id,
            )
        }
    };
    let bindings = match eez_storage::Bindings::from_env(env) {
        Ok(bindings) => bindings,
        Err(_) => return private_error_response("external/unavailable", correlation_id),
    };
    match update_submission_follow_up(
        &bindings.submission_follow_up_repository(),
        submission_id,
        update.expected_version,
        &idempotency_key,
        &update.follow_up,
    )
    .await
    {
        Ok(receipt) => match serde_json::to_value(receipt) {
            Ok(value) => private(crate::json_success_response(
                value,
                StatusCode::OK,
                correlation_id,
                None,
            )),
            Err(_) => private_error_response("internal/unexpected", correlation_id),
        },
        Err(error) => command_error_response(error, correlation_id),
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ParsedUpdate {
    expected_version: i32,
    follow_up: SubmissionFollowUp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UpdateRequestError {
    InvalidJson,
    InvalidFollowUp,
}

fn parse_update(body: &str) -> Result<ParsedUpdate, UpdateRequestError> {
    validate_unique_json(body).map_err(|_| UpdateRequestError::InvalidJson)?;
    let body: UpdateBody =
        serde_json::from_str(body).map_err(|_| UpdateRequestError::InvalidJson)?;
    if body.expected_version < 0 {
        return Err(UpdateRequestError::InvalidJson);
    }
    let follow_up = SubmissionFollowUp::new(
        body.status,
        body.next_action,
        body.due_on,
        body.close_reason,
    )
    .and_then(|follow_up| follow_up.with_correction_target(body.corrects_version))
    .map_err(|error| match error {
        FollowUpValidationError::InvalidText
        | FollowUpValidationError::InvalidDate
        | FollowUpValidationError::CloseReasonRequiresClosedStatus
        | FollowUpValidationError::InvalidCorrectionTarget => UpdateRequestError::InvalidFollowUp,
    })?;
    if follow_up
        .corrects_version()
        .is_some_and(|version| version > body.expected_version)
    {
        return Err(UpdateRequestError::InvalidFollowUp);
    }
    Ok(ParsedUpdate {
        expected_version: body.expected_version,
        follow_up,
    })
}

fn command_error_response(
    error: SubmissionFollowUpCommandError,
    correlation_id: &str,
) -> Response<axum::body::Body> {
    private_error_response(error.code(), correlation_id)
}

fn private_error_response(code: &str, correlation_id: &str) -> Response<axum::body::Body> {
    let error = ApiError::from_code(code);
    crate::log_api_error(&error, correlation_id, error.code());
    private(crate::error_response(&error, correlation_id.to_owned()))
}

fn private(response: Response<axum::body::Body>) -> Response<axum::body::Body> {
    crate::private_lifecycle_response(response)
}

#[cfg(test)]
mod tests {
    use super::{parse_update, submission_id_from_path, UpdateRequestError};
    use eez_domain::FormSubmissionStatus;

    #[test]
    fn follow_up_path_requires_canonical_id_and_exact_suffix() {
        let id = "11111111-1111-4111-8111-111111111111";
        assert_eq!(
            submission_id_from_path(&format!("/api/submissions/{id}/follow-up")),
            Some(id)
        );
        for path in [
            "/api/submissions/not-a-uuid/follow-up",
            "/api/submissions/11111111-1111-4111-8111-111111111111/follow-up/extra",
            "/api/submissions/11111111-1111-4111-8111-11111111111/follow-up",
        ] {
            assert_eq!(submission_id_from_path(path), None, "{path}");
        }
    }

    #[test]
    fn update_parser_requires_complete_typed_disposition_and_normalizes_it() {
        let parsed = parse_update(
            r#"{"expected_version":2,"status":"contacted","next_action":"Send proposal","due_on":"2028-02-29","close_reason":null}"#,
        )
        .expect("valid follow-up update");
        assert_eq!(parsed.expected_version, 2);
        assert_eq!(parsed.follow_up.status(), FormSubmissionStatus::Contacted);
        assert_eq!(parsed.follow_up.next_action(), Some("Send proposal"));
        assert_eq!(parsed.follow_up.due_on(), Some("2028-02-29"));
        assert_eq!(parsed.follow_up.close_reason(), None);
        assert_eq!(
            parse_update(
                r#"{"expected_version":2,"status":"contacted","next_action":"Send proposal","due_on":"2028-02-29"}"#
            ),
            Err(UpdateRequestError::InvalidJson)
        );
        assert_eq!(
            parse_update(
                r#"{"expected_version":2,"status":"contacted","next_action":"Send proposal","due_on":"2028-02-29","close_reason":null,"unexpected":true}"#
            ),
            Err(UpdateRequestError::InvalidJson)
        );
    }

    #[test]
    fn update_parser_rejects_impossible_dates_and_close_reasons_for_open_statuses() {
        for (status, due_on, close_reason) in [
            ("contacted", "2027-02-29", "null"),
            ("new", "2028-02-29", "\"Not a fit\""),
        ] {
            let body = format!(
                r#"{{"expected_version":0,"status":"{status}","next_action":null,"due_on":"{due_on}","close_reason":{close_reason}}}"#
            );
            assert_eq!(
                parse_update(&body),
                Err(UpdateRequestError::InvalidFollowUp)
            );
        }
        assert_eq!(
            parse_update(
                r#"{"expected_version":2,"status":"contacted","next_action":null,"due_on":null,"close_reason":null,"corrects_version":3}"#
            ),
            Err(UpdateRequestError::InvalidFollowUp)
        );
        let latest_correction = parse_update(
            r#"{"expected_version":2,"status":"contacted","next_action":null,"due_on":null,"close_reason":null,"corrects_version":2}"#,
        )
        .expect("latest current event can be corrected");
        assert_eq!(latest_correction.follow_up.corrects_version(), Some(2));
        let correction = parse_update(
            r#"{"expected_version":2,"status":"contacted","next_action":null,"due_on":null,"close_reason":null,"corrects_version":1}"#,
        )
        .expect("valid correction target precedes current version");
        assert_eq!(correction.follow_up.corrects_version(), Some(1));
    }

    #[test]
    fn update_parser_fuzzes_deterministic_json_inputs_without_panicking() {
        for input in crate::json::tests::deterministic_json_fuzz_inputs() {
            let outcome = std::panic::catch_unwind(|| parse_update(&input));
            assert!(outcome.is_ok(), "parser panicked for {input:?}");
        }
    }
}
