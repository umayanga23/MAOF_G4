//! Logical-step simulation using the same dispatcher as the app.
use super::*;
use serde::Serialize;
#[derive(Debug, Clone, Serialize)]
pub struct Evaluation {
    pub policy: Policy,
    pub trace: Vec<String>,
    pub completion_quanta: Vec<u64>,
    pub first_service_quanta: Vec<u64>,
    pub mean_completion_quanta: f64,
    pub max_first_service_quanta: u64,
    pub model: String,
}
pub fn compare() -> Vec<Evaluation> {
    [Policy::Fifo, Policy::RoundRobin, Policy::PriorityAging].into_iter().map(|policy| {
        let mut w = seed(); w.settings.policy = policy; w.settings.worker_slots = 1; w.settings.aging_quanta = 1;
        launch(&mut w, "parallel-review", "Fixed three-agent benchmark", 0).unwrap();
        let r = w.runs.last_mut().unwrap();
        let mut trace = vec![]; let mut completion = vec![0; r.tasks.len()]; let mut first = vec![0; r.tasks.len()];
        for q in 1..100 {
            let jobs = dispatch(r, None, q);
            for job in jobs {
                let i = r.tasks.iter().position(|t| t.definition.id == job.task_id).unwrap();
                if first[i] == 0 { first[i] = q; }
                trace.push(job.agent.name.clone());
                complete(r, &job, StepResult { text: "Simulation".into(), tokens: Some(32), error: None }, q);
                if r.tasks[i].completed_steps == r.tasks[i].definition.steps { completion[i] = q; }
            }
            if r.status.terminal() { break; }
        }
        Evaluation { policy, trace, mean_completion_quanta: completion.iter().sum::<u64>() as f64 / completion.len() as f64,
            max_first_service_quanta: first.iter().copied().max().unwrap_or(0), completion_quanta: completion, first_service_quanta: first,
            model: "Logical simulation: 3 agents × 3 unit-duration model steps, 1 worker, no model calls. Priority ranks 3/5/2, aging every quantum. Not a latency or quality benchmark.".into() }
    }).collect()
}
