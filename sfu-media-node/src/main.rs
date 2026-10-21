use sfu_media_node::proto::sfu::v1::sfu_media_engine_service_server::SfuMediaEngineServiceServer;
use sfu_media_node::{Config, GrpcMediaEngineService, MediaNode};
use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Server;

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C signal handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C (SIGINT), initiating graceful shutdown");
        },
        _ = terminate => {
            tracing::info!("Received SIGTERM, initiating graceful shutdown");
        },
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let config = Config::from_env();

    tracing::info!(
        listen_ip = %config.listen_ip,
        announced_ip = ?config.announced_ip,
        rtc_min_port = config.rtc_min_port,
        rtc_max_port = config.rtc_max_port,
        grpc_port = config.grpc_port,
        worker_count = config.worker_count,
        "Starting SFU Media Node..."
    );

    let grpc_addr: SocketAddr = format!("0.0.0.0:{}", config.grpc_port).parse()?;
    let media_node = Arc::new(MediaNode::new(config).await?);
    let service = GrpcMediaEngineService::new(media_node);

    tracing::info!("gRPC server listening on {}", grpc_addr);

    Server::builder()
        .add_service(SfuMediaEngineServiceServer::new(service))
        .serve_with_shutdown(grpc_addr, shutdown_signal())
        .await?;

    tracing::info!("SFU Media Node shutdown completed gracefully.");

    Ok(())
}
