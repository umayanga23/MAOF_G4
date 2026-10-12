//! Exercises the real service, worker adapter, approvals, persistence and exports.
use api::desktop::{Command, Service};
use domain::prototype::{RunStatus, TaskStatus};
use std::{path::PathBuf, time::Duration};
#[tokio::main]
async fn main() -> Result<(), String> {
    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".cache/smoke-workspace"));
    let service = Service::open(&path)?;
    let mut settings = service.snapshot().await.workspace.settings;
    settings.demo_delay_ms = 50;
    service.command(Command::SaveSettings { settings }).await?;
    service
        .command(Command::Launch {
            workflow_id: "research-brief".into(),
            prompt: "Supervisor demonstration".into(),
        })
        .await?;
    let driver = tokio::spawn(service.clone().drive());
    let result = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let snap = service.snapshot().await;
            if let Some(fault) = snap.fault {
                return Err(fault);
            }
            let run = snap.workspace.runs.last().unwrap();
            for t in &run.tasks {
                if t.status == TaskStatus::Approval {
                    service
                        .command(Command::Approve {
                            run_id: run.id.clone(),
                            task_id: t.definition.id.clone(),
                            allow: true,
                        })
                        .await?;
                }
            }
            if run.status.terminal() {
                if run.status != RunStatus::Completed || run.artifacts.len() != 1 {
                    return Err(format!("Unexpected result: {:?}", run.status));
                }
                println!(
                    "Completed {} tasks / {} model attempts / {} tokens / {} artifact(s).",
                    run.tasks.len(),
                    run.api_calls,
                    run.tokens(),
                    run.artifacts.len()
                );
                println!("Run export: {}", service.export_run(&run.id).await?);
                println!("Comparison: {}", service.export_evaluation().await?);
                return Ok(run.id.clone());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .map_err(|_| "Smoke test timed out".to_string())?;
    driver.abort();
    let _ = driver.await;
    drop(service);
    let id = result?;
    let reopened = Service::open(&path)?;
    let saved = reopened.snapshot().await;
    assert!(saved
        .workspace
        .runs
        .iter()
        .any(|r| r.id == id && r.status == RunStatus::Completed && r.artifacts.len() == 1));
    println!("PASS: persisted run and report survived reopening.");
    Ok(())
}
