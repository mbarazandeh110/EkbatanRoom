use crate::ports::outbound::EventPublisherPort;
use crate::proto::sfu::v1 as proto;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct EventHub {
    sender: broadcast::Sender<proto::NodeEvent>,
}

impl Default for EventHub {
    fn default() -> Self {
        Self::new(4096)
    }
}

impl EventHub {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<proto::NodeEvent> {
        self.sender.subscribe()
    }

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

impl EventPublisherPort for EventHub {
    fn publish_score(
        &self,
        router_id: &str,
        resource_id: &str,
        peer_id: &str,
        is_producer: bool,
        score: u32,
        producer_scores: Vec<u32>,
    ) {
        let event = proto::NodeEvent {
            router_id: router_id.to_string(),
            timestamp_ms: Self::now_ms(),
            payload: Some(proto::node_event::Payload::MediaScore(
                proto::MediaScoreEvent {
                    resource_id: resource_id.to_string(),
                    peer_id: peer_id.to_string(),
                    is_producer,
                    score,
                    producer_scores,
                },
            )),
        };
        let _ = self.sender.send(event);
    }

    fn publish_layer_change(&self, params: crate::ports::outbound::LayerChangeEventParams<'_>) {
        let event = proto::NodeEvent {
            router_id: params.router_id.to_string(),
            timestamp_ms: Self::now_ms(),
            payload: Some(proto::node_event::Payload::LayerChanged(
                proto::LayerChangedEvent {
                    consumer_id: params.consumer_id.to_string(),
                    producer_id: params.producer_id.to_string(),
                    peer_id: params.peer_id.to_string(),
                    preferred_spatial_layer: params.preferred_spatial,
                    current_spatial_layer: params.current_spatial,
                    preferred_temporal_layer: params.preferred_temporal,
                    current_temporal_layer: params.current_temporal,
                },
            )),
        };
        let _ = self.sender.send(event);
    }

    fn publish_active_speaker(
        &self,
        router_id: &str,
        producer_id: &str,
        peer_id: &str,
        volume_dbfs: f32,
    ) {
        let event = proto::NodeEvent {
            router_id: router_id.to_string(),
            timestamp_ms: Self::now_ms(),
            payload: Some(proto::node_event::Payload::ActiveSpeaker(
                proto::ActiveSpeakerEvent {
                    producer_id: producer_id.to_string(),
                    peer_id: peer_id.to_string(),
                    volume_dbfs,
                },
            )),
        };
        let _ = self.sender.send(event);
    }

    fn publish_resource_closed(
        &self,
        router_id: &str,
        resource_type_str: &str,
        resource_id: &str,
        reason: &str,
    ) {
        let r_type = match resource_type_str.to_lowercase().as_str() {
            "router" => proto::ResourceType::Router as i32,
            "transport" => proto::ResourceType::Transport as i32,
            "producer" => proto::ResourceType::Producer as i32,
            "consumer" => proto::ResourceType::Consumer as i32,
            _ => proto::ResourceType::Unspecified as i32,
        };

        let event = proto::NodeEvent {
            router_id: router_id.to_string(),
            timestamp_ms: Self::now_ms(),
            payload: Some(proto::node_event::Payload::ResourceClosed(
                proto::ResourceClosedEvent {
                    resource_type: r_type,
                    resource_id: resource_id.to_string(),
                    reason: reason.to_string(),
                },
            )),
        };
        let _ = self.sender.send(event);
    }

    fn publish_node_capacity(&self, info: &crate::domain::entities::NodeCapacityInfo) {
        let metrics = proto::NodeCapacityMetrics {
            total_capacity_weight: info.total_capacity_weight,
            total_workers: info.total_workers,
            alive_workers: info.alive_workers,
            logical_load_percent: info.logical_load_percent,
            cpu_usage_percent: info.cpu_usage_percent,
            memory_usage_percent: info.memory_usage_percent,
            max_worker_load_percent: info.max_worker_load_percent,
            effective_load_percent: info.effective_load_percent,
            active_routers: info.active_routers,
            active_transports: info.active_transports,
            active_producers: info.active_producers,
            active_consumers: info.active_consumers,
            is_draining: info.is_draining,
        };
        let event = proto::NodeEvent {
            router_id: String::new(),
            timestamp_ms: Self::now_ms(),
            payload: Some(proto::node_event::Payload::NodeCapacity(metrics)),
        };
        let _ = self.sender.send(event);
    }
}
