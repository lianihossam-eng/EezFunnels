use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    Validation,
    Authorization,
    NotFound,
    Conflict,
    RateLimit,
    External,
    Internal,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiError {
    code: String,
    category: ErrorCategory,
    status: u16,
    message: &'static str,
    alert: bool,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub code: String,
    pub category: ErrorCategory,
    pub message: &'static str,
    pub correlation_id: String,
}

impl ApiError {
    pub fn from_storage_error<T: std::fmt::Debug>(_source: &T) -> Self {
        Self::from_code("external/unavailable")
    }

    pub fn from_code(code: &str) -> Self {
        let (code, category, status, message, alert) = match code {
            "validation/invalid_json"
            | "validation/invalid_query"
            | "validation/missing_field"
            | "validation/order_duplicate"
            | "validation/schema_version_missing"
            | "validation/schema_version_unsupported"
            | "validation/section_type_unknown"
            | "validation/section_missing_id" => (
                code.to_owned(),
                ErrorCategory::Validation,
                400,
                "The request is invalid.",
                false,
            ),
            "validation/operation_not_supported" => (
                code.to_owned(),
                ErrorCategory::Validation,
                405,
                "The requested operation is not supported.",
                false,
            ),
            "authorization/token_invalid" | "authorization/token_missing" => (
                code.to_owned(),
                ErrorCategory::Authorization,
                401,
                "Authentication is invalid or missing.",
                false,
            ),
            "authorization/host_not_allowed" => (
                code.to_owned(),
                ErrorCategory::Authorization,
                403,
                "The request host is not allowed.",
                false,
            ),
            "not_found/funnel" | "not_found/slug" | "not_found/revision" => (
                code.to_owned(),
                ErrorCategory::NotFound,
                404,
                "The requested resource was not found.",
                false,
            ),
            "not_found/route" => (
                code.to_owned(),
                ErrorCategory::NotFound,
                404,
                "The requested route was not found.",
                false,
            ),
            "conflict/publish_stale_revision"
            | "conflict/idempotency_payload_mismatch"
            | "conflict/funnel_limit_reached"
            | "conflict/edit_stale_revision"
            | "conflict/revision_limit_reached"
            | "conflict/v2_conversion_blocked"
            | "conflict/v2_publication_blocked"
            | "conflict/v2_typed_command_required"
            | "conflict/v2_stage_active" => (
                code.to_owned(),
                ErrorCategory::Conflict,
                409,
                if code == "conflict/funnel_limit_reached" {
                    "The workspace has reached its funnel limit."
                } else if code == "conflict/v2_conversion_blocked" {
                    "This legacy revision cannot be converted to V2 without losing content."
                } else {
                    "The request conflicts with the current state."
                },
                false,
            ),
            "rate_limit/exceeded" => (
                code.to_owned(),
                ErrorCategory::RateLimit,
                429,
                "Too many requests.",
                false,
            ),
            "external/timeout" => (
                code.to_owned(),
                ErrorCategory::External,
                504,
                "An external service timed out.",
                false,
            ),
            "external/unavailable" => (
                code.to_owned(),
                ErrorCategory::External,
                503,
                "An external service is unavailable.",
                false,
            ),
            "internal/unexpected" => (
                code.to_owned(),
                ErrorCategory::Internal,
                500,
                "An internal error occurred.",
                false,
            ),
            _ => (
                "unknown/unclassified".to_owned(),
                ErrorCategory::Unknown,
                500,
                "An unexpected error occurred.",
                true,
            ),
        };

        Self {
            code,
            category,
            status,
            message,
            alert,
        }
    }

    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn requires_alert(&self) -> bool {
        self.alert
    }

    pub fn response(&self, correlation_id: String) -> ErrorResponse {
        ErrorResponse {
            code: self.code.clone(),
            category: self.category,
            message: self.message,
            correlation_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ApiError, ErrorCategory};

    const CONTRACT_CODES: &[(&str, ErrorCategory, u16)] = &[
        ("validation/invalid_json", ErrorCategory::Validation, 400),
        ("validation/invalid_query", ErrorCategory::Validation, 400),
        ("validation/missing_field", ErrorCategory::Validation, 400),
        ("validation/order_duplicate", ErrorCategory::Validation, 400),
        (
            "validation/schema_version_unsupported",
            ErrorCategory::Validation,
            400,
        ),
        (
            "validation/schema_version_missing",
            ErrorCategory::Validation,
            400,
        ),
        (
            "validation/section_type_unknown",
            ErrorCategory::Validation,
            400,
        ),
        (
            "validation/section_missing_id",
            ErrorCategory::Validation,
            400,
        ),
        (
            "validation/operation_not_supported",
            ErrorCategory::Validation,
            405,
        ),
        (
            "authorization/token_invalid",
            ErrorCategory::Authorization,
            401,
        ),
        (
            "authorization/token_missing",
            ErrorCategory::Authorization,
            401,
        ),
        (
            "authorization/host_not_allowed",
            ErrorCategory::Authorization,
            403,
        ),
        ("not_found/funnel", ErrorCategory::NotFound, 404),
        ("not_found/slug", ErrorCategory::NotFound, 404),
        ("not_found/revision", ErrorCategory::NotFound, 404),
        ("not_found/route", ErrorCategory::NotFound, 404),
        (
            "conflict/publish_stale_revision",
            ErrorCategory::Conflict,
            409,
        ),
        (
            "conflict/idempotency_payload_mismatch",
            ErrorCategory::Conflict,
            409,
        ),
        ("conflict/edit_stale_revision", ErrorCategory::Conflict, 409),
        (
            "conflict/revision_limit_reached",
            ErrorCategory::Conflict,
            409,
        ),
        (
            "conflict/v2_conversion_blocked",
            ErrorCategory::Conflict,
            409,
        ),
        (
            "conflict/v2_publication_blocked",
            ErrorCategory::Conflict,
            409,
        ),
        (
            "conflict/v2_typed_command_required",
            ErrorCategory::Conflict,
            409,
        ),
        ("conflict/v2_stage_active", ErrorCategory::Conflict, 409),
        ("rate_limit/exceeded", ErrorCategory::RateLimit, 429),
        ("external/timeout", ErrorCategory::External, 504),
        ("external/unavailable", ErrorCategory::External, 503),
        ("internal/unexpected", ErrorCategory::Internal, 500),
        ("unknown/unclassified", ErrorCategory::Unknown, 500),
    ];

    #[test]
    fn every_contract_error_has_a_stable_category_and_status() {
        for (code, category, status) in CONTRACT_CODES {
            let error = ApiError::from_code(code);

            assert_eq!(error.code(), *code);
            assert_eq!(error.category, *category);
            assert_eq!(error.status(), *status);
        }
    }

    #[test]
    fn workspace_capacity_error_matches_the_d14_decision() {
        let error = ApiError::from_code("conflict/funnel_limit_reached");
        let response = error.response("request-789".to_owned());

        assert_eq!(error.status(), 409);
        assert_eq!(response.code, "conflict/funnel_limit_reached");
        assert_eq!(response.category, ErrorCategory::Conflict);
        assert_eq!(
            response.message,
            "The workspace has reached its funnel limit."
        );
        assert_eq!(response.correlation_id, "request-789");
    }

    #[test]
    fn internal_responses_hide_details_and_keep_correlation_id() {
        let error = ApiError::from_code("internal/unexpected");
        let body = error.response("request-123".to_owned());
        let serialized = serde_json::to_string(&body).unwrap_or_default();

        assert!(!serialized.contains("database password"));
        assert!(serialized.contains("An internal error occurred."));
        assert!(serialized.contains("request-123"));
    }

    #[test]
    fn unclassified_errors_require_an_alert() {
        assert!(ApiError::from_code("some/new/error").requires_alert());
        assert!(!ApiError::from_code("external/unavailable").requires_alert());
    }

    #[test]
    fn storage_failures_propagate_as_safe_external_errors() {
        let source = "private database connection detail";
        let error = ApiError::from_storage_error(&source);
        let response = error.response("request-456".to_owned());
        let serialized = serde_json::to_string(&response).unwrap_or_default();

        assert_eq!(error.code(), "external/unavailable");
        assert_eq!(error.status(), 503);
        assert!(!serialized.contains(source));
        assert!(serialized.contains("request-456"));
    }
}
