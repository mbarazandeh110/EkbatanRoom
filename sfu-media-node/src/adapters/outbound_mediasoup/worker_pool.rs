use crate::domain::errors::SfuDomainError;
use crate::ports::outbound::{WorkerDeadCallback, WorkerPoolPort};
use mediasoup::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

type OnDeadCallbacks = Arc<parking_lot::RwLock<Vec<WorkerDeadCallback>>>;

pub struct WorkerPool {
    workers: Vec<Worker>,
    load_counts: Vec<Arc<AtomicUsize>>,
    resource_scores: Vec<Arc<AtomicUsize>>,
    dead_flags: Vec<Arc<AtomicBool>>,
    on_dead_callbacks: OnDeadCallbacks,
}

impl WorkerPool {
    pub async fn new(
        worker_manager: &WorkerManager,
        count: usize,
        min_port: u16,
        max_port: u16,
        worker_config: &crate::config::MediasoupWorkerConfig,
    ) -> Result<Self, SfuDomainError> {
        let count = count.max(1);
        let mut workers = Vec::with_capacity(count);
        let mut load_counts = Vec::with_capacity(count);
        let mut resource_scores = Vec::with_capacity(count);
        let mut dead_flags = Vec::with_capacity(count);
        let on_dead_callbacks: OnDeadCallbacks = Arc::new(parking_lot::RwLock::new(Vec::new()));

        for i in 0..count {
            let mut settings = WorkerSettings::default();
            settings.rtc_port_range = min_port..=max_port;
            settings.log_level = worker_config.to_mediasoup_log_level();
            settings.log_tags = worker_config.to_mediasoup_log_tags();
            if let Some(files) = worker_config.to_mediasoup_dtls_files() {
                settings.dtls_files = Some(files);
            }

            let worker = worker_manager
                .create_worker(settings)
                .await
                .map_err(|e| SfuDomainError::Mediasoup(format!("Failed to spawn worker: {e}")))?;

            let is_dead = Arc::new(AtomicBool::new(false));
            let is_dead_clone = is_dead.clone();
            let load_counter = Arc::new(AtomicUsize::new(0));
            let score_counter = Arc::new(AtomicUsize::new(0));
            let load_clone = load_counter.clone();
            let score_clone = score_counter.clone();
            let callbacks_clone = on_dead_callbacks.clone();

            worker
                .on_dead(move |_| {
                    tracing::error!(worker_index = i, "Mediasoup worker died unexpectedly!");
                    let was_alive = !is_dead_clone.swap(true, Ordering::SeqCst);
                    load_clone.store(0, Ordering::SeqCst);
                    score_clone.store(0, Ordering::SeqCst);
                    if was_alive {
                        let cbs = callbacks_clone.read().clone();
                        for cb in cbs {
                            cb(i);
                        }
                    }
                })
                .detach();

            workers.push(worker);
            load_counts.push(load_counter);
            resource_scores.push(score_counter);
            dead_flags.push(is_dead);
        }

        Ok(Self {
            workers,
            load_counts,
            resource_scores,
            dead_flags,
            on_dead_callbacks,
        })
    }

    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }
}

impl WorkerPoolPort for WorkerPool {
    fn get_least_loaded(&self) -> (usize, Worker) {
        let mut min_index = 0;
        let mut min_tuple = (usize::MAX, usize::MAX);

        // Find least loaded alive worker based on (resource_score, router_load_count)
        for (i, score) in self.resource_scores.iter().enumerate() {
            if self.dead_flags[i].load(Ordering::SeqCst) {
                continue;
            }
            let s_val = score.load(Ordering::Relaxed);
            let l_val = self.load_counts[i].load(Ordering::Relaxed);
            let candidate = (s_val, l_val);
            if candidate < min_tuple {
                min_tuple = candidate;
                min_index = i;
            }
        }

        // If all workers are flagged dead, fallback to index 0
        if min_tuple == (usize::MAX, usize::MAX) && !self.workers.is_empty() {
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
        let was_alive = if let Some(flag) = self.dead_flags.get(index) {
            !flag.swap(true, Ordering::SeqCst)
        } else {
            false
        };
        if let Some(counter) = self.load_counts.get(index) {
            counter.store(0, Ordering::SeqCst);
        }
        if let Some(counter) = self.resource_scores.get(index) {
            counter.store(0, Ordering::SeqCst);
        }
        if was_alive {
            let cbs = self.on_dead_callbacks.read().clone();
            for cb in cbs {
                cb(index);
            }
        }
    }

    fn total_workers(&self) -> usize {
        self.workers.len()
    }

    fn alive_workers(&self) -> usize {
        self.dead_flags
            .iter()
            .filter(|f| !f.load(Ordering::SeqCst))
            .count()
    }

    fn is_worker_alive(&self, index: usize) -> bool {
        self.dead_flags
            .get(index)
            .map(|f| !f.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    fn add_resource_score(&self, index: usize, weight: usize) {
        if let Some(counter) = self.resource_scores.get(index) {
            counter.fetch_add(weight, Ordering::Relaxed);
        }
    }

    fn sub_resource_score(&self, index: usize, weight: usize) {
        if let Some(counter) = self.resource_scores.get(index) {
            let mut current = counter.load(Ordering::Relaxed);
            while current > 0 {
                let new_val = current.saturating_sub(weight);
                match counter.compare_exchange_weak(
                    current,
                    new_val,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(actual) => current = actual,
                }
            }
        }
    }

    fn worker_resource_score(&self, index: usize) -> usize {
        if !self.is_worker_alive(index) {
            0
        } else {
            self.resource_scores
                .get(index)
                .map(|c| c.load(Ordering::Relaxed))
                .unwrap_or(0)
        }
    }

    fn total_resource_score(&self) -> usize {
        self.resource_scores
            .iter()
            .enumerate()
            .filter(|(i, _)| self.is_worker_alive(*i))
            .map(|(_, c)| c.load(Ordering::Relaxed))
            .sum()
    }

    fn get_worker(&self, index: usize) -> Option<Worker> {
        self.workers.get(index).cloned()
    }

    fn add_on_dead_callback(&self, callback: WorkerDeadCallback) {
        self.on_dead_callbacks.write().push(callback);
    }
}
