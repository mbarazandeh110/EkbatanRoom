use std::path::PathBuf;

/// Action decided after parsing command line arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAction {
    /// Start the SFU media engine with an optional path to a configuration file.
    Run { config_path: Option<PathBuf> },
    /// Print help information and exit.
    Help,
    /// Print version information and exit.
    Version,
}

/// Command line interface parser for sfu-media-node.
pub struct Cli;

impl Cli {
    /// Parses CLI arguments from an iterator of arguments (including executable name as first arg).
    pub fn parse_from<I, T>(args: I) -> Result<CliAction, String>
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        let mut iter = args.into_iter().map(Into::into).skip(1);
        let mut config_path = None;

        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "-h" | "--help" => return Ok(CliAction::Help),
                "-v" | "--version" | "-V" => return Ok(CliAction::Version),
                "-c" | "--config" => {
                    let next = iter.next().ok_or_else(|| {
                        format!(
                            "Error: Flag '{}' requires a configuration file path argument.\nRun with --help for usage information.",
                            arg
                        )
                    })?;
                    if next.starts_with('-') {
                        return Err(format!(
                            "Error: Expected file path after '{}', found '{}'.\nRun with --help for usage information.",
                            arg, next
                        ));
                    }
                    config_path = Some(PathBuf::from(next));
                }
                arg if arg.starts_with("--config=") => {
                    let path_str = &arg["--config=".len()..];
                    if path_str.trim().is_empty() {
                        return Err(
                            "Error: '--config=' requires a non-empty configuration file path.\nRun with --help for usage information."
                                .to_string(),
                        );
                    }
                    config_path = Some(PathBuf::from(path_str));
                }
                arg if arg.starts_with("-c=") => {
                    let path_str = &arg["-c=".len()..];
                    if path_str.trim().is_empty() {
                        return Err(
                            "Error: '-c=' requires a non-empty configuration file path.\nRun with --help for usage information."
                                .to_string(),
                        );
                    }
                    config_path = Some(PathBuf::from(path_str));
                }
                unknown => {
                    return Err(format!(
                        "Error: Unknown argument '{}'.\nRun with --help for usage information.",
                        unknown
                    ));
                }
            }
        }

        Ok(CliAction::Run { config_path })
    }

    /// Returns the formatted help text.
    pub fn help_text() -> &'static str {
        r#"SFU Media Node (Rust mediasoup Media Plane for EkbatanRoom)

USAGE:
    sfu-media-node [OPTIONS]

OPTIONS:
    -c, --config <FILE>       Path to YAML configuration file (e.g. config.example.yaml, config.yaml)
    -h, --help                Print this help information and exit
    -v, -V, --version         Print version information and exit

CONFIGURATION DISCOVERY ORDER:
    1. CLI argument: Explicit path provided via `-c` or `--config <FILE>`
    2. Environment variable: Path specified in `SFU_CONFIG_FILE`
    3. Auto-discovery in current directory: `config.yaml`, `config.yml`, or `config.conf`
    4. Built-in defaults: Fallback to sensible defaults

CONFIGURATION HIERARCHY & 12-FACTOR OVERRIDES:
    Any environment variable set in the environment overrides the corresponding
    value loaded from the configuration file.

COMMON ENVIRONMENT VARIABLES:
    Network (ListenInfo):
      MEDIASOUP_LISTEN_IP            Bind IP for RTC sockets (default: 0.0.0.0)
      MEDIASOUP_ANNOUNCED_ADDRESS    Announced IP/FQDN for ICE candidates (default: auto)
      RTC_PORT_RANGE                 UDP port range for RTC (default: 40000-49999)
      GRPC_PORT                      gRPC control service port (default: 50051)

    Worker Pool:
      MEDIASOUP_WORKER_COUNT         Number of mediasoup workers (default: CPU cores)
      SFU_WORKER_CAPACITY_WEIGHT     Weight per worker for capacity scoring (default: 1000)
      SFU_WORKER_SPILLOVER_THRESHOLD_PERCENT
                                     Threshold % before spillover to other workers (default: 80.0)
      MEDIASOUP_WORKER_LOG_LEVEL     C++ worker log level: debug, warn, error, none (default: warn)
      MEDIASOUP_WORKER_LOG_TAGS      Worker log tags comma-separated (e.g. ice,dtls,rtp,bwe)
      MEDIASOUP_DTLS_CERT            Path to custom DTLS certificate file
      MEDIASOUP_DTLS_KEY             Path to custom DTLS private key file

    WebRTC Transport Tuning:
      MEDIASOUP_INITIAL_AVAILABLE_OUTGOING_BITRATE
                                     Initial outgoing bandwidth in bps (default: 1000000)
      MEDIASOUP_MIN_OUTGOING_BITRATE Minimum outgoing bitrate floor in bps (default: 600000)
      MEDIASOUP_MAX_OUTGOING_BITRATE Maximum outgoing bitrate cap in bps (default: unlimited)

    Observability & Metrics:
      OBS_LOG_LEVEL                  Log verbosity: trace, debug, info, warn, error (default: info)
      OBS_LOG_FORMAT                 Log format: json, pretty, text (default: json)
      OBS_TRACING_ENABLED            Enable OpenTelemetry distributed tracing (default: false)
      OBS_OTLP_ENDPOINT              OpenTelemetry collector gRPC endpoint (default: http://localhost:4317)
      OBS_TRACE_SAMPLE_RATE          Tracing sample rate: 0.0 to 1.0 (default: 0.1)
      OBS_METRICS_ENABLED            Enable Prometheus /metrics endpoint (default: true)
      OBS_METRICS_ADDR               Prometheus HTTP server listen address (default: 0.0.0.0:9090)
      OBS_LOG_RAW_SDP                Log raw SDP candidate lines at debug level (default: false)

    Enterprise Licensing:
      EKBATAN_LICENSE_KEY            Ed25519 license token string (EKB1.<payload>.<sig>)
      EKBATAN_LICENSE_PUBLIC_KEY     32-byte Ed25519 public key in hexadecimal format

EXAMPLES:
    # Run with default auto-discovery:
    sfu-media-node

    # Run with an explicit configuration file:
    sfu-media-node -c config.example.yaml
    sfu-media-node --config=/etc/sfu/production.yaml

    # Run with config file and override gRPC port via environment variable:
    GRPC_PORT=50060 sfu-media-node -c config.yaml
"#
    }

    /// Returns the version string.
    pub fn version_text() -> String {
        format!("sfu-media-node {}", env!("CARGO_PKG_VERSION"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_help_flag() {
        assert_eq!(
            Cli::parse_from(["sfu-media-node", "-h"]),
            Ok(CliAction::Help)
        );
        assert_eq!(
            Cli::parse_from(["sfu-media-node", "--help"]),
            Ok(CliAction::Help)
        );
    }

    #[test]
    fn test_cli_version_flag() {
        assert_eq!(
            Cli::parse_from(["sfu-media-node", "-v"]),
            Ok(CliAction::Version)
        );
        assert_eq!(
            Cli::parse_from(["sfu-media-node", "-V"]),
            Ok(CliAction::Version)
        );
        assert_eq!(
            Cli::parse_from(["sfu-media-node", "--version"]),
            Ok(CliAction::Version)
        );
        assert!(Cli::version_text().contains("sfu-media-node"));
    }

    #[test]
    fn test_cli_default_run() {
        assert_eq!(
            Cli::parse_from(["sfu-media-node"]),
            Ok(CliAction::Run { config_path: None })
        );
    }

    #[test]
    fn test_cli_config_flag_space() {
        assert_eq!(
            Cli::parse_from(["sfu-media-node", "-c", "my_config.conf"]),
            Ok(CliAction::Run {
                config_path: Some(PathBuf::from("my_config.conf"))
            })
        );
        assert_eq!(
            Cli::parse_from(["sfu-media-node", "--config", "/etc/sfu/config.yaml"]),
            Ok(CliAction::Run {
                config_path: Some(PathBuf::from("/etc/sfu/config.yaml"))
            })
        );
    }

    #[test]
    fn test_cli_config_flag_equals() {
        assert_eq!(
            Cli::parse_from(["sfu-media-node", "-c=my_config.conf"]),
            Ok(CliAction::Run {
                config_path: Some(PathBuf::from("my_config.conf"))
            })
        );
        assert_eq!(
            Cli::parse_from(["sfu-media-node", "--config=/etc/sfu/config.conf"]),
            Ok(CliAction::Run {
                config_path: Some(PathBuf::from("/etc/sfu/config.conf"))
            })
        );
    }

    #[test]
    fn test_cli_missing_config_argument() {
        assert!(Cli::parse_from(["sfu-media-node", "-c"]).is_err());
        assert!(Cli::parse_from(["sfu-media-node", "--config"]).is_err());
        assert!(Cli::parse_from(["sfu-media-node", "-c="]).is_err());
        assert!(Cli::parse_from(["sfu-media-node", "--config="]).is_err());
        assert!(Cli::parse_from(["sfu-media-node", "-c", "--another-flag"]).is_err());
    }

    #[test]
    fn test_cli_unknown_argument() {
        assert!(Cli::parse_from(["sfu-media-node", "--unknown-flag"]).is_err());
        assert!(Cli::parse_from(["sfu-media-node", "random_positional"]).is_err());
    }
}
