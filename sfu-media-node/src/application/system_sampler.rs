use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

pub struct SystemSampler {
    prev_host_total: AtomicU64,
    prev_host_idle: AtomicU64,
    prev_cgroup_usage_usec: AtomicU64,
    prev_cgroup_time_usec: AtomicU64,
    last_sample_instant: parking_lot::Mutex<Instant>,
    last_cpu: parking_lot::Mutex<f32>,
    last_mem: parking_lot::Mutex<f32>,
}

impl Default for SystemSampler {
    fn default() -> Self {
        let (total, idle) = Self::read_proc_stat().unwrap_or((0, 0));
        let (cg_usage, _) = Self::read_cgroup_cpu().unwrap_or((0, None));
        let now_usec = Self::monotonic_usec();
        let mem = Self::read_memory_percent().unwrap_or(0.0);

        Self {
            prev_host_total: AtomicU64::new(total),
            prev_host_idle: AtomicU64::new(idle),
            prev_cgroup_usage_usec: AtomicU64::new(cg_usage),
            prev_cgroup_time_usec: AtomicU64::new(now_usec),
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

    fn monotonic_usec() -> u64 {
        static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        START.get_or_init(Instant::now).elapsed().as_micros() as u64
    }

    /// Samples CPU and Memory usage (Cgroup-aware for K8s, with /proc fallback for Docker Compose/Host).
    pub fn sample(&self) -> (f32, f32) {
        let mut last_instant = self.last_sample_instant.lock();
        if last_instant.elapsed().as_millis() < 500 {
            return (*self.last_cpu.lock(), *self.last_mem.lock());
        }
        *last_instant = Instant::now();

        // 1. Sample CPU: Prefer Cgroup quota if restricted (K8s), otherwise Host /proc/stat
        let mut cpu_updated = false;
        if let Some((curr_cg_usage, Some(quota_cores))) = Self::read_cgroup_cpu() {
            let now_usec = Self::monotonic_usec();
            let prev_usage = self
                .prev_cgroup_usage_usec
                .swap(curr_cg_usage, Ordering::Relaxed);
            let prev_time = self.prev_cgroup_time_usec.swap(now_usec, Ordering::Relaxed);

            let usage_delta = curr_cg_usage.saturating_sub(prev_usage);
            let time_delta = now_usec.saturating_sub(prev_time);

            if time_delta > 0 && quota_cores > 0.0 {
                let capacity_usec = (time_delta as f32) * quota_cores;
                let usage_pct = ((usage_delta as f32 / capacity_usec) * 100.0).clamp(0.0, 100.0);
                *self.last_cpu.lock() = usage_pct;
                cpu_updated = true;
            }
        }

        if !cpu_updated {
            if let Some((curr_total, curr_idle)) = Self::read_proc_stat() {
                let prev_total = self.prev_host_total.swap(curr_total, Ordering::Relaxed);
                let prev_idle = self.prev_host_idle.swap(curr_idle, Ordering::Relaxed);

                let total_diff = curr_total.saturating_sub(prev_total);
                let idle_diff = curr_idle.saturating_sub(prev_idle);

                if total_diff > 0 {
                    let usage = ((total_diff.saturating_sub(idle_diff)) as f32 / total_diff as f32
                        * 100.0)
                        .clamp(0.0, 100.0);
                    *self.last_cpu.lock() = usage;
                }
            }
        }

        // 2. Sample Memory: Prefer Cgroup limit if set (K8s), otherwise Host /proc/meminfo
        if let Some(mem_pct) = Self::read_memory_percent() {
            *self.last_mem.lock() = mem_pct;
        }

        (*self.last_cpu.lock(), *self.last_mem.lock())
    }

    /// Reads Cgroup v2/v1 CPU usage (in microseconds) and effective CPU quota cores (if limited)
    fn read_cgroup_cpu() -> Option<(u64, Option<f32>)> {
        // Try Cgroup v2: /sys/fs/cgroup/cpu.stat & /sys/fs/cgroup/cpu.max
        if let Ok(stat_content) = std::fs::read_to_string("/sys/fs/cgroup/cpu.stat") {
            let mut usage_usec = None;
            for line in stat_content.lines() {
                if let Some(val) = line.strip_prefix("usage_usec ") {
                    usage_usec = val.trim().parse::<u64>().ok();
                    break;
                }
            }
            if let Some(usage) = usage_usec {
                let quota_cores = std::fs::read_to_string("/sys/fs/cgroup/cpu.max")
                    .ok()
                    .and_then(|max_str| {
                        let mut parts = max_str.split_whitespace();
                        let quota_str = parts.next()?;
                        let period_str = parts.next()?;
                        if quota_str == "max" {
                            None
                        } else {
                            let quota = quota_str.parse::<f32>().ok()?;
                            let period = period_str.parse::<f32>().ok()?;
                            if period > 0.0 {
                                Some(quota / period)
                            } else {
                                None
                            }
                        }
                    });
                return Some((usage, quota_cores));
            }
        }

        // Try Cgroup v1: /sys/fs/cgroup/cpuacct/cpuacct.usage (nanoseconds)
        if let Ok(usage_ns_str) = std::fs::read_to_string("/sys/fs/cgroup/cpuacct/cpuacct.usage") {
            if let Ok(usage_ns) = usage_ns_str.trim().parse::<u64>() {
                let quota_cores = std::fs::read_to_string("/sys/fs/cgroup/cpu/cpu.cfs_quota_us")
                    .ok()
                    .and_then(|q_str| {
                        let quota = q_str.trim().parse::<i64>().ok()?;
                        if quota <= 0 {
                            return None;
                        }
                        let period =
                            std::fs::read_to_string("/sys/fs/cgroup/cpu/cpu.cfs_period_us")
                                .ok()?
                                .trim()
                                .parse::<f32>()
                                .ok()?;
                        if period > 0.0 {
                            Some(quota as f32 / period)
                        } else {
                            None
                        }
                    });
                return Some((usage_ns / 1_000, quota_cores));
            }
        }

        None
    }

    /// Reads memory usage percentage from Cgroup v2/v1 if a container limit is active,
    /// otherwise falls back to /proc/meminfo.
    fn read_memory_percent() -> Option<f32> {
        // 1. Try Cgroup v2 (/sys/fs/cgroup/memory.current & memory.max)
        if let (Ok(curr_str), Ok(max_str)) = (
            std::fs::read_to_string("/sys/fs/cgroup/memory.current"),
            std::fs::read_to_string("/sys/fs/cgroup/memory.max"),
        ) {
            let max_trimmed = max_str.trim();
            if max_trimmed != "max" {
                if let (Ok(curr), Ok(max)) =
                    (curr_str.trim().parse::<u64>(), max_trimmed.parse::<u64>())
                {
                    // Ignore unrealistic limits (> 1 PB indicates unlimited)
                    if max > 0 && max < (1u64 << 50) {
                        return Some(((curr as f64 / max as f64) * 100.0).clamp(0.0, 100.0) as f32);
                    }
                }
            }
        }

        // 2. Try Cgroup v1 (/sys/fs/cgroup/memory/memory.usage_in_bytes & memory.limit_in_bytes)
        if let (Ok(curr_str), Ok(max_str)) = (
            std::fs::read_to_string("/sys/fs/cgroup/memory/memory.usage_in_bytes"),
            std::fs::read_to_string("/sys/fs/cgroup/memory/memory.limit_in_bytes"),
        ) {
            if let (Ok(curr), Ok(max)) = (
                curr_str.trim().parse::<u64>(),
                max_str.trim().parse::<u64>(),
            ) {
                if max > 0 && max < (1u64 << 50) {
                    return Some(((curr as f64 / max as f64) * 100.0).clamp(0.0, 100.0) as f32);
                }
            }
        }

        // 3. Fallback to Host /proc/meminfo (Unrestricted Docker Compose / Bare-Metal)
        Self::read_proc_mem()
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
