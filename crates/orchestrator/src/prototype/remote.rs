//! Durable identity and accounting for the latest A2A step.
use super::*;

pub fn is_a2a(task: &TaskRun, mode: Mode) -> bool {
    mode != Mode::Demo
        && task
            .agent
            .external
            .as_ref()
            .is_some_and(|c| c.protocol == ExternalProtocol::A2aJsonRpc)
}
pub fn pending(task: &TaskRun) -> bool {
    task.remote.as_ref().is_some_and(|r| r.pending)
}
pub fn intent(submission_id: String, step: u32, now: u64) -> RemoteTask {
    RemoteTask {
        submission_id,
        task_id: None,
        step,
        state: "queued".into(),
        pending: true,
        updated_at: now,
        uncertain_tokens: 0,
        cancellation: RemoteCancellation::None,
        detail: "Saved dispatch intent; submission has not started.".into(),
    }
}
pub fn retain_uncertainty(task: &mut TaskRun) {
    let reserved = std::mem::take(&mut task.reserved_tokens);
    task.uncertain_tokens += reserved;
    if let Some(remote) = &mut task.remote {
        if remote.pending {
            remote.uncertain_tokens += reserved;
        }
    }
}
/// Release the worker fence without automatically replaying or polling again.
pub fn suspend(run: &mut Run, job: &Job, error: String, now: u64) -> bool {
    let Some(task) = run.tasks.iter_mut().find(|t| {
        t.definition.id == job.task_id && t.attempt_id.as_deref() == Some(&job.attempt_id)
    }) else {
        return false;
    };
    retain_uncertainty(task);
    task.attempt_id = None;
    task.wait = None;
    let stopping = run.status == RunStatus::Stopping;
    task.status = if stopping {
        TaskStatus::Cancelled
    } else {
        TaskStatus::Waiting
    };
    task.wait_reason = error.clone();
    if let Some(remote) = &mut task.remote {
        remote.detail = error.clone();
        remote.updated_at = now;
        if remote.state == "submitting" {
            remote.state = "unknown".into();
        }
        if matches!(
            remote.cancellation,
            RemoteCancellation::Requested | RemoteCancellation::Sent
        ) {
            remote.cancellation = RemoteCancellation::Unconfirmed;
        }
    }
    if !stopping {
        run.status = RunStatus::Paused;
    }
    event(
        run,
        now,
        "remote_unresolved",
        Some(job.task_id.clone()),
        error,
    );
    settle(run, now);
    true
}
pub fn validate(task: &TaskRun, mode: Mode) -> Result<(), String> {
    if let Some(r) = &task.remote {
        if !is_a2a(task, mode)
            || r.submission_id.is_empty()
            || r.submission_id.len() > 128
            || r.task_id
                .as_ref()
                .is_some_and(|id| id.is_empty() || id.len() > 256)
            || r.step == 0
            || r.step > task.definition.steps
            || r.pending && r.step != task.completed_steps + 1
            || r.uncertain_tokens > task.uncertain_tokens
            || r.detail.len() > 2048
            || !matches!(
                r.state.as_str(),
                "queued"
                    | "submitting"
                    | "unknown"
                    | "submitted"
                    | "working"
                    | "completed"
                    | "canceled"
                    | "failed"
                    | "rejected"
                    | "input-required"
                    | "auth-required"
                    | "not_submitted"
            )
        {
            return Err("Invalid saved A2A task identity/accounting.".into());
        }
    }
    Ok(())
}
