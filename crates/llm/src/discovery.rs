//! User-directed read-only Agent Card fetch. No endpoint probe or registration.
use domain::prototype::{
    AgentDiscovery, DiscoveredSkill, DiscoveryRequest, ExternalAgent, ExternalProtocol,
};
use reqwest::{header::HeaderValue, Client};
use serde_json::Value;
use std::time::Duration;

pub async fn discover(request: &DiscoveryRequest) -> Result<AgentDiscovery, String> {
    let config = ExternalAgent {
        protocol: ExternalProtocol::A2aJsonRpc,
        endpoint: request.card_url.clone(),
        timeout_seconds: 15,
        bearer_token_env: request.bearer_token_env.clone(),
        send_context: false,
    };
    config.validate()?;
    let mut client_request = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| "Could not initialize Agent Card fetch.")?
        .get(&request.card_url)
        .header("Accept", "application/json");
    if let Some(name) = &request.bearer_token_env {
        let value = std::env::var(name)
            .map_err(|_| "Agent Card token variable is missing; restart with it set.")?;
        if value.trim().is_empty() {
            return Err("Agent Card token variable is empty.".into());
        }
        let mut header = HeaderValue::from_str(&format!("Bearer {value}"))
            .map_err(|_| "Agent Card token is not a valid HTTP header.")?;
        header.set_sensitive(true);
        client_request = client_request.header("Authorization", header);
    }
    // Whole response deadline includes streamed chunks, not just connection setup.
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut response = client_request
            .send()
            .await
            .map_err(|_| "Agent Card connection failed or timed out.")?;
        if !response.status().is_success() {
            return Err(format!(
                "Agent Card returned HTTP {}; redirects are disabled. Response body omitted.",
                response.status().as_u16()
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Could not read Agent Card response.")?
        {
            if bytes.len() + chunk.len() > 256 * 1024 {
                return Err("Agent Card exceeded 256 KiB.".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| "Agent Card returned invalid JSON.")?;
        parse_card(&request.card_url, value)
    })
    .await
    .map_err(|_| "Agent Card fetch timed out.")?
}
fn text(value: &Value) -> Result<String, String> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "Agent Card is missing a required text field.".into())
}
fn list(value: &Value) -> Result<Vec<String>, String> {
    let values = value
        .as_array()
        .filter(|v| v.len() <= 16)
        .ok_or("Agent Card requires bounded string lists.")?;
    values.iter().map(text).collect()
}
fn flag(value: Option<&Value>) -> Result<bool, String> {
    match value {
        None => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        _ => Err("Invalid Agent Card boolean capability.".into()),
    }
}
fn security_required(
    value: Option<&Value>,
    schemes: Option<&serde_json::Map<String, Value>>,
) -> Result<bool, String> {
    let Some(value) = value else {
        return Ok(false);
    };
    let requirements = value
        .as_array()
        .filter(|r| r.len() <= 16)
        .ok_or("Invalid Agent Card security requirements.")?;
    for requirement in requirements {
        let map = requirement
            .as_object()
            .filter(|m| m.len() <= 16)
            .ok_or("Invalid Agent Card security requirement.")?;
        for (name, scopes) in map {
            if name.is_empty() || name.len() > 80 || !schemes.is_some_and(|s| s.contains_key(name))
            {
                return Err("Agent Card security requirement has no declared scheme.".into());
            }
            if list(scopes)?.iter().any(|s| s.len() > 100) {
                return Err("Agent Card security scopes exceed limits.".into());
            }
        }
    }
    Ok(!requirements.is_empty()
        && requirements
            .iter()
            .all(|r| !r.as_object().unwrap().is_empty()))
}
pub fn parse_card(card_url: &str, value: Value) -> Result<AgentDiscovery, String> {
    if value["protocolVersion"] != "0.3.0" {
        return Err("Discovery supports A2A protocolVersion 0.3.0 only.".into());
    }
    let primary = text(&value["url"])?;
    // Type definition defaults omitted preference to JSONRPC; explicit preference
    // is respected, with a declared JSONRPC additional interface as fallback.
    let preferred = value
        .get("preferredTransport")
        .map(text)
        .transpose()?
        .unwrap_or_else(|| "JSONRPC".into());
    if preferred.is_empty() || preferred.len() > 80 {
        return Err("Invalid Agent Card preferred transport.".into());
    }
    let mut interfaces = vec![(preferred.clone(), primary.clone())];
    if let Some(extra) = value.get("additionalInterfaces") {
        for interface in extra
            .as_array()
            .filter(|v| v.len() <= 8)
            .ok_or("Agent Card interfaces exceed supported limits.")?
        {
            let transport = text(&interface["transport"])?;
            if transport.is_empty() || transport.len() > 80 {
                return Err("Invalid Agent Card transport.".into());
            }
            interfaces.push((transport, text(&interface["url"])?));
        }
    }
    // Check every declared address; do not follow any address while discovering.
    for (_, endpoint) in &interfaces {
        ExternalAgent {
            protocol: ExternalProtocol::A2aJsonRpc,
            endpoint: endpoint.clone(),
            timeout_seconds: 15,
            bearer_token_env: None,
            send_context: false,
        }
        .validate()?;
    }
    let endpoint = interfaces
        .iter()
        .find(|(transport, _)| transport == "JSONRPC")
        .map(|(_, url)| url.clone())
        .ok_or("Agent Card offers no supported JSONRPC interface.")?;
    let capabilities = value["capabilities"]
        .as_object()
        .ok_or("Agent Card is missing capabilities.")?;
    if let Some(extensions) = capabilities.get("extensions") {
        for extension in extensions
            .as_array()
            .filter(|v| v.len() <= 16)
            .ok_or("Invalid Agent Card extensions.")?
        {
            if !extension.is_object() || flag(extension.get("required"))? {
                return Err(
                    "Agent Card requires an extension this client does not support.".into(),
                );
            }
        }
    }
    let schemes = match value.get("securitySchemes") {
        None => None,
        Some(v) => Some(
            v.as_object()
                .filter(|m| m.len() <= 16 && m.values().all(Value::is_object))
                .ok_or("Invalid Agent Card security schemes.")?,
        ),
    };
    let authentication_required = security_required(value.get("security"), schemes)?;
    let input = list(&value["defaultInputModes"])?;
    let output = list(&value["defaultOutputModes"])?;
    let mut skills = Vec::new();
    for skill in value["skills"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 32)
        .ok_or("Agent Card needs 1–32 skills.")?
    {
        skills.push(DiscoveredSkill {
            id: text(&skill["id"])?,
            name: text(&skill["name"])?,
            description: text(&skill["description"])?,
            tags: list(&skill["tags"])?,
            input_modes: skill
                .get("inputModes")
                .map(list)
                .transpose()?
                .unwrap_or_else(|| input.clone()),
            output_modes: skill
                .get("outputModes")
                .map(list)
                .transpose()?
                .unwrap_or_else(|| output.clone()),
            authentication_required: security_required(skill.get("security"), schemes)?
                || authentication_required,
        });
    }
    let card = AgentDiscovery {
        card_url: card_url.into(),
        name: text(&value["name"])?,
        description: text(&value["description"])?,
        agent_version: text(&value["version"])?,
        protocol_version: "0.3.0".into(),
        endpoint,
        streaming: flag(capabilities.get("streaming"))?,
        authentication_required,
        skills,
    };
    card.validate()?;
    Ok(card)
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod tests;
