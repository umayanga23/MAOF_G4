use super::*;

fn automatic() -> Workspace {
    let mut w = seed();
    w.workflows[1].tasks.truncate(1);
    let t = &mut w.workflows[1].tasks[0];
    t.assignment = TaskAssignment::Capabilities;
    t.agent_id.clear();
    t.required_capabilities = vec!["research".into(), "analysis".into()];
    t.steps = 1;
    w
}
fn capable(w: &mut Workspace) {
    for a in &mut w.agents {
        a.capabilities = vec!["research".into(), "analysis".into()];
    }
}
#[test]
fn matches_all_tags_and_breaks_priority_ties_by_id_not_registration_order() {
    let mut w = automatic();
    capable(&mut w);
    for a in &mut w.agents {
        a.priority = 2;
    }
    // One matching tag alone is insufficient, even at highest priority.
    w.agents[0].capabilities.pop();
    w.agents[0].priority = 1;
    w.agents[1].capabilities.push("extra".into());
    for _ in 0..2 {
        let (a, decision) = select(&w.workflows[1].tasks[0], &w.agents, &w.settings).unwrap();
        assert_eq!(a.id, "analyst");
        assert_eq!(decision.unwrap().eligible_agent_ids, ["analyst", "writer"]);
        w.agents.reverse();
    }
}
#[test]
fn unmatched_launch_is_atomic_but_template_can_be_saved() {
    let mut w = automatic();
    validate_workflow(&w.workflows[1], &w.agents).unwrap();
    let before = serde_json::to_string(&w).unwrap();
    assert!(launch(&mut w, "parallel-review", "test", 0)
        .unwrap_err()
        .contains("No agent matches all"));
    assert_eq!(before, serde_json::to_string(&w).unwrap());
    capable(&mut w);
    let mut other = w.workflows[1].tasks[0].clone();
    other.id = "no-match".into();
    other.required_capabilities = vec!["unregistered".into()];
    w.workflows[1].tasks.push(other);
    let before = serde_json::to_string(&w).unwrap();
    assert!(launch(&mut w, "parallel-review", "test", 0).is_err());
    assert_eq!(before, serde_json::to_string(&w).unwrap());
}
#[test]
fn permissions_and_declared_capacity_filter_before_priority_without_grants() {
    let mut w = automatic();
    capable(&mut w);
    let task = &mut w.workflows[1].tasks[0];
    task.publish_report = true;
    task.document_id = Some(Uuid::new_v4().to_string());
    for a in &mut w.agents {
        a.read_permission = Permission::Ask;
    }
    w.agents[2].priority = 1;
    w.agents[2].report_permission = Permission::Deny;
    w.agents[0].priority = 2;
    w.agents[0].memory_mb = w.settings.memory_mb + 1;
    let (a, _) = select(task, &w.agents, &w.settings).unwrap();
    assert_eq!(a.id, "analyst");
    assert_eq!(a.read_permission, Permission::Ask);
    assert_eq!(a.report_permission, Permission::Ask);
    w.agents[1].token_budget = w.settings.step_tokens - 1;
    assert!(select(task, &w.agents, &w.settings)
        .unwrap_err()
        .contains("No permission is increased"));
    w.agents[1].token_budget = w.settings.step_tokens;
    w.agents[1].read_permission = Permission::Deny;
    assert!(select(task, &w.agents, &w.settings).is_err());
    w.agents[1].read_permission = Permission::Allow;
    w.settings.token_budget = w.settings.step_tokens - 1;
    assert!(select(task, &w.agents, &w.settings).is_err());
}
#[test]
fn automatic_report_still_waits_for_task_scoped_approval() {
    let mut w = automatic();
    capable(&mut w);
    w.workflows[1].tasks[0].publish_report = true;
    launch(&mut w, "parallel-review", "test", 0).unwrap();
    let run = &mut w.runs[0];
    let jobs = dispatch(run, None, 1);
    assert_eq!(jobs.len(), 1);
    complete(
        run,
        &jobs[0],
        StepResult {
            text: "test output".into(),
            tokens: Some(12),
            error: None,
        },
        2,
    );
    dispatch(run, None, 3);
    assert_eq!(run.tasks[0].status, TaskStatus::Approval);
    assert_eq!(run.reserved(), 0);
    assert!(run.artifacts.is_empty());
    assert!(approve(run, "wrong-task", true, 4).is_err());
    approve(run, "research", true, 5).unwrap();
    dispatch(run, None, 6);
    assert_eq!(run.artifacts.len(), 1);
}
#[test]
fn saved_assignment_survives_agent_edits_deletion_and_recovery() {
    let mut w = automatic();
    capable(&mut w);
    launch(&mut w, "parallel-review", "test", 0).unwrap();
    assert_eq!(w.workflows[1].tasks[0].agent_id, "");
    let original = serde_json::to_string(&w.runs[0].tasks[0].agent).unwrap();
    assert_eq!(w.runs[0].tasks[0].agent.id, "writer");
    w.agents.clear();
    let mut restored: Workspace =
        serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
    let run = &mut restored.runs[0];
    validate_pinned(&run.tasks[0]).unwrap();
    run.status = RunStatus::Interrupted;
    control(run, "recover", 2).unwrap();
    control(run, "resume", 3).unwrap();
    let jobs = dispatch(run, None, 4);
    assert_eq!(jobs.len(), 1);
    assert_eq!(serde_json::to_string(&jobs[0].agent).unwrap(), original);
    let mut invalid = run.tasks[0].clone();
    invalid.agent.capabilities.clear();
    assert!(validate_pinned(&invalid).is_err());
    invalid = run.tasks[0].clone();
    invalid.assignment_decision = None;
    assert!(validate_pinned(&invalid).is_err());
}
#[test]
fn same_agent_serializes_and_fresh_budget_gate_does_not_reroute() {
    let mut w = automatic();
    capable(&mut w);
    let mut second = w.workflows[1].tasks[0].clone();
    second.id = "second".into();
    w.workflows[1].tasks.push(second);
    launch(&mut w, "parallel-review", "test", 0).unwrap();
    let run = &mut w.runs[0];
    assert!(run.tasks.iter().all(|t| t.agent.id == "writer"));
    let first = dispatch(run, None, 1);
    assert_eq!(first.len(), 1);
    assert!(dispatch(run, None, 2).is_empty());
    complete(
        run,
        &first[0],
        StepResult {
            text: "done".into(),
            tokens: Some(run.settings.step_tokens),
            error: None,
        },
        3,
    );
    run.settings.token_budget = run.settings.step_tokens;
    assert!(dispatch(run, None, 4).is_empty());
    assert_eq!(run.tasks[1].agent.id, "writer");
    assert_eq!(run.tasks[1].model_attempts, 0);
}
#[test]
fn tags_are_bounded_and_manual_assignment_remains_explicit() {
    assert!(validate_tags(
        &[
            "research".into(),
            "email_read".into(),
            "report.write-2".into()
        ],
        32
    )
    .is_ok());
    for tags in [
        vec!["".into()],
        vec!["Research".into()],
        vec!["two words".into()],
        vec!["_bad".into()],
        vec!["x".repeat(65)],
        vec!["same".into(), "same".into()],
        (0..33).map(|i| format!("tag{i}")).collect(),
    ] {
        assert!(validate_tags(&tags, 32).is_err());
    }
    let mut w = seed();
    let task = &mut w.workflows[1].tasks[0];
    assert_eq!(
        select(task, &w.agents, &w.settings).unwrap().0.id,
        "researcher"
    );
    task.required_capabilities.push("research".into());
    assert!(validate_task(task).is_err());
    task.assignment = TaskAssignment::Capabilities;
    assert!(validate_task(task).is_err());
    task.agent_id.clear();
    task.required_capabilities = (0..17).map(|i| format!("tag{i}")).collect();
    assert!(validate_task(task).is_err());
}
