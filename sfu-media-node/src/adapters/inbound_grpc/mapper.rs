use crate::domain::entities::*;
use crate::domain::errors::SfuDomainError;
use crate::domain::validation;
use crate::ports::inbound::{
    ConnectPipeTransportParams, ConnectTransportParams, ConsumeParams, ConsumeResult,
    CreatePipeTransportParams, CreatePipeTransportResult, CreateTransportParams,
    CreateTransportResult, ProduceParams, ProduceResult, RestartIceResult,
};
use crate::proto::sfu::v1 as proto;
use mediasoup::prelude::{RtpCapabilities, RtpParameters};
use mediasoup::types::data_structures::DtlsFingerprint;

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

pub fn parse_rtp_parameters(json_str: &str) -> Result<RtpParameters, SfuDomainError> {
    let trimmed = json_str.trim();
    if trimmed.is_empty() {
        return Err(SfuDomainError::InvalidArgument(
            "rtp_parameters_json cannot be empty".to_string(),
        ));
    }

    let params: RtpParameters = serde_json::from_str(trimmed).map_err(|e| {
        SfuDomainError::InvalidArgument(format!("Invalid rtp_parameters_json: {e}"))
    })?;

    if params.codecs.is_empty() {
        return Err(SfuDomainError::InvalidArgument(
            "rtp_parameters must include at least one codec".to_string(),
        ));
    }

    Ok(params)
}

pub fn parse_rtp_capabilities(json_str: &str) -> Result<RtpCapabilities, SfuDomainError> {
    let trimmed = json_str.trim();
    if trimmed.is_empty() {
        return Err(SfuDomainError::InvalidArgument(
            "rtp_capabilities_json cannot be empty".to_string(),
        ));
    }

    let caps: RtpCapabilities = serde_json::from_str(trimmed).map_err(|e| {
        SfuDomainError::InvalidArgument(format!("Invalid rtp_capabilities_json: {e}"))
    })?;

    if caps.codecs.is_empty() {
        return Err(SfuDomainError::InvalidArgument(
            "rtp_capabilities must include at least one codec".to_string(),
        ));
    }

    Ok(caps)
}

pub fn parse_dtls_fingerprints(
    fingerprints: &[(String, String)],
) -> Result<Vec<DtlsFingerprint>, SfuDomainError> {
    if fingerprints.is_empty() {
        return Err(SfuDomainError::InvalidArgument(
            "At least one DTLS fingerprint is required".to_string(),
        ));
    }

    let mut result = Vec::with_capacity(fingerprints.len());
    for (algorithm, value) in fingerprints {
        let json_val = serde_json::json!({
            "algorithm": algorithm.to_lowercase(),
            "value": value
        });
        let fp: DtlsFingerprint = serde_json::from_value(json_val).map_err(|e| {
            SfuDomainError::InvalidArgument(format!(
                "Invalid DTLS fingerprint with algorithm '{algorithm}': {e}"
            ))
        })?;
        result.push(fp);
    }

    Ok(result)
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
    validation::validate_not_empty("router_id", &req.router_id)?;
    validation::validate_not_empty("transport_id", &req.transport_id)?;

    let dtls_role = validation::parse_dtls_role(&req.dtls_role)?;
    let remote_fingerprints: Vec<DtlsFingerprintInfo> = req
        .remote_fingerprints
        .into_iter()
        .map(|f| DtlsFingerprintInfo {
            algorithm: f.algorithm,
            value: f.value,
        })
        .collect();

    let fp_tuples: Vec<(String, String)> = remote_fingerprints
        .iter()
        .map(|fp| (fp.algorithm.clone(), fp.value.clone()))
        .collect();
    let _ = parse_dtls_fingerprints(&fp_tuples)?;

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
    validation::validate_not_empty("request_id", &req.request_id)?;
    validation::validate_not_empty("router_id", &req.router_id)?;
    validation::validate_not_empty("transport_id", &req.transport_id)?;
    validation::validate_not_empty("peer_id", &req.peer_id)?;

    let kind = validation::parse_media_kind(&req.kind)?;
    let rtp_parameters = parse_rtp_parameters(&req.rtp_parameters_json)?;

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
    validation::validate_not_empty("request_id", &req.request_id)?;
    validation::validate_not_empty("router_id", &req.router_id)?;
    validation::validate_not_empty("subscriber_transport_id", &req.subscriber_transport_id)?;
    validation::validate_not_empty("producer_id", &req.producer_id)?;

    let rtp_capabilities = parse_rtp_capabilities(&req.rtp_capabilities_json)?;

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

pub fn from_create_pipe_transport_request(
    req: proto::CreatePipeTransportRequest,
) -> CreatePipeTransportParams {
    CreatePipeTransportParams {
        request_id: req.request_id,
        router_id: req.router_id,
        enable_srtp: req.enable_srtp,
        enable_rtx: req.enable_rtx,
    }
}

pub fn to_create_pipe_transport_response(
    res: CreatePipeTransportResult,
) -> proto::CreatePipeTransportResponse {
    proto::CreatePipeTransportResponse {
        pipe_transport_id: res.pipe_transport_id,
        ip: res.ip,
        port: res.port,
        srtp_parameters_json: res.srtp_parameters_json,
    }
}

pub fn from_connect_pipe_transport_request(
    req: proto::ConnectPipeTransportRequest,
) -> Result<ConnectPipeTransportParams, SfuDomainError> {
    validation::validate_not_empty("router_id", &req.router_id)?;
    validation::validate_not_empty("pipe_transport_id", &req.pipe_transport_id)?;
    validation::validate_not_empty("remote_ip", &req.remote_ip)?;

    Ok(ConnectPipeTransportParams {
        router_id: req.router_id,
        pipe_transport_id: req.pipe_transport_id,
        remote_ip: req.remote_ip,
        remote_port: req.remote_port,
        remote_srtp_parameters_json: req.remote_srtp_parameters_json,
    })
}
