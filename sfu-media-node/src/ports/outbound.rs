use mediasoup::worker::Worker;

pub trait WorkerPoolPort: Send + Sync {
    fn get_least_loaded(&self) -> (usize, Worker);
    fn increment_load(&self, index: usize);
    fn decrement_load(&self, index: usize);
    fn mark_dead(&self, index: usize);
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

    fn publish_resource_closed(
        &self,
        router_id: &str,
        resource_type: &str,
        resource_id: &str,
        reason: &str,
    );
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
}
