use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use thiserror::Error;

use crate::observability::ObservabilityConfig;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("Invalid listen IP '{0}': must be a valid IPv4 or IPv6 address")]
    InvalidListenIp(String),

    #[error("Invalid RTC port range: start port ({min}) cannot be greater than end port ({max})")]
    InvalidPortRange { min: u16, max: u16 },

    #[error("Invalid worker count: must be greater than 0")]
    InvalidWorkerCount,

    #[error("Failed to parse integer configuration '{key}': {err}")]
    ParseIntError { key: &'static str, err: String },

    #[error("Invalid log format '{0}': must be 'json', 'pretty', or 'text'")]
    InvalidLogFormat(String),

    #[error("Invalid socket address for '{key}': '{val}' ({err})")]
    InvalidSocketAddr {
        key: &'static str,
        val: String,
        err: String,
    },

    #[error("Failed to read configuration file '{path}': {err}")]
    FileReadError { path: String, err: String },

    #[error("Failed to parse YAML configuration file '{path}': {err}")]
    YamlParseError { path: String, err: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediasoupWorkerConfig {
    #[serde(default = "default_worker_log_level")]
    pub log_level: String,
    #[serde(default = "default_worker_log_tags")]
    pub log_tags: Vec<String>,
    #[serde(default)]
    pub dtls_certificate_file: Option<String>,
    #[serde(default)]
    pub dtls_private_key_file: Option<String>,
}

impl Default for MediasoupWorkerConfig {
    fn default() -> Self {
        Self {
            log_level: default_worker_log_level(),
            log_tags: default_worker_log_tags(),
            dtls_certificate_file: None,
            dtls_private_key_file: None,
        }
    }
}

fn default_worker_log_level() -> String {
    "warn".to_string()
}

fn default_worker_log_tags() -> Vec<String> {
    vec![
        "info".to_string(),
        "ice".to_string(),
        "dtls".to_string(),
        "rtp".to_string(),
        "srtp".to_string(),
        "rtcp".to_string(),
        "rtx".to_string(),
        "bwe".to_string(),
        "score".to_string(),
    ]
}

impl MediasoupWorkerConfig {
    pub fn to_mediasoup_log_level(&self) -> mediasoup::worker::WorkerLogLevel {
        match self.log_level.to_lowercase().as_str() {
            "debug" => mediasoup::worker::WorkerLogLevel::Debug,
            "warn" => mediasoup::worker::WorkerLogLevel::Warn,
            "error" => mediasoup::worker::WorkerLogLevel::Error,
            "none" => mediasoup::worker::WorkerLogLevel::None,
            _ => mediasoup::worker::WorkerLogLevel::Warn,
        }
    }

    pub fn to_mediasoup_log_tags(&self) -> Vec<mediasoup::worker::WorkerLogTag> {
        self.log_tags
            .iter()
            .filter_map(|t| match t.to_lowercase().as_str() {
                "info" => Some(mediasoup::worker::WorkerLogTag::Info),
                "ice" => Some(mediasoup::worker::WorkerLogTag::Ice),
                "dtls" => Some(mediasoup::worker::WorkerLogTag::Dtls),
                "rtp" => Some(mediasoup::worker::WorkerLogTag::Rtp),
                "srtp" => Some(mediasoup::worker::WorkerLogTag::Srtp),
                "rtcp" => Some(mediasoup::worker::WorkerLogTag::Rtcp),
                "rtx" => Some(mediasoup::worker::WorkerLogTag::Rtx),
                "bwe" => Some(mediasoup::worker::WorkerLogTag::Bwe),
                "score" => Some(mediasoup::worker::WorkerLogTag::Score),
                "simulcast" => Some(mediasoup::worker::WorkerLogTag::Simulcast),
                "svc" => Some(mediasoup::worker::WorkerLogTag::Svc),
                "sctp" => Some(mediasoup::worker::WorkerLogTag::Sctp),
                "message" => Some(mediasoup::worker::WorkerLogTag::Message),
                _ => None,
            })
            .collect()
    }

    pub fn to_mediasoup_dtls_files(&self) -> Option<mediasoup::worker::WorkerDtlsFiles> {
        match (&self.dtls_certificate_file, &self.dtls_private_key_file) {
            (Some(cert), Some(key)) if !cert.trim().is_empty() && !key.trim().is_empty() => {
                Some(mediasoup::worker::WorkerDtlsFiles {
                    certificate: PathBuf::from(cert),
                    private_key: PathBuf::from(key),
                })
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediasoupWebRtcTransportConfig {
    #[serde(default = "default_initial_available_outgoing_bitrate")]
    pub initial_available_outgoing_bitrate: u64,
    #[serde(default)]
    pub max_incoming_bitrate: Option<u64>,
    #[serde(default)]
    pub min_outgoing_bitrate: Option<u64>,
    #[serde(default)]
    pub max_outgoing_bitrate: Option<u64>,
    #[serde(default = "default_max_message_size")]
    pub max_send_message_size: u32,
    #[serde(default = "default_max_message_size")]
    pub max_receive_message_size: u32,
    #[serde(default = "default_sctp_buffer_size")]
    pub sctp_send_buffer_size: u32,
    #[serde(default = "default_max_message_size")]
    pub sctp_per_stream_send_queue_limit: u32,
    #[serde(default = "default_sctp_buffer_size")]
    pub sctp_max_receiver_window_buffer_size: u32,
    #[serde(default)]
    pub send_buffer_size: Option<u32>,
    #[serde(default)]
    pub recv_buffer_size: Option<u32>,
    #[serde(default = "default_true")]
    pub enable_tcp: bool,
}

impl Default for MediasoupWebRtcTransportConfig {
    fn default() -> Self {
        Self {
            initial_available_outgoing_bitrate: default_initial_available_outgoing_bitrate(),
            max_incoming_bitrate: None,
            min_outgoing_bitrate: None,
            max_outgoing_bitrate: None,
            max_send_message_size: default_max_message_size(),
            max_receive_message_size: default_max_message_size(),
            sctp_send_buffer_size: default_sctp_buffer_size(),
            sctp_per_stream_send_queue_limit: default_max_message_size(),
            sctp_max_receiver_window_buffer_size: default_sctp_buffer_size(),
            send_buffer_size: None,
            recv_buffer_size: None,
            enable_tcp: true,
        }
    }
}

fn default_initial_available_outgoing_bitrate() -> u64 {
    1_000_000
}

fn default_max_message_size() -> u32 {
    5_000_000
}

fn default_sctp_buffer_size() -> u32 {
    7_000_000
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediasoupPlainTransportConfig {
    #[serde(default = "default_max_message_size")]
    pub max_send_message_size: u32,
    #[serde(default = "default_max_message_size")]
    pub max_receive_message_size: u32,
    #[serde(default = "default_sctp_buffer_size")]
    pub sctp_send_buffer_size: u32,
}

impl Default for MediasoupPlainTransportConfig {
    fn default() -> Self {
        Self {
            max_send_message_size: default_max_message_size(),
            max_receive_message_size: default_max_message_size(),
            sctp_send_buffer_size: default_sctp_buffer_size(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediasoupAudioLevelObserverConfig {
    #[serde(default = "default_observer_interval")]
    pub interval_ms: u16,
    #[serde(default = "default_observer_threshold")]
    pub threshold_dbov: i8,
}

impl Default for MediasoupAudioLevelObserverConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_observer_interval(),
            threshold_dbov: default_observer_threshold(),
        }
    }
}

fn default_observer_interval() -> u16 {
    500
}

fn default_observer_threshold() -> i8 {
    -80
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediasoupRouterCodecsConfig {
    #[serde(default = "default_true")]
    pub enable_opus: bool,
    #[serde(default = "default_true")]
    pub enable_vp8: bool,
    #[serde(default = "default_true")]
    pub enable_h264: bool,
    #[serde(default)]
    pub enable_vp9: bool,
    #[serde(default = "default_start_bitrate")]
    pub google_start_bitrate: u32,
}

impl Default for MediasoupRouterCodecsConfig {
    fn default() -> Self {
        Self {
            enable_opus: true,
            enable_vp8: true,
            enable_h264: true,
            enable_vp9: false,
            google_start_bitrate: 1000,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_start_bitrate() -> u32 {
    1000
}

fn default_keyframe_retry_count() -> usize {
    3
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub listen_ip: IpAddr,
    pub announced_address: Option<String>,
    pub port_range: RangeInclusive<u16>,
    pub grpc_port: u16,
    pub worker_count: usize,
    pub worker_capacity_weight: u32,
    pub worker_spillover_threshold_percent: f32,

    #[serde(default)]
    pub worker_settings: MediasoupWorkerConfig,
    #[serde(default)]
    pub webrtc_transport: MediasoupWebRtcTransportConfig,
    #[serde(default)]
    pub plain_transport: MediasoupPlainTransportConfig,
    #[serde(default)]
    pub audio_level_observer: MediasoupAudioLevelObserverConfig,
    #[serde(default)]
    pub router_codecs: MediasoupRouterCodecsConfig,
    #[serde(default)]
    pub observability: ObservabilityConfig,
    #[serde(default)]
    pub license_token: Option<String>,
    #[serde(default)]
    pub license_public_key_hex: Option<String>,
    #[serde(default)]
    pub max_active_workers: Option<usize>,
    #[serde(default)]
    pub max_consumers_per_worker: Option<usize>,
    #[serde(default = "default_keyframe_retry_count")]
    pub keyframe_retry_count: usize,
    #[serde(default)]
    pub default_video_preferred_spatial_layer: Option<u8>,
}

impl Config {
    /// RTC UDP port range.
    pub fn port_range(&self) -> RangeInclusive<u16> {
        self.port_range.clone()
    }

    /// Announced address for WebRTC ICE candidates.
    pub fn announced_address(&self) -> Option<&str> {
        self.announced_address.as_deref()
    }

    /// Construct a `ListenInfo` configuration for mediasoup transports.
    pub fn create_listen_info(
        &self,
        protocol: mediasoup::prelude::Protocol,
    ) -> mediasoup::prelude::ListenInfo {
        mediasoup::prelude::ListenInfo {
            protocol,
            ip: self.listen_ip,
            announced_address: self.announced_address.clone(),
            expose_internal_ip: false,
            port: None,
            port_range: Some(self.port_range.clone()),
            flags: None,
            send_buffer_size: self.webrtc_transport.send_buffer_size,
            recv_buffer_size: self.webrtc_transport.recv_buffer_size,
        }
    }

    /// Construct WebRtcTransportListenInfos with UDP and optionally TCP candidate.
    pub fn create_webrtc_listen_infos(&self) -> mediasoup::prelude::WebRtcTransportListenInfos {
        let mut infos = mediasoup::prelude::WebRtcTransportListenInfos::new(
            self.create_listen_info(mediasoup::prelude::Protocol::Udp),
        );
        if self.webrtc_transport.enable_tcp {
            infos = infos.insert(self.create_listen_info(mediasoup::prelude::Protocol::Tcp));
        }
        infos
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::from_env()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawConfigFile {
    pub network: Option<RawNetworkConfig>,
    pub worker: Option<RawWorkerConfig>,
    pub webrtc_transport: Option<MediasoupWebRtcTransportConfig>,
    pub plain_transport: Option<MediasoupPlainTransportConfig>,
    pub audio_level_observer: Option<MediasoupAudioLevelObserverConfig>,
    pub router_codecs: Option<MediasoupRouterCodecsConfig>,
    pub observability: Option<RawObservabilityConfig>,
    pub license: Option<RawLicenseConfig>,
    pub keyframe_retry_count: Option<usize>,
    pub default_video_preferred_spatial_layer: Option<u8>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawLicenseConfig {
    #[serde(alias = "token", alias = "key")]
    pub license_token: Option<String>,
    #[serde(alias = "public_key", alias = "public_key_hex")]
    pub license_public_key_hex: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawNetworkConfig {
    pub listen_ip: Option<String>,
    pub announced_address: Option<String>,
    pub port_range: Option<String>,
    pub grpc_port: Option<u16>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawWorkerConfig {
    pub worker_count: Option<usize>,
    pub capacity_weight: Option<u32>,
    pub spillover_threshold_percent: Option<f32>,
    pub log_level: Option<String>,
    pub log_tags: Option<Vec<String>>,
    pub dtls_certificate_file: Option<String>,
    pub dtls_private_key_file: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawObservabilityConfig {
    pub log_level: Option<String>,
    pub log_format: Option<String>,
    pub tracing_enabled: Option<bool>,
    #[serde(alias = "otlp_endpoint")]
    pub tracing_endpoint: Option<String>,
    #[serde(alias = "trace_sample_rate")]
    pub tracing_sample_rate: Option<f64>,
    pub metrics_enabled: Option<bool>,
    pub metrics_addr: Option<String>,
    pub log_raw_sdp: Option<bool>,
}

/// Parses a port range string such as "40000-49999", "40000..=49999", or "40000..49999".
pub fn parse_port_range(s: &str) -> Option<(u16, u16)> {
    let s = s.trim();
    if let Some((min_s, max_s)) = s.split_once("..=") {
        if let (Ok(min), Ok(max)) = (min_s.trim().parse::<u16>(), max_s.trim().parse::<u16>()) {
            return Some((min, max));
        }
    }
    if let Some((min_s, max_s)) = s.split_once("..") {
        if let (Ok(min), Ok(max)) = (min_s.trim().parse::<u16>(), max_s.trim().parse::<u16>()) {
            return Some((min, max));
        }
    }
    if let Some((min_s, max_s)) = s.split_once('-') {
        if let (Ok(min), Ok(max)) = (min_s.trim().parse::<u16>(), max_s.trim().parse::<u16>()) {
            return Some((min, max));
        }
    }
    None
}

/// Probes the OS routing table using an unconnected UDP socket to find the
/// primary non-loopback IP address used by this host to reach network destinations.
/// No network packets are actually transmitted across the wire.
pub fn detect_primary_non_loopback_ip() -> Option<IpAddr> {
    let targets = [
        "8.8.8.8:80",
        "1.1.1.1:80",
        "10.255.255.255:80",
        "192.168.255.255:80",
        "172.31.255.255:80",
    ];
    for target in targets {
        if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
            if socket.connect(target).is_ok() {
                if let Ok(local_addr) = socket.local_addr() {
                    let ip = local_addr.ip();
                    if !ip.is_loopback() && !ip.is_unspecified() {
                        return Some(ip);
                    }
                }
            }
        }
    }
    None
}

impl Config {
    pub fn default_values() -> Self {
        Self {
            listen_ip: IpAddr::from([0, 0, 0, 0]),
            announced_address: detect_primary_non_loopback_ip()
                .map(|ip| ip.to_string())
                .or_else(|| Some("127.0.0.1".to_string())),
            port_range: 40000..=49999,
            grpc_port: 50051,
            worker_count: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1),
            worker_capacity_weight: 1000,
            worker_spillover_threshold_percent: 80.0,
            worker_settings: MediasoupWorkerConfig::default(),
            webrtc_transport: MediasoupWebRtcTransportConfig::default(),
            plain_transport: MediasoupPlainTransportConfig::default(),
            audio_level_observer: MediasoupAudioLevelObserverConfig::default(),
            router_codecs: MediasoupRouterCodecsConfig::default(),
            observability: ObservabilityConfig::default(),
            license_token: None,
            license_public_key_hex: None,
            max_active_workers: None,
            max_consumers_per_worker: None,
            keyframe_retry_count: 3,
            default_video_preferred_spatial_layer: None,
        }
    }

    pub fn try_from_env() -> Result<Self, ConfigError> {
        Self::try_load_from_file_and_env(None)
    }

    pub fn try_from_file(path: &Path) -> Result<Self, ConfigError> {
        Self::try_load_from_file_and_env(Some(path))
    }

    pub fn try_load_from_file_and_env(custom_path: Option<&Path>) -> Result<Self, ConfigError> {
        let mut config = Self::default_values();

        // 1. Locate config file: explicit path -> SFU_CONFIG_FILE -> config.yaml -> config.yml -> config.conf
        let file_path = custom_path
            .map(|p| p.to_path_buf())
            .or_else(|| std::env::var("SFU_CONFIG_FILE").ok().map(PathBuf::from))
            .or_else(|| {
                let candidates = ["config.yaml", "config.yml", "config.conf"];
                for candidate in candidates {
                    let p = PathBuf::from(candidate);
                    if p.exists() {
                        return Some(p);
                    }
                }
                None
            });

        // 2. Load and merge YAML/conf file if present
        if let Some(path) = file_path {
            if path.exists() {
                let content =
                    std::fs::read_to_string(&path).map_err(|e| ConfigError::FileReadError {
                        path: path.display().to_string(),
                        err: e.to_string(),
                    })?;
                let raw: RawConfigFile =
                    serde_yaml::from_str(&content).map_err(|e| ConfigError::YamlParseError {
                        path: path.display().to_string(),
                        err: e.to_string(),
                    })?;
                config.apply_raw_file(raw)?;
            } else if custom_path.is_some() || std::env::var("SFU_CONFIG_FILE").is_ok() {
                return Err(ConfigError::FileReadError {
                    path: path.display().to_string(),
                    err: "No such file or directory".to_string(),
                });
            }
        }

        // 3. Apply Environment Variable overrides (highest priority per 12-factor)
        config.apply_env_overrides()?;

        // 4. Validate invariants
        if config.port_range.start() > config.port_range.end() {
            return Err(ConfigError::InvalidPortRange {
                min: *config.port_range.start(),
                max: *config.port_range.end(),
            });
        }

        if config.worker_count == 0 {
            return Err(ConfigError::InvalidWorkerCount);
        }

        Ok(config)
    }

    pub fn from_env() -> Self {
        Self::try_from_env().unwrap_or_else(|e| {
            eprintln!("Configuration warning: {e}. Falling back to default configuration.");
            Self::default_values()
        })
    }

    fn apply_raw_file(&mut self, raw: RawConfigFile) -> Result<(), ConfigError> {
        if let Some(net) = raw.network {
            if let Some(ip_str) = net.listen_ip {
                let trimmed = ip_str.trim();
                if !trimmed.is_empty() {
                    self.listen_ip = IpAddr::from_str(trimmed)
                        .map_err(|_| ConfigError::InvalidListenIp(trimmed.to_string()))?;
                }
            }
            if let Some(ann) = net.announced_address {
                let trimmed = ann.trim();
                if trimmed.eq_ignore_ascii_case("auto") {
                    self.announced_address = detect_primary_non_loopback_ip()
                        .map(|ip| ip.to_string())
                        .or_else(|| Some("127.0.0.1".to_string()));
                } else if trimmed.eq_ignore_ascii_case("none") {
                    self.announced_address = None;
                } else if !trimmed.is_empty() {
                    self.announced_address = Some(trimmed.to_string());
                }
            }
            if let Some(ref range_str) = net.port_range {
                if let Some((min, max)) = parse_port_range(range_str) {
                    self.port_range = min..=max;
                }
            }
            if let Some(port) = net.grpc_port {
                self.grpc_port = port;
            }
        }

        if let Some(w) = raw.worker {
            if let Some(count) = w.worker_count {
                if count > 0 {
                    self.worker_count = count;
                } else {
                    self.worker_count = std::thread::available_parallelism()
                        .map(|n| n.get())
                        .unwrap_or(1);
                }
            }
            if let Some(cap) = w.capacity_weight {
                self.worker_capacity_weight = cap.max(1);
            }
            if let Some(spill) = w.spillover_threshold_percent {
                self.worker_spillover_threshold_percent = spill.clamp(1.0, 100.0);
            }
            if let Some(lvl) = w.log_level {
                self.worker_settings.log_level = lvl;
            }
            if let Some(tags) = w.log_tags {
                self.worker_settings.log_tags = tags;
            }
            if let Some(cert) = w.dtls_certificate_file {
                self.worker_settings.dtls_certificate_file = Some(cert);
            }
            if let Some(key) = w.dtls_private_key_file {
                self.worker_settings.dtls_private_key_file = Some(key);
            }
        }

        if let Some(wt) = raw.webrtc_transport {
            self.webrtc_transport = wt;
        }

        if let Some(pt) = raw.plain_transport {
            self.plain_transport = pt;
        }

        if let Some(alo) = raw.audio_level_observer {
            self.audio_level_observer = alo;
        }

        if let Some(rc) = raw.router_codecs {
            self.router_codecs = rc;
        }

        if let Some(obs) = raw.observability {
            if let Some(lvl) = obs.log_level {
                self.observability.log_level = lvl;
            }
            if let Some(fmt) = obs.log_format {
                let lower = fmt.trim().to_lowercase();
                if lower == "json" || lower == "pretty" || lower == "text" {
                    self.observability.log_format = lower;
                } else {
                    return Err(ConfigError::InvalidLogFormat(fmt));
                }
            }
            if let Some(tr) = obs.tracing_enabled {
                self.observability.tracing_enabled = tr;
            }
            if let Some(endpoint) = obs.tracing_endpoint {
                self.observability.tracing_endpoint = endpoint;
            }
            if let Some(rate) = obs.tracing_sample_rate {
                self.observability.tracing_sample_rate = rate;
            }
            if let Some(me) = obs.metrics_enabled {
                self.observability.metrics_enabled = me;
            }
            if let Some(addr_str) = obs.metrics_addr {
                let trimmed = addr_str.trim();
                let addr =
                    SocketAddr::from_str(trimmed).map_err(|e| ConfigError::InvalidSocketAddr {
                        key: "metrics_addr",
                        val: trimmed.to_string(),
                        err: e.to_string(),
                    })?;
                self.observability.metrics_addr = addr;
            }
            if let Some(sdp) = obs.log_raw_sdp {
                self.observability.log_raw_sdp = sdp;
            }
        }

        if let Some(lic) = raw.license {
            if let Some(token) = lic.license_token {
                let trimmed = token.trim();
                if !trimmed.is_empty() {
                    self.license_token = Some(trimmed.to_string());
                }
            }
            if let Some(pk) = lic.license_public_key_hex {
                let trimmed = pk.trim();
                if !trimmed.is_empty() {
                    self.license_public_key_hex = Some(trimmed.to_string());
                }
            }
        }

        if let Some(k) = raw.keyframe_retry_count {
            self.keyframe_retry_count = k;
        }

        if let Some(l) = raw.default_video_preferred_spatial_layer {
            self.default_video_preferred_spatial_layer = Some(l);
        }

        Ok(())
    }

    fn apply_env_overrides(&mut self) -> Result<(), ConfigError> {
        if let Ok(val) = std::env::var("MEDIASOUP_LISTEN_IP") {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                self.listen_ip = IpAddr::from_str(trimmed)
                    .map_err(|_| ConfigError::InvalidListenIp(trimmed.to_string()))?;
            }
        }

        if let Ok(val) = std::env::var("MEDIASOUP_ANNOUNCED_ADDRESS") {
            let trimmed = val.trim();
            if trimmed.eq_ignore_ascii_case("auto") {
                self.announced_address = detect_primary_non_loopback_ip()
                    .map(|ip| ip.to_string())
                    .or_else(|| Some("127.0.0.1".to_string()));
            } else if trimmed.eq_ignore_ascii_case("none") {
                self.announced_address = None;
            } else if !trimmed.is_empty() {
                self.announced_address = Some(trimmed.to_string());
            }
        }

        if let Ok(s) = std::env::var("RTC_PORT_RANGE") {
            if let Some((min, max)) = parse_port_range(&s) {
                self.port_range = min..=max;
            }
        }

        if let Ok(s) = std::env::var("GRPC_PORT") {
            self.grpc_port = s
                .trim()
                .parse::<u16>()
                .map_err(|e| ConfigError::ParseIntError {
                    key: "GRPC_PORT",
                    err: e.to_string(),
                })?;
        }

        if let Ok(s) = std::env::var("MEDIASOUP_WORKER_COUNT") {
            let count = s
                .trim()
                .parse::<usize>()
                .map_err(|e| ConfigError::ParseIntError {
                    key: "MEDIASOUP_WORKER_COUNT",
                    err: e.to_string(),
                })?;
            if count == 0 {
                return Err(ConfigError::InvalidWorkerCount);
            }
            self.worker_count = count;
        }

        if let Ok(s) = std::env::var("SFU_WORKER_CAPACITY_WEIGHT") {
            let weight = s
                .trim()
                .parse::<u32>()
                .map_err(|e| ConfigError::ParseIntError {
                    key: "SFU_WORKER_CAPACITY_WEIGHT",
                    err: e.to_string(),
                })?;
            if weight == 0 {
                return Err(ConfigError::ParseIntError {
                    key: "SFU_WORKER_CAPACITY_WEIGHT",
                    err: "must be greater than 0".to_string(),
                });
            }
            self.worker_capacity_weight = weight;
        }

        if let Ok(s) = std::env::var("SFU_WORKER_SPILLOVER_THRESHOLD_PERCENT") {
            self.worker_spillover_threshold_percent =
                s.trim().parse::<f32>().unwrap_or(80.0).clamp(1.0, 100.0);
        }

        if let Ok(s) = std::env::var("SFU_MAX_ACTIVE_WORKERS") {
            if let Ok(val) = s.trim().parse::<usize>() {
                self.max_active_workers = Some(val);
            }
        }

        if let Ok(s) = std::env::var("SFU_MAX_CONSUMERS_PER_WORKER") {
            if let Ok(val) = s.trim().parse::<usize>() {
                self.max_consumers_per_worker = Some(val);
            }
        }

        // Mediasoup worker env overrides
        if let Ok(s) = std::env::var("MEDIASOUP_WORKER_LOG_LEVEL") {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                self.worker_settings.log_level = trimmed.to_string();
            }
        }

        if let Ok(s) = std::env::var("MEDIASOUP_WORKER_LOG_TAGS") {
            let tags: Vec<String> = s
                .split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect();
            if !tags.is_empty() {
                self.worker_settings.log_tags = tags;
            }
        }

        if let Ok(s) = std::env::var("MEDIASOUP_DTLS_CERT")
            .or_else(|_| std::env::var("MEDIASOUP_DTLS_CERTIFICATE_FILE"))
            .or_else(|_| std::env::var("MEDIASOUP_WORKER_DTLS_CERT"))
        {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                self.worker_settings.dtls_certificate_file = Some(trimmed.to_string());
            }
        }

        if let Ok(s) = std::env::var("MEDIASOUP_DTLS_KEY")
            .or_else(|_| std::env::var("MEDIASOUP_DTLS_PRIVATE_KEY_FILE"))
            .or_else(|_| std::env::var("MEDIASOUP_WORKER_DTLS_KEY"))
        {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                self.worker_settings.dtls_private_key_file = Some(trimmed.to_string());
            }
        }

        // WebRtcTransport env overrides
        if let Ok(s) = std::env::var("MEDIASOUP_INITIAL_AVAILABLE_OUTGOING_BITRATE")
            .or_else(|_| std::env::var("MEDIASOUP_WEBRTC_INITIAL_AVAILABLE_OUTGOING_BITRATE"))
        {
            if let Ok(val) = s.trim().parse::<u64>() {
                self.webrtc_transport.initial_available_outgoing_bitrate = val;
            }
        }

        if let Ok(s) = std::env::var("MEDIASOUP_MAX_INCOMING_BITRATE") {
            if let Ok(val) = s.trim().parse::<u64>() {
                self.webrtc_transport.max_incoming_bitrate = Some(val);
            }
        }

        if let Ok(s) = std::env::var("MEDIASOUP_MIN_OUTGOING_BITRATE") {
            if let Ok(val) = s.trim().parse::<u64>() {
                self.webrtc_transport.min_outgoing_bitrate = Some(val);
            }
        }

        if let Ok(s) = std::env::var("MEDIASOUP_MAX_OUTGOING_BITRATE") {
            if let Ok(val) = s.trim().parse::<u64>() {
                self.webrtc_transport.max_outgoing_bitrate = Some(val);
            }
        }

        if let Ok(s) = std::env::var("MEDIASOUP_MAX_SEND_MESSAGE_SIZE") {
            if let Ok(val) = s.trim().parse::<u32>() {
                self.webrtc_transport.max_send_message_size = val;
            }
        }

        if let Ok(s) = std::env::var("MEDIASOUP_SCTP_SEND_BUFFER_SIZE") {
            if let Ok(val) = s.trim().parse::<u32>() {
                self.webrtc_transport.sctp_send_buffer_size = val;
            }
        }

        if let Ok(s) = std::env::var("MEDIASOUP_WEBRTC_SEND_BUFFER_SIZE") {
            if let Ok(val) = s.trim().parse::<u32>() {
                self.webrtc_transport.send_buffer_size = Some(val);
            }
        }

        if let Ok(s) = std::env::var("MEDIASOUP_WEBRTC_RECV_BUFFER_SIZE") {
            if let Ok(val) = s.trim().parse::<u32>() {
                self.webrtc_transport.recv_buffer_size = Some(val);
            }
        }

        // PlainTransport env overrides
        if let Ok(s) = std::env::var("MEDIASOUP_PLAIN_MAX_SEND_MESSAGE_SIZE") {
            if let Ok(val) = s.trim().parse::<u32>() {
                self.plain_transport.max_send_message_size = val;
            }
        }

        if let Ok(s) = std::env::var("MEDIASOUP_PLAIN_MAX_RECEIVE_MESSAGE_SIZE") {
            if let Ok(val) = s.trim().parse::<u32>() {
                self.plain_transport.max_receive_message_size = val;
            }
        }

        if let Ok(s) = std::env::var("MEDIASOUP_PLAIN_SCTP_SEND_BUFFER_SIZE") {
            if let Ok(val) = s.trim().parse::<u32>() {
                self.plain_transport.sctp_send_buffer_size = val;
            }
        }

        // AudioLevelObserver env overrides
        if let Ok(s) = std::env::var("MEDIASOUP_AUDIO_OBSERVER_INTERVAL_MS") {
            if let Ok(val) = s.trim().parse::<u16>() {
                self.audio_level_observer.interval_ms = val;
            }
        }

        if let Ok(s) = std::env::var("MEDIASOUP_AUDIO_OBSERVER_THRESHOLD_DB") {
            if let Ok(val) = s.trim().parse::<i8>() {
                self.audio_level_observer.threshold_dbov = val;
            }
        }

        // Codecs env overrides
        if let Ok(s) = std::env::var("MEDIASOUP_CODECS_ENABLE_VP9") {
            self.router_codecs.enable_vp9 = s.eq_ignore_ascii_case("true") || s == "1";
        }

        if let Ok(s) = std::env::var("MEDIASOUP_CODECS_START_BITRATE") {
            if let Ok(val) = s.trim().parse::<u32>() {
                self.router_codecs.google_start_bitrate = val;
            }
        }

        // Observability overrides
        self.observability.apply_env_overrides()?;

        // Licensing env overrides
        if let Ok(s) = std::env::var("EKBATAN_LICENSE_KEY") {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                self.license_token = Some(trimmed.to_string());
            }
        }

        if let Ok(s) = std::env::var("EKBATAN_LICENSE_PUBLIC_KEY") {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                self.license_public_key_hex = Some(trimmed.to_string());
            }
        }

        if let Ok(s) = std::env::var("SFU_KEYFRAME_RETRY_COUNT") {
            if let Ok(val) = s.trim().parse::<usize>() {
                self.keyframe_retry_count = val;
            }
        }

        if let Ok(s) = std::env::var("MEDIASOUP_DEFAULT_VIDEO_PREFERRED_SPATIAL_LAYER")
            .or_else(|_| std::env::var("SFU_DEFAULT_VIDEO_PREFERRED_SPATIAL_LAYER"))
        {
            if let Ok(val) = s.trim().parse::<u8>() {
                self.default_video_preferred_spatial_layer = Some(val);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_default_config() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("MEDIASOUP_LISTEN_IP");
        std::env::remove_var("RTC_PORT_RANGE");
        std::env::remove_var("MEDIASOUP_WORKER_COUNT");
        std::env::remove_var("SFU_CONFIG_FILE");
        let config = Config::default_values();
        assert_eq!(config.port_range, 40000..=49999);
        assert_eq!(config.port_range(), 40000..=49999);
        assert!(config.worker_count >= 1);
        assert_eq!(
            config.webrtc_transport.initial_available_outgoing_bitrate,
            1_000_000
        );
        assert_eq!(config.worker_settings.log_level, "warn");
        assert_eq!(config.audio_level_observer.interval_ms, 500);
    }

    fn with_env_var<F: FnOnce()>(key: &str, val: &str, f: F) {
        let _guard = ENV_LOCK.lock().unwrap();
        let prev = std::env::var(key).ok();
        std::env::set_var(key, val);
        f();
        match prev {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn test_invalid_port_range() {
        let _guard = ENV_LOCK.lock().unwrap();
        let prev = std::env::var("RTC_PORT_RANGE").ok();
        std::env::set_var("RTC_PORT_RANGE", "50000-40000");

        let res = Config::try_from_env();
        assert!(matches!(
            res,
            Err(ConfigError::InvalidPortRange {
                min: 50000,
                max: 40000
            })
        ));

        match prev {
            Some(v) => std::env::set_var("RTC_PORT_RANGE", v),
            None => std::env::remove_var("RTC_PORT_RANGE"),
        }
    }

    #[test]
    fn test_invalid_listen_ip() {
        with_env_var("MEDIASOUP_LISTEN_IP", "not-an-ip", || {
            let res = Config::try_from_env();
            assert!(matches!(res, Err(ConfigError::InvalidListenIp(_))));
        });
    }

    #[test]
    fn test_invalid_worker_count() {
        with_env_var("MEDIASOUP_WORKER_COUNT", "0", || {
            let res = Config::try_from_env();
            assert!(matches!(res, Err(ConfigError::InvalidWorkerCount)));
        });
    }

    #[test]
    fn test_invalid_worker_capacity_weight() {
        with_env_var("SFU_WORKER_CAPACITY_WEIGHT", "not-a-number", || {
            let res = Config::try_from_env();
            assert!(matches!(
                res,
                Err(ConfigError::ParseIntError {
                    key: "SFU_WORKER_CAPACITY_WEIGHT",
                    ..
                })
            ));
        });

        with_env_var("SFU_WORKER_CAPACITY_WEIGHT", "0", || {
            let res = Config::try_from_env();
            assert!(matches!(
                res,
                Err(ConfigError::ParseIntError {
                    key: "SFU_WORKER_CAPACITY_WEIGHT",
                    ..
                })
            ));
        });
    }

    #[test]
    fn test_invalid_log_format() {
        with_env_var("OBS_LOG_FORMAT", "invalid_format", || {
            let res = Config::try_from_env();
            assert!(matches!(res, Err(ConfigError::InvalidLogFormat(_))));
        });
    }

    #[test]
    fn test_invalid_metrics_addr() {
        with_env_var("OBS_METRICS_ADDR", "not-a-socket-addr", || {
            let res = Config::try_from_env();
            assert!(matches!(
                res,
                Err(ConfigError::InvalidSocketAddr {
                    key: "OBS_METRICS_ADDR",
                    ..
                })
            ));
        });
    }

    #[test]
    fn test_readme_env_var_aliases() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::set_var("MEDIASOUP_WORKER_DTLS_CERT", "/tmp/cert.pem");
        std::env::set_var("MEDIASOUP_WORKER_DTLS_KEY", "/tmp/key.pem");
        std::env::set_var(
            "MEDIASOUP_WEBRTC_INITIAL_AVAILABLE_OUTGOING_BITRATE",
            "850000",
        );

        let cfg = Config::try_from_env().unwrap();
        assert_eq!(
            cfg.worker_settings.dtls_certificate_file.as_deref(),
            Some("/tmp/cert.pem")
        );
        assert_eq!(
            cfg.worker_settings.dtls_private_key_file.as_deref(),
            Some("/tmp/key.pem")
        );
        assert_eq!(
            cfg.webrtc_transport.initial_available_outgoing_bitrate,
            850000
        );

        std::env::remove_var("MEDIASOUP_WORKER_DTLS_CERT");
        std::env::remove_var("MEDIASOUP_WORKER_DTLS_KEY");
        std::env::remove_var("MEDIASOUP_WEBRTC_INITIAL_AVAILABLE_OUTGOING_BITRATE");
    }

    #[test]
    fn test_yaml_config_loading_and_env_override() {
        let _guard = ENV_LOCK.lock().unwrap();
        let yaml_content = r#"
network:
  listen_ip: "127.0.0.1"
  announced_address: "192.168.1.100"
  port_range: "41000-42000"
  grpc_port: 50055

worker:
  worker_count: 4
  capacity_weight: 2000
  spillover_threshold_percent: 75.0
  log_level: "debug"
  log_tags:
    - ice
    - dtls

webrtc_transport:
  initial_available_outgoing_bitrate: 1500000
  max_incoming_bitrate: 4000000
  max_send_message_size: 2000000

audio_level_observer:
  interval_ms: 300
  threshold_dbov: -70

router_codecs:
  enable_opus: true
  enable_vp8: true
  enable_h264: true
  enable_vp9: true
  google_start_bitrate: 1500
"#;

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_sfu_config.yaml");
        std::fs::write(&file_path, yaml_content).unwrap();

        // 1. Load directly from file
        let config = Config::try_from_file(&file_path).unwrap();
        assert_eq!(config.listen_ip, IpAddr::from_str("127.0.0.1").unwrap());
        assert_eq!(config.announced_address.as_deref(), Some("192.168.1.100"));
        assert_eq!(config.port_range, 41000..=42000);
        assert_eq!(config.port_range(), 41000..=42000);
        assert_eq!(config.grpc_port, 50055);
        assert_eq!(config.worker_count, 4);
        assert_eq!(config.worker_capacity_weight, 2000);
        assert_eq!(config.worker_spillover_threshold_percent, 75.0);
        assert_eq!(config.worker_settings.log_level, "debug");
        assert_eq!(config.worker_settings.log_tags, vec!["ice", "dtls"]);
        assert_eq!(
            config.webrtc_transport.initial_available_outgoing_bitrate,
            1500000
        );
        assert_eq!(config.webrtc_transport.max_incoming_bitrate, Some(4000000));
        assert_eq!(config.audio_level_observer.interval_ms, 300);
        assert_eq!(config.audio_level_observer.threshold_dbov, -70);
        assert!(config.router_codecs.enable_vp9);
        assert_eq!(config.router_codecs.google_start_bitrate, 1500);

        // 2. Test ENV override takes precedence over YAML
        std::env::set_var("SFU_CONFIG_FILE", file_path.to_str().unwrap());
        std::env::set_var("GRPC_PORT", "50099");
        std::env::set_var("MEDIASOUP_WORKER_LOG_LEVEL", "error");

        let overridden = Config::try_from_env().unwrap();
        assert_eq!(overridden.grpc_port, 50099);
        assert_eq!(overridden.worker_settings.log_level, "error");
        // Non-overridden fields keep values from YAML
        assert_eq!(overridden.audio_level_observer.interval_ms, 300);

        std::env::remove_var("SFU_CONFIG_FILE");
        std::env::remove_var("GRPC_PORT");
        std::env::remove_var("MEDIASOUP_WORKER_LOG_LEVEL");
        let _ = std::fs::remove_file(file_path);
    }

    #[test]
    fn test_detect_primary_non_loopback_ip() {
        if let Some(ip) = detect_primary_non_loopback_ip() {
            assert!(!ip.is_loopback());
            assert!(!ip.is_unspecified());
        }
        let default_cfg = Config::default_values();
        assert!(default_cfg.announced_address.is_some());
    }

    #[test]
    fn test_announced_address_yaml_auto_and_none() {
        let _guard = ENV_LOCK.lock().unwrap();
        let yaml_auto = r#"
network:
  announced_address: "auto"
"#;
        let temp_dir = std::env::temp_dir();
        let path_auto = temp_dir.join("test_sfu_auto.yaml");
        std::fs::write(&path_auto, yaml_auto).unwrap();
        let cfg_auto = Config::try_from_file(&path_auto).unwrap();
        assert!(cfg_auto.announced_address.is_some());
        let _ = std::fs::remove_file(path_auto);

        let yaml_none = r#"
network:
  announced_address: "none"
"#;
        let path_none = temp_dir.join("test_sfu_none.yaml");
        std::fs::write(&path_none, yaml_none).unwrap();
        let cfg_none = Config::try_from_file(&path_none).unwrap();
        assert!(cfg_none.announced_address.is_none());
        let _ = std::fs::remove_file(path_none);

        // Test MEDIASOUP_ANNOUNCED_ADDRESS env var overrides
        std::env::set_var("MEDIASOUP_ANNOUNCED_ADDRESS", "198.51.100.1");
        let cfg_env = Config::try_from_env().unwrap();
        assert_eq!(cfg_env.announced_address.as_deref(), Some("198.51.100.1"));

        std::env::set_var("MEDIASOUP_ANNOUNCED_ADDRESS", "none");
        let cfg_env_none = Config::try_from_env().unwrap();
        assert!(cfg_env_none.announced_address.is_none());

        std::env::set_var("MEDIASOUP_ANNOUNCED_ADDRESS", "auto");
        let cfg_env_auto = Config::try_from_env().unwrap();
        assert!(cfg_env_auto.announced_address.is_some());

        std::env::remove_var("MEDIASOUP_ANNOUNCED_ADDRESS");
    }

    #[test]
    fn test_modern_port_range_and_listen_info() {
        let _guard = ENV_LOCK.lock().unwrap();
        // 1. Test parse_port_range formats
        assert_eq!(parse_port_range("40000-49999"), Some((40000, 49999)));
        assert_eq!(parse_port_range("40000..=49999"), Some((40000, 49999)));
        assert_eq!(parse_port_range("40000..49999"), Some((40000, 49999)));
        assert_eq!(parse_port_range("invalid"), None);

        // 2. Test YAML loading with port_range
        let yaml = r#"
network:
  listen_ip: "127.0.0.1"
  port_range: "42000-43000"
"#;
        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("test_port_range.yaml");
        std::fs::write(&path, yaml).unwrap();
        let cfg = Config::try_from_file(&path).unwrap();
        assert_eq!(cfg.port_range(), 42000..=43000);
        assert_eq!(cfg.port_range, 42000..=43000);
        let _ = std::fs::remove_file(path);

        // 3. Test ENV override with RTC_PORT_RANGE
        std::env::set_var("RTC_PORT_RANGE", "45000..=46000");
        let cfg_env = Config::try_from_env().unwrap();
        assert_eq!(cfg_env.port_range(), 45000..=46000);
        assert_eq!(cfg_env.port_range, 45000..=46000);
        std::env::remove_var("RTC_PORT_RANGE");

        // 4. Test create_listen_info structure
        let listen_info = cfg.create_listen_info(mediasoup::prelude::Protocol::Udp);
        assert_eq!(listen_info.protocol, mediasoup::prelude::Protocol::Udp);
        assert_eq!(listen_info.ip, cfg.listen_ip);
        assert_eq!(listen_info.port_range, Some(cfg.port_range()));
        assert_eq!(listen_info.port, None);
    }
}
