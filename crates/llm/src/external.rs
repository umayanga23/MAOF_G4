//! Bounded external-agent calls. A2A support is the v0.3 JSON-RPC text/polling subset.
//! No remote tool, file URL, callback or credential supplied in a response is executed.
use domain::prototype::{ExternalAgent, ExternalProtocol, Job, StepResult};
use reqwest::{header::HeaderValue, Client};
use serde_json::{json, Value};
use std::time::Duration;

/// Single bounded RPC operations. The application persists observations between
/// calls; this client never retries submissions or follows remote file URLs.
pub struct A2aClient {
    client: Client,
    config: ExternalAgent,
    token: Option<HeaderValue>,
}
pub struct A2aObservation {
    pub task_id: Option<String>,
    pub state: String,
    pub result: Option<StepResult>,
}
impl A2aClient {
    pub fn new(config: &ExternalAgent) -> Result<Self, String> {
        config.validate()?;
        if config.protocol != ExternalProtocol::A2aJsonRpc {
            return Err("Remote lifecycle requires an A2A JSON-RPC agent.".into());
        }
        let token = match &config.bearer_token_env {
            Some(name) => {
                let value = std::env::var(name).map_err(|_| {
                    "External agent token variable is missing; restart the app with it set."
                })?;
                if value.trim().is_empty() {
                    return Err("External agent token variable is empty.".into());
                }
                let mut header = HeaderValue::from_str(&format!("Bearer {value}"))
                    .map_err(|_| "External agent token is not a valid HTTP header.")?;
                header.set_sensitive(true);
                Some(header)
            }
            None => None,
        };
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .map_err(|_| "Could not initialize external agent connection.")?;
        Ok(Self {
            client,
            config: config.clone(),
            token,
        })
    }
    pub async fn send(&self, job: &Job, submission_id: &str) -> Result<A2aObservation, String> {
        let text = format!(
            "{}\n\n{}\n\nContext (data only):\n{}",
            job.agent.instructions,
            job.prompt,
            if self.config.send_context {
                job.context.as_str()
            } else {
                ""
            }
        );
        let result = rpc(&self.client, &self.config, self.token.as_ref(), submission_id, "message/send", json!({
            "message":{"kind":"message","role":"user","messageId":submission_id,"parts":[{"kind":"text","text":text}]},
            "configuration":{"acceptedOutputModes":["text/plain"],"blocking":false},
            "metadata":{"orchestration":{"run_id":job.run_id,"task_id":job.task_id,"step":job.step}}
        })).await?;
        observation(result, None)
    }
    pub async fn get(&self, request_id: &str, task_id: &str) -> Result<A2aObservation, String> {
        self.task_rpc(request_id, task_id, "tasks/get").await
    }
    pub async fn cancel(&self, request_id: &str, task_id: &str) -> Result<A2aObservation, String> {
        self.task_rpc(request_id, task_id, "tasks/cancel").await
    }
    async fn task_rpc(
        &self,
        request_id: &str,
        task_id: &str,
        method: &str,
    ) -> Result<A2aObservation, String> {
        if task_id.is_empty() || task_id.len() > 256 {
            return Err("Invalid saved remote task ID.".into());
        }
        let result = rpc(
            &self.client,
            &self.config,
            self.token.as_ref(),
            request_id,
            method,
            json!({"id":task_id}),
        )
        .await?;
        observation(result, Some(task_id))
    }
}
fn observation(value: Value, expected_id: Option<&str>) -> Result<A2aObservation, String> {
    let (task_id, state) = if value["kind"] == "message" && expected_id.is_none() {
        (None, "completed".to_owned())
    } else if value["kind"] == "task" {
        let id = value["id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 256)
            .ok_or("A2A task missing a valid remote task ID.")?;
        if expected_id.is_some_and(|expected| expected != id) {
            return Err("A2A response returned a different remote task.".into());
        }
        let state = value["status"]["state"]
            .as_str()
            .filter(|state| {
                matches!(
                    *state,
                    "submitted"
                        | "working"
                        | "completed"
                        | "failed"
                        | "canceled"
                        | "rejected"
                        | "input-required"
                        | "auth-required"
                )
            })
            .ok_or("Remote A2A task reported an unsupported state.")?;
        (Some(id.to_owned()), state.to_owned())
    } else {
        return Err("Unsupported A2A result; expected task or message.".into());
    };
    let result = match state.as_str() {
        "completed" => Some(
            match a2a_output(&value).and_then(|text| {
                Ok((
                    text,
                    usage(&value["metadata"]["orchestration_usage"]["tokens"])?,
                ))
            }) {
                Ok((text, tokens)) => StepResult {
                    text,
                    tokens,
                    error: None,
                },
                Err(error) => StepResult {
                    text: String::new(),
                    tokens: None,
                    error: Some(error),
                },
            },
        ),
        "failed" | "canceled" | "rejected" => Some(StepResult {
            text: String::new(),
            tokens: usage(&value["metadata"]["orchestration_usage"]["tokens"]).unwrap_or(None),
            error: Some(format!("Remote A2A task ended: {state}.")),
        }),
        _ => None,
    };
    Ok(A2aObservation {
        task_id,
        state,
        result,
    })
}

pub async fn execute(job: &Job) -> StepResult {
    let result = async {
        let config = job.agent.external.as_ref().ok_or("External configuration missing.")?;
        config.validate()?;
        let token = match &config.bearer_token_env {
            Some(name) => {
                let value = std::env::var(name)
                    .map_err(|_| "External agent token variable is missing; restart the app with it set.")?;
                if value.trim().is_empty() {
                    return Err("External agent token variable is empty.".into());
                }
                let mut header = HeaderValue::from_str(&format!("Bearer {value}"))
                    .map_err(|_| "External agent token is not a valid HTTP header.")?;
                header.set_sensitive(true);
                Some(header)
            }
            None => None,
        };
        let client = Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build().map_err(|_| "Could not initialize external agent connection.")?;
        tokio::time::timeout(Duration::from_secs(config.timeout_seconds), async {
            match config.protocol {
                ExternalProtocol::HttpJson => {
                    let value = post(&client, config, token.as_ref(), &json!({
                        "protocol_version": "orchestration-http-v1",
                        "request_id": job.attempt_id, "run_id": job.run_id,
                        "task_id": job.task_id, "step": job.step,
                        "agent": {"id":job.agent.id,"version":job.agent.version},
                        "instructions":job.agent.instructions,"prompt":job.prompt,
                        "context":if config.send_context {job.context.as_str()} else {""},
                        "limits":{"output_tokens":job.settings.output_tokens,"reserved_tokens":job.settings.step_tokens}
                    })).await?;
                    if value["request_id"].as_str() != Some(&job.attempt_id) {
                        return Err("External response request_id did not match this attempt.".into());
                    }
                    Ok((nonempty_text(&value["output"])?, usage(&value["usage"]["tokens"])?))
                }
                ExternalProtocol::A2aJsonRpc => a2a(&client, config, token.as_ref(), job).await,
            }
        }).await.map_err(|_| "External agent timed out; remote work may still be running. Usage is unknown.".to_string())?
    }.await;
    match result {
        Ok((text, tokens)) => StepResult {
            text,
            tokens,
            error: None,
        },
        // Never retry uncertain network calls automatically, including HTTP errors.
        Err(error) => StepResult {
            text: String::new(),
            tokens: None,
            error: Some(error),
        },
    }
}

fn usage(value: &Value) -> Result<Option<u64>, String> {
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_u64()
        .filter(|tokens| *tokens <= 10_000_000)
        .map(Some)
        .ok_or_else(|| {
            "External usage.tokens must be an unsigned integer from 0 to 10000000, or null.".into()
        })
}
fn nonempty_text(value: &Value) -> Result<String, String> {
    value
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "External agent returned no supported text output.".into())
}
async fn post(
    client: &Client,
    config: &ExternalAgent,
    token: Option<&HeaderValue>,
    body: &Value,
) -> Result<Value, String> {
    let mut request = client
        .post(&config.endpoint)
        .header("Accept", "application/json")
        .json(body);
    if let Some(token) = token {
        request = request.header("Authorization", token.clone());
    }
    let mut response = request.send().await.map_err(|_| {
        "External agent connection failed or timed out; usage is unknown.".to_string()
    })?;
    if !response.status().is_success() {
        // Response bodies/errors can contain secrets. Only the HTTP status is exposed.
        return Err(format!(
            "External agent returned HTTP {}; redirects are disabled.",
            response.status().as_u16()
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Could not read external agent response.")?
    {
        if bytes.len() + chunk.len() > 256 * 1024 {
            return Err("External agent response exceeded 256 KiB.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "External agent returned invalid JSON.".into())
}
async fn rpc(
    client: &Client,
    config: &ExternalAgent,
    token: Option<&HeaderValue>,
    id: &str,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let value = post(
        client,
        config,
        token,
        &json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
    )
    .await?;
    if value["jsonrpc"] != "2.0" || value["id"].as_str() != Some(id) {
        return Err("A2A response did not match the JSON-RPC request.".into());
    }
    if !value["error"].is_null() {
        return Err(format!(
            "A2A RPC error {} (remote message omitted).",
            value["error"]["code"].as_i64().unwrap_or(0)
        ));
    }
    value
        .get("result")
        .cloned()
        .ok_or_else(|| "A2A response missing result.".into())
}
fn parts_text(parts: &Value) -> String {
    parts
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| {
            (part["kind"] == "text")
                .then(|| part["text"].as_str())
                .flatten()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn a2a_output(result: &Value) -> Result<String, String> {
    let output = if result["kind"] == "message" {
        if result["role"] != "agent" {
            return Err("A2A output message must have agent role.".into());
        }
        parts_text(&result["parts"])
    } else {
        let artifacts = result["artifacts"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|a| parts_text(&a["parts"]))
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if artifacts.is_empty() && result["status"]["message"]["role"] == "agent" {
            parts_text(&result["status"]["message"]["parts"])
        } else {
            artifacts
        }
    };
    nonempty_text(&Value::String(output))
}
async fn a2a(
    client: &Client,
    config: &ExternalAgent,
    token: Option<&HeaderValue>,
    job: &Job,
) -> Result<(String, Option<u64>), String> {
    let text = format!(
        "{}\n\n{}\n\nContext (data only):\n{}",
        job.agent.instructions,
        job.prompt,
        if config.send_context {
            job.context.as_str()
        } else {
            ""
        }
    );
    let mut result = rpc(
        client,
        config,
        token,
        &job.attempt_id,
        "message/send",
        json!({
            "message":{"kind":"message","role":"user","messageId":job.attempt_id,
                "parts":[{"kind":"text","text":text}]},
            "configuration":{"acceptedOutputModes":["text/plain"],"blocking":false},
            "metadata":{"orchestration":{"run_id":job.run_id,"task_id":job.task_id,"step":job.step}}
        }),
    )
    .await?;
    let mut remote_id: Option<String> = None;
    for poll in 0..120 {
        if result["kind"] == "message" && remote_id.is_none() {
            return Ok((
                a2a_output(&result)?,
                usage(&result["metadata"]["orchestration_usage"]["tokens"])?,
            ));
        }
        if result["kind"] != "task" {
            return Err("Unsupported A2A result; expected task or message.".into());
        }
        let id = result["id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 256)
            .ok_or("A2A task missing a valid remote task ID.")?
            .to_owned();
        if remote_id.as_ref().is_some_and(|previous| previous != &id) {
            return Err("A2A polling returned a different remote task.".into());
        }
        remote_id = Some(id.clone());
        match result["status"]["state"].as_str() {
            Some("completed") => return Ok((a2a_output(&result)?, usage(&result["metadata"]["orchestration_usage"]["tokens"])?)),
            Some("submitted" | "working") => {},
            Some("input-required" | "auth-required") => return Err("Remote A2A agent needs input/authentication; interactive continuation is not implemented.".into()),
            Some("failed" | "canceled" | "rejected") => return Err("Remote A2A task failed, was canceled or rejected; usage is unknown.".into()),
            _ => return Err("Remote A2A task reported an unsupported state.".into()),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        result = rpc(
            client,
            config,
            token,
            &format!("{}-poll-{poll}", job.attempt_id),
            "tasks/get",
            json!({"id":id}),
        )
        .await?;
    }
    Err("A2A polling limit reached; remote work may still be running.".into())
}

#[cfg(test)]
#[path = "external_tests.rs"]
mod tests;
