use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Audio,
    Video,
}

impl std::fmt::Display for MediaKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaKind::Audio => write!(f, "audio"),
            MediaKind::Video => write!(f, "video"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DtlsRole {
    Auto,
    Client,
    Server,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DtlsFingerprintInfo {
    pub algorithm: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulcastEncodingInfo {
    pub rid: String,
    pub ssrc: u32,
    pub rtx_ssrc: u32,
    pub max_bitrate: u32,
    pub scalability_mode: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsumerLayersInfo {
    pub spatial_layer: u8,
    pub temporal_layer: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceStatsInfo {
    pub stats_json: String,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub current_spatial_layer: i32,
    pub current_temporal_layer: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IceCandidateInfo {
    pub foundation: String,
    pub priority: u32,
    pub ip: String,
    pub protocol: String,
    pub port: u32,
    pub r#type: String,
    pub tcp_type: String,
    pub sdp_candidate_line: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeCapacityInfo {
    pub total_capacity_weight: u32,
    pub total_workers: u32,
    pub alive_workers: u32,
    pub logical_load_percent: f32,
    pub cpu_usage_percent: f32,
    pub memory_usage_percent: f32,
    pub max_worker_load_percent: f32,
    pub effective_load_percent: f32,
    pub active_routers: u32,
    pub active_transports: u32,
    pub active_producers: u32,
    pub active_consumers: u32,
    pub is_draining: bool,
    pub enterprise_cascading_enabled: bool,
    pub license_tier: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DomainResourceType {
    Unspecified,
    Router,
    Transport,
    Producer,
    Consumer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainMediaScoreEvent {
    pub resource_id: String,
    pub peer_id: String,
    pub is_producer: bool,
    pub score: u32,
    pub producer_scores: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainLayerChangedEvent {
    pub consumer_id: String,
    pub producer_id: String,
    pub peer_id: String,
    pub preferred_spatial_layer: i32,
    pub current_spatial_layer: i32,
    pub preferred_temporal_layer: i32,
    pub current_temporal_layer: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainActiveSpeakerEvent {
    pub producer_id: String,
    pub peer_id: String,
    pub volume_dbfs: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainResourceClosedEvent {
    pub resource_type: DomainResourceType,
    pub resource_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DomainNodeEventPayload {
    MediaScore(DomainMediaScoreEvent),
    LayerChanged(DomainLayerChangedEvent),
    ActiveSpeaker(DomainActiveSpeakerEvent),
    ResourceClosed(DomainResourceClosedEvent),
    NodeCapacity(NodeCapacityInfo),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainNodeEvent {
    pub router_id: String,
    pub timestamp_ms: i64,
    pub payload: Option<DomainNodeEventPayload>,
}
