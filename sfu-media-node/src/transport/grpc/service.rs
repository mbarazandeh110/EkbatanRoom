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

#[tonic::async_trait]
impl SfuMediaEngineService for GrpcMediaEngineService {
    type WatchNodeEventsStream =
        Pin<Box<dyn Stream<Item = Result<proto::NodeEvent, Status>> + Send + 'static>>;

    async fn create_router(
        &self,
        request: Request<proto::CreateRouterRequest>,
    ) -> Result<Response<proto::CreateRouterResponse>, Status> {
        let req = request.into_inner();
        let resp = self.node.create_router(req).await.map_err(Status::from)?;
        Ok(Response::new(resp))
    }

    async fn create_transport(
        &self,
        request: Request<proto::CreateTransportRequest>,
    ) -> Result<Response<proto::CreateTransportResponse>, Status> {
        let req = request.into_inner();
        let resp = self
            .node
            .create_transport(req)
            .await
            .map_err(Status::from)?;
        Ok(Response::new(resp))
    }

    async fn connect_transport(
        &self,
        request: Request<proto::ConnectTransportRequest>,
    ) -> Result<Response<proto::ConnectTransportResponse>, Status> {
        let req = request.into_inner();
        let resp = self
            .node
            .connect_transport(req)
            .await
            .map_err(Status::from)?;
        Ok(Response::new(resp))
    }

    async fn restart_ice(
        &self,
        request: Request<proto::RestartIceRequest>,
    ) -> Result<Response<proto::RestartIceResponse>, Status> {
        let req = request.into_inner();
        let resp = self.node.restart_ice(req).await.map_err(Status::from)?;
        Ok(Response::new(resp))
    }

    async fn produce(
        &self,
        request: Request<proto::ProduceRequest>,
    ) -> Result<Response<proto::ProduceResponse>, Status> {
        let req = request.into_inner();
        let resp = self.node.produce(req).await.map_err(Status::from)?;
        Ok(Response::new(resp))
    }

    async fn consume(
        &self,
        request: Request<proto::ConsumeRequest>,
    ) -> Result<Response<proto::ConsumeResponse>, Status> {
        let req = request.into_inner();
        let resp = self.node.consume(req).await.map_err(Status::from)?;
        Ok(Response::new(resp))
    }

    async fn set_media_paused(
        &self,
        request: Request<proto::SetMediaPausedRequest>,
    ) -> Result<Response<proto::SetMediaPausedResponse>, Status> {
        let req = request.into_inner();
        let resp = self
            .node
            .set_media_paused(req)
            .await
            .map_err(Status::from)?;
        Ok(Response::new(resp))
    }

    async fn set_consumer_preferred_layers(
        &self,
        request: Request<proto::SetConsumerPreferredLayersRequest>,
    ) -> Result<Response<proto::SetConsumerPreferredLayersResponse>, Status> {
        let req = request.into_inner();
        let resp = self
            .node
            .set_consumer_preferred_layers(req)
            .await
            .map_err(Status::from)?;
        Ok(Response::new(resp))
    }

    async fn close_resource(
        &self,
        request: Request<proto::CloseResourceRequest>,
    ) -> Result<Response<proto::CloseResourceResponse>, Status> {
        let req = request.into_inner();
        let resp = self.node.close_resource(req).map_err(Status::from)?;
        Ok(Response::new(resp))
    }

    async fn get_stats(
        &self,
        request: Request<proto::GetStatsRequest>,
    ) -> Result<Response<proto::GetStatsResponse>, Status> {
        let req = request.into_inner();
        let resp = self.node.get_stats(req).await.map_err(Status::from)?;
        Ok(Response::new(resp))
    }

    async fn watch_node_events(
        &self,
        _request: Request<proto::WatchNodeEventsRequest>,
    ) -> Result<Response<Self::WatchNodeEventsStream>, Status> {
        let rx = self.node.subscribe_events();
        let stream = BroadcastStream::new(rx).filter_map(|item| match item {
            Ok(event) => Some(Ok(event)),
            Err(e) => {
                tracing::warn!("Event stream skipped lagged items: {e}");
                None
            }
        });

        Ok(Response::new(Box::pin(stream)))
    }
}
