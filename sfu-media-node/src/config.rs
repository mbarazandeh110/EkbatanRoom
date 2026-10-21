use std::net::IpAddr;
use std::str::FromStr;

use crate::observability::ObservabilityConfig;

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_ip: IpAddr,
    pub announced_ip: Option<String>,
    pub rtc_min_port: u16,
    pub rtc_max_port: u16,
    pub grpc_port: u16,
    pub worker_count: usize,
    pub observability: ObservabilityConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self::from_env()
    }
}

impl Config {
    pub fn from_env() -> Self {
        let listen_ip = std::env::var("MEDIASOUP_LISTEN_IP")
            .ok()
            .and_then(|s| IpAddr::from_str(&s).ok())
            .unwrap_or_else(|| IpAddr::from([0, 0, 0, 0]));

        let announced_ip = std::env::var("MEDIASOUP_ANNOUNCED_IP")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                if listen_ip.is_unspecified() {
                    Some("127.0.0.1".to_string())
                } else {
                    Some(listen_ip.to_string())
                }
            });

        let rtc_min_port = std::env::var("RTC_MIN_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(40000);

        let rtc_max_port = std::env::var("RTC_MAX_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(49999);

        let grpc_port = std::env::var("GRPC_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(50051);

        let worker_count = std::env::var("MEDIASOUP_WORKER_COUNT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1)
            });

        let observability = ObservabilityConfig::from_env();

        Self {
            listen_ip,
            announced_ip,
            rtc_min_port,
            rtc_max_port,
            grpc_port,
            worker_count,
            observability,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::from_env();
        assert_eq!(config.rtc_min_port, 40000);
        assert_eq!(config.rtc_max_port, 49999);
        assert!(config.worker_count >= 1);
    }
}
