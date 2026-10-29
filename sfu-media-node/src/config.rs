use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use thiserror::Error;

use crate::observability::ObservabilityConfig;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("Invalid listen IP '{0}': must be a valid IPv4 or IPv6 address")]
    InvalidListenIp(String),

    #[error(
        "Invalid RTC port range: RTC_MIN_PORT ({min}) cannot be greater than RTC_MAX_PORT ({max})"
    )]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub listen_ip: IpAddr,
    pub announced_ip: Option<String>,
    pub rtc_min_port: u16,
    pub rtc_max_port: u16,
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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawNetworkConfig {
    pub listen_ip: Option<String>,
    pub announced_ip: Option<String>,
    pub rtc_min_port: Option<u16>,
    pub rtc_max_port: Option<u16>,
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

impl Config {
    pub fn default_values() -> Self {
        Self {
            listen_ip: IpAddr::from([0, 0, 0, 0]),
            announced_ip: Some("127.0.0.1".to_string()),
            rtc_min_port: 40000,
            rtc_max_port: 49999,
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

        // 1. Locate config file: explicit path -> SFU_CONFIG_FILE -> config.yaml -> config.yml
        let file_path = custom_path
            .map(|p| p.to_path_buf())
            .or_else(|| std::env::var("SFU_CONFIG_FILE").ok().map(PathBuf::from))
            .or_else(|| {
                let p_yaml = PathBuf::from("config.yaml");
                if p_yaml.exists() {
                    Some(p_yaml)
                } else {
                    let p_yml = PathBuf::from("config.yml");
                    if p_yml.exists() {
                        Some(p_yml)
                    } else {
                        None
                    }
                }
            });

        // 2. Load and merge YAML file if present
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
            }
        }

        // 3. Apply Environment Variable overrides (highest priority per 12-factor)
        config.apply_env_overrides()?;

        // 4. Validate invariants
        if config.rtc_min_port > config.rtc_max_port {
            return Err(ConfigError::InvalidPortRange {
                min: config.rtc_min_port,
                max: config.rtc_max_port,
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
            if let Some(ann) = net.announced_ip {
                let trimmed = ann.trim();
                if !trimmed.is_empty() {
                    self.announced_ip = Some(trimmed.to_string());
                }
            }
            if let Some(min_port) = net.rtc_min_port {
                self.rtc_min_port = min_port;
            }
            if let Some(max_port) = net.rtc_max_port {
                self.rtc_max_port = max_port;
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

        if let Ok(val) = std::env::var("MEDIASOUP_ANNOUNCED_IP")
            .or_else(|_| std::env::var("MEDIASOUP_ANNOUNCED_ADDRESS"))
        {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                self.announced_ip = Some(trimmed.to_string());
            }
        }

        if let Ok(s) = std::env::var("RTC_MIN_PORT") {
            self.rtc_min_port =
                s.trim()
                    .parse::<u16>()
                    .map_err(|e| ConfigError::ParseIntError {
                        key: "RTC_MIN_PORT",
                        err: e.to_string(),
                    })?;
        }

        if let Ok(s) = std::env::var("RTC_MAX_PORT") {
            self.rtc_max_port =
                s.trim()
                    .parse::<u16>()
                    .map_err(|e| ConfigError::ParseIntError {
                        key: "RTC_MAX_PORT",
                        err: e.to_string(),
                    })?;
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
        {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                self.worker_settings.dtls_certificate_file = Some(trimmed.to_string());
            }
        }

        if let Ok(s) = std::env::var("MEDIASOUP_DTLS_KEY")
            .or_else(|_| std::env::var("MEDIASOUP_DTLS_PRIVATE_KEY_FILE"))
        {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                self.worker_settings.dtls_private_key_file = Some(trimmed.to_string());
            }
        }

        // WebRtcTransport env overrides
        if let Ok(s) = std::env::var("MEDIASOUP_INITIAL_AVAILABLE_OUTGOING_BITRATE") {
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
        if let Ok(val) = std::env::var("OBS_LOG_LEVEL").or_else(|_| std::env::var("RUST_LOG")) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                self.observability.log_level = trimmed.to_string();
            }
        }

        if let Ok(val) = std::env::var("OBS_LOG_FORMAT") {
            let lower = val.trim().to_lowercase();
            if lower == "json" || lower == "pretty" || lower == "text" {
                self.observability.log_format = lower;
            } else {
                return Err(ConfigError::InvalidLogFormat(val));
            }
        }

        if let Ok(val) = std::env::var("OBS_TRACING_ENABLED") {
            self.observability.tracing_enabled = val.eq_ignore_ascii_case("true") || val == "1";
        }

        if let Ok(val) =
            std::env::var("OBS_OTLP_ENDPOINT").or_else(|_| std::env::var("OBS_TRACING_ENDPOINT"))
        {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                self.observability.tracing_endpoint = trimmed.to_string();
            }
        }

        if let Ok(val) = std::env::var("OBS_TRACE_SAMPLE_RATE")
            .or_else(|_| std::env::var("OBS_TRACING_SAMPLE_RATE"))
        {
            if let Ok(r) = val.parse::<f64>() {
                if (0.0..=1.0).contains(&r) {
                    self.observability.tracing_sample_rate = r;
                }
            }
        }

        if let Ok(val) = std::env::var("OBS_METRICS_ENABLED") {
            self.observability.metrics_enabled = val.eq_ignore_ascii_case("true") || val == "1";
        }

        if let Ok(val) = std::env::var("OBS_METRICS_ADDR") {
            let trimmed = val.trim();
            let addr =
                SocketAddr::from_str(trimmed).map_err(|e| ConfigError::InvalidSocketAddr {
                    key: "OBS_METRICS_ADDR",
                    val: trimmed.to_string(),
                    err: e.to_string(),
                })?;
            self.observability.metrics_addr = addr;
        }

        if let Ok(val) = std::env::var("OBS_LOG_RAW_SDP") {
            self.observability.log_raw_sdp = val.eq_ignore_ascii_case("true") || val == "1";
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
        std::env::remove_var("RTC_MIN_PORT");
        std::env::remove_var("RTC_MAX_PORT");
        std::env::remove_var("MEDIASOUP_WORKER_COUNT");
        std::env::remove_var("SFU_CONFIG_FILE");
        let config = Config::default_values();
        assert_eq!(config.rtc_min_port, 40000);
        assert_eq!(config.rtc_max_port, 49999);
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
        let prev_min = std::env::var("RTC_MIN_PORT").ok();
        let prev_max = std::env::var("RTC_MAX_PORT").ok();
        std::env::set_var("RTC_MIN_PORT", "50000");
        std::env::set_var("RTC_MAX_PORT", "40000");

        let res = Config::try_from_env();
        assert!(matches!(res, Err(ConfigError::InvalidPortRange { .. })));

        match prev_min {
            Some(v) => std::env::set_var("RTC_MIN_PORT", v),
            None => std::env::remove_var("RTC_MIN_PORT"),
        }
        match prev_max {
            Some(v) => std::env::set_var("RTC_MAX_PORT", v),
            None => std::env::remove_var("RTC_MAX_PORT"),
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
    fn test_yaml_config_loading_and_env_override() {
        let _guard = ENV_LOCK.lock().unwrap();
        let yaml_content = r#"
network:
  listen_ip: "127.0.0.1"
  announced_ip: "192.168.1.100"
  rtc_min_port: 41000
  rtc_max_port: 42000
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
        assert_eq!(config.announced_ip.as_deref(), Some("192.168.1.100"));
        assert_eq!(config.rtc_min_port, 41000);
        assert_eq!(config.rtc_max_port, 42000);
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
}
