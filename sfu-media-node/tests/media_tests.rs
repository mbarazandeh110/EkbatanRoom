use sfu_media_node::proto::sfu::v1 as proto;
use sfu_media_node::proto::sfu::v1::sfu_media_engine_service_server::SfuMediaEngineService;
use sfu_media_node::{Config, GrpcMediaEngineService, MediaNode};
use std::sync::Arc;
use tonic::Request;

fn test_config() -> Config {
    let mut config = Config::from_env();
    config.worker_count = 1;
    config
}

#[tokio::test]
async fn test_produce_consume_lifecycle() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    // 1. Create Router
    let r_resp = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r-media".to_string(),
            room_id: "room-media".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let router_id = r_resp.router_id;

    // 2. Create Publisher Transport
    let pub_tr = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-pub-tr".to_string(),
            router_id: router_id.clone(),
            peer_id: "peer-pub".to_string(),
            is_publisher: true,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    // 3. Create Producer (Audio Opus)
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
                "ssrc": 11111111
            }
        ],
        "rtcp": {
            "cname": "audio-test",
            "reducedSize": true
        }
    }"#;

    let prod_resp = service
        .produce(Request::new(proto::ProduceRequest {
            request_id: "req-prod-1".to_string(),
            router_id: router_id.clone(),
            transport_id: pub_tr.transport_id.clone(),
            peer_id: "peer-pub".to_string(),
            kind: "audio".to_string(),
            rtp_parameters_json: opus_rtp_params.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(!prod_resp.producer_id.is_empty());
    assert_eq!(prod_resp.kind, "audio");

    // 4. Create Subscriber Transport
    let sub_tr = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-sub-tr".to_string(),
            router_id: router_id.clone(),
            peer_id: "peer-sub".to_string(),
            is_publisher: false,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    // 5. Incompatible RTP Capabilities -> must fail can_consume
    let incompatible_caps = r#"{
        "codecs": [
            {
                "mimeType": "video/H264",
                "kind": "video",
                "clockRate": 90000,
                "preferredPayloadType": 125,
                "rtcpFeedback": [],
                "parameters": {}
            }
        ],
        "headerExtensions": []
    }"#;

    let consume_incompatible_err = service
        .consume(Request::new(proto::ConsumeRequest {
            request_id: "req-cons-bad".to_string(),
            router_id: router_id.clone(),
            subscriber_transport_id: sub_tr.transport_id.clone(),
            producer_id: prod_resp.producer_id.clone(),
            rtp_capabilities_json: incompatible_caps.to_string(),
            paused: false,
        }))
        .await;

    assert!(consume_incompatible_err.is_err());

    // 6. Compatible RTP Capabilities -> must succeed
    let compatible_caps = r#"{
        "codecs": [
            {
                "mimeType": "audio/opus",
                "kind": "audio",
                "clockRate": 48000,
                "channels": 2,
                "preferredPayloadType": 111,
                "rtcpFeedback": [],
                "parameters": {}
            }
        ],
        "headerExtensions": []
    }"#;

    let cons_resp = service
        .consume(Request::new(proto::ConsumeRequest {
            request_id: "req-cons-good".to_string(),
            router_id: router_id.clone(),
            subscriber_transport_id: sub_tr.transport_id.clone(),
            producer_id: prod_resp.producer_id.clone(),
            rtp_capabilities_json: compatible_caps.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(!cons_resp.consumer_id.is_empty());
    assert_eq!(cons_resp.kind, "audio");
    assert!(cons_resp.payload_type > 0);
    assert_eq!(cons_resp.codec_mime_type, "audio/opus");

    // 7. Pause and Resume Media
    let pause_res = service
        .set_media_paused(Request::new(proto::SetMediaPausedRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Producer as i32,
            resource_id: prod_resp.producer_id.clone(),
            paused: true,
        }))
        .await;
    assert!(pause_res.is_ok());

    let resume_res = service
        .set_media_paused(Request::new(proto::SetMediaPausedRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Producer as i32,
            resource_id: prod_resp.producer_id.clone(),
            paused: false,
        }))
        .await;
    assert!(resume_res.is_ok());

    // 8. Set Consumer Preferred Layers
    let layer_res = service
        .set_consumer_preferred_layers(Request::new(proto::SetConsumerPreferredLayersRequest {
            router_id: router_id.clone(),
            consumer_id: cons_resp.consumer_id.clone(),
            spatial_layer: 0,
            temporal_layer: 0,
        }))
        .await;
    assert!(layer_res.is_ok());

    // 9. Get Stats
    let stats_tr = service
        .get_stats(Request::new(proto::GetStatsRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Transport as i32,
            resource_id: pub_tr.transport_id.clone(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(!stats_tr.stats_json.is_empty());

    let stats_prod = service
        .get_stats(Request::new(proto::GetStatsRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Producer as i32,
            resource_id: prod_resp.producer_id.clone(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(!stats_prod.stats_json.is_empty());

    let stats_cons = service
        .get_stats(Request::new(proto::GetStatsRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Consumer as i32,
            resource_id: cons_resp.consumer_id.clone(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(!stats_cons.stats_json.is_empty());

    // 10. Close Producer
    service
        .close_resource(Request::new(proto::CloseResourceRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Producer as i32,
            resource_id: prod_resp.producer_id.clone(),
        }))
        .await
        .unwrap();

    // 11. Close Router (cascades to all remaining resources)
    service
        .close_resource(Request::new(proto::CloseResourceRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Router as i32,
            resource_id: router_id.clone(),
        }))
        .await
        .unwrap();
}

#[tokio::test]
async fn test_node_events_stream() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());
    let mut rx = node.event_hub.subscribe();

    let r_resp = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r-ev".to_string(),
            room_id: "room-ev".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let tr = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-tr-ev".to_string(),
            router_id: r_resp.router_id.clone(),
            peer_id: "peer-ev".to_string(),
            is_publisher: true,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    // Close transport -> generates ResourceClosed event
    service
        .close_resource(Request::new(proto::CloseResourceRequest {
            router_id: r_resp.router_id.clone(),
            resource_type: proto::ResourceType::Transport as i32,
            resource_id: tr.transport_id.clone(),
        }))
        .await
        .unwrap();

    let received = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
        .await
        .expect("Timeout waiting for event")
        .expect("Failed to receive event");

    assert_eq!(received.router_id, r_resp.router_id);
    match received.payload.unwrap() {
        sfu_media_node::domain::DomainNodeEventPayload::ResourceClosed(rc) => {
            assert_eq!(rc.resource_id, tr.transport_id);
        }
        other => panic!("Expected ResourceClosed, got: {:?}", other),
    }
}
