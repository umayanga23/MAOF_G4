use super::*;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex as StdMutex,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

struct Server {
    endpoint: String,
    requests: Arc<StdMutex<Vec<Value>>>,
    finished: Arc<AtomicBool>,
    worker: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.worker.abort();
    }
}
async fn server(cancel: &'static str, lose_submission: bool) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/a2a", listener.local_addr().unwrap());
    let requests = Arc::new(StdMutex::new(Vec::new()));
    let finished = Arc::new(AtomicBool::new(false));
    let log = requests.clone();
    let done = finished.clone();
    let worker = tokio::spawn(async move {
        let mut canceled = false;
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = tokio::time::timeout(Duration::from_secs(4), async {
                let mut data = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let n = stream.read(&mut chunk).await.unwrap();
                    if n == 0 {
                        return None;
                    }
                    data.extend_from_slice(&chunk[..n]);
                    if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                        let header = String::from_utf8_lossy(&data[..end]).to_lowercase();
                        let length = header
                            .lines()
                            .find_map(|l| {
                                l.strip_prefix("content-length:")
                                    .and_then(|n| n.trim().parse::<usize>().ok())
                            })
                            .unwrap();
                        if data.len() >= end + 4 + length {
                            return Some(
                                serde_json::from_slice::<Value>(&data[end + 4..end + 4 + length])
                                    .unwrap(),
                            );
                        }
                    }
                }
            })
            .await
            .unwrap();
            let Some(request) = request else {
                continue;
            };
            log.lock().unwrap().push(request.clone());
            let method = request["method"].as_str().unwrap();
            if method == "message/send" && lose_submission {
                continue;
            }
            if method == "tasks/cancel" && cancel != "reject" {
                canceled = true;
            }
            if method == "tasks/cancel" && cancel == "lost" {
                continue;
            }
            let response = if method == "tasks/cancel" && cancel == "reject" {
                json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32002,"message":"SECRET_REMOTE_ERROR"}})
            } else {
                let state = if canceled {
                    "canceled"
                } else if done.load(Ordering::SeqCst) {
                    "completed"
                } else {
                    "working"
                };
                json!({"jsonrpc":"2.0","id":request["id"],"result":{"kind":"task","id":"durable-remote-1",
                    "status":{"state":state},"artifacts":if state=="completed" {json!([{"parts":[{"kind":"text","text":"Recovered SAMPLE42 result"}]}])} else {json!([])},
                    "metadata":{"orchestration_usage":{"tokens":91}}}})
            };
            let body = serde_json::to_vec(&response).unwrap();
            let head=format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len());
            let _ = stream.write_all(head.as_bytes()).await;
            let _ = stream.write_all(&body).await;
        }
    });
    Server {
        endpoint,
        requests,
        finished,
        worker,
    }
}
async fn fixture(server: &Server, timeout: u64) -> (std::path::PathBuf, Arc<Service>, Job) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache/remote-recovery-tests")
        .join(uuid::Uuid::new_v4().to_string());
    let service = Service::open(&path).unwrap();
    let mut state = service.snapshot().await.workspace;
    let mut agent = state.agents.remove(0);
    agent.model.clear();
    agent.external = Some(ExternalAgent {
        protocol: ExternalProtocol::A2aJsonRpc,
        endpoint: server.endpoint.clone(),
        timeout_seconds: timeout,
        bearer_token_env: None,
        send_context: false,
    });
    service
        .command(Command::SaveAgent {
            agent: agent.clone(),
        })
        .await
        .unwrap();
    let mut settings = state.settings;
    settings.mode = Mode::Ollama;
    settings.worker_slots = 1;
    settings.step_tokens = 128;
    settings.api_budget = 1;
    service
        .command(Command::SaveSettings { settings })
        .await
        .unwrap();
    let mut workflow = state.workflows[1].clone();
    workflow.tasks.truncate(1);
    workflow.tasks[0].steps = 1;
    workflow.tasks[0].agent_id = agent.id;
    workflow.tasks[0].publish_report = false;
    service
        .command(Command::SaveWorkflow {
            workflow: workflow.clone(),
        })
        .await
        .unwrap();
    service
        .command(Command::Launch {
            workflow_id: workflow.id,
            prompt: "test".into(),
        })
        .await
        .unwrap();
    let job = admit(&service).await;
    (path, service, job)
}
async fn admit(service: &Service) -> Job {
    let mut s = service.0.lock().await;
    let job = engine::dispatch(&mut s.workspace.runs[0], None, now()).remove(0);
    s.store.save(&s.workspace).unwrap();
    job
}
async fn wait(service: &Service, predicate: impl Fn(&Run) -> bool) -> Run {
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            let run = service.snapshot().await.workspace.runs.remove(0);
            if predicate(&run) {
                return run;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("Expected remote lifecycle state within deadline")
}
fn count(server: &Server, method: &str) -> usize {
    server
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|v| v["method"] == method)
        .count()
}
#[tokio::test]
async fn supervised_panic_after_remote_response_preserves_identity_and_resumes_without_replay() {
    let server = server("accept", false).await;
    server.finished.store(true, Ordering::SeqCst);
    let (path, service, original) = fixture(&server, 8).await;
    control(&service, &original.run_id, "stop").await;
    let result = service.execute_remote(&original).await.unwrap();
    finish(&service, &original, result).await;
    assert_eq!(count(&server, "message/send"), 0);
    service
        .command(Command::Launch {
            workflow_id: "parallel-review".into(),
            prompt: "supervised remote response".into(),
        })
        .await
        .unwrap();
    service.1.faults.lock().unwrap().insert(
        "research".into(),
        supervision::TestFault::PanicAfterResponse,
    );
    let driver = tokio::spawn(service.clone().drive());
    let paused = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let r = service
                .snapshot()
                .await
                .workspace
                .runs
                .last()
                .unwrap()
                .clone();
            if r.status == RunStatus::Paused {
                break r;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(paused.uncertain(), 128);
    assert!(paused.tasks[0].attempt_id.is_none());
    assert_eq!(
        paused.tasks[0].remote.as_ref().unwrap().task_id.as_deref(),
        Some("durable-remote-1")
    );
    assert_eq!(service.snapshot().await.runtime.worker_failures, 1);
    assert!(!serde_json::to_string(&paused)
        .unwrap()
        .contains("synthetic private panic payload"));
    control(&service, &paused.id, "resume").await;
    let completed = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let r = service
                .snapshot()
                .await
                .workspace
                .runs
                .last()
                .unwrap()
                .clone();
            if r.status == RunStatus::Completed {
                break r;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(count(&server, "message/send"), 1);
    assert_eq!(count(&server, "tasks/get"), 1);
    assert_eq!(completed.api_calls, 1);
    assert_eq!(completed.tokens(), 91);
    assert_eq!(completed.uncertain(), 0);
    assert_eq!(completed.reserved(), 0);
    driver.abort();
    let _ = driver.await;
    drop(service);
    let reopened = Service::open(&path).unwrap();
    assert_eq!(
        reopened
            .snapshot()
            .await
            .workspace
            .runs
            .last()
            .unwrap()
            .tokens(),
        91
    );
}
async fn finish(service: &Service, job: &Job, result: StepResult) {
    let mut s = service.0.lock().await;
    let mut next = s.workspace.clone();
    assert!(engine::complete(&mut next.runs[0], job, result, now()));
    s.store.save(&next).unwrap();
    s.workspace = next;
}
async fn control(service: &Service, id: &str, action: &str) {
    service
        .command(Command::Control {
            run_id: id.into(),
            action: action.into(),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn restart_gets_saved_task_without_submission_or_new_budget_and_fences_old_result() {
    let server = server("accept", false).await;
    let (path, service, original) = fixture(&server, 8).await;
    let worker = {
        let service = service.clone();
        let job = original.clone();
        tokio::spawn(async move { service.execute_remote(&job).await })
    };
    let saved = wait(&service, |r| {
        r.tasks[0].remote.as_ref().unwrap().task_id.is_some()
    })
    .await;
    let disk: Workspace =
        serde_json::from_slice(&std::fs::read(path.join("workspace.json")).unwrap()).unwrap();
    assert_eq!(
        disk.runs[0].tasks[0]
            .remote
            .as_ref()
            .unwrap()
            .task_id
            .as_deref(),
        Some("durable-remote-1")
    );
    worker.abort();
    let _ = worker.await;
    drop(service);
    let service = Service::open(&path).unwrap();
    let interrupted = service.snapshot().await.workspace.runs.remove(0);
    assert_eq!(interrupted.status, RunStatus::Interrupted);
    assert_eq!(interrupted.uncertain(), 128);
    // Repeated restarts must not compound the same uncertain reservation.
    drop(service);
    let service = Service::open(&path).unwrap();
    assert_eq!(service.snapshot().await.workspace.runs[0].uncertain(), 128);
    control(&service, &saved.id, "recover").await;
    control(&service, &saved.id, "resume").await;
    server.finished.store(true, Ordering::SeqCst);
    let driver = tokio::spawn(service.clone().drive());
    let completed = wait(&service, |r| r.status == RunStatus::Completed).await;
    assert_eq!(count(&server, "message/send"), 1);
    assert!(count(&server, "tasks/get") >= 1);
    assert_eq!(completed.api_calls, 1);
    assert_eq!(completed.tasks[0].model_attempts, 1);
    assert_eq!(completed.tokens(), 91);
    assert_eq!(completed.uncertain(), 0);
    assert_eq!(completed.reserved(), 0);
    assert_eq!(completed.tasks[0].completed_steps, 1);
    assert_eq!(completed.memory.len(), 1);
    assert!(completed.tasks[0].output.contains("SAMPLE42"));
    {
        let mut state = service.0.lock().await;
        assert!(!engine::complete(
            &mut state.workspace.runs[0],
            &original,
            StepResult {
                text: "stale".into(),
                tokens: Some(91),
                error: None
            },
            now()
        ));
    }
    driver.abort();
    let _ = driver.await;
}

#[tokio::test]
async fn lost_submission_identity_is_never_replayed_even_after_restart_or_stop() {
    let server = server("accept", true).await;
    let (path, service, job) = fixture(&server, 3).await;
    assert!(service.execute_remote(&job).await.is_none());
    let paused = service.snapshot().await.workspace.runs.remove(0);
    assert_eq!(paused.status, RunStatus::Paused);
    assert_eq!(paused.uncertain(), 128);
    assert_eq!(paused.tasks[0].remote.as_ref().unwrap().state, "unknown");
    drop(service);
    let service = Service::open(&path).unwrap();
    control(&service, &paused.id, "recover").await;
    control(&service, &paused.id, "resume").await;
    let job = admit(&service).await;
    assert!(service.execute_remote(&job).await.is_none());
    assert_eq!(count(&server, "message/send"), 1);
    assert_eq!(count(&server, "tasks/get"), 0);
    control(&service, &paused.id, "stop").await;
    let job = admit(&service).await;
    assert!(service.execute_remote(&job).await.is_none());
    let stopped = service.snapshot().await.workspace.runs.remove(0);
    assert_eq!(stopped.status, RunStatus::Cancelled);
    assert!(stopped.tasks[0].remote.as_ref().unwrap().pending);
    assert!(service
        .command(Command::Archive { run_id: paused.id })
        .await
        .is_err());
}

#[tokio::test]
async fn stop_confirms_remote_cancellation_and_settles_usage_once() {
    let server = server("accept", false).await;
    let (_, service, job) = fixture(&server, 8).await;
    let worker = {
        let s = service.clone();
        let j = job.clone();
        tokio::spawn(async move { s.execute_remote(&j).await })
    };
    wait(&service, |r| {
        r.tasks[0].remote.as_ref().unwrap().task_id.is_some()
    })
    .await;
    control(&service, &job.run_id, "stop").await;
    let result = worker.await.unwrap().unwrap();
    finish(&service, &job, result).await;
    let stopped = service.snapshot().await.workspace.runs.remove(0);
    assert_eq!(stopped.status, RunStatus::Cancelled);
    assert_eq!(count(&server, "tasks/cancel"), 1);
    assert_eq!(
        stopped.tasks[0].remote.as_ref().unwrap().cancellation,
        RemoteCancellation::Confirmed
    );
    assert!(!stopped.tasks[0].remote.as_ref().unwrap().pending);
    assert_eq!(stopped.tokens(), 91);
    assert_eq!(stopped.tasks[0].completed_steps, 0);
    assert!(stopped.memory.is_empty());
}

#[tokio::test]
async fn rejected_cancel_stays_uncertain_then_manual_recheck_settles_without_replay() {
    let server = server("reject", false).await;
    let (_, service, job) = fixture(&server, 3).await;
    let worker = {
        let s = service.clone();
        let j = job.clone();
        tokio::spawn(async move { s.execute_remote(&j).await })
    };
    wait(&service, |r| {
        r.tasks[0].remote.as_ref().unwrap().task_id.is_some()
    })
    .await;
    control(&service, &job.run_id, "stop").await;
    assert!(worker.await.unwrap().is_none());
    let stopped = service.snapshot().await.workspace.runs.remove(0);
    assert_eq!(stopped.status, RunStatus::Cancelled);
    assert_eq!(stopped.uncertain(), 128);
    assert_eq!(
        stopped.tasks[0].remote.as_ref().unwrap().cancellation,
        RemoteCancellation::Unconfirmed
    );
    assert!(!serde_json::to_string(&stopped)
        .unwrap()
        .contains("SECRET_REMOTE_ERROR"));
    server.finished.store(true, Ordering::SeqCst);
    control(&service, &job.run_id, "reconcile_remote").await;
    let driver = tokio::spawn(service.clone().drive());
    let stopped = wait(&service, |r| {
        r.status == RunStatus::Cancelled && !r.tasks[0].remote.as_ref().unwrap().pending
    })
    .await;
    assert_eq!(count(&server, "message/send"), 1);
    assert_eq!(count(&server, "tasks/cancel"), 1);
    assert_eq!(stopped.tokens(), 91);
    assert_eq!(stopped.uncertain(), 0);
    assert_eq!(stopped.api_calls, 1);
    assert_eq!(stopped.tasks[0].completed_steps, 0);
    assert!(stopped.memory.is_empty());
    driver.abort();
    let _ = driver.await;
}

#[tokio::test]
async fn restart_after_cancel_intent_never_resends_cancel_or_submission() {
    let server = server("lost", false).await;
    let (path, service, job) = fixture(&server, 8).await;
    let worker = {
        let s = service.clone();
        let j = job.clone();
        tokio::spawn(async move { s.execute_remote(&j).await })
    };
    wait(&service, |r| {
        r.tasks[0].remote.as_ref().unwrap().task_id.is_some()
    })
    .await;
    control(&service, &job.run_id, "stop").await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while count(&server, "tasks/cancel") == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    worker.abort();
    let _ = worker.await;
    // Aborting a local observer is not a process crash: an accepted checkpoint
    // now continues through save/publication and retains the workspace lock.
    // Drain that transaction before simulating a fresh process in this runtime.
    {
        let _drained = service.2.lock().await;
    }
    drop(service);
    let service = Service::open(&path).unwrap();
    assert_eq!(
        service.snapshot().await.workspace.runs[0].status,
        RunStatus::Interrupted
    );
    control(&service, &job.run_id, "recover").await;
    let driver = tokio::spawn(service.clone().drive());
    let stopped = wait(&service, |r| r.status == RunStatus::Cancelled).await;
    assert_eq!(
        stopped.tasks[0].remote.as_ref().unwrap().cancellation,
        RemoteCancellation::Confirmed
    );
    assert_eq!(count(&server, "tasks/cancel"), 1);
    assert_eq!(count(&server, "message/send"), 1);
    assert_eq!(stopped.tokens(), 91);
    assert_eq!(stopped.uncertain(), 0);
    driver.abort();
    let _ = driver.await;
}

#[tokio::test]
async fn stop_before_submission_gate_sends_no_network_and_retains_no_uncertainty() {
    let server = server("accept", false).await;
    let (_, service, job) = fixture(&server, 3).await;
    control(&service, &job.run_id, "stop").await;
    let result = service.execute_remote(&job).await.unwrap();
    assert_eq!(result.tokens, Some(0));
    finish(&service, &job, result).await;
    let stopped = service.snapshot().await.workspace.runs.remove(0);
    assert_eq!(stopped.status, RunStatus::Cancelled);
    assert_eq!(
        stopped.tasks[0].remote.as_ref().unwrap().state,
        "not_submitted"
    );
    assert_eq!(stopped.uncertain(), 0);
    assert_eq!(stopped.reserved(), 0);
    assert!(server.requests.lock().unwrap().is_empty());
}
