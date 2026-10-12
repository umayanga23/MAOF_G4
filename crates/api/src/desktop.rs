//! Tauri-independent application service, also exercised by the headless smoke test.
#[path = "durable.rs"]
mod durable;
pub use durable::WriterHealth;
#[path = "remote.rs"]
mod remote;
#[path = "supervision.rs"]
mod supervision;
#[path = "transport.rs"]
mod transport;
use domain::prototype::*;
pub use domain::prototype::{AgentDiscovery, DiscoveryRequest};
use observability::host::Monitor;
use orchestrator::prototype as engine;
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use storage::checkpoint::Store;
use tokio::sync::Mutex;
pub use transport::RuntimeHealth;

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
/// Metadata fetch runs outside the workspace mutex and never saves an agent.
pub async fn discover_agent(request: DiscoveryRequest) -> Result<AgentDiscovery, String> {
    llm::discovery::discover(&request).await
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    SaveAgent {
        agent: Agent,
    },
    DeleteAgent {
        id: String,
    },
    SaveWorkflow {
        workflow: Workflow,
    },
    SaveSettings {
        settings: Settings,
    },
    Launch {
        workflow_id: String,
        prompt: String,
    },
    LaunchDeadlockDemo {
        participants: usize,
    },
    Control {
        run_id: String,
        action: String,
    },
    Approve {
        run_id: String,
        task_id: String,
        allow: bool,
    },
    ImportDocument {
        name: String,
        text: String,
    },
    ApproveDocument {
        run_id: String,
        task_id: String,
        request_id: String,
        allow: bool,
    },
    RevokeDocument {
        run_id: String,
        task_id: String,
    },
    SaveMemoryNote {
        run_id: String,
        task_id: String,
        id: Option<String>,
        text: String,
        visibility: Visibility,
    },
    DeleteMemoryNote {
        run_id: String,
        id: String,
    },
    ClearMemory {
        run_id: String,
    },
    Archive {
        run_id: String,
    },
}
#[derive(Serialize)]
pub struct Snapshot {
    pub workspace: Workspace,
    pub host: HostMetrics,
    pub storage_path: String,
    pub fault: Option<String>,
    pub runtime: RuntimeHealth,
    pub writer: WriterHealth,
}
struct Inner {
    workspace: Workspace,
    store: Arc<Store>,
    monitor: Monitor,
    host: HostMetrics,
    fault: Option<String>,
    library: Arc<tools::documents::Library>,
}
// State snapshots, command ingress, ordered commit/effect gate, durable writer.
// Disk I/O holds the gate, never the snapshot mutex.
pub struct Service(
    Arc<Mutex<Inner>>,
    Arc<transport::Transport>,
    Arc<Mutex<()>>,
    Arc<durable::Writer>,
);
impl Service {
    pub fn open(path: &Path) -> Result<Arc<Self>, String> {
        let store = Store::open(path)?;
        let mut workspace = store.load()?.unwrap_or_else(engine::seed);
        engine::validate_settings(&workspace.settings)?;
        if workspace.documents.len() > 64 {
            return Err("Document library exceeds 64 entries.".into());
        }
        for document in &workspace.documents {
            tools::documents::validate(document)?;
        }
        if workspace.agents.len() > 32
            || workspace.workflows.len() > 30
            || workspace.runs.len() > 100
        {
            return Err("Checkpoint exceeds workspace limits.".into());
        }
        for agent in &workspace.agents {
            engine::validate_agent(agent)?;
        }
        for workflow in &workspace.workflows {
            engine::validate_workflow(workflow, &workspace.agents)?;
        }
        for run in &workspace.runs {
            for task in &run.tasks {
                engine::assignment::validate_pinned(task)?;
                engine::remote::validate(task, run.settings.mode)?;
                if task.agent.discovery.is_some() {
                    engine::validate_agent(&task.agent)?;
                }
            }
        }
        engine::interrupt_on_restart(&mut workspace, now());
        store.save(&workspace)?;
        let mut monitor = Monitor::default();
        let host = monitor.sample();
        let library = Arc::new(tools::documents::Library::open(&path.join("documents"))?);
        let store = Arc::new(store);
        let writer = Arc::new(durable::Writer::new(Arc::downgrade(&store)));
        Ok(Arc::new_cyclic(|owner| {
            Self(
                Arc::new(Mutex::new(Inner {
                    workspace,
                    store,
                    monitor,
                    host,
                    fault: None,
                    library,
                })),
                Arc::new(transport::Transport::new(owner.clone())),
                Arc::new(Mutex::new(())),
                writer,
            )
        }))
    }
    pub async fn snapshot(&self) -> Snapshot {
        let s = self.0.lock().await;
        Snapshot {
            workspace: s.workspace.clone(),
            host: s.host.clone(),
            storage_path: s.store.directory().display().to_string(),
            fault: s.fault.clone(),
            runtime: self.1.health(),
            writer: self.3.health(),
        }
    }
    pub async fn command(&self, command: Command) -> Result<(), String> {
        self.1.enqueue(command)?.await.map_err(|_| {
            "Command reply lost; acceptance is unknown. Inspect state before retrying.".to_string()
        })?
    }
    async fn apply_command(&self, command: Command) -> Result<(), String> {
        let gate = self.2.clone().lock_owned().await;
        let s = self.0.lock().await;
        if let Some(fault) = &s.fault {
            return Err(format!("Storage fault; new work disabled. {fault}"));
        }
        let mut next = s.workspace.clone();
        let library = s.library.clone();
        drop(s);
        let mut before = None;
        match command {
            Command::SaveAgent { mut agent } => {
                engine::validate_agent(&agent)?;
                if let Some(old) = next.agents.iter_mut().find(|a| a.id == agent.id) {
                    agent.version = old.version + 1;
                    *old = agent;
                } else {
                    if next.agents.len() >= 32 {
                        return Err("Maximum 32 agents.".into());
                    }
                    agent.version = 1;
                    next.agents.push(agent);
                }
            }
            Command::DeleteAgent { id } => {
                if next
                    .workflows
                    .iter()
                    .any(|w| w.tasks.iter().any(|t| t.agent_id == id))
                {
                    return Err("Remove this agent from saved workflows first.".into());
                }
                next.agents.retain(|a| a.id != id);
            }
            Command::SaveWorkflow { workflow } => {
                engine::validate_workflow(&workflow, &next.agents)?;
                for task in &workflow.tasks {
                    if task
                        .document_id
                        .as_ref()
                        .is_some_and(|id| !next.documents.iter().any(|d| &d.id == id))
                    {
                        return Err("Select a registered document from this workspace.".into());
                    }
                }
                if let Some(old) = next.workflows.iter_mut().find(|w| w.id == workflow.id) {
                    *old = workflow;
                } else {
                    if next.workflows.len() >= 30 {
                        return Err("Maximum 30 workflows.".into());
                    }
                    next.workflows.push(workflow);
                }
            }
            Command::SaveSettings { settings } => {
                engine::validate_settings(&settings)?;
                next.settings = settings;
            }
            Command::Launch {
                workflow_id,
                prompt,
            } => {
                engine::launch(&mut next, &workflow_id, &prompt, now())?;
            }
            Command::Control { run_id, action } => {
                engine::control(find_run(&mut next, &run_id)?, &action, now())?
            }
            Command::LaunchDeadlockDemo { participants } => {
                engine::deadlock::launch_demo(&mut next, participants, now())?;
            }
            Command::Approve {
                run_id,
                task_id,
                allow,
            } => engine::approve(find_run(&mut next, &run_id)?, &task_id, allow, now())?,
            Command::ImportDocument { name, text } => {
                if next.documents.len() >= 64 {
                    return Err("Maximum 64 registered documents.".into());
                }
                next.documents.push(
                    tokio::task::spawn_blocking(move || library.import(&name, &text))
                        .await
                        .map_err(|_| "Document import worker failed.".to_string())??,
                );
            }
            Command::ApproveDocument {
                run_id,
                task_id,
                request_id,
                allow,
            } => {
                engine::documents::approve_read(
                    find_run(&mut next, &run_id)?,
                    &task_id,
                    &request_id,
                    allow,
                    now(),
                )?;
            }
            Command::RevokeDocument { run_id, task_id } => {
                engine::documents::revoke_read(find_run(&mut next, &run_id)?, &task_id, now())?;
            }
            Command::SaveMemoryNote {
                run_id,
                task_id,
                id,
                text,
                visibility,
            } => {
                let run = find_run(&mut next, &run_id)?;
                let id = memory::save_note(run, &task_id, id.as_deref(), text, visibility, now())?;
                engine::event(
                    run,
                    now(),
                    "memory_note_saved",
                    Some(task_id),
                    format!("User saved memory note {id}. Retention limits apply."),
                );
            }
            Command::DeleteMemoryNote { run_id, id } => {
                let run = find_run(&mut next, &run_id)?;
                memory::delete_note(run, &id)?;
                engine::event(
                    run,
                    now(),
                    "memory_note_deleted",
                    None,
                    format!("User deleted memory note {id}."),
                );
            }
            Command::ClearMemory { run_id } => {
                let run = find_run(&mut next, &run_id)?;
                if !run.status.terminal() {
                    return Err("Memory can only be cleared after a run finishes.".into());
                }
                for t in &mut run.tasks {
                    t.output.clear();
                }
                run.memory.clear();
                run.memory_version = 1;
                engine::event(
                    run,
                    now(),
                    "memory_cleared",
                    None,
                    "Task context cleared by user. Saved artifacts retained.".into(),
                );
            }
            Command::Archive { run_id } => {
                let run = find_run(&mut next, &run_id)?;
                if run.tasks.iter().any(engine::remote::pending) {
                    return Err("Unresolved remote work: export and check the remote task before archiving.".into());
                }
                if !run.status.terminal() {
                    return Err("Only finished runs can be archived.".into());
                }
                let copy = run.clone();
                let name = format!("run-{run_id}.json");
                before = Some(Box::new(move |store: &Store| {
                    store.export(
                        &name,
                        &serde_json::to_vec_pretty(&copy).map_err(|e| e.to_string())?,
                    )
                })
                    as Box<dyn FnOnce(&Store) -> Result<String, String> + Send>);
                next.runs.retain(|r| r.id != run_id);
            }
        }
        self.3.commit(next, self.0.clone(), gate, before).await
    }
    pub async fn export_run(&self, run_id: &str) -> Result<String, String> {
        let run = self
            .0
            .lock()
            .await
            .workspace
            .runs
            .iter()
            .find(|r| r.id == run_id)
            .cloned()
            .ok_or("Run not found.")?;
        self.3
            .export(Box::new(move |store| {
                store.export(
                    &format!("run-{}.json", run.id),
                    &serde_json::to_vec_pretty(&run).map_err(|e| e.to_string())?,
                )
            }))
            .await
    }
    pub async fn export_evaluation(&self) -> Result<String, String> {
        self.3
            .export(Box::new(move |store| {
                let results = engine::evaluation::compare();
                let mut csv =
                    String::from("policy,mean_completion_quanta,max_first_service_quanta,trace\n");
                for r in &results {
                    csv.push_str(&format!(
                        "{:?},{},{},{}\n",
                        r.policy,
                        r.mean_completion_quanta,
                        r.max_first_service_quanta,
                        r.trace.join(" > ")
                    ));
                }
                store.export(
                    "scheduler-comparison.json",
                    &serde_json::to_vec_pretty(&results).map_err(|e| e.to_string())?,
                )?;
                store.export("scheduler-comparison.csv", csv.as_bytes())
            }))
            .await
    }
    pub async fn export_experiment(
        &self,
        config: engine::experiments::Config,
    ) -> Result<String, String> {
        config.validate()?;
        // Experiment computation must not occupy the disk writer. Keep its
        // capacity permit inside the blocking task if the caller is cancelled.
        let permit = self.3.reserve_export()?;
        let (report, permit) = tokio::task::spawn_blocking(move || {
            engine::experiments::compare(config).map(|report| (report, permit))
        })
        .await
        .map_err(|_| "Experiment export preparation failed.".to_string())??;
        self.3
            .export_prepared(
                Box::new(move |store| {
                    let csv = engine::experiments::csv(&report);
                    let json = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
                    let name = format!("scheduler-experiment-{}-{}", report.config.seed, now());
                    store.export(&format!("{name}.json"), &json)?;
                    store.export(&format!("{name}.csv"), csv.as_bytes())
                }),
                permit,
            )
            .await
    }
    #[cfg(test)]
    async fn execute_agent(&self, job: &Job) -> StepResult {
        self.execute_agent_marked(job, &std::sync::atomic::AtomicBool::new(false))
            .await
    }
    async fn execute_agent_marked(
        &self,
        job: &Job,
        may_have_called: &std::sync::atomic::AtomicBool,
    ) -> StepResult {
        // A stop/recovery accepted before this gate prevents the queued request.
        // Once the gate passes, transport is in flight and stop cannot undo it.
        let permitted = {
            let _gate = self.2.lock().await;
            let s = self.0.lock().await;
            s.fault.is_none()
                && s.workspace.runs.iter().any(|run| {
                    run.id == job.run_id
                        && matches!(run.status, RunStatus::Running | RunStatus::Paused)
                        && run.tasks.iter().any(|task| {
                            task.definition.id == job.task_id
                                && task.agent.version == job.agent.version
                                && task.attempt_id.as_deref() == Some(&job.attempt_id)
                        })
                })
        };
        if !permitted {
            return StepResult {
                text: String::new(),
                tokens: Some(0),
                error: Some("Dispatch revoked before the agent request was sent.".into()),
            };
        }
        if job.settings.mode != Mode::Demo {
            may_have_called.store(true, std::sync::atomic::Ordering::Release);
        }
        llm::local::execute(job).await
    }
    fn execute_document(&self, job: Job) {
        // Policy validation, file-open/read and accepting its result serialize
        // with stop/revocation. This small bounded read cannot race a grant update.
        let handle = tokio::runtime::Handle::current();
        let gate = handle.block_on(self.2.clone().lock_owned());
        let s = self.0.blocking_lock();
        if s.fault.is_some() {
            return;
        }
        let mut next = s.workspace.clone();
        let library = s.library.clone();
        drop(s);
        let Some(run) = next.runs.iter_mut().find(|r| {
            r.id == job.run_id && !r.status.terminal() && r.status != RunStatus::Interrupted
        }) else {
            return;
        };
        let Some(task) = run.tasks.iter().find(|t| {
            t.definition.id == job.task_id && t.attempt_id.as_deref() == Some(&job.attempt_id)
        }) else {
            return;
        };
        let JobKind::ReadDocument(request) = &job.kind else {
            return;
        };
        let result = if run.status == RunStatus::Stopping {
            Err("RunStopped: document file was not opened.".into())
        } else if task.agent.version != job.agent.version
            || task.definition.document_id.as_deref() != Some(&request.document.id)
        {
            Err("PolicyDenied: request does not match the pinned task envelope.".into())
        } else if let Some(access) = &task.document {
            if access.request != *request {
                Err("PolicyDenied: request identity changed.".into())
            } else {
                library.read(
                    request,
                    &access.request.document,
                    task.agent.read_permission,
                    access.approval,
                    access.revoked,
                )
            }
        } else {
            Err("PolicyDenied: no registered document scope.".into())
        };
        let result = match result {
            Ok(text) => StepResult {
                text,
                tokens: Some(0),
                error: None,
            },
            Err(error) => StepResult {
                text: String::new(),
                tokens: Some(0),
                error: Some(error),
            },
        };
        if engine::complete(run, &job, result, now()) {
            let _ = handle.block_on(self.3.commit(next, self.0.clone(), gate, None));
        }
    }
}
pub async fn compare_experiment(
    config: engine::experiments::Config,
) -> Result<engine::experiments::Report, String> {
    config.validate()?;
    tokio::task::spawn_blocking(move || engine::experiments::compare(config))
        .await
        .map_err(|e| e.to_string())?
}
fn find_run<'a>(w: &'a mut Workspace, id: &str) -> Result<&'a mut Run, String> {
    w.runs
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or_else(|| "Run not found.".into())
}

#[cfg(test)]
#[path = "desktop_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "remote_tests.rs"]
mod remote_tests;

#[cfg(test)]
#[path = "transport_tests.rs"]
mod transport_tests;

#[cfg(test)]
#[path = "durable_tests.rs"]
mod durable_tests;
