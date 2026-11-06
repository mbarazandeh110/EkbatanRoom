use sfu_media_node::proto::sfu::v1 as proto;
use sfu_media_node::proto::sfu::v1::sfu_media_engine_service_server::SfuMediaEngineService;
use sfu_media_node::{Config, GrpcMediaEngineService, MediaNode};
use std::sync::Arc;
use tonic::Request;

#[tokio::test]
async fn test_custom_webrtc_transport_and_codecs_from_config() {
    let mut config = Config::default_values();
    config.worker_count = 1;
    // Tune WebRtcTransport
    config.webrtc_transport.initial_available_outgoing_bitrate = 2_000_000;
    config.webrtc_transport.max_incoming_bitrate = Some(3_000_000);
    config.webrtc_transport.min_outgoing_bitrate = Some(500_000);
    config.webrtc_transport.max_outgoing_bitrate = Some(4_000_000);
    config.webrtc_transport.max_send_message_size = 1_000_000;
    config.webrtc_transport.sctp_send_buffer_size = 2_000_000;

    // Enable VP9
    config.router_codecs.enable_vp9 = true;
    config.router_codecs.google_start_bitrate = 1200;

    // Audio level observer
    config.audio_level_observer.interval_ms = 350;
    config.audio_level_observer.threshold_dbov = -75;

    let node = Arc::new(MediaNode::new(config).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    // 1. Create Router -> Check that VP9 is present in router RTP capabilities
    let r_resp = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r-tuned".to_string(),
            room_id: "room-tuned".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(r_resp.router_rtp_capabilities_json.contains("video/VP9"));

    // 2. Create WebRtcTransport -> Check successful creation with tuned parameters
    let t_resp = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-t-tuned".to_string(),
            router_id: r_resp.router_id.clone(),
            peer_id: "peer-tuned".to_string(),
            is_publisher: true,
            enable_sctp: true,
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(!t_resp.transport_id.is_empty());

    let _ = node.shutdown(std::time::Duration::from_secs(1)).await;
}

#[tokio::test]
async fn test_yaml_config_integration_with_media_node() {
    let yaml = r#"
network:
  listen_ip: "127.0.0.1"
  announced_ip: "127.0.0.1"
  rtc_min_port: 45000
  rtc_max_port: 45100
  grpc_port: 50060

worker:
  worker_count: 2
  capacity_weight: 1500
  spillover_threshold_percent: 70.0
  log_level: "warn"
  log_tags:
    - ice
    - dtls

webrtc_transport:
  initial_available_outgoing_bitrate: 1800000

router_codecs:
  enable_opus: true
  enable_vp8: true
  enable_h264: true
  enable_vp9: false
"#;

    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("integration_test_config.yaml");
    std::fs::write(&file_path, yaml).unwrap();

    let config = Config::try_from_file(&file_path).unwrap();
    assert_eq!(config.rtc_min_port, 45000);
    assert_eq!(config.rtc_max_port, 45100);
    assert_eq!(
        config.webrtc_transport.initial_available_outgoing_bitrate,
        1800000
    );

    let node = Arc::new(MediaNode::new(config).await.unwrap());
    assert_eq!(node.get_node_capacity_info().total_capacity_weight, 3000); // 2 * 1500

    let _ = node.shutdown(std::time::Duration::from_secs(1)).await;
    let _ = std::fs::remove_file(file_path);
}

#[tokio::test]
async fn test_yaml_observability_aliases_and_worker_count_zero() {
    let yaml = r#"
worker:
  worker_count: 0
  capacity_weight: 1200

plain_transport:
  max_send_message_size: 4000000
  max_receive_message_size: 4000000
  sctp_send_buffer_size: 6000000

observability:
  otlp_endpoint: "http://otel.internal:4317"
  trace_sample_rate: 0.25
  metrics_enabled: true
  metrics_addr: "127.0.0.1:9095"
"#;

    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join("alias_test_config.yaml");
    std::fs::write(&file_path, yaml).unwrap();

    let config = Config::try_from_file(&file_path).unwrap();
    assert!(config.worker_count > 0);
    assert_eq!(config.worker_capacity_weight, 1200);

    assert_eq!(config.plain_transport.max_send_message_size, 4000000);
    assert_eq!(config.plain_transport.sctp_send_buffer_size, 6000000);

    assert_eq!(
        config.observability.tracing_endpoint,
        "http://otel.internal:4317"
    );
    assert!((config.observability.tracing_sample_rate - 0.25).abs() < f64::EPSILON);
    assert_eq!(
        config.observability.metrics_addr.to_string(),
        "127.0.0.1:9095"
    );

    let _ = std::fs::remove_file(file_path);
}

#[tokio::test]
async fn test_load_config_example_yaml_file() {
    let yaml_path = std::path::Path::new("config.example.yaml");
    assert!(
        yaml_path.exists(),
        "config.example.yaml must exist in repo root"
    );

    let config = Config::try_from_file(yaml_path).expect("Failed to parse config.example.yaml");
    assert_eq!(config.rtc_port_range(), 40000..=49999);
    assert_eq!(config.rtc_min_port, 40000);
    assert_eq!(config.rtc_max_port, 49999);
    assert_eq!(config.grpc_port, 50051);
    assert_eq!(
        config.webrtc_transport.initial_available_outgoing_bitrate,
        3_000_000
    );
    assert_eq!(
        config.webrtc_transport.max_incoming_bitrate,
        Some(4_000_000)
    );
    assert_eq!(config.audio_level_observer.interval_ms, 500);
    assert_eq!(config.audio_level_observer.threshold_dbov, -80);
    assert!(config.router_codecs.enable_opus);
    assert!(config.router_codecs.enable_vp8);
    assert!(config.router_codecs.enable_h264);
    assert!(!config.router_codecs.enable_vp9);
    assert!(config.observability.metrics_enabled);
    assert_eq!(config.observability.log_level, "info");
    assert_eq!(config.observability.log_format, "json");
}

#[tokio::test]
async fn test_conf_file_with_license_and_missing_file_error() {
    use std::path::Path;

    // 1. Verify modern non-deprecated fields (announced_address, port_range, license)
    let conf = r#"
network:
  listen_ip: "127.0.0.1"
  announced_address: "192.168.1.50"
  port_range: "42000-43000"
  grpc_port: 50058

license:
  token: "EKB1.eyJwYXlsb2FkIjoidGVzdCJ9.c2lnbmF0dXJl"
  public_key_hex: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
"#;
    let temp_dir = std::env::temp_dir();
    let conf_path = temp_dir.join("test_sfu_license.conf");
    std::fs::write(&conf_path, conf).unwrap();

    let cfg = Config::try_from_file(&conf_path).unwrap();
    assert_eq!(cfg.grpc_port, 50058);
    assert_eq!(cfg.announced_ip.as_deref(), Some("192.168.1.50"));
    assert_eq!(cfg.announced_address(), Some("192.168.1.50"));
    assert_eq!(cfg.rtc_port_range(), 42000..=43000);
    assert_eq!(
        cfg.license_token.as_deref(),
        Some("EKB1.eyJwYXlsb2FkIjoidGVzdCJ9.c2lnbmF0dXJl")
    );
    assert_eq!(
        cfg.license_public_key_hex.as_deref(),
        Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
    );
    let _ = std::fs::remove_file(conf_path);

    // 2. Explicit custom file path that does not exist must return ConfigError::FileReadError
    let missing_path = Path::new("non_existent_sfu_config_12345.conf");
    let err = Config::try_load_from_file_and_env(Some(missing_path));
    assert!(matches!(
        err,
        Err(sfu_media_node::config::ConfigError::FileReadError { .. })
    ));
}
