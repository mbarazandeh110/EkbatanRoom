use crate::adapters::inbound_grpc::mapper;
use crate::adapters::outbound_mediasoup::{EventHub, WorkerPool};
use crate::application::media_service::MediaService;
use crate::application::router_service::RouterService;
use crate::application::state::EngineState;
use crate::application::transport_service::TransportService;
use crate::config::Config;
use crate::domain::errors::SfuDomainError;
use crate::observability::metrics::PrometheusMetrics;
use crate::ports::inbound::{MediaUseCase, RouterUseCase, TransportUseCase};
use crate::proto::sfu::v1 as proto;
use mediasoup::worker_manager::WorkerManager;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

/// MediaNode serves as a unified Application Facade coordinating the hexagonal services:
/// RouterService, TransportService, MediaService, WorkerPool, and EventHub.
pub struct MediaNode {
    pub config: Config,
    pub state: EngineState,
    pub worker_pool: Arc<WorkerPool>,
    pub event_hub: Arc<EventHub>,
    pub metrics: Arc<PrometheusMetrics>,
    pub router_service: Arc<RouterService>,
    pub transport_service: Arc<TransportService>,
    pub media_service: Arc<MediaService>,
}

impl MediaNode {
    pub async fn new(config: Config) -> Result<Self, SfuDomainError> {
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

        let event_hub = Arc::new(EventHub::new(1024));
        let metrics = Arc::new(PrometheusMetrics);
        let state = EngineState::default();

        let router_service = Arc::new(RouterService::new(
            state.clone(),
            worker_pool.clone(),
            event_hub.clone(),
            metrics.clone(),
        ));

        let transport_service = Arc::new(TransportService::new(
            config.clone(),
            state.clone(),
            event_hub.clone(),
            metrics.clone(),
        ));

        let media_service = Arc::new(MediaService::new(
            state.clone(),
            event_hub.clone(),
            metrics.clone(),
        ));

        Ok(Self {
            config,
            state,
            worker_pool,
            event_hub,
            metrics,
            router_service,
            transport_service,
            media_service,
        })
    }

    pub fn is_router_active(&self, router_id: &str) -> bool {
        self.state
            .routers
            .get(router_id)
            .map(|r| !r.is_closed.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<proto::NodeEvent> {
        self.event_hub.subscribe()
    }

    pub async fn create_router(
        &self,
        req: proto::CreateRouterRequest,
    ) -> Result<proto::CreateRouterResponse, SfuDomainError> {
        let (router_id, worker_index, caps_json) = self
            .router_service
            .create_router(&req.request_id, &req.room_id)
            .await?;
        Ok(mapper::to_create_router_response(
            router_id,
            worker_index,
            caps_json,
        ))
    }

    pub async fn create_transport(
        &self,
        req: proto::CreateTransportRequest,
    ) -> Result<proto::CreateTransportResponse, SfuDomainError> {
        let params = mapper::from_create_transport_request(req);
        let res = self.transport_service.create_transport(params).await?;
        Ok(mapper::to_create_transport_response(res))
    }

    pub async fn connect_transport(
        &self,
        req: proto::ConnectTransportRequest,
    ) -> Result<proto::ConnectTransportResponse, SfuDomainError> {
        let params = mapper::from_connect_transport_request(req)?;
        self.transport_service.connect_transport(params).await?;
        Ok(proto::ConnectTransportResponse {})
    }

    pub async fn restart_ice(
        &self,
        req: proto::RestartIceRequest,
    ) -> Result<proto::RestartIceResponse, SfuDomainError> {
        let res = self
            .transport_service
            .restart_ice(&req.router_id, &req.transport_id)
            .await?;
        Ok(mapper::to_restart_ice_response(res))
    }

    pub async fn produce(
        &self,
        req: proto::ProduceRequest,
    ) -> Result<proto::ProduceResponse, SfuDomainError> {
        let params = mapper::from_produce_request(req)?;
        let res = self.media_service.produce(params).await?;
        Ok(mapper::to_produce_response(res))
    }

    pub async fn consume(
        &self,
        req: proto::ConsumeRequest,
    ) -> Result<proto::ConsumeResponse, SfuDomainError> {
        let params = mapper::from_consume_request(req)?;
        let res = self.media_service.consume(params).await?;
        Ok(mapper::to_consume_response(res))
    }

    pub async fn set_media_paused(
        &self,
        req: proto::SetMediaPausedRequest,
    ) -> Result<proto::SetMediaPausedResponse, SfuDomainError> {
        let is_producer = match req.resource_type() {
            proto::ResourceType::Producer => true,
            proto::ResourceType::Consumer => false,
            _ => {
                return Err(SfuDomainError::InvalidArgument(
                    "SetMediaPaused requires Producer or Consumer ResourceType".to_string(),
                ))
            }
        };
        self.media_service
            .set_media_paused(&req.router_id, &req.resource_id, is_producer, req.paused)
            .await?;
        Ok(proto::SetMediaPausedResponse {})
    }

    pub async fn set_consumer_preferred_layers(
        &self,
        req: proto::SetConsumerPreferredLayersRequest,
    ) -> Result<proto::SetConsumerPreferredLayersResponse, SfuDomainError> {
        let (router_id, consumer_id, layers) =
            mapper::from_set_consumer_preferred_layers_request(req)?;
        self.media_service
            .set_consumer_preferred_layers(&router_id, &consumer_id, layers)
            .await?;
        Ok(proto::SetConsumerPreferredLayersResponse {})
    }

    pub fn close_resource(
        &self,
        req: proto::CloseResourceRequest,
    ) -> Result<proto::CloseResourceResponse, SfuDomainError> {
        match req.resource_type() {
            proto::ResourceType::Router => {
                self.router_service.close_router(&req.resource_id)?;
            }
            proto::ResourceType::Transport => {
                self.transport_service
                    .close_transport(&req.router_id, &req.resource_id)?;
            }
            proto::ResourceType::Producer => {
                self.media_service
                    .close_producer(&req.router_id, &req.resource_id)?;
            }
            proto::ResourceType::Consumer => {
                self.media_service
                    .close_consumer(&req.router_id, &req.resource_id)?;
            }
            proto::ResourceType::Unspecified => {
                return Err(SfuDomainError::InvalidArgument(
                    "Resource type unspecified".to_string(),
                ));
            }
        }
        Ok(proto::CloseResourceResponse {})
    }

    pub async fn get_stats(
        &self,
        req: proto::GetStatsRequest,
    ) -> Result<proto::GetStatsResponse, SfuDomainError> {
        let r_type = match req.resource_type() {
            proto::ResourceType::Transport => "transport",
            proto::ResourceType::Producer => "producer",
            proto::ResourceType::Consumer => "consumer",
            _ => {
                return Err(SfuDomainError::InvalidArgument(
                    "GetStats requires Transport, Producer, or Consumer ResourceType".to_string(),
                ))
            }
        };
        let stats = self
            .media_service
            .get_stats(&req.router_id, &req.resource_id, r_type)
            .await?;
        Ok(mapper::to_get_stats_response(stats))
    }

    /// Performs bounded graceful shutdown, closing all active routers and cascading resources cleanly.
    pub async fn shutdown(&self, timeout: Duration) -> Result<(), SfuDomainError> {
        tracing::info!("Initiating bounded graceful shutdown of MediaNode...");
        let router_ids: Vec<String> = self.state.routers.iter().map(|r| r.key().clone()).collect();

        let shutdown_fut = async {
            for rid in router_ids {
                if let Err(e) = self.router_service.close_router(&rid) {
                    tracing::warn!(router_id = %rid, "Error closing router during shutdown: {e}");
                }
            }
        };

        tokio::time::timeout(timeout, shutdown_fut)
            .await
            .map_err(|_| SfuDomainError::Internal("Graceful shutdown timed out".to_string()))?;

        tracing::info!("All routers and associated resources closed cleanly.");
        Ok(())
    }
}
