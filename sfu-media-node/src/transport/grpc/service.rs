use crate::engine::media_node::MediaNode;
use crate::proto::sfu::v1 as proto;
use crate::proto::sfu::v1::sfu_media_engine_service_server::SfuMediaEngineService;
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status};

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
        request_id = tracing::field::Empty,
        status = tracing::field::Empty,
    );
    let _enter = span.enter();

    crate::observability::tracing_extractor::attach_trace_context(&span, request.metadata());
    let req = request.into_inner();

    let res = handler(req).await;
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
            node.create_router(req).await.map_err(Status::from)
        })
        .await
    }

    async fn create_transport(
        &self,
        request: Request<proto::CreateTransportRequest>,
    ) -> Result<Response<proto::CreateTransportResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc("CreateTransport", request, move |req| async move {
            node.create_transport(req).await.map_err(Status::from)
        })
        .await
    }

    async fn connect_transport(
        &self,
        request: Request<proto::ConnectTransportRequest>,
    ) -> Result<Response<proto::ConnectTransportResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc("ConnectTransport", request, move |req| async move {
            node.connect_transport(req).await.map_err(Status::from)
        })
        .await
    }

    async fn restart_ice(
        &self,
        request: Request<proto::RestartIceRequest>,
    ) -> Result<Response<proto::RestartIceResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc("RestartIce", request, move |req| async move {
            node.restart_ice(req).await.map_err(Status::from)
        })
        .await
    }

    async fn produce(
        &self,
        request: Request<proto::ProduceRequest>,
    ) -> Result<Response<proto::ProduceResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc("Produce", request, move |req| async move {
            node.produce(req).await.map_err(Status::from)
        })
        .await
    }

    async fn consume(
        &self,
        request: Request<proto::ConsumeRequest>,
    ) -> Result<Response<proto::ConsumeResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc("Consume", request, move |req| async move {
            node.consume(req).await.map_err(Status::from)
        })
        .await
    }

    async fn set_media_paused(
        &self,
        request: Request<proto::SetMediaPausedRequest>,
    ) -> Result<Response<proto::SetMediaPausedResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc("SetMediaPaused", request, move |req| async move {
            node.set_media_paused(req).await.map_err(Status::from)
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
                node.set_consumer_preferred_layers(req)
                    .await
                    .map_err(Status::from)
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
            node.close_resource(req).map_err(Status::from)
        })
        .await
    }

    async fn get_stats(
        &self,
        request: Request<proto::GetStatsRequest>,
    ) -> Result<Response<proto::GetStatsResponse>, Status> {
        let node = self.node.clone();
        instrument_rpc("GetStats", request, move |req| async move {
            node.get_stats(req).await.map_err(Status::from)
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
            request_id = tracing::field::Empty,
            status = "ok",
        );
        let _enter = span.enter();
        crate::observability::tracing_extractor::attach_trace_context(&span, request.metadata());

        let rx = self.node.subscribe_events();
        let stream = BroadcastStream::new(rx).filter_map(|item| match item {
            Ok(event) => Some(Ok(event)),
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
}
