//! Experimental single-owner, exclusive logical resources. No OS/tool effects.
//! All graph reads and recovery mutations happen in the coordinator transaction.
use super::*;

pub(super) fn acquire_next(run: &mut Run, i: usize, now: u64) {
    let task = &run.tasks[i];
    let Some(resource) = task
        .definition
        .resource_plan
        .get(task.completed_steps as usize)
        .cloned()
    else {
        return;
    };
    if run.resource_owners.contains_key(&resource) {
        return;
    }
    let id = task.definition.id.clone();
    run.resource_owners.insert(resource.clone(), id.clone());
    run.resource_version += 1;
    event(
        run,
        now,
        "resource_acquired",
        Some(id),
        format!("Acquired demo logical resource {resource}."),
    );
}

fn release(run: &mut Run, owner: &str, now: u64) -> Vec<String> {
    let resources: Vec<_> = run
        .resource_owners
        .iter()
        .filter(|(_, v)| *v == owner)
        .map(|(k, _)| k.clone())
        .collect();
    for resource in &resources {
        run.resource_owners.remove(resource);
        run.resource_version += 1;
        event(
            run,
            now,
            "resource_released",
            Some(owner.into()),
            format!("Released demo logical resource {resource}."),
        );
    }
    resources
}

pub(super) fn release_finished(run: &mut Run, now: u64) {
    let finished: Vec<_> = run
        .tasks
        .iter()
        .filter(|t| {
            t.attempt_id.is_none()
                && (t.status.terminal() || t.completed_steps == t.definition.steps)
        })
        .map(|t| t.definition.id.clone())
        .collect();
    for id in finished {
        release(run, &id, now);
    }
}

fn edge(run: &Run, id: &str) -> Option<ResourceEdge> {
    let task = run.tasks.iter().find(|t| t.definition.id == id)?;
    if task.status != TaskStatus::Waiting || task.attempt_id.is_some() {
        return None;
    }
    let Some(WaitKind::Resource { resource, owner }) = &task.wait else {
        return None;
    };
    // A stale wait label is not evidence of present ownership.
    if run.resource_owners.get(resource) != Some(owner) {
        return None;
    }
    let holder = run.tasks.iter().find(|t| &t.definition.id == owner)?;
    if holder.status.terminal() || holder.completed_steps == holder.definition.steps {
        return None;
    }
    Some(ResourceEdge {
        waiter: id.into(),
        resource: resource.clone(),
        owner: owner.clone(),
    })
}

pub fn detect(run: &Run) -> Option<DeadlockWitness> {
    if run.status != RunStatus::Running || run.settings.mode != Mode::Demo {
        return None;
    }
    for task in &run.tasks {
        let mut path: Vec<ResourceEdge> = vec![];
        let mut current = task.definition.id.clone();
        while let Some(next) = edge(run, &current) {
            if let Some(start) = path.iter().position(|e| e.waiter == current) {
                return Some(DeadlockWitness {
                    resource_version: run.resource_version,
                    edges: path[start..].to_vec(),
                });
            }
            current = next.owner.clone();
            path.push(next);
        }
    }
    None
}

/// Reject stale snapshots, then cancel one inactive victim. Never widen authority,
/// replay a tool, or release an active worker's resources.
pub fn recover(run: &mut Run, witness: &DeadlockWitness, now: u64) -> bool {
    if run.status != RunStatus::Running
        || run.settings.mode != Mode::Demo
        || run.resource_version != witness.resource_version
        || witness.edges.len() < 2
        || witness
            .edges
            .iter()
            .map(|e| &e.waiter)
            .collect::<HashSet<_>>()
            .len()
            != witness.edges.len()
        || witness.edges.iter().enumerate().any(|(i, e)| {
            edge(run, &e.waiter).as_ref() != Some(e)
                || e.owner != witness.edges[(i + 1) % witness.edges.len()].waiter
        })
    {
        return false;
    }
    let i = run
        .tasks
        .iter()
        .enumerate()
        .filter(|(_, t)| witness.edges.iter().any(|e| e.waiter == t.definition.id))
        .max_by_key(|(_, t)| (t.agent.priority, &t.definition.id))
        .unwrap()
        .0;
    let victim = run.tasks[i].definition.id.clone();
    let chain = witness
        .edges
        .iter()
        .map(|e| format!("{} --{}--> {}", e.waiter, e.resource, e.owner))
        .collect::<Vec<_>>()
        .join("; ");
    event(
        run,
        now,
        "deadlock_confirmed",
        None,
        format!(
            "Logical resource cycle at version {}: {chain}",
            witness.resource_version
        ),
    );
    run.tasks[i].status = TaskStatus::Cancelled;
    run.tasks[i].wait = None;
    run.tasks[i].wait_reason =
        "Cancelled to break a confirmed logical resource cycle; surviving tasks continue.".into();
    let released = release(run, &victim, now);
    run.deadlock_recoveries.push(DeadlockRecovery {
        at_ms: now,
        witness: witness.clone(),
        victim: victim.clone(),
        released,
    });
    event(run, now, "deadlock_recovered", Some(victim),
        "Cancelled the lowest-priority cycle member (highest task ID breaks ties). Released only its logical resources; no automatic retry.".into());
    true
}

/// A pinned fixture goes through the real driver and persistence, without changing
/// the user's settings, agents or saved templates.
pub fn launch_demo(w: &mut Workspace, participants: usize, now: u64) -> Result<String, String> {
    if !(2..=3).contains(&participants) {
        return Err("Choose two or three participants.".into());
    }
    if w.runs.iter().any(|r| !r.status.terminal()) || w.runs.len() >= 100 {
        return Err(
            "Finish the active run and keep history below 100 runs before starting this demo."
                .into(),
        );
    }
    let mut fixture = seed();
    fixture.settings.mode = Mode::Demo;
    fixture.settings.policy = Policy::RoundRobin;
    fixture.settings.worker_slots = participants;
    fixture.settings.demo_delay_ms = 700;
    let wf = &mut fixture.workflows[1];
    wf.name = format!("{participants}-agent logical resource deadlock");
    wf.tasks.truncate(participants);
    for (i, task) in wf.tasks.iter_mut().enumerate() {
        task.id = format!("task-{}", i + 1);
        task.name = format!(
            "Acquire resource {} then {}",
            i + 1,
            (i + 1) % participants + 1
        );
        task.steps = 2;
        task.resource_plan = vec![
            format!("resource-{}", i + 1),
            format!("resource-{}", (i + 1) % participants + 1),
        ];
    }
    let id = launch(&mut fixture, "parallel-review", "Controlled demo: observe ownership, confirm a cycle, cancel one victim, and let the survivors finish.", now)?;
    w.runs.push(fixture.runs.remove(0));
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn result() -> StepResult {
        StepResult {
            text: "Demo".into(),
            tokens: Some(32),
            error: None,
        }
    }
    fn cycle(size: usize) -> Run {
        let mut w = seed();
        launch_demo(&mut w, size, 0).unwrap();
        w.runs.remove(0)
    }
    #[test]
    fn two_and_three_member_cycles_recover_with_survivor_progress() {
        for size in [2, 3] {
            let mut r = cycle(size);
            let jobs = dispatch(&mut r, None, 0);
            assert_eq!(jobs.len(), size);
            assert!(detect(&r).is_none()); // owners are doing useful work
            for job in jobs {
                complete(&mut r, &job, result(), 1);
            }
            dispatch(&mut r, None, 1);
            assert_eq!(r.deadlock_recoveries.len(), 1);
            let recovery = &r.deadlock_recoveries[0];
            assert_eq!(recovery.witness.edges.len(), size);
            assert_eq!(recovery.victim, "task-2"); // analyst priority 5
            assert_eq!(recovery.released, vec!["resource-2"]);
            assert_eq!(r.resource_owners.len(), size - 1);
            for tick in 2..20 {
                for job in dispatch(&mut r, None, tick) {
                    complete(&mut r, &job, result(), tick);
                }
                if r.status.terminal() {
                    break;
                }
            }
            assert_eq!(r.status, RunStatus::Failed); // not falsely reported as all-success
            assert_eq!(
                r.tasks
                    .iter()
                    .filter(|t| t.status == TaskStatus::Completed)
                    .count(),
                size - 1
            );
            assert!(r.resource_owners.is_empty());
            assert!(r
                .tasks
                .iter()
                .all(|t| t.agent.report_permission == Permission::Ask));
        }
    }
    #[test]
    fn benign_contention_and_stop_do_not_cancel_an_active_owner() {
        let mut r = cycle(2);
        for t in &mut r.tasks {
            t.definition.resource_plan = vec!["shared".into()];
        }
        let jobs = dispatch(&mut r, None, 0);
        assert_eq!(jobs.len(), 1);
        assert!(detect(&r).is_none());
        control(&mut r, "stop", 1).unwrap();
        assert_eq!(r.resource_owners.len(), 1);
        complete(&mut r, &jobs[0], result(), 2);
        assert!(r.resource_owners.is_empty());
        assert_eq!(r.status, RunStatus::Cancelled);
        assert!(r.deadlock_recoveries.is_empty());
    }
    #[test]
    fn stale_witness_is_rejected_and_ties_are_deterministic() {
        let mut r = cycle(2);
        for job in dispatch(&mut r, None, 0) {
            complete(&mut r, &job, result(), 1);
        }
        // Build the authoritative wait snapshot without invoking automatic recovery.
        for i in 0..2 {
            r.tasks[i].agent.priority = 4;
            r.tasks[i].status = TaskStatus::Waiting;
            r.tasks[i].wait = Some(WaitKind::Resource {
                resource: format!("resource-{}", 2 - i),
                owner: format!("task-{}", 2 - i),
            });
        }
        let witness = detect(&r).unwrap();
        r.resource_version += 1;
        assert!(!recover(&mut r, &witness, 2));
        let witness = detect(&r).unwrap();
        r.tasks[0].status = TaskStatus::Ready;
        assert!(!recover(&mut r, &witness, 2));
        r.tasks[0].status = TaskStatus::Waiting;
        assert!(recover(&mut r, &witness, 2));
        assert_eq!(r.deadlock_recoveries[0].victim, "task-2");
        assert!(!recover(&mut r, &witness, 3));
    }
    #[test]
    fn pause_restart_and_approval_preserve_resource_lifetimes() {
        let mut w = seed();
        launch_demo(&mut w, 2, 0).unwrap();
        let jobs = dispatch(&mut w.runs[0], None, 0);
        interrupt_on_restart(&mut w, 1);
        let r = &mut w.runs[0];
        assert_eq!(r.resource_owners.len(), 2);
        assert!(detect(r).is_none());
        assert!(!complete(r, &jobs[0], result(), 2));
        control(r, "recover", 2).unwrap();
        control(r, "stop", 3).unwrap();
        assert!(r.resource_owners.is_empty());
        let mut r = cycle(2);
        r.tasks.truncate(1);
        r.tasks[0].definition.resource_plan = vec!["one".into()];
        r.tasks[0].definition.steps = 1;
        r.tasks[0].definition.publish_report = true;
        let j = dispatch(&mut r, None, 0).remove(0);
        control(&mut r, "pause", 1).unwrap();
        complete(&mut r, &j, result(), 2);
        assert!(r.resource_owners.is_empty());
        control(&mut r, "resume", 3).unwrap();
        dispatch(&mut r, None, 3);
        assert_eq!(r.tasks[0].status, TaskStatus::Approval);
        assert!(detect(&r).is_none());
    }
    #[test]
    fn resource_experiments_are_bounded_and_demo_only() {
        let mut w = seed();
        w.workflows[1].tasks[0].resource_plan = vec!["one".into()];
        w.settings.mode = Mode::Ollama;
        assert!(launch(&mut w, "parallel-review", "test", 0).is_err());
        w.settings.mode = Mode::Demo;
        w.workflows[1].tasks[1].dependencies = vec!["research".into()];
        assert!(launch(&mut w, "parallel-review", "test", 0).is_err());
    }
}
