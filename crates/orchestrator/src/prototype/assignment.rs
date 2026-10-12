//! Launch-time deterministic routing. Dispatch admission and authority remain separate.
use super::*;

pub fn validate_tags(tags: &[String], max: usize) -> Result<(), String> {
    if tags.len() > max
        || tags.iter().collect::<HashSet<_>>().len() != tags.len()
        || tags.iter().any(|t| {
            t.is_empty()
                || t.len() > 64
                || !t.as_bytes()[0].is_ascii_alphanumeric()
                || !t
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_.-".contains(&c))
        })
    {
        return Err(format!("Use at most {max} unique lowercase capability tags, 1–64 characters each: letters, digits, underscore, dot or hyphen."));
    }
    Ok(())
}
pub fn validate_task(task: &TaskDefinition) -> Result<(), String> {
    validate_tags(&task.required_capabilities, 16)?;
    match task.assignment {
        TaskAssignment::Manual if !task.required_capabilities.is_empty()=>
            Err("Manual tasks use the selected agent; remove required capabilities or choose automatic assignment.".into()),
        TaskAssignment::Capabilities if !task.agent_id.is_empty() || task.required_capabilities.is_empty()=>
            Err("Automatic tasks need required capabilities and an empty template agent ID.".into()),
        _=>Ok(()),
    }
}
pub fn select(
    task: &TaskDefinition,
    agents: &[Agent],
    settings: &Settings,
) -> Result<(Agent, Option<AssignmentDecision>), String> {
    validate_task(task)?;
    if task.assignment == TaskAssignment::Manual {
        return agents
            .iter()
            .find(|a| a.id == task.agent_id)
            .cloned()
            .map(|a| (a, None))
            .ok_or_else(|| {
                format!(
                    "Selected agent for task '{}' is no longer registered.",
                    task.name
                )
            });
    }
    let matching: Vec<_> = agents
        .iter()
        .filter(|a| {
            task.required_capabilities
                .iter()
                .all(|tag| a.capabilities.contains(tag))
        })
        .collect();
    if matching.is_empty() {
        return Err(format!("No agent matches all capabilities for task '{}': {}. Add/edit agent tags or use manual assignment.",task.name,task.required_capabilities.join(", ")));
    }
    let mut eligible: Vec<_> = matching
        .into_iter()
        .filter(|a| {
            (!task.publish_report || a.report_permission != Permission::Deny)
                && (task.document_id.is_none() || a.read_permission != Permission::Deny)
                && a.memory_mb <= settings.memory_mb
                && a.token_budget >= settings.step_tokens
                && settings.token_budget >= settings.step_tokens
        })
        .collect();
    eligible.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.id.cmp(&b.id)));
    let Some(agent) = eligible.first() else {
        return Err(format!("Matching agents for task '{}' cannot meet its declared RAM/token limits or document/report permissions. No permission is increased automatically.",task.name));
    };
    validate_agent(agent)?;
    let decision=AssignmentDecision {
        eligible_agent_ids:eligible.iter().map(|a|a.id.clone()).collect(),
        reason:format!("Matched all required tags [{}]; {} eligible agent(s). Chose {} (priority {}, version {}) by highest priority, then stable agent ID. Permissions and dispatch admission remain enforced.",
            task.required_capabilities.join(", "),eligible.len(),agent.name,agent.priority,agent.version),
    };
    Ok(((*agent).clone(), Some(decision)))
}
pub fn validate_pinned(task: &TaskRun) -> Result<(), String> {
    validate_tags(&task.agent.capabilities, 32)?;
    validate_tags(&task.definition.required_capabilities, 16)?;
    if task.definition.assignment == TaskAssignment::Capabilities {
        validate_agent(&task.agent)?;
        let decision = task
            .assignment_decision
            .as_ref()
            .ok_or("Saved automatic assignment has no decision record.")?;
        if task.definition.agent_id != task.agent.id
            || task.definition.required_capabilities.is_empty()
            || !task
                .definition
                .required_capabilities
                .iter()
                .all(|t| task.agent.capabilities.contains(t))
            || decision.reason.is_empty()
            || decision.reason.len() > 2048
            || decision.eligible_agent_ids.is_empty()
            || decision.eligible_agent_ids.len() > 32
            || !decision.eligible_agent_ids.contains(&task.agent.id)
            || decision
                .eligible_agent_ids
                .iter()
                .any(|id| id.is_empty() || id.len() > 80)
            || decision
                .eligible_agent_ids
                .iter()
                .collect::<HashSet<_>>()
                .len()
                != decision.eligible_agent_ids.len()
        {
            return Err(
                "Invalid saved capability assignment; original agent must stay pinned.".into(),
            );
        }
    } else if task.assignment_decision.is_some()
        || !task.definition.required_capabilities.is_empty()
    {
        return Err("Manual assignment checkpoint contains an automatic decision.".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "assignment_tests.rs"]
mod tests;
