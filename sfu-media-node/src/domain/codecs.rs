use crate::config::MediasoupRouterCodecsConfig;
use mediasoup::prelude::*;
use std::num::{NonZeroU32, NonZeroU8};

const CLOCK_RATE_48K: NonZeroU32 = match NonZeroU32::new(48000) {
    Some(v) => v,
    None => unreachable!(),
};

const CLOCK_RATE_90K: NonZeroU32 = match NonZeroU32::new(90000) {
    Some(v) => v,
    None => unreachable!(),
};

const CHANNELS_OPUS: NonZeroU8 = match NonZeroU8::new(2) {
    Some(v) => v,
    None => unreachable!(),
};

/// Returns the standard media codecs supported by the SFU without any runtime unwraps
pub fn standard_media_codecs() -> Vec<RtpCodecCapability> {
    router_codecs_from_config(&MediasoupRouterCodecsConfig::default())
}

/// Builds the media codecs based on the provided configuration
pub fn router_codecs_from_config(config: &MediasoupRouterCodecsConfig) -> Vec<RtpCodecCapability> {
    let mut codecs = Vec::new();

    // Audio: Opus
    if config.enable_opus {
        codecs.push(RtpCodecCapability::Audio {
            mime_type: MimeTypeAudio::Opus,
            preferred_payload_type: None,
            clock_rate: CLOCK_RATE_48K,
            channels: CHANNELS_OPUS,
            parameters: RtpCodecParametersParameters::default(),
            rtcp_feedback: vec![RtcpFeedback::Nack, RtcpFeedback::TransportCc],
        });
    }

    // Video: VP8
    if config.enable_vp8 {
        codecs.push(RtpCodecCapability::Video {
            mime_type: MimeTypeVideo::Vp8,
            preferred_payload_type: None,
            clock_rate: CLOCK_RATE_90K,
            parameters: {
                let mut params = RtpCodecParametersParameters::default();
                if config.google_start_bitrate > 0 {
                    params.insert("x-google-start-bitrate", config.google_start_bitrate);
                }
                params
            },
            rtcp_feedback: vec![
                RtcpFeedback::Nack,
                RtcpFeedback::NackPli,
                RtcpFeedback::CcmFir,
                RtcpFeedback::GoogRemb,
                RtcpFeedback::TransportCc,
            ],
        });
    }

    // Video: VP9
    if config.enable_vp9 {
        codecs.push(RtpCodecCapability::Video {
            mime_type: MimeTypeVideo::Vp9,
            preferred_payload_type: None,
            clock_rate: CLOCK_RATE_90K,
            parameters: {
                let mut params = RtpCodecParametersParameters::default();
                params.insert("profile-id", 2u32);
                if config.google_start_bitrate > 0 {
                    params.insert("x-google-start-bitrate", config.google_start_bitrate);
                }
                params
            },
            rtcp_feedback: vec![
                RtcpFeedback::Nack,
                RtcpFeedback::NackPli,
                RtcpFeedback::CcmFir,
                RtcpFeedback::GoogRemb,
                RtcpFeedback::TransportCc,
            ],
        });
    }

    // Video: H264 (Constrained Baseline)
    if config.enable_h264 {
        codecs.push(RtpCodecCapability::Video {
            mime_type: MimeTypeVideo::H264,
            preferred_payload_type: None,
            clock_rate: CLOCK_RATE_90K,
            parameters: {
                let mut params = RtpCodecParametersParameters::default();
                params.insert("packetization-mode", 1u32);
                params.insert("level-asymmetry-allowed", 1u32);
                params.insert("profile-level-id", "42e01f");
                if config.google_start_bitrate > 0 {
                    params.insert("x-google-start-bitrate", config.google_start_bitrate);
                }
                params
            },
            rtcp_feedback: vec![
                RtcpFeedback::Nack,
                RtcpFeedback::NackPli,
                RtcpFeedback::CcmFir,
                RtcpFeedback::GoogRemb,
                RtcpFeedback::TransportCc,
            ],
        });
    }

    // Fallback safeguard: if all were disabled, keep standard opus + vp8
    if codecs.is_empty() {
        return standard_media_codecs();
    }

    codecs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_codecs() {
        let codecs = standard_media_codecs();
        assert_eq!(codecs.len(), 3);
    }

    #[test]
    fn test_router_codecs_with_vp9() {
        let config = MediasoupRouterCodecsConfig {
            enable_vp9: true,
            ..Default::default()
        };
        let codecs = router_codecs_from_config(&config);
        assert_eq!(codecs.len(), 4);
    }
}
