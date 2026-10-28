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
async fn test_router_lifecycle_and_idempotency() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    // 1. CreateRouter
    let req = proto::CreateRouterRequest {
        request_id: "req-router-1".to_string(),
        room_id: "room-1".to_string(),
    };

    let resp1 = service
        .create_router(Request::new(req.clone()))
        .await
        .unwrap()
        .into_inner();
    assert!(!resp1.router_id.is_empty());
    assert!(!resp1.router_rtp_capabilities_json.is_empty());

    // 2. Idempotent call with same request_id and payload returns identical result
    let resp2 = service
        .create_router(Request::new(req))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp1.router_id, resp2.router_id);
    assert_eq!(resp1.worker_index, resp2.worker_index);

    // 3. Different room with different request_id
    let req2 = proto::CreateRouterRequest {
        request_id: "req-router-2".to_string(),
        room_id: "room-2".to_string(),
    };
    let resp3 = service
        .create_router(Request::new(req2))
        .await
        .unwrap()
        .into_inner();
    assert_ne!(resp1.router_id, resp3.router_id);

    // 4. Close router
    let close_req = proto::CloseResourceRequest {
        router_id: resp1.router_id.clone(),
        resource_type: proto::ResourceType::Router as i32,
        resource_id: resp1.router_id.clone(),
    };
    service
        .close_resource(Request::new(close_req))
        .await
        .unwrap();

    assert!(!node.is_router_active(&resp1.router_id));
}

#[tokio::test]
async fn test_transport_lifecycle_and_ice_restart() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    // 1. Create router
    let r_resp = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "r-tr-1".to_string(),
            room_id: "room-tr-1".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    // 2. Create transport
    let tr_req = proto::CreateTransportRequest {
        request_id: "req-tr-1".to_string(),
        router_id: r_resp.router_id.clone(),
        peer_id: "peer-1".to_string(),
        is_publisher: true,
        enable_sctp: true,
    };

    let tr_resp = service
        .create_transport(Request::new(tr_req.clone()))
        .await
        .unwrap()
        .into_inner();
    assert!(!tr_resp.transport_id.is_empty());
    assert!(!tr_resp.ice_ufrag.is_empty());
    assert!(!tr_resp.ice_pwd.is_empty());
    assert!(!tr_resp.ice_candidates.is_empty());
    assert!(!tr_resp.dtls_fingerprints.is_empty());
    assert!(tr_resp.sctp_port > 0);

    // 3. Idempotent transport creation
    let tr_resp_dup = service
        .create_transport(Request::new(tr_req))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(tr_resp.transport_id, tr_resp_dup.transport_id);

    // 4. Connect transport
    let remote_fp = proto::DtlsFingerprintInfo {
        algorithm: "sha-256".to_string(),
        value: "E4:87:72:DE:DE:D6:75:A5:C0:5E:2B:A3:8F:69:B8:31:84:87:C7:E2:03:7E:63:F6:FD:E5:5D:84:1F:B5:1D:93".to_string(),
    };

    let connect_resp = service
        .connect_transport(Request::new(proto::ConnectTransportRequest {
            router_id: r_resp.router_id.clone(),
            transport_id: tr_resp.transport_id.clone(),
            dtls_role: "client".to_string(),
            remote_fingerprints: vec![remote_fp],
        }))
        .await;
    assert!(connect_resp.is_ok());

    // 5. Restart ICE
    let restart_resp = service
        .restart_ice(Request::new(proto::RestartIceRequest {
            router_id: r_resp.router_id.clone(),
            transport_id: tr_resp.transport_id.clone(),
        }))
        .await
        .unwrap()
        .into_inner();

    assert_ne!(restart_resp.ice_ufrag, tr_resp.ice_ufrag);

    // 6. Cascading close router closes transport too
    service
        .close_resource(Request::new(proto::CloseResourceRequest {
            router_id: r_resp.router_id.clone(),
            resource_type: proto::ResourceType::Router as i32,
            resource_id: r_resp.router_id.clone(),
        }))
        .await
        .unwrap();

    // Trying to restart ICE on closed transport fails
    let err = service
        .restart_ice(Request::new(proto::RestartIceRequest {
            router_id: r_resp.router_id.clone(),
            transport_id: tr_resp.transport_id.clone(),
        }))
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn test_pipe_transport_lifecycle_and_connect() {
    let node = Arc::new(MediaNode::new(test_config()).await.unwrap());
    let service = GrpcMediaEngineService::new(node.clone());

    // 1. Create two routers (e.g. representing two cascaded nodes/routers)
    let r1 = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r-pipe-1".to_string(),
            room_id: "room-pipe-1".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let r2 = service
        .create_router(Request::new(proto::CreateRouterRequest {
            request_id: "req-r-pipe-2".to_string(),
            room_id: "room-pipe-2".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    // 2. Create PipeTransport on Router 1
    let pt1_req = proto::CreatePipeTransportRequest {
        request_id: "req-pt-1".to_string(),
        router_id: r1.router_id.clone(),
        enable_srtp: true,
        enable_rtx: true,
    };

    let pt1_resp = service
        .create_pipe_transport(Request::new(pt1_req.clone()))
        .await
        .unwrap()
        .into_inner();

    assert!(!pt1_resp.pipe_transport_id.is_empty());
    assert!(!pt1_resp.ip.is_empty());
    assert!(pt1_resp.port > 0);
    assert!(!pt1_resp.srtp_parameters_json.is_empty());

    // 3. Idempotent CreatePipeTransport returns identical response
    let pt1_dup = service
        .create_pipe_transport(Request::new(pt1_req))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(pt1_resp.pipe_transport_id, pt1_dup.pipe_transport_id);

    // 4. Create PipeTransport on Router 2
    let pt2_req = proto::CreatePipeTransportRequest {
        request_id: "req-pt-2".to_string(),
        router_id: r2.router_id.clone(),
        enable_srtp: true,
        enable_rtx: true,
    };

    let pt2_resp = service
        .create_pipe_transport(Request::new(pt2_req))
        .await
        .unwrap()
        .into_inner();

    assert!(!pt2_resp.pipe_transport_id.is_empty());
    assert!(!pt2_resp.srtp_parameters_json.is_empty());

    // 5. Connect PipeTransport 1 to PipeTransport 2
    let connect_resp1 = service
        .connect_pipe_transport(Request::new(proto::ConnectPipeTransportRequest {
            router_id: r1.router_id.clone(),
            pipe_transport_id: pt1_resp.pipe_transport_id.clone(),
            remote_ip: pt2_resp.ip.clone(),
            remote_port: pt2_resp.port,
            remote_srtp_parameters_json: pt2_resp.srtp_parameters_json.clone(),
        }))
        .await;
    assert!(connect_resp1.is_ok());

    // 6. Connect PipeTransport 2 to PipeTransport 1
    let connect_resp2 = service
        .connect_pipe_transport(Request::new(proto::ConnectPipeTransportRequest {
            router_id: r2.router_id.clone(),
            pipe_transport_id: pt2_resp.pipe_transport_id.clone(),
            remote_ip: pt1_resp.ip.clone(),
            remote_port: pt1_resp.port,
            remote_srtp_parameters_json: pt1_resp.srtp_parameters_json.clone(),
        }))
        .await;
    assert!(connect_resp2.is_ok());
}
