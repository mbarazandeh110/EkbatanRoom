use mediasoup::worker::Worker;

pub type WorkerDeadCallback = std::sync::Arc<dyn Fn(usize) + Send + Sync>;

pub trait WorkerPoolPort: Send + Sync {
    fn get_least_loaded(&self) -> (usize, Worker);
    fn increment_load(&self, index: usize);
    fn decrement_load(&self, index: usize);
    fn mark_dead(&self, index: usize);
    fn total_workers(&self) -> usize;
    fn alive_workers(&self) -> usize;
    fn is_worker_alive(&self, index: usize) -> bool;
    fn add_resource_score(&self, index: usize, weight: usize);
    fn sub_resource_score(&self, index: usize, weight: usize);
    fn worker_resource_score(&self, index: usize) -> usize;
    fn total_resource_score(&self) -> usize;
    fn get_worker(&self, index: usize) -> Option<Worker>;
    fn add_on_dead_callback(&self, callback: WorkerDeadCallback);
}

#[derive(Debug, Clone)]
pub struct LayerChangeEventParams<'a> {
    pub router_id: &'a str,
    pub consumer_id: &'a str,
    pub producer_id: &'a str,
    pub peer_id: &'a str,
    pub preferred_spatial: i32,
    pub current_spatial: i32,
    pub preferred_temporal: i32,
    pub current_temporal: i32,
}

pub trait EventPublisherPort: Send + Sync {
    fn publish_score(
        &self,
        router_id: &str,
        resource_id: &str,
        peer_id: &str,
        is_producer: bool,
        score: u32,
        producer_scores: Vec<u32>,
    );

    fn publish_layer_change(&self, params: LayerChangeEventParams<'_>);

    fn publish_active_speaker(
        &self,
        router_id: &str,
        producer_id: &str,
        peer_id: &str,
        volume_dbfs: f32,
    );

    fn publish_resource_closed(
        &self,
        router_id: &str,
        resource_type: &str,
        resource_id: &str,
        reason: &str,
    );

    fn publish_node_capacity(&self, info: &crate::domain::entities::NodeCapacityInfo);
}

pub trait MetricsPort: Send + Sync {
    fn inc_active_routers(&self);
    fn dec_active_routers(&self);
    fn inc_active_transports(&self, is_publisher: bool);
    fn dec_active_transports(&self, is_publisher: bool);
    fn inc_active_producers(&self, kind: &str);
    fn dec_active_producers(&self, kind: &str);
    fn inc_active_consumers(&self, kind: &str);
    fn dec_active_consumers(&self, kind: &str);
    fn record_idempotency_hit(&self);
    fn record_grpc_request(&self, method: &'static str, status: &str, duration_secs: f64);
    fn update_node_capacity(
        &self,
        total_capacity_weight: u32,
        logical_load_percent: f32,
        cpu_usage_percent: f32,
        effective_load_percent: f32,
    );
}
