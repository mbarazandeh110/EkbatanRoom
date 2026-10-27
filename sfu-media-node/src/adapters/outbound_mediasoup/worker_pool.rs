use crate::domain::errors::SfuDomainError;
use crate::ports::outbound::WorkerPoolPort;
use mediasoup::prelude::*;
use mediasoup::worker::WorkerLogLevel;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

pub struct WorkerPool {
    workers: Vec<Worker>,
    load_counts: Vec<Arc<AtomicUsize>>,
    dead_flags: Vec<Arc<AtomicBool>>,
}

impl WorkerPool {
    pub async fn new(
        worker_manager: &WorkerManager,
        count: usize,
        min_port: u16,
        max_port: u16,
    ) -> Result<Self, SfuDomainError> {
        let count = count.max(1);
        let mut workers = Vec::with_capacity(count);
        let mut load_counts = Vec::with_capacity(count);
        let mut dead_flags = Vec::with_capacity(count);

        for i in 0..count {
            let mut settings = WorkerSettings::default();
            settings.rtc_port_range = min_port..=max_port;
            settings.log_level = WorkerLogLevel::Warn;

            let worker = worker_manager
                .create_worker(settings)
                .await
                .map_err(|e| SfuDomainError::Mediasoup(format!("Failed to spawn worker: {e}")))?;

            let is_dead = Arc::new(AtomicBool::new(false));
            let is_dead_clone = is_dead.clone();

            worker
                .on_dead(move |_| {
                    tracing::error!(worker_index = i, "Mediasoup worker died unexpectedly!");
                    is_dead_clone.store(true, Ordering::SeqCst);
                })
                .detach();

            workers.push(worker);
            load_counts.push(Arc::new(AtomicUsize::new(0)));
            dead_flags.push(is_dead);
        }

        Ok(Self {
            workers,
            load_counts,
            dead_flags,
        })
    }

    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }
}

impl WorkerPoolPort for WorkerPool {
    fn get_least_loaded(&self) -> (usize, Worker) {
        let mut min_index = 0;
        let mut min_val = usize::MAX;

        // First pass: find least loaded alive worker
        for (i, count) in self.load_counts.iter().enumerate() {
            if self.dead_flags[i].load(Ordering::SeqCst) {
                continue;
            }
            let val = count.load(Ordering::Relaxed);
            if val < min_val {
                min_val = val;
                min_index = i;
            }
        }

        // If all workers are flagged dead, fallback to index 0
        if min_val == usize::MAX && !self.workers.is_empty() {
            tracing::warn!("All workers flagged dead! Falling back to worker 0");
            min_index = 0;
        }

        (min_index, self.workers[min_index].clone())
    }

    fn increment_load(&self, index: usize) {
        if let Some(counter) = self.load_counts.get(index) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn decrement_load(&self, index: usize) {
        if let Some(counter) = self.load_counts.get(index) {
            let mut current = counter.load(Ordering::Relaxed);
            while current > 0 {
                match counter.compare_exchange_weak(
                    current,
                    current - 1,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(actual) => current = actual,
                }
            }
        }
    }

    fn mark_dead(&self, index: usize) {
        if let Some(flag) = self.dead_flags.get(index) {
            flag.store(true, Ordering::SeqCst);
        }
    }
}
