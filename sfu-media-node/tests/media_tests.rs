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

    let resource_closed = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while let Ok(event) = rx.recv().await {
            if event.router_id == r_resp.router_id {
                if let Some(sfu_media_node::domain::DomainNodeEventPayload::ResourceClosed(rc)) =
                    event.payload
                {
                    if rc.resource_id == tr.transport_id {
                        return true;
                    }
                }
            }
        }
        false
    })
    .await;

    assert_eq!(
        resource_closed,
        Ok(true),
        "Expected ResourceClosed event for closed transport"
    );
}

#[tokio::test]
async fn test_video_produce_consume_lifecycle_and_keyframe_request() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    // 1. Create Router
    let r_resp = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r-video".to_string(),
            room_id: "room-video".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let router_id = r_resp.router_id;

    // 2. Create Publisher Transport
    let pub_tr = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-pub-tr-video".to_string(),
            router_id: router_id.clone(),
            peer_id: "peer-alice".to_string(),
            is_publisher: true,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    // 3. Create Video Producer (VP8 + RTX with non-sanitized ORTC parameters from browser)
    // Note: includes missing "encrypt: bool", unknown extension URI, and missing "parameter" in rtcpFeedback
    let video_rtp_params = r#"{
        "mid": "1",
        "codecs": [
            {
                "mimeType": "video/VP8",
                "payloadType": 96,
                "clockRate": 90000,
                "parameters": {},
                "rtcpFeedback": [
                    { "type": "nack", "parameter": "" },
                    { "type": "nack", "parameter": "pli" },
                    { "type": "ccm", "parameter": "fir" },
                    { "type": "goog-remb" },
                    { "type": "transport-cc" }
                ]
            },
            {
                "mimeType": "video/rtx",
                "payloadType": 97,
                "clockRate": 90000,
                "parameters": {
                    "apt": 96
                },
                "rtcpFeedback": []
            }
        ],
        "headerExtensions": [
            {
                "id": 1,
                "uri": "urn:ietf:params:rtp-hdrext:sdes:mid"
            },
            {
                "id": 2,
                "uri": "http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time"
            },
            {
                "id": 3,
                "uri": "urn:ietf:params:rtp-hdrext:toffset"
            }
        ],
        "encodings": [
            {
                "ssrc": 2154539055,
                "rtx": {
                    "ssrc": 1846870575
                },
                "maxBitrate": 1500000
            }
        ],
        "rtcp": {
            "cname": "alice-video"
        }
    }"#;

    let prod_resp = service
        .produce(Request::new(proto::ProduceRequest {
            request_id: "req-prod-video".to_string(),
            router_id: router_id.clone(),
            transport_id: pub_tr.transport_id.clone(),
            peer_id: "peer-alice".to_string(),
            kind: "video".to_string(),
            rtp_parameters_json: video_rtp_params.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(!prod_resp.producer_id.is_empty());
    assert_eq!(prod_resp.kind, "video");
    assert_eq!(prod_resp.encodings.len(), 1);
    assert_eq!(prod_resp.encodings[0].ssrc, 2154539055);
    assert_eq!(prod_resp.encodings[0].rtx_ssrc, 1846870575);

    // 4. Create Subscriber Transport
    let sub_tr = service
        .create_transport(Request::new(proto::CreateTransportRequest {
            request_id: "req-sub-tr-video".to_string(),
            router_id: router_id.clone(),
            peer_id: "peer-bob".to_string(),
            is_publisher: false,
            enable_sctp: false,
        }))
        .await
        .unwrap()
        .into_inner();

    // 5. Consume Video Track (with keyframe request automatically dispatched)
    let video_caps = r#"{
        "codecs": [
            {
                "mimeType": "video/VP8",
                "kind": "video",
                "clockRate": 90000,
                "preferredPayloadType": 96,
                "rtcpFeedback": [
                    { "type": "nack" },
                    { "type": "nack", "parameter": "pli" },
                    { "type": "ccm", "parameter": "fir" }
                ],
                "parameters": {}
            },
            {
                "mimeType": "video/rtx",
                "kind": "video",
                "clockRate": 90000,
                "preferredPayloadType": 97,
                "rtcpFeedback": [],
                "parameters": {
                    "apt": 96
                }
            }
        ],
        "headerExtensions": [
            {
                "kind": "video",
                "uri": "urn:ietf:params:rtp-hdrext:sdes:mid",
                "preferredId": 1
            },
            {
                "kind": "video",
                "uri": "urn:ietf:params:rtp-hdrext:toffset",
                "preferredId": 3
            }
        ]
    }"#;

    let cons_resp = service
        .consume(Request::new(proto::ConsumeRequest {
            request_id: "req-cons-video".to_string(),
            router_id: router_id.clone(),
            subscriber_transport_id: sub_tr.transport_id.clone(),
            producer_id: prod_resp.producer_id.clone(),
            rtp_capabilities_json: video_caps.to_string(),
            paused: false,
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(!cons_resp.consumer_id.is_empty());
    assert_eq!(cons_resp.kind, "video");
    assert_eq!(cons_resp.codec_mime_type, "video/VP8");
    assert!(cons_resp.payload_type >= 96 && cons_resp.payload_type <= 127);
    assert!(cons_resp.ssrc > 0);

    // 6. Test standard ORTC paused consumer creation followed by resume
    let cons_paused_resp = service
        .consume(Request::new(proto::ConsumeRequest {
            request_id: "req-cons-video-paused".to_string(),
            router_id: router_id.clone(),
            subscriber_transport_id: sub_tr.transport_id.clone(),
            producer_id: prod_resp.producer_id.clone(),
            rtp_capabilities_json: video_caps.to_string(),
            paused: true,
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(!cons_paused_resp.consumer_id.is_empty());

    // Resuming the video consumer triggers an immediate RTCP PLI/FIR keyframe request
    service
        .set_media_paused(Request::new(proto::SetMediaPausedRequest {
            router_id: router_id.clone(),
            resource_id: cons_paused_resp.consumer_id.clone(),
            resource_type: proto::ResourceType::Consumer as i32,
            paused: false,
        }))
        .await
        .unwrap();

    // Resuming the video producer triggers keyframe requests across all attached active consumers
    service
        .set_media_paused(Request::new(proto::SetMediaPausedRequest {
            router_id: router_id.clone(),
            resource_id: prod_resp.producer_id.clone(),
            resource_type: proto::ResourceType::Producer as i32,
            paused: false,
        }))
        .await
        .unwrap();
}

#[test]
fn test_defensive_ortc_parameter_sanitization() {
    use sfu_media_node::adapters::inbound_grpc::mapper::{
        parse_rtp_capabilities, parse_rtp_parameters,
    };

    // Case 1: String SSRC and signed negative integer (JavaScript 32-bit overflow)
    // 2154539055 as signed 32-bit integer is -2140428241
    let raw_json = r#"{
        "mid": "0",
        "codecs": [
            {
                "mimeType": "video/VP8",
                "payloadType": "96",
                "clockRate": "90000",
                "parameters": {},
                "rtcpFeedback": [
                    { "type": "nack" },
                    { "type": "nack", "parameter": "pli" }
                ]
            }
        ],
        "headerExtensions": [
            {
                "id": "1",
                "uri": "urn:ietf:params:rtp-hdrext:sdes:mid"
            },
            {
                "id": "2",
                "uri": "urn:ietf:params:rtp-hdrext:toffset"
            }
        ],
        "encodings": [
            {
                "ssrc": -2140428241,
                "rtx": {
                    "ssrc": "1846870575"
                },
                "maxBitrate": "2500000"
            }
        ]
    }"#;

    let parsed = parse_rtp_parameters(raw_json).expect("Should sanitize and parse successfully");
    assert_eq!(parsed.encodings.len(), 1);
    assert_eq!(parsed.encodings[0].ssrc, Some(2154539055));
    assert_eq!(parsed.encodings[0].rtx.as_ref().unwrap().ssrc, 1846870575);
    assert_eq!(parsed.encodings[0].max_bitrate, Some(2500000));
    assert_eq!(parsed.codecs[0].payload_type(), 96);
    assert_eq!(parsed.codecs[0].clock_rate().get(), 90000);

    // Unsupported extension URI should be filtered out
    assert_eq!(parsed.header_extensions.len(), 1);
    assert_eq!(parsed.header_extensions[0].id, 1);
    assert!(!parsed.header_extensions[0].encrypt);
    assert!(parsed.rtcp.reduced_size);

    // Case 2: Capabilities with missing preferredEncrypt, missing direction, unknown URI
    let caps_json = r#"{
        "codecs": [
            {
                "mimeType": "video/VP8",
                "kind": "video",
                "clockRate": "90000",
                "preferredPayloadType": "96"
            }
        ],
        "headerExtensions": [
            {
                "kind": "video",
                "uri": "urn:ietf:params:rtp-hdrext:sdes:mid",
                "preferredId": 1
            },
            {
                "kind": "video",
                "uri": "urn:ietf:params:rtp-hdrext:unknown-extension",
                "preferredId": 2
            }
        ]
    }"#;

    let parsed_caps =
        parse_rtp_capabilities(caps_json).expect("Should sanitize capabilities successfully");
    assert_eq!(parsed_caps.codecs.len(), 1);
    assert_eq!(parsed_caps.codecs[0].preferred_payload_type(), Some(96));
    assert_eq!(parsed_caps.header_extensions.len(), 1);
    assert!(!parsed_caps.header_extensions[0].preferred_encrypt);
}
