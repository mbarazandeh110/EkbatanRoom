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
        .publish_node_capacity(node.get_node_capacity_metrics());

    let event = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    match event.payload {
        Some(proto::node_event::Payload::NodeCapacity(c)) => {
            assert_eq!(c.total_workers, 2);
            assert_eq!(c.total_capacity_weight, 20);
        }
        other => panic!("Expected NodeCapacity event, got {:?}", other),
    }

    let _ = node.shutdown(std::time::Duration::from_secs(1)).await;
}
