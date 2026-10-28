use crate::adapters::inbound_grpc::mapper;
use crate::application::MediaNode;
use crate::ports::inbound::{MediaUseCase, RouterUseCase, TransportUseCase};
use crate::proto::sfu::v1 as proto;
use crate::proto::sfu::v1::sfu_media_engine_service_server::SfuMediaEngineService;
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status};
use tracing::Instrument;

pub struct GrpcMediaEngineService {
    node: Arc<MediaNode>,
}

impl GrpcMediaEngineService {
    pub fn new(node: Arc<MediaNode>) -> Self {
        Self { node }
    }
}

async fn instrument_rpc<Req, Resp, Fut>(
    method: &'static str,
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
            tonic::Code::Aborted => "aborted",
            tonic::Code::Unavailable => "unavailable",
            tonic::Code::Internal => "internal",
            _ => "error",
        },
    };

    span.record("status", status_str);
    crate::observability::metrics::record_grpc_request(method, status_str, duration);

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
        instrument_rpc("CreateRouter", request, move |req| async move {
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
        })
        .await
    }

    async fn create_transport(
        &self,
        request: Request<proto::CreateTransportRequest>,
    ) -> Result<Response<proto::CreateTransportResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc("CreateTransport", request, move |req| async move {
            let params = mapper::from_create_transport_request(req);
            let res = node
                .transport_service
                .create_transport(params)
                .await
                .map_err(Status::from)?;
            Ok(mapper::to_create_transport_response(res))
        })
        .await
    }

    async fn connect_transport(
        &self,
        request: Request<proto::ConnectTransportRequest>,
    ) -> Result<Response<proto::ConnectTransportResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc("ConnectTransport", request, move |req| async move {
            let params = mapper::from_connect_transport_request(req).map_err(Status::from)?;
            node.transport_service
                .connect_transport(params)
                .await
                .map_err(Status::from)?;
            Ok(proto::ConnectTransportResponse {})
        })
        .await
    }

    async fn restart_ice(
        &self,
        request: Request<proto::RestartIceRequest>,
    ) -> Result<Response<proto::RestartIceResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc("RestartIce", request, move |req| async move {
            let res = node
                .transport_service
                .restart_ice(&req.router_id, &req.transport_id)
                .await
                .map_err(Status::from)?;
            Ok(mapper::to_restart_ice_response(res))
        })
        .await
    }

    async fn produce(
        &self,
        request: Request<proto::ProduceRequest>,
    ) -> Result<Response<proto::ProduceResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc("Produce", request, move |req| async move {
            let params = mapper::from_produce_request(req).map_err(Status::from)?;
            let res = node
                .media_service
                .produce(params)
                .await
                .map_err(Status::from)?;
            Ok(mapper::to_produce_response(res))
        })
        .await
    }

    async fn consume(
        &self,
        request: Request<proto::ConsumeRequest>,
    ) -> Result<Response<proto::ConsumeResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc("Consume", request, move |req| async move {
            let params = mapper::from_consume_request(req).map_err(Status::from)?;
            let res = node
                .media_service
                .consume(params)
                .await
                .map_err(Status::from)?;
            Ok(mapper::to_consume_response(res))
        })
        .await
    }

    async fn set_media_paused(
        &self,
        request: Request<proto::SetMediaPausedRequest>,
    ) -> Result<Response<proto::SetMediaPausedResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc("SetMediaPaused", request, move |req| async move {
            let is_producer = match req.resource_type() {
                proto::ResourceType::Producer => true,
                proto::ResourceType::Consumer => false,
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
        })
        .await
    }

    async fn set_consumer_preferred_layers(
        &self,
        request: Request<proto::SetConsumerPreferredLayersRequest>,
    ) -> Result<Response<proto::SetConsumerPreferredLayersResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc(
            "SetConsumerPreferredLayers",
            request,
            move |req| async move {
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
        instrument_rpc("CloseResource", request, move |req| async move {
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
                    node.transport_service
                        .close_transport(&req.router_id, &req.resource_id)
                        .map_err(Status::from)?;
                }
                proto::ResourceType::Producer => {
                    node.media_service
                        .close_producer(&req.router_id, &req.resource_id)
                        .map_err(Status::from)?;
                }
                proto::ResourceType::Consumer => {
                    node.media_service
                        .close_consumer(&req.router_id, &req.resource_id)
                        .map_err(Status::from)?;
                }
                proto::ResourceType::Unspecified => {
                    return Err(Status::invalid_argument("Resource type unspecified"));
                }
            }
            Ok(proto::CloseResourceResponse {})
        })
        .await
    }

    async fn get_stats(
        &self,
        request: Request<proto::GetStatsRequest>,
    ) -> Result<Response<proto::GetStatsResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc("GetStats", request, move |req| async move {
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
        })
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
            status = "ok",
        );
        crate::observability::tracing_extractor::attach_trace_context(&span, request.metadata());

        let rx = self.node.event_hub.subscribe();
        let node_ref = self.node.clone();
        let stream = BroadcastStream::new(rx).filter_map(move |item| match item {
            Ok(event) => {
                let is_passthrough = matches!(
                    event.payload,
                    Some(proto::node_event::Payload::ResourceClosed(_))
                        | Some(proto::node_event::Payload::NodeCapacity(_))
                );
                if is_passthrough || node_ref.is_router_active(&event.router_id) {
                    Some(Ok(event))
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

        crate::observability::metrics::record_grpc_request(
            "WatchNodeEvents",
            "ok",
            start.elapsed().as_secs_f64(),
        );

        Ok(Response::new(Box::pin(stream)))
    }

    async fn get_node_capacity(
        &self,
        request: Request<proto::GetNodeCapacityRequest>,
    ) -> Result<Response<proto::NodeCapacityMetrics>, Status> {
        let node = self.node.clone();
        instrument_rpc("GetNodeCapacity", request, move |_req| async move {
            Ok(node.get_node_capacity_metrics())
        })
        .await
    }

    async fn create_pipe_transport(
        &self,
        request: Request<proto::CreatePipeTransportRequest>,
    ) -> Result<Response<proto::CreatePipeTransportResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc("CreatePipeTransport", request, move |req| async move {
            let params = mapper::from_create_pipe_transport_request(req);
            let res = node
                .transport_service
                .create_pipe_transport(params)
                .await
                .map_err(Status::from)?;
            Ok(mapper::to_create_pipe_transport_response(res))
        })
        .await
    }

    async fn connect_pipe_transport(
        &self,
        request: Request<proto::ConnectPipeTransportRequest>,
    ) -> Result<Response<proto::ConnectPipeTransportResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc("ConnectPipeTransport", request, move |req| async move {
            let params = mapper::from_connect_pipe_transport_request(req).map_err(Status::from)?;
            node.transport_service
                .connect_pipe_transport(params)
                .await
                .map_err(Status::from)?;
            Ok(proto::ConnectPipeTransportResponse {})
        })
        .await
    }
}
