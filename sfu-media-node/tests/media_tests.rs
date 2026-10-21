use sfu_media_node::proto::sfu::v1 as proto;
use sfu_media_node::{Config, MediaNode};
use std::sync::Arc;

fn test_config() -> Config {
    let mut config = Config::from_env();
    config.worker_count = 1;
    config
}

#[tokio::test]
async fn test_produce_consume_lifecycle() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());

    // 1. Create Router
    let r_resp = node
        .create_router(proto::CreateRouterRequest {
            request_id: "req-r-media".to_string(),
            room_id: "room-media".to_string(),
        })
        .await
        .unwrap();

    let router_id = r_resp.router_id;

    // 2. Create Publisher Transport
    let pub_tr = node
        .create_transport(proto::CreateTransportRequest {
            request_id: "req-pub-tr".to_string(),
            router_id: router_id.clone(),
            peer_id: "peer-pub".to_string(),
            is_publisher: true,
            enable_sctp: false,
        })
        .await
        .unwrap();

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

    let prod_resp = node
        .produce(proto::ProduceRequest {
            request_id: "req-prod-1".to_string(),
            router_id: router_id.clone(),
            transport_id: pub_tr.transport_id.clone(),
            peer_id: "peer-pub".to_string(),
            kind: "audio".to_string(),
            rtp_parameters_json: opus_rtp_params.to_string(),
            paused: false,
        })
        .await
        .unwrap();

    assert!(!prod_resp.producer_id.is_empty());
    assert_eq!(prod_resp.kind, "audio");

    // 4. Create Subscriber Transport
    let sub_tr = node
        .create_transport(proto::CreateTransportRequest {
            request_id: "req-sub-tr".to_string(),
            router_id: router_id.clone(),
            peer_id: "peer-sub".to_string(),
            is_publisher: false,
            enable_sctp: false,
        })
        .await
        .unwrap();

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

    let consume_incompatible_err = node
        .consume(proto::ConsumeRequest {
            request_id: "req-cons-bad".to_string(),
            router_id: router_id.clone(),
            subscriber_transport_id: sub_tr.transport_id.clone(),
            producer_id: prod_resp.producer_id.clone(),
            rtp_capabilities_json: incompatible_caps.to_string(),
            paused: false,
        })
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

    let cons_resp = node
        .consume(proto::ConsumeRequest {
            request_id: "req-cons-good".to_string(),
            router_id: router_id.clone(),
            subscriber_transport_id: sub_tr.transport_id.clone(),
            producer_id: prod_resp.producer_id.clone(),
            rtp_capabilities_json: compatible_caps.to_string(),
            paused: false,
        })
        .await
        .unwrap();

    assert!(!cons_resp.consumer_id.is_empty());
    assert_eq!(cons_resp.kind, "audio");
    assert!(cons_resp.payload_type > 0);
    assert_eq!(cons_resp.codec_mime_type, "audio/opus");

    // 7. Pause and Resume Media
    let pause_res = node
        .set_media_paused(proto::SetMediaPausedRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Producer as i32,
            resource_id: prod_resp.producer_id.clone(),
            paused: true,
        })
        .await;
    assert!(pause_res.is_ok());

    let resume_res = node
        .set_media_paused(proto::SetMediaPausedRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Producer as i32,
            resource_id: prod_resp.producer_id.clone(),
            paused: false,
        })
        .await;
    assert!(resume_res.is_ok());

    // 8. Set Consumer Preferred Layers
    let layer_res = node
        .set_consumer_preferred_layers(proto::SetConsumerPreferredLayersRequest {
            router_id: router_id.clone(),
            consumer_id: cons_resp.consumer_id.clone(),
            spatial_layer: 0,
            temporal_layer: 0,
        })
        .await;
    assert!(layer_res.is_ok());

    // 9. Get Stats
    let stats_tr = node
        .get_stats(proto::GetStatsRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Transport as i32,
            resource_id: pub_tr.transport_id.clone(),
        })
        .await
        .unwrap();
    assert!(!stats_tr.stats_json.is_empty());

    let stats_prod = node
        .get_stats(proto::GetStatsRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Producer as i32,
            resource_id: prod_resp.producer_id.clone(),
        })
        .await
        .unwrap();
    assert!(!stats_prod.stats_json.is_empty());

    let stats_cons = node
        .get_stats(proto::GetStatsRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Consumer as i32,
            resource_id: cons_resp.consumer_id.clone(),
        })
        .await
        .unwrap();
    assert!(!stats_cons.stats_json.is_empty());

    // 10. Close Producer
    node.close_resource(proto::CloseResourceRequest {
        router_id: router_id.clone(),
        resource_type: proto::ResourceType::Producer as i32,
        resource_id: prod_resp.producer_id.clone(),
    })
    .unwrap();

    // 11. Close Router (cascades to all remaining resources)
    node.close_resource(proto::CloseResourceRequest {
        router_id: router_id.clone(),
        resource_type: proto::ResourceType::Router as i32,
        resource_id: router_id.clone(),
    })
    .unwrap();
}

#[tokio::test]
async fn test_node_events_stream() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());
    let mut rx = node.subscribe_events();

    let r_resp = node
        .create_router(proto::CreateRouterRequest {
            request_id: "req-r-ev".to_string(),
            room_id: "room-ev".to_string(),
        })
        .await
        .unwrap();

    let tr = node
        .create_transport(proto::CreateTransportRequest {
            request_id: "req-tr-ev".to_string(),
            router_id: r_resp.router_id.clone(),
            peer_id: "peer-ev".to_string(),
            is_publisher: true,
            enable_sctp: false,
        })
        .await
        .unwrap();

    // Close transport -> generates ResourceClosed event
    node.close_resource(proto::CloseResourceRequest {
        router_id: r_resp.router_id.clone(),
        resource_type: proto::ResourceType::Transport as i32,
        resource_id: tr.transport_id.clone(),
    })
    .unwrap();

    let received = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
        .await
        .expect("Timeout waiting for event")
        .expect("Failed to receive event");

    assert_eq!(received.router_id, r_resp.router_id);
    match received.payload.unwrap() {
        proto::node_event::Payload::ResourceClosed(rc) => {
            assert_eq!(rc.resource_id, tr.transport_id);
        }
        other => panic!("Expected ResourceClosed, got: {:?}", other),
    }
}
