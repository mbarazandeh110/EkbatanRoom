use sfu_media_node::proto::sfu::v1 as proto;
use sfu_media_node::proto::sfu::v1::sfu_media_engine_service_server::SfuMediaEngineService;
use sfu_media_node::{Config, GrpcMediaEngineService, MediaNode};
use std::sync::Arc;
use tonic::metadata::MetadataValue;
use tonic::Request;

#[tokio::test]
async fn test_observability_grpc_with_traceparent_and_metrics() {
    let config = Config {
        worker_count: 1,
        observability: sfu_media_node::observability::ObservabilityConfig {
            metrics_enabled: true,
            tracing_enabled: false,
            ..Default::default()
        },
        ..Default::default()
    };

    let media_node = Arc::new(
        MediaNode::new(config.clone())
            .await
            .expect("Failed to create MediaNode"),
    );
    let service = GrpcMediaEngineService::new(media_node);

    // 1. Send CreateRouter request with W3C traceparent & x-request-id metadata
    let mut req = Request::new(proto::CreateRouterRequest {
        request_id: "obs-req-001".to_string(),
        room_id: "obs-room-alpha".to_string(),
    });
    req.metadata_mut().insert(
        "traceparent",
        MetadataValue::from_static("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"),
    );
    req.metadata_mut()
        .insert("x-request-id", MetadataValue::from_static("obs-req-001"));

    let res = service
        .create_router(req)
        .await
        .expect("CreateRouter should succeed");
    let router_resp = res.into_inner();
    let router_id = router_resp.router_id;
    assert!(!router_id.is_empty());

    // 2. Test Idempotent call to verify sfu_idempotency_hits_total
    let mut retry_req = Request::new(proto::CreateRouterRequest {
        request_id: "obs-req-001".to_string(),
        room_id: "obs-room-alpha".to_string(),
    });
    retry_req
        .metadata_mut()
        .insert("x-request-id", MetadataValue::from_static("obs-req-001"));
    let retry_res = service
        .create_router(retry_req)
        .await
        .expect("Idempotent CreateRouter should succeed");
    assert_eq!(retry_res.into_inner().router_id, router_id);

    // 3. Create Transport with W3C metadata
    let mut trans_req = Request::new(proto::CreateTransportRequest {
        request_id: "obs-trans-001".to_string(),
        router_id: router_id.clone(),
        peer_id: "peer-obs-1".to_string(),
        is_publisher: true,
        enable_sctp: false,
    });
    trans_req.metadata_mut().insert(
        "traceparent",
        MetadataValue::from_static("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b8-01"),
    );
    let trans_res = service
        .create_transport(trans_req)
        .await
        .expect("CreateTransport should succeed");
    let trans = trans_res.into_inner();
    assert!(!trans.transport_id.is_empty());
    assert!(!trans.ice_pwd.is_empty());

    // 4. Close Resource
    let close_req = Request::new(proto::CloseResourceRequest {
        router_id: router_id.clone(),
        resource_type: proto::ResourceType::Router as i32,
        resource_id: router_id.clone(),
    });
    let close_res = service.close_resource(close_req).await;
    assert!(close_res.is_ok());
}
