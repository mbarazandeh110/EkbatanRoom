use sfu_media_node::proto::sfu::v1 as proto;
use sfu_media_node::{Config, MediaNode};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

fn test_config() -> Config {
    let mut config = Config::from_env();
    config.worker_count = 1;
    config
}

#[tokio::test]
async fn test_router_close_event_emission() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());
    let mut rx = node.subscribe_events();

    // 1. Create Router
    let r_resp = node
        .create_router(proto::CreateRouterRequest {
            request_id: "req-router-ev-1".to_string(),
            room_id: "room-event-test".to_string(),
        })
        .await
        .unwrap();

    let router_id = r_resp.router_id;

    // 2. Close Router
    node.close_resource(proto::CloseResourceRequest {
        router_id: router_id.clone(),
        resource_type: proto::ResourceType::Router as i32,
        resource_id: router_id.clone(),
    })
    .unwrap();

    // 3. Verify ResourceClosed event was emitted for the Router
    let received = timeout(Duration::from_secs(2), async {
        while let Ok(event) = rx.recv().await {
            if let Some(proto::node_event::Payload::ResourceClosed(rc)) = event.payload {
                if rc.resource_id == router_id
                    && rc.resource_type == proto::ResourceType::Router as i32
                {
                    return true;
                }
            }
        }
        false
    })
    .await;

    assert_eq!(
        received,
        Ok(true),
        "ResourceClosed event for Router must be emitted and delivered before router cleanup"
    );
}

#[tokio::test]
async fn test_producer_close_cascades_to_consumers() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());
    let mut rx = node.subscribe_events();

    // 1. Create Router
    let r_resp = node
        .create_router(proto::CreateRouterRequest {
            request_id: "r-cascade".to_string(),
            room_id: "room-cascade".to_string(),
        })
        .await
        .unwrap();
    let router_id = r_resp.router_id;

    // 2. Publisher Transport & Audio Producer
    let pub_tr = node
        .create_transport(proto::CreateTransportRequest {
            request_id: "tr-pub-casc".to_string(),
            router_id: router_id.clone(),
            peer_id: "peer-pub-casc".to_string(),
            is_publisher: true,
            enable_sctp: false,
        })
        .await
        .unwrap();

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
        "encodings": [{ "ssrc": 22222222 }],
        "rtcp": { "cname": "audio-casc", "reducedSize": true }
    }"#;

    let prod = node
        .produce(proto::ProduceRequest {
            request_id: "prod-casc-1".to_string(),
            router_id: router_id.clone(),
            transport_id: pub_tr.transport_id.clone(),
            peer_id: "peer-pub-casc".to_string(),
            kind: "audio".to_string(),
            rtp_parameters_json: opus_rtp_params.to_string(),
            paused: false,
        })
        .await
        .unwrap();

    // 3. Subscriber Transport & Audio Consumer
    let sub_tr = node
        .create_transport(proto::CreateTransportRequest {
            request_id: "tr-sub-casc".to_string(),
            router_id: router_id.clone(),
            peer_id: "peer-sub-casc".to_string(),
            is_publisher: false,
            enable_sctp: false,
        })
        .await
        .unwrap();

    let rtp_caps = r#"{
        "codecs": [
            {
                "mimeType": "audio/opus",
                "kind": "audio",
                "clockRate": 48000,
                "channels": 2,
                "preferredPayloadType": 111,
                "rtcpFeedback": []
            }
        ],
        "headerExtensions": []
    }"#;

    let cons = node
        .consume(proto::ConsumeRequest {
            request_id: "cons-casc-1".to_string(),
            router_id: router_id.clone(),
            subscriber_transport_id: sub_tr.transport_id.clone(),
            producer_id: prod.producer_id.clone(),
            rtp_capabilities_json: rtp_caps.to_string(),
            paused: false,
        })
        .await
        .unwrap();

    // 4. Close Producer
    node.close_resource(proto::CloseResourceRequest {
        router_id: router_id.clone(),
        resource_type: proto::ResourceType::Producer as i32,
        resource_id: prod.producer_id.clone(),
    })
    .unwrap();

    // 5. Verify Consumer was cascade-closed and cannot be paused
    let pause_res = node
        .set_media_paused(proto::SetMediaPausedRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Consumer as i32,
            resource_id: cons.consumer_id.clone(),
            paused: true,
        })
        .await;

    assert!(
        pause_res.is_err(),
        "Consumer must be cascade-closed and not found/active when producer is closed"
    );

    // 6. Verify ResourceClosed event was emitted for the Consumer
    let received_consumer_close = timeout(Duration::from_secs(2), async {
        while let Ok(event) = rx.recv().await {
            if let Some(proto::node_event::Payload::ResourceClosed(rc)) = event.payload {
                if rc.resource_id == cons.consumer_id
                    && rc.resource_type == proto::ResourceType::Consumer as i32
                {
                    return true;
                }
            }
        }
        false
    })
    .await;

    assert_eq!(
        received_consumer_close,
        Ok(true),
        "ResourceClosed event must be emitted for cascade-closed consumer"
    );
}

#[tokio::test]
async fn test_produce_idempotency_includes_paused() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());

    let r_resp = node
        .create_router(proto::CreateRouterRequest {
            request_id: "r-idem-p".to_string(),
            room_id: "room-idem-p".to_string(),
        })
        .await
        .unwrap();

    let pub_tr = node
        .create_transport(proto::CreateTransportRequest {
            request_id: "tr-idem-p".to_string(),
            router_id: r_resp.router_id.clone(),
            peer_id: "peer-idem-p".to_string(),
            is_publisher: true,
            enable_sctp: false,
        })
        .await
        .unwrap();

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
        "encodings": [{ "ssrc": 33333333 }],
        "rtcp": { "cname": "audio-idem", "reducedSize": true }
    }"#;

    // Produce with paused: false
    let prod1 = node
        .produce(proto::ProduceRequest {
            request_id: "req-prod-conflict".to_string(),
            router_id: r_resp.router_id.clone(),
            transport_id: pub_tr.transport_id.clone(),
            peer_id: "peer-idem-p".to_string(),
            kind: "audio".to_string(),
            rtp_parameters_json: opus_rtp_params.to_string(),
            paused: false,
        })
        .await
        .unwrap();

    assert!(!prod1.producer_id.is_empty());

    // Same request_id but paused: true => must trigger IdempotencyConflict
    let conflict_res = node
        .produce(proto::ProduceRequest {
            request_id: "req-prod-conflict".to_string(),
            router_id: r_resp.router_id.clone(),
            transport_id: pub_tr.transport_id.clone(),
            peer_id: "peer-idem-p".to_string(),
            kind: "audio".to_string(),
            rtp_parameters_json: opus_rtp_params.to_string(),
            paused: true,
        })
        .await;

    assert!(
        conflict_res.is_err(),
        "Changing paused state with identical request_id must trigger IdempotencyConflict"
    );
}
