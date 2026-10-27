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
use crate::ports::outbound::{EventPublisherPort, MetricsPort};
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
    event_publisher: Arc<dyn EventPublisherPort>,
    metrics: Arc<dyn MetricsPort>,
    create_transport_cache: Arc<IdempotencyCache<CreateTransportResult>>,
}

impl TransportService {
    pub fn new(
        config: Config,
        state: EngineState,
        event_publisher: Arc<dyn EventPublisherPort>,
        metrics: Arc<dyn MetricsPort>,
    ) -> Self {
        let create_transport_cache = Arc::new(IdempotencyCache::default());
        let _cleanup_task = create_transport_cache.start_periodic_cleanup(Duration::from_secs(30));

        Self {
            config,
            state,
            event_publisher,
            metrics,
            create_transport_cache,
        }
    }
}

#[async_trait]
impl TransportUseCase for TransportService {
    #[tracing::instrument(skip(self, params), fields(router_id = %params.router_id, peer_id = %params.peer_id, request_id = %params.request_id))]
    async fn create_transport(
        &self,
        params: CreateTransportParams,
    ) -> Result<CreateTransportResult, SfuDomainError> {
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

        self.create_transport_cache
            .execute_or_get_with_hook(
                &params.request_id,
                &payload_bytes,
                || async {
                    // Extract router and DROP guard immediately before .await (Deadlock fix)
                    let router = {
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

                        r_item.router.clone()
                    };

                    let listen_info = ListenInfo {
                        protocol: Protocol::Udp,
                        ip: self.config.listen_ip,
                        announced_address: self.config.announced_ip.clone(),
                        expose_internal_ip: false,
                        port: None,
                        port_range: Some(self.config.rtc_min_port..=self.config.rtc_max_port),
                        flags: None,
                        send_buffer_size: None,
                        recv_buffer_size: None,
                    };

                    let mut options =
                        WebRtcTransportOptions::new(WebRtcTransportListenInfos::new(listen_info));
                    options.enable_sctp = params.enable_sctp;

                    // Await on WebRtcTransport creation without holding any DashMap guard
                    let transport = router.create_webrtc_transport(options).await.map_err(|e| {
                        SfuDomainError::Mediasoup(format!("Failed to create WebRtcTransport: {e}"))
                    })?;

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

    #[tracing::instrument(skip(self, params), fields(router_id = %params.router_id, transport_id = %params.transport_id))]
    async fn connect_transport(
        &self,
        params: ConnectTransportParams,
    ) -> Result<(), SfuDomainError> {
        validation::validate_not_empty("router_id", &params.router_id)?;
        validation::validate_not_empty("transport_id", &params.transport_id)?;

        // Extract transport, check router ownership, and DROP guard before .await (Deadlock fix)
        let transport = {
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

            t_item.transport.clone()
        };

        let role = match params.dtls_role {
            crate::domain::entities::DtlsRole::Auto => {
                mediasoup::types::data_structures::DtlsRole::Auto
            }
            crate::domain::entities::DtlsRole::Client => {
                mediasoup::types::data_structures::DtlsRole::Client
            }
            crate::domain::entities::DtlsRole::Server => {
                mediasoup::types::data_structures::DtlsRole::Server
            }
        };

        let fp_tuples: Vec<(String, String)> = params
            .remote_fingerprints
            .into_iter()
            .map(|fp| (fp.algorithm, fp.value))
            .collect();
        let fingerprints = validation::parse_dtls_fingerprints(&fp_tuples)?;
        let dtls_parameters = DtlsParameters { role, fingerprints };

        transport
            .connect(WebRtcTransportRemoteParameters { dtls_parameters })
            .await
            .map_err(|e| SfuDomainError::Mediasoup(format!("Failed to connect transport: {e}")))?;

        Ok(())
    }

    #[tracing::instrument(skip(self), fields(router_id = %router_id, transport_id = %transport_id))]
    async fn restart_ice(
        &self,
        router_id: &str,
        transport_id: &str,
    ) -> Result<RestartIceResult, SfuDomainError> {
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("transport_id", transport_id)?;

        // Extract transport, check router ownership, and DROP guard before .await (Deadlock fix)
        let transport = {
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

            t_item.transport.clone()
        };

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

    #[tracing::instrument(skip(self), fields(router_id = %router_id, transport_id = %transport_id))]
    fn close_transport(&self, router_id: &str, transport_id: &str) -> Result<(), SfuDomainError> {
        validation::validate_not_empty("router_id", router_id)?;
        validation::validate_not_empty("transport_id", transport_id)?;

        if let Some((_, t_item)) = self.state.transports.remove(transport_id) {
            if t_item.router_id != router_id {
                // Restore if router mismatch
                self.state
                    .transports
                    .insert(transport_id.to_string(), t_item);
                return Err(SfuDomainError::NotFound(format!(
                    "Transport '{transport_id}' does not belong to router '{router_id}'"
                )));
            }

            t_item.is_closed.store(true, Ordering::SeqCst);
            self.metrics.dec_active_transports(t_item.is_publisher);

            if let Some(set) = self.state.router_transports.get_mut(router_id) {
                set.remove(transport_id);
            }

            self.event_publisher.publish_resource_closed(
                router_id,
                "transport",
                transport_id,
                "transport_closed",
            );

            // Cascade close producers on this transport
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

                        // Cascade close all consumers connected to this producer
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

            // Cascade close consumers directly on this transport
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

        Ok(())
    }
}
