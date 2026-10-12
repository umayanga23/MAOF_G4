use super::*;
use durable::TestHook;
use std::sync::{Condvar, Mutex as StdMutex};

struct HeldWrite {
    entered: Arc<tokio::sync::Notify>,
    release: Arc<(StdMutex<bool>, Condvar)>,
}
impl Drop for HeldWrite {
    fn drop(&mut self) {
        let (lock, wake) = &*self.release;
        *lock.lock().unwrap() = true;
        wake.notify_all();
    }
}
fn hold(service: &Service, panic: bool) -> HeldWrite {
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new((StdMutex::new(false), Condvar::new()));
    *service.3.test_hook.lock().unwrap() = Some(TestHook {
        entered: entered.clone(),
        release: release.clone(),
        panic,
    });
    HeldWrite { entered, release }
}
fn open() -> (std::path::PathBuf, Arc<Service>) {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.cache/durable-tests")
        .join(uuid::Uuid::new_v4().to_string());
    let service = Service::open(&path).unwrap();
    (path, service)
}
async fn entered(write: &HeldWrite) {
    tokio::time::timeout(Duration::from_secs(5), write.entered.notified())
        .await
        .unwrap();
}

#[tokio::test]
async fn slow_commit_keeps_snapshots_responsive_and_publishes_only_after_ack() {
    let (path, service) = open();
    let old = service.snapshot().await.workspace.agents[0].clone();
    let write = hold(&service, false);
    let mut changed = old.clone();
    changed.name = "Saved after slow disk".into();
    let mut receipt = service
        .1
        .enqueue(Command::SaveAgent { agent: changed })
        .unwrap();
    entered(&write).await;
    let snap = tokio::time::timeout(Duration::from_millis(250), service.snapshot())
        .await
        .expect("snapshot must not wait for disk");
    assert_eq!(snap.workspace.agents[0].name, old.name);
    assert!(snap.writer.busy);
    assert!(receipt.try_recv().is_err());
    let disk: Workspace =
        serde_json::from_slice(&std::fs::read(path.join("workspace.json")).unwrap()).unwrap();
    assert_eq!(disk.agents[0].name, old.name);
    drop(write);
    receipt.await.unwrap().unwrap();
    let snap = service.snapshot().await;
    assert_eq!(snap.workspace.agents[0].name, "Saved after slow disk");
    assert_eq!(snap.writer.committed_checkpoints, 1);
    drop(service);
    let reopened = Service::open(&path).unwrap();
    assert_eq!(
        reopened.snapshot().await.workspace.agents[0].name,
        "Saved after slow disk"
    );
}
#[tokio::test]
async fn cancelled_submitter_cannot_release_transaction_before_save_and_publication() {
    let (_, service) = open();
    let write = hold(&service, false);
    let gate = service.2.clone().lock_owned().await;
    let mut next = service.snapshot().await.workspace;
    next.agents[0].name = "First committed".into();
    let task = {
        let s = service.clone();
        tokio::spawn(async move { s.3.commit(next, s.0.clone(), gate, None).await })
    };
    entered(&write).await;
    task.abort();
    let _ = task.await;
    let mut agent = service.snapshot().await.workspace.agents[1].clone();
    agent.name = "Second committed".into();
    let mut reply = service.1.enqueue(Command::SaveAgent { agent }).unwrap();
    tokio::task::yield_now().await;
    assert!(reply.try_recv().is_err());
    assert!(
        service.2.try_lock().is_err(),
        "queued writer still owns the gate"
    );
    drop(write);
    reply.await.unwrap().unwrap();
    let snap = service.snapshot().await;
    assert_eq!(snap.workspace.agents[0].name, "First committed");
    assert_eq!(snap.workspace.agents[1].name, "Second committed");
    assert_eq!(snap.writer.committed_checkpoints, 2);
}
#[tokio::test]
async fn dispatch_has_no_worker_or_model_call_until_intent_is_durable() {
    let (_, service) = open();
    service
        .command(Command::Launch {
            workflow_id: "parallel-review".into(),
            prompt: "disk gate".into(),
        })
        .await
        .unwrap();
    let write = hold(&service, false);
    let driver = tokio::spawn(service.clone().drive());
    entered(&write).await;
    let snap = service.snapshot().await;
    assert_eq!(snap.runtime.active_workers, 0);
    assert_eq!(snap.workspace.runs[0].api_calls, 0);
    assert!(snap.workspace.runs[0]
        .tasks
        .iter()
        .all(|t| t.attempt_id.is_none()));
    let mut stop = service
        .1
        .enqueue(Command::Control {
            run_id: snap.workspace.runs[0].id.clone(),
            action: "stop".into(),
        })
        .unwrap();
    tokio::task::yield_now().await;
    assert!(stop.try_recv().is_err());
    drop(write);
    stop.await.unwrap().unwrap();
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let snap = service.snapshot().await;
            if snap.workspace.runs[0].status == RunStatus::Cancelled {
                assert!(snap.workspace.runs[0]
                    .tasks
                    .iter()
                    .all(|t| t.completed_steps == 0));
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    driver.abort();
    let _ = driver.await;
}
#[tokio::test]
async fn failed_or_panicked_writer_retains_old_state_and_freezes_new_dispatch() {
    for panic in [false, true] {
        let (path, service) = open();
        let initial = service.snapshot().await.workspace.agents[0].clone();
        if !panic {
            std::fs::create_dir(path.join("workspace.json.tmp")).unwrap();
        }
        let write = hold(&service, panic);
        let mut agent = initial.clone();
        agent.name = "Uncommitted".into();
        let receipt = service.1.enqueue(Command::SaveAgent { agent }).unwrap();
        entered(&write).await;
        drop(write);
        assert!(receipt.await.unwrap().is_err());
        let snap = service.snapshot().await;
        assert_eq!(snap.workspace.agents[0].name, initial.name);
        assert!(snap.fault.is_some());
        assert_eq!(snap.writer.failures, 1);
        assert!(!snap
            .fault
            .unwrap()
            .contains("private synthetic writer panic"));
        assert!(service
            .command(Command::Launch {
                workflow_id: "parallel-review".into(),
                prompt: "must not dispatch".into()
            })
            .await
            .is_err());
        let driver = tokio::spawn(service.clone().drive());
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(service.snapshot().await.runtime.active_workers, 0);
        driver.abort();
        let _ = driver.await;
    }
}
#[tokio::test]
async fn export_lane_is_bounded_and_does_not_hold_snapshot_lock() {
    let (_, service) = open();
    let write = hold(&service, false);
    let settings = service.snapshot().await.workspace.settings;
    let commit = service
        .1
        .enqueue(Command::SaveSettings { settings })
        .unwrap();
    entered(&write).await;
    let first = {
        let s = service.clone();
        tokio::spawn(async move { s.export_evaluation().await })
    };
    let second = {
        let s = service.clone();
        tokio::spawn(async move { s.export_evaluation().await })
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        while service.3.health().exports_pending < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(service
        .export_evaluation()
        .await
        .unwrap_err()
        .contains("WriterQueueFull"));
    let snap = tokio::time::timeout(Duration::from_millis(250), service.snapshot())
        .await
        .unwrap();
    assert_eq!(snap.writer.exports_pending, 2);
    assert_eq!(snap.writer.rejected_exports, 1);
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    assert_eq!(
        service.3.health().exports_in_flight,
        2,
        "accepted exports retain capacity after caller cancellation"
    );
    drop(write);
    commit.await.unwrap().unwrap();
    assert!(second.await.unwrap().is_ok());
    assert_eq!(service.3.health().exports_pending, 0);
    assert_eq!(service.3.health().exports_in_flight, 0);
}
#[tokio::test]
async fn document_gate_waits_for_revocation_commit_and_never_reads_revoked_input() {
    let (_, service) = open();
    let initial = service.snapshot().await.workspace;
    let mut agent = initial.agents[0].clone();
    agent.read_permission = Permission::Allow;
    service.command(Command::SaveAgent { agent }).await.unwrap();
    service
        .command(Command::ImportDocument {
            name: "private.md".into(),
            text: "PRIVATE_READ".into(),
        })
        .await
        .unwrap();
    let mut workflow = initial.workflows[1].clone();
    workflow.tasks.truncate(1);
    workflow.tasks[0].document_id =
        Some(service.snapshot().await.workspace.documents[0].id.clone());
    workflow.tasks[0].steps = 1;
    service
        .command(Command::SaveWorkflow { workflow })
        .await
        .unwrap();
    service
        .command(Command::Launch {
            workflow_id: "parallel-review".into(),
            prompt: "revocation gate".into(),
        })
        .await
        .unwrap();
    let job = {
        let mut s = service.0.lock().await;
        let job = engine::dispatch(&mut s.workspace.runs[0], None, now()).remove(0);
        s.store.save(&s.workspace).unwrap();
        job
    };
    let write = hold(&service, false);
    let revoke = service
        .1
        .enqueue(Command::RevokeDocument {
            run_id: job.run_id.clone(),
            task_id: job.task_id.clone(),
        })
        .unwrap();
    entered(&write).await;
    let read = {
        let s = service.clone();
        tokio::task::spawn_blocking(move || s.execute_document(job))
    };
    let snap = service.snapshot().await;
    assert!(
        !snap.workspace.runs[0].tasks[0]
            .document
            .as_ref()
            .unwrap()
            .revoked
    );
    drop(write);
    revoke.await.unwrap().unwrap();
    read.await.unwrap();
    let run = service.snapshot().await.workspace.runs[0].clone();
    assert_eq!(run.api_calls, 0);
    assert!(run.memory.is_empty());
    assert!(run.tasks[0].document.as_ref().unwrap().revoked);
    assert!(run.tasks[0].wait_reason.contains("PolicyDenied"));
}
