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
    vec![
        // Audio: Opus
        RtpCodecCapability::Audio {
            mime_type: MimeTypeAudio::Opus,
            preferred_payload_type: None,
            clock_rate: CLOCK_RATE_48K,
            channels: CHANNELS_OPUS,
            parameters: RtpCodecParametersParameters::default(),
            rtcp_feedback: vec![RtcpFeedback::Nack, RtcpFeedback::TransportCc],
        },
        // Video: VP8
        RtpCodecCapability::Video {
            mime_type: MimeTypeVideo::Vp8,
            preferred_payload_type: None,
            clock_rate: CLOCK_RATE_90K,
            parameters: RtpCodecParametersParameters::default(),
            rtcp_feedback: vec![
                RtcpFeedback::Nack,
                RtcpFeedback::NackPli,
                RtcpFeedback::CcmFir,
                RtcpFeedback::GoogRemb,
                RtcpFeedback::TransportCc,
            ],
        },
        // Video: H264 (Constrained Baseline)
        RtpCodecCapability::Video {
            mime_type: MimeTypeVideo::H264,
            preferred_payload_type: None,
            clock_rate: CLOCK_RATE_90K,
            parameters: {
                let mut params = RtpCodecParametersParameters::default();
                params.insert("packetization-mode", 1u32);
                params.insert("level-asymmetry-allowed", 1u32);
                params.insert("profile-level-id", "42e01f");
                params
            },
            rtcp_feedback: vec![
                RtcpFeedback::Nack,
                RtcpFeedback::NackPli,
                RtcpFeedback::CcmFir,
                RtcpFeedback::GoogRemb,
                RtcpFeedback::TransportCc,
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_codecs() {
        let codecs = standard_media_codecs();
        assert_eq!(codecs.len(), 3);
    }
}
