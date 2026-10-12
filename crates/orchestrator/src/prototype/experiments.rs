//! Seeded, event-driven simulation of the production prototype dispatcher.
//! Virtual milliseconds are synthetic service costs, never measured AI latency.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Workload {
    Balanced,
    Mixed,
    MemoryPressure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub seed: u32,
    pub trials: u32,
    pub tasks: u32,
    pub worker_slots: usize,
    pub memory_mb: u64,
    pub arrival_spread_ms: u64,
    pub max_step_ms: u64,
    pub aging_quanta: u64,
    pub workload: Workload,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            seed: 42,
            trials: 5,
            tasks: 12,
            worker_slots: 2,
            memory_mb: 1024,
            arrival_spread_ms: 100,
            max_step_ms: 100,
            aging_quanta: 2,
            workload: Workload::Mixed,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=20).contains(&self.trials)
            || !(3..=24).contains(&self.tasks)
            || !(1..=8).contains(&self.worker_slots)
            || !(64..=8192).contains(&self.memory_mb)
            || self.arrival_spread_ms > 10000
            || !(1..=1000).contains(&self.max_step_ms)
            || !(1..=1000).contains(&self.aging_quanta)
        {
            return Err("Experiment limits: 1–20 trials, 3–24 tasks, 1–8 workers, 64–8192 MB, arrivals 0–10000 ms, steps 1–1000 ms, aging 1–1000.".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Input {
    pub id: String,
    pub priority: u8,
    pub memory_mb: u64,
    pub release_after_ms: u64,
    pub step_durations_ms: Vec<u64>,
    pub dependencies: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub at_ms: u64,
    pub task_id: String,
    pub step: u32,
    pub duration_ms: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMetrics {
    pub task_id: String,
    pub status: TaskStatus,
    pub release_at_ms: u64,
    pub first_service_at_ms: Option<u64>,
    pub finished_at_ms: Option<u64>,
    pub service_ms: u64,
    pub ready_wait_ms: u64,
    pub resource_wait_ms: u64,
    pub dependency_wait_ms: u64,
    pub response_ms: Option<u64>,
    pub turnaround_ms: Option<u64>,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trial {
    pub trial: u32,
    pub policy: Policy,
    pub makespan_ms: u64,
    pub tasks: Vec<TaskMetrics>,
    pub trace: Vec<Trace>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub policy: Policy,
    pub completed: usize,
    pub rejected: usize,
    pub total: usize,
    pub mean_turnaround_ms: Option<f64>,
    pub p95_turnaround_ms: Option<u64>,
    pub mean_response_ms: Option<f64>,
    pub mean_ready_wait_ms: f64,
    pub mean_resource_wait_ms: f64,
    pub mean_dependency_wait_ms: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub format_version: u32,
    pub engine: String,
    pub config: Config,
    pub workloads: Vec<Vec<Input>>,
    pub trials: Vec<Trial>,
    pub summaries: Vec<Summary>,
    pub notes: String,
}

// Fixed algorithm for cross-platform reproducibility; not for security.
struct Rng(u32);
impl Rng {
    fn below(&mut self, upper: u64) -> u64 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        ((self.0 as u64) * upper) >> 32
    }
}
fn workload(config: &Config, trial: u32) -> Vec<Input> {
    let mut rng = Rng(config.seed.wrapping_add(trial.wrapping_mul(0x9e3779b9)));
    (0..config.tasks)
        .map(|i| {
            let balanced = config.workload == Workload::Balanced;
            let steps = if balanced { 3 } else { 1 + rng.below(8) };
            Input {
                id: format!("task-{:02}", i + 1),
                priority: (1 + rng.below(9)) as u8,
                memory_mb: match config.workload {
                    Workload::Balanced => 128,
                    Workload::Mixed => [128, 256, 512][rng.below(3) as usize],
                    Workload::MemoryPressure => [256, 512, 768, 1024][rng.below(4) as usize],
                },
                release_after_ms: if i == 0 {
                    0
                } else {
                    rng.below(config.arrival_spread_ms + 1)
                },
                step_durations_ms: (0..steps)
                    .map(|_| {
                        if balanced {
                            config.max_step_ms
                        } else {
                            1 + rng.below(config.max_step_ms)
                        }
                    })
                    .collect(),
                dependencies: if config.workload == Workload::Mixed && i > 0 && i % 4 == 3 {
                    vec![format!("task-{:02}", i)]
                } else {
                    vec![]
                },
            }
        })
        .collect()
}

fn simulate(config: &Config, input: &[Input], trial: u32, policy: Policy) -> Result<Trial, String> {
    let mut w = seed();
    w.settings.policy = policy;
    w.settings.worker_slots = config.worker_slots;
    w.settings.memory_mb = config.memory_mb;
    w.settings.aging_quanta = config.aging_quanta;
    w.settings.step_tokens = 32;
    w.settings.token_budget = 1_000_000;
    w.settings.api_budget = 1000;
    let base = w.agents[0].clone();
    w.agents = input
        .iter()
        .map(|t| Agent {
            id: t.id.clone(),
            name: t.id.clone(),
            priority: t.priority,
            memory_mb: t.memory_mb,
            ..base.clone()
        })
        .collect();
    w.workflows = vec![Workflow {
        id: "experiment".into(),
        name: "Seeded scheduler experiment".into(),
        tasks: input
            .iter()
            .map(|t| TaskDefinition {
                id: t.id.clone(),
                name: t.id.clone(),
                agent_id: t.id.clone(),
                assignment: TaskAssignment::Manual,
                required_capabilities: vec![],
                dependencies: t.dependencies.clone(),
                steps: t.step_durations_ms.len() as u32,
                publish_report: false,
                fail_once: false,
                release_after_ms: t.release_after_ms,
                resource_plan: vec![],
                document_id: None,
            })
            .collect(),
    }];
    launch(
        &mut w,
        "experiment",
        "Synthetic workload; no model calls.",
        0,
    )?;
    let run = &mut w.runs[0];
    let mut metrics: Vec<_> = input
        .iter()
        .map(|t| TaskMetrics {
            task_id: t.id.clone(),
            status: TaskStatus::Ready,
            release_at_ms: t.release_after_ms,
            first_service_at_ms: None,
            finished_at_ms: None,
            service_ms: 0,
            ready_wait_ms: 0,
            resource_wait_ms: 0,
            dependency_wait_ms: 0,
            response_ms: None,
            turnaround_ms: None,
            reason: String::new(),
        })
        .collect();
    let mut pending: Vec<(u64, Job)> = vec![];
    let mut trace = vec![];
    let mut now = 0;
    // At most 192 completions + 24 releases; the bound detects engine stalls.
    for _ in 0..1000 {
        pending.sort_by(|a, b| (a.0, &a.1.task_id).cmp(&(b.0, &b.1.task_id)));
        while pending.first().is_some_and(|(at, _)| *at == now) {
            let (_, job) = pending.remove(0);
            complete(
                run,
                &job,
                StepResult {
                    text: "Synthetic result".into(),
                    tokens: Some(32),
                    error: None,
                },
                now,
            );
        }
        for job in dispatch(run, None, now) {
            let i = input.iter().position(|t| t.id == job.task_id).unwrap();
            if metrics[i].first_service_at_ms.is_none() {
                metrics[i].first_service_at_ms = Some(now);
                metrics[i].response_ms = Some(now - input[i].release_after_ms);
            }
            let duration = input[i].step_durations_ms[job.step as usize - 1];
            trace.push(Trace {
                at_ms: now,
                task_id: job.task_id.clone(),
                step: job.step,
                duration_ms: duration,
            });
            pending.push((now + duration, job));
        }
        for (t, m) in run.tasks.iter().zip(&mut metrics) {
            m.status = t.status;
            if t.status.terminal() && m.finished_at_ms.is_none() {
                m.finished_at_ms = Some(now);
                m.reason = t.wait_reason.clone();
                if t.status == TaskStatus::Completed {
                    m.turnaround_ms = Some(now - m.release_at_ms);
                }
            }
        }
        if run.status.terminal() {
            return Ok(Trial {
                trial,
                policy,
                makespan_ms: now,
                tasks: metrics,
                trace,
            });
        }
        let next = pending
            .iter()
            .map(|(at, _)| *at)
            .chain(
                input
                    .iter()
                    .filter(|t| t.release_after_ms > now)
                    .map(|t| t.release_after_ms),
            )
            .min()
            .ok_or("Experiment stalled without a pending completion or arrival.")?;
        let delta = next - now;
        for (t, m) in run.tasks.iter().zip(&mut metrics) {
            if t.status.terminal() {
                continue;
            }
            if t.attempt_id.is_some() {
                m.service_ms += delta;
                continue;
            }
            match &t.wait {
                Some(WaitKind::Arrival) => {}
                Some(WaitKind::Dependency) => m.dependency_wait_ms += delta,
                None | Some(WaitKind::Worker) => m.ready_wait_ms += delta,
                _ => m.resource_wait_ms += delta,
            }
        }
        now = next;
    }
    Err("Experiment exceeded the bounded event count.".into())
}
fn mean(values: &[u64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<u64>() as f64 / values.len() as f64)
}
pub fn compare(config: Config) -> Result<Report, String> {
    config.validate()?;
    let policies = [
        Policy::Fifo,
        Policy::RoundRobin,
        Policy::StrictPriority,
        Policy::PriorityAging,
    ];
    let workloads: Vec<_> = (0..config.trials).map(|t| workload(&config, t)).collect();
    let mut trials = vec![];
    for (i, input) in workloads.iter().enumerate() {
        for policy in policies {
            trials.push(simulate(&config, input, i as u32 + 1, policy)?);
        }
    }
    let summaries = policies
        .into_iter()
        .map(|policy| {
            let tasks: Vec<_> = trials
                .iter()
                .filter(|t| t.policy == policy)
                .flat_map(|t| &t.tasks)
                .collect();
            let mut turnaround: Vec<_> = tasks.iter().filter_map(|t| t.turnaround_ms).collect();
            turnaround.sort_unstable();
            Summary {
                policy,
                completed: turnaround.len(),
                rejected: tasks.len() - turnaround.len(),
                total: tasks.len(),
                mean_turnaround_ms: mean(&turnaround),
                p95_turnaround_ms: if turnaround.is_empty() {
                    None
                } else {
                    Some(turnaround[(turnaround.len() * 95).div_ceil(100) - 1])
                },
                mean_response_ms: mean(
                    &tasks
                        .iter()
                        .filter_map(|t| t.response_ms)
                        .collect::<Vec<_>>(),
                ),
                mean_ready_wait_ms: tasks.iter().map(|t| t.ready_wait_ms).sum::<u64>() as f64
                    / tasks.len() as f64,
                mean_resource_wait_ms: tasks.iter().map(|t| t.resource_wait_ms).sum::<u64>() as f64
                    / tasks.len() as f64,
                mean_dependency_wait_ms: tasks.iter().map(|t| t.dependency_wait_ms).sum::<u64>()
                    as f64
                    / tasks.len() as f64,
            }
        })
        .collect();
    Ok(Report { format_version: 1, engine: "prototype-dispatcher/experiment-v1".into(), config, workloads, trials, summaries,
        notes: "Virtual milliseconds; no AI calls. Each trial uses identical inputs for all policies. Completion means/p95 include completed tasks only; response includes dispatched tasks. Wait means include all tasks. Ready wait includes worker contention; resource wait includes memory/agent/token waits. Wait attribution is exclusive, not a causal decomposition. P95 uses nearest rank. FIFO retains template order within equal release times. Aging uses dispatch counts, not elapsed time. No superiority or starvation-bound claim.".into() })
}
/// Raw observations, with absent response/completion left blank rather than zero.
pub fn csv(report: &Report) -> String {
    let mut text = String::from("trial,policy,task_id,status,release_ms,first_service_ms,finished_ms,service_ms,ready_wait_ms,resource_wait_ms,dependency_wait_ms,response_ms,turnaround_ms\n");
    let optional = |n: Option<u64>| n.map(|n| n.to_string()).unwrap_or_default();
    for trial in &report.trials {
        for t in &trial.tasks {
            text.push_str(&format!(
                "{},{:?},{},{:?},{},{},{},{},{},{},{},{},{}\n",
                trial.trial,
                trial.policy,
                t.task_id,
                t.status,
                t.release_at_ms,
                optional(t.first_service_at_ms),
                optional(t.finished_at_ms),
                t.service_ms,
                t.ready_wait_ms,
                t.resource_wait_ms,
                t.dependency_wait_ms,
                optional(t.response_ms),
                optional(t.turnaround_ms)
            ));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seed_replays_exactly_and_trials_change_inputs() {
        let config = Config::default();
        let a = compare(config.clone()).unwrap();
        let b = compare(config).unwrap();
        assert_eq!(
            serde_json::to_value(&a).unwrap(),
            serde_json::to_value(b).unwrap()
        );
        assert_ne!(
            serde_json::to_value(&a.workloads[0]).unwrap(),
            serde_json::to_value(&a.workloads[1]).unwrap()
        );
        assert_eq!(a.trials.len(), 20);
        assert!(a.summaries.iter().all(|s| s.completed == 60));
        assert_eq!(csv(&a).lines().count(), 241);
    }
    #[test]
    fn time_partitions_and_admission_hold_for_all_presets() {
        for workload in [
            Workload::Balanced,
            Workload::Mixed,
            Workload::MemoryPressure,
        ] {
            let report = compare(Config {
                workload,
                trials: 2,
                memory_mb: 768,
                ..Config::default()
            })
            .unwrap();
            for trial in &report.trials {
                let input = &report.workloads[trial.trial as usize - 1];
                for (i, t) in trial.tasks.iter().enumerate() {
                    if let Some(turnaround) = t.turnaround_ms {
                        assert_eq!(
                            turnaround,
                            t.service_ms
                                + t.ready_wait_ms
                                + t.resource_wait_ms
                                + t.dependency_wait_ms
                        );
                        assert_eq!(t.service_ms, input[i].step_durations_ms.iter().sum::<u64>());
                    }
                    if input[i].memory_mb > report.config.memory_mb {
                        assert_eq!(t.status, TaskStatus::Failed);
                        assert!(t.first_service_at_ms.is_none());
                    }
                }
                for step in &trial.trace {
                    let active: Vec<_> = trial
                        .trace
                        .iter()
                        .filter(|s| s.at_ms <= step.at_ms && s.at_ms + s.duration_ms > step.at_ms)
                        .collect();
                    assert!(active.len() <= report.config.worker_slots);
                    let memory: u64 = active
                        .iter()
                        .map(|s| input.iter().find(|t| t.id == s.task_id).unwrap().memory_mb)
                        .sum();
                    assert!(memory <= report.config.memory_mb);
                }
            }
        }
    }
    #[test]
    fn known_arrival_fixture_has_independent_expected_waits() {
        let input = vec![
            Input {
                id: "a".into(),
                priority: 9,
                memory_mb: 128,
                release_after_ms: 0,
                step_durations_ms: vec![10, 10],
                dependencies: vec![],
            },
            Input {
                id: "b".into(),
                priority: 1,
                memory_mb: 128,
                release_after_ms: 5,
                step_durations_ms: vec![4],
                dependencies: vec![],
            },
        ];
        let config = Config {
            worker_slots: 1,
            ..Config::default()
        };
        let fifo = simulate(&config, &input, 1, Policy::Fifo).unwrap();
        assert_eq!(fifo.tasks[1].response_ms, Some(15));
        assert_eq!(fifo.tasks[1].turnaround_ms, Some(19));
        let priority = simulate(&config, &input, 1, Policy::StrictPriority).unwrap();
        assert_eq!(priority.tasks[1].response_ms, Some(5));
        assert_eq!(priority.tasks[1].turnaround_ms, Some(9));
        assert_eq!(priority.tasks[0].ready_wait_ms, 4);
        assert_eq!(
            priority
                .trace
                .iter()
                .map(|t| t.task_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "a"]
        );
    }
    #[test]
    fn no_completed_tasks_are_reported_as_missing_not_zero() {
        let report = compare(Config {
            memory_mb: 64,
            ..Config::default()
        })
        .unwrap();
        assert!(report.summaries.iter().all(|s| s.completed == 0
            && s.rejected == s.total
            && s.mean_turnaround_ms.is_none()
            && s.p95_turnaround_ms.is_none()));
        assert!(compare(Config {
            trials: 21,
            ..Config::default()
        })
        .is_err());
    }
}
