use crate::config::Config;
use crate::domain::error::SfuError;
use crate::domain::idempotency::IdempotencyCache;
use crate::domain::validation;
use crate::engine::codecs::standard_media_codecs;
use crate::engine::worker_pool::WorkerPool;
use crate::proto::sfu::v1 as proto;

use dashmap::{DashMap, DashSet};
use mediasoup::consumer::ConsumerRtpStreamStat;
use mediasoup::prelude::*;
use mediasoup::types::data_structures::{DtlsFingerprint, DtlsParameters};
use mediasoup::types::scalability_modes::ScalabilityMode;
use mediasoup::webrtc_transport::WebRtcTransportRemoteParameters;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct RouterItem {
    pub router: Router,
    pub room_id: String,
    pub worker_index: usize,
    pub is_closed: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct TransportItem {
    pub transport: WebRtcTransport,
    pub router_id: String,
    pub peer_id: String,
    pub is_publisher: bool,
    pub is_closed: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct ProducerItem {
    pub producer: Producer,
    pub router_id: String,
    pub transport_id: String,
    pub peer_id: String,
    pub kind: MediaKind,
    pub is_closed: Arc<AtomicBool>,
}

#[derive(Clone)]
pub struct ConsumerItem {
    pub consumer: Consumer,
    pub router_id: String,
    pub transport_id: String,
    pub producer_id: String,
    pub peer_id: String,
    pub kind: MediaKind,
    pub is_closed: Arc<AtomicBool>,
}

pub struct MediaNode {
    config: Config,
    worker_pool: Arc<WorkerPool>,
    routers: Arc<DashMap<String, RouterItem>>,
    room_to_router: Arc<DashMap<String, String>>,
    transports: Arc<DashMap<String, TransportItem>>,
    router_transports: Arc<DashMap<String, DashSet<String>>>,
    producers: Arc<DashMap<String, ProducerItem>>,
    transport_producers: Arc<DashMap<String, DashSet<String>>>,
    consumers: Arc<DashMap<String, ConsumerItem>>,
    transport_consumers: Arc<DashMap<String, DashSet<String>>>,

    event_sender: broadcast::Sender<proto::NodeEvent>,
    create_router_cache: IdempotencyCache<proto::CreateRouterResponse>,
    create_transport_cache: IdempotencyCache<proto::CreateTransportResponse>,
    produce_cache: IdempotencyCache<proto::ProduceResponse>,
    consume_cache: IdempotencyCache<proto::ConsumeResponse>,
}

impl MediaNode {
    pub async fn new(config: Config) -> Result<Self, SfuError> {
        let worker_manager = WorkerManager::new();
        let worker_pool = Arc::new(
            WorkerPool::new(
                &worker_manager,
                config.worker_count,
                config.rtc_min_port,
                config.rtc_max_port,
            )
            .await?,
        );

        let (event_sender, _) = broadcast::channel(1024);

        Ok(Self {
            config,
            worker_pool,
            routers: Arc::new(DashMap::new()),
            room_to_router: Arc::new(DashMap::new()),
            transports: Arc::new(DashMap::new()),
            router_transports: Arc::new(DashMap::new()),
            producers: Arc::new(DashMap::new()),
            transport_producers: Arc::new(DashMap::new()),
            consumers: Arc::new(DashMap::new()),
            transport_consumers: Arc::new(DashMap::new()),
            event_sender,
            create_router_cache: IdempotencyCache::default(),
            create_transport_cache: IdempotencyCache::default(),
            produce_cache: IdempotencyCache::default(),
            consume_cache: IdempotencyCache::default(),
        })
    }

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    pub fn is_router_active(&self, router_id: &str) -> bool {
        self.routers
            .get(router_id)
            .map(|r| !r.is_closed.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    fn emit_event(&self, router_id: &str, payload: proto::node_event::Payload) {
        if !self.is_router_active(router_id) {
            return;
        }

        let event = proto::NodeEvent {
            router_id: router_id.to_string(),
            timestamp_ms: Self::now_ms(),
            payload: Some(payload),
        };

        let _ = self.event_sender.send(event);
    }

    // --- 1. Router Lifecycle ---
    pub async fn create_router(
        &self,
        req: proto::CreateRouterRequest,
    ) -> Result<proto::CreateRouterResponse, SfuError> {
        validation::validate_not_empty("request_id", &req.request_id)?;
        validation::validate_not_empty("room_id", &req.room_id)?;

        let payload_bytes = format!("{}:{}", req.request_id, req.room_id).into_bytes();

        self.create_router_cache
            .execute_or_get(&req.request_id, &payload_bytes, || async {
                // If room already has an active router, reuse it
                if let Some(router_id) = self.room_to_router.get(&req.room_id) {
                    if let Some(item) = self.routers.get(router_id.value()) {
                        if !item.is_closed.load(Ordering::SeqCst) {
                            let caps_json = serde_json::to_string(&item.router.rtp_capabilities())
                                .map_err(|e| SfuError::Internal(e.to_string()))?;

                            return Ok(proto::CreateRouterResponse {
                                router_id: item.router.id().to_string(),
                                worker_index: item.worker_index as u32,
                                router_rtp_capabilities_json: caps_json,
                            });
                        }
                    }
                }

                let (worker_index, worker) = self.worker_pool.get_least_loaded();
                let router_codecs = standard_media_codecs();
                let router = worker
                    .create_router(RouterOptions::new(router_codecs))
                    .await
                    .map_err(|e| SfuError::Mediasoup(format!("Failed to create router: {e}")))?;

                let router_id = router.id().to_string();
                let caps_json = serde_json::to_string(&router.rtp_capabilities())
                    .map_err(|e| SfuError::Internal(e.to_string()))?;

                let item = RouterItem {
                    router,
                    room_id: req.room_id.clone(),
                    worker_index,
                    is_closed: Arc::new(AtomicBool::new(false)),
                };

                self.worker_pool.increment_load(worker_index);
                self.routers.insert(router_id.clone(), item);
                self.room_to_router
                    .insert(req.room_id.clone(), router_id.clone());
                self.router_transports
                    .insert(router_id.clone(), DashSet::new());

                Ok(proto::CreateRouterResponse {
                    router_id,
                    worker_index: worker_index as u32,
                    router_rtp_capabilities_json: caps_json,
                })
            })
            .await
    }

    // --- 2. Transport Lifecycle ---
    pub async fn create_transport(
        &self,
        req: proto::CreateTransportRequest,
    ) -> Result<proto::CreateTransportResponse, SfuError> {
        validation::validate_not_empty("request_id", &req.request_id)?;
        validation::validate_not_empty("router_id", &req.router_id)?;
        validation::validate_not_empty("peer_id", &req.peer_id)?;

        let payload_bytes = format!(
            "{}:{}:{}:{}:{}",
            req.request_id, req.router_id, req.peer_id, req.is_publisher, req.enable_sctp
        )
        .into_bytes();

        self.create_transport_cache
            .execute_or_get(&req.request_id, &payload_bytes, || async {
                let router_item = self.routers.get(&req.router_id).ok_or_else(|| {
                    SfuError::NotFound(format!("Router '{}' not found", req.router_id))
                })?;

                if router_item.is_closed.load(Ordering::SeqCst) {
                    return Err(SfuError::NotFound(format!(
                        "Router '{}' is closed",
                        req.router_id
                    )));
                }

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
                options.enable_sctp = req.enable_sctp;

                let transport = router_item
                    .router
                    .create_webrtc_transport(options)
                    .await
                    .map_err(|e| {
                        SfuError::Mediasoup(format!("Failed to create WebRtcTransport: {e}"))
                    })?;

                let transport_id = transport.id().to_string();
                let ice_ufrag = transport.ice_parameters().username_fragment.clone();
                let ice_pwd = transport.ice_parameters().password.clone();
                let raw_ice_json = serde_json::to_string(transport.ice_parameters())
                    .map_err(|e| SfuError::Internal(e.to_string()))?;

                let dtls_params = transport.dtls_parameters();
                let raw_dtls_json = serde_json::to_string(&dtls_params)
                    .map_err(|e| SfuError::Internal(e.to_string()))?;

                let ice_candidates = transport
                    .ice_candidates()
                    .iter()
                    .map(|c| proto::IceCandidateInfo {
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
                        sdp_candidate_line: format!(
                            "candidate:{} 1 {} {} {} {} typ {} generation 0",
                            c.foundation,
                            format!("{:?}", c.protocol).to_lowercase(),
                            c.priority,
                            c.address,
                            c.port,
                            format!("{:?}", c.r#type).to_lowercase()
                        ),
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
                        proto::DtlsFingerprintInfo {
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
                    router_id: req.router_id.clone(),
                    peer_id: req.peer_id.clone(),
                    is_publisher: req.is_publisher,
                    is_closed: Arc::new(AtomicBool::new(false)),
                };

                self.transports.insert(transport_id.clone(), item);
                if let Some(set) = self.router_transports.get(&req.router_id) {
                    set.insert(transport_id.clone());
                }
                self.transport_producers
                    .insert(transport_id.clone(), DashSet::new());
                self.transport_consumers
                    .insert(transport_id.clone(), DashSet::new());

                Ok(proto::CreateTransportResponse {
                    transport_id,
                    ice_ufrag,
                    ice_pwd,
                    ice_candidates,
                    dtls_fingerprints,
                    sctp_port,
                    raw_ice_parameters_json: raw_ice_json,
                    raw_dtls_parameters_json: raw_dtls_json,
                })
            })
            .await
    }

    pub async fn connect_transport(
        &self,
        req: proto::ConnectTransportRequest,
    ) -> Result<proto::ConnectTransportResponse, SfuError> {
        validation::validate_not_empty("router_id", &req.router_id)?;
        validation::validate_not_empty("transport_id", &req.transport_id)?;

        let transport_item = self.transports.get(&req.transport_id).ok_or_else(|| {
            SfuError::NotFound(format!("Transport '{}' not found", req.transport_id))
        })?;

        if transport_item.is_closed.load(Ordering::SeqCst) {
            return Err(SfuError::NotFound(format!(
                "Transport '{}' is closed",
                req.transport_id
            )));
        }

        let role = validation::parse_dtls_role(&req.dtls_role)?;
        let fp_tuples: Vec<(String, String)> = req
            .remote_fingerprints
            .into_iter()
            .map(|fp| (fp.algorithm, fp.value))
            .collect();
        let fingerprints = validation::parse_dtls_fingerprints(&fp_tuples)?;

        let dtls_parameters = DtlsParameters { role, fingerprints };

        transport_item
            .transport
            .connect(WebRtcTransportRemoteParameters { dtls_parameters })
            .await
            .map_err(|e| SfuError::Mediasoup(format!("Failed to connect transport: {e}")))?;

        Ok(proto::ConnectTransportResponse {})
    }

    pub async fn restart_ice(
        &self,
        req: proto::RestartIceRequest,
    ) -> Result<proto::RestartIceResponse, SfuError> {
        validation::validate_not_empty("router_id", &req.router_id)?;
        validation::validate_not_empty("transport_id", &req.transport_id)?;

        let transport_item = self.transports.get(&req.transport_id).ok_or_else(|| {
            SfuError::NotFound(format!("Transport '{}' not found", req.transport_id))
        })?;

        if transport_item.is_closed.load(Ordering::SeqCst) {
            return Err(SfuError::NotFound(format!(
                "Transport '{}' is closed",
                req.transport_id
            )));
        }

        let new_ice = transport_item
            .transport
            .restart_ice()
            .await
            .map_err(|e| SfuError::Mediasoup(format!("Failed to restart ICE: {e}")))?;

        let raw_ice_json =
            serde_json::to_string(&new_ice).map_err(|e| SfuError::Internal(e.to_string()))?;

        Ok(proto::RestartIceResponse {
            ice_ufrag: new_ice.username_fragment,
            ice_pwd: new_ice.password,
            raw_ice_parameters_json: raw_ice_json,
        })
    }

    // --- 3. Cascading Close ---
    pub fn close_resource(
        &self,
        req: proto::CloseResourceRequest,
    ) -> Result<proto::CloseResourceResponse, SfuError> {
        validation::validate_not_empty("router_id", &req.router_id)?;
        validation::validate_not_empty("resource_id", &req.resource_id)?;

        match req.resource_type() {
            proto::ResourceType::Router => {
                if let Some((_, item)) = self.routers.remove(&req.resource_id) {
                    item.is_closed.store(true, Ordering::SeqCst);
                    self.room_to_router.remove(&item.room_id);
                    self.worker_pool.decrement_load(item.worker_index);

                    // Cascade close transports
                    if let Some((_, transport_ids)) =
                        self.router_transports.remove(&req.resource_id)
                    {
                        for tid in transport_ids {
                            self.close_transport_internal(&tid);
                        }
                    }

                    self.emit_event(
                        &req.router_id,
                        proto::node_event::Payload::ResourceClosed(proto::ResourceClosedEvent {
                            resource_type: proto::ResourceType::Router as i32,
                            resource_id: req.resource_id.clone(),
                            reason: "router_closed".to_string(),
                        }),
                    );
                }
            }
            proto::ResourceType::Transport => {
                self.close_transport_internal(&req.resource_id);
                if let Some(set) = self.router_transports.get(&req.router_id) {
                    set.remove(&req.resource_id);
                }
                self.emit_event(
                    &req.router_id,
                    proto::node_event::Payload::ResourceClosed(proto::ResourceClosedEvent {
                        resource_type: proto::ResourceType::Transport as i32,
                        resource_id: req.resource_id.clone(),
                        reason: "transport_closed".to_string(),
                    }),
                );
            }
            proto::ResourceType::Producer => {
                if let Some((_, item)) = self.producers.remove(&req.resource_id) {
                    item.is_closed.store(true, Ordering::SeqCst);
                    if let Some(set) = self.transport_producers.get(&item.transport_id) {
                        set.remove(&req.resource_id);
                    }
                    self.emit_event(
                        &req.router_id,
                        proto::node_event::Payload::ResourceClosed(proto::ResourceClosedEvent {
                            resource_type: proto::ResourceType::Producer as i32,
                            resource_id: req.resource_id.clone(),
                            reason: "producer_closed".to_string(),
                        }),
                    );
                }
            }
            proto::ResourceType::Consumer => {
                if let Some((_, item)) = self.consumers.remove(&req.resource_id) {
                    item.is_closed.store(true, Ordering::SeqCst);
                    if let Some(set) = self.transport_consumers.get(&item.transport_id) {
                        set.remove(&req.resource_id);
                    }
                    self.emit_event(
                        &req.router_id,
                        proto::node_event::Payload::ResourceClosed(proto::ResourceClosedEvent {
                            resource_type: proto::ResourceType::Consumer as i32,
                            resource_id: req.resource_id.clone(),
                            reason: "consumer_closed".to_string(),
                        }),
                    );
                }
            }
            proto::ResourceType::Unspecified => {
                return Err(SfuError::InvalidArgument("Invalid resource_type".into()));
            }
        }

        Ok(proto::CloseResourceResponse {})
    }

    fn close_transport_internal(&self, transport_id: &str) {
        if let Some((_, t_item)) = self.transports.remove(transport_id) {
            t_item.is_closed.store(true, Ordering::SeqCst);

            // Close producers on this transport
            if let Some((_, p_ids)) = self.transport_producers.remove(transport_id) {
                for pid in p_ids {
                    if let Some((_, p)) = self.producers.remove(&pid) {
                        p.is_closed.store(true, Ordering::SeqCst);
                    }
                }
            }

            // Close consumers on this transport
            if let Some((_, c_ids)) = self.transport_consumers.remove(transport_id) {
                for cid in c_ids {
                    if let Some((_, c)) = self.consumers.remove(&cid) {
                        c.is_closed.store(true, Ordering::SeqCst);
                    }
                }
            }
        }
    }

    // --- 4. Media Produce & Consume (Slice 3) ---
    pub async fn produce(
        &self,
        req: proto::ProduceRequest,
    ) -> Result<proto::ProduceResponse, SfuError> {
        validation::validate_not_empty("request_id", &req.request_id)?;
        validation::validate_not_empty("router_id", &req.router_id)?;
        validation::validate_not_empty("transport_id", &req.transport_id)?;
        validation::validate_not_empty("peer_id", &req.peer_id)?;

        let kind = validation::parse_media_kind(&req.kind)?;
        let rtp_parameters = validation::parse_rtp_parameters(&req.rtp_parameters_json)?;

        let payload_bytes = format!(
            "{}:{}:{}:{}:{}:{}",
            req.request_id,
            req.router_id,
            req.transport_id,
            req.peer_id,
            req.kind,
            req.rtp_parameters_json
        )
        .into_bytes();

        self.produce_cache
            .execute_or_get(&req.request_id, &payload_bytes, || async {
                let transport_item = self.transports.get(&req.transport_id).ok_or_else(|| {
                    SfuError::NotFound(format!("Transport '{}' not found", req.transport_id))
                })?;

                if transport_item.is_closed.load(Ordering::SeqCst) {
                    return Err(SfuError::NotFound(format!(
                        "Transport '{}' is closed",
                        req.transport_id
                    )));
                }

                let mut options = ProducerOptions::new(kind, rtp_parameters);
                options.paused = req.paused;

                let producer = transport_item
                    .transport
                    .produce(options)
                    .await
                    .map_err(|e| SfuError::Mediasoup(format!("Failed to produce: {e}")))?;

                let producer_id = producer.id().to_string();
                let is_closed = Arc::new(AtomicBool::new(false));

                // Register on_score callback
                let event_tx = self.event_sender.clone();
                let r_id = req.router_id.clone();
                let p_id = producer_id.clone();
                let peer_id = req.peer_id.clone();
                let routers_ref = self.routers.clone();
                let closed_ref = is_closed.clone();

                producer
                    .on_score(move |scores| {
                        if closed_ref.load(Ordering::SeqCst) {
                            return;
                        }
                        if let Some(r) = routers_ref.get(&r_id) {
                            if r.is_closed.load(Ordering::SeqCst) {
                                return;
                            }
                        } else {
                            return;
                        }

                        let avg_score = if scores.is_empty() {
                            10
                        } else {
                            (scores.iter().map(|s| s.score as u32).sum::<u32>())
                                / (scores.len() as u32)
                        };
                        let all_scores = scores.iter().map(|s| s.score as u32).collect();

                        let event = proto::NodeEvent {
                            router_id: r_id.clone(),
                            timestamp_ms: Self::now_ms(),
                            payload: Some(proto::node_event::Payload::MediaScore(
                                proto::MediaScoreEvent {
                                    resource_id: p_id.clone(),
                                    peer_id: peer_id.clone(),
                                    is_producer: true,
                                    score: avg_score,
                                    producer_scores: all_scores,
                                },
                            )),
                        };
                        let _ = event_tx.send(event);
                    })
                    .detach();

                let encodings = producer
                    .rtp_parameters()
                    .encodings
                    .iter()
                    .map(|enc| proto::SimulcastEncodingInfo {
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

                let item = ProducerItem {
                    producer,
                    router_id: req.router_id.clone(),
                    transport_id: req.transport_id.clone(),
                    peer_id: req.peer_id.clone(),
                    kind,
                    is_closed,
                };

                self.producers.insert(producer_id.clone(), item);
                if let Some(set) = self.transport_producers.get(&req.transport_id) {
                    set.insert(producer_id.clone());
                }

                Ok(proto::ProduceResponse {
                    producer_id,
                    kind: req.kind,
                    encodings,
                })
            })
            .await
    }

    pub async fn consume(
        &self,
        req: proto::ConsumeRequest,
    ) -> Result<proto::ConsumeResponse, SfuError> {
        validation::validate_not_empty("request_id", &req.request_id)?;
        validation::validate_not_empty("router_id", &req.router_id)?;
        validation::validate_not_empty("subscriber_transport_id", &req.subscriber_transport_id)?;
        validation::validate_not_empty("producer_id", &req.producer_id)?;

        let rtp_capabilities = validation::parse_rtp_capabilities(&req.rtp_capabilities_json)?;

        let payload_bytes = format!(
            "{}:{}:{}:{}:{}",
            req.request_id,
            req.router_id,
            req.subscriber_transport_id,
            req.producer_id,
            req.rtp_capabilities_json
        )
        .into_bytes();

        self.consume_cache
            .execute_or_get(&req.request_id, &payload_bytes, || async {
                let router_item = self.routers.get(&req.router_id).ok_or_else(|| {
                    SfuError::NotFound(format!("Router '{}' not found", req.router_id))
                })?;

                if router_item.is_closed.load(Ordering::SeqCst) {
                    return Err(SfuError::NotFound(format!(
                        "Router '{}' is closed",
                        req.router_id
                    )));
                }

                let producer_id: ProducerId = req
                    .producer_id
                    .parse()
                    .map_err(|e| SfuError::InvalidArgument(format!("Invalid producer_id: {e}")))?;

                // Mandatory can_consume check
                if !router_item
                    .router
                    .can_consume(&producer_id, &rtp_capabilities)
                {
                    return Err(SfuError::IncompatibleCodec(format!(
                        "Router cannot consume producer '{}' with given RTP capabilities",
                        req.producer_id
                    )));
                }

                let transport_item = self
                    .transports
                    .get(&req.subscriber_transport_id)
                    .ok_or_else(|| {
                        SfuError::NotFound(format!(
                            "Transport '{}' not found",
                            req.subscriber_transport_id
                        ))
                    })?;

                if transport_item.is_closed.load(Ordering::SeqCst) {
                    return Err(SfuError::NotFound(format!(
                        "Transport '{}' is closed",
                        req.subscriber_transport_id
                    )));
                }

                let mut options = ConsumerOptions::new(producer_id, rtp_capabilities);
                options.paused = req.paused;

                let consumer = transport_item
                    .transport
                    .consume(options)
                    .await
                    .map_err(|e| SfuError::Mediasoup(format!("Failed to consume: {e}")))?;

                let consumer_id = consumer.id().to_string();
                let is_closed = Arc::new(AtomicBool::new(false));

                // Register score and layer change callbacks
                let event_tx = self.event_sender.clone();
                let r_id = req.router_id.clone();
                let c_id = consumer_id.clone();
                let p_id_str = req.producer_id.clone();
                let peer_id = transport_item.peer_id.clone();
                let routers_ref = self.routers.clone();
                let closed_ref = is_closed.clone();

                consumer
                    .on_score({
                        let r_id = r_id.clone();
                        let c_id = c_id.clone();
                        let peer_id = peer_id.clone();
                        let event_tx = event_tx.clone();
                        let routers_ref = routers_ref.clone();
                        let closed_ref = closed_ref.clone();

                        move |score| {
                            if closed_ref.load(Ordering::SeqCst) {
                                return;
                            }
                            if let Some(r) = routers_ref.get(&r_id) {
                                if r.is_closed.load(Ordering::SeqCst) {
                                    return;
                                }
                            } else {
                                return;
                            }

                            let event = proto::NodeEvent {
                                router_id: r_id.clone(),
                                timestamp_ms: Self::now_ms(),
                                payload: Some(proto::node_event::Payload::MediaScore(
                                    proto::MediaScoreEvent {
                                        resource_id: c_id.clone(),
                                        peer_id: peer_id.clone(),
                                        is_producer: false,
                                        score: score.score as u32,
                                        producer_scores: score
                                            .producer_scores
                                            .iter()
                                            .map(|&s| s as u32)
                                            .collect(),
                                    },
                                )),
                            };
                            let _ = event_tx.send(event);
                        }
                    })
                    .detach();

                let consumer_kind = consumer.kind();
                let consumer_for_closure = consumer.clone();
                consumer
                    .on_layers_change({
                        let r_id = r_id.clone();
                        let c_id = c_id.clone();
                        let p_id = p_id_str.clone();
                        let peer_id = peer_id.clone();
                        let event_tx = event_tx.clone();
                        let routers_ref = routers_ref.clone();
                        let closed_ref = closed_ref.clone();

                        move |layers| {
                            if closed_ref.load(Ordering::SeqCst) {
                                return;
                            }
                            if let Some(r) = routers_ref.get(&r_id) {
                                if r.is_closed.load(Ordering::SeqCst) {
                                    return;
                                }
                            } else {
                                return;
                            }

                            let pref = consumer_for_closure.preferred_layers();
                            let event = proto::NodeEvent {
                                router_id: r_id.clone(),
                                timestamp_ms: Self::now_ms(),
                                payload: Some(proto::node_event::Payload::LayerChanged(
                                    proto::LayerChangedEvent {
                                        consumer_id: c_id.clone(),
                                        producer_id: p_id.clone(),
                                        peer_id: peer_id.clone(),
                                        preferred_spatial_layer: pref
                                            .map(|p| p.spatial_layer as i32)
                                            .unwrap_or(-1),
                                        current_spatial_layer: layers
                                            .map(|l| l.spatial_layer as i32)
                                            .unwrap_or(-1),
                                        preferred_temporal_layer: pref
                                            .and_then(|p| p.temporal_layer)
                                            .map(|t| t as i32)
                                            .unwrap_or(-1),
                                        current_temporal_layer: layers
                                            .and_then(|l| l.temporal_layer)
                                            .map(|t| t as i32)
                                            .unwrap_or(-1),
                                    },
                                )),
                            };
                            let _ = event_tx.send(event);
                        }
                    })
                    .detach();

                let rtp_params = consumer.rtp_parameters();
                let raw_rtp_json = serde_json::to_string(rtp_params)
                    .map_err(|e| SfuError::Internal(e.to_string()))?;

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
                    .map(|enc| proto::SimulcastEncodingInfo {
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

                let item = ConsumerItem {
                    consumer,
                    router_id: req.router_id.clone(),
                    transport_id: req.subscriber_transport_id.clone(),
                    producer_id: req.producer_id.clone(),
                    peer_id: transport_item.peer_id.clone(),
                    kind: consumer_kind,
                    is_closed,
                };

                self.consumers.insert(consumer_id.clone(), item);
                if let Some(set) = self.transport_consumers.get(&req.subscriber_transport_id) {
                    set.insert(consumer_id.clone());
                }

                Ok(proto::ConsumeResponse {
                    consumer_id,
                    producer_id: req.producer_id,
                    kind: format!("{:?}", consumer_kind).to_lowercase(),
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
            })
            .await
    }

    pub async fn set_media_paused(
        &self,
        req: proto::SetMediaPausedRequest,
    ) -> Result<proto::SetMediaPausedResponse, SfuError> {
        validation::validate_not_empty("router_id", &req.router_id)?;
        validation::validate_not_empty("resource_id", &req.resource_id)?;

        match req.resource_type() {
            proto::ResourceType::Producer => {
                let producer_item = self.producers.get(&req.resource_id).ok_or_else(|| {
                    SfuError::NotFound(format!("Producer '{}' not found", req.resource_id))
                })?;

                if req.paused {
                    producer_item.producer.pause().await.map_err(|e| {
                        SfuError::Mediasoup(format!("Failed to pause producer: {e}"))
                    })?;
                } else {
                    producer_item.producer.resume().await.map_err(|e| {
                        SfuError::Mediasoup(format!("Failed to resume producer: {e}"))
                    })?;
                }
            }
            proto::ResourceType::Consumer => {
                let consumer_item = self.consumers.get(&req.resource_id).ok_or_else(|| {
                    SfuError::NotFound(format!("Consumer '{}' not found", req.resource_id))
                })?;

                if req.paused {
                    consumer_item.consumer.pause().await.map_err(|e| {
                        SfuError::Mediasoup(format!("Failed to pause consumer: {e}"))
                    })?;
                } else {
                    consumer_item.consumer.resume().await.map_err(|e| {
                        SfuError::Mediasoup(format!("Failed to resume consumer: {e}"))
                    })?;
                }
            }
            other => {
                return Err(SfuError::InvalidArgument(format!(
                    "Invalid resource_type {:?} for SetMediaPaused",
                    other
                )));
            }
        }

        Ok(proto::SetMediaPausedResponse {})
    }

    pub async fn set_consumer_preferred_layers(
        &self,
        req: proto::SetConsumerPreferredLayersRequest,
    ) -> Result<proto::SetConsumerPreferredLayersResponse, SfuError> {
        validation::validate_not_empty("router_id", &req.router_id)?;
        validation::validate_not_empty("consumer_id", &req.consumer_id)?;

        let consumer_item = self.consumers.get(&req.consumer_id).ok_or_else(|| {
            SfuError::NotFound(format!("Consumer '{}' not found", req.consumer_id))
        })?;

        let spatial_layer = if req.spatial_layer < 0 {
            0
        } else {
            req.spatial_layer as u8
        };
        let temporal_layer = if req.temporal_layer < 0 {
            None
        } else {
            Some(req.temporal_layer as u8)
        };

        consumer_item
            .consumer
            .set_preferred_layers(ConsumerLayers {
                spatial_layer,
                temporal_layer,
            })
            .await
            .map_err(|e| SfuError::Mediasoup(format!("Failed to set preferred layers: {e}")))?;

        Ok(proto::SetConsumerPreferredLayersResponse {})
    }

    pub async fn get_stats(
        &self,
        req: proto::GetStatsRequest,
    ) -> Result<proto::GetStatsResponse, SfuError> {
        validation::validate_not_empty("router_id", &req.router_id)?;
        validation::validate_not_empty("resource_id", &req.resource_id)?;

        match req.resource_type() {
            proto::ResourceType::Transport => {
                let transport_item = self.transports.get(&req.resource_id).ok_or_else(|| {
                    SfuError::NotFound(format!("Transport '{}' not found", req.resource_id))
                })?;

                let stats = transport_item.transport.get_stats().await.map_err(|e| {
                    SfuError::Mediasoup(format!("Failed to get transport stats: {e}"))
                })?;

                let stats_json =
                    serde_json::to_string(&stats).map_err(|e| SfuError::Internal(e.to_string()))?;

                let bytes_sent = stats.iter().map(|s| s.bytes_sent).sum();
                let bytes_received = stats.iter().map(|s| s.bytes_received).sum();
                let packets_sent = 0;
                let packets_received = 0;

                Ok(proto::GetStatsResponse {
                    stats_json,
                    bytes_sent,
                    bytes_received,
                    packets_sent,
                    packets_received,
                    current_spatial_layer: -1,
                    current_temporal_layer: -1,
                })
            }
            proto::ResourceType::Producer => {
                let producer_item = self.producers.get(&req.resource_id).ok_or_else(|| {
                    SfuError::NotFound(format!("Producer '{}' not found", req.resource_id))
                })?;

                let stats = producer_item.producer.get_stats().await.map_err(|e| {
                    SfuError::Mediasoup(format!("Failed to get producer stats: {e}"))
                })?;

                let stats_json =
                    serde_json::to_string(&stats).map_err(|e| SfuError::Internal(e.to_string()))?;

                let bytes_sent = 0;
                let bytes_received = stats.iter().map(|s| s.byte_count).sum();
                let packets_sent = 0;
                let packets_received = stats.iter().map(|s| s.packet_count).sum();

                Ok(proto::GetStatsResponse {
                    stats_json,
                    bytes_sent,
                    bytes_received,
                    packets_sent,
                    packets_received,
                    current_spatial_layer: -1,
                    current_temporal_layer: -1,
                })
            }
            proto::ResourceType::Consumer => {
                let consumer_item = self.consumers.get(&req.resource_id).ok_or_else(|| {
                    SfuError::NotFound(format!("Consumer '{}' not found", req.resource_id))
                })?;

                let stats = consumer_item.consumer.get_stats().await.map_err(|e| {
                    SfuError::Mediasoup(format!("Failed to get consumer stats: {e}"))
                })?;

                let stats_json =
                    serde_json::to_string(&stats).map_err(|e| SfuError::Internal(e.to_string()))?;

                let (bytes_sent, packets_sent) =
                    stats.iter().fold((0u64, 0u64), |(b, p), s| match s {
                        ConsumerRtpStreamStat::Outbound(out) => {
                            (b + out.byte_count, p + out.packet_count)
                        }
                        ConsumerRtpStreamStat::Inbound(in_b) => {
                            (b + in_b.byte_count, p + in_b.packet_count)
                        }
                    });
                let bytes_received = 0;
                let packets_received = 0;

                let current_layers = consumer_item.consumer.current_layers();
                let current_spatial_layer =
                    current_layers.map(|l| l.spatial_layer as i32).unwrap_or(-1);
                let current_temporal_layer = current_layers
                    .and_then(|l| l.temporal_layer)
                    .map(|t| t as i32)
                    .unwrap_or(-1);

                Ok(proto::GetStatsResponse {
                    stats_json,
                    bytes_sent,
                    bytes_received,
                    packets_sent,
                    packets_received,
                    current_spatial_layer,
                    current_temporal_layer,
                })
            }
            other => Err(SfuError::InvalidArgument(format!(
                "Invalid resource_type {:?} for GetStats",
                other
            ))),
        }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<proto::NodeEvent> {
        self.event_sender.subscribe()
    }
}
