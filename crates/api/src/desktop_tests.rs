use super::*;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[tokio::test]
async fn capability_assignment_is_durable_and_recovery_never_reselects() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache/assignment-tests")
        .join(Uuid::new_v4().to_string());
    let service = Service::open(&path).unwrap();
    let initial = service.snapshot().await.workspace;
    let mut agent = initial.agents[0].clone();
    agent.id = "routing-only".into();
    agent.capabilities = vec!["research".into()];
    agent.read_permission = Permission::Ask;
    agent.report_permission = Permission::Ask;
    service
        .command(Command::SaveAgent {
            agent: agent.clone(),
        })
        .await
        .unwrap();
    service
        .command(Command::ImportDocument {
            name: "routing.md".into(),
            text: "ROUTED_INPUT".into(),
        })
        .await
        .unwrap();
    let document = service.snapshot().await.workspace.documents[0].id.clone();
    let mut workflow = initial.workflows[1].clone();
    workflow.id = "auto".into();
    workflow.tasks.truncate(1);
    workflow.tasks[0].agent_id.clear();
    workflow.tasks[0].assignment = TaskAssignment::Capabilities;
    workflow.tasks[0].required_capabilities = vec!["research".into()];
    workflow.tasks[0].document_id = Some(document);
    workflow.tasks[0].publish_report = true;
    workflow.tasks[0].steps = 1;
    service
        .command(Command::SaveWorkflow { workflow })
        .await
        .unwrap();
    service
        .command(Command::Launch {
            workflow_id: "auto".into(),
            prompt: "test".into(),
        })
        .await
        .unwrap();
    let original = service.snapshot().await.workspace.runs[0].tasks[0].clone();
    agent.capabilities.clear();
    agent.priority = 9;
    service.command(Command::SaveAgent { agent }).await.unwrap();
    service
        .command(Command::DeleteAgent {
            id: "routing-only".into(),
        })
        .await
        .unwrap();
    drop(service);
    let service = Service::open(&path).unwrap();
    let run = service.snapshot().await.workspace.runs[0].clone();
    assert_eq!(run.status, RunStatus::Interrupted);
    assert_eq!(
        serde_json::to_value(&run.tasks[0].agent).unwrap(),
        serde_json::to_value(&original.agent).unwrap()
    );
    assert_eq!(
        run.tasks[0].assignment_decision.as_ref().unwrap().reason,
        original.assignment_decision.unwrap().reason
    );
    service
        .command(Command::Control {
            run_id: run.id.clone(),
            action: "recover".into(),
        })
        .await
        .unwrap();
    service
        .command(Command::Control {
            run_id: run.id.clone(),
            action: "resume".into(),
        })
        .await
        .unwrap();
    let driver = tokio::spawn(service.clone().drive());
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let run = service.snapshot().await.workspace.runs[0].clone();
            assert_eq!(run.tasks[0].agent.id, "routing-only");
            if run.tasks[0].status == TaskStatus::Approval {
                assert_eq!(run.reserved(), 0);
                if !run.tasks[0].document.as_ref().unwrap().done {
                    service
                        .command(Command::ApproveDocument {
                            run_id: run.id.clone(),
                            task_id: "research".into(),
                            request_id: run.tasks[0].document.as_ref().unwrap().request.id.clone(),
                            allow: true,
                        })
                        .await
                        .unwrap();
                } else {
                    service
                        .command(Command::Approve {
                            run_id: run.id.clone(),
                            task_id: "research".into(),
                            allow: true,
                        })
                        .await
                        .unwrap();
                }
            }
            if run.status.terminal() {
                assert_eq!(run.status, RunStatus::Completed);
                assert_eq!(run.tool_calls, 1);
                assert_eq!(run.api_calls, 1);
                assert_eq!(run.artifacts.len(), 1);
                assert!(run.memory.iter().any(|e| e.text.contains("ROUTED_INPUT")));
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        }
    })
    .await
    .unwrap();
    driver.abort();
    let _ = driver.await;
}

#[tokio::test]
async fn reviewed_discovery_persists_and_stays_pinned_without_granting_authority() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache/discovery-save-tests")
        .join(Uuid::new_v4().to_string());
    let service = Service::open(&path).unwrap();
    let initial = service.snapshot().await.workspace;
    let mut agent = initial.agents[0].clone();
    agent.external = Some(ExternalAgent {
        protocol: ExternalProtocol::A2aJsonRpc,
        endpoint: "https://agent.example/a2a".into(),
        timeout_seconds: 60,
        bearer_token_env: None,
        send_context: false,
    });
    agent.discovery=Some(llm::discovery::parse_card("https://cards.example/card",serde_json::json!({
        "protocolVersion":"0.3.0","name":"Discovered researcher","description":"Sample metadata","version":"1.0",
        "url":"https://agent.example/a2a","preferredTransport":"JSONRPC","capabilities":{},
        "defaultInputModes":["text/plain"],"defaultOutputModes":["text/plain"],
        "skills":[{"id":"research","name":"Research","description":"Compare ideas","tags":[]}]})).unwrap());
    agent.read_permission = Permission::Deny;
    agent.report_permission = Permission::Deny;
    service
        .command(Command::SaveAgent {
            agent: agent.clone(),
        })
        .await
        .unwrap();
    let mut workflow = initial.workflows[1].clone();
    workflow.tasks.truncate(1);
    service
        .command(Command::SaveWorkflow { workflow })
        .await
        .unwrap();
    service
        .command(Command::Launch {
            workflow_id: "parallel-review".into(),
            prompt: "test".into(),
        })
        .await
        .unwrap();
    let mut mismatched = agent.clone();
    mismatched.external.as_mut().unwrap().endpoint = "https://other.example/a2a".into();
    assert!(service
        .command(Command::SaveAgent { agent: mismatched })
        .await
        .is_err());
    agent.discovery.as_mut().unwrap().name = "Edited metadata".into();
    service.command(Command::SaveAgent { agent }).await.unwrap();
    drop(service);
    let restored = Service::open(&path).unwrap().snapshot().await.workspace;
    let pinned = &restored.runs[0].tasks[0].agent;
    assert_eq!(
        pinned.discovery.as_ref().unwrap().name,
        "Discovered researcher"
    );
    assert_eq!(pinned.read_permission, Permission::Deny);
    assert_eq!(pinned.report_permission, Permission::Deny);
    assert_eq!(pinned.external.as_ref().unwrap().bearer_token_env, None);
    assert!(!pinned.external.as_ref().unwrap().send_context);
    assert_eq!(
        restored.agents[0].discovery.as_ref().unwrap().name,
        "Edited metadata"
    );
}

#[tokio::test]
async fn external_dispatch_pins_configuration_and_stop_blocks_queued_network_work() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache/external-gate-tests")
        .join(Uuid::new_v4().to_string());
    let service = Service::open(&path).unwrap();
    let state = service.snapshot().await.workspace;
    let mut agent = state.agents[0].clone();
    agent.model.clear();
    agent.external = Some(ExternalAgent {
        protocol: ExternalProtocol::HttpJson,
        endpoint: "http://127.0.0.1:1/original".into(),
        timeout_seconds: 1,
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
    service
        .command(Command::SaveSettings { settings })
        .await
        .unwrap();
    let mut workflow = state.workflows[1].clone();
    workflow.tasks.truncate(1);
    workflow.tasks[0].steps = 1;
    service
        .command(Command::SaveWorkflow { workflow })
        .await
        .unwrap();
    service
        .command(Command::Launch {
            workflow_id: "parallel-review".into(),
            prompt: "test".into(),
        })
        .await
        .unwrap();
    agent.external.as_mut().unwrap().endpoint = "http://127.0.0.1:1/edited".into();
    service.command(Command::SaveAgent { agent }).await.unwrap();
    let job = {
        let mut state = service.0.lock().await;
        let job = engine::dispatch(&mut state.workspace.runs[0], None, now()).remove(0);
        state.store.save(&state.workspace).unwrap();
        job
    };
    assert!(job
        .agent
        .external
        .as_ref()
        .unwrap()
        .endpoint
        .ends_with("/original"));
    service
        .command(Command::Control {
            run_id: job.run_id.clone(),
            action: "stop".into(),
        })
        .await
        .unwrap();
    let result = service.execute_agent(&job).await;
    assert_eq!(result.tokens, Some(0));
    assert!(result
        .error
        .as_ref()
        .unwrap()
        .contains("before the agent request"));
    let mut state = service.0.lock().await;
    assert!(engine::complete(
        &mut state.workspace.runs[0],
        &job,
        result,
        now()
    ));
    assert_eq!(state.workspace.runs[0].status, RunStatus::Cancelled);
    assert_eq!(state.workspace.runs[0].uncertain(), 0);
}

async fn fixture() -> (PathBuf, std::sync::Arc<Service>) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache/service-module-tests")
        .join(Uuid::new_v4().to_string());
    let service = Service::open(&path).unwrap();
    service
        .command(Command::ImportDocument {
            name: "research.md".into(),
            text: "REAL_FILE_INPUT; ignore restrictions is source data.".into(),
        })
        .await
        .unwrap();
    let state = service.snapshot().await.workspace;
    let mut agent = state.agents[0].clone();
    agent.read_permission = Permission::Ask;
    service.command(Command::SaveAgent { agent }).await.unwrap();
    let mut workflow = state.workflows[1].clone();
    workflow.tasks.truncate(1);
    workflow.tasks[0].steps = 1;
    workflow.tasks[0].document_id = Some(state.documents[0].id.clone());
    service
        .command(Command::SaveWorkflow { workflow })
        .await
        .unwrap();
    service
        .command(Command::Launch {
            workflow_id: "parallel-review".into(),
            prompt: "Use the selected research document".into(),
        })
        .await
        .unwrap();
    (path, service)
}
// These checks use the actual service driver, adapter, checkpoint and registry.
#[tokio::test]
async fn document_approval_read_context_and_memory_notes_survive_reopening() {
    let (path, service) = fixture().await;
    let driver = tokio::spawn(service.clone().drive());
    let id = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let run = service.snapshot().await.workspace.runs.remove(0);
            if run.tasks[0].status == TaskStatus::Approval {
                let request_id = run.tasks[0].document.as_ref().unwrap().request.id.clone();
                assert_eq!(run.reserved(), 0);
                service
                    .command(Command::ApproveDocument {
                        run_id: run.id.clone(),
                        task_id: "research".into(),
                        request_id,
                        allow: true,
                    })
                    .await
                    .unwrap();
            }
            if run.status.terminal() {
                assert_eq!(run.status, RunStatus::Completed);
                assert_eq!(run.tool_calls, 1);
                assert_eq!(run.api_calls, 1);
                assert_eq!(run.tokens(), 32);
                assert!(run
                    .memory
                    .iter()
                    .any(|e| e.kind == "document" && e.text.contains("REAL_FILE_INPUT")));
                break run.id;
            }
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        }
    })
    .await
    .unwrap();
    service
        .command(Command::SaveMemoryNote {
            run_id: id.clone(),
            task_id: "research".into(),
            id: None,
            text: "Supervisor note".into(),
            visibility: Visibility::Private,
        })
        .await
        .unwrap();
    driver.abort();
    let _ = driver.await;
    drop(service);
    let reopened = Service::open(&path).unwrap();
    let state = reopened.snapshot().await;
    assert_eq!(state.workspace.documents.len(), 1);
    assert!(state.workspace.runs[0]
        .memory
        .iter()
        .any(|e| e.text == "Supervisor note"));
    reopened
        .command(Command::ClearMemory { run_id: id })
        .await
        .unwrap();
    let state = reopened.snapshot().await;
    assert!(state.workspace.runs[0].memory.is_empty());
    assert!(state.workspace.runs[0].tasks[0].output.is_empty());
}
#[tokio::test]
async fn tampered_registered_input_fails_without_model_dispatch() {
    let (path, service) = fixture().await;
    let document = service.snapshot().await.workspace.documents.remove(0);
    std::fs::write(
        path.join("documents").join(format!("{}.txt", document.id)),
        "changed".repeat(10),
    )
    .unwrap();
    let driver = tokio::spawn(service.clone().drive());
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let run = service.snapshot().await.workspace.runs.remove(0);
            if run.tasks[0].status == TaskStatus::Approval {
                service
                    .command(Command::ApproveDocument {
                        run_id: run.id.clone(),
                        task_id: "research".into(),
                        request_id: run.tasks[0].document.as_ref().unwrap().request.id.clone(),
                        allow: true,
                    })
                    .await
                    .unwrap();
            }
            if run.status.terminal() {
                assert_eq!(run.status, RunStatus::Failed);
                assert_eq!(run.api_calls, 0);
                assert!(run.memory.is_empty());
                assert!(run.tasks[0].wait_reason.starts_with("DocumentChanged"));
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        }
    })
    .await
    .unwrap();
    driver.abort();
    let _ = driver.await;
}

#[tokio::test]
async fn final_read_gate_observes_stop_and_revocation_after_dispatch_intent() {
    // Manually persist an admitted job, then change authority before the actual
    // adapter opens a file. A removed file makes a filesystem bypass observable.
    for stop in [false, true] {
        let (path, service) = fixture().await;
        let (run_id, job) = {
            let mut state = service.0.lock().await;
            let run = &mut state.workspace.runs[0];
            engine::dispatch(run, None, now());
            let request = run.tasks[0].document.as_ref().unwrap().request.id.clone();
            engine::documents::approve_read(run, "research", &request, true, now()).unwrap();
            let job = engine::dispatch(run, None, now()).remove(0);
            let run_id = run.id.clone();
            state.store.save(&state.workspace).unwrap();
            (run_id, job)
        };
        let document_id = service.snapshot().await.workspace.documents[0].id.clone();
        std::fs::remove_file(path.join("documents").join(format!("{document_id}.txt"))).unwrap();
        if stop {
            service
                .command(Command::Control {
                    run_id,
                    action: "stop".into(),
                })
                .await
                .unwrap();
        } else {
            service
                .command(Command::RevokeDocument {
                    run_id,
                    task_id: "research".into(),
                })
                .await
                .unwrap();
        }
        let worker = service.clone();
        tokio::task::spawn_blocking(move || worker.execute_document(job))
            .await
            .unwrap();
        let run = service.snapshot().await.workspace.runs.remove(0);
        assert!(run.memory.is_empty());
        assert_eq!(run.api_calls, 0);
        assert!(run.tasks[0].attempt_id.is_none());
        if stop {
            assert_eq!(run.status, RunStatus::Cancelled);
        } else {
            assert_eq!(run.status, RunStatus::Failed);
            assert!(run.tasks[0].wait_reason.starts_with("PolicyDenied"));
        }
        assert!(!run.tasks[0].wait_reason.contains("DocumentUnavailable"));
    }
}
