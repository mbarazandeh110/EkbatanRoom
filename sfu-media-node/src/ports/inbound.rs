use crate::domain::entities::MediaKind;
use crate::domain::entities::*;
use crate::domain::errors::SfuDomainError;
use async_trait::async_trait;
use mediasoup::prelude::*;

#[derive(Debug, Clone)]
pub struct CreateTransportParams {
    pub request_id: String,
    pub router_id: String,
    pub peer_id: String,
    pub is_publisher: bool,
    pub enable_sctp: bool,
}

#[derive(Debug, Clone)]
pub struct CreateTransportResult {
    pub transport_id: String,
    pub ice_ufrag: String,
    pub ice_pwd: String,
    pub ice_candidates: Vec<IceCandidateInfo>,
    pub dtls_fingerprints: Vec<DtlsFingerprintInfo>,
    pub sctp_port: u32,
    pub raw_ice_parameters_json: String,
    pub raw_dtls_parameters_json: String,
}

#[derive(Debug, Clone)]
pub struct ConnectTransportParams {
    pub router_id: String,
    pub transport_id: String,
    pub dtls_role: DtlsRole,
    pub remote_fingerprints: Vec<mediasoup::types::data_structures::DtlsFingerprint>,
}

#[derive(Debug, Clone)]
pub struct CreatePipeTransportParams {
    pub request_id: String,
    pub router_id: String,
    pub enable_srtp: bool,
    pub enable_rtx: bool,
}

#[derive(Debug, Clone)]
pub struct CreatePipeTransportResult {
    pub pipe_transport_id: String,
    pub ip: String,
    pub port: u32,
    pub srtp_parameters_json: String,
}

#[derive(Debug, Clone)]
pub struct ConnectPipeTransportParams {
    pub router_id: String,
    pub pipe_transport_id: String,
    pub remote_ip: String,
    pub remote_port: u32,
    pub remote_srtp_parameters_json: String,
}

#[derive(Debug, Clone)]
pub struct PipeProduceParams {
    pub request_id: String,
    pub router_id: String,
    pub pipe_transport_id: String,
    pub producer_id: String,
    pub kind: MediaKind,
    pub rtp_parameters: RtpParameters,
    pub paused: bool,
    pub raw_rtp_parameters_json: String,
}

#[derive(Debug, Clone)]
pub struct PipeConsumeParams {
    pub request_id: String,
    pub router_id: String,
    pub pipe_transport_id: String,
    pub producer_id: String,
}

#[derive(Debug, Clone)]
pub struct ProduceParams {
    pub request_id: String,
    pub router_id: String,
    pub transport_id: String,
    pub peer_id: String,
    pub kind: MediaKind,
    pub rtp_parameters: RtpParameters,
    pub paused: bool,
    pub raw_rtp_parameters_json: String,
}

#[derive(Debug, Clone)]
pub struct ProduceResult {
    pub producer_id: String,
    pub kind: String,
    pub encodings: Vec<SimulcastEncodingInfo>,
}

#[derive(Debug, Clone)]
pub struct ConsumeParams {
    pub request_id: String,
    pub router_id: String,
    pub subscriber_transport_id: String,
    pub producer_id: String,
    pub rtp_capabilities: RtpCapabilities,
    pub paused: bool,
    pub raw_rtp_capabilities_json: String,
}

#[derive(Debug, Clone)]
pub struct ConsumeResult {
    pub consumer_id: String,
    pub producer_id: String,
    pub kind: String,
    pub rtp_parameters_json: String,
    pub ssrc: u32,
    pub rtx_ssrc: u32,
    pub payload_type: u32,
    pub rtx_payload_type: u32,
    pub codec_mime_type: String,
    pub clock_rate: u32,
    pub encodings: Vec<SimulcastEncodingInfo>,
    pub initial_spatial_layer: i32,
    pub initial_temporal_layer: i32,
}

#[async_trait]
pub trait RouterUseCase: Send + Sync {
    async fn create_router(
        &self,
        request_id: &str,
        room_id: &str,
    ) -> Result<(String, u32, String), SfuDomainError>;

    fn close_router(&self, router_id: &str) -> Result<(), SfuDomainError>;
}

#[derive(Debug, Clone)]
pub struct RestartIceResult {
    pub ice_ufrag: String,
    pub ice_pwd: String,
    pub raw_ice_parameters_json: String,
}

#[async_trait]
pub trait TransportUseCase: Send + Sync {
    async fn create_transport(
        &self,
        params: CreateTransportParams,
    ) -> Result<CreateTransportResult, SfuDomainError>;

    async fn connect_transport(&self, params: ConnectTransportParams)
        -> Result<(), SfuDomainError>;

    async fn restart_ice(
        &self,
        router_id: &str,
        transport_id: &str,
    ) -> Result<RestartIceResult, SfuDomainError>;

    fn close_transport(&self, router_id: &str, transport_id: &str) -> Result<(), SfuDomainError>;
}

#[async_trait]
pub trait MediaUseCase: Send + Sync {
    async fn produce(&self, params: ProduceParams) -> Result<ProduceResult, SfuDomainError>;

    async fn consume(&self, params: ConsumeParams) -> Result<ConsumeResult, SfuDomainError>;

    async fn set_media_paused(
        &self,
        router_id: &str,
        resource_id: &str,
        is_producer: bool,
        paused: bool,
    ) -> Result<(), SfuDomainError>;

    async fn set_consumer_preferred_layers(
        &self,
        router_id: &str,
        consumer_id: &str,
        layers: ConsumerLayersInfo,
    ) -> Result<(), SfuDomainError>;

    fn close_producer(&self, router_id: &str, producer_id: &str) -> Result<(), SfuDomainError>;

    fn close_consumer(&self, router_id: &str, consumer_id: &str) -> Result<(), SfuDomainError>;

    async fn get_stats(
        &self,
        router_id: &str,
        resource_id: &str,
        resource_type: &str,
    ) -> Result<ResourceStatsInfo, SfuDomainError>;
}

#[async_trait]
pub trait InterNodePipeUseCase: Send + Sync {
    async fn create_pipe_transport(
        &self,
        params: CreatePipeTransportParams,
    ) -> Result<CreatePipeTransportResult, SfuDomainError>;

    async fn connect_pipe_transport(
        &self,
        params: ConnectPipeTransportParams,
    ) -> Result<(), SfuDomainError>;

    async fn pipe_produce(
        &self,
        params: PipeProduceParams,
    ) -> Result<ProduceResult, SfuDomainError>;

    async fn pipe_consume(
        &self,
        params: PipeConsumeParams,
    ) -> Result<ConsumeResult, SfuDomainError>;

    fn is_enterprise_enabled(&self) -> bool;

    fn abort_cleanup_tasks(&self) {}
}
