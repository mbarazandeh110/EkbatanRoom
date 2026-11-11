use crate::application::state::{EngineState, TransportItem};
use crate::config::Config;
use crate::domain::errors::SfuDomainError;
use crate::domain::idempotency::IdempotencyCache;
use crate::domain::validation;
use crate::observability::redaction::{log_raw_sdp_if_enabled, redact_ice_pwd};
use crate::ports::inbound::{
    ConnectTransportParams, CreateTransportParams, CreateTransportResult, RestartIceResult,
    TransportUseCase,
};
use crate::ports::outbound::{EventPublisherPort, MetricsPort, WorkerPoolPort};
use async_trait::async_trait;
use dashmap::DashSet;
use mediasoup::prelude::*;
use mediasoup::types::data_structures::{DtlsFingerprint, DtlsParameters};
use mediasoup::webrtc_transport::WebRtcTransportRemoteParameters;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct TransportService {
    config: Config,
    state: EngineState,
    worker_pool: Arc<dyn WorkerPoolPort>,
    event_publisher: Arc<dyn EventPublisherPort>,
    metrics: Arc<dyn MetricsPort>,
    create_transport_cache: Arc<IdempotencyCache<CreateTransportResult>>,
    cleanup_handles: Vec<tokio::task::AbortHandle>,
}

impl TransportService {
    pub fn new(
        config: Config,
        state: EngineState,
        worker_pool: Arc<dyn WorkerPoolPort>,
        event_publisher: Arc<dyn EventPublisherPort>,
        metrics: Arc<dyn MetricsPort>,
    ) -> Self {
        let create_transport_cache = Arc::new(IdempotencyCache::default());
        let cleanup_task = create_transport_cache.start_periodic_cleanup(Duration::from_secs(30));

        Self {
            config,
            state,
            worker_pool,
            event_publisher,
            metrics,
            create_transport_cache,
            cleanup_handles: vec![cleanup_task.abort_handle()],
        }
    }

    pub fn abort_cleanup_tasks(&self) {
        for handle in &self.cleanup_handles {
            handle.abort();
        }
    }
}

#[async_trait]
impl TransportUseCase for TransportService {
    #[tracing::instrument(
        skip(self, params),
        fields(
            room_id = tracing::field::Empty,
            router_id = %params.router_id,
            peer_id = %params.peer_id,
            request_id = %params.request_id
        )
    )]
    async fn create_transport(
        &self,
        params: CreateTransportParams,
    ) -> Result<CreateTransportResult, SfuDomainError> {
        if let Some(room_id) = self.state.get_room_id_for_router(&params.router_id) {
            tracing::Span::current().record("room_id", room_id.as_str());
        }
        if self.state.is_draining.load(Ordering::SeqCst) {
            return Err(SfuDomainError::SystemDraining(
                "Node is draining; rejecting new transport".to_string(),
            ));
        }

        validation::validate_not_empty("request_id", &params.request_id)?;
        validation::validate_not_empty("router_id", &params.router_id)?;
        validation::validate_not_empty("peer_id", &params.peer_id)?;

        let payload_bytes = format!(
            "{}:{}:{}:{}:{}",
            params.request_id,
            params.router_id,
            params.peer_id,
            params.is_publisher,
            params.enable_sctp
        )
        .into_bytes();

        let metrics_ref = self.metrics.clone();
        let state_for_valid = self.state.clone();

        self.create_transport_cache
            .execute_or_get_validated(
                &params.request_id,
                &payload_bytes,
                move |cached: &CreateTransportResult| {
                    state_for_valid
                        .transports
                        .get(&cached.transport_id)
                        .map(|t| !t.is_closed.load(Ordering::SeqCst))
                        .unwrap_or(false)
                },
                || async {
                    // Extract primary router info and sub_routers map (DROP guard immediately)
                    let (primary_router, primary_worker_index, sub_routers) = {
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

                        (
                            r_item.router.clone(),
                            r_item.worker_index,
                            r_item.sub_routers.clone(),
                        )
                    };

                    // Peer Affinity & Worker Spillover with liveness verification:
                    let peer_affinity_key = (params.router_id.clone(), params.peer_id.clone());
                    let target_worker = {
                        let worker_from_affinity = self
                            .state
                            .peer_worker_affinity
                            .get(&peer_affinity_key)
                            .map(|v| *v);

                        match worker_from_affinity {
                            Some(w) if self.worker_pool.is_worker_alive(w) => w,
                            _ => {
                                let primary_alive =
                                    self.worker_pool.is_worker_alive(primary_worker_index);
                                let primary_score =
                                    self.worker_pool.worker_resource_score(primary_worker_index);
                                let threshold_score = ((self.config.worker_capacity_weight as f64)
                                    * (self.config.worker_spillover_threshold_percent as f64)
                                    / 100.0) as usize;

                                let chosen_worker =
                                    if !primary_alive || primary_score >= threshold_score {
                                        let (least_loaded, _) = self.worker_pool.get_least_loaded();
                                        least_loaded
                                    } else {
                                        primary_worker_index
                                    };

                                self.state
                                    .peer_worker_affinity
                                    .insert(peer_affinity_key, chosen_worker);
                                chosen_worker
                            }
                        }
                    };

                    let (router, physical_router_id, worker_index) =
                        if target_worker == primary_worker_index {
                            (
                                primary_router.clone(),
                                primary_router.id().to_string(),
                                primary_worker_index,
                            )
                        } else {
                            // Double-checked locking with singleflight to eliminate concurrent duplicate sub-router creation
                            let sub_router = if let Some(sub) = sub_routers.get(&target_worker) {
                                sub.router.clone()
                            } else {
                                let sf_key = (params.router_id.clone(), target_worker);
                                let lock = self
                                    .state
                                    .sub_router_singleflight
                                    .entry(sf_key.clone())
                                    .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                                    .clone();

                                let _key_guard = crate::application::state::SingleflightKeyGuard::new(
                                    self.state.sub_router_singleflight.clone(),
                                    sf_key.clone(),
                                );
                                let _guard = lock.lock().await;

                                let router = if let Some(sub) = sub_routers.get(&target_worker) {
                                    sub.router.clone()
                                } else {
                                    let worker = self
                                        .worker_pool
                                        .get_worker(target_worker)
                                        .ok_or_else(|| {
                                            SfuDomainError::Internal(format!(
                                                "Worker {} not found in pool",
                                                target_worker
                                            ))
                                        })?;

                                    let mut router_options = RouterOptions::default();
                                    router_options.media_codecs =
                                        crate::domain::codecs::router_codecs_from_config(
                                            &self.config.router_codecs,
                                        );
                                    let new_sub = worker.create_router(router_options).await.map_err(|e| {
                                        SfuDomainError::Mediasoup(format!(
                                            "Failed to create sub-router on worker {target_worker}: {e}"
                                        ))
                                    })?;

                                    let mut observer_options =
                                        mediasoup::audio_level_observer::AudioLevelObserverOptions::default();
                                    observer_options.interval =
                                        self.config.audio_level_observer.interval_ms;
                                    observer_options.threshold =
                                        self.config.audio_level_observer.threshold_dbov;
                                    let audio_level_observer = new_sub
                                        .create_audio_level_observer(observer_options)
                                        .await
                                        .map_err(|e| {
                                            SfuDomainError::Mediasoup(format!(
                                                "Failed to create AudioLevelObserver for sub-router: {e}"
                                            ))
                                        })?;

                                    let is_closed = Arc::new(AtomicBool::new(false));
                                    let event_publisher = self.event_publisher.clone();
                                    let r_id = params.router_id.clone();
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
                                                    .get_producer(&r_id, &p_id)
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

                                    let sub_item = crate::application::state::SubRouterItem {
                                        router: new_sub.clone(),
                                        worker_index: target_worker,
                                        audio_level_observer,
                                        is_closed,
                                    };

                                    if !self.state.is_router_active(&params.router_id)
                                        || !self.worker_pool.is_worker_alive(target_worker)
                                    {
                                        return Err(SfuDomainError::NotFound(format!(
                                            "Router '{}' closed or worker {} died during sub-router creation",
                                            params.router_id, target_worker
                                        )));
                                    }

                                    sub_routers.insert(target_worker, sub_item);
                                    self.worker_pool.increment_load(target_worker);
                                    new_sub
                                };

                                router
                            };

                            let id = sub_router.id().to_string();
                            (sub_router, id, target_worker)
                        };

                    let listen_info = self.config.create_listen_info(Protocol::Udp);

                    let mut options =
                        WebRtcTransportOptions::new(WebRtcTransportListenInfos::new(listen_info));
                    options.enable_sctp = params.enable_sctp;
                    options.initial_available_outgoing_bitrate =
                        self.config.webrtc_transport.initial_available_outgoing_bitrate;
                    options.max_send_message_size =
                        self.config.webrtc_transport.max_send_message_size;
                    options.max_receive_message_size =
                        self.config.webrtc_transport.max_receive_message_size;
                    options.sctp_send_buffer_size =
                        self.config.webrtc_transport.sctp_send_buffer_size;
                    options.sctp_per_stream_send_queue_limit =
                        self.config.webrtc_transport.sctp_per_stream_send_queue_limit;
                    options.sctp_max_receiver_window_buffer_size =
                        self.config.webrtc_transport.sctp_max_receiver_window_buffer_size;

                    // Await on WebRtcTransport creation without holding any DashMap guard
                    let transport = router.create_webrtc_transport(options).await.map_err(|e| {
                        SfuDomainError::Mediasoup(format!("Failed to create WebRtcTransport: {e}"))
                    })?;

                    if !self.state.is_router_active(&params.router_id) {
                        drop(transport);
                        return Err(SfuDomainError::NotFound(format!(
                            "Router '{}' was closed during transport creation",
                            params.router_id
                        )));
                    }

                    if let Some(bitrate) = self.config.webrtc_transport.max_incoming_bitrate {
                        if let Err(e) = transport.set_max_incoming_bitrate(bitrate).await {
                            tracing::warn!(transport_id = %transport.id(), "Failed to set max_incoming_bitrate: {e}");
                        }
                    }
                    let min_bitrate = self.config.webrtc_transport.min_outgoing_bitrate.unwrap_or(600_000);
                    if min_bitrate > 0 {
                        if let Err(e) = transport.set_min_outgoing_bitrate(min_bitrate).await {
                            tracing::warn!(transport_id = %transport.id(), "Failed to set min_outgoing_bitrate: {e}");
                        }
                    }
                    if let Some(bitrate) = self.config.webrtc_transport.max_outgoing_bitrate {
                        if let Err(e) = transport.set_max_outgoing_bitrate(bitrate).await {
                            tracing::warn!(transport_id = %transport.id(), "Failed to set max_outgoing_bitrate: {e}");
                        }
                    }

                    // Re-verify router liveness after bitrate configuration awaits
                    if !self.state.is_router_active(&params.router_id) {
                        drop(transport);
                        return Err(SfuDomainError::NotFound(format!(
                            "Router '{}' was closed during transport bitrate configuration",
                            params.router_id
                        )));
                    }

                    let transport_id = transport.id().to_string();
                    let ice_ufrag = transport.ice_parameters().username_fragment.clone();
                    let ice_pwd = transport.ice_parameters().password.clone();

                    tracing::debug!(
                        transport_id = %transport_id,
                        ice_ufrag = %ice_ufrag,
                        ice_pwd = %redact_ice_pwd(&ice_pwd),
                        "WebRtcTransport created"
                    );

                    let raw_ice_json = serde_json::to_string(transport.ice_parameters())
                        .map_err(|e| SfuDomainError::Internal(e.to_string()))?;

                    let dtls_params = transport.dtls_parameters();
                    let raw_dtls_json = serde_json::to_string(&dtls_params)
                        .map_err(|e| SfuDomainError::Internal(e.to_string()))?;

                    let ice_candidates: Vec<crate::domain::entities::IceCandidateInfo> = transport
                        .ice_candidates()
                        .iter()
                        .map(|c| {
                            let sdp_line = format!(
                                "candidate:{} 1 {} {} {} {} typ {} generation 0",
                                c.foundation,
                                format!("{:?}", c.protocol).to_lowercase(),
                                c.priority,
                                c.address,
                                c.port,
                                format!("{:?}", c.r#type).to_lowercase()
                            );
                            log_raw_sdp_if_enabled(
                                self.config.observability.log_raw_sdp,
                                "create_transport_candidate",
                                &sdp_line,
                            );

                            crate::domain::entities::IceCandidateInfo {
                                foundation: c.foundation.clone(),
                                priority: c.priority,
                                ip: c.address.clone(),
                                protocol: format!("{:?}", c.protocol).to_lowercase(),
                                port: c.port as u32,
                                r#type: format!("{:?}", c.r#type).to_lowercase(),
                                tcp_type: c
                                    .tcp_type
                                    .map(|t| format!("{:?}", t).to_lowercase())
                                    .unwrap_or_default(),
                                sdp_candidate_line: sdp_line,
                            }
                        })
                        .collect();

                    let dtls_fingerprints = dtls_params
                        .fingerprints
                        .iter()
                        .map(|fp| {
                            let algorithm = match fp {
                                DtlsFingerprint::Sha1 { .. } => "sha-1",
                                DtlsFingerprint::Sha224 { .. } => "sha-224",
                                DtlsFingerprint::Sha256 { .. } => "sha-256",
                                DtlsFingerprint::Sha384 { .. } => "sha-384",
                                DtlsFingerprint::Sha512 { .. } => "sha-512",
                            };
                            crate::domain::entities::DtlsFingerprintInfo {
                                algorithm: algorithm.to_string(),
                                value: fp.value_string(),
                            }
                        })
                        .collect();

                    let sctp_port = transport
                        .sctp_parameters()
                        .map(|s| s.port as u32)
                        .unwrap_or(0);

                    let item = TransportItem {
                        transport,
                        router_id: params.router_id.clone(),
                        physical_router_id,
                        worker_index,
                        peer_id: params.peer_id.clone(),
                        is_publisher: params.is_publisher,
                        is_closed: Arc::new(AtomicBool::new(false)),
                    };

                    self.state.transports.insert(transport_id.clone(), item);
                    if let Some(set) = self.state.router_transports.get_mut(&params.router_id) {
                        set.insert(transport_id.clone());
                    }
                    self.state
                        .transport_producers
                        .insert(transport_id.clone(), DashSet::new());
                    self.state
                        .transport_consumers
                        .insert(transport_id.clone(), DashSet::new());
                    self.worker_pool.add_resource_score(worker_index, 1);
                    self.metrics.inc_active_transports(params.is_publisher);

                    Ok(CreateTransportResult {
                        transport_id,
                        ice_ufrag,
                        ice_pwd,
                        ice_candidates,
                        dtls_fingerprints,
                        sctp_port,
                        raw_ice_parameters_json: raw_ice_json,
                        raw_dtls_parameters_json: raw_dtls_json,
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
            transport_id = %params.transport_id,
            peer_id = tracing::field::Empty
        )
    )]
    async fn connect_transport(
        &self,
        params: ConnectTransportParams,
    ) -> Result<(), SfuDomainError> {
        if let Some(room_id) = self.state.get_room_id_for_router(&params.router_id) {
            tracing::Span::current().record("room_id", room_id.as_str());
        }
        validation::validate_not_empty("router_id", &params.router_id)?;
        validation::validate_not_empty("transport_id", &params.transport_id)?;

        // Extract transport, check router ownership, and DROP guard before .await (Deadlock fix)
        let (transport, peer_id, is_publisher) = {
            let t_item = self
                .state
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
                t_item.peer_id.clone(),
                t_item.is_publisher,
            )
        };
        tracing::Span::current().record("peer_id", peer_id.as_str());

        // In Mediasoup's ORTC model, WebRtcTransportRemoteParameters::dtls_parameters.role
        // represents the REMOTE endpoint's DTLS role (dtlsRemoteRole).
        // In WebRtcTransport.cpp:
        // - if dtlsRemoteRole == CLIENT -> Mediasoup local role = SERVER (waits passively for ClientHello)
        // - if dtlsRemoteRole == SERVER or AUTO -> Mediasoup local role = CLIENT (proactively sends ClientHello)
        //
        // When ConnectTransport is invoked with DtlsRole::Client (e.g. for publisher transports
        // where Mediasoup must actively initiate the DTLS handshake via ClientHello to prevent
        // handshake deadlock with ICE-controlling agents), Mediasoup must be instructed to become
        // DTLS Client. To achieve this, the remote role passed to Mediasoup must be Server.
        // Reciprocal DTLS role mapping:
        // - Requested role Client -> remote role Server (Mediasoup becomes active Client)
        // - Requested role Server -> remote role Client (Mediasoup becomes passive Server)
        // - Requested role Auto -> remote role Auto (Mediasoup defaults to active Client)
        let mediasoup_remote_role = match params.dtls_role {
            crate::domain::entities::DtlsRole::Client => {
                mediasoup::types::data_structures::DtlsRole::Server
            }
            crate::domain::entities::DtlsRole::Server => {
                mediasoup::types::data_structures::DtlsRole::Client
            }
            crate::domain::entities::DtlsRole::Auto => {
                mediasoup::types::data_structures::DtlsRole::Auto
            }
        };

        tracing::info!(
            router_id = %params.router_id,
            transport_id = %params.transport_id,
            peer_id = %peer_id,
            is_publisher = is_publisher,
            requested_dtls_role = ?params.dtls_role,
            mediasoup_remote_role = ?mediasoup_remote_role,
            "Connecting transport with reciprocal DTLS role assignment"
        );

        let fp_tuples: Vec<(String, String)> = params
            .remote_fingerprints
            .into_iter()
            .map(|f| (f.algorithm, f.value))
            .collect();
        let fingerprints = crate::domain::validation::parse_dtls_fingerprints(&fp_tuples)?;

        let dtls_parameters = DtlsParameters {
            role: mediasoup_remote_role,
            fingerprints,
        };

        transport
            .connect(WebRtcTransportRemoteParameters { dtls_parameters })
            .await
            .map_err(|e| SfuDomainError::Mediasoup(format!("Failed to connect transport: {e}")))?;

        let established_local_role = transport.dtls_parameters().role;
        tracing::info!(
            transport_id = %params.transport_id,
            established_local_dtls_role = ?established_local_role,
            "WebRtcTransport DTLS role successfully established"
        );

        Ok(())
    }

    #[tracing::instrument(
        skip(self),
        fields(
            room_id = tracing::field::Empty,
            router_id = %router_id,
            transport_id = %transport_id,
            peer_id = tracing::field::Empty
        )
    )]
    async fn restart_ice(
        &self,
        router_id: &str,
        transport_id: &str,
    ) -> Result<RestartIceResult, SfuDomainError> {
        if let Some(room_id) = self.state.get_room_id_for_router(router_id) {
            tracing::Span::current().record("room_id", room_id.as_str());
        }
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("transport_id", transport_id)?;

        // Extract transport, check router ownership, and DROP guard before .await (Deadlock fix)
        let (transport, peer_id) = {
            let t_item = self.state.transports.get(transport_id).ok_or_else(|| {
                SfuDomainError::NotFound(format!("Transport '{transport_id}' not found"))
            })?;

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

            (t_item.transport.clone(), t_item.peer_id.clone())
        };
        tracing::Span::current().record("peer_id", peer_id.as_str());

        let new_ice = transport
            .restart_ice()
            .await
            .map_err(|e| SfuDomainError::Mediasoup(format!("Failed to restart ICE: {e}")))?;

        let raw_ice_json =
            serde_json::to_string(&new_ice).map_err(|e| SfuDomainError::Internal(e.to_string()))?;

        Ok(RestartIceResult {
            ice_ufrag: new_ice.username_fragment,
            ice_pwd: new_ice.password,
            raw_ice_parameters_json: raw_ice_json,
        })
    }

    #[tracing::instrument(
        skip(self),
        fields(
            room_id = tracing::field::Empty,
            router_id = %router_id,
            transport_id = %transport_id,
            peer_id = tracing::field::Empty
        )
    )]
    fn close_transport(&self, router_id: &str, transport_id: &str) -> Result<(), SfuDomainError> {
        if let Some(room_id) = self.state.get_room_id_for_router(router_id) {
            tracing::Span::current().record("room_id", room_id.as_str());
        }
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("transport_id", transport_id)?;

        if let Some(t_ref) = self.state.transports.get(transport_id) {
            if t_ref.router_id != router_id {
                return Err(SfuDomainError::NotFound(format!(
                    "Transport '{transport_id}' does not belong to router '{router_id}'"
                )));
            }
        }

        if let Some((_, t_item)) = self
            .state
            .transports
            .remove_if(transport_id, |_, item| item.router_id == router_id)
        {
            tracing::Span::current().record("peer_id", t_item.peer_id.as_str());

            t_item.is_closed.store(true, Ordering::SeqCst);
            self.worker_pool.sub_resource_score(t_item.worker_index, 1);
            self.metrics.dec_active_transports(t_item.is_publisher);

            if let Some(set) = self.state.router_transports.get_mut(router_id) {
                set.remove(transport_id);
            }

            let has_other_transports = self
                .state
                .router_transports
                .get(router_id)
                .map(|tids| {
                    tids.iter().any(|tid| {
                        self.state
                            .transports
                            .get(&*tid)
                            .map(|t| t.peer_id == t_item.peer_id)
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if !has_other_transports {
                self.state
                    .peer_worker_affinity
                    .remove(&(router_id.to_string(), t_item.peer_id.clone()));
            }

            self.event_publisher.publish_resource_closed(
                router_id,
                "transport",
                transport_id,
                "transport_closed",
            );

            cascade_close_producers_and_consumers(
                &self.state,
                self.worker_pool.as_ref(),
                self.metrics.as_ref(),
                self.event_publisher.as_ref(),
                router_id,
                transport_id,
            );
            return Ok(());
        }

        if let Some(p_ref) = self.state.pipe_transports.get(transport_id) {
            if p_ref.router_id != router_id {
                return Err(SfuDomainError::NotFound(format!(
                    "PipeTransport '{transport_id}' does not belong to router '{router_id}'"
                )));
            }
        }

        if let Some((_, p_item)) = self
            .state
            .pipe_transports
            .remove_if(transport_id, |_, item| item.router_id == router_id)
        {
            p_item.is_closed.store(true, Ordering::SeqCst);
            self.worker_pool.sub_resource_score(p_item.worker_index, 1);
            self.metrics.dec_active_transports(false);

            if let Some(set) = self.state.router_pipe_transports.get_mut(router_id) {
                set.remove(transport_id);
            }

            self.event_publisher.publish_resource_closed(
                router_id,
                "transport",
                transport_id,
                "pipe_transport_closed",
            );

            cascade_close_producers_and_consumers(
                &self.state,
                self.worker_pool.as_ref(),
                self.metrics.as_ref(),
                self.event_publisher.as_ref(),
                router_id,
                transport_id,
            );
        }

        Ok(())
    }
}

pub(crate) fn cascade_close_producers_and_consumers(
    state: &crate::application::state::EngineState,
    worker_pool: &dyn crate::ports::outbound::WorkerPoolPort,
    metrics: &dyn crate::ports::outbound::MetricsPort,
    event_publisher: &dyn crate::ports::outbound::EventPublisherPort,
    router_id: &str,
    transport_id: &str,
) {
    // Cascade close producers on this transport
    if let Some((_, p_ids)) = state.transport_producers.remove(transport_id) {
        for pid in p_ids {
            if let Some((_, p)) = state
                .producers
                .remove(&(router_id.to_string(), pid.clone()))
            {
                p.is_closed.store(true, Ordering::SeqCst);
                let p_weight = if p.kind == crate::domain::entities::MediaKind::Audio {
                    2
                } else {
                    10
                };
                worker_pool.sub_resource_score(p.worker_index, p_weight);
                metrics.dec_active_producers(&p.kind.to_string());

                // Close any piped instances of this producer belonging to this router
                let piped_entries = state.take_piped_producers_for_router(router_id, &pid);
                for entry in piped_entries {
                    worker_pool.sub_resource_score(p.worker_index, 1);
                    worker_pool.sub_resource_score(entry.target_worker_index, 1);
                }

                event_publisher.publish_resource_closed(
                    router_id,
                    "producer",
                    &pid,
                    "cascade_closed",
                );

                // Cascade close only consumers connected to this producer on this router
                let c_ids = state.take_consumers_for_producer(router_id, &pid);
                for cid in c_ids {
                    if let Some(c) = state.cascade_remove_consumer(&cid) {
                        let c_weight = if c.kind == crate::domain::entities::MediaKind::Audio {
                            1
                        } else {
                            2
                        };
                        worker_pool.sub_resource_score(c.worker_index, c_weight);
                        metrics.dec_active_consumers(&c.kind.to_string());
                        event_publisher.publish_resource_closed(
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

    // Cascade close consumers directly on this transport
    if let Some((_, c_ids)) = state.transport_consumers.remove(transport_id) {
        for cid in c_ids {
            if let Some(c) = state.cascade_remove_consumer(&cid) {
                let c_weight = if c.kind == crate::domain::entities::MediaKind::Audio {
                    1
                } else {
                    2
                };
                worker_pool.sub_resource_score(c.worker_index, c_weight);
                metrics.dec_active_consumers(&c.kind.to_string());

                // Clean up intra-node pipe if this was the last consumer on the target physical router
                if let Some((piped_entry, pub_worker_idx)) = state
                    .cleanup_piped_producer_if_last_consumer(
                        &c.router_id,
                        &c.producer_id,
                        &c.physical_router_id,
                    )
                {
                    worker_pool.sub_resource_score(pub_worker_idx, 1);
                    worker_pool.sub_resource_score(piped_entry.target_worker_index, 1);
                }

                event_publisher.publish_resource_closed(
                    router_id,
                    "consumer",
                    &cid,
                    "cascade_closed",
                );
            }
        }
    }
}
