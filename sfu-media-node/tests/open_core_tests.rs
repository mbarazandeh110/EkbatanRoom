use sfu_media_node::proto::sfu::v1 as proto;
use sfu_media_node::proto::sfu::v1::sfu_media_engine_service_server::SfuMediaEngineService;
use sfu_media_node::{Config, GrpcMediaEngineService, MediaNode};
use std::sync::Arc;
use tonic::Request;

fn test_config() -> Config {
    let mut config = Config::from_env();
    config.worker_count = 2;
    config.worker_capacity_weight = 10;
    config.worker_spillover_threshold_percent = 50.0;
    config
}

pub fn create_test_enterprise_license() -> String {
    let priv_hex = "02dc0bfacd35d5f44baada453877d98d2dfadcfa175d67f211195a6eba102eb4";
    let priv_bytes = hex::decode(priv_hex).unwrap();
    let mut priv_arr = [0u8; 32];
    priv_arr.copy_from_slice(&priv_bytes);

    let claims = sfu_media_node::domain::license::LicenseClaims {
        license_id: "test-ent-lic".to_string(),
        customer_id: "ekbatan-test".to_string(),
        tier: "enterprise".to_string(),
        features: vec![sfu_media_node::domain::license::FEATURE_INTER_NODE_CASCADING.to_string()],
        cluster_limits: sfu_media_node::domain::license::ClusterLimits::default(),
        allowed_domains: vec![],
        issued_at: 1000,
        expires_at: 0, // perpetual
        grace_period_days: 0,
    };

    sfu_media_node::domain::license::generate_license_token(&claims, &priv_arr).unwrap()
}

#[tokio::test]
async fn test_open_core_node_capacity_and_feature_matrix() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    // 1. In default boot without license token: cascading is disabled and tier is community
    let cap = service
        .get_node_capacity(Request::new(proto::GetNodeCapacityRequest {}))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(cap.total_workers, 2);
    assert_eq!(cap.alive_workers, 2);
    assert_eq!(cap.total_capacity_weight, 20);
    assert!(!cap.enterprise_cascading_enabled);
    assert_eq!(cap.license_tier, "community");

    // 2. Apply valid Enterprise license
    let token = create_test_enterprise_license();
    let apply_resp = service
        .apply_license(Request::new(proto::ApplyLicenseRequest {
            license_token: token,
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(apply_resp.is_valid);
    assert_eq!(apply_resp.license_tier, "enterprise");

    let cap_after = service
        .get_node_capacity(Request::new(proto::GetNodeCapacityRequest {}))
        .await
        .unwrap()
        .into_inner();

    let is_enterprise = cfg!(feature = "enterprise");
    assert_eq!(cap_after.enterprise_cascading_enabled, is_enterprise);
    assert_eq!(cap_after.license_tier, "enterprise");

    let _ = node.shutdown(std::time::Duration::from_secs(1)).await;
}

#[cfg(not(feature = "enterprise"))]
#[tokio::test]
async fn test_community_edition_pipe_rpcs_return_unimplemented() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    let r_resp = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r-comm".to_string(),
            room_id: "room-comm".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    // 1. CreatePipeTransport -> Unimplemented
    let err1 = service
        .create_pipe_transport(Request::new(proto::CreatePipeTransportRequest {
            request_id: "req-pt-comm-1".to_string(),
            router_id: r_resp.router_id.clone(),
            enable_srtp: false,
            enable_rtx: false,
        }))
        .await
        .unwrap_err();
    assert_eq!(err1.code(), tonic::Code::Unimplemented);
    assert!(err1.message().contains("Enterprise feature"));

    // 2. ConnectPipeTransport -> Unimplemented
    let err2 = service
        .connect_pipe_transport(Request::new(proto::ConnectPipeTransportRequest {
            router_id: r_resp.router_id.clone(),
            pipe_transport_id: "pipe-any".to_string(),
            remote_ip: "127.0.0.1".to_string(),
            remote_port: 40002,
            remote_srtp_parameters_json: String::new(),
        }))
        .await
        .unwrap_err();
    assert_eq!(err2.code(), tonic::Code::Unimplemented);
    assert!(err2.message().contains("Enterprise feature"));

    // 3. PipeProduce -> Unimplemented
    let opus_params = r#"{
        "mid": "0",
        "codecs": [{
            "mimeType": "audio/opus",
            "payloadType": 111,
            "clockRate": 48000,
            "channels": 2,
            "parameters": {},
            "rtcpFeedback": []
        }],
        "headerExtensions": [],
        "encodings": [{ "ssrc": 11111 }],
        "rtcp": { "cname": "test-audio", "reducedSize": true }
    }"#;

    let err3 = service
        .pipe_produce(Request::new(proto::PipeProduceRequest {
            request_id: "req-pp-comm".to_string(),
            router_id: r_resp.router_id.clone(),
            pipe_transport_id: "pipe-any".to_string(),
            source_producer_id: "prod-any".to_string(),
            kind: "audio".to_string(),
            rtp_parameters_json: opus_params.to_string(),
            paused: false,
        }))
        .await
        .unwrap_err();
    assert_eq!(err3.code(), tonic::Code::Unimplemented);
    assert!(err3.message().contains("Enterprise feature"));

    // 4. PipeConsume -> Unimplemented
    let err4 = service
        .pipe_consume(Request::new(proto::PipeConsumeRequest {
            request_id: "req-pc-comm".to_string(),
            router_id: r_resp.router_id.clone(),
            pipe_transport_id: "pipe-any".to_string(),
            producer_id: "prod-any".to_string(),
        }))
        .await
        .unwrap_err();
    assert_eq!(err4.code(), tonic::Code::Unimplemented);
    assert!(err4.message().contains("Enterprise feature"));

    let _ = node.shutdown(std::time::Duration::from_secs(1)).await;
}

#[cfg(feature = "enterprise")]
#[tokio::test]
async fn test_enterprise_edition_pipe_cascade_e2e() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    service
        .apply_license(Request::new(proto::ApplyLicenseRequest {
            license_token: create_test_enterprise_license(),
        }))
        .await
        .unwrap();

    // 1. Create two routers representing two cascaded instances
    let r1 = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r1-cascade".to_string(),
            room_id: "room-cascade-1".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let r2 = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r2-cascade".to_string(),
            room_id: "room-cascade-2".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    // 2. Create publisher transport on Router 1
    let t1_pub = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-t1-pub".to_string(),
            router_id: r1.router_id.clone(),
            peer_id: "publisher-node-1".to_string(),
            is_publisher: true,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    // Produce audio on Router 1
    let opus_params = r#"{
        "mid": "0",
        "codecs": [{
            "mimeType": "audio/opus",
            "payloadType": 111,
            "clockRate": 48000,
            "channels": 2,
            "parameters": {},
            "rtcpFeedback": []
        }],
        "headerExtensions": [],
        "encodings": [{ "ssrc": 11111 }],
        "rtcp": { "cname": "test-audio", "reducedSize": true }
    }"#;

    let p1_prod = service
        .produce(Request::new(proto::ProduceRequest {
            request_id: "req-p1-prod".to_string(),
            router_id: r1.router_id.clone(),
            transport_id: t1_pub.transport_id.clone(),
            peer_id: "publisher-node-1".to_string(),
            kind: "audio".to_string(),
            rtp_parameters_json: opus_params.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    // 3. Create PipeTransport on Router 1
    let pt1 = service
        .create_pipe_transport(Request::new(proto::CreatePipeTransportRequest {
            request_id: "req-pt1".to_string(),
            router_id: r1.router_id.clone(),
            enable_srtp: true,
            enable_rtx: true,
        }))
        .await
        .unwrap()
        .into_inner();

    // 4. Create PipeTransport on Router 2
    let pt2 = service
        .create_pipe_transport(Request::new(proto::CreatePipeTransportRequest {
            request_id: "req-pt2".to_string(),
            router_id: r2.router_id.clone(),
            enable_srtp: true,
            enable_rtx: true,
        }))
        .await
        .unwrap()
        .into_inner();

    // 5. Connect PipeTransport 1 to 2, and 2 to 1
    service
        .connect_pipe_transport(Request::new(proto::ConnectPipeTransportRequest {
            router_id: r1.router_id.clone(),
            pipe_transport_id: pt1.pipe_transport_id.clone(),
            remote_ip: pt2.ip.clone(),
            remote_port: pt2.port,
            remote_srtp_parameters_json: pt2.srtp_parameters_json.clone(),
        }))
        .await
        .unwrap();

    service
        .connect_pipe_transport(Request::new(proto::ConnectPipeTransportRequest {
            router_id: r2.router_id.clone(),
            pipe_transport_id: pt2.pipe_transport_id.clone(),
            remote_ip: pt1.ip.clone(),
            remote_port: pt1.port,
            remote_srtp_parameters_json: pt1.srtp_parameters_json.clone(),
        }))
        .await
        .unwrap();

    // 6. PipeConsume from Router 1's PipeTransport (consumes local producer to send over pipe)
    let pipe_consume_resp = service
        .pipe_consume(Request::new(proto::PipeConsumeRequest {
            request_id: "req-pipe-cons-1".to_string(),
            router_id: r1.router_id.clone(),
            pipe_transport_id: pt1.pipe_transport_id.clone(),
            producer_id: p1_prod.producer_id.clone(),
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(!pipe_consume_resp.consumer_id.is_empty());
    assert_eq!(pipe_consume_resp.kind, "audio");

    // 7. PipeProduce on Router 2's PipeTransport (injects piped stream into router 2)
    let pipe_prod_resp = service
        .pipe_produce(Request::new(proto::PipeProduceRequest {
            request_id: "req-pipe-prod-2".to_string(),
            router_id: r2.router_id.clone(),
            pipe_transport_id: pt2.pipe_transport_id.clone(),
            source_producer_id: p1_prod.producer_id.clone(),
            kind: "audio".to_string(),
            rtp_parameters_json: pipe_consume_resp.rtp_parameters_json.clone(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(pipe_prod_resp.producer_id, p1_prod.producer_id);
    assert_eq!(pipe_prod_resp.kind, "audio");

    // 8. Now a consumer on Router 2 can consume this cascaded producer!
    let t2_sub = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-t2-sub".to_string(),
            router_id: r2.router_id.clone(),
            peer_id: "subscriber-node-2".to_string(),
            is_publisher: false,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    let opus_caps = r#"{
        "codecs": [
            {
                "mimeType": "audio/opus",
                "kind": "audio",
                "preferredPayloadType": 111,
                "clockRate": 48000,
                "channels": 2,
                "parameters": {},
                "rtcpFeedback": []
            }
        ],
        "headerExtensions": []
    }"#;

    let sub_consume_resp = service
        .consume(Request::new(proto::ConsumeRequest {
            request_id: "req-sub2-consume".to_string(),
            router_id: r2.router_id.clone(),
            subscriber_transport_id: t2_sub.transport_id.clone(),
            producer_id: pipe_prod_resp.producer_id.clone(),
            rtp_capabilities_json: opus_caps.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(!sub_consume_resp.consumer_id.is_empty());
    assert_eq!(sub_consume_resp.kind, "audio");

    // Verify cascaded producer is properly registered in producer_consumers mapping
    assert!(node
        .state
        .producer_consumers
        .contains_key(&pipe_prod_resp.producer_id));
    let cascaded_consumers = node
        .state
        .producer_consumers
        .get(&pipe_prod_resp.producer_id)
        .unwrap();
    assert!(cascaded_consumers.contains(&sub_consume_resp.consumer_id));
    drop(cascaded_consumers);

    // 9. Close pt2 PipeTransport and verify cascading cleanup removes producer and consumer without leaks
    service
        .close_resource(Request::new(proto::CloseResourceRequest {
            router_id: r2.router_id.clone(),
            resource_type: proto::ResourceType::Transport as i32,
            resource_id: pt2.pipe_transport_id.clone(),
        }))
        .await
        .unwrap();

    assert!(!node
        .state
        .pipe_transports
        .contains_key(&pt2.pipe_transport_id));
    assert!(node
        .state
        .get_producer(&r2.router_id, &pipe_prod_resp.producer_id)
        .is_none());
    assert!(node
        .state
        .get_producer(&r1.router_id, &p1_prod.producer_id)
        .is_some());
    assert!(!node
        .state
        .consumers
        .contains_key(&sub_consume_resp.consumer_id));

    let _ = node.shutdown(std::time::Duration::from_secs(1)).await;
}

#[cfg(feature = "enterprise")]
#[tokio::test]
async fn test_enterprise_pipe_produce_idempotency_conflict_on_kind_or_params() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    service
        .apply_license(Request::new(proto::ApplyLicenseRequest {
            license_token: create_test_enterprise_license(),
        }))
        .await
        .unwrap();

    let r = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r-idemp-pipe".to_string(),
            room_id: "room-idemp-pipe".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let pt = service
        .create_pipe_transport(Request::new(proto::CreatePipeTransportRequest {
            request_id: "req-pt-idemp-pipe".to_string(),
            router_id: r.router_id.clone(),
            enable_srtp: false,
            enable_rtx: false,
        }))
        .await
        .unwrap()
        .into_inner();

    let opus_params = r#"{
        "mid": "0",
        "codecs": [{
            "mimeType": "audio/opus",
            "payloadType": 111,
            "clockRate": 48000,
            "channels": 2,
            "parameters": {},
            "rtcpFeedback": []
        }],
        "headerExtensions": [],
        "encodings": [{ "ssrc": 77777 }],
        "rtcp": { "cname": "audio-test", "reducedSize": true }
    }"#;

    // First produce succeeds
    let _prod1 = service
        .pipe_produce(Request::new(proto::PipeProduceRequest {
            request_id: "req-pipe-produce-same-id".to_string(),
            router_id: r.router_id.clone(),
            pipe_transport_id: pt.pipe_transport_id.clone(),
            source_producer_id: "77777777-7777-7777-7777-777777777777".to_string(),
            kind: "audio".to_string(),
            rtp_parameters_json: opus_params.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    // Second produce with identical payload returns cached result (Idempotent hit)
    let prod_idemp = service
        .pipe_produce(Request::new(proto::PipeProduceRequest {
            request_id: "req-pipe-produce-same-id".to_string(),
            router_id: r.router_id.clone(),
            pipe_transport_id: pt.pipe_transport_id.clone(),
            source_producer_id: "77777777-7777-7777-7777-777777777777".to_string(),
            kind: "audio".to_string(),
            rtp_parameters_json: opus_params.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        prod_idemp.producer_id,
        "77777777-7777-7777-7777-777777777777"
    );

    // Third produce with same request_id but different kind -> AlreadyExists (IdempotencyConflict)
    let err_conflict_kind = service
        .pipe_produce(Request::new(proto::PipeProduceRequest {
            request_id: "req-pipe-produce-same-id".to_string(),
            router_id: r.router_id.clone(),
            pipe_transport_id: pt.pipe_transport_id.clone(),
            source_producer_id: "77777777-7777-7777-7777-777777777777".to_string(),
            kind: "video".to_string(),
            rtp_parameters_json: opus_params.to_string(),
            paused: false,
        }))
        .await
        .unwrap_err();
    assert_eq!(err_conflict_kind.code(), tonic::Code::AlreadyExists);

    let _ = node.shutdown(std::time::Duration::from_secs(1)).await;
}

#[cfg(feature = "enterprise")]
#[tokio::test]
async fn test_enterprise_pipe_consume_sub_router_spillover() {
    let mut config = test_config();
    config.worker_count = 2;
    config.worker_capacity_weight = 10;
    config.worker_spillover_threshold_percent = 50.0;

    let node = Arc::new(MediaNode::new(config).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    service
        .apply_license(Request::new(proto::ApplyLicenseRequest {
            license_token: create_test_enterprise_license(),
        }))
        .await
        .unwrap();

    let r = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r-spill".to_string(),
            room_id: "room-spill".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    // 1. Peer 1 on Worker 0 publishes video (adds 10 points load -> Worker 0 reaches 100% capacity)
    let p1_pub = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-p1-spill-pub".to_string(),
            router_id: r.router_id.clone(),
            peer_id: "peer-1".to_string(),
            is_publisher: true,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    let vp8_rtp_params = r#"{
        "mid": "0",
        "codecs": [
            {
                "mimeType": "video/VP8",
                "payloadType": 96,
                "clockRate": 90000,
                "parameters": {},
                "rtcpFeedback": []
            }
        ],
        "headerExtensions": [],
        "encodings": [{ "ssrc": 99999999 }],
        "rtcp": { "cname": "video-spill", "reducedSize": true }
    }"#;

    let _p1_prod = service
        .produce(Request::new(proto::ProduceRequest {
            request_id: "req-p1-prod-fill".to_string(),
            router_id: r.router_id.clone(),
            transport_id: p1_pub.transport_id.clone(),
            peer_id: "peer-1".to_string(),
            kind: "video".to_string(),
            rtp_parameters_json: vp8_rtp_params.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    // 2. Peer 2 arrives -> Spills over to Worker 1 and creates publisher transport and Sub-Router on Worker 1!
    let p2_pub = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-p2-spill-pub".to_string(),
            router_id: r.router_id.clone(),
            peer_id: "peer-2".to_string(),
            is_publisher: true,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    let p2_transport_item = node.state.transports.get(&p2_pub.transport_id).unwrap();
    assert_eq!(p2_transport_item.worker_index, 1);
    assert_ne!(p2_transport_item.physical_router_id, r.router_id);
    drop(p2_transport_item);

    let opus_rtp_params = r#"{
        "mid": "0",
        "codecs": [
            {
                "mimeType": "audio/opus",
                "payloadType": 111,
                "clockRate": 48000,
                "channels": 2,
                "parameters": {},
                "rtcpFeedback": []
            }
        ],
        "headerExtensions": [],
        "encodings": [{ "ssrc": 88888888 }],
        "rtcp": { "cname": "audio-spill-sub", "reducedSize": true }
    }"#;

    let p2_prod = service
        .produce(Request::new(proto::ProduceRequest {
            request_id: "req-p2-spill-prod".to_string(),
            router_id: r.router_id.clone(),
            transport_id: p2_pub.transport_id.clone(),
            peer_id: "peer-2".to_string(),
            kind: "audio".to_string(),
            rtp_parameters_json: opus_rtp_params.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    let p2_prod_item = node
        .state
        .get_producer(&r.router_id, &p2_prod.producer_id)
        .unwrap();
    assert_eq!(p2_prod_item.worker_index, 1);
    assert_ne!(p2_prod_item.physical_router_id, r.router_id);
    drop(p2_prod_item);

    // 3. Create PipeTransport on the room's main router (Worker 0)
    let pt = service
        .create_pipe_transport(Request::new(proto::CreatePipeTransportRequest {
            request_id: "req-pt-main-router".to_string(),
            router_id: r.router_id.clone(),
            enable_srtp: false,
            enable_rtx: false,
        }))
        .await
        .unwrap()
        .into_inner();

    // 4. PipeConsume Peer 2's producer (which lives on Worker 1's Sub-Router)!
    // EnterprisePipeService must transparently pipe the stream from Worker 1's Sub-Router
    // to Worker 0's main router before calling pipe_transport.consume.
    let pipe_consume_resp = service
        .pipe_consume(Request::new(proto::PipeConsumeRequest {
            request_id: "req-pc-sub-router".to_string(),
            router_id: r.router_id.clone(),
            pipe_transport_id: pt.pipe_transport_id.clone(),
            producer_id: p2_prod.producer_id.clone(),
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(!pipe_consume_resp.consumer_id.is_empty());
    assert_eq!(pipe_consume_resp.producer_id, p2_prod.producer_id);
    assert_eq!(pipe_consume_resp.kind, "audio");

    // Verify transparent piped producer entry was created for main router
    let piped = node
        .state
        .piped_producers
        .get(&p2_prod.producer_id)
        .unwrap();
    assert!(piped.iter().any(|e| e.target_router_id == r.router_id));
    drop(piped);

    let _ = node.shutdown(std::time::Duration::from_secs(1)).await;
}

#[cfg(feature = "enterprise")]
#[tokio::test]
async fn test_enterprise_license_gate_enforcement() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    let r = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r-gate".to_string(),
            room_id: "room-gate".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    // 1. Without license: Pipe operations must return PermissionDenied
    let err_unlicensed = service
        .create_pipe_transport(Request::new(proto::CreatePipeTransportRequest {
            request_id: "req-pt-unlicensed".to_string(),
            router_id: r.router_id.clone(),
            enable_srtp: false,
            enable_rtx: false,
        }))
        .await
        .unwrap_err();

    assert_eq!(err_unlicensed.code(), tonic::Code::PermissionDenied);
    assert!(err_unlicensed
        .message()
        .contains("Valid Enterprise license is required"));

    // 2. Apply invalid license token
    let invalid_resp = service
        .apply_license(Request::new(proto::ApplyLicenseRequest {
            license_token: "EKB1.badpayload.badsig".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(!invalid_resp.is_valid);
    assert!(!invalid_resp.error_message.is_empty());

    // 3. Apply valid Enterprise license token
    let valid_token = create_test_enterprise_license();
    let valid_resp = service
        .apply_license(Request::new(proto::ApplyLicenseRequest {
            license_token: valid_token,
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(valid_resp.is_valid);
    assert_eq!(valid_resp.license_tier, "enterprise");
    assert!(valid_resp
        .features
        .contains(&sfu_media_node::domain::license::FEATURE_INTER_NODE_CASCADING.to_string()));

    // 4. Now pipe operation must succeed!
    let pt_resp = service
        .create_pipe_transport(Request::new(proto::CreatePipeTransportRequest {
            request_id: "req-pt-licensed".to_string(),
            router_id: r.router_id.clone(),
            enable_srtp: false,
            enable_rtx: false,
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(!pt_resp.pipe_transport_id.is_empty());
    assert!(!pt_resp.ip.is_empty());

    let _ = node.shutdown(std::time::Duration::from_secs(1)).await;
}
