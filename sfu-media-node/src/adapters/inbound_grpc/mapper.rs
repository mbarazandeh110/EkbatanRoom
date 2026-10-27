use crate::domain::entities::*;
use crate::domain::errors::SfuDomainError;
use crate::ports::inbound::{
    ConnectTransportParams, ConsumeParams, ConsumeResult, CreateTransportParams,
    CreateTransportResult, ProduceParams, ProduceResult, RestartIceResult,
};
use crate::proto::sfu::v1 as proto;
use mediasoup::prelude::{RtpCapabilities, RtpParameters};

impl From<SfuDomainError> for tonic::Status {
    fn from(err: SfuDomainError) -> Self {
        match err {
            SfuDomainError::InvalidArgument(msg) => tonic::Status::invalid_argument(msg),
            SfuDomainError::NotFound(msg) => tonic::Status::not_found(msg),
            SfuDomainError::AlreadyExists(msg) => tonic::Status::already_exists(msg),
            SfuDomainError::IncompatibleCodec(msg) => tonic::Status::failed_precondition(msg),
            SfuDomainError::IdempotencyConflict(msg) => tonic::Status::already_exists(msg),
            SfuDomainError::InFlight => tonic::Status::aborted("Operation currently in flight"),
            SfuDomainError::Mediasoup(msg) => {
                tonic::Status::internal(format!("Mediasoup error: {msg}"))
            }
            SfuDomainError::Internal(msg) => tonic::Status::internal(msg),
        }
    }
}

pub fn to_create_router_response(
    router_id: String,
    worker_index: u32,
    caps_json: String,
) -> proto::CreateRouterResponse {
    proto::CreateRouterResponse {
        router_id,
        worker_index,
        router_rtp_capabilities_json: caps_json,
    }
}

pub fn from_create_transport_request(req: proto::CreateTransportRequest) -> CreateTransportParams {
    CreateTransportParams {
        request_id: req.request_id,
        router_id: req.router_id,
        peer_id: req.peer_id,
        is_publisher: req.is_publisher,
        enable_sctp: req.enable_sctp,
    }
}

pub fn to_create_transport_response(res: CreateTransportResult) -> proto::CreateTransportResponse {
    proto::CreateTransportResponse {
        transport_id: res.transport_id,
        ice_ufrag: res.ice_ufrag,
        ice_pwd: res.ice_pwd,
        ice_candidates: res
            .ice_candidates
            .into_iter()
            .map(|c| proto::IceCandidateInfo {
                foundation: c.foundation,
                priority: c.priority,
                ip: c.ip,
                protocol: c.protocol,
                port: c.port,
                r#type: c.r#type,
                tcp_type: c.tcp_type,
                sdp_candidate_line: c.sdp_candidate_line,
            })
            .collect(),
        dtls_fingerprints: res
            .dtls_fingerprints
            .into_iter()
            .map(|f| proto::DtlsFingerprintInfo {
                algorithm: f.algorithm,
                value: f.value,
            })
            .collect(),
        sctp_port: res.sctp_port,
        raw_ice_parameters_json: res.raw_ice_parameters_json,
        raw_dtls_parameters_json: res.raw_dtls_parameters_json,
    }
}

pub fn from_connect_transport_request(
    req: proto::ConnectTransportRequest,
) -> Result<ConnectTransportParams, SfuDomainError> {
    let dtls_role = match req.dtls_role.to_lowercase().as_str() {
        "auto" | "" => DtlsRole::Auto,
        "client" => DtlsRole::Client,
        "server" => DtlsRole::Server,
        other => {
            return Err(SfuDomainError::InvalidArgument(format!(
                "Invalid dtls_role: {other}"
            )))
        }
    };
    let remote_fingerprints = req
        .remote_fingerprints
        .into_iter()
        .map(|f| DtlsFingerprintInfo {
            algorithm: f.algorithm,
            value: f.value,
        })
        .collect();

    Ok(ConnectTransportParams {
        router_id: req.router_id,
        transport_id: req.transport_id,
        dtls_role,
        remote_fingerprints,
    })
}

pub fn to_restart_ice_response(res: RestartIceResult) -> proto::RestartIceResponse {
    proto::RestartIceResponse {
        ice_ufrag: res.ice_ufrag,
        ice_pwd: res.ice_pwd,
        raw_ice_parameters_json: res.raw_ice_parameters_json,
    }
}

pub fn from_produce_request(req: proto::ProduceRequest) -> Result<ProduceParams, SfuDomainError> {
    let kind = match req.kind.to_lowercase().as_str() {
        "audio" => MediaKind::Audio,
        "video" => MediaKind::Video,
        other => {
            return Err(SfuDomainError::InvalidArgument(format!(
                "Invalid media kind: {other}"
            )))
        }
    };

    let rtp_parameters: RtpParameters =
        serde_json::from_str(&req.rtp_parameters_json).map_err(|e| {
            SfuDomainError::InvalidArgument(format!("Invalid rtp_parameters_json: {e}"))
        })?;

    Ok(ProduceParams {
        request_id: req.request_id,
        router_id: req.router_id,
        transport_id: req.transport_id,
        peer_id: req.peer_id,
        kind,
        rtp_parameters,
        paused: req.paused,
        raw_rtp_parameters_json: req.rtp_parameters_json,
    })
}

pub fn to_produce_response(res: ProduceResult) -> proto::ProduceResponse {
    proto::ProduceResponse {
        producer_id: res.producer_id,
        kind: res.kind,
        encodings: res
            .encodings
            .into_iter()
            .map(|e| proto::SimulcastEncodingInfo {
                rid: e.rid,
                ssrc: e.ssrc,
                rtx_ssrc: e.rtx_ssrc,
                max_bitrate: e.max_bitrate,
                scalability_mode: e.scalability_mode,
            })
            .collect(),
    }
}

pub fn from_consume_request(req: proto::ConsumeRequest) -> Result<ConsumeParams, SfuDomainError> {
    let rtp_capabilities: RtpCapabilities = serde_json::from_str(&req.rtp_capabilities_json)
        .map_err(|e| {
            SfuDomainError::InvalidArgument(format!("Invalid rtp_capabilities_json: {e}"))
        })?;

    Ok(ConsumeParams {
        request_id: req.request_id,
        router_id: req.router_id,
        subscriber_transport_id: req.subscriber_transport_id,
        producer_id: req.producer_id,
        rtp_capabilities,
        paused: req.paused,
        raw_rtp_capabilities_json: req.rtp_capabilities_json,
    })
}

pub fn to_consume_response(res: ConsumeResult) -> proto::ConsumeResponse {
    proto::ConsumeResponse {
        consumer_id: res.consumer_id,
        producer_id: res.producer_id,
        kind: res.kind,
        rtp_parameters_json: res.rtp_parameters_json,
        ssrc: res.ssrc,
        rtx_ssrc: res.rtx_ssrc,
        payload_type: res.payload_type,
        rtx_payload_type: res.rtx_payload_type,
        codec_mime_type: res.codec_mime_type,
        clock_rate: res.clock_rate,
        encodings: res
            .encodings
            .into_iter()
            .map(|e| proto::SimulcastEncodingInfo {
                rid: e.rid,
                ssrc: e.ssrc,
                rtx_ssrc: e.rtx_ssrc,
                max_bitrate: e.max_bitrate,
                scalability_mode: e.scalability_mode,
            })
            .collect(),
        initial_spatial_layer: res.initial_spatial_layer,
        initial_temporal_layer: res.initial_temporal_layer,
    }
}

pub fn from_set_consumer_preferred_layers_request(
    req: proto::SetConsumerPreferredLayersRequest,
) -> Result<(String, String, ConsumerLayersInfo), SfuDomainError> {
    if !(0..=255).contains(&req.spatial_layer) {
        return Err(SfuDomainError::InvalidArgument(format!(
            "Invalid spatial_layer: {}",
            req.spatial_layer
        )));
    }
    let temporal_layer = if req.temporal_layer >= 0 {
        if req.temporal_layer > 255 {
            return Err(SfuDomainError::InvalidArgument(format!(
                "Invalid temporal_layer: {}",
                req.temporal_layer
            )));
        }
        Some(req.temporal_layer as u8)
    } else {
        None
    };

    Ok((
        req.router_id,
        req.consumer_id,
        ConsumerLayersInfo {
            spatial_layer: req.spatial_layer as u8,
            temporal_layer,
        },
    ))
}

pub fn to_get_stats_response(info: ResourceStatsInfo) -> proto::GetStatsResponse {
    proto::GetStatsResponse {
        stats_json: info.stats_json,
        bytes_sent: info.bytes_sent,
        bytes_received: info.bytes_received,
        packets_sent: info.packets_sent,
        packets_received: info.packets_received,
        current_spatial_layer: info.current_spatial_layer,
        current_temporal_layer: info.current_temporal_layer,
    }
}
