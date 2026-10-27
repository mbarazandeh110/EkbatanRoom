use std::net::SocketAddr;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq)]
pub struct ObservabilityConfig {
    pub log_level: String,
    pub log_format: String,
    pub tracing_enabled: bool,
    pub tracing_endpoint: String,
    pub tracing_sample_rate: f64,
    pub metrics_enabled: bool,
    pub metrics_addr: SocketAddr,
    pub log_raw_sdp: bool,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

impl ObservabilityConfig {
    pub fn from_env() -> Self {
        let log_level = std::env::var("OBS_LOG_LEVEL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "info".to_string());

        let log_format = std::env::var("OBS_LOG_FORMAT")
            .ok()
            .map(|s| s.to_lowercase())
            .filter(|s| s == "json" || s == "pretty" || s == "text")
            .unwrap_or_else(|| "json".to_string());

        let tracing_enabled = std::env::var("OBS_TRACING_ENABLED")
            .ok()
            .map(|s| s.eq_ignore_ascii_case("true") || s == "1")
            .unwrap_or(false);

        // Prioritize OBS_OTLP_ENDPOINT per SIGNALING_CONTRACT.md, fallback to OBS_TRACING_ENDPOINT
        let tracing_endpoint = std::env::var("OBS_OTLP_ENDPOINT")
            .or_else(|_| std::env::var("OBS_TRACING_ENDPOINT"))
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "http://localhost:4317".to_string());

        // Prioritize OBS_TRACE_SAMPLE_RATE per SIGNALING_CONTRACT.md (default 0.1), fallback to OBS_TRACING_SAMPLE_RATE
        let tracing_sample_rate = std::env::var("OBS_TRACE_SAMPLE_RATE")
            .or_else(|_| std::env::var("OBS_TRACING_SAMPLE_RATE"))
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|&r| (0.0..=1.0).contains(&r))
            .unwrap_or(0.1);

        let metrics_enabled = std::env::var("OBS_METRICS_ENABLED")
            .ok()
            .map(|s| s.eq_ignore_ascii_case("true") || s == "1")
            .unwrap_or(true);

        let metrics_addr = std::env::var("OBS_METRICS_ADDR")
            .ok()
            .and_then(|s| SocketAddr::from_str(&s).ok())
            .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 9090)));

        let log_raw_sdp = std::env::var("OBS_LOG_RAW_SDP")
            .ok()
            .map(|s| s.eq_ignore_ascii_case("true") || s == "1")
            .unwrap_or(false);

        Self {
            log_level,
            log_format,
            tracing_enabled,
            tracing_endpoint,
            tracing_sample_rate,
            metrics_enabled,
            metrics_addr,
            log_raw_sdp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static OBS_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_default_observability_config() {
        let _guard = OBS_ENV_LOCK.lock().unwrap();
        let config = ObservabilityConfig::default();
        assert_eq!(config.log_level, "info");
        assert_eq!(config.log_format, "json");
        assert!(!config.tracing_enabled);
        assert_eq!(config.tracing_endpoint, "http://localhost:4317");
        assert_eq!(config.tracing_sample_rate, 0.1);
        assert!(config.metrics_enabled);
        assert_eq!(config.metrics_addr, SocketAddr::from(([0, 0, 0, 0], 9090)));
        assert!(!config.log_raw_sdp);
    }

    #[test]
    fn test_observability_config_from_env() {
        let _guard = OBS_ENV_LOCK.lock().unwrap();

        let original_vars: Vec<(&str, Option<String>)> = vec![
            ("OBS_LOG_LEVEL", std::env::var("OBS_LOG_LEVEL").ok()),
            ("OBS_LOG_FORMAT", std::env::var("OBS_LOG_FORMAT").ok()),
            (
                "OBS_TRACING_ENABLED",
                std::env::var("OBS_TRACING_ENABLED").ok(),
            ),
            (
                "OBS_TRACING_ENDPOINT",
                std::env::var("OBS_TRACING_ENDPOINT").ok(),
            ),
            (
                "OBS_TRACING_SAMPLE_RATE",
                std::env::var("OBS_TRACING_SAMPLE_RATE").ok(),
            ),
            (
                "OBS_METRICS_ENABLED",
                std::env::var("OBS_METRICS_ENABLED").ok(),
            ),
            ("OBS_METRICS_ADDR", std::env::var("OBS_METRICS_ADDR").ok()),
            ("OBS_LOG_RAW_SDP", std::env::var("OBS_LOG_RAW_SDP").ok()),
        ];

        std::env::set_var("OBS_LOG_LEVEL", "debug");
        std::env::set_var("OBS_LOG_FORMAT", "text");
        std::env::set_var("OBS_TRACING_ENABLED", "true");
        std::env::set_var("OBS_TRACING_ENDPOINT", "http://collector:4317");
        std::env::set_var("OBS_TRACING_SAMPLE_RATE", "0.5");
        std::env::set_var("OBS_METRICS_ENABLED", "false");
        std::env::set_var("OBS_METRICS_ADDR", "127.0.0.1:9191");
        std::env::set_var("OBS_LOG_RAW_SDP", "true");

        let config = ObservabilityConfig::from_env();
        assert_eq!(config.log_level, "debug");
        assert_eq!(config.log_format, "text");
        assert!(config.tracing_enabled);
        assert_eq!(config.tracing_endpoint, "http://collector:4317");
        assert_eq!(config.tracing_sample_rate, 0.5);
        assert!(!config.metrics_enabled);
        assert_eq!(
            config.metrics_addr,
            SocketAddr::from(([127, 0, 0, 1], 9191))
        );
        assert!(config.log_raw_sdp);

        for (key, val) in original_vars {
            match val {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }
}
