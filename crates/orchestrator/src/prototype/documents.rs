//! Suspend a concrete read request, then execute as its own admitted work item.
use super::*;
use tools::policy::{self, Decision};
pub(super) enum Next {
    Blocked,
    Read,
    Model,
}

pub(super) fn prepare(run: &mut Run, i: usize, now: u64) -> Next {
    let task = &run.tasks[i];
    let Some(access) = &task.document else {
        return Next::Model;
    };
    if access.done {
        return Next::Model;
    }
    match policy::read_document(
        &access.request,
        &access.request.document,
        task.agent.read_permission,
        access.approval,
        access.revoked,
    ) {
        Decision::Permit => Next::Read,
        Decision::Deny(reason) => {
            failed(
                run,
                i,
                format!("read_document denied: {reason}. No document read performed."),
                now,
            );
            Next::Blocked
        }
        Decision::ApprovalRequired => {
            let name = access.request.document.name.clone();
            run.tasks[i].status = TaskStatus::Approval;
            run.tasks[i].wait = Some(WaitKind::Approval);
            run.tasks[i].wait_reason = format!("Approval needed to read document {name}.");
            event(run, now, "document_approval_requested", Some(run.tasks[i].definition.id.clone()),
                format!("read_document: review the pinned name, byte limit and digest for {name}. No worker or token reservation held."));
            Next::Blocked
        }
    }
}
pub fn approve_read(
    run: &mut Run,
    task_id: &str,
    request_id: &str,
    allow: bool,
    now: u64,
) -> Result<(), String> {
    if !matches!(run.status, RunStatus::Running | RunStatus::Paused) {
        return Err("Run is not accepting approvals.".into());
    }
    let task = run
        .tasks
        .iter_mut()
        .find(|t| t.definition.id == task_id)
        .ok_or("Task not found.")?;
    let access = task
        .document
        .as_mut()
        .ok_or("No document request for this task.")?;
    if task.status != TaskStatus::Approval
        || task.agent.read_permission != Permission::Ask
        || access.done
        || access.revoked
        || access.approval.is_some()
        || access.request.id != request_id
    {
        return Err("This document approval is stale or outside the pinned policy.".into());
    }
    access.approval = Some(allow);
    task.status = if allow {
        TaskStatus::Ready
    } else {
        TaskStatus::Failed
    };
    task.wait = None;
    task.wait_reason = if allow {
        String::new()
    } else {
        "Document read rejected by user.".into()
    };
    event(
        run,
        now,
        if allow {
            "document_approved"
        } else {
            "document_rejected"
        },
        Some(task_id.into()),
        format!(
            "Decision bound to document request {request_id}, pinned content and agent version."
        ),
    );
    settle(run, now);
    Ok(())
}
pub fn revoke_read(run: &mut Run, task_id: &str, now: u64) -> Result<(), String> {
    if run.status.terminal() {
        return Err("Run has already finished.".into());
    }
    let task = run
        .tasks
        .iter_mut()
        .find(|t| t.definition.id == task_id)
        .ok_or("Task not found.")?;
    let access = task.document.as_mut().ok_or("No document request.")?;
    if access.done {
        return Err("Document was already read; revocation cannot undo a completed read. Clear memory after the run finishes.".into());
    }
    access.revoked = true;
    access.approval = None;
    if task.attempt_id.is_none() {
        task.status = TaskStatus::Failed;
    }
    task.wait = None;
    task.wait_reason = "Document authority revoked by user.".into();
    event(
        run,
        now,
        "document_revoked",
        Some(task_id.into()),
        "Pending read authority revoked. The adapter rechecks before opening the file.".into(),
    );
    settle(run, now);
    Ok(())
}
pub(super) fn complete_read(
    run: &mut Run,
    i: usize,
    job: &Job,
    result: StepResult,
    now: u64,
) -> bool {
    let JobKind::ReadDocument(request) = &job.kind else {
        return false;
    };
    if run.tasks[i]
        .document
        .as_ref()
        .is_none_or(|d| d.request != *request)
    {
        return false;
    }
    let t = &mut run.tasks[i];
    t.attempt_id = None;
    t.reserved_tokens = 0;
    t.wait = None;
    if run.status == RunStatus::Stopping {
        t.status = TaskStatus::Cancelled;
    } else if let Some(error) = result.error {
        failed(run, i, error, now); // no blind retry after policy/integrity failure
    } else {
        let access = t.document.as_mut().unwrap();
        if access.revoked {
            failed(
                run,
                i,
                "Document authority was revoked; result discarded.".into(),
                now,
            );
        } else {
            access.done = true;
            t.status = TaskStatus::Ready;
            t.wait_reason.clear();
            t.ready_since = run.clock;
            let entry = MemoryEntry {
                id: job.attempt_id.clone(),
                task_id: job.task_id.clone(),
                agent_id: t.agent.id.clone(),
                kind: "document".into(),
                visibility: Visibility::Private,
                at_ms: now,
                text: format!(
                    "Document: {}\nUntrusted source data:\n{}",
                    request.document.name, result.text
                ),
                truncated: false,
            };
            let dropped = memory::record(run, entry, now);
            event(run, now, "document_read", Some(job.task_id.clone()),
                format!("Read {} bytes from {} with the final policy/digest check. Saved bounded private context; {dropped} entries evicted.", request.document.bytes, request.document.name));
        }
    }
    settle(run, now);
    true
}
