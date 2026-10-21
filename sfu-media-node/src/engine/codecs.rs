use mediasoup::prelude::*;
use std::num::{NonZeroU32, NonZeroU8};

pub fn standard_media_codecs() -> Vec<RtpCodecCapability> {
    vec![
        // Audio: Opus (48000Hz, 2ch)
        RtpCodecCapability::Audio {
            mime_type: MimeTypeAudio::Opus,
            preferred_payload_type: None,
            clock_rate: NonZeroU32::new(48000).unwrap(),
            channels: NonZeroU8::new(2).unwrap(),
            parameters: RtpCodecParametersParameters::default(),
            rtcp_feedback: vec![RtcpFeedback::Nack, RtcpFeedback::TransportCc],
        },
        // Video: VP8 (90000Hz)
        RtpCodecCapability::Video {
            mime_type: MimeTypeVideo::Vp8,
            preferred_payload_type: None,
            clock_rate: NonZeroU32::new(90000).unwrap(),
            parameters: RtpCodecParametersParameters::default(),
            rtcp_feedback: vec![
                RtcpFeedback::Nack,
                RtcpFeedback::NackPli,
                RtcpFeedback::CcmFir,
                RtcpFeedback::GoogRemb,
                RtcpFeedback::TransportCc,
            ],
        },
        // Video: H264 (90000Hz, packetization-mode=1, profile-level-id=42e01f)
        RtpCodecCapability::Video {
            mime_type: MimeTypeVideo::H264,
            preferred_payload_type: None,
            clock_rate: NonZeroU32::new(90000).unwrap(),
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
