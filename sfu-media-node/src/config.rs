use std::net::IpAddr;
use std::str::FromStr;
use thiserror::Error;

use crate::observability::ObservabilityConfig;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error(
        "Invalid listen IP in MEDIASOUP_LISTEN_IP '{0}': must be a valid IPv4 or IPv6 address"
    )]
    InvalidListenIp(String),

    #[error(
        "Invalid RTC port range: RTC_MIN_PORT ({min}) cannot be greater than RTC_MAX_PORT ({max})"
    )]
    InvalidPortRange { min: u16, max: u16 },

    #[error("Invalid worker count: MEDIASOUP_WORKER_COUNT must be greater than 0")]
    InvalidWorkerCount,

    #[error("Failed to parse integer environment variable '{key}': {err}")]
    ParseIntError { key: &'static str, err: String },
}

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_ip: IpAddr,
    pub announced_ip: Option<String>,
    pub rtc_min_port: u16,
    pub rtc_max_port: u16,
    pub grpc_port: u16,
    pub worker_count: usize,
    pub worker_capacity_weight: u32,
    pub worker_spillover_threshold_percent: f32,
    pub observability: ObservabilityConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self::from_env()
    }
}

impl Config {
    pub fn try_from_env() -> Result<Self, ConfigError> {
        let listen_ip = match std::env::var("MEDIASOUP_LISTEN_IP") {
            Ok(val) => {
                let trimmed = val.trim();
                if trimmed.is_empty() {
                    IpAddr::from([0, 0, 0, 0])
                } else {
                    IpAddr::from_str(trimmed)
                        .map_err(|_| ConfigError::InvalidListenIp(trimmed.to_string()))?
                }
            }
            Err(_) => IpAddr::from([0, 0, 0, 0]),
        };

        let announced_ip = std::env::var("MEDIASOUP_ANNOUNCED_IP")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                if listen_ip.is_unspecified() {
                    Some("127.0.0.1".to_string())
                } else {
                    Some(listen_ip.to_string())
                }
            });

        let rtc_min_port = match std::env::var("RTC_MIN_PORT") {
            Ok(s) => s
                .trim()
                .parse::<u16>()
                .map_err(|e| ConfigError::ParseIntError {
                    key: "RTC_MIN_PORT",
                    err: e.to_string(),
                })?,
            Err(_) => 40000,
        };

        let rtc_max_port = match std::env::var("RTC_MAX_PORT") {
            Ok(s) => s
                .trim()
                .parse::<u16>()
                .map_err(|e| ConfigError::ParseIntError {
                    key: "RTC_MAX_PORT",
                    err: e.to_string(),
                })?,
            Err(_) => 49999,
        };

        if rtc_min_port > rtc_max_port {
            return Err(ConfigError::InvalidPortRange {
                min: rtc_min_port,
                max: rtc_max_port,
            });
        }

        let grpc_port = match std::env::var("GRPC_PORT") {
            Ok(s) => s
                .trim()
                .parse::<u16>()
                .map_err(|e| ConfigError::ParseIntError {
                    key: "GRPC_PORT",
                    err: e.to_string(),
                })?,
            Err(_) => 50051,
        };

        let worker_count = match std::env::var("MEDIASOUP_WORKER_COUNT") {
            Ok(s) => {
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
                count
            }
            Err(_) => std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1),
        };

        let worker_capacity_weight = match std::env::var("SFU_WORKER_CAPACITY_WEIGHT") {
            Ok(s) => s.trim().parse::<u32>().unwrap_or(1000).max(1),
            Err(_) => 1000,
        };

        let worker_spillover_threshold_percent =
            match std::env::var("SFU_WORKER_SPILLOVER_THRESHOLD_PERCENT") {
                Ok(s) => s.trim().parse::<f32>().unwrap_or(80.0).clamp(1.0, 100.0),
                Err(_) => 80.0,
            };

        let observability = ObservabilityConfig::from_env();

        Ok(Self {
            listen_ip,
            announced_ip,
            rtc_min_port,
            rtc_max_port,
            grpc_port,
            worker_count,
            worker_capacity_weight,
            worker_spillover_threshold_percent,
            observability,
        })
    }

    pub fn from_env() -> Self {
        Self::try_from_env().unwrap_or_else(|e| {
            eprintln!("Configuration warning: {e}. Falling back to default configuration.");
            Self {
                listen_ip: std::net::IpAddr::from([0, 0, 0, 0]),
                announced_ip: Some("127.0.0.1".to_string()),
                rtc_min_port: 40000,
                rtc_max_port: 49999,
                grpc_port: 50051,
                worker_count: std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1),
                worker_capacity_weight: 1000,
                worker_spillover_threshold_percent: 80.0,
                observability: ObservabilityConfig::default(),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_default_config() {
        let _guard = ENV_LOCK.lock().unwrap();
        std::env::remove_var("MEDIASOUP_LISTEN_IP");
        std::env::remove_var("RTC_MIN_PORT");
        std::env::remove_var("RTC_MAX_PORT");
        std::env::remove_var("MEDIASOUP_WORKER_COUNT");
        let config = Config::from_env();
        assert_eq!(config.rtc_min_port, 40000);
        assert_eq!(config.rtc_max_port, 49999);
        assert!(config.worker_count >= 1);
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
}
