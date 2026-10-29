use crate::adapters::inbound_grpc::mapper;
use crate::application::MediaNode;
use crate::ports::inbound::{MediaUseCase, RouterUseCase, TransportUseCase};
use crate::ports::outbound::MetricsPort;
use crate::proto::sfu::v1 as proto;
use crate::proto::sfu::v1::sfu_media_engine_service_server::SfuMediaEngineService;
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status};
use tracing::Instrument;

#[derive(Clone)]
pub struct GrpcMediaEngineService {
    node: Arc<MediaNode>,
}

impl GrpcMediaEngineService {
    pub fn new(node: Arc<MediaNode>) -> Self {
        Self { node }
    }
}

struct TracedStream<S> {
    inner: S,
    span: tracing::Span,
}

impl<S: Stream + Unpin> Stream for TracedStream<S> {
    type Item = S::Item;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.as_mut().get_mut();
        let span = &this.span;
        let inner = &mut this.inner;
        span.in_scope(|| Pin::new(inner).poll_next(cx))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

async fn instrument_rpc<Req, Resp, Fut>(
    method: &'static str,
    metrics: Arc<dyn MetricsPort>,
    request: Request<Req>,
    handler: impl FnOnce(Req) -> Fut,
) -> Result<Response<Resp>, Status>
where
    Fut: std::future::Future<Output = Result<Resp, Status>>,
{
    let start = std::time::Instant::now();
    let span = tracing::info_span!(
        "grpc_call",
        method = method,
        trace_id = tracing::field::Empty,
        span_id = tracing::field::Empty,
        request_id = tracing::field::Empty,
        room_id = tracing::field::Empty,
        peer_id = tracing::field::Empty,
        status = tracing::field::Empty,
    );

    crate::observability::tracing_extractor::attach_trace_context(&span, request.metadata());
    let req = request.into_inner();

    let res = handler(req).instrument(span.clone()).await;
    let duration = start.elapsed().as_secs_f64();

    let status_str = match &res {
        Ok(_) => "ok",
        Err(status) => match status.code() {
            tonic::Code::InvalidArgument => "invalid_argument",
            tonic::Code::NotFound => "not_found",
            tonic::Code::AlreadyExists => "already_exists",
            tonic::Code::FailedPrecondition => "failed_precondition",
            tonic::Code::Aborted => "aborted",
            tonic::Code::Unavailable => "unavailable",
            tonic::Code::Unimplemented => "unimplemented",
            tonic::Code::Internal => "internal",
            _ => "error",
        },
    };

    span.record("status", status_str);
    metrics.record_grpc_request(method, status_str, duration);

    res.map(Response::new)
}

#[tonic::async_trait]
impl SfuMediaEngineService for GrpcMediaEngineService {
    type WatchNodeEventsStream =
        Pin<Box<dyn Stream<Item = Result<proto::NodeEvent, Status>> + Send + 'static>>;

    async fn create_router(
        &self,
        request: Request<proto::CreateRouterRequest>,
    ) -> Result<Response<proto::CreateRouterResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc(
            "CreateRouter",
            self.node.metrics.clone(),
            request,
            move |req| async move {
                tracing::Span::current().record("request_id", &req.request_id);
                tracing::Span::current().record("room_id", &req.room_id);
                let (router_id, worker_index, caps_json) = node
                    .router_service
                    .create_router(&req.request_id, &req.room_id)
                    .await
                    .map_err(Status::from)?;
                Ok(mapper::to_create_router_response(
                    router_id,
                    worker_index,
                    caps_json,
                ))
            },
        )
        .await
    }

    async fn create_transport(
        &self,
        request: Request<proto::CreateTransportRequest>,
    ) -> Result<Response<proto::CreateTransportResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc(
            "CreateTransport",
            self.node.metrics.clone(),
            request,
            move |req| async move {
                tracing::Span::current().record("request_id", &req.request_id);
                tracing::Span::current().record("peer_id", &req.peer_id);
                if let Some(room_id) = node.state.get_room_id_for_router(&req.router_id) {
                    tracing::Span::current().record("room_id", &room_id);
                }
                let params = mapper::from_create_transport_request(req);
                let res = node
                    .transport_service
                    .create_transport(params)
                    .await
                    .map_err(Status::from)?;
                Ok(mapper::to_create_transport_response(res))
            },
        )
        .await
    }

    async fn connect_transport(
        &self,
        request: Request<proto::ConnectTransportRequest>,
    ) -> Result<Response<proto::ConnectTransportResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc(
            "ConnectTransport",
            self.node.metrics.clone(),
            request,
            move |req| async move {
                if let Some(t) = node.state.transports.get(&req.transport_id) {
                    tracing::Span::current().record("peer_id", &t.peer_id);
                }
                if let Some(room_id) = node.state.get_room_id_for_router(&req.router_id) {
                    tracing::Span::current().record("room_id", &room_id);
                }
                let params = mapper::from_connect_transport_request(req).map_err(Status::from)?;
                node.transport_service
                    .connect_transport(params)
                    .await
                    .map_err(Status::from)?;
                Ok(proto::ConnectTransportResponse {})
            },
        )
        .await
    }

    async fn restart_ice(
        &self,
        request: Request<proto::RestartIceRequest>,
    ) -> Result<Response<proto::RestartIceResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc(
            "RestartIce",
            self.node.metrics.clone(),
            request,
            move |req| async move {
                if let Some(t) = node.state.transports.get(&req.transport_id) {
                    tracing::Span::current().record("peer_id", &t.peer_id);
                }
                if let Some(room_id) = node.state.get_room_id_for_router(&req.router_id) {
                    tracing::Span::current().record("room_id", &room_id);
                }
                let res = node
                    .transport_service
                    .restart_ice(&req.router_id, &req.transport_id)
                    .await
                    .map_err(Status::from)?;
                Ok(mapper::to_restart_ice_response(res))
            },
        )
        .await
    }

    async fn produce(
        &self,
        request: Request<proto::ProduceRequest>,
    ) -> Result<Response<proto::ProduceResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc(
            "Produce",
            self.node.metrics.clone(),
            request,
            move |req| async move {
                tracing::Span::current().record("request_id", &req.request_id);
                tracing::Span::current().record("peer_id", &req.peer_id);
                if let Some(room_id) = node.state.get_room_id_for_router(&req.router_id) {
                    tracing::Span::current().record("room_id", &room_id);
                }
                let params = mapper::from_produce_request(req).map_err(Status::from)?;
                let res = node
                    .media_service
                    .produce(params)
                    .await
                    .map_err(Status::from)?;
                Ok(mapper::to_produce_response(res))
            },
        )
        .await
    }

    async fn consume(
        &self,
        request: Request<proto::ConsumeRequest>,
    ) -> Result<Response<proto::ConsumeResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc(
            "Consume",
            self.node.metrics.clone(),
            request,
            move |req| async move {
                tracing::Span::current().record("request_id", &req.request_id);
                if let Some(t) = node.state.transports.get(&req.subscriber_transport_id) {
                    tracing::Span::current().record("peer_id", &t.peer_id);
                }
                if let Some(room_id) = node.state.get_room_id_for_router(&req.router_id) {
                    tracing::Span::current().record("room_id", &room_id);
                }
                let params = mapper::from_consume_request(req).map_err(Status::from)?;
                let res = node
                    .media_service
                    .consume(params)
                    .await
                    .map_err(Status::from)?;
                Ok(mapper::to_consume_response(res))
            },
        )
        .await
    }

    async fn set_media_paused(
        &self,
        request: Request<proto::SetMediaPausedRequest>,
    ) -> Result<Response<proto::SetMediaPausedResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc(
            "SetMediaPaused",
            self.node.metrics.clone(),
            request,
            move |req| async move {
                if let Some(room_id) = node.state.get_room_id_for_router(&req.router_id) {
                    tracing::Span::current().record("room_id", &room_id);
                }
                let is_producer = match req.resource_type() {
                    proto::ResourceType::Producer => {
                        if let Some(p) = node.state.producers.get(&req.resource_id) {
                            tracing::Span::current().record("peer_id", &p.peer_id);
                        }
                        true
                    }
                    proto::ResourceType::Consumer => {
                        if let Some(c) = node.state.consumers.get(&req.resource_id) {
                            tracing::Span::current().record("peer_id", &c.peer_id);
                        }
                        false
                    }
                    _ => {
                        return Err(Status::invalid_argument(
                            "SetMediaPaused requires Producer or Consumer ResourceType",
                        ));
                    }
                };
                node.media_service
                    .set_media_paused(&req.router_id, &req.resource_id, is_producer, req.paused)
                    .await
                    .map_err(Status::from)?;
                Ok(proto::SetMediaPausedResponse {})
            },
        )
        .await
    }

    async fn set_consumer_preferred_layers(
        &self,
        request: Request<proto::SetConsumerPreferredLayersRequest>,
    ) -> Result<Response<proto::SetConsumerPreferredLayersResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc(
            "SetConsumerPreferredLayers",
            self.node.metrics.clone(),
            request,
            move |req| async move {
                if let Some(room_id) = node.state.get_room_id_for_router(&req.router_id) {
                    tracing::Span::current().record("room_id", &room_id);
                }
                if let Some(c) = node.state.consumers.get(&req.consumer_id) {
                    tracing::Span::current().record("peer_id", &c.peer_id);
                }
                let (router_id, consumer_id, layers) =
                    mapper::from_set_consumer_preferred_layers_request(req)
                        .map_err(Status::from)?;
                node.media_service
                    .set_consumer_preferred_layers(&router_id, &consumer_id, layers)
                    .await
                    .map_err(Status::from)?;
                Ok(proto::SetConsumerPreferredLayersResponse {})
            },
        )
        .await
    }

    async fn close_resource(
        &self,
        request: Request<proto::CloseResourceRequest>,
    ) -> Result<Response<proto::CloseResourceResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc(
            "CloseResource",
            self.node.metrics.clone(),
            request,
            move |req| async move {
                let router_id_for_lookup = if !req.router_id.is_empty() {
                    &req.router_id
                } else if req.resource_type() == proto::ResourceType::Router {
                    &req.resource_id
                } else {
                    ""
                };
                if !router_id_for_lookup.is_empty() {
                    if let Some(room_id) = node.state.get_room_id_for_router(router_id_for_lookup) {
                        tracing::Span::current().record("room_id", &room_id);
                    }
                }
                match req.resource_type() {
                    proto::ResourceType::Router => {
                        if !req.router_id.is_empty()
                            && !req.resource_id.is_empty()
                            && req.router_id != req.resource_id
                        {
                            return Err(Status::invalid_argument(
                                "router_id and resource_id must match when closing a Router",
                            ));
                        }
                        let r_id = if !req.resource_id.is_empty() {
                            &req.resource_id
                        } else {
                            &req.router_id
                        };
                        node.router_service
                            .close_router(r_id)
                            .map_err(Status::from)?;
                    }
                    proto::ResourceType::Transport => {
                        if let Some(t) = node.state.transports.get(&req.resource_id) {
                            tracing::Span::current().record("peer_id", &t.peer_id);
                        }
                        node.transport_service
                            .close_transport(&req.router_id, &req.resource_id)
                            .map_err(Status::from)?;
                    }
                    proto::ResourceType::Producer => {
                        if let Some(p) = node.state.producers.get(&req.resource_id) {
                            tracing::Span::current().record("peer_id", &p.peer_id);
                        }
                        node.media_service
                            .close_producer(&req.router_id, &req.resource_id)
                            .map_err(Status::from)?;
                    }
                    proto::ResourceType::Consumer => {
                        if let Some(c) = node.state.consumers.get(&req.resource_id) {
                            tracing::Span::current().record("peer_id", &c.peer_id);
                        }
                        node.media_service
                            .close_consumer(&req.router_id, &req.resource_id)
                            .map_err(Status::from)?;
                    }
                    proto::ResourceType::Unspecified => {
                        return Err(Status::invalid_argument("Resource type unspecified"));
                    }
                }
                Ok(proto::CloseResourceResponse {})
            },
        )
        .await
    }

    async fn get_stats(
        &self,
        request: Request<proto::GetStatsRequest>,
    ) -> Result<Response<proto::GetStatsResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc(
            "GetStats",
            self.node.metrics.clone(),
            request,
            move |req| async move {
                if let Some(room_id) = node.state.get_room_id_for_router(&req.router_id) {
                    tracing::Span::current().record("room_id", &room_id);
                }
                match req.resource_type() {
                    proto::ResourceType::Transport => {
                        if let Some(t) = node.state.transports.get(&req.resource_id) {
                            tracing::Span::current().record("peer_id", &t.peer_id);
                        }
                    }
                    proto::ResourceType::Producer => {
                        if let Some(p) = node.state.producers.get(&req.resource_id) {
                            tracing::Span::current().record("peer_id", &p.peer_id);
                        }
                    }
                    proto::ResourceType::Consumer => {
                        if let Some(c) = node.state.consumers.get(&req.resource_id) {
                            tracing::Span::current().record("peer_id", &c.peer_id);
                        }
                    }
                    _ => {}
                }
                let r_type = match req.resource_type() {
                    proto::ResourceType::Transport => "transport",
                    proto::ResourceType::Producer => "producer",
                    proto::ResourceType::Consumer => "consumer",
                    _ => {
                        return Err(Status::invalid_argument(
                            "GetStats requires Transport, Producer, or Consumer ResourceType",
                        ));
                    }
                };
                let stats = node
                    .media_service
                    .get_stats(&req.router_id, &req.resource_id, r_type)
                    .await
                    .map_err(Status::from)?;
                Ok(mapper::to_get_stats_response(stats))
            },
        )
        .await
    }

    async fn watch_node_events(
        &self,
        request: Request<proto::WatchNodeEventsRequest>,
    ) -> Result<Response<Self::WatchNodeEventsStream>, Status> {
        let start = std::time::Instant::now();
        let span = tracing::info_span!(
            "grpc_call",
            method = "WatchNodeEvents",
            trace_id = tracing::field::Empty,
            span_id = tracing::field::Empty,
            request_id = tracing::field::Empty,
            room_id = tracing::field::Empty,
            peer_id = tracing::field::Empty,
            status = "ok",
        );
        crate::observability::tracing_extractor::attach_trace_context(&span, request.metadata());

        let rx = self.node.event_hub.subscribe();
        let node_ref = self.node.clone();
        let stream = BroadcastStream::new(rx).filter_map(move |item| match item {
            Ok(event) => {
                let is_passthrough = matches!(
                    event.payload,
                    Some(crate::domain::entities::DomainNodeEventPayload::ResourceClosed(_))
                        | Some(crate::domain::entities::DomainNodeEventPayload::NodeCapacity(_))
                );
                if is_passthrough || node_ref.is_router_active(&event.router_id) {
                    Some(Ok(mapper::to_proto_node_event(event)))
                } else {
                    tracing::debug!(
                        router_id = %event.router_id,
                        "Filtered out zombie event for closed router"
                    );
                    None
                }
            }
            Err(e) => {
                tracing::warn!("Event stream skipped lagged items: {e}");
                None
            }
        });

        self.node.metrics.record_grpc_request(
            "WatchNodeEvents",
            "ok",
            start.elapsed().as_secs_f64(),
        );

        let traced_stream = TracedStream {
            inner: stream,
            span,
        };

        Ok(Response::new(Box::pin(traced_stream)))
    }

    async fn get_node_capacity(
        &self,
        request: Request<proto::GetNodeCapacityRequest>,
    ) -> Result<Response<proto::NodeCapacityMetrics>, Status> {
        let node = self.node.clone();
        instrument_rpc(
            "GetNodeCapacity",
            self.node.metrics.clone(),
            request,
            move |_req| async move {
                Ok(mapper::to_node_capacity_metrics(
                    node.get_node_capacity_info(),
                ))
            },
        )
        .await
    }

    async fn create_pipe_transport(
        &self,
        request: Request<proto::CreatePipeTransportRequest>,
    ) -> Result<Response<proto::CreatePipeTransportResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc(
            "CreatePipeTransport",
            self.node.metrics.clone(),
            request,
            move |req| async move {
                tracing::Span::current().record("request_id", &req.request_id);
                if let Some(room_id) = node.state.get_room_id_for_router(&req.router_id) {
                    tracing::Span::current().record("room_id", &room_id);
                }
                let params = mapper::from_create_pipe_transport_request(req);
                let res = node
                    .pipe_service
                    .create_pipe_transport(params)
                    .await
                    .map_err(Status::from)?;
                Ok(mapper::to_create_pipe_transport_response(res))
            },
        )
        .await
    }

    async fn connect_pipe_transport(
        &self,
        request: Request<proto::ConnectPipeTransportRequest>,
    ) -> Result<Response<proto::ConnectPipeTransportResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc(
            "ConnectPipeTransport",
            self.node.metrics.clone(),
            request,
            move |req| async move {
                if let Some(room_id) = node.state.get_room_id_for_router(&req.router_id) {
                    tracing::Span::current().record("room_id", &room_id);
                }
                let params =
                    mapper::from_connect_pipe_transport_request(req).map_err(Status::from)?;
                node.pipe_service
                    .connect_pipe_transport(params)
                    .await
                    .map_err(Status::from)?;
                Ok(proto::ConnectPipeTransportResponse {})
            },
        )
        .await
    }

    async fn pipe_produce(
        &self,
        request: Request<proto::PipeProduceRequest>,
    ) -> Result<Response<proto::ProduceResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc(
            "PipeProduce",
            self.node.metrics.clone(),
            request,
            move |req| async move {
                tracing::Span::current().record("request_id", &req.request_id);
                if let Some(room_id) = node.state.get_room_id_for_router(&req.router_id) {
                    tracing::Span::current().record("room_id", &room_id);
                }
                let params = mapper::from_pipe_produce_request(req).map_err(Status::from)?;
                let res = node
                    .pipe_service
                    .pipe_produce(params)
                    .await
                    .map_err(Status::from)?;
                Ok(mapper::to_produce_response(res))
            },
        )
        .await
    }

    async fn pipe_consume(
        &self,
        request: Request<proto::PipeConsumeRequest>,
    ) -> Result<Response<proto::ConsumeResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc(
            "PipeConsume",
            self.node.metrics.clone(),
            request,
            move |req| async move {
                tracing::Span::current().record("request_id", &req.request_id);
                if let Some(room_id) = node.state.get_room_id_for_router(&req.router_id) {
                    tracing::Span::current().record("room_id", &room_id);
                }
                let params = mapper::from_pipe_consume_request(req).map_err(Status::from)?;
                let res = node
                    .pipe_service
                    .pipe_consume(params)
                    .await
                    .map_err(Status::from)?;
                Ok(mapper::to_consume_response(res))
            },
        )
        .await
    }
}
