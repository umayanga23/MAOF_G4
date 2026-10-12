//! Reserve one report slot before durable dispatch, track every worker and never
//! infer that a remote operation stopped merely because its local task failed.
use super::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::{
    sync::mpsc,
    task::{Id, JoinSet},
};

pub(super) enum Outcome {
    Result(StepResult),
    Handled,
    Failure {
        may_have_called: bool,
        reason: &'static str,
    },
}
struct Report {
    job: Job,
    outcome: Outcome,
}
struct Tracked {
    job: Job,
    sent: Arc<AtomicBool>,
    may_have_called: Arc<AtomicBool>,
}
struct DriverGuard(Arc<transport::Transport>);
impl Drop for DriverGuard {
    fn drop(&mut self) {
        self.0.driver_running.store(false, Ordering::Release);
    }
}
#[cfg(test)]
#[derive(Clone, Copy)]
pub(super) enum TestFault {
    PanicBefore,
    PanicAfterResponse,
    NoReport,
    StallBefore,
}

impl Service {
    /// Exactly one driver lifetime per Service. Reopen its checkpoint after a
    /// driver/runtime shutdown; starting a competing driver cannot replay work.
    pub async fn drive(self: Arc<Self>) {
        if self.1.driver_started.swap(true, Ordering::AcqRel) {
            return;
        }
        self.1.driver_running.store(true, Ordering::Release);
        let _guard = DriverGuard(self.1.clone());
        let (sender, mut reports) = mpsc::channel::<Report>(transport::REPORT_SLOTS);
        let mut workers = JoinSet::new();
        let mut tracked: HashMap<Id, Tracked> = HashMap::new();
        let mut ticks = tokio::time::interval(Duration::from_millis(250));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut n = 0u64;
        loop {
            tokio::select! {
                biased;
                Some(report)=reports.recv()=>{
                    self.1.reports_pending.fetch_sub(1,Ordering::Relaxed);
                    self.accept_outcome(&report.job,report.outcome).await;
                }
                Some(joined)=workers.join_next_with_id(), if !workers.is_empty()=>{
                    let (id,reason)=match joined {
                        Ok((id,()))=>(id,None),
                        Err(error)=>(error.id(),Some(if error.is_panic(){"WorkerPanicked: worker failed; panic payload is not exposed."}else{"WorkerCancelled: local worker ended without a result; remote effects may remain."})),
                    };
                    if let Some(worker)=tracked.remove(&id) {
                        self.1.active_workers.store(tracked.len() as u64,Ordering::Relaxed);
                        if let Some(reason)=reason.or_else(||(!worker.sent.load(Ordering::Acquire)).then_some("WorkerMissingResult: worker exited without reporting its outcome.")) {
                            // A failed worker dropped its pre-reserved report slot.
                            // Never await a send while the driver is its receiver.
                            match sender.clone().try_reserve_owned() {
                                Ok(permit)=>{
                                    self.1.reports_pending.fetch_add(1,Ordering::Relaxed);
                                    permit.send(Report {job:worker.job,outcome:Outcome::Failure {may_have_called:worker.may_have_called.load(Ordering::Acquire),reason}});
                                }
                                Err(_)=>{
                                    // Defensive fallback still settles the original fence.
                                    self.accept_outcome(&worker.job,Outcome::Failure {may_have_called:worker.may_have_called.load(Ordering::Acquire),reason}).await;
                                }
                            }
                        }
                    }
                }
                _=ticks.tick()=>{
                    n+=1;
                    let mut permits=Vec::new();
                    for _ in workers.len()..transport::REPORT_SLOTS {
                        match sender.clone().try_reserve_owned(){Ok(p)=>permits.push(p),Err(_)=>break}
                    }
                    let jobs=match self.dispatch_jobs(permits.len(),n%4==0).await {
                        Ok(jobs)=>jobs,Err(_)=>continue,
                    };
                    for job in jobs {
                        let permit=permits.pop().expect("dispatch cannot exceed reserved report capacity");
                        let service=self.clone();
                        let sent=Arc::new(AtomicBool::new(false));
                        let may_have_called=Arc::new(AtomicBool::new(false));
                        let record=Tracked {job:job.clone(),sent:sent.clone(),may_have_called:may_have_called.clone()};
                        #[cfg(test)]
                        let fault=self.1.faults.lock().unwrap().remove(&job.task_id);
                        let handle=if matches!(job.kind,JobKind::ReadDocument(_)) {
                            workers.spawn_blocking(move||{
                                #[cfg(test)]
                                if matches!(fault,Some(TestFault::PanicBefore)){panic!("synthetic private panic payload");}
                                service.execute_document(job.clone());
                                service.1.reports_pending.fetch_add(1,Ordering::Relaxed);
                                permit.send(Report {job,outcome:Outcome::Handled});
                                sent.store(true,Ordering::Release);
                            })
                        }else{
                            workers.spawn(async move {
                                #[cfg(test)]
                                if matches!(fault,Some(TestFault::NoReport)){return;}
                                let operation=async {
                                    #[cfg(test)]
                                    match fault {
                                        Some(TestFault::PanicBefore)=>panic!("synthetic private panic payload"),
                                        Some(TestFault::StallBefore)=>std::future::pending::<()>().await,
                                        _=>{}
                                    }
                                    let result=if job.settings.mode!=Mode::Demo&&job.agent.external.as_ref().is_some_and(|e|e.protocol==ExternalProtocol::A2aJsonRpc) {
                                        service.execute_remote(&job).await
                                    }else {Some(service.execute_agent_marked(&job,&may_have_called).await)};
                                    #[cfg(test)]
                                    if matches!(fault,Some(TestFault::PanicAfterResponse)){panic!("synthetic private panic payload");}
                                    result.map(Outcome::Result).unwrap_or(Outcome::Handled)
                                };
                                let deadline=service.worker_deadline(&job);
                                let outcome=match tokio::time::timeout(deadline,operation).await {
                                    Ok(outcome)=>outcome,
                                    Err(_)=>Outcome::Failure {may_have_called:may_have_called.load(Ordering::Acquire),reason:"WorkerTimedOut: local observation deadline reached; possible remote usage is retained."},
                                };
                                let outcome=bounded(outcome);
                                service.1.reports_pending.fetch_add(1,Ordering::Relaxed);
                                permit.send(Report {job,outcome});
                                sent.store(true,Ordering::Release);
                            })
                        };
                        tracked.insert(handle.id(),record);
                        self.1.active_workers.store(tracked.len() as u64,Ordering::Relaxed);
                    }
                }
            }
        }
    }
    async fn dispatch_jobs(&self, capacity: usize, sample_host: bool) -> Result<Vec<Job>, String> {
        let gate = self.2.clone().lock_owned().await;
        let mut s = self.0.lock().await;
        if sample_host {
            s.host = s.monitor.sample();
        }
        if let Some(error) = &s.fault {
            return Err(error.clone());
        }
        let available = s.host.available_memory_mb;
        let mut next = s.workspace.clone();
        drop(s);
        let mut changed = false;
        let jobs = if let Some(run) = next.runs.iter_mut().find(|r| !r.status.terminal()) {
            let sequence = run.event_sequence;
            let status = run.status;
            let jobs = engine::dispatch_with_capacity(run, Some(available), now(), capacity);
            changed = sequence != run.event_sequence || status != run.status || !jobs.is_empty();
            if capacity == 0
                && run
                    .tasks
                    .iter()
                    .any(|t| t.wait_reason.contains("transport capacity"))
            {
                self.1.transport_waits.fetch_add(1, Ordering::Relaxed);
            }
            jobs
        } else {
            vec![]
        };
        if changed {
            self.3.commit(next, self.0.clone(), gate, None).await?;
        }
        Ok(jobs)
    }
    fn worker_deadline(&self, job: &Job) -> Duration {
        #[cfg(test)]
        {
            let value = self.1.deadline_ms.load(Ordering::Relaxed);
            if value > 0 {
                return Duration::from_millis(value);
            }
        }
        if job.settings.mode == Mode::Demo {
            Duration::from_millis(job.settings.demo_delay_ms + 2000)
        } else {
            Duration::from_secs(
                job.agent
                    .external
                    .as_ref()
                    .map(|a| a.timeout_seconds)
                    .unwrap_or(60)
                    + 5,
            )
        }
    }
    pub(super) async fn accept_outcome(&self, job: &Job, outcome: Outcome) {
        if matches!(outcome, Outcome::Handled) {
            return;
        }
        let gate = self.2.clone().lock_owned().await;
        let s = self.0.lock().await;
        if s.fault.is_some() {
            return;
        }
        let mut next = s.workspace.clone();
        drop(s);
        let failure = matches!(&outcome, Outcome::Failure { .. });
        if let Some(run) = next.runs.iter_mut().find(|r| r.id == job.run_id) {
            let accepted = match outcome {
                Outcome::Result(result) => engine::complete(run, job, result, now()),
                Outcome::Failure {
                    may_have_called,
                    reason,
                } => engine::worker_failed(run, job, may_have_called, reason, now()),
                Outcome::Handled => false,
            };
            if accepted {
                if self
                    .3
                    .commit(next, self.0.clone(), gate, None)
                    .await
                    .is_ok()
                    && failure
                {
                    self.1.worker_failures.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
}
fn bounded(outcome: Outcome) -> Outcome {
    if let Outcome::Result(mut result) = outcome {
        // Matches the coordinator's existing persisted model output bound.
        result.text = result.text.chars().take(16000).collect();
        result.error = result.error.map(|text| text.chars().take(512).collect());
        Outcome::Result(result)
    } else {
        outcome
    }
}
