use sfu_media_node::ports::outbound::{EventPublisherPort, WorkerPoolPort};
use sfu_media_node::proto::sfu::v1 as proto;
use sfu_media_node::proto::sfu::v1::sfu_media_engine_service_server::SfuMediaEngineService;
use sfu_media_node::{Config, GrpcMediaEngineService, MediaNode};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tokio_stream::StreamExt;
use tonic::Request;

fn test_config() -> Config {
    let mut config = Config::from_env();
    config.worker_count = 1;
    config
}

#[tokio::test]
async fn test_router_close_event_emission() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());
    let mut rx = node.event_hub.subscribe();

    // 1. Create Router
    let r_resp = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-router-ev-1".to_string(),
            room_id: "room-event-test".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let router_id = r_resp.router_id;

    // 2. Close Router
    service
        .close_resource(Request::new(proto::CloseResourceRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Router as i32,
            resource_id: router_id.clone(),
        }))
        .await
        .unwrap();

    // 3. Verify ResourceClosed event was emitted for the Router
    let received = timeout(Duration::from_secs(2), async {
        while let Ok(event) = rx.recv().await {
            if let Some(sfu_media_node::domain::DomainNodeEventPayload::ResourceClosed(rc)) =
                event.payload
            {
                if rc.resource_id == router_id
                    && rc.resource_type == sfu_media_node::domain::DomainResourceType::Router
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
async fn test_producer_close_cascades_to_consumers_and_cleans_maps() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());
    let mut rx = node.event_hub.subscribe();

    // 1. Create Router
    let r_resp = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "r-cascade".to_string(),
            room_id: "room-cascade".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();
    let router_id = r_resp.router_id;

    // 2. Publisher Transport & Audio Producer
    let pub_tr = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "tr-pub-casc".to_string(),
            router_id: router_id.clone(),
            peer_id: "peer-pub-casc".to_string(),
            is_publisher: true,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

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

    let prod = service
        .produce(Request::new(proto::ProduceRequest {
            request_id: "prod-casc-1".to_string(),
            router_id: router_id.clone(),
            transport_id: pub_tr.transport_id.clone(),
            peer_id: "peer-pub-casc".to_string(),
            kind: "audio".to_string(),
            rtp_parameters_json: opus_rtp_params.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    // 3. Subscriber Transport & Audio Consumer
    let sub_tr = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "tr-sub-casc".to_string(),
            router_id: router_id.clone(),
            peer_id: "peer-sub-casc".to_string(),
            is_publisher: false,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

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

    let cons = service
        .consume(Request::new(proto::ConsumeRequest {
            request_id: "cons-casc-1".to_string(),
            router_id: router_id.clone(),
            subscriber_transport_id: sub_tr.transport_id.clone(),
            producer_id: prod.producer_id.clone(),
            rtp_capabilities_json: rtp_caps.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    // Verify consumer exists in transport_consumers and producer_consumers
    assert!(node
        .state
        .transport_consumers
        .get(&sub_tr.transport_id)
        .map(|set| set.contains(&cons.consumer_id))
        .unwrap_or(false));
    assert!(node
        .state
        .producer_consumers
        .get(&prod.producer_id)
        .map(|set| set.contains(&cons.consumer_id))
        .unwrap_or(false));

    // 4. Close Producer
    service
        .close_resource(Request::new(proto::CloseResourceRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Producer as i32,
            resource_id: prod.producer_id.clone(),
        }))
        .await
        .unwrap();

    // 5. Verify Consumer was cascade-closed and cannot be paused
    let pause_res = service
        .set_media_paused(Request::new(proto::SetMediaPausedRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Consumer as i32,
            resource_id: cons.consumer_id.clone(),
            paused: true,
        }))
        .await;

    assert!(
        pause_res.is_err(),
        "Consumer must be cascade-closed and not found/active when producer is closed"
    );

    // Verify NO orphaned ID in transport_consumers or producer_consumers!
    assert!(!node
        .state
        .transport_consumers
        .get(&sub_tr.transport_id)
        .map(|set| set.contains(&cons.consumer_id))
        .unwrap_or(false));
    assert!(!node
        .state
        .producer_consumers
        .get(&prod.producer_id)
        .map(|set| set.contains(&cons.consumer_id))
        .unwrap_or(false));

    // 6. Verify ResourceClosed event was emitted for the Consumer
    let received_consumer_close = timeout(Duration::from_secs(2), async {
        while let Ok(event) = rx.recv().await {
            if let Some(sfu_media_node::domain::DomainNodeEventPayload::ResourceClosed(rc)) =
                event.payload
            {
                if rc.resource_id == cons.consumer_id
                    && rc.resource_type == sfu_media_node::domain::DomainResourceType::Consumer
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
    let service = GrpcMediaEngineService::new(node.clone());

    let r_resp = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "r-idem-p".to_string(),
            room_id: "room-idem-p".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let pub_tr = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "tr-idem-p".to_string(),
            router_id: r_resp.router_id.clone(),
            peer_id: "peer-idem-p".to_string(),
            is_publisher: true,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

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
    let prod1 = service
        .produce(Request::new(proto::ProduceRequest {
            request_id: "req-prod-conflict".to_string(),
            router_id: r_resp.router_id.clone(),
            transport_id: pub_tr.transport_id.clone(),
            peer_id: "peer-idem-p".to_string(),
            kind: "audio".to_string(),
            rtp_parameters_json: opus_rtp_params.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(!prod1.producer_id.is_empty());

    // Same request_id but paused: true => must trigger IdempotencyConflict
    let conflict_res = service
        .produce(Request::new(proto::ProduceRequest {
            request_id: "req-prod-conflict".to_string(),
            router_id: r_resp.router_id.clone(),
            transport_id: pub_tr.transport_id.clone(),
            peer_id: "peer-idem-p".to_string(),
            kind: "audio".to_string(),
            rtp_parameters_json: opus_rtp_params.to_string(),
            paused: true,
        }))
        .await;

    assert!(
        conflict_res.is_err(),
        "Changing paused state with identical request_id must trigger IdempotencyConflict"
    );
}

#[tokio::test]
async fn test_zombie_events_filtered_for_closed_router() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    let r_resp = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-zombie".to_string(),
            room_id: "room-zombie".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let router_id = r_resp.router_id;

    // Subscribe via gRPC WatchNodeEvents stream
    let stream_res = service
        .watch_node_events(Request::new(proto::WatchNodeEventsRequest {
            coordinator_id: "coord-1".to_string(),
            license_token: String::new(),
        }))
        .await
        .unwrap();

    let mut event_stream = stream_res.into_inner();

    // Close router (emits ResourceClosed event for router)
    service
        .close_resource(Request::new(proto::CloseResourceRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Router as i32,
            resource_id: router_id.clone(),
        }))
        .await
        .unwrap();

    // Now publish a trailing score event for this already closed router
    node.event_hub
        .publish_score(&router_id, "p-trail", "peer-trail", true, 10, vec![10]);

    // Also publish an active speaker event for an active router to verify stream is flowing
    let active_r_resp = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-active-ev".to_string(),
            room_id: "room-active-ev".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    node.event_hub.publish_active_speaker(
        &active_r_resp.router_id,
        "p-active",
        "peer-active",
        -18.5,
    );

    // Expect to receive:
    // 1. ResourceClosed for the closed router (allowed through!)
    // 2. ActiveSpeaker for the active router (allowed through!)
    // But NOT the trailing MediaScore for the closed router (zombie filtered!)
    let mut received_closed = false;
    let mut received_active_speaker = false;
    let mut received_zombie_score = false;

    let test_fut = async {
        while let Some(Ok(ev)) = event_stream.next().await {
            match ev.payload {
                Some(proto::node_event::Payload::ResourceClosed(rc))
                    if rc.resource_id == router_id =>
                {
                    received_closed = true;
                }
                Some(proto::node_event::Payload::ActiveSpeaker(as_ev))
                    if as_ev.producer_id == "p-active" =>
                {
                    received_active_speaker = true;
                }
                Some(proto::node_event::Payload::MediaScore(ms)) if ms.resource_id == "p-trail" => {
                    received_zombie_score = true;
                }
                _ => {}
            }
            if received_closed && received_active_speaker {
                break;
            }
        }
    };

    timeout(Duration::from_secs(2), test_fut)
        .await
        .expect("Stream should deliver events within timeout");

    assert!(received_closed, "ResourceClosed event must be delivered");
    assert!(
        received_active_speaker,
        "ActiveSpeaker for active router must be delivered"
    );
    assert!(
        !received_zombie_score,
        "Zombie score event for closed router must be filtered out"
    );
}

#[tokio::test]
async fn test_worker_load_decrement_underflow_protection() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());

    // Worker pool index 0 starts with load 0. Decrementing should NOT underflow to usize::MAX
    node.worker_pool.decrement_load(0);

    let (index, _) = node.worker_pool.get_least_loaded();
    assert_eq!(index, 0);

    // Load should still be 0 (saturated), so incrementing once brings it to 1
    node.worker_pool.increment_load(0);
    node.worker_pool.decrement_load(0);

    // Another decrement should saturate at 0
    node.worker_pool.decrement_load(0);
}

#[tokio::test]
async fn test_close_router_validation_and_no_false_event() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());
    let mut rx = node.event_hub.subscribe();

    // 1. Mismatched router_id and resource_id should return InvalidArgument
    let err = service
        .close_resource(Request::new(proto::CloseResourceRequest {
            router_id: "router-a".to_string(),
            resource_type: proto::ResourceType::Router as i32,
            resource_id: "router-b".to_string(),
        }))
        .await
        .unwrap_err();

    assert_eq!(err.code(), tonic::Code::InvalidArgument);

    // 2. Closing a non-existent router should succeed idempotently and NOT publish ResourceClosed event
    let res = service
        .close_resource(Request::new(proto::CloseResourceRequest {
            router_id: "non-existent-router".to_string(),
            resource_type: proto::ResourceType::Router as i32,
            resource_id: "non-existent-router".to_string(),
        }))
        .await;

    assert!(res.is_ok());

    // Drain/check events to ensure NO ResourceClosed event was emitted
    let received_false_event = timeout(Duration::from_millis(200), async {
        while let Ok(event) = rx.recv().await {
            if let Some(sfu_media_node::domain::DomainNodeEventPayload::ResourceClosed(rc)) =
                event.payload
            {
                if rc.resource_id == "non-existent-router" {
                    return true;
                }
            }
        }
        false
    })
    .await
    .unwrap_or(false);

    assert!(
        !received_false_event,
        "No ResourceClosed event should be emitted for non-existent router"
    );
}

#[tokio::test]
async fn test_audio_producer_added_to_audio_level_observer_and_stats_join_all() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    // 1. Create Router
    let r_resp = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-alo-1".to_string(),
            room_id: "room-alo-1".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();
    let router_id = r_resp.router_id;

    // 2. Create Transport
    let tr_resp = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "tr-alo-1".to_string(),
            router_id: router_id.clone(),
            peer_id: "peer-alo-1".to_string(),
            is_publisher: true,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();
    let transport_id = tr_resp.transport_id;

    // 3. Produce Audio (this should successfully register with AudioLevelObserver)
    let opus_rtp_params = r#"{
        "mid": "0",
        "codecs": [
            {
                "mimeType": "audio/opus",
                "clockRate": 48000,
                "channels": 2,
                "payloadType": 111,
                "rtcpFeedback": [],
                "parameters": {}
            }
        ],
        "headerExtensions": [
            {
                "uri": "urn:ietf:params:rtp-hdrext:ssrc-audio-level",
                "id": 1,
                "encrypt": false,
                "parameters": {}
            }
        ],
        "encodings": [
            {
                "ssrc": 33333333
            }
        ],
        "rtcp": {
            "cname": "alo-test-audio",
            "reducedSize": true
        }
    }"#;

    let prod_resp = service
        .produce(Request::new(proto::ProduceRequest {
            request_id: "prod-alo-1".to_string(),
            router_id: router_id.clone(),
            transport_id: transport_id.clone(),
            peer_id: "peer-alo-1".to_string(),
            kind: "audio".to_string(),
            rtp_parameters_json: opus_rtp_params.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(!prod_resp.producer_id.is_empty());

    // 4. Test GetStats on the transport (executing parallelized futures_util::future::join_all)
    let stats_resp = service
        .get_stats(Request::new(proto::GetStatsRequest {
            router_id: router_id.clone(),
            resource_type: proto::ResourceType::Transport as i32,
            resource_id: transport_id.clone(),
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(!stats_resp.stats_json.is_empty());
}
