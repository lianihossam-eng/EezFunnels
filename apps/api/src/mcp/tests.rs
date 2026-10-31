use axum::http::{HeaderMap, HeaderValue};
use serde_json::{json, Value};

use super::{
    authorized, bounded_response_value, command_error_tool_result, command_parameters_bytes,
    has_json_content_type, has_streamable_http_accept, list_commands_result, mcp_http_error_parts,
    negotiate_protocol_version, parse_message, protocol_version_header, response_protocol_version,
    tools_list_result, ClientMessage, JsonRpcId, McpHttpError, ParseError, MAX_BATCH_MESSAGES,
    MAX_COMMAND_PARAMETERS_BYTES, MAX_RESPONSE_BYTES, MCP_PROTOCOL_VERSION, TOOL_SPECS,
};
use crate::errors::{ApiError, ErrorCategory};
use axum::http::StatusCode;
use eez_commands::command_registry;

#[test]
fn deterministic_mcp_json_fuzzes_ten_thousand_cases_without_panicking() {
    let inputs = crate::json::tests::deterministic_json_fuzz_inputs();
    assert_eq!(inputs.len(), 10_000);

    for (case_number, input) in inputs.iter().enumerate() {
        let result = std::panic::catch_unwind(|| parse_message(input, "2025-03-26"));
        assert!(
            result.is_ok(),
            "MCP JSON parser panicked for case {case_number}: {input:?}"
        );
    }
}

#[test]
fn accepts_string_and_integer_request_ids() {
    assert!(matches!(
        parse_message(r#"{"jsonrpc":"2.0","id":"client-1","method":"ping"}"#, "2025-03-26"),
        Ok(super::ParsedMessages { messages, is_batch: false })
            if matches!(messages.as_slice(), [ClientMessage::Request(super::RpcRequest { id: JsonRpcId::String(value), .. })] if value == "client-1")
    ));
    assert!(matches!(
        parse_message(r#"{"jsonrpc":"2.0","id":7,"method":"ping"}"#, "2025-03-26"),
        Ok(super::ParsedMessages { messages, is_batch: false })
            if matches!(messages.as_slice(), [ClientMessage::Request(super::RpcRequest { id: JsonRpcId::Number(value), .. })] if value.as_i64() == Some(7))
    ));
}

#[test]
fn absent_id_is_notification_and_explicit_null_is_invalid() {
    assert!(matches!(
        parse_message(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, "2025-03-26"),
        Ok(super::ParsedMessages { messages, is_batch: false })
            if matches!(messages.as_slice(), [ClientMessage::Notification(_)])
    ));
    assert!(matches!(
        parse_message(r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#, "2025-03-26"),
        Ok(super::ParsedMessages { messages, is_batch: false })
            if messages == vec![ClientMessage::InvalidRequest(None)]
    ));
}

#[test]
fn march_protocol_accepts_bounded_batches_and_june_rejects_them() {
    let batch = r#"[{"jsonrpc":"2.0","id":1,"method":"ping"},{"jsonrpc":"2.0","method":"notifications/initialized"},{"jsonrpc":"2.0","id":2,"method":"tools/list"}]"#;
    let parsed = parse_message(batch, "2025-03-26").expect("March supports JSON-RPC batches");
    assert!(parsed.is_batch);
    assert_eq!(parsed.messages.len(), 3);
    assert!(matches!(parsed.messages[0], ClientMessage::Request(_)));
    assert!(matches!(parsed.messages[1], ClientMessage::Notification(_)));
    assert!(matches!(parsed.messages[2], ClientMessage::Request(_)));
    assert_eq!(
        parse_message(batch, "2025-06-18"),
        Err(ParseError::InvalidRequest(None))
    );
    assert_eq!(
        parse_message("[]", "2025-03-26"),
        Err(ParseError::InvalidRequest(None))
    );
    let too_many = format!("[{}]", vec!["null"; MAX_BATCH_MESSAGES + 1].join(","));
    assert_eq!(
        parse_message(&too_many, "2025-03-26"),
        Err(ParseError::InvalidRequest(None))
    );
}

#[test]
fn march_batches_reject_initialize_and_mixed_responses_before_tool_dispatch() {
    let initialize_with_tool_call = r#"[{"jsonrpc":"2.0","id":"init","method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1"}}},{"jsonrpc":"2.0","id":"call","method":"tools/call","params":{"name":"execute_command","arguments":{"command_id":"funnel.create","parameters":{"name":"must-not-run","slug":"must-not-run"},"idempotency_key":"must-not-run"}}}]"#;
    assert_eq!(
        parse_message(initialize_with_tool_call, "2025-03-26"),
        Err(ParseError::InvalidRequest(None))
    );

    let invalid_initialize_with_tool_call = r#"[{"jsonrpc":"2.0","id":"init","method":"initialize","params":null},{"jsonrpc":"2.0","id":"call","method":"tools/call","params":{"name":"execute_command","arguments":{"command_id":"funnel.create","parameters":{"name":"must-not-run","slug":"must-not-run"},"idempotency_key":"must-not-run"}}}]"#;
    assert_eq!(
        parse_message(invalid_initialize_with_tool_call, "2025-03-26"),
        Err(ParseError::InvalidRequest(None))
    );

    let response_with_tool_call = r#"[{"jsonrpc":"2.0","id":"response-1","result":{}},{"jsonrpc":"2.0","id":"call","method":"tools/call","params":{"name":"execute_command","arguments":{"command_id":"funnel.create","parameters":{"name":"must-not-run","slug":"must-not-run"},"idempotency_key":"must-not-run"}}}]"#;
    assert_eq!(
        parse_message(response_with_tool_call, "2025-03-26"),
        Err(ParseError::InvalidRequest(None))
    );

    let response_with_notification = r#"[{"jsonrpc":"2.0","id":"response-1","result":{}},{"jsonrpc":"2.0","method":"notifications/initialized"}]"#;
    assert_eq!(
        parse_message(response_with_notification, "2025-03-26"),
        Err(ParseError::InvalidRequest(None))
    );
}

#[test]
fn march_batch_accepts_response_only_batches() {
    let responses = r#"[{"jsonrpc":"2.0","id":"response-1","result":{}},{"jsonrpc":"2.0","id":"response-2","error":{"code":-32600,"message":"Invalid request."}}]"#;
    let parsed = parse_message(responses, "2025-03-26").expect("response-only batch is valid");
    assert!(parsed.is_batch);
    assert_eq!(
        parsed.messages,
        vec![ClientMessage::Response, ClientMessage::Response]
    );
}

#[test]
fn negotiates_only_the_documented_protocol_subset() {
    assert_eq!(negotiate_protocol_version("2025-03-26"), "2025-03-26");
    assert_eq!(negotiate_protocol_version("2025-06-18"), "2025-06-18");
    assert_eq!(negotiate_protocol_version("2024-11-05"), "2025-06-18");
    assert_eq!(negotiate_protocol_version("2025-11-25"), "2025-06-18");
}

#[test]
fn validates_protocol_version_headers_and_uses_streamable_http_fallback() {
    let mut headers = HeaderMap::new();
    assert_eq!(protocol_version_header(&headers), Ok(None));

    headers.insert(MCP_PROTOCOL_VERSION, HeaderValue::from_static("2025-03-26"));
    assert_eq!(protocol_version_header(&headers), Ok(Some("2025-03-26")));
    headers.insert(MCP_PROTOCOL_VERSION, HeaderValue::from_static("2025-06-18"));
    assert_eq!(protocol_version_header(&headers), Ok(Some("2025-06-18")));
    headers.insert(MCP_PROTOCOL_VERSION, HeaderValue::from_static("2025-11-25"));
    assert_eq!(protocol_version_header(&headers), Err(()));

    headers.remove(MCP_PROTOCOL_VERSION);
    headers.append(MCP_PROTOCOL_VERSION, HeaderValue::from_static("2025-03-26"));
    headers.append(MCP_PROTOCOL_VERSION, HeaderValue::from_static("2025-06-18"));
    assert_eq!(protocol_version_header(&headers), Err(()));
}

#[test]
fn response_protocol_version_uses_init_negotiation_then_header_or_legacy_default() {
    let super::ParsedMessages { messages, .. } = parse_message(
        r#"{"jsonrpc":"2.0","id":"init","method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#,
        "2025-03-26",
    )
    .expect("valid initialize request");
    let Some(ClientMessage::Request(request)) = messages.into_iter().next() else {
        panic!("initialize is a request");
    };
    assert_eq!(
        response_protocol_version(&request, Some("2025-06-18")),
        "2025-03-26"
    );

    let super::ParsedMessages { messages, .. } = parse_message(
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        "2025-03-26",
    )
    .expect("valid tools/list request");
    let Some(ClientMessage::Request(request)) = messages.into_iter().next() else {
        panic!("tools/list is a request");
    };
    assert_eq!(
        response_protocol_version(&request, Some("2025-06-18")),
        "2025-06-18"
    );
    assert_eq!(response_protocol_version(&request, None), "2025-03-26");
}

#[test]
fn duplicate_json_keys_are_parse_errors() {
    assert_eq!(
        parse_message(
            r#"{"jsonrpc":"2.0","id":3,"method":"ping","params":{"x":1,"x":2}}"#,
            "2025-03-26"
        ),
        Err(ParseError::Parse)
    );
}

#[test]
fn tool_inventory_is_fixed_and_command_discovery_matches_registry() {
    assert_eq!(TOOL_SPECS.len(), 4);
    let result = tools_list_result();
    let listed = result
        .get("tools")
        .and_then(Value::as_array)
        .expect("tool list array");
    assert_eq!(listed.len(), TOOL_SPECS.len());
    let commands = command_registry();
    let listed_commands = list_commands_result();
    let listed = listed_commands
        .get("commands")
        .and_then(Value::as_array)
        .expect("command list array");
    assert_eq!(commands.len(), 23);
    assert_eq!(listed.len(), 9);
    let ids = listed
        .iter()
        .filter_map(|command| command.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert!(ids.contains(&"funnel.public.read"));
    for hidden in [
        "funnel.create.v2",
        "funnel.document.edit.v2",
        "funnel.document.conversion.preview.v2",
        "funnel.document.conversion.commit.v2",
        "funnel.document.read.v2",
        "funnel.document.preview.v2",
        "funnel.document.stage.v2",
        "form.submission.list",
        "form.submission.status.set",
        "form.completion.report",
        "interaction.report.read",
        "service.outcome.report.read",
        "funnel.revision.save.v2",
        "funnel.revision.history.read.v2",
    ] {
        assert!(!ids.contains(&hidden));
        assert!(!super::mcp_command_is_executable(hidden));
    }
    assert!(super::mcp_command_is_executable("funnel.create"));
    assert!(!super::mcp_command_is_executable("funnel.missing"));
}

#[test]
fn authorization_and_content_type_require_single_valid_headers() {
    let mut headers = HeaderMap::new();
    headers.append(
        "authorization",
        HeaderValue::from_static("Bearer token-value"),
    );
    assert!(authorized(&headers, "token-value"));
    headers.remove("authorization");
    headers.insert(
        "authorization",
        HeaderValue::from_static("bearer token-value"),
    );
    assert!(authorized(&headers, "token-value"));
    headers.insert(
        "authorization",
        HeaderValue::from_static("bEaReR  token-value"),
    );
    assert!(authorized(&headers, "token-value"));
    headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer token-value "),
    );
    assert!(!authorized(&headers, "token-value"));
    headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer wrong-token"),
    );
    headers.append(
        "authorization",
        HeaderValue::from_static("Bearer token-value"),
    );
    assert!(!authorized(&headers, "token-value"));

    let mut content_type = HeaderMap::new();
    content_type.append("content-type", HeaderValue::from_static("application/json"));
    assert!(has_json_content_type(&content_type));
    content_type.append("content-type", HeaderValue::from_static("text/plain"));
    assert!(!has_json_content_type(&content_type));
    content_type.clear();
    content_type.insert(
        "content-type",
        HeaderValue::from_static("application/json; charset=utf-8, text/plain"),
    );
    assert!(!has_json_content_type(&content_type));
}

#[test]
fn streamable_http_accept_requires_both_json_and_event_stream() {
    let mut accept = HeaderMap::new();
    assert!(!has_streamable_http_accept(&accept));

    accept.insert("accept", HeaderValue::from_static("application/json"));
    assert!(!has_streamable_http_accept(&accept));

    accept.insert("accept", HeaderValue::from_static("text/event-stream"));
    assert!(!has_streamable_http_accept(&accept));

    accept.insert("accept", HeaderValue::from_static("*/*"));
    assert!(!has_streamable_http_accept(&accept));

    accept.insert(
        "accept",
        HeaderValue::from_static("application/json;q=0, text/event-stream"),
    );
    assert!(!has_streamable_http_accept(&accept));

    accept.insert(
        "accept",
        HeaderValue::from_static("Application/JSON; q=0.8, text/event-stream"),
    );
    assert!(has_streamable_http_accept(&accept));

    accept.insert(
        "accept",
        HeaderValue::from_static("application/json, text/event-stream"),
    );
    accept.append("accept", HeaderValue::from_static("text/plain"));
    assert!(has_streamable_http_accept(&accept));
}

#[test]
fn serialized_command_parameters_enforce_the_limit_at_the_boundary() {
    let within_limit = json!({
        "value": "x".repeat(MAX_COMMAND_PARAMETERS_BYTES - 12),
    });
    assert_eq!(
        command_parameters_bytes(&within_limit)
            .expect("parameters at the limit are accepted")
            .len(),
        MAX_COMMAND_PARAMETERS_BYTES
    );

    let over_limit = json!({
        "value": "x".repeat(MAX_COMMAND_PARAMETERS_BYTES - 11),
    });
    assert!(command_parameters_bytes(&over_limit).is_err());
}

#[test]
fn oversized_tool_result_is_replaced_with_a_complete_safe_error() {
    let large_value =
        json!({ "content": [{ "type": "text", "text": "x".repeat(MAX_RESPONSE_BYTES) }] });
    let response = bounded_response_value(json!("request-1"), large_value, true, "correlation-1")
        .expect("tool response has a bounded fallback");
    let response = serde_json::to_vec(&response).expect("complete JSON-RPC response");
    assert!(response.len() <= MAX_RESPONSE_BYTES);
    let response = serde_json::from_slice::<Value>(&response).expect("complete JSON-RPC response");
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("fallback text");
    assert!(text.contains("response/too_large"));
    assert!(!text.contains(&"x".repeat(128)));
}

#[test]
fn command_tool_errors_preserve_the_c04_error_fields() {
    let response = command_error_tool_result(
        &ApiError::from_code("validation/missing_field"),
        "request-123",
    );
    assert_eq!(response["isError"], true);
    let text = response["content"][0]["text"]
        .as_str()
        .expect("tool error text");
    let error = serde_json::from_str::<Value>(text).expect("complete C-04 error payload");
    assert_eq!(error["code"], "validation/missing_field");
    assert_eq!(error["category"], "validation");
    assert_eq!(error["message"], "The request is invalid.");
    assert_eq!(error["correlation_id"], "request-123");
}

#[test]
fn mcp_http_errors_emit_exact_c04_envelopes_for_every_status_path() {
    let expected = [
        (
            McpHttpError::InvalidOrigin,
            StatusCode::FORBIDDEN,
            "request/invalid_origin",
            ErrorCategory::Validation,
            "The request Origin is not allowed.",
        ),
        (
            McpHttpError::InvalidPath,
            StatusCode::BAD_REQUEST,
            "request/invalid_path",
            ErrorCategory::Validation,
            "Only the exact /mcp path is supported.",
        ),
        (
            McpHttpError::UnsupportedMediaType,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "request/unsupported_media_type",
            ErrorCategory::Validation,
            "Content-Type must be application/json.",
        ),
        (
            McpHttpError::UnsupportedAccept,
            StatusCode::NOT_ACCEPTABLE,
            "request/unsupported_accept",
            ErrorCategory::Validation,
            "Accept must include application/json and text/event-stream.",
        ),
        (
            McpHttpError::InvalidProtocolVersion,
            StatusCode::BAD_REQUEST,
            "request/invalid_protocol_version",
            ErrorCategory::Validation,
            "The MCP protocol version is unsupported.",
        ),
        (
            McpHttpError::RequestTooLarge,
            StatusCode::PAYLOAD_TOO_LARGE,
            "request/too_large",
            ErrorCategory::Validation,
            "The request body exceeds the configured size limit.",
        ),
        (
            McpHttpError::ResponseTooLarge,
            StatusCode::INTERNAL_SERVER_ERROR,
            "response/too_large",
            ErrorCategory::Internal,
            "The response exceeds the configured size limit.",
        ),
        (
            McpHttpError::InternalUnexpected,
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal/unexpected",
            ErrorCategory::Internal,
            "The request could not be completed.",
        ),
    ];

    assert_eq!(McpHttpError::ALL.len(), expected.len());
    for (error_kind, status, code, category, message) in expected {
        assert!(McpHttpError::ALL.contains(&error_kind));
        assert_eq!(error_kind.details(), (status, code, category, message));

        let (actual_status, body) = mcp_http_error_parts(error_kind, "request-456");
        assert_eq!(actual_status, status);
        let body_text = std::str::from_utf8(&body).expect("JSON error body is UTF-8");
        assert!(!body_text.contains("secret-token"));
        assert!(!body_text.contains("sensitive request body"));
        let value = serde_json::from_slice::<Value>(&body).expect("C-04 JSON error envelope");
        assert_eq!(value.as_object().map(|object| object.len()), Some(4));
        assert_eq!(value["code"], code);
        assert_eq!(value["category"], serde_json::to_value(category).unwrap());
        assert_eq!(value["message"], message);
        assert_eq!(value["correlation_id"], "request-456");
    }
}
