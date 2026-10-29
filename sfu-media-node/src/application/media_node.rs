use crate::adapters::outbound_mediasoup::{EventHub, WorkerPool};
use crate::application::media_service::MediaService;
use crate::application::router_service::RouterService;
use crate::application::state::EngineState;
use crate::application::system_sampler::SystemSampler;
use crate::application::transport_service::TransportService;
use crate::config::Config;
use crate::domain::entities::NodeCapacityInfo;
use crate::domain::errors::SfuDomainError;
use crate::observability::metrics::PrometheusMetrics;
use crate::ports::inbound::RouterUseCase;
use crate::ports::outbound::{EventPublisherPort, MetricsPort, WorkerPoolPort};
use mediasoup::worker_manager::WorkerManager;
use std::sync::atomic::Ordering;
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
    pub pipe_service: Arc<dyn crate::ports::inbound::InterNodePipeUseCase>,
    system_sampler: Arc<SystemSampler>,
    capacity_task_abort: Option<tokio::task::AbortHandle>,
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
                &config.worker_settings,
            )
            .await?,
        );

        let event_hub = Arc::new(EventHub::new(4096));
        let metrics = Arc::new(PrometheusMetrics);
        let state = EngineState::default();

        let state_dead = state.clone();
        let worker_pool_dead = worker_pool.clone();
        let metrics_dead = metrics.clone();
        let event_hub_dead = event_hub.clone();
        worker_pool.add_on_dead_callback(Arc::new(move |w_idx| {
            state_dead.cleanup_dead_worker(
                w_idx,
                worker_pool_dead.as_ref(),
                metrics_dead.as_ref(),
                event_hub_dead.as_ref(),
            );
        }));

        let router_service = Arc::new(RouterService::new(
            state.clone(),
            worker_pool.clone(),
            event_hub.clone(),
            metrics.clone(),
            config.clone(),
        ));

        let transport_service = Arc::new(TransportService::new(
            config.clone(),
            state.clone(),
            worker_pool.clone(),
            event_hub.clone(),
            metrics.clone(),
        ));

        let media_service = Arc::new(MediaService::new(
            state.clone(),
            worker_pool.clone(),
            event_hub.clone(),
            metrics.clone(),
        ));

        #[cfg(not(feature = "enterprise"))]
        let pipe_service: Arc<dyn crate::ports::inbound::InterNodePipeUseCase> =
            Arc::new(crate::application::pipe_service::CommunityDisabledPipeService);

        #[cfg(feature = "enterprise")]
        let pipe_service: Arc<dyn crate::ports::inbound::InterNodePipeUseCase> = Arc::new(
            crate::application::pipe_service::EnterprisePipeService::new(
                config.clone(),
                state.clone(),
                metrics.clone(),
                worker_pool.clone(),
                event_hub.clone(),
            ),
        );

        let system_sampler = Arc::new(SystemSampler::new());

        let is_enterprise = pipe_service.is_enterprise_enabled();

        // Spawn periodic capacity publishing background task (every 1s, with >=3% hysteresis or 5s heartbeat)
        let sampler_clone = system_sampler.clone();
        let worker_pool_clone = worker_pool.clone();
        let state_clone = state.clone();
        let event_hub_clone = event_hub.clone();
        let metrics_clone = metrics.clone();
        let config_clone = config.clone();

        let capacity_task = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(1));
            let mut heartbeat_ticks = 0u32;
            let mut last_published_load = 0.0f32;

            loop {
                ticker.tick().await;

                let info = compute_node_capacity(
                    worker_pool_clone.as_ref(),
                    &config_clone,
                    &sampler_clone,
                    &state_clone,
                    is_enterprise,
                );

                // Update Prometheus metrics every tick
                metrics_clone.update_node_capacity(
                    info.total_capacity_weight,
                    info.logical_load_percent,
                    info.cpu_usage_percent,
                    info.effective_load_percent,
                );

                // Check hysteresis and heartbeat
                let delta = (info.effective_load_percent - last_published_load).abs();
                heartbeat_ticks += 1;

                if delta >= 3.0 || heartbeat_ticks >= 5 {
                    last_published_load = info.effective_load_percent;
                    heartbeat_ticks = 0;
                    event_hub_clone.publish_node_capacity(&info);
                }
            }
        });

        Ok(Self {
            config,
            state,
            worker_pool,
            event_hub,
            metrics,
            router_service,
            transport_service,
            media_service,
            pipe_service,
            system_sampler,
            capacity_task_abort: Some(capacity_task.abort_handle()),
        })
    }

    pub fn is_router_active(&self, router_id: &str) -> bool {
        self.state.is_router_active(router_id)
    }

    pub fn get_node_capacity_info(&self) -> NodeCapacityInfo {
        compute_node_capacity(
            self.worker_pool.as_ref(),
            &self.config,
            &self.system_sampler,
            &self.state,
            self.pipe_service.is_enterprise_enabled(),
        )
    }

    /// Marks the node as draining immediately and broadcasts updated capacity metrics to coordinators.
    pub fn start_draining(&self) {
        let was_draining = self.state.is_draining.swap(true, Ordering::SeqCst);
        if !was_draining {
            let info = self.get_node_capacity_info();
            self.metrics.update_node_capacity(
                info.total_capacity_weight,
                info.logical_load_percent,
                info.cpu_usage_percent,
                info.effective_load_percent,
            );
            self.event_hub.publish_node_capacity(&info);
        }
    }

    /// Performs bounded graceful shutdown, closing all active routers and cascading resources cleanly.
    pub async fn shutdown(&self, timeout: Duration) -> Result<(), SfuDomainError> {
        tracing::info!("Initiating bounded graceful shutdown of MediaNode...");

        self.start_draining();

        if let Some(handle) = &self.capacity_task_abort {
            handle.abort();
        }

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

pub fn compute_node_capacity(
    worker_pool: &dyn WorkerPoolPort,
    config: &Config,
    system_sampler: &SystemSampler,
    state: &EngineState,
    enterprise_cascading_enabled: bool,
) -> NodeCapacityInfo {
    let alive_workers = worker_pool.alive_workers() as u32;
    let total_workers = worker_pool.total_workers() as u32;
    let total_capacity_weight = alive_workers * config.worker_capacity_weight;
    let total_score = worker_pool.total_resource_score();

    let logical_load_percent = if total_capacity_weight > 0 {
        ((total_score as f32 / total_capacity_weight as f32) * 100.0).clamp(0.0, 100.0)
    } else {
        100.0
    };

    let (cpu_usage_percent, memory_usage_percent) = system_sampler.sample();

    let mut max_worker_load_percent = 0.0f32;
    for i in 0..total_workers as usize {
        if !worker_pool.is_worker_alive(i) {
            continue;
        }
        let w_score = worker_pool.worker_resource_score(i);
        let w_pct = if config.worker_capacity_weight > 0 {
            (w_score as f32 / config.worker_capacity_weight as f32) * 100.0
        } else {
            0.0
        };
        if w_pct > max_worker_load_percent {
            max_worker_load_percent = w_pct;
        }
    }

    let effective_load_percent = f32::max(logical_load_percent, cpu_usage_percent);

    NodeCapacityInfo {
        total_capacity_weight,
        total_workers,
        alive_workers,
        logical_load_percent,
        cpu_usage_percent,
        memory_usage_percent,
        max_worker_load_percent,
        effective_load_percent,
        active_routers: state.routers.len() as u32,
        active_transports: (state.transports.len() + state.pipe_transports.len()) as u32,
        active_producers: state.producers.len() as u32,
        active_consumers: state.consumers.len() as u32,
        is_draining: state.is_draining.load(Ordering::SeqCst) || alive_workers == 0,
        enterprise_cascading_enabled,
    }
}
