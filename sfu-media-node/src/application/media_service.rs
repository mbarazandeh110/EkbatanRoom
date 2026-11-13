use crate::application::state::{ConsumerItem, EngineState, PipedProducerEntry, ProducerItem};
use crate::config::Config;
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
use mediasoup::consumer::ConsumerRtpStreamStat;
use mediasoup::prelude::*;
use mediasoup::router::PipeToRouterOptions;
use mediasoup::types::scalability_modes::ScalabilityMode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
enum AnyMediaTransport {
    WebRtc(WebRtcTransport),
    Plain(mediasoup::plain_transport::PlainTransport),
}

impl AnyMediaTransport {
    async fn produce(&self, options: ProducerOptions) -> Result<Producer, String> {
        match self {
            Self::WebRtc(t) => t.produce(options).await.map_err(|e| e.to_string()),
            Self::Plain(t) => t.produce(options).await.map_err(|e| e.to_string()),
        }
    }

    async fn consume(&self, options: ConsumerOptions) -> Result<Consumer, String> {
        match self {
            Self::WebRtc(t) => t.consume(options).await.map_err(|e| e.to_string()),
            Self::Plain(t) => t.consume(options).await.map_err(|e| e.to_string()),
        }
    }

    fn is_active_in_state(&self, state: &EngineState, transport_id: &str) -> bool {
        match self {
            Self::WebRtc(_) => state
                .transports
                .get(transport_id)
                .map(|t| !t.is_closed.load(Ordering::SeqCst))
                .unwrap_or(false),
            Self::Plain(_) => state
                .plain_transports
                .get(transport_id)
                .map(|t| !t.is_closed.load(Ordering::SeqCst))
                .unwrap_or(false),
        }
    }
}

pub struct MediaService {
    config: Config,
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
        config: Config,
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
            config,
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

    fn rollback_unused_pipe(&self, router_id: &str, producer_id: &str, target_router_id: &str) {
        if let Some((piped_entry, pub_worker_idx)) = self
            .state
            .cleanup_piped_producer_if_last_consumer(router_id, producer_id, target_router_id)
        {
            self.worker_pool.sub_resource_score(pub_worker_idx, 1);
            self.worker_pool
                .sub_resource_score(piped_entry.target_worker_index, 1);
        }
    }

    fn resolve_media_transport(
        &self,
        router_id: &str,
        transport_id: &str,
    ) -> Result<(AnyMediaTransport, String, String, usize), SfuDomainError> {
        if let Some(t_item) = self.state.transports.get(transport_id) {
            if t_item.is_closed.load(Ordering::SeqCst) {
                return Err(SfuDomainError::NotFound(format!(
                    "Transport '{transport_id}' is closed"
                )));
            }
            if t_item.router_id != router_id {
                return Err(SfuDomainError::NotFound(format!(
                    "Transport '{transport_id}' does not belong to router '{router_id}'"
                )));
            }
            return Ok((
                AnyMediaTransport::WebRtc(t_item.transport.clone()),
                t_item.peer_id.clone(),
                t_item.physical_router_id.clone(),
                t_item.worker_index,
            ));
        }

        if let Some(pt_item) = self.state.plain_transports.get(transport_id) {
            if pt_item.is_closed.load(Ordering::SeqCst) {
                return Err(SfuDomainError::NotFound(format!(
                    "PlainTransport '{transport_id}' is closed"
                )));
            }
            if pt_item.router_id != router_id {
                return Err(SfuDomainError::NotFound(format!(
                    "PlainTransport '{transport_id}' does not belong to router '{router_id}'"
                )));
            }
            return Ok((
                AnyMediaTransport::Plain(pt_item.transport.clone()),
                "plain-transport".to_string(),
                pt_item.router_id.clone(),
                pt_item.worker_index,
            ));
        }

        Err(SfuDomainError::NotFound(format!(
            "Transport '{transport_id}' not found"
        )))
    }
}

pub fn resolve_default_video_preferred_spatial_layer(producer_encodings_count: usize) -> u8 {
    if let Ok(val) = std::env::var("MEDIASOUP_DEFAULT_VIDEO_PREFERRED_SPATIAL_LAYER")
        .or_else(|_| std::env::var("SFU_DEFAULT_VIDEO_PREFERRED_SPATIAL_LAYER"))
    {
        if let Ok(layer) = val.trim().parse::<u8>() {
            return layer;
        }
    }
    if producer_encodings_count > 1 {
        // Simulcast stream: default to highest available spatial layer (e.g. 2 for 3 layers: 180p, 360p, 720p)
        (producer_encodings_count.saturating_sub(1)).min(2) as u8
    } else {
        // Single-layer stream: layer 0
        0
    }
}

pub(crate) fn extract_simulcast_encodings(
    rtp_parameters: &RtpParameters,
) -> Vec<SimulcastEncodingInfo> {
    rtp_parameters
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
        .collect()
}

pub(crate) fn attach_producer_score_listener(
    producer: &Producer,
    is_closed: &Arc<AtomicBool>,
    state: &EngineState,
    event_publisher: &Arc<dyn EventPublisherPort>,
    router_id: &str,
    producer_id: &str,
    peer_id: &str,
) {
    let event_publisher = event_publisher.clone();
    let r_id = router_id.to_string();
    let p_id = producer_id.to_string();
    let peer_id = peer_id.to_string();
    let state_ref = state.clone();
    let closed_ref = is_closed.clone();
    let last_score = Arc::new(std::sync::atomic::AtomicU32::new(u32::MAX));

    producer
        .on_score(move |scores| {
            if closed_ref.load(Ordering::SeqCst) || !state_ref.is_router_active(&r_id) {
                return;
            }

            let avg_score = if scores.is_empty() {
                10
            } else {
                (scores.iter().map(|s| s.score as u32).sum::<u32>()) / (scores.len() as u32)
            };

            let prev = last_score.swap(avg_score, Ordering::Relaxed);
            if prev == avg_score {
                return; // Skip if score hasn't changed (Hot-path throttling)
            }

            let all_scores = scores.iter().map(|s| s.score as u32).collect();

            event_publisher.publish_score(&r_id, &p_id, &peer_id, true, avg_score, all_scores);
        })
        .detach();
}

pub(crate) fn attach_consumer_score_listener(
    consumer: &Consumer,
    is_closed: &Arc<AtomicBool>,
    state: &EngineState,
    event_publisher: &Arc<dyn EventPublisherPort>,
    router_id: &str,
    consumer_id: &str,
    peer_id: &str,
) {
    let r_id = router_id.to_string();
    let c_id = consumer_id.to_string();
    let peer_id = peer_id.to_string();
    let event_publisher = event_publisher.clone();
    let state_ref = state.clone();
    let closed_ref = is_closed.clone();
    let last_consumer_score = Arc::new(std::sync::atomic::AtomicU32::new(u32::MAX));

    consumer
        .on_score(move |score| {
            if closed_ref.load(Ordering::SeqCst) || !state_ref.is_router_active(&r_id) {
                return;
            }

            let cur_score = score.score as u32;
            let prev = last_consumer_score.swap(cur_score, Ordering::Relaxed);
            if prev == cur_score {
                return;
            }

            event_publisher.publish_score(
                &r_id,
                &c_id,
                &peer_id,
                false,
                cur_score,
                score.producer_scores.iter().map(|&s| s as u32).collect(),
            );
        })
        .detach();
}

#[async_trait]
impl MediaUseCase for MediaService {
    #[tracing::instrument(
        skip(self, params),
        fields(
            room_id = tracing::field::Empty,
            router_id = %params.router_id,
            transport_id = %params.transport_id,
            peer_id = %params.peer_id,
            request_id = %params.request_id
        )
    )]
    async fn produce(&self, params: ProduceParams) -> Result<ProduceResult, SfuDomainError> {
        if let Some(room_id) = self.state.get_room_id_for_router(&params.router_id) {
            tracing::Span::current().record("room_id", room_id.as_str());
        }
        if self.state.is_draining.load(Ordering::SeqCst) {
            return Err(SfuDomainError::SystemDraining(
                "Node is draining; rejecting new producer".to_string(),
            ));
        }

        validation::validate_not_empty("request_id", &params.request_id)?;
        validation::validate_not_empty("router_id", &params.router_id)?;
        validation::validate_not_empty("transport_id", &params.transport_id)?;
        validation::validate_not_empty("peer_id", &params.peer_id)?;

        let paused_bytes = if params.paused { b"1" } else { b"0" };
        let kind_bytes = match params.kind {
            MediaKind::Audio => b"audio",
            MediaKind::Video => b"video",
        };
        let payload_parts: [&[u8]; 7] = [
            params.request_id.as_bytes(),
            params.router_id.as_bytes(),
            params.transport_id.as_bytes(),
            params.peer_id.as_bytes(),
            kind_bytes,
            params.raw_rtp_parameters_json.as_bytes(),
            paused_bytes,
        ];

        let metrics_ref = self.metrics.clone();
        let state_for_valid = self.state.clone();
        let router_id_for_valid = params.router_id.clone();

        self.produce_cache
            .execute_or_get_validated_parts(
                &params.request_id,
                &payload_parts,
                move |cached: &ProduceResult| {
                    state_for_valid
                        .get_producer(&router_id_for_valid, &cached.producer_id)
                        .map(|p| !p.is_closed.load(Ordering::SeqCst))
                        .unwrap_or(false)
                },
                || async {
                    // Resolve transport (WebRtc or Plain), check router ownership, and DROP guard before .await
                    let (transport, _transport_peer_id, physical_router_id, worker_index) =
                        self.resolve_media_transport(&params.router_id, &params.transport_id)?;

                    let ms_kind = to_mediasoup_kind(params.kind);

                    tracing::debug!(
                        router_id = %params.router_id,
                        transport_id = %params.transport_id,
                        peer_id = %params.peer_id,
                        kind = %params.kind,
                        encodings = ?params.rtp_parameters.encodings,
                        codecs = ?params.rtp_parameters.codecs,
                        header_extensions = ?params.rtp_parameters.header_extensions,
                        "Starting transport.produce with sanitized RTP parameters"
                    );

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

                    let transport_active =
                        transport.is_active_in_state(&self.state, &params.transport_id);
                    if !transport_active {
                        drop(producer);
                        return Err(SfuDomainError::NotFound(format!(
                            "Transport '{}' was closed during produce",
                            params.transport_id
                        )));
                    }

                    let producer_id = producer.id().to_string();
                    let is_closed = Arc::new(AtomicBool::new(false));

                    attach_producer_score_listener(
                        &producer,
                        &is_closed,
                        &self.state,
                        &self.event_publisher,
                        &params.router_id,
                        &producer_id,
                        &params.peer_id,
                    );

                    let encodings = extract_simulcast_encodings(producer.rtp_parameters());

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

                    // Verify router and transport are still active after all .await points
                    if !self.state.is_router_active(&params.router_id) {
                        drop(producer);
                        return Err(SfuDomainError::NotFound(format!(
                            "Router '{}' was closed during produce",
                            params.router_id
                        )));
                    }

                    let transport_active =
                        transport.is_active_in_state(&self.state, &params.transport_id);
                    if !transport_active {
                        drop(producer);
                        return Err(SfuDomainError::NotFound(format!(
                            "Transport '{}' was closed during produce",
                            params.transport_id
                        )));
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
                        .entry(producer_id.clone())
                        .or_default();
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

    #[tracing::instrument(
        skip(self, params),
        fields(
            room_id = tracing::field::Empty,
            router_id = %params.router_id,
            transport_id = %params.subscriber_transport_id,
            producer_id = %params.producer_id,
            peer_id = tracing::field::Empty,
            request_id = %params.request_id
        )
    )]
    async fn consume(&self, params: ConsumeParams) -> Result<ConsumeResult, SfuDomainError> {
        if let Some(room_id) = self.state.get_room_id_for_router(&params.router_id) {
            tracing::Span::current().record("room_id", room_id.as_str());
        }
        if self.state.is_draining.load(Ordering::SeqCst) {
            return Err(SfuDomainError::SystemDraining(
                "Node is draining; rejecting new consumer".to_string(),
            ));
        }

        validation::validate_not_empty("request_id", &params.request_id)?;
        validation::validate_not_empty("router_id", &params.router_id)?;
        validation::validate_not_empty("subscriber_transport_id", &params.subscriber_transport_id)?;
        validation::validate_not_empty("producer_id", &params.producer_id)?;

        let paused_bytes = if params.paused { b"1" } else { b"0" };
        let payload_parts: [&[u8]; 6] = [
            params.request_id.as_bytes(),
            params.router_id.as_bytes(),
            params.subscriber_transport_id.as_bytes(),
            params.producer_id.as_bytes(),
            params.raw_rtp_capabilities_json.as_bytes(),
            paused_bytes,
        ];

        let metrics_ref = self.metrics.clone();
        let state_for_valid = self.state.clone();

        self.consume_cache
            .execute_or_get_validated_parts(
                &params.request_id,
                &payload_parts,
                move |cached: &ConsumeResult| {
                    state_for_valid
                        .consumers
                        .get(&cached.consumer_id)
                        .map(|c| !c.is_closed.load(Ordering::SeqCst))
                        .unwrap_or(false)
                },
                || async {
                    let (
                        producer_physical_router_id,
                        producer_worker_index,
                        producer_kind,
                        producer_encodings_count,
                    ) = {
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

                        (
                            p_item.physical_router_id.clone(),
                            p_item.worker_index,
                            p_item.kind,
                            p_item.producer.rtp_parameters().encodings.len(),
                        )
                    };

                    let (transport, peer_id, sub_physical_router_id, sub_worker_index) =
                        self.resolve_media_transport(&params.router_id, &params.subscriber_transport_id)?;
                    tracing::Span::current().record("peer_id", peer_id.as_str());

                    // Enforce max consumers per worker license and config quota
                    let licensed_consumers = self.state.get_active_license().and_then(|lic| {
                        let limit = lic.max_consumers_per_worker() as usize;
                        if limit > 0 {
                            Some(limit)
                        } else {
                            None
                        }
                    });
                    let effective_max_consumers = match (licensed_consumers, self.config.max_consumers_per_worker) {
                        (Some(l), Some(c)) => Some(std::cmp::min(l, c)),
                        (Some(l), None) => Some(l),
                        (None, Some(c)) => Some(c),
                        (None, None) => None,
                    };

                    if let Some(max_c) = effective_max_consumers {
                        let current_consumers_on_worker = self.state.get_worker_consumers(sub_worker_index);

                        if current_consumers_on_worker >= max_c {
                            return Err(SfuDomainError::ResourceExhausted(format!(
                                "Worker {sub_worker_index} reached maximum consumer limit ({max_c})"
                            )));
                        }
                    }

                    // Protect pipe against premature removal by concurrent requests (Race Condition Fix)
                    let mut in_flight_pipe_counter = None;

                    if producer_physical_router_id != sub_physical_router_id {
                        let singleflight_key = format!(
                            "{}:{}:{}",
                            params.router_id, params.producer_id, sub_physical_router_id
                        );
                        let lock = self
                            .state
                            .pipe_singleflight
                            .entry(singleflight_key.clone())
                            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                            .clone();

                        let _key_guard = crate::application::state::SingleflightKeyGuard::new(
                            self.state.pipe_singleflight.clone(),
                            singleflight_key.clone(),
                        );
                        let _guard = lock.lock().await;

                        let existing_counter = self
                            .state
                            .piped_producers
                            .get(&params.producer_id)
                            .and_then(|entries| {
                                entries
                                    .iter()
                                    .find(|e| {
                                        e.router_id == params.router_id
                                            && e.target_router_id == sub_physical_router_id
                                    })
                                    .map(|e| {
                                        e.in_flight_consumes
                                            .fetch_add(1, Ordering::SeqCst);
                                        e.in_flight_consumes.clone()
                                    })
                            });

                        if let Some(counter) = existing_counter {
                            in_flight_pipe_counter = Some(counter);
                        } else {
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

                            if producer_kind == MediaKind::Video {
                                let pipe_consumer_clone = pipe_pair.pipe_consumer.clone();
                                tokio::spawn(async move {
                                    let _ = pipe_consumer_clone.request_key_frame().await;
                                });
                            }

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

                            let counter = Arc::new(std::sync::atomic::AtomicUsize::new(1));
                            in_flight_pipe_counter = Some(counter.clone());

                            let entry = PipedProducerEntry {
                                router_id: params.router_id.clone(),
                                pipe_consumer: pipe_pair.pipe_consumer,
                                pipe_producer,
                                source_worker_index: producer_worker_index,
                                target_router_id: sub_physical_router_id.clone(),
                                target_worker_index: sub_worker_index,
                                in_flight_consumes: counter,
                            };

                            self.state
                                .piped_producers
                                .entry(params.producer_id.clone())
                                .or_default()
                                .push(entry);
                        }
                    }

                    let _in_flight_guard =
                        crate::application::state::PipedConsumeInFlightGuard::new(
                            in_flight_pipe_counter,
                        );

                    let target_router = {
                        let r_item = match self.state.routers.get(&params.router_id) {
                            Some(r) if !r.is_closed.load(Ordering::SeqCst) => r,
                            _ => {
                                drop(_in_flight_guard);
                                self.rollback_unused_pipe(
                                    &params.router_id,
                                    &params.producer_id,
                                    &sub_physical_router_id,
                                );
                                return Err(SfuDomainError::NotFound(format!(
                                    "Router '{}' not found or closed",
                                    params.router_id
                                )));
                            }
                        };

                        if sub_worker_index == r_item.worker_index {
                            r_item.router.clone()
                        } else {
                            let sub_r = r_item
                                .sub_routers
                                .get(&sub_worker_index)
                                .map(|s| s.router.clone());
                            match sub_r {
                                Some(r) => r,
                                None => {
                                    drop(r_item);
                                    drop(_in_flight_guard);
                                    self.rollback_unused_pipe(
                                        &params.router_id,
                                        &params.producer_id,
                                        &sub_physical_router_id,
                                    );
                                    return Err(SfuDomainError::NotFound(format!(
                                        "Sub-router on worker {sub_worker_index} not found"
                                    )));
                                }
                            }
                        }
                    };

                    let producer_id: ProducerId = match params.producer_id.parse() {
                        Ok(id) => id,
                        Err(e) => {
                            drop(_in_flight_guard);
                            self.rollback_unused_pipe(
                                &params.router_id,
                                &params.producer_id,
                                &sub_physical_router_id,
                            );
                            return Err(SfuDomainError::InvalidArgument(format!(
                                "Invalid producer_id: {e}"
                            )));
                        }
                    };

                    if !target_router.can_consume(&producer_id, &params.rtp_capabilities) {
                        drop(_in_flight_guard);
                        self.rollback_unused_pipe(
                            &params.router_id,
                            &params.producer_id,
                            &sub_physical_router_id,
                        );
                        return Err(SfuDomainError::IncompatibleCodec(format!(
                            "Router cannot consume producer '{}' with given RTP capabilities",
                            params.producer_id
                        )));
                    }

                    let mut options = ConsumerOptions::new(producer_id, params.rtp_capabilities);
                    options.paused = params.paused;

                    let video_preferred_layers = if producer_kind == MediaKind::Video {
                        let spatial_layer = self
                            .config
                            .default_video_preferred_spatial_layer
                            .unwrap_or_else(|| {
                                resolve_default_video_preferred_spatial_layer(
                                    producer_encodings_count,
                                )
                            });
                        let layers = ConsumerLayers {
                            spatial_layer,
                            temporal_layer: Some(2),
                        };
                        options.preferred_layers = Some(layers);
                        Some(layers)
                    } else {
                        None
                    };

                    let consumer = match transport.consume(options).await {
                        Ok(c) => c,
                        Err(e) => {
                            drop(_in_flight_guard);
                            self.rollback_unused_pipe(
                                &params.router_id,
                                &params.producer_id,
                                &sub_physical_router_id,
                            );
                            return Err(SfuDomainError::Mediasoup(format!(
                                "Failed to consume: {e}"
                            )));
                        }
                    };

                    if producer_kind == MediaKind::Audio {
                        let _ = consumer.set_priority(255).await;
                    } else if producer_encodings_count == 1 {
                        let _ = consumer.set_priority(192).await;
                    } else {
                        let _ = consumer.set_priority(128).await;
                    }

                    let consumer_id = consumer.id().to_string();
                    let is_closed = Arc::new(AtomicBool::new(false));

                    let event_publisher = self.event_publisher.clone();
                    let r_id = params.router_id.clone();
                    let c_id = consumer_id.clone();
                    let p_id_str = params.producer_id.clone();
                    let peer_id_clone = peer_id.clone();
                    let state_ref = self.state.clone();
                    let closed_ref = is_closed.clone();

                    attach_consumer_score_listener(
                        &consumer,
                        &is_closed,
                        &self.state,
                        &self.event_publisher,
                        &params.router_id,
                        &consumer_id,
                        &peer_id,
                    );

                    // Issue 1.1 Fix: Use WeakConsumer to prevent Arc reference cycle and memory leak
                    let weak_consumer = consumer.downgrade();
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

                                let pref = weak_consumer
                                    .upgrade()
                                    .and_then(|c| c.preferred_layers());
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

                                if let Some(l) = layers {
                                    let transitions = state_ref.dynacast_tracker.update_consumer(
                                        &c_id,
                                        &p_id,
                                        Some(l.spatial_layer as i32),
                                        false,
                                    );
                                    for t in transitions {
                                        event_publisher.publish_dynacast_layer(
                                            &r_id,
                                            &p_id,
                                            t.spatial_layer,
                                            t.is_active,
                                            t.active_consumers,
                                        );
                                    }
                                }
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

                    let encodings = extract_simulcast_encodings(rtp_params);

                    let current_layers = consumer.current_layers();
                    let preferred_layers = consumer.preferred_layers();
                    let initial_spatial_layer = current_layers
                        .map(|l| l.spatial_layer as i32)
                        .or_else(|| preferred_layers.map(|l| l.spatial_layer as i32))
                        .or_else(|| video_preferred_layers.map(|l| l.spatial_layer as i32))
                        .unwrap_or(-1);
                    let initial_temporal_layer = current_layers
                        .and_then(|l| l.temporal_layer)
                        .map(|t| t as i32)
                        .or_else(|| preferred_layers.and_then(|l| l.temporal_layer).map(|t| t as i32))
                        .or_else(|| video_preferred_layers.and_then(|l| l.temporal_layer).map(|t| t as i32))
                        .unwrap_or(-1);

                    let consumer_kind: MediaKind = from_mediasoup_kind(consumer.kind());

                    // Issue 3.1: Keyframe request and retry loop only for non-paused video consumer
                    if !params.paused && consumer.kind() == mediasoup::prelude::MediaKind::Video {
                        if let Err(e) = consumer.request_key_frame().await {
                            tracing::warn!(
                                consumer_id = %consumer_id,
                                error = %e,
                                "Failed to request initial keyframe for video consumer"
                            );
                        } else {
                            tracing::debug!(
                                consumer_id = %consumer_id,
                                "Requested initial keyframe for video consumer"
                            );
                        }

                        let retry_weak_consumer = consumer.downgrade();
                        let retry_consumer_id = consumer_id.clone();
                        let retry_is_closed = is_closed.clone();
                        let retry_state = self.state.clone();
                        let retry_router_id = params.router_id.clone();
                        let retry_attempts: usize = self.config.keyframe_retry_count;

                        tokio::spawn(async move {
                            for attempt in 1..=retry_attempts {
                                tokio::time::sleep(Duration::from_millis(1500)).await;

                                let Some(retry_consumer) = retry_weak_consumer.upgrade() else {
                                    break;
                                };

                                if retry_consumer.closed()
                                    || retry_consumer.paused()
                                    || retry_is_closed.load(Ordering::SeqCst)
                                    || !retry_state.is_router_active(&retry_router_id)
                                {
                                    break;
                                }

                                if let Some(current) = retry_consumer.current_layers() {
                                    tracing::debug!(
                                        consumer_id = %retry_consumer_id,
                                        spatial_layer = current.spatial_layer,
                                        temporal_layer = ?current.temporal_layer,
                                        "Video consumer layers promoted; stopping keyframe retry loop"
                                    );
                                    break;
                                }

                                tracing::debug!(
                                    consumer_id = %retry_consumer_id,
                                    attempt = attempt,
                                    max_attempts = retry_attempts,
                                    "Retrying keyframe request for pending video consumer"
                                );
                                let _ = retry_consumer.request_key_frame().await;
                            }
                        });
                    }

                    if !self.state.is_router_active(&params.router_id) {
                        drop(consumer);
                        drop(_in_flight_guard);
                        self.rollback_unused_pipe(
                            &params.router_id,
                            &params.producer_id,
                            &sub_physical_router_id,
                        );
                        return Err(SfuDomainError::NotFound(format!(
                            "Router '{}' was closed during consume",
                            params.router_id
                        )));
                    }

                    let transport_active =
                        transport.is_active_in_state(&self.state, &params.subscriber_transport_id);
                    if !transport_active {
                        drop(consumer);
                        drop(_in_flight_guard);
                        self.rollback_unused_pipe(
                            &params.router_id,
                            &params.producer_id,
                            &sub_physical_router_id,
                        );
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
                        drop(_in_flight_guard);
                        self.rollback_unused_pipe(
                            &params.router_id,
                            &params.producer_id,
                            &sub_physical_router_id,
                        );
                        return Err(SfuDomainError::NotFound(format!(
                            "Producer '{}' was closed during consume",
                            params.producer_id
                        )));
                    }

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
                    self.state.inc_worker_consumers(sub_worker_index);
                    if let Some(set) = self
                        .state
                        .transport_consumers
                        .get(&params.subscriber_transport_id)
                    {
                        set.insert(consumer_id.clone());
                    }
                    self.state
                        .producer_consumers
                        .entry(params.producer_id.clone())
                        .or_default()
                        .insert(consumer_id.clone());

                    // Drop in-flight guard safely after registration in producer_consumers
                    drop(_in_flight_guard);

                    let c_weight = if consumer_kind == MediaKind::Audio { 1 } else { 2 };
                    self.worker_pool.add_resource_score(sub_worker_index, c_weight);
                    self.metrics
                        .inc_active_consumers(&consumer_kind.to_string());

                    if consumer_kind == MediaKind::Video {
                        let initial_spatial = initial_spatial_layer.max(0);
                        let transitions = self.state.dynacast_tracker.update_consumer(
                            &consumer_id,
                            &params.producer_id,
                            Some(initial_spatial),
                            params.paused,
                        );
                        for t in transitions {
                            self.event_publisher.publish_dynacast_layer(
                                &params.router_id,
                                &params.producer_id,
                                t.spatial_layer,
                                t.is_active,
                                t.active_consumers,
                            );
                        }
                    }

                    Ok(ConsumeResult {
                        consumer_id,
                        producer_id: params.producer_id.clone(),
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

    #[tracing::instrument(
        skip(self),
        fields(
            room_id = tracing::field::Empty,
            router_id = %router_id,
            resource_id = %resource_id,
            peer_id = tracing::field::Empty
        )
    )]
    async fn set_media_paused(
        &self,
        router_id: &str,
        resource_id: &str,
        is_producer: bool,
        paused: bool,
    ) -> Result<(), SfuDomainError> {
        if let Some(room_id) = self.state.get_room_id_for_router(router_id) {
            tracing::Span::current().record("room_id", room_id.as_str());
        }
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("resource_id", resource_id)?;

        if is_producer {
            // Extract producer and DROP guard before .await
            let (producer, peer_id) = {
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

                (p_item.producer.clone(), p_item.peer_id.clone())
            };
            tracing::Span::current().record("peer_id", peer_id.as_str());

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
                .map(|entries| {
                    entries
                        .iter()
                        .filter(|e| e.router_id == router_id)
                        .map(|e| e.pipe_producer.clone())
                        .collect()
                })
                .unwrap_or_default();

            for p in piped_to_pause {
                if paused {
                    let _ = p.pause().await;
                } else {
                    let _ = p.resume().await;
                }
            }

            // When a video producer is resumed (unmuted), request a keyframe for all active consumers
            if !paused && producer.kind() == mediasoup::prelude::MediaKind::Video {
                let consumer_ids: Vec<String> = self
                    .state
                    .producer_consumers
                    .get(resource_id)
                    .map(|set| set.iter().map(|entry| entry.key().clone()).collect())
                    .unwrap_or_default();
                for cid in consumer_ids {
                    if let Some(c_item) = self.state.consumers.get(&cid) {
                        if !c_item.is_closed.load(Ordering::SeqCst) && !c_item.consumer.paused() {
                            let c_clone = c_item.consumer.clone();
                            tokio::spawn(async move {
                                let _ = c_clone.request_key_frame().await;
                            });
                        }
                    }
                }
            }
        } else {
            // Extract consumer and DROP guard before .await
            let (consumer, peer_id, producer_id, c_kind) = {
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

                (
                    c_item.consumer.clone(),
                    c_item.peer_id.clone(),
                    c_item.producer_id.clone(),
                    c_item.kind,
                )
            };
            tracing::Span::current().record("peer_id", peer_id.as_str());

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

                // Immediate RTCP PLI/FIR keyframe dispatch when resuming a video consumer.
                // In standard ORTC workflows (such as mediasoup-client or livekit-client),
                // consumers are created paused (paused: true) until local WebRTC transceivers
                // and media elements are ready, and then resumed. Without an explicit keyframe
                // request on resume, the subscriber receives only delta frames (P-frames) that
                // cannot be decoded, causing video playback to stall or freeze for up to 30 seconds.
                if consumer.kind() == mediasoup::prelude::MediaKind::Video {
                    if let Err(e) = consumer.request_key_frame().await {
                        tracing::warn!(
                            consumer_id = %resource_id,
                            error = %e,
                            "Failed to request keyframe on video consumer resume"
                        );
                    } else {
                        tracing::debug!(
                            consumer_id = %resource_id,
                            "Requested keyframe on video consumer resume"
                        );
                    }
                }
            }

            if c_kind == MediaKind::Video {
                let current_spatial = consumer
                    .current_layers()
                    .map(|l| l.spatial_layer as i32)
                    .or_else(|| consumer.preferred_layers().map(|l| l.spatial_layer as i32))
                    .unwrap_or(0);
                let transitions = self.state.dynacast_tracker.update_consumer(
                    resource_id,
                    &producer_id,
                    Some(current_spatial),
                    paused,
                );
                for t in transitions {
                    self.event_publisher.publish_dynacast_layer(
                        router_id,
                        &producer_id,
                        t.spatial_layer,
                        t.is_active,
                        t.active_consumers,
                    );
                }
            }
        }

        Ok(())
    }

    #[tracing::instrument(
        skip(self),
        fields(
            room_id = tracing::field::Empty,
            router_id = %router_id,
            consumer_id = %consumer_id,
            peer_id = tracing::field::Empty
        )
    )]
    async fn set_consumer_preferred_layers(
        &self,
        router_id: &str,
        consumer_id: &str,
        layers: ConsumerLayersInfo,
    ) -> Result<(), SfuDomainError> {
        if let Some(room_id) = self.state.get_room_id_for_router(router_id) {
            tracing::Span::current().record("room_id", room_id.as_str());
        }
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("consumer_id", consumer_id)?;

        // Extract consumer and DROP guard before .await
        let (consumer, peer_id, producer_id, c_kind) = {
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

            (
                c_item.consumer.clone(),
                c_item.peer_id.clone(),
                c_item.producer_id.clone(),
                c_item.kind,
            )
        };
        tracing::Span::current().record("peer_id", peer_id.as_str());

        consumer
            .set_preferred_layers(ConsumerLayers {
                spatial_layer: layers.spatial_layer,
                temporal_layer: layers.temporal_layer,
            })
            .await
            .map_err(|e| {
                SfuDomainError::Mediasoup(format!("Failed to set preferred layers: {e}"))
            })?;

        // Immediately request a keyframe so that layer transition produces an Intra-frame without stalling
        if consumer.kind() == mediasoup::prelude::MediaKind::Video {
            if let Err(e) = consumer.request_key_frame().await {
                tracing::debug!(
                    consumer_id = %consumer_id,
                    error = %e,
                    "Failed to request keyframe after layer update"
                );
            }
        }

        if c_kind == MediaKind::Video {
            let is_paused = consumer.paused();
            let transitions = self.state.dynacast_tracker.update_consumer(
                consumer_id,
                &producer_id,
                Some(layers.spatial_layer as i32),
                is_paused,
            );
            for t in transitions {
                self.event_publisher.publish_dynacast_layer(
                    router_id,
                    &producer_id,
                    t.spatial_layer,
                    t.is_active,
                    t.active_consumers,
                );
            }
        }

        Ok(())
    }

    #[tracing::instrument(
        skip(self),
        fields(
            room_id = tracing::field::Empty,
            router_id = %router_id,
            consumer_id = %consumer_id,
        )
    )]
    async fn request_key_frame(
        &self,
        router_id: &str,
        consumer_id: &str,
    ) -> Result<(), SfuDomainError> {
        if let Some(room_id) = self.state.get_room_id_for_router(router_id) {
            tracing::Span::current().record("room_id", room_id.as_str());
        }
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("consumer_id", consumer_id)?;

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

            if c_item.kind != MediaKind::Video {
                return Err(SfuDomainError::InvalidArgument(format!(
                    "Consumer '{consumer_id}' is not a video consumer"
                )));
            }

            c_item.consumer.clone()
        };

        consumer
            .request_key_frame()
            .await
            .map_err(|e| SfuDomainError::Mediasoup(format!("Failed to request keyframe: {e}")))?;

        Ok(())
    }

    #[tracing::instrument(
        skip(self),
        fields(
            room_id = tracing::field::Empty,
            router_id = %router_id,
            producer_id = %producer_id,
            peer_id = tracing::field::Empty
        )
    )]
    fn close_producer(&self, router_id: &str, producer_id: &str) -> Result<(), SfuDomainError> {
        if let Some(room_id) = self.state.get_room_id_for_router(router_id) {
            tracing::Span::current().record("room_id", room_id.as_str());
        }
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("producer_id", producer_id)?;

        if let Some((_, item)) = self
            .state
            .producers
            .remove(&(router_id.to_string(), producer_id.to_string()))
        {
            tracing::Span::current().record("peer_id", item.peer_id.as_str());
            item.is_closed.store(true, Ordering::SeqCst);
            let p_weight = if item.kind == MediaKind::Audio { 2 } else { 10 };
            self.worker_pool
                .sub_resource_score(item.worker_index, p_weight);
            self.metrics.dec_active_producers(&item.kind.to_string());

            if let Some(set) = self.state.transport_producers.get(&item.transport_id) {
                set.remove(producer_id);
            }

            let piped_entries = self
                .state
                .take_piped_producers_for_router(router_id, producer_id);
            for entry in piped_entries {
                self.worker_pool.sub_resource_score(item.worker_index, 1);
                self.worker_pool
                    .sub_resource_score(entry.target_worker_index, 1);
                // Dropping entry closes both pipe_consumer and pipe_producer via mediasoup RAII
            }

            self.state.dynacast_tracker.remove_producer(producer_id);

            self.event_publisher.publish_resource_closed(
                router_id,
                "producer",
                producer_id,
                "producer_closed",
            );

            // Cascade close all consumers connected to this producer on this router
            let c_ids = self
                .state
                .take_consumers_for_producer(router_id, producer_id);
            for cid in c_ids {
                self.state.dynacast_tracker.remove_consumer(&cid);
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

        Ok(())
    }

    #[tracing::instrument(
        skip(self),
        fields(
            room_id = tracing::field::Empty,
            router_id = %router_id,
            consumer_id = %consumer_id,
            peer_id = tracing::field::Empty
        )
    )]
    fn close_consumer(&self, router_id: &str, consumer_id: &str) -> Result<(), SfuDomainError> {
        if let Some(room_id) = self.state.get_room_id_for_router(router_id) {
            tracing::Span::current().record("room_id", room_id.as_str());
        }
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("consumer_id", consumer_id)?;

        let router_matches = self
            .state
            .consumers
            .get(consumer_id)
            .map(|c| (c.router_id == router_id, c.peer_id.clone()));

        match router_matches {
            Some((true, peer_id)) => {
                tracing::Span::current().record("peer_id", peer_id.as_str());
                if let Some(item) = self.state.cascade_remove_consumer(consumer_id) {
                    if item.kind == MediaKind::Video {
                        let transitions = self.state.dynacast_tracker.remove_consumer(consumer_id);
                        for t in transitions {
                            self.event_publisher.publish_dynacast_layer(
                                router_id,
                                &item.producer_id,
                                t.spatial_layer,
                                t.is_active,
                                t.active_consumers,
                            );
                        }
                    }

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
            Some((false, _)) => Err(SfuDomainError::NotFound(format!(
                "Consumer '{consumer_id}' does not belong to router '{router_id}'"
            ))),
            None => Ok(()),
        }
    }

    #[tracing::instrument(
        skip(self),
        fields(
            room_id = tracing::field::Empty,
            router_id = %router_id,
            resource_id = %resource_id,
            peer_id = tracing::field::Empty
        )
    )]
    async fn get_stats(
        &self,
        router_id: &str,
        resource_id: &str,
        resource_type: &str,
    ) -> Result<ResourceStatsInfo, SfuDomainError> {
        if let Some(room_id) = self.state.get_room_id_for_router(router_id) {
            tracing::Span::current().record("room_id", room_id.as_str());
        }
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("resource_id", resource_id)?;

        match resource_type.to_lowercase().as_str() {
            "transport" => {
                enum TargetTransport {
                    WebRtc(mediasoup::webrtc_transport::WebRtcTransport),
                    Plain(mediasoup::plain_transport::PlainTransport),
                    Pipe(mediasoup::pipe_transport::PipeTransport),
                }

                impl TargetTransport {
                    async fn fetch_stats_summary(
                        &self,
                    ) -> Result<(String, u64, u64), SfuDomainError> {
                        match self {
                            Self::WebRtc(transport) => {
                                let stats = transport
                                    .get_stats()
                                    .await
                                    .map_err(|e| SfuDomainError::Mediasoup(e.to_string()))?;
                                let stats_json = serde_json::to_string(&stats)
                                    .map_err(|e| SfuDomainError::Internal(e.to_string()))?;
                                let bytes_sent = stats.iter().map(|s| s.bytes_sent).sum();
                                let bytes_received = stats.iter().map(|s| s.bytes_received).sum();
                                Ok((stats_json, bytes_sent, bytes_received))
                            }
                            Self::Plain(transport) => {
                                let stats = transport
                                    .get_stats()
                                    .await
                                    .map_err(|e| SfuDomainError::Mediasoup(e.to_string()))?;
                                let stats_json = serde_json::to_string(&stats)
                                    .map_err(|e| SfuDomainError::Internal(e.to_string()))?;
                                let bytes_sent = stats.iter().map(|s| s.bytes_sent).sum();
                                let bytes_received = stats.iter().map(|s| s.bytes_received).sum();
                                Ok((stats_json, bytes_sent, bytes_received))
                            }
                            Self::Pipe(transport) => {
                                let stats = transport
                                    .get_stats()
                                    .await
                                    .map_err(|e| SfuDomainError::Mediasoup(e.to_string()))?;
                                let stats_json = serde_json::to_string(&stats)
                                    .map_err(|e| SfuDomainError::Internal(e.to_string()))?;
                                let bytes_sent = stats.iter().map(|s| s.bytes_sent).sum();
                                let bytes_received = stats.iter().map(|s| s.bytes_received).sum();
                                Ok((stats_json, bytes_sent, bytes_received))
                            }
                        }
                    }
                }

                let (target_transport, peer_id) = {
                    if let Some(t_item) = self.state.transports.get(resource_id) {
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

                        (
                            TargetTransport::WebRtc(t_item.transport.clone()),
                            t_item.peer_id.clone(),
                        )
                    } else if let Some(pt_item) = self.state.plain_transports.get(resource_id) {
                        if pt_item.is_closed.load(Ordering::SeqCst) {
                            return Err(SfuDomainError::NotFound(format!(
                                "Transport '{resource_id}' is closed"
                            )));
                        }

                        if pt_item.router_id != router_id {
                            return Err(SfuDomainError::NotFound(format!(
                                "Transport '{resource_id}' does not belong to router '{router_id}'"
                            )));
                        }

                        (
                            TargetTransport::Plain(pt_item.transport.clone()),
                            "plain-transport".to_string(),
                        )
                    } else if let Some(p_item) = self.state.pipe_transports.get(resource_id) {
                        if p_item.is_closed.load(Ordering::SeqCst) {
                            return Err(SfuDomainError::NotFound(format!(
                                "Transport '{resource_id}' is closed"
                            )));
                        }

                        if p_item.router_id != router_id {
                            return Err(SfuDomainError::NotFound(format!(
                                "Transport '{resource_id}' does not belong to router '{router_id}'"
                            )));
                        }

                        (
                            TargetTransport::Pipe(p_item.transport.clone()),
                            "pipe-transport".to_string(),
                        )
                    } else {
                        return Err(SfuDomainError::NotFound(format!(
                            "Transport '{resource_id}' not found"
                        )));
                    }
                };
                tracing::Span::current().record("peer_id", peer_id.as_str());

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

                let (transport_res, p_stats_list, c_stats_list) = tokio::join!(
                    target_transport.fetch_stats_summary(),
                    futures_util::future::join_all(producer_futs),
                    futures_util::future::join_all(consumer_futs),
                );
                let (stats_json, bytes_sent, bytes_received) = transport_res?;

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
                let (producer, peer_id) = {
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

                    (p_item.producer.clone(), p_item.peer_id.clone())
                };
                tracing::Span::current().record("peer_id", peer_id.as_str());

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
                let (consumer, current_layers, peer_id) = {
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

                    (
                        c_item.consumer.clone(),
                        c_item.consumer.current_layers(),
                        c_item.peer_id.clone(),
                    )
                };
                tracing::Span::current().record("peer_id", peer_id.as_str());

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
