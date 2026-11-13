use crate::application::state::{DataConsumerItem, DataProducerItem, EngineState};
use crate::config::Config;
use crate::domain::errors::SfuDomainError;
use crate::domain::idempotency::IdempotencyCache;
use crate::domain::validation;
use crate::ports::inbound::*;
use crate::ports::outbound::{EventPublisherPort, MetricsPort, WorkerPoolPort};
use async_trait::async_trait;
use mediasoup::data_consumer::DataConsumerOptions;
use mediasoup::data_producer::{DataProducerId, DataProducerOptions};
use mediasoup::direct_transport::DirectTransportOptions;
use mediasoup::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
enum AnyDataTransport {
    WebRtc(WebRtcTransport),
    Plain(mediasoup::plain_transport::PlainTransport),
}

impl AnyDataTransport {
    async fn produce_data(&self, options: DataProducerOptions) -> Result<DataProducer, String> {
        match self {
            Self::WebRtc(t) => t.produce_data(options).await.map_err(|e| e.to_string()),
            Self::Plain(t) => t.produce_data(options).await.map_err(|e| e.to_string()),
        }
    }

    async fn consume_data(&self, options: DataConsumerOptions) -> Result<DataConsumer, String> {
        match self {
            Self::WebRtc(t) => t.consume_data(options).await.map_err(|e| e.to_string()),
            Self::Plain(t) => t.consume_data(options).await.map_err(|e| e.to_string()),
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

pub struct DataService {
    _config: Config,
    state: EngineState,
    _worker_pool: Arc<dyn WorkerPoolPort>,
    event_publisher: Arc<dyn EventPublisherPort>,
    metrics: Arc<dyn MetricsPort>,
    produce_data_cache: Arc<IdempotencyCache<ProduceDataResult>>,
    consume_data_cache: Arc<IdempotencyCache<ConsumeDataResult>>,
    cleanup_handles: Vec<tokio::task::AbortHandle>,
}

impl DataService {
    pub fn new(
        config: Config,
        state: EngineState,
        worker_pool: Arc<dyn WorkerPoolPort>,
        event_publisher: Arc<dyn EventPublisherPort>,
        metrics: Arc<dyn MetricsPort>,
    ) -> Self {
        let produce_data_cache = Arc::new(IdempotencyCache::default());
        let consume_data_cache = Arc::new(IdempotencyCache::default());

        let p_clean = produce_data_cache.start_periodic_cleanup(Duration::from_secs(30));
        let c_clean = consume_data_cache.start_periodic_cleanup(Duration::from_secs(30));

        Self {
            _config: config,
            state,
            _worker_pool: worker_pool,
            event_publisher,
            metrics,
            produce_data_cache,
            consume_data_cache,
            cleanup_handles: vec![p_clean.abort_handle(), c_clean.abort_handle()],
        }
    }

    pub fn abort_cleanup_tasks(&self) {
        for handle in &self.cleanup_handles {
            handle.abort();
        }
    }

    fn resolve_data_transport(
        &self,
        router_id: &str,
        transport_id: &str,
    ) -> Result<(AnyDataTransport, String, usize), SfuDomainError> {
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
                AnyDataTransport::WebRtc(t_item.transport.clone()),
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
                AnyDataTransport::Plain(pt_item.transport.clone()),
                pt_item.router_id.clone(),
                pt_item.worker_index,
            ));
        }

        Err(SfuDomainError::NotFound(format!(
            "Transport '{transport_id}' not found"
        )))
    }
}

#[async_trait]
impl DataUseCase for DataService {
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
    async fn produce_data(
        &self,
        params: ProduceDataParams,
    ) -> Result<ProduceDataResult, SfuDomainError> {
        if let Some(room_id) = self.state.get_room_id_for_router(&params.router_id) {
            tracing::Span::current().record("room_id", room_id.as_str());
        }
        if self.state.is_draining.load(Ordering::SeqCst) {
            return Err(SfuDomainError::SystemDraining(
                "Node is draining; rejecting new data producer".to_string(),
            ));
        }

        validation::validate_not_empty("request_id", &params.request_id)?;
        validation::validate_not_empty("router_id", &params.router_id)?;
        validation::validate_not_empty("transport_id", &params.transport_id)?;
        validation::validate_not_empty("peer_id", &params.peer_id)?;

        let payload_bytes = format!(
            "{}:{}:{}:{}:{}:{}",
            params.request_id,
            params.router_id,
            params.transport_id,
            params.peer_id,
            params.label,
            params.protocol
        )
        .into_bytes();

        let state_for_valid = self.state.clone();
        let router_id_valid = params.router_id.clone();
        let metrics_ref = self.metrics.clone();

        self.produce_data_cache
            .execute_or_get_validated(
                &params.request_id,
                &payload_bytes,
                move |cached: &ProduceDataResult| {
                    state_for_valid
                        .data_producers
                        .get(&(router_id_valid.clone(), cached.data_producer_id.clone()))
                        .map(|dp| !dp.is_closed.load(Ordering::SeqCst))
                        .unwrap_or(false)
                },
                || async {
                    let (transport, physical_router_id, worker_index) =
                        self.resolve_data_transport(&params.router_id, &params.transport_id)?;

                    let sctp_params: Option<SctpStreamParameters> =
                        if !params.sctp_stream_parameters_json.trim().is_empty() {
                            serde_json::from_str(&params.sctp_stream_parameters_json).map_err(
                                |e| {
                                    SfuDomainError::InvalidArgument(format!(
                                        "Invalid sctp_stream_parameters_json: {e}"
                                    ))
                                },
                            )?
                        } else {
                            None
                        };

                    let mut options = if let Some(sctp) = sctp_params {
                        DataProducerOptions::new_sctp(sctp)
                    } else {
                        DataProducerOptions::new_direct()
                    };

                    if !params.label.is_empty() {
                        options.label = params.label.clone();
                    }
                    if !params.protocol.is_empty() {
                        options.protocol = params.protocol.clone();
                    }

                    let data_producer = transport.produce_data(options).await.map_err(|e| {
                        SfuDomainError::Mediasoup(format!("Failed to produce data: {e}"))
                    })?;

                    if !self.state.is_router_active(&params.router_id) {
                        drop(data_producer);
                        return Err(SfuDomainError::NotFound(format!(
                            "Router '{}' was closed during produce_data",
                            params.router_id
                        )));
                    }

                    if !transport.is_active_in_state(&self.state, &params.transport_id) {
                        drop(data_producer);
                        return Err(SfuDomainError::NotFound(format!(
                            "Transport '{}' was closed during produce_data",
                            params.transport_id
                        )));
                    }

                    let data_producer_id = data_producer.id().to_string();
                    let sctp_json = data_producer
                        .sctp_stream_parameters()
                        .and_then(|s| serde_json::to_string(&s).ok())
                        .unwrap_or_default();

                    let item = DataProducerItem {
                        data_producer,
                        router_id: params.router_id.clone(),
                        physical_router_id,
                        worker_index,
                        transport_id: params.transport_id.clone(),
                        peer_id: params.peer_id.clone(),
                        label: params.label.clone(),
                        protocol: params.protocol.clone(),
                        is_closed: Arc::new(AtomicBool::new(false)),
                    };

                    self.state
                        .data_producers
                        .insert((params.router_id.clone(), data_producer_id.clone()), item);
                    self.state
                        .transport_data_producers
                        .entry(params.transport_id.clone())
                        .or_default()
                        .insert(data_producer_id.clone());

                    Ok(ProduceDataResult {
                        data_producer_id,
                        label: params.label,
                        protocol: params.protocol,
                        sctp_stream_parameters_json: sctp_json,
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
            data_producer_id = %params.data_producer_id,
            request_id = %params.request_id
        )
    )]
    async fn consume_data(
        &self,
        params: ConsumeDataParams,
    ) -> Result<ConsumeDataResult, SfuDomainError> {
        if let Some(room_id) = self.state.get_room_id_for_router(&params.router_id) {
            tracing::Span::current().record("room_id", room_id.as_str());
        }
        if self.state.is_draining.load(Ordering::SeqCst) {
            return Err(SfuDomainError::SystemDraining(
                "Node is draining; rejecting new data consumer".to_string(),
            ));
        }

        validation::validate_not_empty("request_id", &params.request_id)?;
        validation::validate_not_empty("router_id", &params.router_id)?;
        validation::validate_not_empty("transport_id", &params.transport_id)?;
        validation::validate_not_empty("data_producer_id", &params.data_producer_id)?;

        let payload_bytes = format!(
            "{}:{}:{}:{}",
            params.request_id, params.router_id, params.transport_id, params.data_producer_id
        )
        .into_bytes();

        let state_for_valid = self.state.clone();
        let metrics_ref = self.metrics.clone();

        self.consume_data_cache
            .execute_or_get_validated(
                &params.request_id,
                &payload_bytes,
                move |cached: &ConsumeDataResult| {
                    state_for_valid
                        .data_consumers
                        .get(&cached.data_consumer_id)
                        .map(|dc| !dc.is_closed.load(Ordering::SeqCst))
                        .unwrap_or(false)
                },
                || async {
                    let (transport, sub_physical_router_id, sub_worker_index) =
                        self.resolve_data_transport(&params.router_id, &params.transport_id)?;

                    let (label, protocol, pub_physical_router_id, pub_worker_index) = {
                        let data_producer_item = self
                            .state
                            .data_producers
                            .get(&(params.router_id.clone(), params.data_producer_id.clone()))
                            .ok_or_else(|| {
                                SfuDomainError::NotFound(format!(
                                    "DataProducer '{}' not found",
                                    params.data_producer_id
                                ))
                            })?;

                        if data_producer_item.is_closed.load(Ordering::SeqCst) {
                            return Err(SfuDomainError::NotFound(format!(
                                "DataProducer '{}' is closed",
                                params.data_producer_id
                            )));
                        }

                        (
                            data_producer_item.label.clone(),
                            data_producer_item.protocol.clone(),
                            data_producer_item.physical_router_id.clone(),
                            data_producer_item.worker_index,
                        )
                    };

                    let data_prod_id: DataProducerId =
                        params.data_producer_id.parse().map_err(|e| {
                            SfuDomainError::InvalidArgument(format!(
                                "Invalid data_producer_id: {e}"
                            ))
                        })?;

                    if pub_physical_router_id != sub_physical_router_id {
                        let already_piped = self
                            .state
                            .piped_data_producers
                            .get(&params.data_producer_id)
                            .map(|entries| {
                                entries.iter().any(|e| {
                                    e.router_id == params.router_id
                                        && e.target_router_id == sub_physical_router_id
                                })
                            })
                            .unwrap_or(false);

                        if !already_piped {
                            let (pub_router, sub_router) = {
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

                                let pub_r = if pub_worker_index == r_item.worker_index {
                                    r_item.router.clone()
                                } else {
                                    r_item
                                        .sub_routers
                                        .get(&pub_worker_index)
                                        .map(|s| s.router.clone())
                                        .ok_or_else(|| {
                                            SfuDomainError::NotFound(format!(
                                                "Sub-router on worker {pub_worker_index} not found"
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

                            let pipe_pair = pub_router
                                .pipe_data_producer_to_router(
                                    data_prod_id,
                                    mediasoup::router::PipeToRouterOptions::new(sub_router),
                                )
                                .await
                                .map_err(|e| {
                                    SfuDomainError::Mediasoup(format!(
                                        "Failed to pipe data producer to router: {e}"
                                    ))
                                })?;

                            let pipe_producer = pipe_pair.pipe_data_producer.into_inner();
                            let entry = crate::application::state::PipedDataProducerEntry {
                                router_id: params.router_id.clone(),
                                pipe_data_consumer: pipe_pair.pipe_data_consumer,
                                pipe_data_producer: pipe_producer,
                                target_router_id: sub_physical_router_id.clone(),
                            };

                            self.state
                                .piped_data_producers
                                .entry(params.data_producer_id.clone())
                                .or_default()
                                .push(entry);
                        }
                    }

                    let options = DataConsumerOptions::new_sctp(data_prod_id);
                    let data_consumer = transport.consume_data(options).await.map_err(|e| {
                        SfuDomainError::Mediasoup(format!("Failed to consume data: {e}"))
                    })?;

                    if !self.state.is_router_active(&params.router_id) {
                        drop(data_consumer);
                        return Err(SfuDomainError::NotFound(format!(
                            "Router '{}' was closed during consume_data",
                            params.router_id
                        )));
                    }

                    if !transport.is_active_in_state(&self.state, &params.transport_id) {
                        drop(data_consumer);
                        return Err(SfuDomainError::NotFound(format!(
                            "Transport '{}' was closed during consume_data",
                            params.transport_id
                        )));
                    }

                    let data_consumer_id = data_consumer.id().to_string();
                    let sctp_json = data_consumer
                        .sctp_stream_parameters()
                        .and_then(|s| serde_json::to_string(&s).ok())
                        .unwrap_or_default();

                    let item = DataConsumerItem {
                        data_consumer,
                        router_id: params.router_id.clone(),
                        transport_id: params.transport_id.clone(),
                        data_producer_id: params.data_producer_id.clone(),
                        label: label.clone(),
                        protocol: protocol.clone(),
                        is_closed: Arc::new(AtomicBool::new(false)),
                    };

                    self.state
                        .data_consumers
                        .insert(data_consumer_id.clone(), item);
                    self.state
                        .transport_data_consumers
                        .entry(params.transport_id.clone())
                        .or_default()
                        .insert(data_consumer_id.clone());

                    Ok(ConsumeDataResult {
                        data_consumer_id,
                        data_producer_id: params.data_producer_id,
                        label,
                        protocol,
                        sctp_stream_parameters_json: sctp_json,
                    })
                },
                move || {
                    metrics_ref.record_idempotency_hit();
                },
            )
            .await
    }

    #[tracing::instrument(
        skip(self, payload),
        fields(
            room_id = tracing::field::Empty,
            router_id = %router_id,
            label = %label,
            protocol = %protocol,
            bytes = payload.len()
        )
    )]
    async fn send_direct_data(
        &self,
        router_id: &str,
        label: &str,
        protocol: &str,
        payload: &[u8],
    ) -> Result<(), SfuDomainError> {
        if let Some(room_id) = self.state.get_room_id_for_router(router_id) {
            tracing::Span::current().record("room_id", room_id.as_str());
        }
        validation::validate_not_empty("router_id", router_id)?;

        let router = {
            let r_item = self.state.routers.get(router_id).ok_or_else(|| {
                SfuDomainError::NotFound(format!("Router '{router_id}' not found"))
            })?;

            if r_item.is_closed.load(Ordering::SeqCst) {
                return Err(SfuDomainError::NotFound(format!(
                    "Router '{router_id}' is closed"
                )));
            }

            r_item.router.clone()
        };

        let direct_transport = match self.state.direct_transports.get(router_id) {
            Some(dt) => dt.clone(),
            None => {
                let dt = router
                    .create_direct_transport(DirectTransportOptions::default())
                    .await
                    .map_err(|e| {
                        SfuDomainError::Mediasoup(format!("Failed to create DirectTransport: {e}"))
                    })?;
                self.state
                    .direct_transports
                    .insert(router_id.to_string(), dt.clone());
                dt
            }
        };

        let key = (
            router_id.to_string(),
            label.to_string(),
            protocol.to_string(),
        );

        let direct_producer = match self.state.direct_data_producers.get(&key) {
            Some(dp) => dp.clone(),
            None => {
                let mut prod_opts = DataProducerOptions::new_direct();
                if !label.is_empty() {
                    prod_opts.label = label.to_string();
                }
                if !protocol.is_empty() {
                    prod_opts.protocol = protocol.to_string();
                }

                let dp_generic = direct_transport
                    .produce_data(prod_opts)
                    .await
                    .map_err(|e| {
                        SfuDomainError::Mediasoup(format!("Failed to produce direct data: {e}"))
                    })?;

                let dp = match dp_generic {
                    DataProducer::Direct(dp) => dp,
                    _ => {
                        return Err(SfuDomainError::Internal(
                            "Expected DirectDataProducer".to_string(),
                        ));
                    }
                };

                self.state.direct_data_producers.insert(key, dp.clone());
                dp
            }
        };

        direct_producer
            .send(
                WebRtcMessage::Binary(payload.to_vec().into()),
                None,
                None,
                None,
            )
            .map_err(|e| {
                SfuDomainError::Mediasoup(format!("Failed to send direct message: {e}"))
            })?;

        Ok(())
    }

    fn close_data_producer(
        &self,
        router_id: &str,
        data_producer_id: &str,
    ) -> Result<(), SfuDomainError> {
        if let Some((_, dp)) = self
            .state
            .data_producers
            .remove(&(router_id.to_string(), data_producer_id.to_string()))
        {
            dp.is_closed.store(true, Ordering::SeqCst);
            if let Some(set) = self
                .state
                .transport_data_producers
                .get_mut(&dp.transport_id)
            {
                set.remove(data_producer_id);
            }
            self.state.piped_data_producers.remove(data_producer_id);
            self.event_publisher.publish_resource_closed(
                router_id,
                "data_producer",
                data_producer_id,
                "explicit_close",
            );
        }
        Ok(())
    }

    fn close_data_consumer(
        &self,
        router_id: &str,
        data_consumer_id: &str,
    ) -> Result<(), SfuDomainError> {
        if let Some((_, dc)) = self.state.data_consumers.remove(data_consumer_id) {
            dc.is_closed.store(true, Ordering::SeqCst);
            if let Some(set) = self
                .state
                .transport_data_consumers
                .get_mut(&dc.transport_id)
            {
                set.remove(data_consumer_id);
            }
            self.event_publisher.publish_resource_closed(
                router_id,
                "data_consumer",
                data_consumer_id,
                "explicit_close",
            );
        }
        Ok(())
    }
}
