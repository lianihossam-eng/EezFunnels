use crate::state::{now, LocalState, RetryTicket};
use crate::types::{Endpoint, FunnelId, FunnelName, OperationId, Result, RevisionId};
use eez_domain::FunnelBlueprint;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::io::{self, Read, Write};
use std::time::Duration;

const RESPONSE_LIMIT: usize = 2 * 1024 * 1024;
const EDITOR_RESPONSE_LIMIT: usize = 8 * 1024 * 1024;
const ERROR_RESPONSE_LIMIT: usize = 16 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateInput {
    pub name: FunnelName,
    pub blueprint: FunnelBlueprint,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PublishInput {
    revision_id: RevisionId,
    #[serde(deserialize_with = "required_pointer")]
    expected_published_revision_id: Option<RevisionId>,
}

#[derive(Deserialize, Serialize)]
struct FunnelSummary {
    id: FunnelId,
    name: FunnelName,
    slug: String,
    #[serde(deserialize_with = "required_pointer")]
    published_revision_id: Option<RevisionId>,
}
impl FunnelSummary {
    fn validate(&self) -> Result<()> {
        if !(1..=128).contains(&self.slug.len())
            || !self
                .slug
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            || self.slug.starts_with('-')
            || self.slug.ends_with('-')
        {
            return Err("server returned an invalid funnel slug".into());
        }
        Ok(())
    }
}
#[derive(Deserialize, Serialize)]
struct ListResponse {
    command_id: String,
    items: Vec<FunnelSummary>,
}
#[derive(Deserialize, Serialize)]
struct CreateResponse {
    command_id: String,
    funnel: FunnelSummary,
    initial_revision: InitialRevision,
}
#[derive(Deserialize, Serialize)]
struct InitialRevision {
    id: RevisionId,
}
#[derive(Deserialize)]
struct EditorResponse {
    command_id: String,
    funnel: EditorFunnel,
    base_revision_id: RevisionId,
}
#[derive(Deserialize)]
struct EditorFunnel {
    id: FunnelId,
}
#[derive(Deserialize, Serialize)]
struct PublishResponse {
    command_id: String,
    funnel_id: FunnelId,
    published_revision_id: RevisionId,
}
#[derive(Deserialize)]
struct ErrorResponse {
    code: String,
}

struct ResponseFailure {
    message: String,
    unknown_outcome: bool,
}
impl ResponseFailure {
    fn uncertain(message: &str) -> Self {
        Self {
            message: message.into(),
            unknown_outcome: true,
        }
    }
}

enum Mutation {
    Create(CreateInput),
    Publish(FunnelId, PublishInput),
}
impl Mutation {
    fn from_ticket(ticket: &RetryTicket) -> Result<Self> {
        if ticket.path == "/api/funnels" {
            return serde_json::from_str(&ticket.body)
                .map(Self::Create)
                .map_err(|_| "invalid create retry body".into());
        }
        let id = ticket
            .path
            .strip_prefix("/api/funnels/")
            .and_then(|path| path.strip_suffix("/publish"))
            .ok_or("invalid retry route")?;
        let id = FunnelId::try_from(id.to_owned())?;
        let input: PublishInput =
            serde_json::from_str(&ticket.body).map_err(|_| "invalid publish retry body")?;
        if input.expected_published_revision_id.as_ref() == Some(&input.revision_id) {
            return Err("retry ticket contains an invalid no-op publish".into());
        }
        Ok(Self::Publish(id, input))
    }
}

pub struct Funnels {
    state: LocalState,
    agent: ureq::Agent,
}
impl Funnels {
    pub fn new(state: LocalState) -> Self {
        Self {
            state,
            agent: ureq::AgentBuilder::new()
                .redirects(0)
                .max_idle_connections(0)
                .timeout_connect(Duration::from_secs(10))
                .timeout_read(Duration::from_secs(30))
                .timeout_write(Duration::from_secs(30))
                .timeout(Duration::from_secs(60))
                .build(),
        }
    }
    pub fn list(&self) -> Result<String> {
        json(&self.read_list(&self.state.endpoint()?)?)
    }
    pub fn create(&self, input: CreateInput) -> Result<String> {
        let body = serde_json::to_string(&input).map_err(|_| "cannot encode create")?;
        self.start(self.state.endpoint()?, "/api/funnels".into(), body)
    }
    pub fn publish(&self, id: FunnelId) -> Result<String> {
        let endpoint = self.state.endpoint()?;
        let list = self.read_list(&endpoint)?;
        let funnel = list
            .items
            .into_iter()
            .find(|funnel| funnel.id == id)
            .ok_or("funnel was not found in the workspace")?;
        let editor: EditorResponse = self.get(
            &endpoint,
            &format!("/api/funnels/{}", id.as_str()),
            EDITOR_RESPONSE_LIMIT,
        )?;
        if editor.command_id != "funnel.editor.read" || editor.funnel.id != id {
            return Err("server returned an inconsistent editor response".into());
        }
        if funnel.published_revision_id.as_ref() == Some(&editor.base_revision_id) {
            return json(
                &serde_json::json!({"funnel_id":id,"published_revision_id":editor.base_revision_id,"already_published":true}),
            );
        }
        let body = serde_json::to_string(&PublishInput {
            revision_id: editor.base_revision_id,
            expected_published_revision_id: funnel.published_revision_id,
        })
        .map_err(|_| "cannot encode publish")?;
        self.start(
            endpoint,
            format!("/api/funnels/{}/publish", id.as_str()),
            body,
        )
    }
    pub fn retry(&self, operation_id: OperationId) -> Result<String> {
        let ticket = self.state.load(&operation_id)?;
        self.send(&ticket)
    }
    fn start(&self, endpoint: Endpoint, path: String, body: String) -> Result<String> {
        let ticket = RetryTicket {
            version: 1,
            operation_id: OperationId::fresh(),
            created_at: now()?,
            endpoint,
            path,
            body,
        };
        self.state.save(&ticket)?;
        self.send(&ticket)
    }
    fn send(&self, ticket: &RetryTicket) -> Result<String> {
        let mutation = Mutation::from_ticket(ticket)?;
        writeln!(
            io::stderr().lock(),
            "Saved operation. Retry the exact request with: eez funnels retry {}",
            ticket.operation_id.as_str()
        )
        .map_err(|_| "cannot print retry operation; request was not sent")?;
        let response = self
            .agent
            .post(&ticket.endpoint.route(&ticket.path))
            .set("Accept", "application/json")
            .set("Content-Type", "application/json")
            .set("Idempotency-Key", ticket.operation_id.as_str())
            .send_string(&ticket.body);
        let expected_status = match &mutation {
            Mutation::Create(_) => 201,
            Mutation::Publish(_, _) => 200,
        };
        let bytes = self
            .read_response(response, expected_status, RESPONSE_LIMIT)
            .map_err(|error| {
                if error.unknown_outcome {
                    unknown_outcome(&error.message, ticket)
                } else {
                    error.message
                }
            })?;
        let result = (|| match mutation {
            Mutation::Create(input) => {
                let response: CreateResponse = decode(&bytes)?;
                response.funnel.validate()?;
                if response.command_id != "funnel.create"
                    || String::from(response.funnel.name.clone()) != String::from(input.name)
                    || response.funnel.published_revision_id.is_some()
                {
                    return Err("server returned an inconsistent create response".into());
                }
                json(&response)
            }
            Mutation::Publish(id, input) => {
                let response: PublishResponse = decode(&bytes)?;
                if response.command_id != "funnel.publish"
                    || response.funnel_id != id
                    || response.published_revision_id != input.revision_id
                {
                    return Err("server returned an inconsistent publish response".into());
                }
                json(&response)
            }
        })();
        result.map_err(|error: String| unknown_outcome(&error, ticket))
    }
    fn read_list(&self, endpoint: &Endpoint) -> Result<ListResponse> {
        let response: ListResponse = self.get(endpoint, "/api/funnels", RESPONSE_LIMIT)?;
        if response.command_id != "funnel.list" || response.items.len() > 100 {
            return Err("server returned an invalid funnel list".into());
        }
        let mut ids = std::collections::HashSet::new();
        for funnel in &response.items {
            funnel.validate()?;
            if !ids.insert(funnel.id.as_str()) {
                return Err("server returned duplicate funnel IDs".into());
            }
        }
        Ok(response)
    }
    fn get<T: DeserializeOwned>(
        &self,
        endpoint: &Endpoint,
        path: &str,
        success_response_limit: usize,
    ) -> Result<T> {
        let response = self
            .agent
            .get(&endpoint.route(path))
            .set("Accept", "application/json")
            .call();
        decode(
            &self
                .read_response(response, 200, success_response_limit)
                .map_err(|error| error.message)?,
        )
    }
    fn read_response(
        &self,
        result: std::result::Result<ureq::Response, ureq::Error>,
        expected_status: u16,
        success_response_limit: usize,
    ) -> std::result::Result<Vec<u8>, ResponseFailure> {
        let response = match result {
            Ok(response) | Err(ureq::Error::Status(_, response)) => response,
            Err(ureq::Error::Transport(_)) => {
                return Err(ResponseFailure::uncertain("Worker connection failed"))
            }
        };
        let status = response.status();
        let response_limit = if status == expected_status {
            success_response_limit
        } else {
            ERROR_RESPONSE_LIMIT
        };
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(response_limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ResponseFailure::uncertain("cannot read Worker response"))?;
        if bytes.len() > response_limit {
            let message = if status == expected_status {
                format!(
                    "Worker response exceeds {} MiB limit",
                    response_limit / (1024 * 1024)
                )
            } else {
                format!("Worker returned HTTP {status}")
            };
            return Err(ResponseFailure {
                message,
                unknown_outcome: status == expected_status || !(400..500).contains(&status),
            });
        }
        if status != expected_status {
            let code = serde_json::from_slice::<ErrorResponse>(&bytes)
                .ok()
                .map(|error| error.code)
                .filter(|code| {
                    (1..=128).contains(&code.len())
                        && code.bytes().all(|byte| {
                            byte.is_ascii_lowercase()
                                || byte.is_ascii_digit()
                                || matches!(byte, b'/' | b'_' | b'-')
                        })
                });
            return Err(ResponseFailure {
                message: match code {
                    Some(code) => format!("Worker returned HTTP {status}: {code}"),
                    None => format!("Worker returned HTTP {status}"),
                },
                unknown_outcome: !(400..500).contains(&status),
            });
        }
        Ok(bytes)
    }
}
fn unknown_outcome(error: &str, ticket: &RetryTicket) -> String {
    format!(
        "{error}; mutation outcome may be unknown. Retry with: eez funnels retry {}",
        ticket.operation_id.as_str()
    )
}
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    serde_json::from_slice(bytes).map_err(|_| "Worker returned invalid or unsupported JSON".into())
}
fn json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|_| "cannot encode CLI output".into())
}

fn required_pointer<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<RevisionId>, D::Error> {
    Option::<RevisionId>::deserialize(deserializer)
}
