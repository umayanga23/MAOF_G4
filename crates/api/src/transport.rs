//! Bounded mutation ingress. Accepted commands outlive a cancelled caller, but
//! acknowledgements are sent only after the checkpoint transaction completes.
use super::*;
use std::{
    io,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    sync::{Mutex as StdMutex, Weak},
};
use tokio::sync::{mpsc, oneshot, OwnedSemaphorePermit, Semaphore};

pub const NORMAL_SLOTS: usize = 32;
pub const CONTROL_SLOTS: usize = 8;
pub const NORMAL_BYTES: usize = 2 * 1024 * 1024;
pub const CONTROL_BYTES: usize = 64 * 1024;
pub const MAX_COMMAND_BYTES: usize = 256 * 1024;
pub const MAX_CONTROL_BYTES: usize = 8 * 1024;
pub const REPORT_SLOTS: usize = 8;

pub(super) struct Envelope {
    command: Command,
    reply: oneshot::Sender<Result<(), String>>,
    _bytes: OwnedSemaphorePermit,
}
struct Inbox {
    normal: mpsc::Receiver<Envelope>,
    control: mpsc::Receiver<Envelope>,
    pumping: bool,
    control_burst: usize,
}
pub(super) struct Transport {
    normal: mpsc::Sender<Envelope>,
    control: mpsc::Sender<Envelope>,
    normal_bytes: Arc<Semaphore>,
    control_bytes: Arc<Semaphore>,
    inbox: StdMutex<Inbox>,
    owner: Weak<Service>,
    pub driver_started: AtomicBool,
    pub driver_running: AtomicBool,
    pub active_workers: AtomicU64,
    pub reports_pending: AtomicU64,
    pub rejected_commands: AtomicU64,
    pub worker_failures: AtomicU64,
    pub transport_waits: AtomicU64,
    #[cfg(test)]
    pub faults: StdMutex<std::collections::HashMap<String, super::supervision::TestFault>>,
    #[cfg(test)]
    pub deadline_ms: AtomicU64,
    #[cfg(test)]
    pub applied_lanes: StdMutex<Vec<bool>>,
}
#[derive(Debug, Serialize)]
pub struct RuntimeHealth {
    pub command_capacity: usize,
    pub control_capacity: usize,
    pub commands_pending: usize,
    pub controls_pending: usize,
    pub command_bytes_in_flight: usize,
    pub command_byte_capacity: usize,
    pub result_capacity: usize,
    pub results_pending: u64,
    pub active_workers: u64,
    pub driver_running: bool,
    pub rejected_commands: u64,
    pub worker_failures: u64,
    pub transport_waits: u64,
}
impl Command {
    fn control_lane(&self) -> bool {
        matches!(
            self,
            Self::Control { .. }
                | Self::Approve { .. }
                | Self::ApproveDocument { .. }
                | Self::RevokeDocument { .. }
        )
    }
}
impl Transport {
    pub fn new(owner: Weak<Service>) -> Self {
        let (normal, normal_rx) = mpsc::channel(NORMAL_SLOTS);
        let (control, control_rx) = mpsc::channel(CONTROL_SLOTS);
        Self {
            normal,
            control,
            normal_bytes: Arc::new(Semaphore::new(NORMAL_BYTES)),
            control_bytes: Arc::new(Semaphore::new(CONTROL_BYTES)),
            inbox: StdMutex::new(Inbox {
                normal: normal_rx,
                control: control_rx,
                pumping: false,
                control_burst: 0,
            }),
            owner,
            driver_started: AtomicBool::new(false),
            driver_running: AtomicBool::new(false),
            active_workers: AtomicU64::new(0),
            reports_pending: AtomicU64::new(0),
            rejected_commands: AtomicU64::new(0),
            worker_failures: AtomicU64::new(0),
            transport_waits: AtomicU64::new(0),
            #[cfg(test)]
            faults: StdMutex::new(std::collections::HashMap::new()),
            #[cfg(test)]
            deadline_ms: AtomicU64::new(0),
            #[cfg(test)]
            applied_lanes: StdMutex::new(Vec::new()),
        }
    }
    pub fn health(&self) -> RuntimeHealth {
        RuntimeHealth {
            command_capacity: NORMAL_SLOTS,
            control_capacity: CONTROL_SLOTS,
            commands_pending: NORMAL_SLOTS - self.normal.capacity(),
            controls_pending: CONTROL_SLOTS - self.control.capacity(),
            command_bytes_in_flight: NORMAL_BYTES - self.normal_bytes.available_permits()
                + CONTROL_BYTES
                - self.control_bytes.available_permits(),
            command_byte_capacity: NORMAL_BYTES + CONTROL_BYTES,
            result_capacity: REPORT_SLOTS,
            results_pending: self.reports_pending.load(Ordering::Relaxed),
            active_workers: self.active_workers.load(Ordering::Relaxed),
            driver_running: self.driver_running.load(Ordering::Relaxed),
            rejected_commands: self.rejected_commands.load(Ordering::Relaxed),
            worker_failures: self.worker_failures.load(Ordering::Relaxed),
            transport_waits: self.transport_waits.load(Ordering::Relaxed),
        }
    }
    pub fn enqueue(
        self: &Arc<Self>,
        command: Command,
    ) -> Result<oneshot::Receiver<Result<(), String>>, String> {
        let urgent = command.control_lane();
        let limit = if urgent {
            MAX_CONTROL_BYTES
        } else {
            MAX_COMMAND_BYTES
        };
        let reject = |message: &str| {
            self.rejected_commands.fetch_add(1, Ordering::Relaxed);
            message.to_string()
        };
        tokio::runtime::Handle::try_current()
            .map_err(|_| reject("Command service needs an active Tokio runtime."))?;
        let bytes=encoded_size(&command,limit).map_err(|_|reject("CommandTooLarge: request exceeds the supported command payload limit; no command accepted."))?;
        let budget = if urgent {
            &self.control_bytes
        } else {
            &self.normal_bytes
        };
        let permit=budget.clone().try_acquire_many_owned(bytes as u32).map_err(|_|reject("CommandQueueFull: payload budget is full. No command accepted; inspect the workspace before retrying."))?;
        let (reply, receipt) = oneshot::channel();
        // Serialize try_send and pump shutdown to avoid stranding the last enqueue.
        let mut inbox = self.inbox.lock().unwrap();
        let sender = if urgent { &self.control } else { &self.normal };
        sender.try_send(Envelope {command,reply,_bytes:permit}).map_err(|_|reject("CommandQueueFull: command lane is full. No command accepted; wait for queued work to finish."))?;
        if !inbox.pumping {
            inbox.pumping = true;
            tokio::spawn(self.clone().pump());
        }
        Ok(receipt)
    }
    async fn pump(self: Arc<Self>) {
        loop {
            let envelope = {
                let mut inbox = self.inbox.lock().unwrap();
                // At most four controls overtake an already queued normal command.
                let next = if inbox.control_burst < 4 {
                    inbox.control.try_recv().ok().map(|e| {
                        inbox.control_burst += 1;
                        e
                    })
                } else {
                    None
                };
                let next = next
                    .or_else(|| {
                        inbox.normal.try_recv().ok().map(|e| {
                            inbox.control_burst = 0;
                            e
                        })
                    })
                    .or_else(|| {
                        inbox.control.try_recv().ok().map(|e| {
                            inbox.control_burst = 1;
                            e
                        })
                    });
                if next.is_none() {
                    inbox.pumping = false;
                }
                next
            };
            let Some(envelope) = envelope else {
                return;
            };
            #[cfg(test)]
            let urgent = envelope.command.control_lane();
            let result = if let Some(service) = self.owner.upgrade() {
                service.apply_command(envelope.command).await
            } else {
                Err("Command service closed before durable acceptance.".into())
            };
            #[cfg(test)]
            self.applied_lanes.lock().unwrap().push(urgent);
            // Do not keep a strong Service/store lock alive after acknowledging.
            drop(envelope._bytes);
            let _ = envelope.reply.send(result);
            tokio::task::yield_now().await;
        }
    }
}
fn encoded_size(value: &Command, limit: usize) -> Result<usize, serde_json::Error> {
    struct Count {
        size: usize,
        limit: usize,
    }
    impl io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.size.saturating_add(bytes.len()) > self.limit {
                return Err(io::Error::other("command payload limit"));
            }
            self.size += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count { size: 0, limit };
    serde_json::to_writer(&mut count, value)?;
    Ok(count.size.max(1))
}
