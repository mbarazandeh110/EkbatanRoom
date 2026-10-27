use crate::application::state::{EngineState, RouterItem};
use crate::domain::codecs::standard_media_codecs;
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
    cache: Arc<IdempotencyCache<(String, u32, String)>>,
}

impl RouterService {
    pub fn new(
        state: EngineState,
        worker_pool: Arc<dyn WorkerPoolPort>,
        event_publisher: Arc<dyn EventPublisherPort>,
        metrics: Arc<dyn MetricsPort>,
    ) -> Self {
        let cache = Arc::new(IdempotencyCache::default());
        let _cleanup_task = cache.start_periodic_cleanup(Duration::from_secs(30));

        Self {
            state,
            worker_pool,
            event_publisher,
            metrics,
            cache,
        }
    }

    fn cascade_close_transport_internal(&self, router_id: &str, transport_id: &str) {
        if let Some((_, t_item)) = self.state.transports.remove(transport_id) {
            t_item.is_closed.store(true, Ordering::SeqCst);
            self.metrics.dec_active_transports(t_item.is_publisher);

            self.event_publisher.publish_resource_closed(
                router_id,
                "transport",
                transport_id,
                "cascade_closed",
            );

            // Close producers on this transport
            if let Some((_, p_ids)) = self.state.transport_producers.remove(transport_id) {
                for pid in p_ids {
                    if let Some((_, p)) = self.state.producers.remove(&pid) {
                        p.is_closed.store(true, Ordering::SeqCst);
                        self.metrics.dec_active_producers(&p.kind.to_string());

                        self.event_publisher.publish_resource_closed(
                            router_id,
                            "producer",
                            &pid,
                            "cascade_closed",
                        );

                        // Cascade to consumers of this producer
                        if let Some((_, c_ids)) = self.state.producer_consumers.remove(&pid) {
                            for cid in c_ids {
                                if let Some(c) = self.state.cascade_remove_consumer(&cid) {
                                    self.metrics.dec_active_consumers(&c.kind.to_string());
                                    self.event_publisher.publish_resource_closed(
                                        router_id,
                                        "consumer",
                                        &cid,
                                        "cascade_closed",
                                    );
                                }
                            }
                        }
                    }
                }
            }

            // Close consumers directly on this transport
            if let Some((_, c_ids)) = self.state.transport_consumers.remove(transport_id) {
                for cid in c_ids {
                    if let Some(c) = self.state.cascade_remove_consumer(&cid) {
                        self.metrics.dec_active_consumers(&c.kind.to_string());
                        self.event_publisher.publish_resource_closed(
                            router_id,
                            "consumer",
                            &cid,
                            "cascade_closed",
                        );
                    }
                }
            }
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
        validation::validate_not_empty("request_id", request_id)?;
        validation::validate_not_empty("room_id", room_id)?;

        let payload_bytes = format!("{request_id}:{room_id}").into_bytes();
        let metrics_ref = self.metrics.clone();

        self.cache
            .execute_or_get_with_hook(
                request_id,
                &payload_bytes,
                || async {
                    // Check if room already has an active router (Scope guard dropped immediately)
                    let existing_active_router = {
                        if let Some(r_id) = self.state.room_to_router.get(room_id) {
                            self.state.routers.get(r_id.value()).and_then(|r| {
                                if !r.is_closed.load(Ordering::SeqCst) {
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

                    // Obtain least loaded worker and spawn router without holding any DashMap lock
                    let (worker_index, worker) = self.worker_pool.get_least_loaded();
                    let router_codecs = standard_media_codecs();
                    let router = worker
                        .create_router(RouterOptions::new(router_codecs))
                        .await
                        .map_err(|e| {
                            SfuDomainError::Mediasoup(format!("Failed to create router: {e}"))
                        })?;

                    let router_id = router.id().to_string();

                    // Atomic registration in room_to_router to prevent concurrent race conditions
                    use dashmap::mapref::entry::Entry;
                    let winner = match self.state.room_to_router.entry(room_id.to_string()) {
                        Entry::Occupied(mut occ) => {
                            let existing_id = occ.get().clone();
                            let is_active = self
                                .state
                                .routers
                                .get(&existing_id)
                                .map(|r| !r.is_closed.load(Ordering::SeqCst))
                                .unwrap_or(false);

                            if is_active {
                                // Another concurrent request won the race; drop our newly created router
                                let winner_item = self
                                    .state
                                    .routers
                                    .get(&existing_id)
                                    .map(|r| (r.router.clone(), r.worker_index));
                                Some(winner_item)
                            } else {
                                // Previous router was closed; replace entry with our new router
                                occ.insert(router_id.clone());
                                None
                            }
                        }
                        Entry::Vacant(vac) => {
                            vac.insert(router_id.clone());
                            None
                        }
                    };

                    if let Some(Some((w_router, w_worker_index))) = winner {
                        let caps_json = serde_json::to_string(&w_router.rtp_capabilities())
                            .map_err(|e| SfuDomainError::Internal(e.to_string()))?;
                        return Ok((w_router.id().to_string(), w_worker_index as u32, caps_json));
                    }

                    let caps_json = serde_json::to_string(&router.rtp_capabilities())
                        .map_err(|e| SfuDomainError::Internal(e.to_string()))?;

                    let item = RouterItem {
                        router,
                        room_id: room_id.to_string(),
                        worker_index,
                        is_closed: Arc::new(AtomicBool::new(false)),
                    };

                    self.worker_pool.increment_load(worker_index);
                    self.state.routers.insert(router_id.clone(), item);
                    self.state
                        .router_transports
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

        // 1. Emit ResourceClosedEvent for router before removing or closing
        self.event_publisher.publish_resource_closed(
            router_id,
            "router",
            router_id,
            "router_closed",
        );

        // 2. Remove router from active maps
        if let Some((_, item)) = self.state.routers.remove(router_id) {
            item.is_closed.store(true, Ordering::SeqCst);
            self.state.room_to_router.remove(&item.room_id);
            self.worker_pool.decrement_load(item.worker_index);
            self.metrics.dec_active_routers();

            // 3. Cascade close transports
            if let Some((_, transport_ids)) = self.state.router_transports.remove(router_id) {
                for tid in transport_ids {
                    self.cascade_close_transport_internal(router_id, &tid);
                }
            }
        }

        Ok(())
    }
}
