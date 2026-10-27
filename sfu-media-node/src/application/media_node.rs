use crate::adapters::outbound_mediasoup::{EventHub, WorkerPool};
use crate::application::media_service::MediaService;
use crate::application::router_service::RouterService;
use crate::application::state::EngineState;
use crate::application::transport_service::TransportService;
use crate::config::Config;
use crate::domain::errors::SfuDomainError;
use crate::observability::metrics::PrometheusMetrics;
use crate::ports::inbound::RouterUseCase;
use mediasoup::worker_manager::WorkerManager;
use std::sync::Arc;
use std::time::Duration;

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

        let event_hub = Arc::new(EventHub::new(4096));
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
        self.state.is_router_active(router_id)
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
