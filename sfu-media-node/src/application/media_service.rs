use crate::application::state::{ConsumerItem, EngineState, PipedProducerEntry, ProducerItem};
use crate::domain::entities::MediaKind;
use crate::domain::entities::*;
use crate::domain::errors::SfuDomainError;
use crate::domain::idempotency::IdempotencyCache;
use crate::domain::validation;
use crate::ports::inbound::{
    ConsumeParams, ConsumeResult, MediaUseCase, ProduceParams, ProduceResult,
};
use crate::ports::outbound::{EventPublisherPort, MetricsPort, WorkerPoolPort};
use async_trait::async_trait;
use dashmap::DashSet;
use mediasoup::consumer::ConsumerRtpStreamStat;
use mediasoup::prelude::*;
use mediasoup::router::PipeToRouterOptions;
use mediasoup::types::scalability_modes::ScalabilityMode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct MediaService {
    state: EngineState,
    worker_pool: Arc<dyn WorkerPoolPort>,
    event_publisher: Arc<dyn EventPublisherPort>,
    metrics: Arc<dyn MetricsPort>,
    produce_cache: Arc<IdempotencyCache<ProduceResult>>,
    consume_cache: Arc<IdempotencyCache<ConsumeResult>>,
    cleanup_handles: Vec<tokio::task::AbortHandle>,
}

impl MediaService {
    pub fn new(
        state: EngineState,
        worker_pool: Arc<dyn WorkerPoolPort>,
        event_publisher: Arc<dyn EventPublisherPort>,
        metrics: Arc<dyn MetricsPort>,
    ) -> Self {
        let produce_cache = Arc::new(IdempotencyCache::default());
        let p_clean = produce_cache.start_periodic_cleanup(Duration::from_secs(30));

        let consume_cache = Arc::new(IdempotencyCache::default());
        let c_clean = consume_cache.start_periodic_cleanup(Duration::from_secs(30));

        Self {
            state,
            worker_pool,
            event_publisher,
            metrics,
            produce_cache,
            consume_cache,
            cleanup_handles: vec![p_clean.abort_handle(), c_clean.abort_handle()],
        }
    }

    pub fn abort_cleanup_tasks(&self) {
        for handle in &self.cleanup_handles {
            handle.abort();
        }
    }
}

#[async_trait]
impl MediaUseCase for MediaService {
    #[tracing::instrument(skip(self, params), fields(router_id = %params.router_id, transport_id = %params.transport_id, peer_id = %params.peer_id, request_id = %params.request_id))]
    async fn produce(&self, params: ProduceParams) -> Result<ProduceResult, SfuDomainError> {
        if self.state.is_draining.load(Ordering::SeqCst) {
            return Err(SfuDomainError::SystemDraining(
                "Node is draining; rejecting new producer".to_string(),
            ));
        }

        validation::validate_not_empty("request_id", &params.request_id)?;
        validation::validate_not_empty("router_id", &params.router_id)?;
        validation::validate_not_empty("transport_id", &params.transport_id)?;
        validation::validate_not_empty("peer_id", &params.peer_id)?;

        // Include params.paused in payload hash (Logic bug fix)
        let payload_bytes = format!(
            "{}:{}:{}:{}:{}:{}:{}",
            params.request_id,
            params.router_id,
            params.transport_id,
            params.peer_id,
            params.kind,
            params.raw_rtp_parameters_json,
            params.paused
        )
        .into_bytes();

        let metrics_ref = self.metrics.clone();

        self.produce_cache
            .execute_or_get_with_hook(
                &params.request_id,
                &payload_bytes,
                || async {
                    // Extract transport, check router ownership, and DROP guard before .await (Deadlock fix)
                    let (transport, physical_router_id, worker_index) = {
                        let t_item =
                            self.state
                                .transports
                                .get(&params.transport_id)
                                .ok_or_else(|| {
                                    SfuDomainError::NotFound(format!(
                                        "Transport '{}' not found",
                                        params.transport_id
                                    ))
                                })?;

                        if t_item.is_closed.load(Ordering::SeqCst) {
                            return Err(SfuDomainError::NotFound(format!(
                                "Transport '{}' is closed",
                                params.transport_id
                            )));
                        }

                        if t_item.router_id != params.router_id {
                            return Err(SfuDomainError::NotFound(format!(
                                "Transport '{}' does not belong to router '{}'",
                                params.transport_id, params.router_id
                            )));
                        }

                        (
                            t_item.transport.clone(),
                            t_item.physical_router_id.clone(),
                            t_item.worker_index,
                        )
                    };

                    let ms_kind = to_mediasoup_kind(params.kind);

                    let mut options = ProducerOptions::new(ms_kind, params.rtp_parameters);
                    options.paused = params.paused;

                    // Await on produce without holding any DashMap guard
                    let producer = transport.produce(options).await.map_err(|e| {
                        SfuDomainError::Mediasoup(format!("Failed to produce: {e}"))
                    })?;

                    if !self.state.is_router_active(&params.router_id) {
                        drop(producer);
                        return Err(SfuDomainError::NotFound(format!(
                            "Router '{}' was closed during produce",
                            params.router_id
                        )));
                    }

                    let transport_active = self
                        .state
                        .transports
                        .get(&params.transport_id)
                        .map(|t| !t.is_closed.load(Ordering::SeqCst))
                        .unwrap_or(false);
                    if !transport_active {
                        drop(producer);
                        return Err(SfuDomainError::NotFound(format!(
                            "Transport '{}' was closed during produce",
                            params.transport_id
                        )));
                    }

                    let producer_id = producer.id().to_string();
                    let is_closed = Arc::new(AtomicBool::new(false));

                    // Register score callback with hot-path change throttling
                    let event_publisher = self.event_publisher.clone();
                    let r_id = params.router_id.clone();
                    let p_id = producer_id.clone();
                    let peer_id = params.peer_id.clone();
                    let state_ref = self.state.clone();
                    let closed_ref = is_closed.clone();
                    let last_score = Arc::new(std::sync::atomic::AtomicU32::new(u32::MAX));

                    producer
                        .on_score(move |scores| {
                            if closed_ref.load(Ordering::SeqCst)
                                || !state_ref.is_router_active(&r_id)
                            {
                                return;
                            }

                            let avg_score = if scores.is_empty() {
                                10
                            } else {
                                (scores.iter().map(|s| s.score as u32).sum::<u32>())
                                    / (scores.len() as u32)
                            };

                            let prev = last_score.swap(avg_score, Ordering::Relaxed);
                            if prev == avg_score {
                                return; // Skip if score hasn't changed (Hot-path throttling)
                            }

                            let all_scores = scores.iter().map(|s| s.score as u32).collect();

                            event_publisher
                                .publish_score(&r_id, &p_id, &peer_id, true, avg_score, all_scores);
                        })
                        .detach();

                    let encodings = producer
                        .rtp_parameters()
                        .encodings
                        .iter()
                        .map(|enc| SimulcastEncodingInfo {
                            rid: enc.rid.clone().unwrap_or_default(),
                            ssrc: enc.ssrc.unwrap_or(0),
                            rtx_ssrc: enc.rtx.as_ref().map(|r| r.ssrc).unwrap_or(0),
                            max_bitrate: enc.max_bitrate.unwrap_or(0) as u32,
                            scalability_mode: if enc.scalability_mode == ScalabilityMode::None {
                                String::new()
                            } else {
                                enc.scalability_mode.to_string()
                            },
                        })
                        .collect();

                    if params.kind == MediaKind::Audio {
                        let observer_opt =
                            self.state.routers.get(&params.router_id).and_then(|r| {
                                if worker_index == r.worker_index {
                                    Some(r.audio_level_observer.clone())
                                } else {
                                    r.sub_routers
                                        .get(&worker_index)
                                        .map(|sub| sub.audio_level_observer.clone())
                                }
                            });
                        if let Some(observer) = observer_opt {
                            use mediasoup::rtp_observer::{
                                RtpObserver, RtpObserverAddProducerOptions,
                            };
                            if let Err(e) = observer
                                .add_producer(RtpObserverAddProducerOptions::new(producer.id()))
                                .await
                            {
                                tracing::warn!(
                                    router_id = %params.router_id,
                                    producer_id = %producer_id,
                                    worker_index = %worker_index,
                                    error = %e,
                                    "Failed to add audio producer to AudioLevelObserver"
                                );
                            }
                        }
                    }

                    let item = ProducerItem {
                        producer,
                        router_id: params.router_id.clone(),
                        physical_router_id,
                        worker_index,
                        transport_id: params.transport_id.clone(),
                        peer_id: params.peer_id.clone(),
                        kind: params.kind,
                        is_closed,
                    };

                    self.state
                        .producers
                        .insert((params.router_id.clone(), producer_id.clone()), item);
                    if let Some(set) = self.state.transport_producers.get(&params.transport_id) {
                        set.insert(producer_id.clone());
                    }
                    self.state
                        .producer_consumers
                        .insert(producer_id.clone(), DashSet::new());
                    let p_weight = if params.kind == MediaKind::Audio {
                        2
                    } else {
                        10
                    };
                    self.worker_pool.add_resource_score(worker_index, p_weight);
                    self.metrics.inc_active_producers(&params.kind.to_string());

                    Ok(ProduceResult {
                        producer_id,
                        kind: params.kind.to_string(),
                        encodings,
                    })
                },
                move || {
                    metrics_ref.record_idempotency_hit();
                },
            )
            .await
    }

    #[tracing::instrument(skip(self, params), fields(router_id = %params.router_id, transport_id = %params.subscriber_transport_id, producer_id = %params.producer_id, request_id = %params.request_id))]
    async fn consume(&self, params: ConsumeParams) -> Result<ConsumeResult, SfuDomainError> {
        if self.state.is_draining.load(Ordering::SeqCst) {
            return Err(SfuDomainError::SystemDraining(
                "Node is draining; rejecting new consumer".to_string(),
            ));
        }

        validation::validate_not_empty("request_id", &params.request_id)?;
        validation::validate_not_empty("router_id", &params.router_id)?;
        validation::validate_not_empty("subscriber_transport_id", &params.subscriber_transport_id)?;
        validation::validate_not_empty("producer_id", &params.producer_id)?;

        // Include params.paused in payload hash (Logic bug fix)
        let payload_bytes = format!(
            "{}:{}:{}:{}:{}:{}",
            params.request_id,
            params.router_id,
            params.subscriber_transport_id,
            params.producer_id,
            params.raw_rtp_capabilities_json,
            params.paused
        )
        .into_bytes();

        let metrics_ref = self.metrics.clone();

        self.consume_cache
            .execute_or_get_with_hook(
                &params.request_id,
                &payload_bytes,
                || async {
                    let (producer_physical_router_id, producer_worker_index) = {
                        let p_item = self
                            .state
                            .get_producer(&params.router_id, &params.producer_id)
                            .ok_or_else(|| {
                                SfuDomainError::NotFound(format!(
                                    "Producer '{}' not found",
                                    params.producer_id
                                ))
                            })?;

                        if p_item.is_closed.load(Ordering::SeqCst) {
                            return Err(SfuDomainError::NotFound(format!(
                                "Producer '{}' is closed",
                                params.producer_id
                            )));
                        }

                        (p_item.physical_router_id.clone(), p_item.worker_index)
                    };

                    // Extract subscriber transport, verify router ownership, and DROP guard before .await
                    let (transport, peer_id, sub_physical_router_id, sub_worker_index) = {
                        let t_item = self
                            .state
                            .transports
                            .get(&params.subscriber_transport_id)
                            .ok_or_else(|| {
                                SfuDomainError::NotFound(format!(
                                    "Transport '{}' not found",
                                    params.subscriber_transport_id
                                ))
                            })?;

                        if t_item.is_closed.load(Ordering::SeqCst) {
                            return Err(SfuDomainError::NotFound(format!(
                                "Transport '{}' is closed",
                                params.subscriber_transport_id
                            )));
                        }

                        if t_item.router_id != params.router_id {
                            return Err(SfuDomainError::NotFound(format!(
                                "Transport '{}' does not belong to router '{}'",
                                params.subscriber_transport_id, params.router_id
                            )));
                        }

                        (
                            t_item.transport.clone(),
                            t_item.peer_id.clone(),
                            t_item.physical_router_id.clone(),
                            t_item.worker_index,
                        )
                    };

                    // Intra-Node Multi-Worker Spillover: pipe if producer and subscriber transport are on different workers
                    if producer_physical_router_id != sub_physical_router_id {
                        let singleflight_key =
                            format!("{}:{}", params.producer_id, sub_physical_router_id);
                        let lock = self
                            .state
                            .pipe_singleflight
                            .entry(singleflight_key.clone())
                            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                            .clone();

                        let _guard = lock.lock().await;
                        let _key_guard = crate::application::state::SingleflightKeyGuard::new(
                            self.state.pipe_singleflight.clone(),
                            singleflight_key.clone(),
                        );

                        let already_piped = self
                            .state
                            .piped_producers
                            .get(&params.producer_id)
                            .map(|entries| {
                                entries
                                    .iter()
                                    .any(|e| e.target_router_id == sub_physical_router_id)
                            })
                            .unwrap_or(false);

                        if !already_piped {
                            let (pub_router, sub_router) = {
                                let r_item = self.state.routers.get(&params.router_id).ok_or_else(|| {
                                    SfuDomainError::NotFound(format!(
                                        "Router '{}' not found",
                                        params.router_id
                                    ))
                                })?;

                                if r_item.is_closed.load(Ordering::SeqCst) {
                                    return Err(SfuDomainError::NotFound(format!(
                                        "Router '{}' is closed",
                                        params.router_id
                                    )));
                                }

                                let pub_r = if producer_worker_index == r_item.worker_index {
                                    r_item.router.clone()
                                } else {
                                    r_item
                                        .sub_routers
                                        .get(&producer_worker_index)
                                        .map(|s| s.router.clone())
                                        .ok_or_else(|| {
                                            SfuDomainError::NotFound(format!(
                                                "Sub-router on worker {producer_worker_index} not found"
                                            ))
                                        })?
                                };

                                let sub_r = if sub_worker_index == r_item.worker_index {
                                    r_item.router.clone()
                                } else {
                                    r_item
                                        .sub_routers
                                        .get(&sub_worker_index)
                                        .map(|s| s.router.clone())
                                        .ok_or_else(|| {
                                            SfuDomainError::NotFound(format!(
                                                "Sub-router on worker {sub_worker_index} not found"
                                            ))
                                        })?
                                };

                                (pub_r, sub_r)
                            };

                            let producer_id_obj: ProducerId =
                                params.producer_id.parse().map_err(|e| {
                                    SfuDomainError::InvalidArgument(format!(
                                        "Invalid producer_id: {e}"
                                    ))
                                })?;

                            // Pre-check codec capability on source router before performing cross-worker pipe
                            if !pub_router.can_consume(&producer_id_obj, &params.rtp_capabilities) {
                                return Err(SfuDomainError::IncompatibleCodec(format!(
                                    "Router cannot consume producer '{}' with given RTP capabilities",
                                    params.producer_id
                                )));
                            }

                            let pipe_pair = pub_router
                                .pipe_producer_to_router(
                                    producer_id_obj,
                                    PipeToRouterOptions::new(sub_router),
                                )
                                .await
                                .map_err(|e| {
                                    SfuDomainError::Mediasoup(format!(
                                        "Failed to pipe producer to router: {e}"
                                    ))
                                })?;

                            let is_source_paused = self
                                .state
                                .get_producer(&params.router_id, &params.producer_id)
                                .map(|p| p.producer.paused())
                                .unwrap_or(false);

                            let pipe_producer = pipe_pair.pipe_producer.into_inner();
                            if is_source_paused {
                                let _ = pipe_producer.pause().await;
                            }

                            self.worker_pool.add_resource_score(producer_worker_index, 1);
                            self.worker_pool.add_resource_score(sub_worker_index, 1);

                            let entry = PipedProducerEntry {
                                pipe_consumer: pipe_pair.pipe_consumer,
                                pipe_producer,
                                target_router_id: sub_physical_router_id.clone(),
                                target_worker_index: sub_worker_index,
                            };

                            self.state
                                .piped_producers
                                .entry(params.producer_id.clone())
                                .or_default()
                                .push(entry);
                        }
                    }

                    // Extract target router where subscriber transport lives
                    let target_router = {
                        let r_item = self.state.routers.get(&params.router_id).ok_or_else(|| {
                            SfuDomainError::NotFound(format!(
                                "Router '{}' not found",
                                params.router_id
                            ))
                        })?;

                        if r_item.is_closed.load(Ordering::SeqCst) {
                            return Err(SfuDomainError::NotFound(format!(
                                "Router '{}' is closed",
                                params.router_id
                            )));
                        }

                        if sub_worker_index == r_item.worker_index {
                            r_item.router.clone()
                        } else {
                            r_item
                                .sub_routers
                                .get(&sub_worker_index)
                                .map(|s| s.router.clone())
                                .ok_or_else(|| {
                                    SfuDomainError::NotFound(format!(
                                        "Sub-router on worker {sub_worker_index} not found"
                                    ))
                                })?
                        }
                    };

                    let producer_id: ProducerId = params.producer_id.parse().map_err(|e| {
                        SfuDomainError::InvalidArgument(format!("Invalid producer_id: {e}"))
                    })?;

                    // Mandatory can_consume check on target router
                    if !target_router.can_consume(&producer_id, &params.rtp_capabilities) {
                        return Err(SfuDomainError::IncompatibleCodec(format!(
                            "Router cannot consume producer '{}' with given RTP capabilities",
                            params.producer_id
                        )));
                    }

                    let mut options = ConsumerOptions::new(producer_id, params.rtp_capabilities);
                    options.paused = params.paused;

                    // Await on consume without holding any DashMap guard
                    let consumer = transport.consume(options).await.map_err(|e| {
                        SfuDomainError::Mediasoup(format!("Failed to consume: {e}"))
                    })?;

                    if !self.state.is_router_active(&params.router_id) {
                        drop(consumer);
                        return Err(SfuDomainError::NotFound(format!(
                            "Router '{}' was closed during consume",
                            params.router_id
                        )));
                    }

                    let transport_active = self
                        .state
                        .transports
                        .get(&params.subscriber_transport_id)
                        .map(|t| !t.is_closed.load(Ordering::SeqCst))
                        .unwrap_or(false);
                    if !transport_active {
                        drop(consumer);
                        return Err(SfuDomainError::NotFound(format!(
                            "Transport '{}' was closed during consume",
                            params.subscriber_transport_id
                        )));
                    }

                    let producer_active = self
                        .state
                        .get_producer(&params.router_id, &params.producer_id)
                        .map(|p| !p.is_closed.load(Ordering::SeqCst))
                        .unwrap_or(false);
                    if !producer_active {
                        drop(consumer);
                        return Err(SfuDomainError::NotFound(format!(
                            "Producer '{}' was closed during consume",
                            params.producer_id
                        )));
                    }

                    let consumer_id = consumer.id().to_string();
                    let is_closed = Arc::new(AtomicBool::new(false));

                    // Register callbacks
                    let event_publisher = self.event_publisher.clone();
                    let r_id = params.router_id.clone();
                    let c_id = consumer_id.clone();
                    let p_id_str = params.producer_id.clone();
                    let peer_id_clone = peer_id.clone();
                    let state_ref = self.state.clone();
                    let closed_ref = is_closed.clone();

                    let last_consumer_score = Arc::new(std::sync::atomic::AtomicU32::new(u32::MAX));
                    consumer
                        .on_score({
                            let r_id = r_id.clone();
                            let c_id = c_id.clone();
                            let peer_id = peer_id_clone.clone();
                            let event_publisher = event_publisher.clone();
                            let state_ref = state_ref.clone();
                            let closed_ref = closed_ref.clone();
                            let last_score_ref = last_consumer_score.clone();

                            move |score| {
                                if closed_ref.load(Ordering::SeqCst)
                                    || !state_ref.is_router_active(&r_id)
                                {
                                    return;
                                }

                                let cur_score = score.score as u32;
                                let prev = last_score_ref.swap(cur_score, Ordering::Relaxed);
                                if prev == cur_score {
                                    return; // Hot-Path throttling: skip if score unchanged
                                }

                                event_publisher.publish_score(
                                    &r_id,
                                    &c_id,
                                    &peer_id,
                                    false,
                                    cur_score,
                                    score.producer_scores.iter().map(|&s| s as u32).collect(),
                                );
                            }
                        })
                        .detach();

                    let consumer_for_closure = consumer.clone();
                    consumer
                        .on_layers_change({
                            let r_id = r_id.clone();
                            let c_id = c_id.clone();
                            let p_id = p_id_str.clone();
                            let peer_id = peer_id_clone;
                            let event_publisher = event_publisher.clone();
                            let state_ref = state_ref.clone();
                            let closed_ref = closed_ref.clone();

                            move |layers| {
                                if closed_ref.load(Ordering::SeqCst)
                                    || !state_ref.is_router_active(&r_id)
                                {
                                    return;
                                }

                                let pref = consumer_for_closure.preferred_layers();
                                event_publisher.publish_layer_change(
                                    crate::ports::outbound::LayerChangeEventParams {
                                        router_id: &r_id,
                                        consumer_id: &c_id,
                                        producer_id: &p_id,
                                        peer_id: &peer_id,
                                        preferred_spatial: pref
                                            .map(|p| p.spatial_layer as i32)
                                            .unwrap_or(-1),
                                        current_spatial: layers
                                            .map(|l| l.spatial_layer as i32)
                                            .unwrap_or(-1),
                                        preferred_temporal: pref
                                            .and_then(|p| p.temporal_layer)
                                            .map(|t| t as i32)
                                            .unwrap_or(-1),
                                        current_temporal: layers
                                            .and_then(|l| l.temporal_layer)
                                            .map(|t| t as i32)
                                            .unwrap_or(-1),
                                    },
                                );
                            }
                        })
                        .detach();

                    let rtp_params = consumer.rtp_parameters();
                    let raw_rtp_json = serde_json::to_string(rtp_params)
                        .map_err(|e| SfuDomainError::Internal(e.to_string()))?;

                    let first_codec = rtp_params.codecs.first();
                    let first_encoding = rtp_params.encodings.first();

                    let ssrc = first_encoding.and_then(|e| e.ssrc).unwrap_or(0);
                    let rtx_ssrc = first_encoding
                        .and_then(|e| e.rtx.as_ref())
                        .map(|r| r.ssrc)
                        .unwrap_or(0);
                    let payload_type = first_codec.map(|c| c.payload_type() as u32).unwrap_or(0);
                    let rtx_payload_type = rtp_params
                        .codecs
                        .iter()
                        .find(|c| c.mime_type().as_str().to_lowercase().ends_with("/rtx"))
                        .map(|c| c.payload_type() as u32)
                        .unwrap_or(0);
                    let codec_mime_type = first_codec
                        .map(|c| c.mime_type().as_str().to_string())
                        .unwrap_or_default();
                    let clock_rate = first_codec.map(|c| c.clock_rate().get()).unwrap_or(0);

                    let encodings = rtp_params
                        .encodings
                        .iter()
                        .map(|enc| SimulcastEncodingInfo {
                            rid: enc.rid.clone().unwrap_or_default(),
                            ssrc: enc.ssrc.unwrap_or(0),
                            rtx_ssrc: enc.rtx.as_ref().map(|r| r.ssrc).unwrap_or(0),
                            max_bitrate: enc.max_bitrate.unwrap_or(0) as u32,
                            scalability_mode: if enc.scalability_mode == ScalabilityMode::None {
                                String::new()
                            } else {
                                enc.scalability_mode.to_string()
                            },
                        })
                        .collect();

                    let current_layers = consumer.current_layers();
                    let initial_spatial_layer =
                        current_layers.map(|l| l.spatial_layer as i32).unwrap_or(-1);
                    let initial_temporal_layer = current_layers
                        .and_then(|l| l.temporal_layer)
                        .map(|t| t as i32)
                        .unwrap_or(-1);

                    let consumer_kind: MediaKind = from_mediasoup_kind(consumer.kind());

                    let item = ConsumerItem {
                        consumer,
                        router_id: params.router_id.clone(),
                        physical_router_id: sub_physical_router_id,
                        worker_index: sub_worker_index,
                        transport_id: params.subscriber_transport_id.clone(),
                        producer_id: params.producer_id.clone(),
                        peer_id,
                        kind: consumer_kind,
                        is_closed,
                    };

                    self.state.consumers.insert(consumer_id.clone(), item);
                    if let Some(set) = self
                        .state
                        .transport_consumers
                        .get(&params.subscriber_transport_id)
                    {
                        set.insert(consumer_id.clone());
                    }
                    if let Some(set) = self.state.producer_consumers.get(&params.producer_id) {
                        set.insert(consumer_id.clone());
                    }
                    let c_weight = if consumer_kind == MediaKind::Audio { 1 } else { 2 };
                    self.worker_pool.add_resource_score(sub_worker_index, c_weight);
                    self.metrics
                        .inc_active_consumers(&consumer_kind.to_string());

                    Ok(ConsumeResult {
                        consumer_id,
                        producer_id: params.producer_id,
                        kind: consumer_kind.to_string(),
                        rtp_parameters_json: raw_rtp_json,
                        ssrc,
                        rtx_ssrc,
                        payload_type,
                        rtx_payload_type,
                        codec_mime_type,
                        clock_rate,
                        encodings,
                        initial_spatial_layer,
                        initial_temporal_layer,
                    })
                },
                move || {
                    metrics_ref.record_idempotency_hit();
                },
            )
            .await
    }

    #[tracing::instrument(skip(self), fields(router_id = %router_id, resource_id = %resource_id))]
    async fn set_media_paused(
        &self,
        router_id: &str,
        resource_id: &str,
        is_producer: bool,
        paused: bool,
    ) -> Result<(), SfuDomainError> {
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("resource_id", resource_id)?;

        if is_producer {
            // Extract producer and DROP guard before .await
            let producer = {
                let p_item = self
                    .state
                    .get_producer(router_id, resource_id)
                    .ok_or_else(|| {
                        SfuDomainError::NotFound(format!("Producer '{resource_id}' not found"))
                    })?;

                if p_item.is_closed.load(Ordering::SeqCst) {
                    return Err(SfuDomainError::NotFound(format!(
                        "Producer '{resource_id}' is closed"
                    )));
                }

                p_item.producer.clone()
            };

            if paused {
                producer
                    .pause()
                    .await
                    .map_err(|e| SfuDomainError::Mediasoup(e.to_string()))?;
            } else {
                producer
                    .resume()
                    .await
                    .map_err(|e| SfuDomainError::Mediasoup(e.to_string()))?;
            }

            let piped_to_pause: Vec<_> = self
                .state
                .piped_producers
                .get(resource_id)
                .map(|entries| entries.iter().map(|e| e.pipe_producer.clone()).collect())
                .unwrap_or_default();

            for p in piped_to_pause {
                if paused {
                    let _ = p.pause().await;
                } else {
                    let _ = p.resume().await;
                }
            }
        } else {
            // Extract consumer and DROP guard before .await
            let consumer = {
                let c_item = self.state.consumers.get(resource_id).ok_or_else(|| {
                    SfuDomainError::NotFound(format!("Consumer '{resource_id}' not found"))
                })?;

                if c_item.is_closed.load(Ordering::SeqCst) {
                    return Err(SfuDomainError::NotFound(format!(
                        "Consumer '{resource_id}' is closed"
                    )));
                }

                if c_item.router_id != router_id {
                    return Err(SfuDomainError::NotFound(format!(
                        "Consumer '{resource_id}' does not belong to router '{router_id}'"
                    )));
                }

                c_item.consumer.clone()
            };

            if paused {
                consumer
                    .pause()
                    .await
                    .map_err(|e| SfuDomainError::Mediasoup(e.to_string()))?;
            } else {
                consumer
                    .resume()
                    .await
                    .map_err(|e| SfuDomainError::Mediasoup(e.to_string()))?;
            }
        }

        Ok(())
    }

    #[tracing::instrument(skip(self), fields(router_id = %router_id, consumer_id = %consumer_id))]
    async fn set_consumer_preferred_layers(
        &self,
        router_id: &str,
        consumer_id: &str,
        layers: ConsumerLayersInfo,
    ) -> Result<(), SfuDomainError> {
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("consumer_id", consumer_id)?;

        // Extract consumer and DROP guard before .await
        let consumer = {
            let c_item = self.state.consumers.get(consumer_id).ok_or_else(|| {
                SfuDomainError::NotFound(format!("Consumer '{consumer_id}' not found"))
            })?;

            if c_item.is_closed.load(Ordering::SeqCst) {
                return Err(SfuDomainError::NotFound(format!(
                    "Consumer '{consumer_id}' is closed"
                )));
            }

            if c_item.router_id != router_id {
                return Err(SfuDomainError::NotFound(format!(
                    "Consumer '{consumer_id}' does not belong to router '{router_id}'"
                )));
            }

            c_item.consumer.clone()
        };

        consumer
            .set_preferred_layers(ConsumerLayers {
                spatial_layer: layers.spatial_layer,
                temporal_layer: layers.temporal_layer,
            })
            .await
            .map_err(|e| {
                SfuDomainError::Mediasoup(format!("Failed to set preferred layers: {e}"))
            })?;

        Ok(())
    }

    #[tracing::instrument(skip(self), fields(router_id = %router_id, producer_id = %producer_id))]
    fn close_producer(&self, router_id: &str, producer_id: &str) -> Result<(), SfuDomainError> {
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("producer_id", producer_id)?;

        if let Some((_, item)) = self
            .state
            .producers
            .remove(&(router_id.to_string(), producer_id.to_string()))
        {
            item.is_closed.store(true, Ordering::SeqCst);
            let p_weight = if item.kind == MediaKind::Audio { 2 } else { 10 };
            self.worker_pool
                .sub_resource_score(item.worker_index, p_weight);
            self.metrics.dec_active_producers(&item.kind.to_string());

            if let Some(set) = self.state.transport_producers.get(&item.transport_id) {
                set.remove(producer_id);
            }

            if let Some((_, piped_entries)) = self.state.piped_producers.remove(producer_id) {
                for entry in piped_entries {
                    self.worker_pool.sub_resource_score(item.worker_index, 1);
                    self.worker_pool
                        .sub_resource_score(entry.target_worker_index, 1);
                    // Dropping entry closes both pipe_consumer and pipe_producer via mediasoup RAII
                }
            }
            self.state
                .pipe_singleflight
                .retain(|k, _| !k.starts_with(&format!("{}:", producer_id)));

            self.event_publisher.publish_resource_closed(
                router_id,
                "producer",
                producer_id,
                "producer_closed",
            );

            // Cascade close all consumers connected to this producer (Fix orphaned consumers)
            if let Some((_, c_ids)) = self.state.producer_consumers.remove(producer_id) {
                for cid in c_ids {
                    if let Some(c) = self.state.cascade_remove_consumer(&cid) {
                        let c_weight = if c.kind == MediaKind::Audio { 1 } else { 2 };
                        self.worker_pool
                            .sub_resource_score(c.worker_index, c_weight);
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

        Ok(())
    }

    #[tracing::instrument(skip(self), fields(router_id = %router_id, consumer_id = %consumer_id))]
    fn close_consumer(&self, router_id: &str, consumer_id: &str) -> Result<(), SfuDomainError> {
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("consumer_id", consumer_id)?;

        let router_matches = self
            .state
            .consumers
            .get(consumer_id)
            .map(|c| c.router_id == router_id);

        match router_matches {
            Some(true) => {
                if let Some(item) = self.state.cascade_remove_consumer(consumer_id) {
                    let c_weight = if item.kind == MediaKind::Audio { 1 } else { 2 };
                    self.worker_pool
                        .sub_resource_score(item.worker_index, c_weight);
                    self.metrics.dec_active_consumers(&item.kind.to_string());

                    // Clean up intra-node pipe if this was the last consumer on the target physical router
                    if let Some((piped_entry, pub_worker_idx)) =
                        self.state.cleanup_piped_producer_if_last_consumer(
                            &item.router_id,
                            &item.producer_id,
                            &item.physical_router_id,
                        )
                    {
                        self.worker_pool.sub_resource_score(pub_worker_idx, 1);
                        self.worker_pool
                            .sub_resource_score(piped_entry.target_worker_index, 1);
                    }

                    self.event_publisher.publish_resource_closed(
                        router_id,
                        "consumer",
                        consumer_id,
                        "consumer_closed",
                    );
                }
                Ok(())
            }
            Some(false) => Err(SfuDomainError::NotFound(format!(
                "Consumer '{consumer_id}' does not belong to router '{router_id}'"
            ))),
            None => Ok(()),
        }
    }

    #[tracing::instrument(skip(self), fields(router_id = %router_id, resource_id = %resource_id))]
    async fn get_stats(
        &self,
        router_id: &str,
        resource_id: &str,
        resource_type: &str,
    ) -> Result<ResourceStatsInfo, SfuDomainError> {
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("resource_id", resource_id)?;

        match resource_type.to_lowercase().as_str() {
            "transport" => {
                let transport = {
                    let t_item = self.state.transports.get(resource_id).ok_or_else(|| {
                        SfuDomainError::NotFound(format!("Transport '{resource_id}' not found"))
                    })?;

                    if t_item.is_closed.load(Ordering::SeqCst) {
                        return Err(SfuDomainError::NotFound(format!(
                            "Transport '{resource_id}' is closed"
                        )));
                    }

                    if t_item.router_id != router_id {
                        return Err(SfuDomainError::NotFound(format!(
                            "Transport '{resource_id}' does not belong to router '{router_id}'"
                        )));
                    }

                    t_item.transport.clone()
                };

                let producer_ids: Vec<String> = self
                    .state
                    .transport_producers
                    .get(resource_id)
                    .map(|set| set.iter().map(|id| id.clone()).collect())
                    .unwrap_or_default();

                let producer_futs: Vec<_> = producer_ids
                    .into_iter()
                    .filter_map(|pid| {
                        self.state
                            .get_producer(router_id, &pid)
                            .map(|p| p.producer.clone())
                    })
                    .map(|p| async move { p.get_stats().await.ok().unwrap_or_default() })
                    .collect();

                let consumer_ids: Vec<String> = self
                    .state
                    .transport_consumers
                    .get(resource_id)
                    .map(|set| set.iter().map(|id| id.clone()).collect())
                    .unwrap_or_default();

                let consumer_futs: Vec<_> = consumer_ids
                    .into_iter()
                    .filter_map(|cid| self.state.consumers.get(&cid).map(|c| c.consumer.clone()))
                    .map(|c| async move { c.get_stats().await.ok().unwrap_or_default() })
                    .collect();

                let (transport_stats_res, p_stats_list, c_stats_list) = tokio::join!(
                    transport.get_stats(),
                    futures_util::future::join_all(producer_futs),
                    futures_util::future::join_all(consumer_futs),
                );

                let stats =
                    transport_stats_res.map_err(|e| SfuDomainError::Mediasoup(e.to_string()))?;

                let stats_json = serde_json::to_string(&stats)
                    .map_err(|e| SfuDomainError::Internal(e.to_string()))?;

                let bytes_sent = stats.iter().map(|s| s.bytes_sent).sum();
                let bytes_received = stats.iter().map(|s| s.bytes_received).sum();

                // Aggregate packet counts from attached producers and consumers
                let mut packets_sent = 0u64;
                let mut packets_received = 0u64;

                for p_stats in p_stats_list {
                    for s in p_stats {
                        packets_received += s.packet_count;
                    }
                }

                for c_stats in c_stats_list {
                    for s in c_stats {
                        if let ConsumerRtpStreamStat::Outbound(out) = s {
                            packets_sent += out.packet_count;
                        }
                    }
                }

                Ok(ResourceStatsInfo {
                    stats_json,
                    bytes_sent,
                    bytes_received,
                    packets_sent,
                    packets_received,
                    current_spatial_layer: -1,
                    current_temporal_layer: -1,
                })
            }
            "producer" => {
                let producer = {
                    let p_item =
                        self.state
                            .get_producer(router_id, resource_id)
                            .ok_or_else(|| {
                                SfuDomainError::NotFound(format!(
                                    "Producer '{resource_id}' not found"
                                ))
                            })?;

                    if p_item.is_closed.load(Ordering::SeqCst) {
                        return Err(SfuDomainError::NotFound(format!(
                            "Producer '{resource_id}' is closed"
                        )));
                    }

                    p_item.producer.clone()
                };

                let stats = producer
                    .get_stats()
                    .await
                    .map_err(|e| SfuDomainError::Mediasoup(e.to_string()))?;

                let stats_json = serde_json::to_string(&stats)
                    .map_err(|e| SfuDomainError::Internal(e.to_string()))?;

                let (bytes_received, packets_received) =
                    stats.iter().fold((0u64, 0u64), |(b, p), s| {
                        (b + s.byte_count, p + s.packet_count)
                    });

                Ok(ResourceStatsInfo {
                    stats_json,
                    bytes_sent: 0,
                    bytes_received,
                    packets_sent: 0,
                    packets_received,
                    current_spatial_layer: -1,
                    current_temporal_layer: -1,
                })
            }
            "consumer" => {
                let (consumer, current_layers) = {
                    let c_item = self.state.consumers.get(resource_id).ok_or_else(|| {
                        SfuDomainError::NotFound(format!("Consumer '{resource_id}' not found"))
                    })?;

                    if c_item.is_closed.load(Ordering::SeqCst) {
                        return Err(SfuDomainError::NotFound(format!(
                            "Consumer '{resource_id}' is closed"
                        )));
                    }

                    if c_item.router_id != router_id {
                        return Err(SfuDomainError::NotFound(format!(
                            "Consumer '{resource_id}' does not belong to router '{router_id}'"
                        )));
                    }

                    (c_item.consumer.clone(), c_item.consumer.current_layers())
                };

                let stats = consumer
                    .get_stats()
                    .await
                    .map_err(|e| SfuDomainError::Mediasoup(e.to_string()))?;

                let stats_json = serde_json::to_string(&stats)
                    .map_err(|e| SfuDomainError::Internal(e.to_string()))?;

                // Fix Consumer double counting: Only Outbound goes to bytes_sent / packets_sent!
                let (bytes_sent, packets_sent) =
                    stats.iter().fold((0u64, 0u64), |(b, p), s| match s {
                        ConsumerRtpStreamStat::Outbound(out) => {
                            (b + out.byte_count, p + out.packet_count)
                        }
                        ConsumerRtpStreamStat::Inbound(_) => (b, p),
                    });

                let current_spatial_layer =
                    current_layers.map(|l| l.spatial_layer as i32).unwrap_or(-1);
                let current_temporal_layer = current_layers
                    .and_then(|l| l.temporal_layer)
                    .map(|t| t as i32)
                    .unwrap_or(-1);

                Ok(ResourceStatsInfo {
                    stats_json,
                    bytes_sent,
                    bytes_received: 0,
                    packets_sent,
                    packets_received: 0,
                    current_spatial_layer,
                    current_temporal_layer,
                })
            }
            other => Err(SfuDomainError::InvalidArgument(format!(
                "Invalid resource_type '{other}' for get_stats"
            ))),
        }
    }
}

pub(crate) fn to_mediasoup_kind(kind: MediaKind) -> mediasoup::prelude::MediaKind {
    match kind {
        MediaKind::Audio => mediasoup::prelude::MediaKind::Audio,
        MediaKind::Video => mediasoup::prelude::MediaKind::Video,
    }
}

pub(crate) fn from_mediasoup_kind(kind: mediasoup::prelude::MediaKind) -> MediaKind {
    match kind {
        mediasoup::prelude::MediaKind::Audio => MediaKind::Audio,
        mediasoup::prelude::MediaKind::Video => MediaKind::Video,
    }
}
