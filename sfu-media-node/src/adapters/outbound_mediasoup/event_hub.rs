use crate::domain::entities::*;
use crate::ports::outbound::EventPublisherPort;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct EventHub {
    sender: broadcast::Sender<DomainNodeEvent>,
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

    pub fn subscribe(&self) -> broadcast::Receiver<DomainNodeEvent> {
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
        let event = DomainNodeEvent {
            router_id: router_id.to_string(),
            timestamp_ms: Self::now_ms(),
            payload: Some(DomainNodeEventPayload::MediaScore(DomainMediaScoreEvent {
                resource_id: resource_id.to_string(),
                peer_id: peer_id.to_string(),
                is_producer,
                score,
                producer_scores,
            })),
        };
        let _ = self.sender.send(event);
    }

    fn publish_layer_change(&self, params: crate::ports::outbound::LayerChangeEventParams<'_>) {
        let event = DomainNodeEvent {
            router_id: params.router_id.to_string(),
            timestamp_ms: Self::now_ms(),
            payload: Some(DomainNodeEventPayload::LayerChanged(
                DomainLayerChangedEvent {
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
        let event = DomainNodeEvent {
            router_id: router_id.to_string(),
            timestamp_ms: Self::now_ms(),
            payload: Some(DomainNodeEventPayload::ActiveSpeaker(
                DomainActiveSpeakerEvent {
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
            "router" => DomainResourceType::Router,
            "transport" => DomainResourceType::Transport,
            "producer" => DomainResourceType::Producer,
            "consumer" => DomainResourceType::Consumer,
            _ => DomainResourceType::Unspecified,
        };

        let event = DomainNodeEvent {
            router_id: router_id.to_string(),
            timestamp_ms: Self::now_ms(),
            payload: Some(DomainNodeEventPayload::ResourceClosed(
                DomainResourceClosedEvent {
                    resource_type: r_type,
                    resource_id: resource_id.to_string(),
                    reason: reason.to_string(),
                },
            )),
        };
        let _ = self.sender.send(event);
    }

    fn publish_node_capacity(&self, info: &crate::domain::entities::NodeCapacityInfo) {
        let event = DomainNodeEvent {
            router_id: String::new(),
            timestamp_ms: Self::now_ms(),
            payload: Some(DomainNodeEventPayload::NodeCapacity(info.clone())),
        };
        let _ = self.sender.send(event);
    }
}
