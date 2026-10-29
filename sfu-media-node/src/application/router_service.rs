use crate::application::state::{EngineState, RouterItem};
use crate::domain::errors::SfuDomainError;
use crate::domain::idempotency::IdempotencyCache;
use crate::domain::validation;
use crate::ports::inbound::RouterUseCase;
use crate::ports::outbound::{EventPublisherPort, MetricsPort, WorkerPoolPort};
use async_trait::async_trait;
use dashmap::DashSet;
use mediasoup::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct RouterService {
    state: EngineState,
    worker_pool: Arc<dyn WorkerPoolPort>,
    event_publisher: Arc<dyn EventPublisherPort>,
    metrics: Arc<dyn MetricsPort>,
    config: crate::Config,
    cache: Arc<IdempotencyCache<(String, u32, String)>>,
}

impl RouterService {
    pub fn new(
        state: EngineState,
        worker_pool: Arc<dyn WorkerPoolPort>,
        event_publisher: Arc<dyn EventPublisherPort>,
        metrics: Arc<dyn MetricsPort>,
        config: crate::Config,
    ) -> Self {
        let cache = Arc::new(IdempotencyCache::default());
        let _cleanup_task = cache.start_periodic_cleanup(Duration::from_secs(30));

        Self {
            state,
            worker_pool,
            event_publisher,
            metrics,
            config,
            cache,
        }
    }

    fn cascade_close_transport_internal(&self, router_id: &str, transport_id: &str) {
        if let Some((_, t_item)) = self.state.transports.remove(transport_id) {
            t_item.is_closed.store(true, Ordering::SeqCst);
            self.worker_pool.sub_resource_score(t_item.worker_index, 1);
            self.metrics.dec_active_transports(t_item.is_publisher);

            self.event_publisher.publish_resource_closed(
                router_id,
                "transport",
                transport_id,
                "cascade_closed",
            );

            crate::application::transport_service::cascade_close_producers_and_consumers(
                &self.state,
                self.worker_pool.as_ref(),
                self.metrics.as_ref(),
                self.event_publisher.as_ref(),
                router_id,
                transport_id,
            );
        }
    }
}

#[async_trait]
impl RouterUseCase for RouterService {
    #[tracing::instrument(skip(self), fields(room_id = %room_id, request_id = %request_id))]
    async fn create_router(
        &self,
        request_id: &str,
        room_id: &str,
    ) -> Result<(String, u32, String), SfuDomainError> {
        if self.state.is_draining.load(Ordering::SeqCst) {
            return Err(SfuDomainError::SystemDraining(
                "Node is draining; rejecting new router".to_string(),
            ));
        }

        validation::validate_not_empty("request_id", request_id)?;
        validation::validate_not_empty("room_id", room_id)?;

        let payload_bytes = format!("{request_id}:{room_id}").into_bytes();
        let metrics_ref = self.metrics.clone();

        self.cache
            .execute_or_get_with_hook(
                request_id,
                &payload_bytes,
                || async {
                    // Acquire singleflight lock for this room_id to serialize concurrent creation
                    let lock = self
                        .state
                        .room_singleflight
                        .entry(room_id.to_string())
                        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                        .clone();

                    let _guard = lock.lock().await;
                    let _key_guard = crate::application::state::SingleflightKeyGuard::new(
                        self.state.room_singleflight.clone(),
                        room_id.to_string(),
                    );

                    // Check if room already has an active router (double-checked under room lock)
                    let existing_active_router = {
                        if let Some(r_id) = self.state.room_to_router.get(room_id) {
                            self.state.routers.get(r_id.value()).and_then(|r| {
                                if !r.is_closed.load(Ordering::SeqCst)
                                    && self.worker_pool.is_worker_alive(r.worker_index)
                                {
                                    Some((r.router.clone(), r.worker_index))
                                } else {
                                    None
                                }
                            })
                        } else {
                            None
                        }
                    };

                    if let Some((router, worker_index)) = existing_active_router {
                        let caps_json = serde_json::to_string(&router.rtp_capabilities())
                            .map_err(|e| SfuDomainError::Internal(e.to_string()))?;
                        return Ok((router.id().to_string(), worker_index as u32, caps_json));
                    }

                    // Obtain least loaded worker and spawn router
                    let (worker_index, worker) = self.worker_pool.get_least_loaded();
                    let router_codecs = crate::domain::codecs::router_codecs_from_config(
                        &self.config.router_codecs,
                    );
                    let router = worker
                        .create_router(RouterOptions::new(router_codecs))
                        .await
                        .map_err(|e| {
                            SfuDomainError::Mediasoup(format!("Failed to create router: {e}"))
                        })?;

                    let router_id = router.id().to_string();
                    let caps_json = serde_json::to_string(&router.rtp_capabilities())
                        .map_err(|e| SfuDomainError::Internal(e.to_string()))?;

                    let mut observer_options =
                        mediasoup::audio_level_observer::AudioLevelObserverOptions::default();
                    observer_options.interval = self.config.audio_level_observer.interval_ms;
                    observer_options.threshold = self.config.audio_level_observer.threshold_dbov;
                    let audio_level_observer = router
                        .create_audio_level_observer(observer_options)
                        .await
                        .map_err(|e| {
                            SfuDomainError::Mediasoup(format!(
                                "Failed to create AudioLevelObserver: {e}"
                            ))
                        })?;

                    let is_closed = Arc::new(AtomicBool::new(false));

                    let event_publisher = self.event_publisher.clone();
                    let r_id = router_id.clone();
                    let state_ref = self.state.clone();
                    let closed_ref = is_closed.clone();

                    audio_level_observer
                        .on_volumes(move |volumes| {
                            if closed_ref.load(Ordering::SeqCst)
                                || !state_ref.is_router_active(&r_id)
                            {
                                return;
                            }

                            for v in volumes {
                                let p_id = v.producer.id().to_string();
                                let peer_id = state_ref
                                    .producers
                                    .get(&p_id)
                                    .map(|p| p.peer_id.clone())
                                    .unwrap_or_default();

                                event_publisher.publish_active_speaker(
                                    &r_id,
                                    &p_id,
                                    &peer_id,
                                    v.volume as f32,
                                );
                            }
                        })
                        .detach();

                    let item = RouterItem {
                        router,
                        room_id: room_id.to_string(),
                        worker_index,
                        audio_level_observer,
                        is_closed,
                        sub_routers: Arc::new(dashmap::DashMap::new()),
                    };

                    self.worker_pool.increment_load(worker_index);
                    self.state.routers.insert(router_id.clone(), item);
                    self.state
                        .room_to_router
                        .insert(room_id.to_string(), router_id.clone());
                    self.state
                        .router_transports
                        .insert(router_id.clone(), DashSet::new());
                    self.state
                        .router_pipe_transports
                        .insert(router_id.clone(), DashSet::new());
                    self.metrics.inc_active_routers();

                    Ok((router_id, worker_index as u32, caps_json))
                },
                move || {
                    metrics_ref.record_idempotency_hit();
                },
            )
            .await
    }

    #[tracing::instrument(skip(self), fields(router_id = %router_id))]
    fn close_router(&self, router_id: &str) -> Result<(), SfuDomainError> {
        validation::validate_not_empty("router_id", router_id)?;

        // Remove router from active maps; emit ResourceClosedEvent ONLY if router actually existed
        if let Some((_, item)) = self.state.routers.remove(router_id) {
            item.is_closed.store(true, Ordering::SeqCst);
            self.state.room_to_router.remove(&item.room_id);
            self.state.room_singleflight.remove(&item.room_id);
            self.worker_pool.decrement_load(item.worker_index);

            // Clean up all sub-routers for this room
            for sub_entry in item.sub_routers.iter() {
                sub_entry.value().is_closed.store(true, Ordering::SeqCst);
                self.worker_pool
                    .decrement_load(sub_entry.value().worker_index);
            }
            item.sub_routers.clear();

            // Clean up peer affinity and singleflights for this router
            self.state
                .peer_worker_affinity
                .retain(|k, _| k.0 != router_id);
            self.state
                .sub_router_singleflight
                .retain(|k, _| k.0 != router_id);

            self.metrics.dec_active_routers();

            self.event_publisher.publish_resource_closed(
                router_id,
                "router",
                router_id,
                "router_closed",
            );

            // Cascade close transports
            if let Some((_, transport_ids)) = self.state.router_transports.remove(router_id) {
                for tid in transport_ids {
                    self.cascade_close_transport_internal(router_id, &tid);
                }
            }

            // Cascade close pipe transports
            if let Some((_, pipe_ids)) = self.state.router_pipe_transports.remove(router_id) {
                for pid in pipe_ids {
                    if let Some((_, pt)) = self.state.pipe_transports.remove(&pid) {
                        pt.is_closed.store(true, Ordering::SeqCst);
                        self.worker_pool.sub_resource_score(pt.worker_index, 1);
                        self.metrics.dec_active_transports(false);
                    }
                    self.event_publisher.publish_resource_closed(
                        router_id,
                        "transport",
                        &pid,
                        "cascade_closed",
                    );
                    crate::application::transport_service::cascade_close_producers_and_consumers(
                        &self.state,
                        self.worker_pool.as_ref(),
                        self.metrics.as_ref(),
                        self.event_publisher.as_ref(),
                        router_id,
                        &pid,
                    );
                }
            }
        }

        Ok(())
    }
}
