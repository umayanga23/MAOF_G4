use super::*;

#[test]
fn older_checkpoint_fields_default_without_changing_existing_runs() {
    let mut w = launched();
    dispatch(&mut w.runs[0], None, 1);
    let mut json = serde_json::to_value(&w).unwrap();
    for task in json["runs"][0]["tasks"].as_array_mut().unwrap() {
        task.as_object_mut().unwrap().remove("remote");
        task.as_object_mut().unwrap().remove("assignment_decision");
    }
    json.as_object_mut().unwrap().remove("documents");
    for agent in json["agents"].as_array_mut().unwrap() {
        agent.as_object_mut().unwrap().remove("read_permission");
        agent.as_object_mut().unwrap().remove("external");
        agent.as_object_mut().unwrap().remove("discovery");
        agent.as_object_mut().unwrap().remove("capabilities");
    }
    json["settings"]
        .as_object_mut()
        .unwrap()
        .remove("memory_policy");
    json["settings"]
        .as_object_mut()
        .unwrap()
        .remove("tool_budget");
    for workflow in json["workflows"].as_array_mut().unwrap() {
        for task in workflow["tasks"].as_array_mut().unwrap() {
            task.as_object_mut().unwrap().remove("release_after_ms");
            task.as_object_mut().unwrap().remove("resource_plan");
            task.as_object_mut().unwrap().remove("document_id");
            task.as_object_mut().unwrap().remove("assignment");
            task.as_object_mut()
                .unwrap()
                .remove("required_capabilities");
        }
    }
    let r = json["runs"][0].as_object_mut().unwrap();
    for field in [
        "resource_owners",
        "resource_version",
        "deadlock_recoveries",
        "memory",
        "memory_version",
        "tool_calls",
    ] {
        r.remove(field);
    }
    for t in r["tasks"].as_array_mut().unwrap() {
        t.as_object_mut().unwrap().remove("wait");
        t.as_object_mut().unwrap().remove("document");
        t.as_object_mut().unwrap().remove("model_attempts");
        t["agent"]
            .as_object_mut()
            .unwrap()
            .remove("read_permission");
        t["agent"].as_object_mut().unwrap().remove("external");
        t["agent"].as_object_mut().unwrap().remove("discovery");
        t["agent"].as_object_mut().unwrap().remove("capabilities");
        t["definition"]
            .as_object_mut()
            .unwrap()
            .remove("assignment");
        t["definition"]
            .as_object_mut()
            .unwrap()
            .remove("required_capabilities");
        t["definition"]
            .as_object_mut()
            .unwrap()
            .remove("document_id");
        t["definition"]
            .as_object_mut()
            .unwrap()
            .remove("release_after_ms");
        t["definition"]
            .as_object_mut()
            .unwrap()
            .remove("resource_plan");
    }
    let restored: Workspace = serde_json::from_value(json).unwrap();
    assert!(restored.agents.iter().all(|a| a.capabilities.is_empty()));
    assert!(restored.runs[0]
        .tasks
        .iter()
        .all(|t| t.agent.capabilities.is_empty()
            && t.assignment_decision.is_none()
            && t.definition.assignment == TaskAssignment::Manual
            && t.definition.required_capabilities.is_empty()));
    assert!(restored.agents.iter().all(|agent| agent.external.is_none()));
    assert!(restored.runs[0]
        .tasks
        .iter()
        .all(|task| task.agent.external.is_none()));
    assert_eq!(restored.runs[0].reserved(), w.runs[0].reserved());
    assert!(restored.runs[0].resource_owners.is_empty());
    assert_eq!(restored.runs[0].tasks[0].definition.release_after_ms, 0);
    assert_eq!(
        restored.runs[0].tasks[0].agent.read_permission,
        Permission::Deny
    );
    assert_eq!(restored.runs[0].memory_version, 0);
}
fn launched() -> Workspace {
    let mut w = seed();
    launch(&mut w, "parallel-review", "test", 0).unwrap();
    w
}
#[test]
fn transport_capacity_is_reserved_before_budget_and_dispatch() {
    let mut w = launched();
    let r = &mut w.runs[0];
    assert!(dispatch_with_capacity(r, None, 1, 0).is_empty());
    assert_eq!(r.api_calls, 0);
    assert_eq!(r.reserved(), 0);
    assert!(r.tasks.iter().all(|t| t.attempt_id.is_none()));
    assert!(r
        .tasks
        .iter()
        .any(|t| t.wait_reason.contains("transport capacity")));
    let jobs = dispatch_with_capacity(r, None, 2, 1);
    assert_eq!(jobs.len(), 1);
    assert_eq!(r.api_calls, 1);
    assert_eq!(r.reserved(), r.settings.step_tokens);
    assert!(complete(r, &jobs[0], ok(), 3));
    run_to_end(r);
    assert_eq!(r.status, RunStatus::Completed);
}
fn ok() -> StepResult {
    StepResult {
        text: "accepted output".into(),
        tokens: Some(32),
        error: None,
    }
}
fn run_to_end(r: &mut Run) {
    for n in 0..100 {
        for job in dispatch(r, None, n) {
            complete(r, &job, ok(), n);
        }
        if r.status.terminal() {
            return;
        }
    }
    panic!("run did not settle: {:?}", r.status);
}
#[test]
fn reservations_are_shared_and_released_once() {
    let mut w = launched();
    let r = w.runs.last_mut().unwrap();
    r.settings.token_budget = r.settings.step_tokens;
    let jobs = dispatch(r, None, 1);
    assert_eq!(jobs.len(), 1);
    assert_eq!(r.reserved(), r.settings.step_tokens);
    assert!(complete(r, &jobs[0], ok(), 2));
    assert!(!complete(r, &jobs[0], ok(), 3));
    assert_eq!(r.tokens(), 32);
    assert_eq!(r.reserved(), 0);
    run_to_end(r);
    assert_eq!(r.status, RunStatus::Failed);
}
#[test]
fn infeasible_priority_does_not_block_feasible_agent() {
    let mut w = launched();
    let r = w.runs.last_mut().unwrap();
    r.tasks[0].agent.priority = 1;
    r.tasks[0].agent.memory_mb = 2048;
    let jobs = dispatch(r, None, 1);
    assert_eq!(r.tasks[0].status, TaskStatus::Failed);
    assert_eq!(jobs.len(), 2);
    assert!(jobs.iter().all(|j| j.agent.id != "researcher"));
}
#[test]
fn priority_fifo_and_round_robin_are_observably_different() {
    let results = evaluation::compare();
    assert_eq!(
        &results[0].trace[..3],
        &["Researcher", "Researcher", "Researcher"]
    );
    assert_eq!(&results[1].trace[..3], &["Researcher", "Analyst", "Writer"]);
    assert_eq!(results[2].trace[0], "Writer");
    assert!(results
        .iter()
        .all(|r| r.trace.len() == 9 && r.completion_quanta.iter().all(|q| *q > 0)));
    assert_eq!(
        serde_json::to_string(&results).unwrap(),
        serde_json::to_string(&evaluation::compare()).unwrap()
    );
}
#[test]
fn aging_improves_waiting_low_priority_service() {
    let mut w = launched();
    let r = w.runs.last_mut().unwrap();
    r.settings.worker_slots = 1;
    r.settings.aging_quanta = 1;
    r.tasks[0].agent.priority = 1;
    r.tasks[0].definition.steps = 8;
    r.tasks[1].agent.priority = 4;
    r.tasks[1].definition.steps = 1;
    r.tasks[2].agent.priority = 9;
    let mut first_low = 99;
    for q in 1..9 {
        for j in dispatch(r, None, q) {
            if j.agent.id == "analyst" {
                first_low = first_low.min(q);
            }
            complete(r, &j, ok(), q);
        }
    }
    assert!(
        first_low < 8,
        "aging must serve low priority before high priority finishes"
    );
}
#[test]
fn cycles_and_missing_dependencies_are_rejected_before_run() {
    let mut w = seed();
    w.workflows[0].tasks[0].dependencies = vec!["report".into()];
    assert!(launch(&mut w, "research-brief", "test", 0)
        .unwrap_err()
        .contains("Circular wait"));
    assert!(w.runs.is_empty());
    w.workflows[0].tasks[0].dependencies = vec!["missing".into()];
    assert!(launch(&mut w, "research-brief", "test", 0).is_err());
}
#[test]
fn approval_is_scoped_and_hard_deny_cannot_be_overridden() {
    let mut w = seed();
    w.workflows[1].tasks[0].publish_report = true;
    launch(&mut w, "parallel-review", "test", 0).unwrap();
    let r = w.runs.last_mut().unwrap();
    for n in 0..20 {
        for j in dispatch(r, None, n) {
            complete(r, &j, ok(), n);
        }
    }
    assert_eq!(r.tasks[0].status, TaskStatus::Approval);
    assert!(r.artifacts.is_empty());
    assert!(approve(r, "wrong-task", true, 22).is_err());
    approve(r, "research", true, 23).unwrap();
    assert!(approve(r, "research", true, 24).is_err());
    run_to_end(r);
    assert_eq!(r.artifacts.len(), 1);
    assert!(dispatch(r, None, 25).is_empty());
    assert_eq!(r.artifacts.len(), 1);
    let mut w = seed();
    w.agents[0].report_permission = Permission::Deny;
    w.workflows[1].tasks[0].publish_report = true;
    launch(&mut w, "parallel-review", "test", 0).unwrap();
    let r = w.runs.last_mut().unwrap();
    run_to_end(r);
    assert_eq!(r.tasks[0].status, TaskStatus::Failed);
    assert!(r.artifacts.is_empty());
    assert!(approve(r, "research", true, 99).is_err());
}
#[test]
fn pause_drains_and_stop_never_resurrects_work() {
    let mut w = launched();
    let r = w.runs.last_mut().unwrap();
    let jobs = dispatch(r, None, 1);
    control(r, "pause", 2).unwrap();
    assert!(dispatch(r, None, 3).is_empty());
    for j in &jobs {
        complete(r, j, ok(), 4);
    }
    assert_eq!(r.status, RunStatus::Paused);
    control(r, "resume", 5).unwrap();
    let jobs = dispatch(r, None, 6);
    control(r, "stop", 7).unwrap();
    assert_eq!(r.status, RunStatus::Stopping);
    assert!(dispatch(r, None, 8).is_empty());
    for j in &jobs {
        complete(r, j, ok(), 9);
    }
    assert_eq!(r.status, RunStatus::Cancelled);
    assert!(control(r, "resume", 10).is_err());
    assert!(!complete(r, &jobs[0], ok(), 11));
}
#[test]
fn restart_preserves_uncertainty_and_requires_explicit_recovery() {
    let mut w = launched();
    let jobs = dispatch(w.runs.last_mut().unwrap(), None, 1);
    let reserved = w.runs[0].reserved();
    interrupt_on_restart(&mut w, 2);
    let r = &mut w.runs[0];
    assert_eq!(r.status, RunStatus::Interrupted);
    assert_eq!(r.uncertain(), reserved);
    assert_eq!(r.reserved(), 0);
    assert!(dispatch(r, None, 3).is_empty());
    control(r, "recover", 4).unwrap();
    assert_eq!(r.status, RunStatus::Paused);
    assert!(!complete(r, &jobs[0], ok(), 5));
    control(r, "resume", 6).unwrap();
    assert!(!dispatch(r, None, 7).is_empty());
}
#[test]
fn unknown_usage_is_not_free_and_is_not_automatically_retried() {
    let mut w = launched();
    let r = w.runs.last_mut().unwrap();
    let jobs = dispatch(r, None, 1);
    complete(
        r,
        &jobs[0],
        StepResult {
            text: String::new(),
            tokens: None,
            error: Some("timeout".into()),
        },
        2,
    );
    let t = r
        .tasks
        .iter()
        .find(|t| t.definition.id == jobs[0].task_id)
        .unwrap();
    assert_eq!(t.uncertain_tokens, r.settings.step_tokens);
    assert_eq!(t.status, TaskStatus::Failed);
    assert_eq!(t.retries, 0);
}
#[test]
fn real_usage_overrun_is_recorded_and_pauses_dispatch() {
    let mut w = launched();
    let r = w.runs.last_mut().unwrap();
    let jobs = dispatch(r, None, 1);
    let n = r.settings.step_tokens + 50;
    complete(
        r,
        &jobs[0],
        StepResult {
            text: "output".into(),
            tokens: Some(n),
            error: None,
        },
        2,
    );
    assert_eq!(r.status, RunStatus::Paused);
    assert_eq!(r.tokens(), n);
    assert!(dispatch(r, None, 3).is_empty());
}
#[test]
fn agent_versions_are_pinned_and_same_agent_is_serial() {
    let mut w = seed();
    w.workflows[1].tasks[1].agent_id = "researcher".into();
    launch(&mut w, "parallel-review", "test", 0).unwrap();
    w.agents[0].priority = 9;
    let r = w.runs.last_mut().unwrap();
    assert_eq!(r.tasks[0].agent.priority, 3);
    let jobs = dispatch(r, None, 1);
    assert_eq!(
        jobs.iter().filter(|j| j.agent.id == "researcher").count(),
        1
    );
}
#[test]
fn retry_is_bounded_and_counts_a_fresh_call() {
    let mut w = launched();
    let r = w.runs.last_mut().unwrap();
    r.settings.worker_slots = 1;
    let j = dispatch(r, None, 1).remove(0);
    complete(
        r,
        &j,
        StepResult {
            text: String::new(),
            tokens: Some(0),
            error: Some("demo fault".into()),
        },
        2,
    );
    let retry = dispatch(r, None, 3).remove(0);
    assert_ne!(j.attempt_id, retry.attempt_id);
    assert_eq!(r.api_calls, 2);
    complete(
        r,
        &retry,
        StepResult {
            text: String::new(),
            tokens: Some(0),
            error: Some("again".into()),
        },
        4,
    );
    assert_eq!(
        r.tasks
            .iter()
            .find(|t| t.definition.id == j.task_id)
            .unwrap()
            .status,
        TaskStatus::Failed
    );
}
