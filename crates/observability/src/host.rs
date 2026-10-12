use domain::prototype::HostMetrics;
use sysinfo::System;
pub struct Monitor(System);
impl Default for Monitor {
    fn default() -> Self {
        Self(System::new_all())
    }
}
impl Monitor {
    pub fn sample(&mut self) -> HostMetrics {
        self.0.refresh_cpu_usage();
        self.0.refresh_memory();
        HostMetrics {
            cpu_percent: self.0.global_cpu_usage(),
            used_memory_mb: self.0.used_memory() / 1024 / 1024,
            total_memory_mb: self.0.total_memory() / 1024 / 1024,
            available_memory_mb: self.0.available_memory() / 1024 / 1024,
            gpu: None,
        }
    }
}
