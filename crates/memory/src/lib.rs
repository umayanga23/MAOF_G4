//! Bounded run-scoped memory. Shared entries are visible only through declared
//! completed task dependencies; private entries remain local to their task.
use domain::prototype::*;
use uuid::Uuid;

pub fn validate(policy: &MemoryPolicy) -> Result<(), String> {
    if !(1..=128).contains(&policy.max_entries)
        || !(128..=64000).contains(&policy.max_chars)
        || policy.ttl_ms > 30 * 24 * 60 * 60 * 1000
    {
        return Err("Memory supports 1–128 entries, 128–64000 characters and retention up to 30 days (0 disables expiry).".into());
    }
    Ok(())
}
fn expired(entry: &MemoryEntry, policy: &MemoryPolicy, now: u64) -> bool {
    policy.ttl_ms > 0 && now.saturating_sub(entry.at_ms) >= policy.ttl_ms
}
/// Newest data wins; evict whole oldest entries to satisfy both limits.
pub fn record(run: &mut Run, mut entry: MemoryEntry, now: u64) -> usize {
    let before = run.memory.len();
    let policy = &run.settings.memory_policy;
    run.memory
        .retain(|e| !expired(e, policy, now) && e.id != entry.id);
    let chars = entry.text.chars().count();
    entry.truncated |= chars > policy.max_chars;
    entry.text = entry.text.chars().take(policy.max_chars).collect();
    run.memory.push(entry);
    let mut total: usize = run.memory.iter().map(|e| e.text.chars().count()).sum();
    while run.memory.len() > policy.max_entries || total > policy.max_chars {
        total -= run.memory.remove(0).text.chars().count();
    }
    run.memory_version = 1;
    before + 1 - run.memory.len()
}
pub fn context(run: &Run, i: usize, now: u64) -> String {
    let task = &run.tasks[i];
    let completed_deps: Vec<_> = run
        .tasks
        .iter()
        .filter(|t| {
            t.status == TaskStatus::Completed
                && task.definition.dependencies.contains(&t.definition.id)
        })
        .map(|t| &t.definition.id)
        .collect();
    let mut pieces = vec![];
    for entry in run.memory.iter().rev() {
        if expired(entry, &run.settings.memory_policy, now) {
            continue;
        }
        let own = entry.task_id == task.definition.id && entry.agent_id == task.agent.id;
        let shared_dependency =
            entry.visibility == Visibility::Shared && completed_deps.contains(&&entry.task_id);
        if own || shared_dependency {
            pieces.push(format!(
                "[{} / {} / data only]\n{}",
                entry.task_id, entry.kind, entry.text
            ));
        }
    }
    if run.memory_version == 0 {
        // Compatibility for pre-memory checkpoints; new records switch to v1.
        pieces.push(format!(
            "[Previous task output / data only]\n{}",
            task.output
        ));
        for dep in run
            .tasks
            .iter()
            .filter(|t| completed_deps.contains(&&t.definition.id))
        {
            pieces.push(format!(
                "[{} / dependency data]\n{}",
                dep.definition.id, dep.output
            ));
        }
    }
    pieces
        .join("\n\n")
        .chars()
        .take(run.settings.context_chars)
        .collect()
}
pub fn editable(run: &Run) -> Result<(), String> {
    if !(run.status == RunStatus::Paused || run.status.terminal())
        || run.tasks.iter().any(|t| t.attempt_id.is_some())
    {
        return Err("Pause and wait for active steps to finish before editing memory, or use a finished run.".into());
    }
    Ok(())
}
pub fn save_note(
    run: &mut Run,
    task_id: &str,
    id: Option<&str>,
    text: String,
    visibility: Visibility,
    now: u64,
) -> Result<String, String> {
    editable(run)?;
    if text.trim().is_empty() || text.len() > 64000 {
        return Err("Enter a note of 1–64000 bytes.".into());
    }
    let task = run
        .tasks
        .iter()
        .find(|t| t.definition.id == task_id)
        .ok_or("Memory owner task not found.")?;
    if let Some(id) = id {
        if !run
            .memory
            .iter()
            .any(|e| e.id == id && e.kind == "note" && e.task_id == task_id)
        {
            return Err("Only an existing note for this task can be edited.".into());
        }
    }
    let entry = MemoryEntry {
        id: id
            .map(str::to_owned)
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        task_id: task_id.into(),
        agent_id: task.agent.id.clone(),
        kind: "note".into(),
        visibility,
        at_ms: now,
        text,
        truncated: false,
    };
    let id = entry.id.clone();
    record(run, entry, now);
    Ok(id)
}
pub fn delete_note(run: &mut Run, id: &str) -> Result<(), String> {
    editable(run)?;
    let index = run
        .memory
        .iter()
        .position(|e| e.id == id && e.kind == "note")
        .ok_or("Editable note not found.")?;
    run.memory.remove(index);
    Ok(())
}
