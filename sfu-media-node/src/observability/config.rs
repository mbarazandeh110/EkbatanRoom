use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
        Self {
            log_level: "info".to_string(),
            log_format: "json".to_string(),
            tracing_enabled: false,
            tracing_endpoint: "http://localhost:4317".to_string(),
            tracing_sample_rate: 0.1,
            metrics_enabled: true,
            metrics_addr: SocketAddr::from(([0, 0, 0, 0], 9090)),
            log_raw_sdp: false,
        }
    }
}

impl ObservabilityConfig {
    pub fn apply_env_overrides(&mut self) -> Result<(), crate::config::ConfigError> {
        if let Ok(val) = std::env::var("OBS_LOG_LEVEL").or_else(|_| std::env::var("RUST_LOG")) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                self.log_level = trimmed.to_string();
            }
        }

        if let Ok(val) = std::env::var("OBS_LOG_FORMAT") {
            let lower = val.trim().to_lowercase();
            if lower == "json" || lower == "pretty" || lower == "text" {
                self.log_format = lower;
            } else {
                return Err(crate::config::ConfigError::InvalidLogFormat(val));
            }
        }

        if let Ok(val) = std::env::var("OBS_TRACING_ENABLED") {
            self.tracing_enabled = val.eq_ignore_ascii_case("true") || val == "1";
        }

        if let Ok(val) =
            std::env::var("OBS_OTLP_ENDPOINT").or_else(|_| std::env::var("OBS_TRACING_ENDPOINT"))
        {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                self.tracing_endpoint = trimmed.to_string();
            }
        }

        if let Ok(val) = std::env::var("OBS_TRACE_SAMPLE_RATE")
            .or_else(|_| std::env::var("OBS_TRACING_SAMPLE_RATE"))
        {
            if let Ok(r) = val.parse::<f64>() {
                if (0.0..=1.0).contains(&r) {
                    self.tracing_sample_rate = r;
                }
            }
        }

        if let Ok(val) = std::env::var("OBS_METRICS_ENABLED") {
            self.metrics_enabled = val.eq_ignore_ascii_case("true") || val == "1";
        }

        if let Ok(val) = std::env::var("OBS_METRICS_ADDR") {
            let trimmed = val.trim();
            let addr = SocketAddr::from_str(trimmed).map_err(|e| {
                crate::config::ConfigError::InvalidSocketAddr {
                    key: "OBS_METRICS_ADDR",
                    val: trimmed.to_string(),
                    err: e.to_string(),
                }
            })?;
            self.metrics_addr = addr;
        }

        if let Ok(val) = std::env::var("OBS_LOG_RAW_SDP") {
            self.log_raw_sdp = val.eq_ignore_ascii_case("true") || val == "1";
        }

        Ok(())
    }

    pub fn from_env() -> Self {
        let mut config = Self::default();
        let _ = config.apply_env_overrides();
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::ENV_LOCK;

    #[test]
    fn test_default_observability_config() {
        let _guard = ENV_LOCK.lock().unwrap();
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
        let _guard = ENV_LOCK.lock().unwrap();

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
