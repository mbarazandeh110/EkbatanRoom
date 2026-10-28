use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub struct SystemSampler {
    prev_total: AtomicU64,
    prev_idle: AtomicU64,
    last_sample_instant: parking_lot::Mutex<Instant>,
    last_cpu: parking_lot::Mutex<f32>,
    last_mem: parking_lot::Mutex<f32>,
}

impl Default for SystemSampler {
    fn default() -> Self {
        let (total, idle) = Self::read_proc_stat().unwrap_or((0, 0));
        let mem = Self::read_proc_mem().unwrap_or(0.0);
        Self {
            prev_total: AtomicU64::new(total),
            prev_idle: AtomicU64::new(idle),
            last_sample_instant: parking_lot::Mutex::new(Instant::now()),
            last_cpu: parking_lot::Mutex::new(0.0),
            last_mem: parking_lot::Mutex::new(mem),
        }
    }
}

impl SystemSampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Samples CPU and Memory usage. Throttled so that calls within 500ms return cached values.
    pub fn sample(&self) -> (f32, f32) {
        let mut last_instant = self.last_sample_instant.lock();
        if last_instant.elapsed().as_millis() < 500 {
            return (*self.last_cpu.lock(), *self.last_mem.lock());
        }
        *last_instant = Instant::now();

        // Sample CPU
        if let Some((curr_total, curr_idle)) = Self::read_proc_stat() {
            let prev_total = self.prev_total.swap(curr_total, Ordering::Relaxed);
            let prev_idle = self.prev_idle.swap(curr_idle, Ordering::Relaxed);

            let total_diff = curr_total.saturating_sub(prev_total);
            let idle_diff = curr_idle.saturating_sub(prev_idle);

            if total_diff > 0 {
                let usage = ((total_diff.saturating_sub(idle_diff)) as f32 / total_diff as f32
                    * 100.0)
                    .clamp(0.0, 100.0);
                *self.last_cpu.lock() = usage;
            }
        }

        // Sample Memory
        if let Some(mem_pct) = Self::read_proc_mem() {
            *self.last_mem.lock() = mem_pct;
        }

        (*self.last_cpu.lock(), *self.last_mem.lock())
    }

    fn read_proc_stat() -> Option<(u64, u64)> {
        let content = std::fs::read_to_string("/proc/stat").ok()?;
        let first_line = content.lines().next()?;
        if !first_line.starts_with("cpu ") {
            return None;
        }

        let parts: Vec<u64> = first_line
            .split_whitespace()
            .skip(1)
            .filter_map(|s| s.parse::<u64>().ok())
            .collect();

        if parts.len() >= 4 {
            let user = parts[0];
            let nice = parts[1];
            let system = parts[2];
            let idle = parts[3];
            let iowait = parts.get(4).copied().unwrap_or(0);
            let irq = parts.get(5).copied().unwrap_or(0);
            let softirq = parts.get(6).copied().unwrap_or(0);
            let steal = parts.get(7).copied().unwrap_or(0);

            let total = user + nice + system + idle + iowait + irq + softirq + steal;
            let idle_total = idle + iowait;
            Some((total, idle_total))
        } else {
            None
        }
    }

    fn read_proc_mem() -> Option<f32> {
        let content = std::fs::read_to_string("/proc/meminfo").ok()?;
        let mut total_kb = 0u64;
        let mut available_kb = 0u64;

        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                total_kb = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            } else if line.starts_with("MemAvailable:") {
                available_kb = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            }
            if total_kb > 0 && available_kb > 0 {
                break;
            }
        }

        if total_kb > 0 {
            let used = total_kb.saturating_sub(available_kb);
            Some(((used as f32 / total_kb as f32) * 100.0).clamp(0.0, 100.0))
        } else {
            None
        }
    }
}
