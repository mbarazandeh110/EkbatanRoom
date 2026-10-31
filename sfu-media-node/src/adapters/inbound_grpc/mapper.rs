use crate::domain::entities::*;
use crate::domain::errors::SfuDomainError;
use crate::domain::validation;
use crate::ports::inbound::{
    ConnectPipeTransportParams, ConnectTransportParams, ConsumeParams, ConsumeResult,
    CreatePipeTransportParams, CreatePipeTransportResult, CreateTransportParams,
    CreateTransportResult, PipeConsumeParams, PipeProduceParams, ProduceParams, ProduceResult,
    RestartIceResult,
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
            SfuDomainError::SystemDraining(msg) => tonic::Status::unavailable(msg),
            SfuDomainError::EnterpriseFeatureRequired(msg) => tonic::Status::unimplemented(msg),
            SfuDomainError::LicensePermissionDenied(msg) => tonic::Status::permission_denied(msg),
            SfuDomainError::InvalidLicense(msg) => tonic::Status::invalid_argument(msg),
        }
    }
}

fn is_supported_header_extension_uri(uri: &str) -> bool {
    matches!(
        uri,
        "urn:ietf:params:rtp-hdrext:sdes:mid"
            | "urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id"
            | "urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id"
            | "http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time"
            | "http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01"
            | "urn:ietf:params:rtp-hdrext:ssrc-audio-level"
            | "https://aomediacodec.github.io/av1-rtp-spec/#dependency-descriptor-rtp-header-extension"
            | "urn:3gpp:video-orientation"
            | "http://www.webrtc.org/experiments/rtp-hdrext/abs-capture-time"
            | "http://www.webrtc.org/experiments/rtp-hdrext/playout-delay"
            | "urn:mediasoup:params:rtp-hdrext:packet-id"
    )
}

fn sanitize_ssrc_value(val: &serde_json::Value) -> Option<u32> {
    match val {
        serde_json::Value::Number(num) => {
            if let Some(u) = num.as_u64() {
                Some(u as u32)
            } else if let Some(i) = num.as_i64() {
                Some(i as u32)
            } else {
                num.as_f64().map(|f| f as u32)
            }
        }
        serde_json::Value::String(s) => {
            let trimmed = s.trim();
            if let Ok(u) = trimmed.parse::<u64>() {
                Some(u as u32)
            } else if let Ok(i) = trimmed.parse::<i64>() {
                Some(i as u32)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn parse_rtp_parameters(json_str: &str) -> Result<RtpParameters, SfuDomainError> {
    let trimmed = json_str.trim();
    if trimmed.is_empty() {
        return Err(SfuDomainError::InvalidArgument(
            "rtp_parameters_json cannot be empty".to_string(),
        ));
    }

    let mut root_val: serde_json::Value = serde_json::from_str(trimmed).map_err(|e| {
        SfuDomainError::InvalidArgument(format!("Invalid rtp_parameters_json: {e}"))
    })?;

    if let Some(root_obj) = root_val.as_object_mut() {
        // 1. Sanitize headerExtensions
        if let Some(exts) = root_obj
            .get_mut("headerExtensions")
            .and_then(|v| v.as_array_mut())
        {
            exts.retain_mut(|ext| {
                let Some(ext_map) = ext.as_object_mut() else {
                    return false;
                };

                // Inject default encrypt: false if absent or null
                if !ext_map.contains_key("encrypt")
                    || ext_map.get("encrypt").map(|v| v.is_null()).unwrap_or(false)
                {
                    ext_map.insert("encrypt".to_string(), serde_json::Value::Bool(false));
                }

                // Sanitize numeric id if sent as string
                if let Some(id_val) = ext_map.get("id") {
                    if let Some(id_str) = id_val.as_str() {
                        if let Ok(num) = id_str.trim().parse::<u16>() {
                            ext_map.insert("id".to_string(), serde_json::json!(num));
                        }
                    }
                }

                // Validate and filter URI
                if let Some(uri_val) = ext_map.get("uri").and_then(|u| u.as_str()) {
                    if is_supported_header_extension_uri(uri_val) {
                        true
                    } else {
                        tracing::debug!(
                            uri = %uri_val,
                            "Filtering unsupported RTP header extension URI in produce parameters"
                        );
                        false
                    }
                } else {
                    false
                }
            });
        }

        // 2. Sanitize encodings (ssrc, rtx.ssrc, codecPayloadType, maxBitrate)
        if let Some(encs) = root_obj.get_mut("encodings").and_then(|v| v.as_array_mut()) {
            for enc in encs {
                if let Some(enc_map) = enc.as_object_mut() {
                    // Sanitize primary media SSRC
                    if let Some(ssrc_val) = enc_map.get("ssrc") {
                        if let Some(cleaned_ssrc) = sanitize_ssrc_value(ssrc_val) {
                            enc_map.insert("ssrc".to_string(), serde_json::json!(cleaned_ssrc));
                        }
                    }

                    // Sanitize RTX SSRC
                    if let Some(rtx_map) = enc_map.get_mut("rtx").and_then(|r| r.as_object_mut()) {
                        if let Some(rtx_ssrc_val) = rtx_map.get("ssrc") {
                            if let Some(cleaned_rtx_ssrc) = sanitize_ssrc_value(rtx_ssrc_val) {
                                rtx_map.insert(
                                    "ssrc".to_string(),
                                    serde_json::json!(cleaned_rtx_ssrc),
                                );
                            }
                        }
                    }

                    // Sanitize codecPayloadType if sent as string
                    if let Some(cpt_val) = enc_map.get("codecPayloadType") {
                        if let Some(cpt_str) = cpt_val.as_str() {
                            if let Ok(num) = cpt_str.trim().parse::<u8>() {
                                enc_map
                                    .insert("codecPayloadType".to_string(), serde_json::json!(num));
                            }
                        }
                    }

                    // Sanitize maxBitrate if sent as string
                    if let Some(mbr_val) = enc_map.get("maxBitrate") {
                        if let Some(mbr_str) = mbr_val.as_str() {
                            if let Ok(num) = mbr_str.trim().parse::<u64>() {
                                enc_map.insert("maxBitrate".to_string(), serde_json::json!(num));
                            }
                        }
                    }
                }
            }
        }

        // 3. Sanitize codecs (payloadType, clockRate, channels, rtcpFeedback)
        if let Some(codecs) = root_obj.get_mut("codecs").and_then(|v| v.as_array_mut()) {
            for codec in codecs {
                if let Some(codec_map) = codec.as_object_mut() {
                    if let Some(pt_val) = codec_map.get("payloadType") {
                        if let Some(pt_str) = pt_val.as_str() {
                            if let Ok(num) = pt_str.trim().parse::<u8>() {
                                codec_map.insert("payloadType".to_string(), serde_json::json!(num));
                            }
                        }
                    }
                    if let Some(cr_val) = codec_map.get("clockRate") {
                        if let Some(cr_str) = cr_val.as_str() {
                            if let Ok(num) = cr_str.trim().parse::<u32>() {
                                codec_map.insert("clockRate".to_string(), serde_json::json!(num));
                            }
                        }
                    }
                    if let Some(ch_val) = codec_map.get("channels") {
                        if let Some(ch_str) = ch_val.as_str() {
                            if let Ok(num) = ch_str.trim().parse::<u8>() {
                                codec_map.insert("channels".to_string(), serde_json::json!(num));
                            }
                        }
                    }
                    if let Some(rtcp_fb) = codec_map
                        .get_mut("rtcpFeedback")
                        .and_then(|v| v.as_array_mut())
                    {
                        for fb in rtcp_fb {
                            if let Some(fb_map) = fb.as_object_mut() {
                                if !fb_map.contains_key("parameter")
                                    || fb_map
                                        .get("parameter")
                                        .map(|v| v.is_null())
                                        .unwrap_or(false)
                                {
                                    fb_map.insert("parameter".to_string(), serde_json::json!(""));
                                }
                            }
                        }
                    }
                }
            }
        }

        // 4. Sanitize rtcp (ensure reducedSize default)
        if let Some(rtcp_map) = root_obj.get_mut("rtcp").and_then(|v| v.as_object_mut()) {
            if !rtcp_map.contains_key("reducedSize")
                || rtcp_map
                    .get("reducedSize")
                    .map(|v| v.is_null())
                    .unwrap_or(false)
            {
                rtcp_map.insert("reducedSize".to_string(), serde_json::Value::Bool(true));
            }
        } else {
            root_obj.insert(
                "rtcp".to_string(),
                serde_json::json!({ "reducedSize": true }),
            );
        }
    }

    let params: RtpParameters = serde_json::from_value(root_val).map_err(|e| {
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

    let mut root_val: serde_json::Value = serde_json::from_str(trimmed).map_err(|e| {
        SfuDomainError::InvalidArgument(format!("Invalid rtp_capabilities_json: {e}"))
    })?;

    if let Some(root_obj) = root_val.as_object_mut() {
        // Sanitize headerExtensions
        if let Some(exts) = root_obj
            .get_mut("headerExtensions")
            .and_then(|v| v.as_array_mut())
        {
            exts.retain_mut(|ext| {
                let Some(ext_map) = ext.as_object_mut() else {
                    return false;
                };

                if !ext_map.contains_key("preferredEncrypt")
                    || ext_map
                        .get("preferredEncrypt")
                        .map(|v| v.is_null())
                        .unwrap_or(false)
                {
                    ext_map.insert(
                        "preferredEncrypt".to_string(),
                        serde_json::Value::Bool(false),
                    );
                }

                if !ext_map.contains_key("direction")
                    || ext_map
                        .get("direction")
                        .map(|v| v.is_null())
                        .unwrap_or(false)
                {
                    ext_map.insert("direction".to_string(), serde_json::json!("sendrecv"));
                }

                if let Some(uri_val) = ext_map.get("uri").and_then(|u| u.as_str()) {
                    if is_supported_header_extension_uri(uri_val) {
                        true
                    } else {
                        tracing::debug!(
                            uri = %uri_val,
                            "Filtering unsupported RTP header extension URI in capabilities"
                        );
                        false
                    }
                } else {
                    false
                }
            });
        }

        // Sanitize codecs
        if let Some(codecs) = root_obj.get_mut("codecs").and_then(|v| v.as_array_mut()) {
            for codec in codecs {
                if let Some(codec_map) = codec.as_object_mut() {
                    if let Some(pt_val) = codec_map.get("preferredPayloadType") {
                        if let Some(pt_str) = pt_val.as_str() {
                            if let Ok(num) = pt_str.trim().parse::<u8>() {
                                codec_map.insert(
                                    "preferredPayloadType".to_string(),
                                    serde_json::json!(num),
                                );
                            }
                        }
                    }
                    if let Some(cr_val) = codec_map.get("clockRate") {
                        if let Some(cr_str) = cr_val.as_str() {
                            if let Ok(num) = cr_str.trim().parse::<u32>() {
                                codec_map.insert("clockRate".to_string(), serde_json::json!(num));
                            }
                        }
                    }
                    if let Some(ch_val) = codec_map.get("channels") {
                        if let Some(ch_str) = ch_val.as_str() {
                            if let Ok(num) = ch_str.trim().parse::<u8>() {
                                codec_map.insert("channels".to_string(), serde_json::json!(num));
                            }
                        }
                    }
                    if let Some(rtcp_fb) = codec_map
                        .get_mut("rtcpFeedback")
                        .and_then(|v| v.as_array_mut())
                    {
                        for fb in rtcp_fb {
                            if let Some(fb_map) = fb.as_object_mut() {
                                if !fb_map.contains_key("parameter")
                                    || fb_map
                                        .get("parameter")
                                        .map(|v| v.is_null())
                                        .unwrap_or(false)
                                {
                                    fb_map.insert("parameter".to_string(), serde_json::json!(""));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let caps: RtpCapabilities = serde_json::from_value(root_val).map_err(|e| {
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
    let fp_tuples: Vec<(String, String)> = req
        .remote_fingerprints
        .into_iter()
        .map(|f| (f.algorithm, f.value))
        .collect();
    let remote_fingerprints = parse_dtls_fingerprints(&fp_tuples)?;

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

    tracing::debug!(
        room_id = %req.router_id,
        peer_id = %req.peer_id,
        kind = %req.kind,
        encodings = ?rtp_parameters.encodings,
        codecs = ?rtp_parameters.codecs,
        "deserialized Producer rtp_parameters"
    );

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

pub fn to_node_capacity_metrics(info: NodeCapacityInfo) -> proto::NodeCapacityMetrics {
    proto::NodeCapacityMetrics {
        total_capacity_weight: info.total_capacity_weight,
        total_workers: info.total_workers,
        alive_workers: info.alive_workers,
        logical_load_percent: info.logical_load_percent,
        cpu_usage_percent: info.cpu_usage_percent,
        memory_usage_percent: info.memory_usage_percent,
        max_worker_load_percent: info.max_worker_load_percent,
        effective_load_percent: info.effective_load_percent,
        active_routers: info.active_routers,
        active_transports: info.active_transports,
        active_producers: info.active_producers,
        active_consumers: info.active_consumers,
        is_draining: info.is_draining,
        enterprise_cascading_enabled: info.enterprise_cascading_enabled,
        license_tier: info.license_tier,
    }
}

pub fn from_pipe_produce_request(
    req: proto::PipeProduceRequest,
) -> Result<PipeProduceParams, SfuDomainError> {
    validation::validate_not_empty("request_id", &req.request_id)?;
    validation::validate_not_empty("router_id", &req.router_id)?;
    validation::validate_not_empty("pipe_transport_id", &req.pipe_transport_id)?;
    validation::validate_not_empty("source_producer_id", &req.source_producer_id)?;

    let kind = validation::parse_media_kind(&req.kind)?;
    let rtp_parameters = parse_rtp_parameters(&req.rtp_parameters_json)?;

    tracing::debug!(
        router_id = %req.router_id,
        pipe_transport_id = %req.pipe_transport_id,
        source_producer_id = %req.source_producer_id,
        kind = %req.kind,
        encodings = ?rtp_parameters.encodings,
        codecs = ?rtp_parameters.codecs,
        "deserialized PipeProducer rtp_parameters"
    );

    Ok(PipeProduceParams {
        request_id: req.request_id,
        router_id: req.router_id,
        pipe_transport_id: req.pipe_transport_id,
        producer_id: req.source_producer_id,
        kind,
        rtp_parameters,
        paused: req.paused,
        raw_rtp_parameters_json: req.rtp_parameters_json,
    })
}

pub fn from_pipe_consume_request(
    req: proto::PipeConsumeRequest,
) -> Result<PipeConsumeParams, SfuDomainError> {
    validation::validate_not_empty("request_id", &req.request_id)?;
    validation::validate_not_empty("router_id", &req.router_id)?;
    validation::validate_not_empty("pipe_transport_id", &req.pipe_transport_id)?;
    validation::validate_not_empty("producer_id", &req.producer_id)?;

    Ok(PipeConsumeParams {
        request_id: req.request_id,
        router_id: req.router_id,
        pipe_transport_id: req.pipe_transport_id,
        producer_id: req.producer_id,
    })
}

pub fn to_proto_node_event(event: DomainNodeEvent) -> proto::NodeEvent {
    let payload = event.payload.map(|p| match p {
        DomainNodeEventPayload::MediaScore(s) => {
            proto::node_event::Payload::MediaScore(proto::MediaScoreEvent {
                resource_id: s.resource_id,
                peer_id: s.peer_id,
                is_producer: s.is_producer,
                score: s.score,
                producer_scores: s.producer_scores,
            })
        }
        DomainNodeEventPayload::LayerChanged(l) => {
            proto::node_event::Payload::LayerChanged(proto::LayerChangedEvent {
                consumer_id: l.consumer_id,
                producer_id: l.producer_id,
                peer_id: l.peer_id,
                preferred_spatial_layer: l.preferred_spatial_layer,
                current_spatial_layer: l.current_spatial_layer,
                preferred_temporal_layer: l.preferred_temporal_layer,
                current_temporal_layer: l.current_temporal_layer,
            })
        }
        DomainNodeEventPayload::ActiveSpeaker(a) => {
            proto::node_event::Payload::ActiveSpeaker(proto::ActiveSpeakerEvent {
                producer_id: a.producer_id,
                peer_id: a.peer_id,
                volume_dbfs: a.volume_dbfs,
            })
        }
        DomainNodeEventPayload::ResourceClosed(r) => {
            let r_type = match r.resource_type {
                DomainResourceType::Router => proto::ResourceType::Router as i32,
                DomainResourceType::Transport => proto::ResourceType::Transport as i32,
                DomainResourceType::Producer => proto::ResourceType::Producer as i32,
                DomainResourceType::Consumer => proto::ResourceType::Consumer as i32,
                DomainResourceType::Unspecified => proto::ResourceType::Unspecified as i32,
            };
            proto::node_event::Payload::ResourceClosed(proto::ResourceClosedEvent {
                resource_type: r_type,
                resource_id: r.resource_id,
                reason: r.reason,
            })
        }
        DomainNodeEventPayload::NodeCapacity(info) => {
            proto::node_event::Payload::NodeCapacity(to_node_capacity_metrics(info))
        }
    });

    proto::NodeEvent {
        router_id: event.router_id,
        timestamp_ms: event.timestamp_ms,
        payload,
    }
}
