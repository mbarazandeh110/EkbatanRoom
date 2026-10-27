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

impl From<MediaKind> for mediasoup::prelude::MediaKind {
    fn from(kind: MediaKind) -> Self {
        match kind {
            MediaKind::Audio => mediasoup::prelude::MediaKind::Audio,
            MediaKind::Video => mediasoup::prelude::MediaKind::Video,
        }
    }
}

impl From<mediasoup::prelude::MediaKind> for MediaKind {
    fn from(kind: mediasoup::prelude::MediaKind) -> Self {
        match kind {
            mediasoup::prelude::MediaKind::Audio => MediaKind::Audio,
            mediasoup::prelude::MediaKind::Video => MediaKind::Video,
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
