use super::*;

fn run() -> Run {
    let mut w = seed();
    launch(&mut w, "research-brief", "module tests", 0).unwrap();
    w.runs.remove(0)
}
fn entry(
    id: &str,
    task: &str,
    agent: &str,
    visibility: Visibility,
    text: &str,
    at_ms: u64,
) -> MemoryEntry {
    MemoryEntry {
        id: id.into(),
        task_id: task.into(),
        agent_id: agent.into(),
        visibility,
        kind: "note".into(),
        text: text.into(),
        at_ms,
        truncated: false,
    }
}
#[test]
fn memory_private_dependency_and_expiry_boundaries_are_enforced() {
    let mut r = run();
    r.settings.memory_policy.ttl_ms = 100;
    memory::record(
        &mut r,
        entry(
            "a",
            "research",
            "researcher",
            Visibility::Private,
            "PRIVATE_SOURCE",
            0,
        ),
        0,
    );
    memory::record(
        &mut r,
        entry(
            "b",
            "research",
            "researcher",
            Visibility::Shared,
            "SHARED_FINDING",
            0,
        ),
        0,
    );
    memory::record(
        &mut r,
        entry(
            "c",
            "analyse",
            "analyst",
            Visibility::Shared,
            "UNFINISHED",
            0,
        ),
        0,
    );
    r.tasks[0].status = TaskStatus::Completed;
    let writer = memory::context(&r, 2, 1);
    assert!(writer.contains("SHARED_FINDING"));
    assert!(!writer.contains("PRIVATE_SOURCE"));
    assert!(!writer.contains("UNFINISHED"));
    assert!(memory::context(&r, 0, 1).contains("PRIVATE_SOURCE"));
    assert!(memory::context(&r, 2, 100).is_empty());
    assert_eq!(r.memory.len(), 3); // expiry excludes context; history is still inspectable
    r.settings.context_chars = 7;
    assert!(memory::context(&r, 0, 1).chars().count() <= 7);
}
#[test]
fn memory_unicode_size_eviction_and_note_editing_preserve_ownership() {
    let mut r = run();
    r.settings.memory_policy.max_entries = 2;
    r.settings.memory_policy.max_chars = 128;
    memory::record(
        &mut r,
        entry(
            "old",
            "research",
            "researcher",
            Visibility::Private,
            "old",
            0,
        ),
        0,
    );
    let unicode = "\u{0dc3}\u{0dd2}".repeat(100);
    memory::record(
        &mut r,
        entry(
            "new",
            "research",
            "researcher",
            Visibility::Private,
            &unicode,
            1,
        ),
        1,
    );
    assert_eq!(r.memory.len(), 1);
    assert!(r.memory[0].truncated);
    assert_eq!(r.memory[0].text.chars().count(), 128);
    assert!(memory::save_note(
        &mut r,
        "research",
        None,
        "text".into(),
        Visibility::Private,
        2
    )
    .is_err());
    control(&mut r, "pause", 2).unwrap();
    let id = memory::save_note(
        &mut r,
        "research",
        None,
        "user note".into(),
        Visibility::Private,
        3,
    )
    .unwrap();
    assert!(memory::save_note(
        &mut r,
        "analyse",
        Some(&id),
        "move".into(),
        Visibility::Shared,
        4
    )
    .is_err());
    memory::save_note(
        &mut r,
        "research",
        Some(&id),
        "edited".into(),
        Visibility::Shared,
        4,
    )
    .unwrap();
    assert!(r.memory.iter().any(|e| e.id == id && e.text == "edited"));
    memory::delete_note(&mut r, &id).unwrap();
    assert!(!r.memory.iter().any(|e| e.id == id));
}
fn document_run(permission: Permission) -> Run {
    let mut w = seed();
    let document = Document {
        id: Uuid::new_v4().to_string(),
        name: "notes.md".into(),
        bytes: 7,
        sha256: "a".repeat(64),
    };
    w.workflows[1].tasks.truncate(1);
    w.workflows[1].tasks[0].document_id = Some(document.id.clone());
    w.workflows[1].tasks[0].steps = 1;
    w.agents[0].read_permission = permission;
    w.documents.push(document);
    launch(&mut w, "parallel-review", "test", 0).unwrap();
    w.runs.remove(0)
}
#[test]
fn concrete_document_approval_reserves_nothing_and_read_is_a_separate_quantum() {
    let mut r = document_run(Permission::Ask);
    assert!(dispatch(&mut r, None, 0).is_empty());
    assert_eq!(r.tasks[0].status, TaskStatus::Approval);
    assert_eq!(r.api_calls, 0);
    assert_eq!(r.reserved(), 0);
    assert!(r.tasks[0].attempt_id.is_none());
    assert!(approve(&mut r, "research", true, 1).is_err()); // report approval cannot approve read
    assert!(documents::approve_read(&mut r, "research", "stale", true, 1).is_err());
    let request = r.tasks[0].document.as_ref().unwrap().request.id.clone();
    documents::approve_read(&mut r, "research", &request, true, 1).unwrap();
    let job = dispatch(&mut r, None, 1).remove(0);
    assert!(matches!(job.kind, JobKind::ReadDocument(_)));
    assert_eq!(r.api_calls, 0);
    assert_eq!(r.tool_calls, 1);
    assert_eq!(r.reserved(), 0);
    assert_eq!(r.tasks[0].completed_steps, 0);
    let result = || StepResult {
        text: "SOURCE_DATA".into(),
        tokens: Some(0),
        error: None,
    };
    assert!(complete(&mut r, &job, result(), 2));
    assert!(!complete(&mut r, &job, result(), 3));
    let model = dispatch(&mut r, None, 3).remove(0);
    assert_eq!(model.kind, JobKind::Model);
    assert!(model.context.contains("SOURCE_DATA"));
    assert_eq!(r.api_calls, 1);
    assert_eq!(r.tasks[0].model_attempts, 1);
    assert_eq!(r.memory[0].visibility, Visibility::Private);
}
#[test]
fn denied_revoked_and_restart_reads_never_reuse_old_approval() {
    let mut denied = document_run(Permission::Deny);
    assert!(dispatch(&mut denied, None, 0).is_empty());
    assert_eq!(denied.status, RunStatus::Failed);
    let mut exhausted = document_run(Permission::Allow);
    exhausted.settings.tool_budget = 0;
    assert!(dispatch(&mut exhausted, None, 0).is_empty());
    assert_eq!(exhausted.status, RunStatus::Failed);
    assert_eq!(exhausted.tool_calls, 0);
    assert_eq!(exhausted.api_calls, 0);
    let mut r = document_run(Permission::Ask);
    dispatch(&mut r, None, 0);
    let request = r.tasks[0].document.as_ref().unwrap().request.id.clone();
    documents::approve_read(&mut r, "research", &request, true, 1).unwrap();
    let job = dispatch(&mut r, None, 1).remove(0);
    let mut w = seed();
    w.runs.push(r);
    interrupt_on_restart(&mut w, 2);
    let r = &mut w.runs[0];
    assert_eq!(r.uncertain(), 0); // document read has no model token liability
    control(r, "recover", 3).unwrap();
    control(r, "resume", 4).unwrap();
    assert!(dispatch(r, None, 4).is_empty());
    assert_eq!(r.tasks[0].status, TaskStatus::Approval);
    assert_ne!(r.tasks[0].document.as_ref().unwrap().request.id, request);
    assert!(documents::approve_read(r, "research", &request, true, 4).is_err());
    assert!(!complete(
        r,
        &job,
        StepResult {
            text: "stale".into(),
            tokens: Some(0),
            error: None
        },
        4
    ));
    documents::revoke_read(r, "research", 5).unwrap();
    assert!(documents::approve_read(r, "research", &request, true, 5).is_err());
    assert_eq!(r.status, RunStatus::Failed);
}
