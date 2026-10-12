use super::*;
use super::{
    supervision::{Outcome, TestFault},
    transport::*,
};
use std::{path::PathBuf, sync::atomic::Ordering};

fn open() -> (PathBuf, Arc<Service>) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache/transport-tests")
        .join(uuid::Uuid::new_v4().to_string());
    let service = Service::open(&path).unwrap();
    (path, service)
}
async fn launch(service: &Service, tasks: usize) {
    let state = service.snapshot().await.workspace;
    let mut settings = state.settings;
    settings.demo_delay_ms = 50;
    settings.max_retries = 3;
    service
        .command(Command::SaveSettings { settings })
        .await
        .unwrap();
    let mut workflow = state.workflows[1].clone();
    workflow.tasks.truncate(tasks);
    for t in &mut workflow.tasks {
        t.steps = 1;
    }
    service
        .command(Command::SaveWorkflow { workflow })
        .await
        .unwrap();
    service
        .command(Command::Launch {
            workflow_id: "parallel-review".into(),
            prompt: "transport test".into(),
        })
        .await
        .unwrap();
}
async fn wait(service: &Service, test: impl Fn(&Run) -> bool) -> Run {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let r = service.snapshot().await.workspace.runs[0].clone();
            if test(&r) {
                return r;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("worker should settle without a stuck Running attempt")
}

#[tokio::test]
async fn full_normal_lane_keeps_control_capacity_and_stop_overtakes_queued_launch() {
    let (_, service) = open();
    launch(&service, 1).await;
    let run_id = service.snapshot().await.workspace.runs[0].id.clone();
    let lock = service.0.lock().await;
    let settings = lock.workspace.settings.clone();
    let first = service
        .1
        .enqueue(Command::SaveSettings {
            settings: settings.clone(),
        })
        .unwrap();
    tokio::task::yield_now().await; // The first normal command waits at the state gate.
    let mut queued = Vec::new();
    // First queued normal launch can succeed only if Stop runs before it.
    queued.push(
        service
            .1
            .enqueue(Command::Launch {
                workflow_id: "parallel-review".into(),
                prompt: "next after stop".into(),
            })
            .unwrap(),
    );
    for _ in 1..NORMAL_SLOTS {
        queued.push(
            service
                .1
                .enqueue(Command::SaveSettings {
                    settings: settings.clone(),
                })
                .unwrap(),
        );
    }
    assert!(service
        .1
        .enqueue(Command::SaveSettings { settings })
        .unwrap_err()
        .contains("CommandQueueFull"));
    let stop = service
        .1
        .enqueue(Command::Control {
            run_id,
            action: "stop".into(),
        })
        .unwrap();
    assert_eq!(service.1.health().commands_pending, NORMAL_SLOTS);
    assert_eq!(service.1.health().controls_pending, 1);
    drop(lock);
    first.await.unwrap().unwrap();
    stop.await.unwrap().unwrap();
    for receipt in queued {
        receipt.await.unwrap().unwrap();
    }
    let s = service.snapshot().await;
    assert_eq!(s.workspace.runs[0].status, RunStatus::Cancelled);
    assert_eq!(s.workspace.runs.len(), 2);
    assert_eq!(s.runtime.rejected_commands, 1);
    assert_eq!(s.runtime.command_bytes_in_flight, 0);
}
#[tokio::test]
async fn control_lane_is_bounded_and_four_control_burst_does_not_starve_normal_work() {
    let (_, service) = open();
    launch(&service, 1).await;
    let run_id = service.snapshot().await.workspace.runs[0].id.clone();
    let lock = service.0.lock().await;
    service.1.applied_lanes.lock().unwrap().clear();
    let first = service
        .1
        .enqueue(Command::SaveSettings {
            settings: lock.workspace.settings.clone(),
        })
        .unwrap();
    tokio::task::yield_now().await;
    let mut controls = Vec::new();
    for i in 0..CONTROL_SLOTS {
        controls.push(
            service
                .1
                .enqueue(Command::Control {
                    run_id: run_id.clone(),
                    action: if i % 2 == 0 { "pause" } else { "resume" }.into(),
                })
                .unwrap(),
        );
    }
    // Inspect actual application order, independent of when callers observe ACKs.
    let normal = service
        .1
        .enqueue(Command::SaveSettings {
            settings: lock.workspace.settings.clone(),
        })
        .unwrap();
    assert!(service
        .1
        .enqueue(Command::Control {
            run_id,
            action: "pause".into()
        })
        .unwrap_err()
        .contains("CommandQueueFull"));
    drop(lock);
    first.await.unwrap().unwrap();
    normal.await.unwrap().unwrap();
    for receipt in controls {
        receipt.await.unwrap().unwrap();
    }
    assert_eq!(
        *service.1.applied_lanes.lock().unwrap(),
        vec![false, true, true, true, true, false, true, true, true, true]
    );
}
#[tokio::test]
async fn payload_budget_rejects_before_acceptance_and_cancelled_caller_does_not_cancel_command() {
    let (path, service) = open();
    let lock = service.0.lock().await;
    let first = service
        .1
        .enqueue(Command::SaveSettings {
            settings: lock.workspace.settings.clone(),
        })
        .unwrap();
    tokio::task::yield_now().await;
    let mut receipts = Vec::new();
    let large = "x".repeat(200000); // Decoded but invalid task input; bounded transport must still account for it.
    while let Ok(receipt) = service.1.enqueue(Command::Launch {
        workflow_id: "research-brief".into(),
        prompt: large.clone(),
    }) {
        receipts.push(receipt);
    }
    assert!(receipts.len() < NORMAL_SLOTS);
    assert!(service.1.health().command_bytes_in_flight <= NORMAL_BYTES + CONTROL_BYTES);
    assert!(service
        .1
        .enqueue(Command::ImportDocument {
            name: "large.md".into(),
            text: "x".repeat(MAX_COMMAND_BYTES + 1)
        })
        .unwrap_err()
        .contains("CommandTooLarge"));
    let mut agent = lock.workspace.agents[0].clone();
    agent.name = "Persist despite cancelled caller".into();
    // Separate control capacity is still available when normal payload budget is exhausted.
    let urgent = service
        .1
        .enqueue(Command::Control {
            run_id: "missing".into(),
            action: "stop".into(),
        })
        .unwrap();
    drop(lock);
    first.await.unwrap().unwrap();
    urgent.await.unwrap().unwrap_err();
    for receipt in receipts {
        receipt.await.unwrap().unwrap_err();
    }
    let receipt = service.1.enqueue(Command::SaveAgent { agent }).unwrap();
    drop(receipt);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if service.snapshot().await.workspace.agents[0].name
                == "Persist despite cancelled caller"
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let saved: Workspace =
        serde_json::from_slice(&std::fs::read(path.join("workspace.json")).unwrap()).unwrap();
    assert_eq!(saved.agents[0].name, "Persist despite cancelled caller");
}
#[tokio::test]
async fn command_ack_requires_persistence_and_store_failure_does_not_publish_mutation() {
    let (path, service) = open();
    let initial = service.snapshot().await.workspace.agents[0].clone();
    std::fs::create_dir(path.join("workspace.json.tmp")).unwrap();
    let mut agent = initial.clone();
    agent.name = "Must not appear".into();
    assert!(service.command(Command::SaveAgent { agent }).await.is_err());
    let state = service.snapshot().await;
    assert!(state.fault.is_some());
    assert_eq!(state.workspace.agents[0].name, initial.name);
    assert!(service
        .command(Command::Launch {
            workflow_id: "research-brief".into(),
            prompt: "test".into()
        })
        .await
        .is_err());
    assert!(service.snapshot().await.workspace.runs.is_empty());
}
#[tokio::test]
async fn panic_missing_result_and_watchdog_fail_only_original_task_without_retries() {
    for fault in [
        TestFault::PanicBefore,
        TestFault::NoReport,
        TestFault::StallBefore,
    ] {
        let (_, service) = open();
        launch(&service, 2).await;
        service
            .1
            .faults
            .lock()
            .unwrap()
            .insert("research".into(), fault);
        if matches!(fault, TestFault::StallBefore) {
            service.1.deadline_ms.store(100, Ordering::Relaxed);
        }
        let driver = tokio::spawn(service.clone().drive());
        let run = wait(&service, |r| r.status.terminal()).await;
        assert_eq!(run.tasks[0].status, TaskStatus::Failed);
        assert_eq!(run.tasks[0].retries, 0);
        assert_eq!(run.tasks[1].status, TaskStatus::Completed);
        assert_eq!(run.reserved(), 0);
        assert_eq!(run.uncertain(), 0);
        assert_eq!(run.tokens(), 32);
        assert!(run.tasks.iter().all(|t| t.attempt_id.is_none()));
        assert_eq!(service.snapshot().await.runtime.worker_failures, 1);
        assert!(!serde_json::to_string(&run)
            .unwrap()
            .contains("synthetic private panic payload"));
        driver.abort();
        let _ = driver.await;
    }
}
#[tokio::test]
async fn document_worker_panic_opens_no_file_and_retains_no_model_usage() {
    let (_, service) = open();
    service
        .command(Command::ImportDocument {
            name: "input.md".into(),
            text: "SECRET_INPUT".into(),
        })
        .await
        .unwrap();
    let state = service.snapshot().await.workspace;
    let mut agent = state.agents[0].clone();
    agent.read_permission = Permission::Allow;
    service.command(Command::SaveAgent { agent }).await.unwrap();
    let mut wf = state.workflows[1].clone();
    wf.tasks.truncate(1);
    wf.tasks[0].document_id = Some(state.documents[0].id.clone());
    service
        .command(Command::SaveWorkflow { workflow: wf })
        .await
        .unwrap();
    service
        .command(Command::Launch {
            workflow_id: "parallel-review".into(),
            prompt: "test".into(),
        })
        .await
        .unwrap();
    service
        .1
        .faults
        .lock()
        .unwrap()
        .insert("research".into(), TestFault::PanicBefore);
    let driver = tokio::spawn(service.clone().drive());
    let run = wait(&service, |r| r.status.terminal()).await;
    assert_eq!(run.status, RunStatus::Failed);
    assert_eq!(run.api_calls, 0);
    assert_eq!(run.uncertain(), 0);
    assert!(run.memory.is_empty());
    driver.abort();
    let _ = driver.await;
}
#[tokio::test]
async fn duplicate_driver_cannot_dispatch_and_stale_failure_cannot_settle_twice() {
    let (_, service) = open();
    launch(&service, 1).await;
    let job = {
        let mut s = service.0.lock().await;
        engine::dispatch(&mut s.workspace.runs[0], None, now()).remove(0)
    };
    service
        .accept_outcome(
            &job,
            Outcome::Failure {
                may_have_called: true,
                reason: "WorkerPanicked",
            },
        )
        .await;
    let before = serde_json::to_string(&service.snapshot().await.workspace).unwrap();
    service
        .accept_outcome(
            &job,
            Outcome::Failure {
                may_have_called: true,
                reason: "WorkerPanicked",
            },
        )
        .await;
    assert_eq!(
        before,
        serde_json::to_string(&service.snapshot().await.workspace).unwrap()
    );
    assert_eq!(service.snapshot().await.runtime.worker_failures, 1);
    assert_eq!(
        service.snapshot().await.workspace.runs[0].uncertain(),
        job.settings.step_tokens
    );
    let driver = tokio::spawn(service.clone().drive());
    tokio::task::yield_now().await;
    let duplicate = tokio::spawn(service.clone().drive());
    duplicate.await.unwrap();
    assert!(service.snapshot().await.runtime.driver_running);
    driver.abort();
    let _ = driver.await;
    assert!(!service.snapshot().await.runtime.driver_running);
}
