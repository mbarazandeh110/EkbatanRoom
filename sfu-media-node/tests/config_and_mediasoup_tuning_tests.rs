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
