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
    pub remote_fingerprints: Vec<DtlsFingerprintInfo>,
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

#[derive(Debug, Clone)]
pub struct CreatePlainTransportParams {
    pub request_id: String,
    pub router_id: String,
    pub rtcp_mux: bool,
    pub comedia: bool,
    pub enable_sctp: bool,
    pub listening_ip: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CreatePlainTransportResult {
    pub transport_id: String,
    pub ip: String,
    pub port: u32,
    pub rtcp_port: u32,
}

#[derive(Debug, Clone)]
pub struct ConnectPlainTransportParams {
    pub router_id: String,
    pub transport_id: String,
    pub remote_ip: String,
    pub remote_port: u32,
    pub remote_rtcp_port: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct ProduceDataParams {
    pub request_id: String,
    pub router_id: String,
    pub transport_id: String,
    pub peer_id: String,
    pub sctp_stream_parameters_json: String,
    pub label: String,
    pub protocol: String,
}

#[derive(Debug, Clone)]
pub struct ProduceDataResult {
    pub data_producer_id: String,
    pub label: String,
    pub protocol: String,
    pub sctp_stream_parameters_json: String,
}

#[derive(Debug, Clone)]
pub struct ConsumeDataParams {
    pub request_id: String,
    pub router_id: String,
    pub transport_id: String,
    pub data_producer_id: String,
}

#[derive(Debug, Clone)]
pub struct ConsumeDataResult {
    pub data_consumer_id: String,
    pub data_producer_id: String,
    pub label: String,
    pub protocol: String,
    pub sctp_stream_parameters_json: String,
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

    async fn create_plain_transport(
        &self,
        params: CreatePlainTransportParams,
    ) -> Result<CreatePlainTransportResult, SfuDomainError>;

    async fn connect_plain_transport(
        &self,
        params: ConnectPlainTransportParams,
    ) -> Result<(), SfuDomainError>;
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

    async fn request_key_frame(
        &self,
        router_id: &str,
        consumer_id: &str,
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
pub trait DataUseCase: Send + Sync {
    async fn produce_data(
        &self,
        params: ProduceDataParams,
    ) -> Result<ProduceDataResult, SfuDomainError>;

    async fn consume_data(
        &self,
        params: ConsumeDataParams,
    ) -> Result<ConsumeDataResult, SfuDomainError>;

    async fn send_direct_data(
        &self,
        router_id: &str,
        label: &str,
        protocol: &str,
        payload: &[u8],
    ) -> Result<(), SfuDomainError>;

    fn close_data_producer(
        &self,
        router_id: &str,
        data_producer_id: &str,
    ) -> Result<(), SfuDomainError>;
    fn close_data_consumer(
        &self,
        router_id: &str,
        data_consumer_id: &str,
    ) -> Result<(), SfuDomainError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyLicenseResult {
    pub is_valid: bool,
    pub license_tier: String,
    pub expires_at_timestamp: i64,
    pub features: Vec<String>,
    pub issued_to: String,
    pub error_message: String,
}

#[async_trait]
pub trait LicenseUseCase: Send + Sync {
    async fn apply_license(&self, token: &str) -> Result<ApplyLicenseResult, SfuDomainError>;
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
