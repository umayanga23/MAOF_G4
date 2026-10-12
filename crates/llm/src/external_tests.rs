use super::*;
use domain::prototype::{Agent, ExternalProtocol, JobKind, Mode, Permission, Settings};
use std::io::{Read, Write};
use std::net::TcpListener;

fn job(endpoint: String, protocol: ExternalProtocol) -> Job {
    Job {
        kind: JobKind::Model,
        run_id: "run".into(),
        task_id: "task".into(),
        attempt_id: "attempt".into(),
        prompt: "Analyze supplied test data".into(),
        context: "PRIVATE_CONTEXT_MARKER".into(),
        step: 1,
        fail_demo: false,
        agent: Agent {
            id: "external".into(),
            version: 7,
            name: "External analyst".into(),
            role: "analyst".into(),
            instructions: "Analyze only supplied data".into(),
            model: String::new(),
            priority: 1,
            memory_mb: 64,
            token_budget: 16000,
            report_permission: Permission::Ask,
            read_permission: Permission::Deny,
            discovery: None,
            capabilities: vec![],
            external: Some(ExternalAgent {
                protocol,
                endpoint,
                timeout_seconds: 5,
                bearer_token_env: None,
                send_context: false,
            }),
        },
        settings: Settings {
            mode: Mode::Ollama,
            ..Settings::default()
        },
    }
}

fn fixture<F>(
    requests: usize,
    handler: F,
) -> (String, std::thread::JoinHandle<Vec<(String, Value)>>)
where
    F: Fn(usize, &Value) -> (u16, Value) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/execute", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(12);
        let mut received = Vec::new();
        for index in 0..requests {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    Err(e) => panic!("Fixture did not receive expected request: {e}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0u8; 4096];
            let (head, body) = loop {
                let size = stream.read(&mut buffer).unwrap();
                assert!(size > 0);
                bytes.extend_from_slice(&buffer[..size]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&bytes[..end]).into_owned();
                    let size = head
                        .to_lowercase()
                        .lines()
                        .find_map(|l| {
                            l.strip_prefix("content-length:")
                                .map(str::trim)
                                .and_then(|s| s.parse::<usize>().ok())
                        })
                        .unwrap();
                    if bytes.len() >= end + 4 + size {
                        break (
                            head,
                            serde_json::from_slice::<Value>(&bytes[end + 4..end + 4 + size])
                                .unwrap(),
                        );
                    }
                }
            };
            let (status, response) = handler(index, &body);
            let text = serde_json::to_string(&response).unwrap();
            let redirect = if status == 302 {
                "Location: http://127.0.0.1:1/leak\r\n"
            } else {
                ""
            };
            let _ = write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\n{redirect}Content-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
            received.push((head, body));
        }
        received
    });
    (endpoint, server)
}

#[tokio::test]
async fn http_external_agent_uses_auth_and_scoped_context_without_ollama() {
    let (endpoint, server) = fixture(1, |_, request| {
        (
            200,
            json!({"request_id":request["request_id"],"output":"External findings","usage":{"tokens":37}}),
        )
    });
    let mut j = job(endpoint, ExternalProtocol::HttpJson);
    let variable = format!("ORCHESTRATION_AGENT_TEST_{}", std::process::id());
    std::env::set_var(&variable, "fixture-token");
    j.agent.external.as_mut().unwrap().bearer_token_env = Some(variable.clone());
    let result = super::super::local::execute(&j).await;
    std::env::remove_var(&variable);
    assert_eq!(result.text, "External findings");
    assert_eq!(result.tokens, Some(37));
    assert!(result.error.is_none());
    let received = server.join().unwrap();
    assert!(received[0].0.starts_with("POST /execute"));
    assert!(received[0]
        .0
        .to_lowercase()
        .contains("authorization: bearer fixture-token"));
    assert_eq!(received[0].1["context"], "");
    assert_eq!(received[0].1["agent"]["version"], 7);
    assert!(!serde_json::to_string(&j.agent)
        .unwrap()
        .contains("fixture-token"));
}

#[tokio::test]
async fn http_correlations_redirects_usage_and_errors_fail_closed() {
    for (status, body, error) in [
        (
            200,
            json!({"request_id":"other","output":"wrong task"}),
            "request_id",
        ),
        (
            200,
            json!({"request_id":"attempt","output":"text","usage":{"tokens":-1}}),
            "unsigned",
        ),
        (
            200,
            json!({"request_id":"attempt","output":" "}),
            "text output",
        ),
        (302, json!({"secret":"do-not-expose"}), "302"),
        (401, json!({"secret":"do-not-expose"}), "401"),
    ] {
        let (endpoint, server) = fixture(1, move |_, _| (status, body.clone()));
        let result = execute(&job(endpoint, ExternalProtocol::HttpJson)).await;
        assert_eq!(result.tokens, None);
        let message = result.error.unwrap();
        assert!(message.contains(error), "{message}");
        assert!(!message.contains("do-not-expose"));
        server.join().unwrap();
    }
    let (endpoint, server) = fixture(1, |_, r| {
        (
            200,
            json!({"request_id":r["request_id"],"output":"Unknown usage"}),
        )
    });
    let mut j = job(endpoint, ExternalProtocol::HttpJson);
    j.agent.external.as_mut().unwrap().send_context = true;
    let result = execute(&j).await;
    assert!(result.error.is_none());
    assert_eq!(result.tokens, None);
    assert_eq!(
        server.join().unwrap()[0].1["context"],
        "PRIVATE_CONTEXT_MARKER"
    );
}

#[tokio::test]
async fn a2a_polls_same_remote_task_and_extracts_artifacts() {
    let (endpoint, server) = fixture(3, |i, r| {
        (
            200,
            json!({"jsonrpc":"2.0","id":r["id"],"result":{
                "kind":"task","id":"remote-1","contextId":"remote-context",
                "status":{"state":if i==2 {"completed"} else {"working"}},
                "artifacts":if i==2 {json!([{"parts":[{"kind":"text","text":"Remote analysis"},{"kind":"file","file":{"uri":"http://127.0.0.1:1/do-not-fetch"}}]}])} else {json!([])},
                "metadata":{"orchestration_usage":{"tokens":91}}
            }}),
        )
    });
    let result = execute(&job(endpoint, ExternalProtocol::A2aJsonRpc)).await;
    assert_eq!(result.text, "Remote analysis");
    assert_eq!(result.tokens, Some(91));
    let received = server.join().unwrap();
    assert_eq!(received[0].1["method"], "message/send");
    assert_eq!(received[1].1["method"], "tasks/get");
    assert_eq!(received[1].1["params"]["id"], "remote-1");
    assert!(!received[0].1.to_string().contains("PRIVATE_CONTEXT_MARKER"));
}

#[tokio::test]
async fn a2a_message_unknown_usage_and_unsupported_states_are_explicit() {
    let (endpoint, server) = fixture(1, |_, r| {
        (
            200,
            json!({"jsonrpc":"2.0","id":r["id"],"result":{"kind":"message","role":"agent","parts":[{"kind":"text","text":"Immediate result"}]}}),
        )
    });
    let result = execute(&job(endpoint, ExternalProtocol::A2aJsonRpc)).await;
    assert_eq!(result.text, "Immediate result");
    assert_eq!(result.tokens, None);
    assert!(result.error.is_none());
    server.join().unwrap();
    for state in [
        "input-required",
        "auth-required",
        "failed",
        "canceled",
        "rejected",
        "unexpected",
    ] {
        let (endpoint, server) = fixture(1, move |_, r| {
            (
                200,
                json!({"jsonrpc":"2.0","id":r["id"],"result":{"kind":"task","id":"remote-1","status":{"state":state}}}),
            )
        });
        let result = execute(&job(endpoint, ExternalProtocol::A2aJsonRpc)).await;
        assert!(result.error.is_some());
        assert_eq!(result.tokens, None);
        server.join().unwrap();
    }
}

#[tokio::test]
async fn remote_timeout_and_demo_never_imply_remote_cancellation() {
    let (endpoint, server) = fixture(1, |_, r| {
        std::thread::sleep(Duration::from_secs(2));
        (
            200,
            json!({"request_id":r["request_id"],"output":"Too late"}),
        )
    });
    let mut j = job(endpoint, ExternalProtocol::HttpJson);
    j.agent.external.as_mut().unwrap().timeout_seconds = 1;
    let result = execute(&j).await;
    assert!(result.error.is_some());
    assert_eq!(result.tokens, None);
    server.join().unwrap();
    let mut j = job(
        "http://127.0.0.1:1/unreachable".into(),
        ExternalProtocol::HttpJson,
    );
    j.settings.mode = Mode::Demo;
    j.settings.demo_delay_ms = 50;
    let result = super::super::local::execute(&j).await;
    assert!(result.text.contains("DEMO OUTPUT"));
    assert_eq!(result.tokens, Some(32));
}

#[test]
fn external_endpoint_validation_blocks_embedded_secrets_and_unsafe_transport() {
    for url in [
        "http://example.com/run",
        "file:///tmp/data",
        "https://user:secret@example.com/run",
        "https://example.com/run?token=secret",
        "https://example.com/run#fragment",
    ] {
        assert!(job(url.into(), ExternalProtocol::HttpJson)
            .agent
            .external
            .unwrap()
            .validate()
            .is_err());
    }
    let mut config = job("https://example.com/run".into(), ExternalProtocol::HttpJson)
        .agent
        .external
        .unwrap();
    assert!(config.validate().is_ok());
    config.bearer_token_env = Some("OTHER_SERVICE_SECRET".into());
    assert!(config.validate().is_err());
}

#[tokio::test]
async fn lifecycle_client_rejects_changed_task_identity_and_wrong_message_role() {
    let (endpoint, server) = fixture(1, |_, r| {
        (
            200,
            json!({"jsonrpc":"2.0","id":r["id"],"result":{
                "kind":"task","id":"different-task","status":{"state":"completed"},"artifacts":[{"parts":[{"kind":"text","text":"wrong result"}]}]
            }}),
        )
    });
    let j = job(endpoint, ExternalProtocol::A2aJsonRpc);
    let client = A2aClient::new(j.agent.external.as_ref().unwrap()).unwrap();
    assert!(client
        .get("query", "original-task")
        .await
        .err()
        .unwrap()
        .contains("different remote task"));
    let calls = server.join().unwrap();
    assert_eq!(calls[0].1["params"]["id"], "original-task");
    let (endpoint, server) = fixture(1, |_, r| {
        (
            200,
            json!({"jsonrpc":"2.0","id":r["id"],"result":{
                "kind":"message","role":"user","parts":[{"kind":"text","text":"not an agent result"}]
            }}),
        )
    });
    let j = job(endpoint, ExternalProtocol::A2aJsonRpc);
    let client = A2aClient::new(j.agent.external.as_ref().unwrap()).unwrap();
    let response = client.send(&j, "submission").await.unwrap();
    assert!(response
        .result
        .unwrap()
        .error
        .unwrap()
        .contains("agent role"));
    server.join().unwrap();
}
