//! A2A lifecycle: every submission/status/cancel boundary is checkpointed.
use super::*;
use llm::external::{A2aClient, A2aObservation};

impl Service {
    pub(super) async fn execute_remote(&self, job: &Job) -> Option<StepResult> {
        let operation = self.remote_loop(job);
        let result = tokio::time::timeout(
            Duration::from_secs(job.agent.external.as_ref().unwrap().timeout_seconds),
            operation,
        )
        .await;
        match result {
            Ok(Ok(result)) => Some(result),
            result => {
                let detail = match result {
                    Ok(Err(error)) => error,
                    Err(_) => "A2A observation timed out. Remote work may still be running; resume to check the saved task, not resubmit it.".into(),
                    _ => unreachable!(),
                };
                let gate = self.2.clone().lock_owned().await;
                let s = self.0.lock().await;
                if s.fault.is_some() {
                    return None;
                }
                let mut next = s.workspace.clone();
                drop(s);
                if let Some(run) = next.runs.iter_mut().find(|r| r.id == job.run_id) {
                    if engine::remote::suspend(run, job, detail, now()) {
                        let _ = self.3.commit(next, self.0.clone(), gate, None).await;
                    }
                }
                None
            }
        }
    }
    async fn remote_snapshot(&self, job: &Job) -> Result<(RemoteTask, bool), String> {
        let s = self.0.lock().await;
        if s.fault.is_some() {
            return Err("Storage fault; remote observation stopped.".into());
        }
        let run = s
            .workspace
            .runs
            .iter()
            .find(|r| {
                r.id == job.run_id
                    && matches!(
                        r.status,
                        RunStatus::Running | RunStatus::Paused | RunStatus::Stopping
                    )
            })
            .ok_or("Run no longer accepts this remote operation.")?;
        let task = run
            .tasks
            .iter()
            .find(|t| {
                t.definition.id == job.task_id
                    && t.agent.version == job.agent.version
                    && t.attempt_id.as_deref() == Some(&job.attempt_id)
            })
            .ok_or("Remote operation fence no longer matches this task.")?;
        let remote = task
            .remote
            .clone()
            .filter(|r| r.pending && r.step == job.step)
            .ok_or("No unresolved remote task for this step.")?;
        Ok((remote, run.status == RunStatus::Stopping))
    }
    async fn remote_save(
        &self,
        job: &Job,
        sending: bool,
        kind: &str,
        update: impl FnOnce(&mut RemoteTask),
    ) -> Result<(), String> {
        let gate = self.2.clone().lock_owned().await;
        let s = self.0.lock().await;
        if s.fault.is_some() {
            return Err("Storage fault; remote observation stopped.".into());
        }
        let mut next = s.workspace.clone();
        drop(s);
        let run = next
            .runs
            .iter_mut()
            .find(|r| {
                r.id == job.run_id && !r.status.terminal() && r.status != RunStatus::Interrupted
            })
            .ok_or("Remote run fence changed.")?;
        if sending && run.status == RunStatus::Stopping {
            return Err("Stop accepted before A2A submission; no request sent.".into());
        }
        let task = run
            .tasks
            .iter_mut()
            .find(|t| {
                t.definition.id == job.task_id
                    && t.agent.version == job.agent.version
                    && t.attempt_id.as_deref() == Some(&job.attempt_id)
            })
            .ok_or("Remote attempt fence changed.")?;
        let remote = task
            .remote
            .as_mut()
            .filter(|r| r.pending)
            .ok_or("Remote step already settled.")?;
        update(remote);
        remote.updated_at = now();
        let message = format!(
            "Remote state: {}; cancellation: {:?}. {}",
            remote.state, remote.cancellation, remote.detail
        );
        engine::event(run, now(), kind, Some(job.task_id.clone()), message);
        self.3.commit(next, self.0.clone(), gate, None).await
    }
    async fn remote_observed(&self, job: &Job, observation: &A2aObservation) -> Result<(), String> {
        let (previous, _) = self.remote_snapshot(job).await?;
        if previous
            .task_id
            .as_ref()
            .is_some_and(|id| observation.task_id.as_ref() != Some(id))
        {
            return Err("Remote response identity changed; original identity retained.".into());
        }
        self.remote_save(job, false, "remote_status", |r| {
            r.task_id = observation.task_id.clone();
            r.state = observation.state.clone();
            r.detail = if r.state == "canceled" {
                "Remote cancellation confirmed. This does not undo prior effects.".into()
            } else {
                "Observed the saved remote task; no new submission.".into()
            };
            if r.state == "canceled" {
                r.cancellation = RemoteCancellation::Confirmed;
            } else if observation.result.is_some() && r.cancellation != RemoteCancellation::None {
                r.cancellation = RemoteCancellation::Unconfirmed;
                r.detail =
                    "Remote task ended without confirming cancellation; prior effects may exist."
                        .into();
            }
        })
        .await
    }
    async fn remote_loop(&self, job: &Job) -> Result<StepResult, String> {
        let client = A2aClient::new(job.agent.external.as_ref().unwrap())?;
        let (saved, stopping) = self.remote_snapshot(job).await?;
        let mut observation = if let Some(id) = saved.task_id.as_deref() {
            // Stopping should request cancel before doing a status fetch.
            if stopping {
                None
            } else {
                Some(
                    client
                        .get(&format!("{}-resume", job.attempt_id), id)
                        .await?,
                )
            }
        } else if saved.state == "queued" {
            if stopping {
                self.remote_save(job, false, "remote_not_sent", |r| {
                    r.state = "not_submitted".into();
                    r.detail = "Stop accepted before submission; no remote work started.".into();
                    r.cancellation = RemoteCancellation::Confirmed;
                })
                .await?;
                return Ok(StepResult {
                    text: String::new(),
                    tokens: Some(0),
                    error: Some("Stopped before remote submission.".into()),
                });
            }
            let submission = self
                .remote_save(job, true, "remote_submission", |r| {
                    r.state = "submitting".into();
                    r.detail =
                        "Submission may be in flight; never automatically replay this identity."
                            .into();
                })
                .await;
            if let Err(error) = submission {
                // Stop may have arrived between the snapshot and durable gate.
                if self.remote_snapshot(job).await?.1 {
                    self.remote_save(job, false, "remote_not_sent", |r| {
                        r.state = "not_submitted".into();
                        r.detail =
                            "Stop accepted before submission; no remote work started.".into();
                        r.cancellation = RemoteCancellation::Confirmed;
                    })
                    .await?;
                    return Ok(StepResult {
                        text: String::new(),
                        tokens: Some(0),
                        error: Some("Stopped before remote submission.".into()),
                    });
                }
                return Err(error);
            }
            Some(client.send(job, &saved.submission_id).await?)
        } else {
            return Err("Submission outcome unknown and remote task ID unavailable. Review the remote service; this task will not be submitted again.".into());
        };
        for poll in 0..120 {
            if let Some(value) = observation.take() {
                self.remote_observed(job, &value).await?;
                if let Some(result) = value.result {
                    return Ok(result);
                }
                if matches!(value.state.as_str(), "input-required" | "auth-required") {
                    return Err("Remote agent needs input/authentication. Interactive continuation is not implemented; saved identity retained.".into());
                }
            }
            let (saved, stopping) = self.remote_snapshot(job).await?;
            let id = saved
                .task_id
                .as_deref()
                .ok_or("Remote identity unavailable; no replay permitted.")?;
            if stopping
                && matches!(
                    saved.cancellation,
                    RemoteCancellation::None | RemoteCancellation::Requested
                )
            {
                // Persist intent before RPC. A crash/timeout after this point never
                // assumes the remote cancellation was accepted, or blindly retries.
                self.remote_save(job, false, "remote_cancel_requested", |r| {
                    r.cancellation = RemoteCancellation::Sent;
                    r.detail = "Cancellation requested; awaiting remote confirmation.".into();
                })
                .await?;
                match client.cancel(&format!("{}-cancel",job.attempt_id),id).await {
                    Ok(value)=>observation=Some(value),
                    Err(error)=>self.remote_save(job,false,"remote_cancel_unconfirmed",|r| {
                        r.cancellation=RemoteCancellation::Unconfirmed;
                        r.detail=format!("Cancellation not confirmed. {error} Remote work may still be running.");
                    }).await?,
                }
                if observation.is_some() {
                    continue;
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
            observation = Some(
                client
                    .get(&format!("{}-poll-{poll}", job.attempt_id), id)
                    .await?,
            );
        }
        Err("Remote status polling limit reached; saved identity retained.".into())
    }
}
