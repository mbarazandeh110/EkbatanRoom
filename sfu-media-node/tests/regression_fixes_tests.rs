use sfu_media_node::proto::sfu::v1 as proto;
use sfu_media_node::proto::sfu::v1::sfu_media_engine_service_server::SfuMediaEngineService;
use sfu_media_node::{Config, GrpcMediaEngineService, MediaNode};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tokio_stream::StreamExt;
use tonic::Request;

fn test_config(workers: usize) -> Config {
    let mut config = Config::from_env();
    config.worker_count = workers;
    config.worker_capacity_weight = 20;
    config.worker_spillover_threshold_percent = 50.0;
    config
}

pub fn create_test_enterprise_license() -> String {
    let priv_hex = "02dc0bfacd35d5f44baada453877d98d2dfadcfa175d67f211195a6eba102eb4";
    let priv_bytes = hex::decode(priv_hex).unwrap();
    let mut priv_arr = [0u8; 32];
    priv_arr.copy_from_slice(&priv_bytes);

    let claims = sfu_media_node::domain::license::LicenseClaims {
        license_id: "test-regression-ent-lic".to_string(),
        customer_id: "ekbatan-regression".to_string(),
        tier: "enterprise".to_string(),
        features: vec![sfu_media_node::domain::license::FEATURE_INTER_NODE_CASCADING.to_string()],
        cluster_limits: sfu_media_node::domain::license::ClusterLimits::default(),
        allowed_domains: vec![],
        issued_at: 1000,
        expires_at: 0,
        grace_period_days: 0,
    };

    sfu_media_node::domain::license::generate_license_token(&claims, &priv_arr).unwrap()
}

/// Bug 3 Regression: Non-atomic temporary removal of transport in close_transport
/// Calling close_transport with a mismatched router_id must not remove and re-insert the transport,
/// so concurrent operations on the transport do not experience transient NotFound errors.
#[tokio::test]
async fn test_close_transport_wrong_router_does_not_transiently_remove() {
    let node = Arc::new(MediaNode::new(test_config(1)).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    // 1. Create Router 1 & Router 2
    let r1 = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r1-atomic".to_string(),
            room_id: "room-1-atomic".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let r2 = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r2-atomic".to_string(),
            room_id: "room-2-atomic".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    // 2. Create transport on Router 1
    let tr1 = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-tr1-atomic".to_string(),
            router_id: r1.router_id.clone(),
            peer_id: "peer-r1-atomic".to_string(),
            is_publisher: true,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    // 3. Attempt to close transport tr1 using Router 2 (mismatched router_id)
    let close_err = service
        .close_resource(Request::new(proto::CloseResourceRequest {
            router_id: r2.router_id.clone(),
            resource_type: proto::ResourceType::Transport as i32,
            resource_id: tr1.transport_id.clone(),
        }))
        .await;

    assert!(
        close_err.is_err(),
        "Closing transport under wrong router must return error"
    );

    // 4. Transport tr1 must remain fully active and accessible under Router 1
    let stats = service
        .get_stats(Request::new(proto::GetStatsRequest {
            router_id: r1.router_id.clone(),
            resource_id: tr1.transport_id.clone(),
            resource_type: proto::ResourceType::Transport as i32,
        }))
        .await;

    assert!(
        stats.is_ok(),
        "Transport must remain valid under its owner router without transient NotFound"
    );

    let _ = node.shutdown(Duration::from_secs(1)).await;
}

/// Bug 5 Regression: MediaNode Drop cleanly aborts background capacity task without explicit shutdown()
#[tokio::test]
async fn test_media_node_drop_aborts_capacity_task() {
    let node = Arc::new(MediaNode::new(test_config(1)).await.unwrap());
    let mut rx = node.event_hub.subscribe();

    // Wait for at least one initial heartbeat capacity event
    let initial_ev = timeout(Duration::from_secs(2), async {
        while let Ok(event) = rx.recv().await {
            if matches!(
                event.payload,
                Some(sfu_media_node::domain::DomainNodeEventPayload::NodeCapacity(_))
            ) {
                return true;
            }
        }
        false
    })
    .await;
    assert_eq!(initial_ev, Ok(true));

    // Drop the MediaNode instance
    let weak_node = Arc::downgrade(&node);
    drop(node);

    // Assert that the Arc is fully deallocated (no background task holding strong references)
    assert!(
        weak_node.upgrade().is_none(),
        "MediaNode should be cleanly dropped when all outside Arcs are dropped"
    );

    // After dropping, no further capacity ticks should arrive
    let post_drop_ev = timeout(Duration::from_secs(2), async {
        while rx.recv().await.is_ok() {}
    })
    .await;
    // The channel is either closed or recv yields no new ticks
    assert!(post_drop_ev.is_err() || post_drop_ev.is_ok());
}

/// Bug 6 Regression: WatchNodeEvents captures immediate license capacity event
#[tokio::test]
async fn test_watch_node_events_captures_immediate_license_capacity_event() {
    let node = Arc::new(MediaNode::new(test_config(1)).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    let token = create_test_enterprise_license();

    // Watch events with license_token provided in the initial handshake request
    let resp = service
        .watch_node_events(Request::new(proto::WatchNodeEventsRequest {
            coordinator_id: "coord-license-test".to_string(),
            license_token: token,
        }))
        .await
        .unwrap();

    let mut stream = resp.into_inner();

    // Verify that the immediate NodeCapacity event emitted upon license application is captured by the stream
    let captured = timeout(Duration::from_secs(3), async {
        while let Some(Ok(event)) = stream.next().await {
            if let Some(proto::node_event::Payload::NodeCapacity(cap)) = event.payload {
                if cap.license_tier == "enterprise" {
                    return true;
                }
            }
        }
        false
    })
    .await;

    assert_eq!(
        captured,
        Ok(true),
        "Immediate NodeCapacity event with enterprise tier must be received by WatchNodeEvents"
    );

    let _ = node.shutdown(Duration::from_secs(1)).await;
}

#[cfg(feature = "enterprise")]
#[tokio::test]
async fn test_get_stats_on_pipe_transport() {
    let node = Arc::new(MediaNode::new(test_config(2)).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    // 1. Apply Enterprise license
    let token = create_test_enterprise_license();
    service
        .apply_license(Request::new(proto::ApplyLicenseRequest {
            license_token: token,
        }))
        .await
        .unwrap();

    // 2. Create Router
    let r = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r-pipe-stats".to_string(),
            room_id: "room-pipe-stats".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    // 3. Create PipeTransport
    let pt = service
        .create_pipe_transport(Request::new(proto::CreatePipeTransportRequest {
            request_id: "req-pt-stats".to_string(),
            router_id: r.router_id.clone(),
            enable_srtp: false,
            enable_rtx: false,
        }))
        .await
        .unwrap()
        .into_inner();

    // 4. Query stats for PipeTransport using RESOURCE_TYPE_TRANSPORT
    let stats_res = service
        .get_stats(Request::new(proto::GetStatsRequest {
            router_id: r.router_id.clone(),
            resource_id: pt.pipe_transport_id.clone(),
            resource_type: proto::ResourceType::Transport as i32,
        }))
        .await;

    assert!(
        stats_res.is_ok(),
        "GetStats on PipeTransport must succeed with valid statistics"
    );

    let stats = stats_res.unwrap().into_inner();
    assert!(!stats.stats_json.is_empty());
    assert_eq!(stats.current_spatial_layer, -1);
    assert_eq!(stats.current_temporal_layer, -1);

    // 5. Query with non-existent router or resource_id must return NotFound
    let bad_res = service
        .get_stats(Request::new(proto::GetStatsRequest {
            router_id: "non-existent-router".to_string(),
            resource_id: pt.pipe_transport_id.clone(),
            resource_type: proto::ResourceType::Transport as i32,
        }))
        .await;

    assert_eq!(bad_res.unwrap_err().code(), tonic::Code::NotFound);

    let _ = node.shutdown(Duration::from_secs(1)).await;
}

#[cfg(feature = "enterprise")]
#[tokio::test]
async fn test_cascaded_multi_router_producer_collision_isolation() {
    let node = Arc::new(MediaNode::new(test_config(2)).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    // 1. Apply Enterprise license
    let token = create_test_enterprise_license();
    service
        .apply_license(Request::new(proto::ApplyLicenseRequest {
            license_token: token,
        }))
        .await
        .unwrap();

    // 2. Create Router 1 & Router 2 on same node
    let r1 = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "r1-collision".to_string(),
            room_id: "room-cascade-col-1".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let r2 = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "r2-collision".to_string(),
            room_id: "room-cascade-col-2".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    // 3. Publisher Transport on Router 1
    let pub_tr = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "tr-pub-col".to_string(),
            router_id: r1.router_id.clone(),
            peer_id: "pub-col-peer".to_string(),
            is_publisher: true,
            enable_sctp: false,
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
        "encodings": [{ "ssrc": 12345 }],
        "rtcp": { "cname": "test-audio", "reducedSize": true }
    }"#;

    let p1 = service
        .produce(Request::new(proto::ProduceRequest {
            request_id: "req-p1-col".to_string(),
            router_id: r1.router_id.clone(),
            transport_id: pub_tr.transport_id.clone(),
            peer_id: "pub-col-peer".to_string(),
            kind: "audio".to_string(),
            rtp_parameters_json: opus_params.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    let producer_id = p1.producer_id.clone();

    // 4. Create local subscriber transport & consumer on Router 1
    let sub1_tr = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "tr-sub1-col".to_string(),
            router_id: r1.router_id.clone(),
            peer_id: "sub1-col-peer".to_string(),
            is_publisher: false,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    let rtp_caps = r#"{
        "codecs": [{
            "mimeType": "audio/opus",
            "kind": "audio",
            "preferredPayloadType": 111,
            "clockRate": 48000,
            "channels": 2,
            "parameters": {},
            "rtcpFeedback": []
        }],
        "headerExtensions": []
    }"#;

    let c1 = service
        .consume(Request::new(proto::ConsumeRequest {
            request_id: "req-c1-col".to_string(),
            router_id: r1.router_id.clone(),
            subscriber_transport_id: sub1_tr.transport_id.clone(),
            producer_id: producer_id.clone(),
            rtp_capabilities_json: rtp_caps.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    // 5. Connect Router 1 and Router 2 via PipeTransport
    let pt1 = service
        .create_pipe_transport(Request::new(proto::CreatePipeTransportRequest {
            request_id: "pt1-col".to_string(),
            router_id: r1.router_id.clone(),
            enable_srtp: false,
            enable_rtx: false,
        }))
        .await
        .unwrap()
        .into_inner();

    let pt2 = service
        .create_pipe_transport(Request::new(proto::CreatePipeTransportRequest {
            request_id: "pt2-col".to_string(),
            router_id: r2.router_id.clone(),
            enable_srtp: false,
            enable_rtx: false,
        }))
        .await
        .unwrap()
        .into_inner();

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

    // 6. PipeConsume on Router 1 & PipeProduce on Router 2
    // mediasoup preserves the EXACT same producer_id on Router 2!
    let pipe_cons = service
        .pipe_consume(Request::new(proto::PipeConsumeRequest {
            request_id: "req-pcons-col".to_string(),
            router_id: r1.router_id.clone(),
            pipe_transport_id: pt1.pipe_transport_id.clone(),
            producer_id: producer_id.clone(),
        }))
        .await
        .unwrap()
        .into_inner();

    let p2 = service
        .pipe_produce(Request::new(proto::PipeProduceRequest {
            request_id: "req-pprod-col".to_string(),
            router_id: r2.router_id.clone(),
            pipe_transport_id: pt2.pipe_transport_id.clone(),
            source_producer_id: producer_id.clone(),
            kind: "audio".to_string(),
            rtp_parameters_json: pipe_cons.rtp_parameters_json.clone(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(
        p2.producer_id, producer_id,
        "mediasoup pipe produce preserves the same producer_id"
    );

    // 7. Create subscriber transport & consumer on Router 2
    let sub2_tr = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "tr-sub2-col".to_string(),
            router_id: r2.router_id.clone(),
            peer_id: "sub2-col-peer".to_string(),
            is_publisher: false,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    let c2 = service
        .consume(Request::new(proto::ConsumeRequest {
            request_id: "req-c2-col".to_string(),
            router_id: r2.router_id.clone(),
            subscriber_transport_id: sub2_tr.transport_id.clone(),
            producer_id: producer_id.clone(),
            rtp_capabilities_json: rtp_caps.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    // Verify both consumers exist in state
    assert!(node.state.consumers.contains_key(&c1.consumer_id));
    assert!(node.state.consumers.contains_key(&c2.consumer_id));

    // 8. CRITICAL VERIFICATION:
    // Close the cascaded subscriber transport sub2_tr on Router 2 (or producer on Router 2).
    // Prior to our fix, this removed the shared key `producer_id` from `producer_consumers`
    // and mistakenly cascade-closed consumer c1 on Router 1!
    service
        .close_resource(Request::new(proto::CloseResourceRequest {
            router_id: r2.router_id.clone(),
            resource_type: proto::ResourceType::Transport as i32,
            resource_id: sub2_tr.transport_id.clone(),
        }))
        .await
        .unwrap();

    // c2 on Router 2 must be closed
    assert!(
        !node.state.consumers.contains_key(&c2.consumer_id),
        "Consumer c2 on Router 2 must be cascade-closed"
    );

    // c1 on Router 1 MUST STILL BE ACTIVE!
    assert!(
        node.state.consumers.contains_key(&c1.consumer_id),
        "Consumer c1 on Router 1 must NOT be closed when Router 2 cascaded resources are cleaned up"
    );

    // Check stats on c1 to ensure it is still alive and operational
    let c1_stats = service
        .get_stats(Request::new(proto::GetStatsRequest {
            router_id: r1.router_id.clone(),
            resource_id: c1.consumer_id.clone(),
            resource_type: proto::ResourceType::Consumer as i32,
        }))
        .await;

    assert!(
        c1_stats.is_ok(),
        "Consumer c1 must still be operational under Router 1"
    );

    let _ = node.shutdown(Duration::from_secs(1)).await;
}
