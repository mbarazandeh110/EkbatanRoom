use crate::application::state::{
    ConsumerItem, EngineState, PipeTransportItem, PipedProducerEntry, ProducerItem,
};
use crate::config::Config;
use crate::domain::entities::{MediaKind as DomainMediaKind, SimulcastEncodingInfo};
use crate::domain::errors::SfuDomainError;
use crate::domain::idempotency::IdempotencyCache;
use crate::domain::validation;
use crate::ports::inbound::*;
use crate::ports::outbound::{EventPublisherPort, MetricsPort, WorkerPoolPort};
use async_trait::async_trait;
use mediasoup::pipe_transport::{PipeTransportOptions, PipeTransportRemoteParameters};
use mediasoup::prelude::*;
use mediasoup::router::PipeToRouterOptions;
use mediasoup::transport::Transport;
use mediasoup::types::scalability_modes::ScalabilityMode;
use mediasoup::types::srtp_parameters::SrtpParameters;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct CommunityDisabledPipeService;

#[async_trait]
impl InterNodePipeUseCase for CommunityDisabledPipeService {
    async fn create_pipe_transport(
        &self,
        _params: CreatePipeTransportParams,
    ) -> Result<CreatePipeTransportResult, SfuDomainError> {
        Err(SfuDomainError::EnterpriseFeatureRequired(
            "Inter-node room cascading is an Enterprise feature".to_string(),
        ))
    }

    async fn connect_pipe_transport(
        &self,
        _params: ConnectPipeTransportParams,
    ) -> Result<(), SfuDomainError> {
        Err(SfuDomainError::EnterpriseFeatureRequired(
            "Inter-node room cascading is an Enterprise feature".to_string(),
        ))
    }

    async fn pipe_produce(
        &self,
        _params: PipeProduceParams,
    ) -> Result<ProduceResult, SfuDomainError> {
        Err(SfuDomainError::EnterpriseFeatureRequired(
            "Inter-node room cascading is an Enterprise feature".to_string(),
        ))
    }

    async fn pipe_consume(
        &self,
        _params: PipeConsumeParams,
    ) -> Result<ConsumeResult, SfuDomainError> {
        Err(SfuDomainError::EnterpriseFeatureRequired(
            "Inter-node room cascading is an Enterprise feature".to_string(),
        ))
    }

    fn is_enterprise_enabled(&self) -> bool {
        false
    }
}

pub struct EnterprisePipeService {
    config: Config,
    state: EngineState,
    metrics: Arc<dyn MetricsPort>,
    worker_pool: Arc<dyn WorkerPoolPort>,
    event_publisher: Arc<dyn EventPublisherPort>,
    create_pipe_transport_cache: Arc<IdempotencyCache<CreatePipeTransportResult>>,
    pipe_produce_cache: Arc<IdempotencyCache<ProduceResult>>,
    pipe_consume_cache: Arc<IdempotencyCache<ConsumeResult>>,
    cleanup_handles: Vec<tokio::task::AbortHandle>,
}

impl EnterprisePipeService {
    pub fn new(
        config: Config,
        state: EngineState,
        metrics: Arc<dyn MetricsPort>,
        worker_pool: Arc<dyn WorkerPoolPort>,
        event_publisher: Arc<dyn EventPublisherPort>,
    ) -> Self {
        let create_pipe_transport_cache = Arc::new(IdempotencyCache::default());
        let pipe_produce_cache = Arc::new(IdempotencyCache::default());
        let pipe_consume_cache = Arc::new(IdempotencyCache::default());

        let c1 = create_pipe_transport_cache.start_periodic_cleanup(Duration::from_secs(30));
        let c2 = pipe_produce_cache.start_periodic_cleanup(Duration::from_secs(30));
        let c3 = pipe_consume_cache.start_periodic_cleanup(Duration::from_secs(30));

        Self {
            config,
            state,
            metrics,
            worker_pool,
            event_publisher,
            create_pipe_transport_cache,
            pipe_produce_cache,
            pipe_consume_cache,
            cleanup_handles: vec![c1.abort_handle(), c2.abort_handle(), c3.abort_handle()],
        }
    }

    fn check_enterprise_license(&self) -> Result<(), SfuDomainError> {
        if !self.state.is_enterprise_license_valid() {
            return Err(SfuDomainError::LicensePermissionDenied(
                "Valid Enterprise license is required for inter-node room cascading".to_string(),
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl InterNodePipeUseCase for EnterprisePipeService {
    async fn create_pipe_transport(
        &self,
        params: CreatePipeTransportParams,
    ) -> Result<CreatePipeTransportResult, SfuDomainError> {
        if self.state.is_draining.load(Ordering::SeqCst) {
            return Err(SfuDomainError::SystemDraining(
                "Node is draining; rejecting new pipe transport".to_string(),
            ));
        }

        self.check_enterprise_license()?;

        validation::validate_not_empty("request_id", &params.request_id)?;
        validation::validate_not_empty("router_id", &params.router_id)?;

        let payload_bytes = format!(
            "{}:{}:{}:{}",
            params.request_id, params.router_id, params.enable_srtp, params.enable_rtx
        )
        .into_bytes();

        let metrics_ref = self.metrics.clone();

        self.create_pipe_transport_cache
            .execute_or_get_with_hook(
                &params.request_id,
                &payload_bytes,
                || async {
                    let (router, worker_index) = {
                        let r_item =
                            self.state.routers.get(&params.router_id).ok_or_else(|| {
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

                        (r_item.router.clone(), r_item.worker_index)
                    };

                    let listen_info = ListenInfo {
                        protocol: Protocol::Udp,
                        ip: self.config.listen_ip,
                        announced_address: self.config.announced_ip.clone(),
                        expose_internal_ip: false,
                        port: None,
                        port_range: Some(self.config.rtc_min_port..=self.config.rtc_max_port),
                        flags: None,
                        send_buffer_size: self.config.webrtc_transport.send_buffer_size,
                        recv_buffer_size: self.config.webrtc_transport.recv_buffer_size,
                    };

                    let mut options = PipeTransportOptions::new(listen_info);
                    options.enable_srtp = params.enable_srtp;
                    options.enable_rtx = params.enable_rtx;

                    let transport = router.create_pipe_transport(options).await.map_err(|e| {
                        SfuDomainError::Mediasoup(format!("Failed to create PipeTransport: {e}"))
                    })?;

                    if !self.state.is_router_active(&params.router_id) {
                        drop(transport);
                        return Err(SfuDomainError::NotFound(format!(
                            "Router '{}' was closed during pipe transport creation",
                            params.router_id
                        )));
                    }

                    let pipe_transport_id = transport.id().to_string();
                    let tuple = transport.tuple();
                    let ip = tuple.local_address().to_string();
                    let port = tuple.local_port() as u32;
                    let srtp_parameters_json = transport
                        .srtp_parameters()
                        .and_then(|s| serde_json::to_string(&s).ok())
                        .unwrap_or_default();

                    let item = PipeTransportItem {
                        transport,
                        router_id: params.router_id.clone(),
                        worker_index,
                        is_closed: Arc::new(AtomicBool::new(false)),
                    };

                    self.state
                        .pipe_transports
                        .insert(pipe_transport_id.clone(), item);
                    self.state
                        .router_pipe_transports
                        .entry(params.router_id.clone())
                        .or_default()
                        .insert(pipe_transport_id.clone());

                    self.worker_pool.add_resource_score(worker_index, 1);
                    self.metrics.inc_active_transports(false);

                    Ok(CreatePipeTransportResult {
                        pipe_transport_id,
                        ip,
                        port,
                        srtp_parameters_json,
                    })
                },
                move || {
                    metrics_ref.record_idempotency_hit();
                },
            )
            .await
    }

    async fn connect_pipe_transport(
        &self,
        params: ConnectPipeTransportParams,
    ) -> Result<(), SfuDomainError> {
        self.check_enterprise_license()?;

        validation::validate_not_empty("router_id", &params.router_id)?;
        validation::validate_not_empty("pipe_transport_id", &params.pipe_transport_id)?;
        validation::validate_not_empty("remote_ip", &params.remote_ip)?;

        let transport = {
            let t_item = self
                .state
                .pipe_transports
                .get(&params.pipe_transport_id)
                .ok_or_else(|| {
                    SfuDomainError::NotFound(format!(
                        "PipeTransport '{}' not found",
                        params.pipe_transport_id
                    ))
                })?;

            if t_item.is_closed.load(Ordering::SeqCst) {
                return Err(SfuDomainError::NotFound(format!(
                    "PipeTransport '{}' is closed",
                    params.pipe_transport_id
                )));
            }

            if t_item.router_id != params.router_id {
                return Err(SfuDomainError::NotFound(format!(
                    "PipeTransport '{}' does not belong to router '{}'",
                    params.pipe_transport_id, params.router_id
                )));
            }

            t_item.transport.clone()
        };

        let remote_ip = IpAddr::from_str(&params.remote_ip).map_err(|e| {
            SfuDomainError::InvalidArgument(format!(
                "Invalid remote_ip '{}': {e}",
                params.remote_ip
            ))
        })?;

        let srtp_parameters = if !params.remote_srtp_parameters_json.trim().is_empty() {
            Some(
                serde_json::from_str::<SrtpParameters>(&params.remote_srtp_parameters_json)
                    .map_err(|e| {
                        SfuDomainError::InvalidArgument(format!(
                            "Invalid remote_srtp_parameters_json: {e}"
                        ))
                    })?,
            )
        } else {
            None
        };

        transport
            .connect(PipeTransportRemoteParameters {
                ip: remote_ip,
                port: params.remote_port as u16,
                srtp_parameters,
            })
            .await
            .map_err(|e| {
                SfuDomainError::Mediasoup(format!("Failed to connect PipeTransport: {e}"))
            })?;

        Ok(())
    }

    async fn pipe_produce(
        &self,
        params: PipeProduceParams,
    ) -> Result<ProduceResult, SfuDomainError> {
        if self.state.is_draining.load(Ordering::SeqCst) {
            return Err(SfuDomainError::SystemDraining(
                "Node is draining; rejecting pipe produce".to_string(),
            ));
        }

        self.check_enterprise_license()?;

        validation::validate_not_empty("request_id", &params.request_id)?;
        validation::validate_not_empty("router_id", &params.router_id)?;
        validation::validate_not_empty("pipe_transport_id", &params.pipe_transport_id)?;
        validation::validate_not_empty("producer_id", &params.producer_id)?;

        let payload_bytes = format!(
            "{}:{}:{}:{}:{}:{}:{}",
            params.request_id,
            params.router_id,
            params.pipe_transport_id,
            params.producer_id,
            params.kind,
            params.paused,
            params.raw_rtp_parameters_json
        )
        .into_bytes();

        let metrics_ref = self.metrics.clone();

        self.pipe_produce_cache
            .execute_or_get_with_hook(
                &params.request_id,
                &payload_bytes,
                || async {
                    let (pipe_transport, transport_worker_index) = {
                        let t_item = self
                            .state
                            .pipe_transports
                            .get(&params.pipe_transport_id)
                            .ok_or_else(|| {
                                SfuDomainError::NotFound(format!(
                                    "PipeTransport '{}' not found",
                                    params.pipe_transport_id
                                ))
                            })?;

                        if t_item.is_closed.load(Ordering::SeqCst) {
                            return Err(SfuDomainError::NotFound(format!(
                                "PipeTransport '{}' is closed",
                                params.pipe_transport_id
                            )));
                        }

                        if t_item.router_id != params.router_id {
                            return Err(SfuDomainError::NotFound(format!(
                                "PipeTransport '{}' does not belong to router '{}'",
                                params.pipe_transport_id, params.router_id
                            )));
                        }

                        (t_item.transport.clone(), t_item.worker_index)
                    };

                    let producer_id_obj: ProducerId = params.producer_id.parse().map_err(|e| {
                        SfuDomainError::InvalidArgument(format!(
                            "Invalid source producer_id '{}': {e}",
                            params.producer_id
                        ))
                    })?;

                    let ms_kind = match params.kind {
                        DomainMediaKind::Audio => mediasoup::prelude::MediaKind::Audio,
                        DomainMediaKind::Video => mediasoup::prelude::MediaKind::Video,
                    };

                    let mut producer_options = ProducerOptions::new_pipe_transport(
                        producer_id_obj,
                        ms_kind,
                        params.rtp_parameters.clone(),
                    );
                    producer_options.paused = params.paused;

                    let producer = pipe_transport
                        .produce(producer_options)
                        .await
                        .map_err(|e| {
                            SfuDomainError::Mediasoup(format!(
                                "Failed to produce on PipeTransport: {e}"
                            ))
                        })?;

                    if !self.state.is_router_active(&params.router_id) {
                        drop(producer);
                        return Err(SfuDomainError::NotFound(format!(
                            "Router '{}' was closed during pipe produce",
                            params.router_id
                        )));
                    }

                    let pt_active = self
                        .state
                        .pipe_transports
                        .get(&params.pipe_transport_id)
                        .map(|t| !t.is_closed.load(Ordering::SeqCst))
                        .unwrap_or(false);
                    if !pt_active {
                        drop(producer);
                        return Err(SfuDomainError::NotFound(format!(
                            "PipeTransport '{}' was closed during pipe produce",
                            params.pipe_transport_id
                        )));
                    }

                    let producer_id = producer.id().to_string();
                    let kind_str = match producer.kind() {
                        mediasoup::prelude::MediaKind::Audio => "audio",
                        mediasoup::prelude::MediaKind::Video => "video",
                    };

                    let encodings = producer
                        .rtp_parameters()
                        .encodings
                        .iter()
                        .map(|e| SimulcastEncodingInfo {
                            rid: e.rid.clone().unwrap_or_default(),
                            ssrc: e.ssrc.unwrap_or(0),
                            rtx_ssrc: e.rtx.as_ref().map(|r| r.ssrc).unwrap_or(0),
                            max_bitrate: e.max_bitrate.unwrap_or(0) as u32,
                            scalability_mode: if e.scalability_mode == ScalabilityMode::None {
                                String::new()
                            } else {
                                e.scalability_mode.to_string()
                            },
                        })
                        .collect();

                    let is_closed = Arc::new(AtomicBool::new(false));
                    let r_id = params.router_id.clone();
                    let p_id = producer_id.clone();
                    let peer_id = "inter-node-cascade".to_string();
                    let state_ref = self.state.clone();
                    let closed_ref = is_closed.clone();
                    let last_score = Arc::new(std::sync::atomic::AtomicU32::new(u32::MAX));
                    let event_publisher = self.event_publisher.clone();

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

                    if params.kind == DomainMediaKind::Audio {
                        let observer_opt =
                            self.state.routers.get(&params.router_id).and_then(|r| {
                                if transport_worker_index == r.worker_index {
                                    Some(r.audio_level_observer.clone())
                                } else {
                                    r.sub_routers
                                        .get(&transport_worker_index)
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
                                    worker_index = %transport_worker_index,
                                    error = %e,
                                    "Failed to add pipe audio producer to AudioLevelObserver"
                                );
                            }
                        }
                    }

                    let item = ProducerItem {
                        producer,
                        router_id: params.router_id.clone(),
                        physical_router_id: params.router_id.clone(),
                        worker_index: transport_worker_index,
                        transport_id: params.pipe_transport_id.clone(),
                        peer_id: "inter-node-cascade".to_string(),
                        kind: params.kind,
                        is_closed,
                    };

                    self.state
                        .producers
                        .insert((params.router_id.clone(), producer_id.clone()), item);
                    self.state
                        .transport_producers
                        .entry(params.pipe_transport_id.clone())
                        .or_default()
                        .insert(producer_id.clone());
                    self.state
                        .producer_consumers
                        .entry(producer_id.clone())
                        .or_default();

                    let p_weight = if params.kind == DomainMediaKind::Audio {
                        2
                    } else {
                        10
                    };
                    self.worker_pool
                        .add_resource_score(transport_worker_index, p_weight);

                    self.metrics.inc_active_producers(kind_str);

                    Ok(ProduceResult {
                        producer_id,
                        kind: kind_str.to_string(),
                        encodings,
                    })
                },
                move || {
                    metrics_ref.record_idempotency_hit();
                },
            )
            .await
    }

    async fn pipe_consume(
        &self,
        params: PipeConsumeParams,
    ) -> Result<ConsumeResult, SfuDomainError> {
        if self.state.is_draining.load(Ordering::SeqCst) {
            return Err(SfuDomainError::SystemDraining(
                "Node is draining; rejecting pipe consume".to_string(),
            ));
        }

        self.check_enterprise_license()?;

        validation::validate_not_empty("request_id", &params.request_id)?;
        validation::validate_not_empty("router_id", &params.router_id)?;
        validation::validate_not_empty("pipe_transport_id", &params.pipe_transport_id)?;
        validation::validate_not_empty("producer_id", &params.producer_id)?;

        let payload_bytes = format!(
            "{}:{}:{}:{}",
            params.request_id, params.router_id, params.pipe_transport_id, params.producer_id
        )
        .into_bytes();

        let metrics_ref = self.metrics.clone();

        self.pipe_consume_cache
            .execute_or_get_with_hook(
                &params.request_id,
                &payload_bytes,
                || async {
                    let pipe_transport = {
                        let t_item = self
                            .state
                            .pipe_transports
                            .get(&params.pipe_transport_id)
                            .ok_or_else(|| {
                                SfuDomainError::NotFound(format!(
                                    "PipeTransport '{}' not found",
                                    params.pipe_transport_id
                                ))
                            })?;

                        if t_item.is_closed.load(Ordering::SeqCst) {
                            return Err(SfuDomainError::NotFound(format!(
                                "PipeTransport '{}' is closed",
                                params.pipe_transport_id
                            )));
                        }

                        if t_item.router_id != params.router_id {
                            return Err(SfuDomainError::NotFound(format!(
                                "PipeTransport '{}' does not belong to router '{}'",
                                params.pipe_transport_id, params.router_id
                            )));
                        }

                        t_item.transport.clone()
                    };

                    let (producer_physical_router_id, producer_worker_index, is_source_paused) = {
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
                            p_item.producer.paused(),
                        )
                    };

                    let main_worker_index = {
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

                        r_item.worker_index
                    };

                    // If producer was created on a Sub-Router, pipe it to main router where pipe_transport lives
                    if producer_physical_router_id != params.router_id {
                        let singleflight_key =
                            format!("{}:{}", params.producer_id, params.router_id);
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
                                    .any(|e| e.target_router_id == params.router_id)
                            })
                            .unwrap_or(false);

                        if !already_piped {
                            let (pub_router, target_router) = {
                                let r_item = self
                                    .state
                                    .routers
                                    .get(&params.router_id)
                                    .ok_or_else(|| {
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

                                let target_r = r_item.router.clone();
                                (pub_r, target_r)
                            };

                            let producer_id_obj: ProducerId =
                                params.producer_id.parse().map_err(|e| {
                                    SfuDomainError::InvalidArgument(format!(
                                        "Invalid producer_id: {e}"
                                    ))
                                })?;

                            let pipe_pair = pub_router
                                .pipe_producer_to_router(
                                    producer_id_obj,
                                    PipeToRouterOptions::new(target_router),
                                )
                                .await
                                .map_err(|e| {
                                    SfuDomainError::Mediasoup(format!(
                                        "Failed to pipe producer to main router: {e}"
                                    ))
                                })?;

                            let pipe_producer = pipe_pair.pipe_producer.into_inner();
                            if is_source_paused {
                                let _ = pipe_producer.pause().await;
                            }

                            self.worker_pool
                                .add_resource_score(producer_worker_index, 1);
                            self.worker_pool
                                .add_resource_score(main_worker_index, 1);

                            let entry = PipedProducerEntry {
                                pipe_consumer: pipe_pair.pipe_consumer,
                                pipe_producer,
                                target_router_id: params.router_id.clone(),
                                target_worker_index: main_worker_index,
                            };

                            self.state
                                .piped_producers
                                .entry(params.producer_id.clone())
                                .or_default()
                                .push(entry);
                        }
                    }

                    let producer_id_obj: ProducerId = params.producer_id.parse().map_err(|e| {
                        SfuDomainError::InvalidArgument(format!(
                            "Invalid producer_id '{}': {e}",
                            params.producer_id
                        ))
                    })?;

                    let mut consumer_options =
                        ConsumerOptions::new(producer_id_obj, RtpCapabilities::default());
                    consumer_options.paused = is_source_paused;

                    let consumer = pipe_transport
                        .consume(consumer_options)
                        .await
                        .map_err(|e| {
                            SfuDomainError::Mediasoup(format!(
                                "Failed to consume on PipeTransport: {e}"
                            ))
                        })?;

                    if !self.state.is_router_active(&params.router_id) {
                        drop(consumer);
                        return Err(SfuDomainError::NotFound(format!(
                            "Router '{}' was closed during pipe consume",
                            params.router_id
                        )));
                    }

                    let pt_active = self
                        .state
                        .pipe_transports
                        .get(&params.pipe_transport_id)
                        .map(|t| !t.is_closed.load(Ordering::SeqCst))
                        .unwrap_or(false);
                    if !pt_active {
                        drop(consumer);
                        return Err(SfuDomainError::NotFound(format!(
                            "PipeTransport '{}' was closed during pipe consume",
                            params.pipe_transport_id
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
                            "Producer '{}' was closed during pipe consume",
                            params.producer_id
                        )));
                    }

                    let consumer_id = consumer.id().to_string();
                    let (kind_str, domain_kind) = match consumer.kind() {
                        mediasoup::prelude::MediaKind::Audio => ("audio", DomainMediaKind::Audio),
                        mediasoup::prelude::MediaKind::Video => ("video", DomainMediaKind::Video),
                    };

                    let rtp_parameters = consumer.rtp_parameters();
                    let rtp_parameters_json =
                        serde_json::to_string(&rtp_parameters).unwrap_or_else(|_| "{}".to_string());

                    let encodings: Vec<SimulcastEncodingInfo> = rtp_parameters
                        .encodings
                        .iter()
                        .map(|e| SimulcastEncodingInfo {
                            rid: e.rid.clone().unwrap_or_default(),
                            ssrc: e.ssrc.unwrap_or(0),
                            rtx_ssrc: e.rtx.as_ref().map(|r| r.ssrc).unwrap_or(0),
                            max_bitrate: e.max_bitrate.unwrap_or(0) as u32,
                            scalability_mode: if e.scalability_mode == ScalabilityMode::None {
                                String::new()
                            } else {
                                e.scalability_mode.to_string()
                            },
                        })
                        .collect();

                    let ssrc = encodings.first().map(|e| e.ssrc).unwrap_or(0);
                    let rtx_ssrc = encodings.first().map(|e| e.rtx_ssrc).unwrap_or(0);
                    let first_codec = rtp_parameters.codecs.first();
                    let payload_type = first_codec.map(|c| c.payload_type() as u32).unwrap_or(0);
                    let rtx_payload_type = rtp_parameters
                        .codecs
                        .iter()
                        .find(|c| c.mime_type().as_str().to_lowercase().ends_with("/rtx"))
                        .map(|c| c.payload_type() as u32)
                        .unwrap_or(0);
                    let codec_mime_type = first_codec
                        .map(|c| c.mime_type().as_str().to_string())
                        .unwrap_or_default();
                    let clock_rate = first_codec.map(|c| c.clock_rate().get()).unwrap_or(0);

                    let is_closed = Arc::new(AtomicBool::new(false));
                    let r_id = params.router_id.clone();
                    let c_id = consumer_id.clone();
                    let peer_id = "inter-node-cascade".to_string();
                    let state_ref = self.state.clone();
                    let closed_ref = is_closed.clone();
                    let last_consumer_score = Arc::new(std::sync::atomic::AtomicU32::new(u32::MAX));
                    let event_publisher = self.event_publisher.clone();

                    consumer
                        .on_score(move |score| {
                            if closed_ref.load(Ordering::SeqCst)
                                || !state_ref.is_router_active(&r_id)
                            {
                                return;
                            }

                            let s_val = score.score as u32;
                            let prev = last_consumer_score.swap(s_val, Ordering::Relaxed);
                            if prev == s_val {
                                return;
                            }

                            event_publisher.publish_score(
                                &r_id,
                                &c_id,
                                &peer_id,
                                false,
                                s_val,
                                vec![s_val],
                            );
                        })
                        .detach();

                    let consumer_for_kf = consumer.clone();
                    let item = ConsumerItem {
                        consumer,
                        router_id: params.router_id.clone(),
                        physical_router_id: params.router_id.clone(),
                        worker_index: main_worker_index,
                        transport_id: params.pipe_transport_id.clone(),
                        producer_id: params.producer_id.clone(),
                        peer_id: "inter-node-cascade".to_string(),
                        kind: domain_kind,
                        is_closed,
                    };

                    self.state.consumers.insert(consumer_id.clone(), item);
                    self.state
                        .transport_consumers
                        .entry(params.pipe_transport_id.clone())
                        .or_default()
                        .insert(consumer_id.clone());
                    self.state
                        .producer_consumers
                        .entry(params.producer_id.clone())
                        .or_default()
                        .insert(consumer_id.clone());

                    let c_weight = if domain_kind == DomainMediaKind::Audio {
                        1
                    } else {
                        2
                    };
                    self.worker_pool.add_resource_score(main_worker_index, c_weight);

                    self.metrics.inc_active_consumers(kind_str);

                    // Immediate RTCP PLI/FIR keyframe dispatch for video pipe consumer
                    if consumer_for_kf.kind() == mediasoup::prelude::MediaKind::Video {
                        if let Err(e) = consumer_for_kf.request_key_frame().await {
                            tracing::warn!(
                                consumer_id = %consumer_id,
                                error = %e,
                                "Failed to request initial keyframe for pipe video consumer"
                            );
                        } else {
                            tracing::debug!(
                                consumer_id = %consumer_id,
                                "Requested initial keyframe for pipe video consumer"
                            );
                        }
                    }

                    Ok(ConsumeResult {
                        consumer_id,
                        producer_id: params.producer_id,
                        kind: kind_str.to_string(),
                        rtp_parameters_json,
                        ssrc,
                        rtx_ssrc,
                        payload_type,
                        rtx_payload_type,
                        codec_mime_type,
                        clock_rate,
                        encodings,
                        initial_spatial_layer: -1,
                        initial_temporal_layer: -1,
                    })
                },
                move || {
                    metrics_ref.record_idempotency_hit();
                },
            )
            .await
    }

    fn is_enterprise_enabled(&self) -> bool {
        self.state.is_enterprise_license_valid()
    }

    fn abort_cleanup_tasks(&self) {
        for handle in &self.cleanup_handles {
            handle.abort();
        }
    }
}
