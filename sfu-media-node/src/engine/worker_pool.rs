use crate::domain::error::SfuError;
use mediasoup::prelude::*;
use mediasoup::worker::WorkerLogLevel;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub struct WorkerPool {
    workers: Vec<Worker>,
    load_counts: Vec<Arc<AtomicUsize>>,
}

impl WorkerPool {
    pub async fn new(
        worker_manager: &WorkerManager,
        count: usize,
        min_port: u16,
        max_port: u16,
    ) -> Result<Self, SfuError> {
        let count = count.max(1);
        let mut workers = Vec::with_capacity(count);
        let mut load_counts = Vec::with_capacity(count);

        for _ in 0..count {
            let mut settings = WorkerSettings::default();
            settings.rtc_port_range = min_port..=max_port;
            settings.log_level = WorkerLogLevel::Warn;

            let worker = worker_manager
                .create_worker(settings)
                .await
                .map_err(|e| SfuError::Mediasoup(format!("Failed to spawn worker: {e}")))?;

            workers.push(worker);
            load_counts.push(Arc::new(AtomicUsize::new(0)));
        }

        Ok(Self {
            workers,
            load_counts,
        })
    }

    pub fn get_least_loaded(&self) -> (usize, Worker) {
        let mut min_index = 0;
        let mut min_val = usize::MAX;

        for (i, count) in self.load_counts.iter().enumerate() {
            let val = count.load(Ordering::Relaxed);
            if val < min_val {
                min_val = val;
                min_index = i;
            }
        }

        (min_index, self.workers[min_index].clone())
    }

    pub fn increment_load(&self, index: usize) {
        if let Some(counter) = self.load_counts.get(index) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn decrement_load(&self, index: usize) {
        if let Some(counter) = self.load_counts.get(index) {
            counter.fetch_sub(1, Ordering::Relaxed);
        }
    }

    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }
}
