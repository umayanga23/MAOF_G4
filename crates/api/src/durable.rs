//! Serialized checkpoint replacement outside the UI/state mutex. A queued
//! commit owns its transaction gate through save AND publication, even when
//! the submitting future disappears. This is a checkpoint writer, not a journal.
use super::*;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex as StdMutex, Weak,
};
use tokio::sync::{mpsc, oneshot, OwnedMutexGuard, OwnedSemaphorePermit, Semaphore};

pub(super) const COMMIT_SLOTS: usize = 1;
pub(super) const EXPORT_SLOTS: usize = 2;
type Export = Box<dyn FnOnce(&Store) -> Result<String, String> + Send>;
struct Commit {
    next: Arc<Workspace>,
    state: Arc<Mutex<Inner>>,
    gate: OwnedMutexGuard<()>,
    before: Option<Export>,
    reply: oneshot::Sender<Result<(), String>>,
}
struct ExportRequest {
    operation: Export,
    store: Arc<Store>,
    reply: oneshot::Sender<Result<String, String>>,
    permit: OwnedSemaphorePermit,
}
struct Inbox {
    commits: mpsc::Receiver<Commit>,
    exports: mpsc::Receiver<ExportRequest>,
    pumping: bool,
    burst: usize,
}
pub(super) struct Writer {
    commits: mpsc::Sender<Commit>,
    exports: mpsc::Sender<ExportRequest>,
    inbox: StdMutex<Inbox>,
    store: Weak<Store>,
    busy: AtomicBool,
    committed: AtomicU64,
    failures: AtomicU64,
    rejected_exports: AtomicU64,
    last_commit_ms: AtomicU64,
    export_budget: Arc<Semaphore>,
    #[cfg(test)]
    pub test_hook: StdMutex<Option<TestHook>>,
}
#[derive(Serialize)]
pub struct WriterHealth {
    pub commit_capacity: usize,
    pub export_capacity: usize,
    pub commits_pending: usize,
    pub exports_pending: usize,
    pub busy: bool,
    pub committed_checkpoints: u64,
    pub failures: u64,
    pub rejected_exports: u64,
    pub last_commit_ms: u64,
    pub exports_in_flight: usize,
    pub export_in_flight_limit: usize,
}
#[cfg(test)]
pub(super) struct TestHook {
    pub entered: Arc<tokio::sync::Notify>,
    pub release: Arc<(StdMutex<bool>, std::sync::Condvar)>,
    pub panic: bool,
}
impl Writer {
    pub fn new(store: Weak<Store>) -> Self {
        let (commits, commit_rx) = mpsc::channel(COMMIT_SLOTS);
        let (exports, export_rx) = mpsc::channel(EXPORT_SLOTS);
        Self {
            commits,
            exports,
            store,
            inbox: StdMutex::new(Inbox {
                commits: commit_rx,
                exports: export_rx,
                pumping: false,
                burst: 0,
            }),
            busy: AtomicBool::new(false),
            committed: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            rejected_exports: AtomicU64::new(0),
            last_commit_ms: AtomicU64::new(0),
            export_budget: Arc::new(Semaphore::new(EXPORT_SLOTS + 1)),
            #[cfg(test)]
            test_hook: StdMutex::new(None),
        }
    }
    pub fn health(&self) -> WriterHealth {
        WriterHealth {
            commit_capacity: COMMIT_SLOTS,
            export_capacity: EXPORT_SLOTS,
            commits_pending: COMMIT_SLOTS - self.commits.capacity(),
            exports_pending: EXPORT_SLOTS - self.exports.capacity(),
            busy: self.busy.load(Ordering::Relaxed),
            committed_checkpoints: self.committed.load(Ordering::Relaxed),
            failures: self.failures.load(Ordering::Relaxed),
            rejected_exports: self.rejected_exports.load(Ordering::Relaxed),
            last_commit_ms: self.last_commit_ms.load(Ordering::Relaxed),
            exports_in_flight: EXPORT_SLOTS + 1 - self.export_budget.available_permits(),
            export_in_flight_limit: EXPORT_SLOTS + 1,
        }
    }
    pub async fn commit(
        self: &Arc<Self>,
        next: Workspace,
        state: Arc<Mutex<Inner>>,
        gate: OwnedMutexGuard<()>,
        before: Option<Export>,
    ) -> Result<(), String> {
        let (reply, receipt) = oneshot::channel();
        let enqueue_error = {
            let mut inbox = self.inbox.lock().unwrap();
            match self.commits.try_send(Commit {
                    next: Arc::new(next),
                    state: state.clone(),
                    gate,
                    before,
                    reply,
                }) {
                Ok(()) => { self.start(&mut inbox); None }
                Err(_) => Some("WriterUnavailable: checkpoint was not accepted; dispatch disabled. Reopen the workspace.".to_string()),
            }
        };
        if let Some(error) = enqueue_error {
            state.lock().await.fault = Some(error.clone());
            self.failures.fetch_add(1, Ordering::Relaxed);
            return Err(error);
        }
        receipt.await.map_err(|_| {
            "WriterReplyLost: checkpoint outcome unknown; reopen and inspect the workspace."
                .to_string()
        })?
    }
    pub async fn export(self: &Arc<Self>, operation: Export) -> Result<String, String> {
        let permit = self.reserve_export()?;
        self.export_prepared(operation, permit).await
    }
    pub fn reserve_export(&self) -> Result<OwnedSemaphorePermit, String> {
        self.export_budget.clone().try_acquire_owned().map_err(|_| {
            self.rejected_exports.fetch_add(1, Ordering::Relaxed);
            "WriterQueueFull: export preparation capacity is full; this export was not accepted."
                .to_string()
        })
    }
    pub async fn export_prepared(
        self: &Arc<Self>,
        operation: Export,
        permit: OwnedSemaphorePermit,
    ) -> Result<String, String> {
        let store = self
            .store
            .upgrade()
            .ok_or("WriterClosed: workspace closed.")?;
        let (reply, receipt) = oneshot::channel();
        {
            let mut inbox = self.inbox.lock().unwrap();
            self.exports
                .try_send(ExportRequest {
                    operation,
                    store,
                    reply,
                    permit,
                })
                .map_err(|_| {
                    self.rejected_exports.fetch_add(1, Ordering::Relaxed);
                    "WriterQueueFull: export queue is full; this export was not accepted."
                        .to_string()
                })?;
            self.start(&mut inbox);
        }
        receipt.await.map_err(|_| {
            "WriterReplyLost: export outcome unknown; inspect the exports directory.".to_string()
        })?
    }
    fn start(self: &Arc<Self>, inbox: &mut Inbox) {
        if !inbox.pumping {
            inbox.pumping = true;
            tokio::spawn(self.clone().pump());
        }
    }
    async fn pump(self: Arc<Self>) {
        enum Request {
            Commit(Commit),
            Export(ExportRequest),
        }
        loop {
            let request = {
                let mut i = self.inbox.lock().unwrap();
                let next = if i.burst < 4 {
                    i.commits.try_recv().ok().map(|r| {
                        i.burst += 1;
                        Request::Commit(r)
                    })
                } else {
                    None
                };
                let next = next
                    .or_else(|| {
                        i.exports.try_recv().ok().map(|r| {
                            i.burst = 0;
                            Request::Export(r)
                        })
                    })
                    .or_else(|| {
                        i.commits.try_recv().ok().map(|r| {
                            i.burst = 1;
                            Request::Commit(r)
                        })
                    });
                if next.is_none() {
                    i.pumping = false;
                }
                next
            };
            let Some(request) = request else {
                return;
            };
            self.busy.store(true, Ordering::Release);
            match request {
                Request::Commit(Commit {
                    next,
                    state,
                    gate,
                    before,
                    reply,
                }) => {
                    let started = std::time::Instant::now();
                    let copy = next.clone();
                    let store = self.store.upgrade();
                    #[cfg(test)]
                    let hook = self.test_hook.lock().unwrap().take();
                    let result=tokio::task::spawn_blocking(move||{
                        #[cfg(test)] if let Some(hook)=hook {
                            hook.entered.notify_one();
                            let (lock,wake)=&*hook.release;
                            let mut released=lock.lock().unwrap();
                            while !*released {released=wake.wait(released).unwrap();}
                            if hook.panic {panic!("private synthetic writer panic");}
                        }
                        let store=store.ok_or("WriterClosed: workspace closed before checkpoint.")?;
                        if let Some(export)=before {export(&store)?;}
                        store.save(&copy)
                    }).await.unwrap_or_else(|_| Err("WriterFailed: checkpoint worker terminated; durability is uncertain. Reopen the workspace.".into()));
                    {
                        let mut s = state.lock().await;
                        match &result {
                            Ok(()) => {
                                s.workspace =
                                    Arc::try_unwrap(next).unwrap_or_else(|w| (*w).clone());
                                self.committed.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(error) => {
                                s.fault = Some(error.clone());
                                self.failures.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                    self.last_commit_ms.store(
                        started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                        Ordering::Relaxed,
                    );
                    self.busy.store(false, Ordering::Release);
                    // Release Store ownership before ACK so an immediate reopen is safe.
                    drop(state);
                    drop(gate);
                    let _ = reply.send(result);
                }
                Request::Export(ExportRequest {
                    operation,
                    store,
                    reply,
                    permit,
                }) => {
                    let result=tokio::task::spawn_blocking(move||operation(&store)).await
                        .unwrap_or_else(|_| Err("WriterFailed: export worker terminated; inspect the exports directory.".into()));
                    if result.is_err() {
                        self.failures.fetch_add(1, Ordering::Relaxed);
                    }
                    self.busy.store(false, Ordering::Release);
                    drop(permit);
                    let _ = reply.send(result);
                }
            }
            tokio::task::yield_now().await;
        }
    }
}
