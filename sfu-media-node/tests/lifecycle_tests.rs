use sfu_media_node::proto::sfu::v1 as proto;
use sfu_media_node::{Config, MediaNode};
use std::sync::Arc;

fn test_config() -> Config {
    let mut config = Config::from_env();
    config.worker_count = 1;
    config
}

#[tokio::test]
async fn test_router_lifecycle_and_idempotency() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());

    // 1. CreateRouter
    let req = proto::CreateRouterRequest {
        request_id: "req-router-1".to_string(),
        room_id: "room-1".to_string(),
    };

    let resp1 = node.create_router(req.clone()).await.unwrap();
    assert!(!resp1.router_id.is_empty());
    assert!(!resp1.router_rtp_capabilities_json.is_empty());

    // 2. Idempotent call with same request_id and payload returns identical result
    let resp2 = node.create_router(req).await.unwrap();
    assert_eq!(resp1.router_id, resp2.router_id);
    assert_eq!(resp1.worker_index, resp2.worker_index);

    // 3. Different room with different request_id
    let req2 = proto::CreateRouterRequest {
        request_id: "req-router-2".to_string(),
        room_id: "room-2".to_string(),
    };
    let resp3 = node.create_router(req2).await.unwrap();
    assert_ne!(resp1.router_id, resp3.router_id);

    // 4. Close router
    let close_req = proto::CloseResourceRequest {
        router_id: resp1.router_id.clone(),
        resource_type: proto::ResourceType::Router as i32,
        resource_id: resp1.router_id.clone(),
    };
    node.close_resource(close_req).unwrap();

    assert!(!node.is_router_active(&resp1.router_id));
}

#[tokio::test]
async fn test_transport_lifecycle_and_ice_restart() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());

    // 1. Create router
    let r_resp = node
        .create_router(proto::CreateRouterRequest {
            request_id: "r-tr-1".to_string(),
            room_id: "room-tr-1".to_string(),
        })
        .await
        .unwrap();

    // 2. Create transport
    let tr_req = proto::CreateTransportRequest {
        request_id: "req-tr-1".to_string(),
        router_id: r_resp.router_id.clone(),
        peer_id: "peer-1".to_string(),
        is_publisher: true,
        enable_sctp: true,
    };

    let tr_resp = node.create_transport(tr_req.clone()).await.unwrap();
    assert!(!tr_resp.transport_id.is_empty());
    assert!(!tr_resp.ice_ufrag.is_empty());
    assert!(!tr_resp.ice_pwd.is_empty());
    assert!(!tr_resp.ice_candidates.is_empty());
    assert!(!tr_resp.dtls_fingerprints.is_empty());
    assert!(tr_resp.sctp_port > 0);

    // 3. Idempotent transport creation
    let tr_resp_dup = node.create_transport(tr_req).await.unwrap();
    assert_eq!(tr_resp.transport_id, tr_resp_dup.transport_id);

    // 4. Connect transport
    let remote_fp = proto::DtlsFingerprintInfo {
        algorithm: "sha-256".to_string(),
        value: "E4:87:72:DE:DE:D6:75:A5:C0:5E:2B:A3:8F:69:B8:31:84:87:C7:E2:03:7E:63:F6:FD:E5:5D:84:1F:B5:1D:93".to_string(),
    };

    let connect_resp = node
        .connect_transport(proto::ConnectTransportRequest {
            router_id: r_resp.router_id.clone(),
            transport_id: tr_resp.transport_id.clone(),
            dtls_role: "client".to_string(),
            remote_fingerprints: vec![remote_fp],
        })
        .await;
    assert!(connect_resp.is_ok());

    // 5. Restart ICE
    let restart_resp = node
        .restart_ice(proto::RestartIceRequest {
            router_id: r_resp.router_id.clone(),
            transport_id: tr_resp.transport_id.clone(),
        })
        .await
        .unwrap();

    assert_ne!(restart_resp.ice_ufrag, tr_resp.ice_ufrag);

    // 6. Cascading close router closes transport too
    node.close_resource(proto::CloseResourceRequest {
        router_id: r_resp.router_id.clone(),
        resource_type: proto::ResourceType::Router as i32,
        resource_id: r_resp.router_id.clone(),
    })
    .unwrap();

    // Trying to restart ICE on closed transport fails
    let err = node
        .restart_ice(proto::RestartIceRequest {
            router_id: r_resp.router_id.clone(),
            transport_id: tr_resp.transport_id.clone(),
        })
        .await;
    assert!(err.is_err());
}
