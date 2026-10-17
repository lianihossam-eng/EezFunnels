use axum::body::{to_bytes, Body};
use axum::http::header::{ACCEPT, ALLOW, AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::response::Response;
use eez_commands::{command_ids, command_registry, CommandActor, IdempotencyPolicy, McpExposure};
use serde::Serialize;
use serde_json::{Map, Number, Value};
use worker::{Env, HttpRequest};

use crate::errors::{ApiError, ErrorCategory};
use crate::json::{validate_unique_json, JsonValidationError};

const MAX_REQUEST_BYTES: usize = 32 * 1024;
const MAX_COMMAND_PARAMETERS_BYTES: usize = 16 * 1024;
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const MAX_BATCH_MESSAGES: usize = 64;
const MCP_PROTOCOL_VERSION: HeaderName = HeaderName::from_static("mcp-protocol-version");
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26"];
const LATEST_PROTOCOL_VERSION: &str = "2025-06-18";
const DEFAULT_PROTOCOL_VERSION: &str = "2025-03-26";

#[derive(Clone, Debug, PartialEq)]
enum JsonRpcId {
    Number(Number),
    String(String),
}

impl JsonRpcId {
    fn as_value(&self) -> Value {
        match self {
            Self::Number(value) => Value::Number(value.clone()),
            Self::String(value) => Value::String(value.clone()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct RpcRequest {
    id: JsonRpcId,
    method: String,
    params: Option<Value>,
}

#[derive(Clone, Debug, PartialEq)]
struct RpcNotification {
    method: String,
    params: Option<Value>,
}

#[derive(Clone, Debug, PartialEq)]
enum ClientMessage {
    Request(RpcRequest),
    Notification(RpcNotification),
    Response,
    InvalidRequest(Option<Value>),
}

#[derive(Clone, Debug, PartialEq)]
struct ParsedMessages {
    messages: Vec<ClientMessage>,
    is_batch: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum ParseError {
    Parse,
    InvalidRequest(Option<Value>),
}

#[derive(Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

impl RpcResponse {
    fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    fn failure(id: Value, code: i32, message: &'static str) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(RpcError { code, message }),
        }
    }
}

#[derive(Serialize)]
struct RpcError {
    code: i32,
    message: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ToolOperation {
    ListCommands,
    ExecuteCommand,
    ListFunnels,
    GetFunnel,
}

#[derive(Clone, Copy)]
struct ToolSpec {
    name: &'static str,
    description: &'static str,
    operation: ToolOperation,
    schema: fn() -> Value,
    read_only: bool,
}

const TOOL_SPECS: [ToolSpec; 4] = [
    ToolSpec {
        name: "list_commands",
        description:
            "List the executable command registry, including actors and idempotency rules.",
        operation: ToolOperation::ListCommands,
        schema: empty_object_schema,
        read_only: true,
    },
    ToolSpec {
        name: "execute_command",
        description: "Execute a registered funnel command through the shared command session.",
        operation: ToolOperation::ExecuteCommand,
        schema: execute_command_schema,
        read_only: false,
    },
    ToolSpec {
        name: "list_funnels",
        description: "List funnels in the workspace.",
        operation: ToolOperation::ListFunnels,
        schema: empty_object_schema,
        read_only: true,
    },
    ToolSpec {
        name: "get_funnel",
        description: "Read an editable funnel and its current revision.",
        operation: ToolOperation::GetFunnel,
        schema: get_funnel_schema,
        read_only: true,
    },
];

pub(crate) async fn handle(
    env: &Env,
    request: HttpRequest,
    correlation_id: &str,
) -> Response<Body> {
    let expected_token = match env.secret("EEZ_MCP_TOKEN") {
        Ok(secret) => secret.to_string(),
        Err(_) => return api_error_response("external/unavailable", correlation_id),
    };
    if expected_token.is_empty() {
        return api_error_response("external/unavailable", correlation_id);
    }
    if !authorized(request.headers(), &expected_token) {
        return api_error_response("authorization/token_invalid", correlation_id);
    }
    if !crate::origin::matches_request_host(request.headers(), request.uri().scheme_str()) {
        return mcp_http_error(McpHttpError::InvalidOrigin, correlation_id);
    }
    if request.uri().path() != "/mcp" || request.uri().query().is_some() {
        return mcp_http_error(McpHttpError::InvalidPath, correlation_id);
    }
    if request.method() != Method::POST {
        let mut response = api_error_response("validation/operation_not_supported", correlation_id);
        response
            .headers_mut()
            .insert(ALLOW, HeaderValue::from_static("POST"));
        return response;
    }
    if !has_json_content_type(request.headers()) {
        return mcp_http_error(McpHttpError::UnsupportedMediaType, correlation_id);
    }
    if !has_streamable_http_accept(request.headers()) {
        return mcp_http_error(McpHttpError::UnsupportedAccept, correlation_id);
    }
    let request_protocol_version = match protocol_version_header(request.headers()) {
        Ok(version) => version,
        Err(()) => return mcp_http_error(McpHttpError::InvalidProtocolVersion, correlation_id),
    };

    let body = match to_bytes(Body::new(request.into_body()), MAX_REQUEST_BYTES).await {
        Ok(body) => body,
        Err(_) => return mcp_http_error(McpHttpError::RequestTooLarge, correlation_id),
    };
    let body = match std::str::from_utf8(&body) {
        Ok(body) => body,
        Err(_) => {
            return json_rpc_error_response(
                ParseError::Parse,
                request_protocol_version.unwrap_or(DEFAULT_PROTOCOL_VERSION),
                correlation_id,
            )
        }
    };
    let protocol_version = request_protocol_version.unwrap_or(DEFAULT_PROTOCOL_VERSION);
    let messages = match parse_message(body, protocol_version) {
        Ok(messages) => messages,
        Err(error) => return json_rpc_error_response(error, protocol_version, correlation_id),
    };

    let response_protocol_version = messages
        .messages
        .iter()
        .find_map(|message| match message {
            ClientMessage::Request(request) => {
                Some(response_protocol_version(request, request_protocol_version))
            }
            _ => None,
        })
        .unwrap_or(protocol_version);
    let mut responses = Vec::new();
    for message in messages.messages {
        match message {
            ClientMessage::Request(request) => {
                let is_tool_call = request.method == "tools/call";
                let response = match dispatch_request(env, request, correlation_id).await {
                    Ok((id, result)) => {
                        bounded_response_value(id, result, is_tool_call, correlation_id)
                    }
                    Err((id, code, message)) => bounded_error_response_value(id, code, message),
                };
                let Some(response) = response else {
                    return mcp_http_error(McpHttpError::ResponseTooLarge, correlation_id);
                };
                responses.push(response);
            }
            ClientMessage::InvalidRequest(id) => {
                let response = bounded_error_response_value(
                    id.unwrap_or(Value::Null),
                    -32600,
                    "Invalid request.",
                );
                let Some(response) = response else {
                    return mcp_http_error(McpHttpError::ResponseTooLarge, correlation_id);
                };
                responses.push(response);
            }
            ClientMessage::Notification(notification) => {
                let _ = (notification.method, notification.params);
            }
            ClientMessage::Response => {}
        }
    }
    if responses.is_empty() {
        return empty_response_with_protocol(StatusCode::ACCEPTED, response_protocol_version);
    }
    let response_value = if messages.is_batch {
        Value::Array(responses)
    } else {
        responses.pop().unwrap_or(Value::Null)
    };
    let response = serde_json::to_vec(&response_value).ok();
    match response.filter(|bytes| bytes.len() <= MAX_RESPONSE_BYTES) {
        Some(bytes) => {
            json_bytes_response_with_protocol(StatusCode::OK, bytes, response_protocol_version)
        }
        None => mcp_http_error(McpHttpError::ResponseTooLarge, correlation_id),
    }
}

async fn dispatch_request(
    env: &Env,
    request: RpcRequest,
    correlation_id: &str,
) -> Result<(Value, Value), (Value, i32, &'static str)> {
    let id = request.id.as_value();
    match request.method.as_str() {
        "initialize" => initialize_result(&request.params)
            .map(|result| (id.clone(), result))
            .map_err(|()| (id, -32602, "Invalid params.")),
        "ping" => match optional_object(&request.params) {
            Some(_) => Ok((id, serde_json::json!({}))),
            None => Err((id, -32602, "Invalid params.")),
        },
        "tools/list" => match optional_object(&request.params) {
            Some(_) => Ok((id, tools_list_result())),
            None => Err((id, -32602, "Invalid params.")),
        },
        "tools/call" => {
            let Some(params) = optional_object(&request.params) else {
                return Err((id, -32602, "Invalid params."));
            };
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Err((id, -32602, "Invalid params."));
            };
            let arguments = match params.get("arguments") {
                None => Map::new(),
                Some(Value::Object(arguments)) => arguments.clone(),
                Some(_) => return Err((id, -32602, "Invalid params.")),
            };
            let result = match TOOL_SPECS.iter().find(|tool| tool.name == name) {
                Some(tool) => call_tool(env, tool.operation, &arguments, correlation_id).await,
                None => return Err((id, -32602, "Unknown tool.")),
            };
            Ok((id, result))
        }
        _ => Err((id, -32601, "Method not found.")),
    }
}

fn initialize_result(params: &Option<Value>) -> Result<Value, ()> {
    let params = optional_object(params).ok_or(())?;
    let version = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .ok_or(())?;
    if params
        .get("capabilities")
        .and_then(Value::as_object)
        .is_none()
    {
        return Err(());
    }
    let client_info = params
        .get("clientInfo")
        .and_then(Value::as_object)
        .ok_or(())?;
    if client_info.get("name").and_then(Value::as_str).is_none()
        || client_info.get("version").and_then(Value::as_str).is_none()
    {
        return Err(());
    }
    Ok(serde_json::json!({
        "protocolVersion": negotiate_protocol_version(version),
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": "eez-funnel",
            "version": env!("CARGO_PKG_VERSION"),
        },
    }))
}

fn negotiate_protocol_version(client_version: &str) -> &'static str {
    SUPPORTED_PROTOCOL_VERSIONS
        .iter()
        .copied()
        .find(|version| *version == client_version)
        .unwrap_or(LATEST_PROTOCOL_VERSION)
}

fn protocol_version_header(headers: &HeaderMap<HeaderValue>) -> Result<Option<&'static str>, ()> {
    let mut values = headers.get_all(&MCP_PROTOCOL_VERSION).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(());
    }
    let value = value.to_str().map_err(|_| ())?;
    SUPPORTED_PROTOCOL_VERSIONS
        .iter()
        .copied()
        .find(|supported| *supported == value)
        .map(Some)
        .ok_or(())
}

fn response_protocol_version(
    request: &RpcRequest,
    request_protocol_version: Option<&'static str>,
) -> &'static str {
    if request.method == "initialize" {
        request
            .params
            .as_ref()
            .and_then(|params| params.get("protocolVersion"))
            .and_then(Value::as_str)
            .map(negotiate_protocol_version)
            .unwrap_or(LATEST_PROTOCOL_VERSION)
    } else {
        request_protocol_version.unwrap_or(DEFAULT_PROTOCOL_VERSION)
    }
}

fn tools_list_result() -> Value {
    let tools = TOOL_SPECS
        .iter()
        .map(|tool| {
            serde_json::json!({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": (tool.schema)(),
                "annotations": {
                    "readOnlyHint": tool.read_only,
                    "destructiveHint": !tool.read_only,
                    "idempotentHint": tool.read_only,
                    "openWorldHint": false,
                },
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({ "tools": tools })
}

async fn call_tool(
    env: &Env,
    operation: ToolOperation,
    arguments: &Map<String, Value>,
    correlation_id: &str,
) -> Value {
    match operation {
        ToolOperation::ListCommands => {
            if !arguments.is_empty() {
                return tool_error(
                    "validation/invalid_json",
                    "validation",
                    "The tool arguments are invalid.",
                    correlation_id,
                );
            }
            tool_success(list_commands_result())
        }
        ToolOperation::ExecuteCommand => {
            if !contains_only_keys(arguments, &["command_id", "parameters", "idempotency_key"]) {
                return tool_error(
                    "validation/invalid_json",
                    "validation",
                    "The tool arguments are invalid.",
                    correlation_id,
                );
            }
            let (Some(command_id), Some(parameters)) = (
                arguments.get("command_id").and_then(Value::as_str),
                arguments.get("parameters").and_then(Value::as_object),
            ) else {
                return tool_error(
                    "validation/missing_field",
                    "validation",
                    "Required tool arguments are missing.",
                    correlation_id,
                );
            };
            let idempotency_key = match arguments.get("idempotency_key") {
                None => None,
                Some(Value::String(key)) => Some(key.as_str()),
                Some(_) => {
                    return tool_error(
                        "validation/invalid_json",
                        "validation",
                        "The tool arguments are invalid.",
                        correlation_id,
                    )
                }
            };
            execute_registered_command(
                env,
                command_id,
                &Value::Object(parameters.clone()),
                idempotency_key,
                correlation_id,
            )
            .await
        }
        ToolOperation::ListFunnels => {
            if !arguments.is_empty() {
                return tool_error(
                    "validation/invalid_json",
                    "validation",
                    "The tool arguments are invalid.",
                    correlation_id,
                );
            }
            execute_registered_command(
                env,
                command_ids::FUNNEL_LIST,
                &Value::Object(Map::new()),
                None,
                correlation_id,
            )
            .await
        }
        ToolOperation::GetFunnel => {
            if !contains_only_keys(arguments, &["funnel_id"]) {
                return tool_error(
                    "validation/invalid_json",
                    "validation",
                    "The tool arguments are invalid.",
                    correlation_id,
                );
            }
            let Some(funnel_id) = arguments.get("funnel_id").and_then(Value::as_str) else {
                return tool_error(
                    "validation/missing_field",
                    "validation",
                    "Required tool arguments are missing.",
                    correlation_id,
                );
            };
            let parameters = serde_json::json!({ "funnel_id": funnel_id });
            execute_registered_command(
                env,
                command_ids::FUNNEL_EDITOR_READ,
                &parameters,
                None,
                correlation_id,
            )
            .await
        }
    }
}

async fn execute_registered_command(
    env: &Env,
    command_id: &str,
    parameters: &Value,
    idempotency_key: Option<&str>,
    correlation_id: &str,
) -> Value {
    if !mcp_command_is_executable(command_id) {
        return tool_error(
            "validation/operation_not_supported",
            "validation",
            "The requested operation is not supported.",
            correlation_id,
        );
    }
    let parameters = match command_parameters_bytes(parameters) {
        Ok(parameters) => parameters,
        Err(()) => {
            return tool_error(
                "request/parameters_too_large",
                "validation",
                "The command parameters exceed the configured size limit.",
                correlation_id,
            )
        }
    };
    let parameters = match std::str::from_utf8(&parameters) {
        Ok(parameters) => parameters,
        Err(_) => {
            return tool_error(
                "internal/unexpected",
                "internal",
                "The command could not be completed.",
                correlation_id,
            )
        }
    };
    match super::execute_funnel_command(env, command_id, parameters, idempotency_key).await {
        Ok(result) => tool_success(result),
        Err(error) => command_error_tool_result(&error, correlation_id),
    }
}

fn mcp_command_is_executable(command_id: &str) -> bool {
    command_registry()
        .iter()
        .find(|command| command.id() == command_id)
        .is_some_and(|command| command.mcp_exposure() == McpExposure::Executable)
}

fn command_parameters_bytes(parameters: &Value) -> Result<Vec<u8>, ()> {
    let bytes = serde_json::to_vec(parameters).map_err(|_| ())?;
    if bytes.len() > MAX_COMMAND_PARAMETERS_BYTES {
        return Err(());
    }
    Ok(bytes)
}

fn tool_success(value: Value) -> Value {
    tool_result(value, false)
}

fn tool_error(code: &str, category: &str, message: &str, correlation_id: &str) -> Value {
    tool_result(
        serde_json::json!({
            "code": code,
            "category": category,
            "message": message,
            "correlation_id": correlation_id,
        }),
        true,
    )
}

fn command_error_tool_result(error: &ApiError, correlation_id: &str) -> Value {
    let payload =
        serde_json::to_value(error.response(correlation_id.to_owned())).unwrap_or_else(|_| {
            serde_json::json!({
                "code": "internal/unexpected",
                "category": "internal",
                "message": "The command could not be completed.",
                "correlation_id": correlation_id,
            })
        });
    tool_result(payload, true)
}

fn list_commands_result() -> Value {
    let commands = command_registry()
        .iter()
        .filter(|command| command.mcp_exposure() == McpExposure::Executable)
        .map(|command| {
            let actor = match command.actor() {
                CommandActor::LocalUser => "local_user",
                CommandActor::Public => "public",
            };
            let idempotency = match command.idempotency_policy() {
                IdempotencyPolicy::Required => "required",
                IdempotencyPolicy::NotRequired => "not_required",
            };
            serde_json::json!({
                "id": command.id(),
                "description": command.description(),
                "actor": actor,
                "idempotency": idempotency,
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({ "commands": commands })
}

fn tool_result(value: Value, is_error: bool) -> Value {
    let text = match serde_json::to_string(&value) {
        Ok(text) => text,
        Err(_) => String::from("{\"code\":\"internal/unexpected\",\"category\":\"internal\",\"message\":\"The command could not be completed.\",\"correlation_id\":\"correlation-id-unavailable\"}"),
    };
    serde_json::json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error,
    })
}

fn bounded_response_value(
    id: Value,
    result: Value,
    is_tool_call: bool,
    correlation_id: &str,
) -> Option<Value> {
    let response = serde_json::to_value(RpcResponse::success(id.clone(), result)).ok()?;
    let bytes = serde_json::to_vec(&response).ok()?;
    if bytes.len() <= MAX_RESPONSE_BYTES {
        return Some(response);
    }
    if !is_tool_call {
        return None;
    }
    let result = tool_error(
        "response/too_large",
        "internal",
        "The tool result exceeds the configured size limit.",
        correlation_id,
    );
    let fallback = serde_json::to_value(RpcResponse::success(id, result)).ok()?;
    let bytes = serde_json::to_vec(&fallback).ok()?;
    (bytes.len() <= MAX_RESPONSE_BYTES).then_some(fallback)
}

fn bounded_error_response_value(id: Value, code: i32, message: &'static str) -> Option<Value> {
    let response = serde_json::to_value(RpcResponse::failure(id, code, message)).ok()?;
    let bytes = serde_json::to_vec(&response).ok()?;
    (bytes.len() <= MAX_RESPONSE_BYTES).then_some(response)
}

fn parse_message(input: &str, protocol_version: &str) -> Result<ParsedMessages, ParseError> {
    validate_unique_json(input).map_err(|error| match error {
        JsonValidationError::Invalid | JsonValidationError::DuplicateObjectKey => ParseError::Parse,
    })?;
    let value = serde_json::from_str::<Value>(input).map_err(|_| ParseError::Parse)?;
    if let Some(batch) = value.as_array() {
        if protocol_version != "2025-03-26" || batch.is_empty() || batch.len() > MAX_BATCH_MESSAGES
        {
            return Err(ParseError::InvalidRequest(None));
        }
        let contains_initialize = batch.iter().any(|message| {
            message
                .get("method")
                .and_then(Value::as_str)
                .is_some_and(|method| method == "initialize")
        });
        let messages = batch.iter().map(parse_message_value).collect::<Vec<_>>();
        let has_response = messages
            .iter()
            .any(|message| matches!(message, ClientMessage::Response));
        let is_response_only = messages
            .iter()
            .all(|message| matches!(message, ClientMessage::Response));
        if contains_initialize || (has_response && !is_response_only) {
            return Err(ParseError::InvalidRequest(None));
        }
        return Ok(ParsedMessages {
            messages,
            is_batch: true,
        });
    }
    Ok(ParsedMessages {
        messages: vec![parse_message_value(&value)],
        is_batch: false,
    })
}

fn parse_message_value(value: &Value) -> ClientMessage {
    let Some(object) = value.as_object() else {
        return ClientMessage::InvalidRequest(None);
    };
    let id = match object.get("id") {
        None => None,
        Some(Value::String(value)) => Some(JsonRpcId::String(value.clone())),
        Some(Value::Number(value)) if value.is_i64() || value.is_u64() => {
            Some(JsonRpcId::Number(value.clone()))
        }
        Some(_) => return ClientMessage::InvalidRequest(None),
    };
    let error_id = id.as_ref().map(JsonRpcId::as_value);
    if object.get("jsonrpc") != Some(&Value::String(String::from("2.0"))) {
        return ClientMessage::InvalidRequest(error_id);
    }
    let params = object.get("params").cloned();
    if params
        .as_ref()
        .is_some_and(|params| !params.is_object() && !params.is_array())
    {
        return ClientMessage::InvalidRequest(error_id);
    }
    if let Some(method) = object.get("method").and_then(Value::as_str) {
        return match id {
            Some(id) => ClientMessage::Request(RpcRequest {
                id,
                method: method.to_owned(),
                params,
            }),
            None => ClientMessage::Notification(RpcNotification {
                method: method.to_owned(),
                params,
            }),
        };
    }
    let has_result = object.contains_key("result");
    let has_error = object.contains_key("error");
    let valid_error = object
        .get("error")
        .and_then(Value::as_object)
        .is_some_and(|error| {
            error.get("code").and_then(Value::as_i64).is_some()
                && error.get("message").and_then(Value::as_str).is_some()
        });
    if id.is_some() && has_result != has_error && (!has_error || valid_error) {
        ClientMessage::Response
    } else {
        ClientMessage::InvalidRequest(error_id)
    }
}

fn json_rpc_error_response(
    error: ParseError,
    protocol_version: &'static str,
    correlation_id: &str,
) -> Response<Body> {
    let response = match error {
        ParseError::Parse => RpcResponse::failure(Value::Null, -32700, "Parse error."),
        ParseError::InvalidRequest(id) => {
            RpcResponse::failure(id.unwrap_or(Value::Null), -32600, "Invalid request.")
        }
    };
    match serde_json::to_vec(&response) {
        Ok(bytes) => json_bytes_response_with_protocol(StatusCode::OK, bytes, protocol_version),
        Err(_) => mcp_http_error(McpHttpError::InternalUnexpected, correlation_id),
    }
}

fn authorized(headers: &HeaderMap<HeaderValue>, expected: &str) -> bool {
    let mut values = headers.get_all(AUTHORIZATION).iter();
    let Some(value) = values.next() else {
        return false;
    };
    if values.next().is_some() {
        return false;
    }
    value.to_str().ok().is_some_and(|value| {
        let Some((scheme, credentials)) = value.split_once(' ') else {
            return false;
        };
        let supplied = credentials.trim_start_matches(' ');
        scheme.eq_ignore_ascii_case("Bearer") && !supplied.is_empty() && supplied == expected
    })
}

fn has_json_content_type(headers: &HeaderMap<HeaderValue>) -> bool {
    let mut values = headers.get_all(CONTENT_TYPE).iter();
    let Some(value) = values.next() else {
        return false;
    };
    if values.next().is_some() {
        return false;
    }
    value.to_str().ok().is_some_and(|value| {
        if value.contains(',') {
            return false;
        }
        value
            .split(';')
            .next()
            .is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case("application/json"))
    })
}

fn has_streamable_http_accept(headers: &HeaderMap<HeaderValue>) -> bool {
    let mut has_json = false;
    let mut has_event_stream = false;
    let mut saw_accept_header = false;
    for value in headers.get_all(ACCEPT).iter() {
        saw_accept_header = true;
        let Ok(value) = value.to_str() else {
            return false;
        };
        for media_range in value.split(',') {
            let mut parts = media_range.trim().split(';');
            let Some(media_type) = parts.next() else {
                continue;
            };
            let media_type = media_type.trim();
            let target = if media_type.eq_ignore_ascii_case("application/json") {
                Some(&mut has_json)
            } else if media_type.eq_ignore_ascii_case("text/event-stream") {
                Some(&mut has_event_stream)
            } else {
                None
            };
            let Some(target) = target else {
                continue;
            };
            let mut quality = 1.0_f32;
            let mut has_quality = false;
            let mut valid = true;
            for parameter in parts {
                let Some((name, parameter_value)) = parameter.trim().split_once('=') else {
                    valid = false;
                    break;
                };
                if name.trim().eq_ignore_ascii_case("q") {
                    if has_quality {
                        valid = false;
                        break;
                    }
                    has_quality = true;
                    let Ok(parsed_quality) = parameter_value.trim().parse::<f32>() else {
                        valid = false;
                        break;
                    };
                    if !parsed_quality.is_finite() || !(0.0..=1.0).contains(&parsed_quality) {
                        valid = false;
                        break;
                    }
                    quality = parsed_quality;
                }
            }
            if valid && quality > 0.0 {
                *target = true;
            }
        }
    }
    saw_accept_header && has_json && has_event_stream
}

fn optional_object(params: &Option<Value>) -> Option<Map<String, Value>> {
    match params {
        None => Some(Map::new()),
        Some(Value::Object(object)) => Some(object.clone()),
        Some(_) => None,
    }
}

fn contains_only_keys(object: &Map<String, Value>, allowed: &[&str]) -> bool {
    object.keys().all(|key| allowed.contains(&key.as_str()))
}

fn empty_object_schema() -> Value {
    serde_json::json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

fn execute_command_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "command_id": { "type": "string" },
            "parameters": { "type": "object" },
            "idempotency_key": { "type": "string" },
        },
        "required": ["command_id", "parameters"],
        "additionalProperties": false,
    })
}

fn get_funnel_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": { "funnel_id": { "type": "string" } },
        "required": ["funnel_id"],
        "additionalProperties": false,
    })
}

fn api_error_response(code: &str, correlation_id: &str) -> Response<Body> {
    let error = ApiError::from_code(code);
    super::error_response(&error, correlation_id.to_owned())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum McpHttpError {
    InvalidOrigin,
    InvalidPath,
    UnsupportedMediaType,
    UnsupportedAccept,
    InvalidProtocolVersion,
    RequestTooLarge,
    ResponseTooLarge,
    InternalUnexpected,
}

impl McpHttpError {
    #[cfg(test)]
    const ALL: [Self; 8] = [
        Self::InvalidOrigin,
        Self::InvalidPath,
        Self::UnsupportedMediaType,
        Self::UnsupportedAccept,
        Self::InvalidProtocolVersion,
        Self::RequestTooLarge,
        Self::ResponseTooLarge,
        Self::InternalUnexpected,
    ];

    fn details(self) -> (StatusCode, &'static str, ErrorCategory, &'static str) {
        match self {
            Self::InvalidOrigin => (
                StatusCode::FORBIDDEN,
                "request/invalid_origin",
                ErrorCategory::Validation,
                "The request Origin is not allowed.",
            ),
            Self::InvalidPath => (
                StatusCode::BAD_REQUEST,
                "request/invalid_path",
                ErrorCategory::Validation,
                "Only the exact /mcp path is supported.",
            ),
            Self::UnsupportedMediaType => (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "request/unsupported_media_type",
                ErrorCategory::Validation,
                "Content-Type must be application/json.",
            ),
            Self::UnsupportedAccept => (
                StatusCode::NOT_ACCEPTABLE,
                "request/unsupported_accept",
                ErrorCategory::Validation,
                "Accept must include application/json and text/event-stream.",
            ),
            Self::InvalidProtocolVersion => (
                StatusCode::BAD_REQUEST,
                "request/invalid_protocol_version",
                ErrorCategory::Validation,
                "The MCP protocol version is unsupported.",
            ),
            Self::RequestTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "request/too_large",
                ErrorCategory::Validation,
                "The request body exceeds the configured size limit.",
            ),
            Self::ResponseTooLarge => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "response/too_large",
                ErrorCategory::Internal,
                "The response exceeds the configured size limit.",
            ),
            Self::InternalUnexpected => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal/unexpected",
                ErrorCategory::Internal,
                "The request could not be completed.",
            ),
        }
    }
}

fn mcp_http_error_parts(error: McpHttpError, correlation_id: &str) -> (StatusCode, Vec<u8>) {
    let (status, code, category, message) = error.details();
    let body = serde_json::to_vec(&serde_json::json!({
        "code": code,
        "category": category,
        "message": message,
        "correlation_id": correlation_id,
    }))
    .unwrap_or_default();
    (status, body)
}

fn mcp_http_error(error: McpHttpError, correlation_id: &str) -> Response<Body> {
    let (status, body) = mcp_http_error_parts(error, correlation_id);
    json_bytes_response(status, body)
}

fn json_bytes_response(status: StatusCode, body: Vec<u8>) -> Response<Body> {
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    response
}

fn json_bytes_response_with_protocol(
    status: StatusCode,
    body: Vec<u8>,
    protocol_version: &'static str,
) -> Response<Body> {
    let mut response = json_bytes_response(status, body);
    response.headers_mut().insert(
        MCP_PROTOCOL_VERSION,
        HeaderValue::from_static(protocol_version),
    );
    response
}

fn empty_response(status: StatusCode) -> Response<Body> {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = status;
    response
}

fn empty_response_with_protocol(
    status: StatusCode,
    protocol_version: &'static str,
) -> Response<Body> {
    let mut response = empty_response(status);
    response.headers_mut().insert(
        MCP_PROTOCOL_VERSION,
        HeaderValue::from_static(protocol_version),
    );
    response
}

#[cfg(test)]
mod tests;
