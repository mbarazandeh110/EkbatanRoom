use sfu_media_node::ports::outbound::{EventPublisherPort, WorkerPoolPort};
use sfu_media_node::proto::sfu::v1 as proto;
use sfu_media_node::proto::sfu::v1::sfu_media_engine_service_server::SfuMediaEngineService;
use sfu_media_node::{Config, GrpcMediaEngineService, MediaNode};
use std::sync::Arc;
use tokio_stream::StreamExt;
use tonic::Request;

fn multi_worker_config() -> Config {
    let mut config = Config::from_env();
    config.worker_count = 2;
    config.worker_capacity_weight = 10;
    config.worker_spillover_threshold_percent = 50.0;
    config
}

#[tokio::test]
async fn test_multi_worker_spillover_and_peer_affinity() {
    let node = Arc::new(MediaNode::new(multi_worker_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    // 1. Create Router
    let r_resp = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r-spillover".to_string(),
            room_id: "room-spillover".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let router_id = r_resp.router_id;
    let primary_worker = r_resp.worker_index as usize;
    assert_eq!(primary_worker, 0);

    // 2. Peer 1 creates publisher transport
    let p1_pub = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-p1-pub".to_string(),
            router_id: router_id.clone(),
            peer_id: "peer-1".to_string(),
            is_publisher: true,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    let p1_pub_item = node.state.transports.get(&p1_pub.transport_id).unwrap();
    assert_eq!(p1_pub_item.worker_index, 0);
    drop(p1_pub_item);

    // 3. Peer 1 produces a video stream (weight 10) to exceed the 50% spillover threshold (threshold = 10 * 50% = 5)
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
        "encodings": [
            {
                "ssrc": 22222222
            }
        ],
        "rtcp": {
            "cname": "video-test",
            "reducedSize": true
        }
    }"#;

    let p1_prod = service
        .produce(Request::new(proto::ProduceRequest {
            request_id: "req-p1-prod".to_string(),
            router_id: router_id.clone(),
            transport_id: p1_pub.transport_id.clone(),
            peer_id: "peer-1".to_string(),
            kind: "video".to_string(),
            rtp_parameters_json: vp8_rtp_params.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(!p1_prod.producer_id.is_empty());
    let worker_0_score = node.worker_pool.worker_resource_score(0);
    // Transport (1) + VideoProducer (10) = 11
    assert!(worker_0_score >= 10);

    // 4. Peer 1 creates a subscriber transport -> MUST respect Peer Affinity and stay on worker 0!
    let p1_sub = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-p1-sub".to_string(),
            router_id: router_id.clone(),
            peer_id: "peer-1".to_string(),
            is_publisher: false,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    let p1_sub_item = node.state.transports.get(&p1_sub.transport_id).unwrap();
    assert_eq!(
        p1_sub_item.worker_index, 0,
        "Peer 1 subscriber transport must have affinity with worker 0"
    );
    drop(p1_sub_item);

    // 5. Peer 2 arrives (new peer). Worker 0 load is > 50% threshold.
    // Therefore, Peer 2 MUST spill over to Worker 1!
    let p2_pub = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-p2-pub".to_string(),
            router_id: router_id.clone(),
            peer_id: "peer-2".to_string(),
            is_publisher: true,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    let p2_pub_item = node.state.transports.get(&p2_pub.transport_id).unwrap();
    assert_eq!(
        p2_pub_item.worker_index, 1,
        "Peer 2 must spill over to worker 1"
    );
    assert_ne!(
        p2_pub_item.physical_router_id, router_id,
        "Peer 2 must be on sub-router"
    );
    drop(p2_pub_item);

    // 6. Peer 2 creates subscriber transport -> MUST respect Peer Affinity and stay on worker 1!
    let p2_sub = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-p2-sub".to_string(),
            router_id: router_id.clone(),
            peer_id: "peer-2".to_string(),
            is_publisher: false,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    let p2_sub_item = node.state.transports.get(&p2_sub.transport_id).unwrap();
    assert_eq!(
        p2_sub_item.worker_index, 1,
        "Peer 2 subscriber transport must have affinity with worker 1"
    );
    drop(p2_sub_item);

    let _ = node.shutdown(std::time::Duration::from_secs(1)).await;
}

#[tokio::test]
async fn test_cross_worker_transparent_piped_consume() {
    let node = Arc::new(MediaNode::new(multi_worker_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    // 1. Create Router
    let r_resp = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r-cross".to_string(),
            room_id: "room-cross".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let router_id = r_resp.router_id;

    // 2. Peer 1 on Worker 0 creates publisher transport and video producer
    let p1_pub = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-c-p1-pub".to_string(),
            router_id: router_id.clone(),
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
        "encodings": [
            {
                "ssrc": 33333333
            }
        ],
        "rtcp": {
            "cname": "video-cross",
            "reducedSize": true
        }
    }"#;

    let p1_prod = service
        .produce(Request::new(proto::ProduceRequest {
            request_id: "req-c-p1-prod".to_string(),
            router_id: router_id.clone(),
            transport_id: p1_pub.transport_id.clone(),
            peer_id: "peer-1".to_string(),
            kind: "video".to_string(),
            rtp_parameters_json: vp8_rtp_params.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    // 3. Peer 2 on Worker 1 creates subscriber transport (spillover because Worker 0 load >= threshold)
    let p2_sub = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-c-p2-sub".to_string(),
            router_id: router_id.clone(),
            peer_id: "peer-2".to_string(),
            is_publisher: false,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    let p2_sub_item = node.state.transports.get(&p2_sub.transport_id).unwrap();
    assert_eq!(p2_sub_item.worker_index, 1);
    let sub_physical_router_id = p2_sub_item.physical_router_id.clone();
    drop(p2_sub_item);

    // 4. Peer 2 consumes Peer 1's producer across workers!
    let vp8_caps = r#"{
        "codecs": [
            {
                "mimeType": "video/VP8",
                "kind": "video",
                "preferredPayloadType": 96,
                "clockRate": 90000,
                "parameters": {},
                "rtcpFeedback": []
            }
        ],
        "headerExtensions": []
    }"#;

    let consume_resp = service
        .consume(Request::new(proto::ConsumeRequest {
            request_id: "req-c-consume".to_string(),
            router_id: router_id.clone(),
            subscriber_transport_id: p2_sub.transport_id.clone(),
            producer_id: p1_prod.producer_id.clone(),
            rtp_capabilities_json: vp8_caps.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(!consume_resp.consumer_id.is_empty());
    assert_eq!(consume_resp.kind, "video");

    // Verify pipe exists in state.piped_producers
    let piped_entries = node
        .state
        .piped_producers
        .get(&p1_prod.producer_id)
        .expect("Producer should be piped to worker 1");
    assert_eq!(piped_entries.len(), 1);
    assert_eq!(piped_entries[0].target_router_id, sub_physical_router_id);
    assert_eq!(piped_entries[0].target_worker_index, 1);
    drop(piped_entries);

    // 5. Test set_media_paused propagates to piped producer
    service
        .set_media_paused(Request::new(proto::SetMediaPausedRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Producer as i32,
            resource_id: p1_prod.producer_id.clone(),
            paused: true,
        }))
        .await
        .unwrap();

    let piped_entries = node
        .state
        .piped_producers
        .get(&p1_prod.producer_id)
        .unwrap();
    assert!(piped_entries[0].pipe_producer.paused());
    drop(piped_entries);

    // 6. Test close producer cleans up piped producer
    service
        .close_resource(Request::new(proto::CloseResourceRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Producer as i32,
            resource_id: p1_prod.producer_id.clone(),
        }))
        .await
        .unwrap();

    assert!(!node
        .state
        .piped_producers
        .contains_key(&p1_prod.producer_id));

    let _ = node.shutdown(std::time::Duration::from_secs(1)).await;
}

#[tokio::test]
async fn test_node_capacity_metrics_and_events() {
    let node = Arc::new(MediaNode::new(multi_worker_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    // 1. Call GetNodeCapacity RPC directly
    let cap = service
        .get_node_capacity(Request::new(proto::GetNodeCapacityRequest {}))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(cap.total_workers, 2);
    assert_eq!(cap.alive_workers, 2);
    assert_eq!(cap.total_capacity_weight, 20); // 2 workers * weight 10
    assert_eq!(cap.active_routers, 0);
    assert_eq!(cap.active_transports, 0);
    assert_eq!(
        cap.enterprise_cascading_enabled,
        cfg!(feature = "enterprise")
    );

    // 2. Subscribe to WatchNodeEvents and verify NodeCapacity event arrives
    let mut stream = service
        .watch_node_events(Request::new(proto::WatchNodeEventsRequest {
            coordinator_id: "coord-test".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    // Trigger an immediate capacity event by publishing one
    node.event_hub
        .publish_node_capacity(&node.get_node_capacity_info());

    let event = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    match event.payload {
        Some(proto::node_event::Payload::NodeCapacity(c)) => {
            assert_eq!(c.total_workers, 2);
            assert_eq!(c.total_capacity_weight, 20);
            assert_eq!(c.enterprise_cascading_enabled, cfg!(feature = "enterprise"));
        }
        other => panic!("Expected NodeCapacity event, got {:?}", other),
    }

    let _ = node.shutdown(std::time::Duration::from_secs(1)).await;
}

#[tokio::test]
async fn test_sub_router_audio_observer_and_pipe_transport_cleanup() {
    let node = Arc::new(MediaNode::new(multi_worker_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    // 1. Create Router
    let r_resp = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r-sub-audio".to_string(),
            room_id: "room-sub-audio".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let router_id = r_resp.router_id;

    // 2. Add resource load to Worker 0 so spillover triggers for Peer 2
    node.worker_pool.add_resource_score(0, 10);

    // 3. Peer 2 creates publisher transport (spills over to Worker 1)
    let p2_pub = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-p2-pub-audio".to_string(),
            router_id: router_id.clone(),
            peer_id: "peer-2".to_string(),
            is_publisher: true,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    let p2_item = node.state.transports.get(&p2_pub.transport_id).unwrap();
    assert_eq!(p2_item.worker_index, 1);
    drop(p2_item);

    // 4. Peer 2 produces AUDIO on Worker 1's sub-router -> MUST add to sub-router's AudioLevelObserver without error!
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
        "encodings": [
            {
                "ssrc": 55555555
            }
        ],
        "rtcp": {
            "cname": "audio-sub-test",
            "reducedSize": true
        }
    }"#;

    let p2_prod = service
        .produce(Request::new(proto::ProduceRequest {
            request_id: "req-p2-prod-audio".to_string(),
            router_id: router_id.clone(),
            transport_id: p2_pub.transport_id.clone(),
            peer_id: "peer-2".to_string(),
            kind: "audio".to_string(),
            rtp_parameters_json: opus_rtp_params.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(!p2_prod.producer_id.is_empty());

    // 5. Create a PipeTransport and ensure it is tracked in state.pipe_transports
    #[cfg(feature = "enterprise")]
    {
        let pipe_resp = service
            .create_pipe_transport(Request::new(proto::CreatePipeTransportRequest {
                request_id: "req-pipe-cleanup".to_string(),
                router_id: router_id.clone(),
                enable_srtp: false,
                enable_rtx: false,
            }))
            .await
            .unwrap()
            .into_inner();

        assert!(!pipe_resp.pipe_transport_id.is_empty());
        assert_eq!(node.state.pipe_transports.len(), 1);
    }
    #[cfg(not(feature = "enterprise"))]
    {
        use mediasoup::prelude::*;
        let router = node.state.routers.get(&router_id).unwrap().router.clone();
        let pt = router
            .create_pipe_transport(mediasoup::pipe_transport::PipeTransportOptions::new(
                ListenInfo {
                    protocol: Protocol::Udp,
                    ip: "127.0.0.1".parse().unwrap(),
                    announced_address: None,
                    expose_internal_ip: false,
                    port: None,
                    port_range: None,
                    flags: None,
                    send_buffer_size: None,
                    recv_buffer_size: None,
                },
            ))
            .await
            .unwrap();
        let pt_id = pt.id().to_string();
        node.state.pipe_transports.insert(
            pt_id.clone(),
            sfu_media_node::application::state::PipeTransportItem {
                transport: pt,
                router_id: router_id.clone(),
                is_closed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            },
        );
        node.state
            .router_pipe_transports
            .entry(router_id.clone())
            .or_default()
            .insert(pt_id);
        assert_eq!(node.state.pipe_transports.len(), 1);
    }

    // 6. Close the router -> MUST cascade and clean up PipeTransport (preventing memory leak!)
    service
        .close_resource(Request::new(proto::CloseResourceRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Router as i32,
            resource_id: router_id.clone(),
        }))
        .await
        .unwrap();

    assert!(
        node.state.pipe_transports.is_empty(),
        "pipe_transports should be empty after close_router"
    );

    let _ = node.shutdown(std::time::Duration::from_secs(1)).await;
}

#[tokio::test]
async fn test_initial_router_distribution_using_load_counts() {
    let node = Arc::new(MediaNode::new(multi_worker_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    // Create room 1 -> should go to worker 0
    let r1 = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-dist-1".to_string(),
            room_id: "room-dist-1".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(r1.worker_index, 0);

    // Create room 2 -> since resource_scores are equal (0 == 0), load_counts tie-breaking
    // should pick worker 1 (load_count 0 vs 1)!
    let r2 = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-dist-2".to_string(),
            room_id: "room-dist-2".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(r2.worker_index, 1);

    let _ = node.shutdown(std::time::Duration::from_secs(1)).await;
}

#[tokio::test]
async fn test_draining_flag_on_shutdown() {
    let node = Arc::new(MediaNode::new(multi_worker_config()).await.unwrap());
    assert!(!node
        .state
        .is_draining
        .load(std::sync::atomic::Ordering::SeqCst));
    assert!(!node.get_node_capacity_info().is_draining);

    node.shutdown(std::time::Duration::from_secs(1))
        .await
        .unwrap();

    assert!(node
        .state
        .is_draining
        .load(std::sync::atomic::Ordering::SeqCst));
    assert!(node.get_node_capacity_info().is_draining);
}

#[tokio::test]
async fn test_draining_rejects_new_resources() {
    let node = Arc::new(MediaNode::new(multi_worker_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    // Mark as draining
    node.state
        .is_draining
        .store(true, std::sync::atomic::Ordering::SeqCst);

    // 1. Create Router should be rejected with Unavailable
    let r_err = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r-draining".to_string(),
            room_id: "room-draining".to_string(),
        }))
        .await
        .unwrap_err();
    assert_eq!(r_err.code(), tonic::Code::Unavailable);

    // 2. Create Transport should be rejected with Unavailable
    let t_err = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-t-draining".to_string(),
            router_id: "router-draining".to_string(),
            peer_id: "peer-draining".to_string(),
            is_publisher: true,
            enable_sctp: false,
        }))
        .await
        .unwrap_err();
    assert_eq!(t_err.code(), tonic::Code::Unavailable);

    // 3. Create PipeTransport should be rejected with Unavailable
    let p_err = service
        .create_pipe_transport(Request::new(proto::CreatePipeTransportRequest {
            request_id: "req-p-draining".to_string(),
            router_id: "router-draining".to_string(),
            enable_srtp: false,
            enable_rtx: false,
        }))
        .await
        .unwrap_err();
    #[cfg(feature = "enterprise")]
    assert_eq!(p_err.code(), tonic::Code::Unavailable);
    #[cfg(not(feature = "enterprise"))]
    assert_eq!(p_err.code(), tonic::Code::Unimplemented);

    // 4. Produce should be rejected with Unavailable
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
        "encodings": [{ "ssrc": 11111111 }],
        "rtcp": { "cname": "audio-test", "reducedSize": true }
    }"#;

    let prod_err = service
        .produce(Request::new(proto::ProduceRequest {
            request_id: "req-prod-draining".to_string(),
            router_id: "router-draining".to_string(),
            transport_id: "transport-draining".to_string(),
            peer_id: "peer-draining".to_string(),
            kind: "audio".to_string(),
            rtp_parameters_json: opus_params.to_string(),
            paused: false,
        }))
        .await
        .unwrap_err();
    assert_eq!(prod_err.code(), tonic::Code::Unavailable);

    // 5. Consume should be rejected with Unavailable
    let rtp_caps = r#"{
        "codecs": [{
            "mimeType": "audio/opus",
            "kind": "audio",
            "clockRate": 48000,
            "channels": 2,
            "preferredPayloadType": 111,
            "rtcpFeedback": []
        }],
        "headerExtensions": []
    }"#;

    let cons_err = service
        .consume(Request::new(proto::ConsumeRequest {
            request_id: "req-cons-draining".to_string(),
            router_id: "router-draining".to_string(),
            subscriber_transport_id: "transport-draining".to_string(),
            producer_id: "prod-draining".to_string(),
            rtp_capabilities_json: rtp_caps.to_string(),
            paused: false,
        }))
        .await
        .unwrap_err();
    assert_eq!(cons_err.code(), tonic::Code::Unavailable);
}

#[tokio::test]
async fn test_dead_primary_worker_spillover() {
    let node = Arc::new(MediaNode::new(multi_worker_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    // 1. Create router (will be assigned to worker 0)
    let r_resp = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r-dead-worker".to_string(),
            room_id: "room-dead-worker".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    assert_eq!(r_resp.worker_index, 0);

    // 2. Mark worker 0 as dead (simulating mediasoup-worker crash)
    node.worker_pool.mark_dead(0);
    assert!(!node.worker_pool.is_worker_alive(0));

    // 3. New peer creates transport on this router
    // Even though worker 0 is primary, because it is dead, transport must fallback to worker 1
    let t_resp = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-t-dead-worker".to_string(),
            router_id: r_resp.router_id.clone(),
            peer_id: "peer-fallback".to_string(),
            is_publisher: true,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    let item = node.state.transports.get(&t_resp.transport_id).unwrap();
    assert_eq!(item.worker_index, 1);
    drop(item);

    let _ = node.shutdown(std::time::Duration::from_secs(1)).await;
}

#[tokio::test]
async fn test_sub_router_concurrent_creation_singleflight() {
    let node = Arc::new(MediaNode::new(multi_worker_config()).await.unwrap());
    let service = Arc::new(GrpcMediaEngineService::new(node.clone()));

    // 1. Create router on worker 0
    let r_resp = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r-conc".to_string(),
            room_id: "room-conc".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let router_id = r_resp.router_id;

    // 2. Exceed spillover threshold on worker 0 so new peers spillover to worker 1
    node.worker_pool.add_resource_score(0, 10);

    // 3. Launch 5 concurrent create_transport requests for different peers
    let mut handles = Vec::new();
    for i in 0..5 {
        let svc = service.clone();
        let rid = router_id.clone();
        let peer_id = format!("peer-conc-{}", i);
        let req_id = format!("req-conc-{}", i);
        handles.push(tokio::spawn(async move {
            svc.create_transport(Request::new(proto::CreateTransportRequest {
                request_id: req_id,
                router_id: rid,
                peer_id,
                is_publisher: true,
                enable_sctp: false,
            }))
            .await
        }));
    }

    for h in handles {
        let res = h.await.unwrap().unwrap().into_inner();
        let item = node.state.transports.get(&res.transport_id).unwrap();
        assert_eq!(item.worker_index, 1);
    }

    // 4. Verify that exactly ONE sub-router was created on worker 1 (no duplicates overwritten)
    let router_entry = node.state.routers.get(&router_id).unwrap();
    assert_eq!(router_entry.sub_routers.len(), 1);
    assert!(router_entry.sub_routers.contains_key(&1));
    drop(router_entry);

    let _ = node.shutdown(std::time::Duration::from_secs(1)).await;
}

#[tokio::test]
async fn test_worker_death_resets_scores_and_prevents_capacity_spike() {
    let mut config = multi_worker_config();
    config.worker_count = 2;
    config.worker_capacity_weight = 100;
    let node = Arc::new(MediaNode::new(config).await.unwrap());

    // 1. Add resource score 50 to worker 0
    node.worker_pool.add_resource_score(0, 50);
    assert_eq!(node.worker_pool.worker_resource_score(0), 50);
    assert_eq!(node.worker_pool.total_resource_score(), 50);

    let info_before = node.get_node_capacity_info();
    assert_eq!(info_before.alive_workers, 2);
    assert_eq!(info_before.total_capacity_weight, 200);
    assert_eq!(info_before.logical_load_percent, 25.0); // 50 / 200 = 25%

    // 2. Mark worker 0 as dead (simulating crash)
    node.worker_pool.mark_dead(0);
    assert!(!node.worker_pool.is_worker_alive(0));
    assert_eq!(node.worker_pool.worker_resource_score(0), 0);
    assert_eq!(node.worker_pool.total_resource_score(), 0);

    let info_after = node.get_node_capacity_info();
    assert_eq!(info_after.alive_workers, 1);
    assert_eq!(info_after.total_capacity_weight, 100);
    // logical_load_percent must be 0%, NOT spiking falsely to 50%
    assert_eq!(info_after.logical_load_percent, 0.0);
    assert_eq!(info_after.max_worker_load_percent, 0.0);

    let _ = node.shutdown(std::time::Duration::from_secs(1)).await;
}

#[tokio::test]
async fn test_piped_producer_cleanup_on_last_sub_router_consumer_closed() {
    let node = Arc::new(MediaNode::new(multi_worker_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    // 1. Create Router on Room 1 (anchored to worker 0)
    let r_resp = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-clean-pipe-r".to_string(),
            room_id: "room-clean-pipe".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let router_id = r_resp.router_id;

    // 2. Peer 1 on Worker 0 publishes video producer (weight 10 >= threshold 5)
    let p1_pub = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-clean-p1-pub".to_string(),
            router_id: router_id.clone(),
            peer_id: "peer-pub-0".to_string(),
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
        "encodings": [{ "ssrc": 44444444 }],
        "rtcp": { "cname": "video-clean", "reducedSize": true }
    }"#;

    let p1_prod = service
        .produce(Request::new(proto::ProduceRequest {
            request_id: "req-clean-prod".to_string(),
            router_id: router_id.clone(),
            transport_id: p1_pub.transport_id.clone(),
            peer_id: "peer-pub-0".to_string(),
            kind: "video".to_string(),
            rtp_parameters_json: vp8_rtp_params.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    // 3. Peer 2 creates subscriber transport (spillover to worker 1)
    let p2_sub = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-clean-p2-sub".to_string(),
            router_id: router_id.clone(),
            peer_id: "peer-sub-1".to_string(),
            is_publisher: false,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    let p2_sub_item = node.state.transports.get(&p2_sub.transport_id).unwrap();
    assert_eq!(p2_sub_item.worker_index, 1);
    drop(p2_sub_item);

    let w0_score_before = node.worker_pool.worker_resource_score(0);
    let w1_score_before = node.worker_pool.worker_resource_score(1);

    // 4. Peer 2 consumes Peer 1's producer across workers -> triggers pipe_to_router
    let vp8_caps = r#"{
        "codecs": [
            {
                "mimeType": "video/VP8",
                "kind": "video",
                "preferredPayloadType": 96,
                "clockRate": 90000,
                "parameters": {},
                "rtcpFeedback": []
            }
        ],
        "headerExtensions": []
    }"#;

    let consume_resp = service
        .consume(Request::new(proto::ConsumeRequest {
            request_id: "req-clean-consume".to_string(),
            router_id: router_id.clone(),
            subscriber_transport_id: p2_sub.transport_id.clone(),
            producer_id: p1_prod.producer_id.clone(),
            rtp_capabilities_json: vp8_caps.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    // Verify pipe exists and score was incremented
    assert!(node
        .state
        .piped_producers
        .contains_key(&p1_prod.producer_id));
    assert_eq!(
        node.worker_pool.worker_resource_score(0),
        w0_score_before + 1
    );
    assert_eq!(
        node.worker_pool.worker_resource_score(1),
        w1_score_before + 1 + 2
    ); // 1 pipe + 2 video consumer

    // 5. Close the consumer on Worker 1. Consumer score is decremented,
    // but PipedProducerEntry remains cached to prevent race conditions on subsequent consumer arrivals.
    service
        .close_resource(Request::new(proto::CloseResourceRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Consumer as i32,
            resource_id: consume_resp.consumer_id.clone(),
        }))
        .await
        .unwrap();

    assert!(
        node.state
            .piped_producers
            .contains_key(&p1_prod.producer_id),
        "Piped producer remains cached until producer or router closes"
    );
    assert_eq!(
        node.worker_pool.worker_resource_score(0),
        w0_score_before + 1
    );
    assert_eq!(
        node.worker_pool.worker_resource_score(1),
        w1_score_before + 1
    );

    // 6. Close the producer. Now PipedProducerEntry MUST be cleaned up and worker scores returned to baseline!
    service
        .close_resource(Request::new(proto::CloseResourceRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Producer as i32,
            resource_id: p1_prod.producer_id.clone(),
        }))
        .await
        .unwrap();

    assert!(
        !node
            .state
            .piped_producers
            .contains_key(&p1_prod.producer_id),
        "Piped producer must be removed when the producer closes"
    );

    assert_eq!(
        node.worker_pool.worker_resource_score(0),
        w0_score_before - 10
    );
    assert_eq!(node.worker_pool.worker_resource_score(1), w1_score_before);

    let _ = node.shutdown(std::time::Duration::from_secs(1)).await;
}
