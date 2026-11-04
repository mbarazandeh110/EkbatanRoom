#[cfg(test)]
mod tests {
    use mediasoup::prelude::*;
    use std::net::IpAddr;
    use std::str::FromStr;

    #[tokio::test]
    async fn test_full_api_surface() {
        let worker_manager = WorkerManager::new();
        let mut worker_settings = WorkerSettings::default();
        worker_settings.rtc_port_range = 40000..=49999;
        let worker = worker_manager
            .create_worker(worker_settings)
            .await
            .expect("Failed to create worker");

        // Codecs
        let codecs = vec![
            RtpCodecCapability::Audio {
                mime_type: MimeTypeAudio::Opus,
                preferred_payload_type: None,
                clock_rate: std::num::NonZeroU32::new(48000).unwrap(),
                channels: std::num::NonZeroU8::new(2).unwrap(),
                parameters: RtpCodecParametersParameters::default(),
                rtcp_feedback: vec![RtcpFeedback::Nack],
            },
            RtpCodecCapability::Video {
                mime_type: MimeTypeVideo::Vp8,
                preferred_payload_type: None,
                clock_rate: std::num::NonZeroU32::new(90000).unwrap(),
                parameters: RtpCodecParametersParameters::default(),
                rtcp_feedback: vec![
                    RtcpFeedback::Nack,
                    RtcpFeedback::NackPli,
                    RtcpFeedback::CcmFir,
                    RtcpFeedback::GoogRemb,
                    RtcpFeedback::TransportCc,
                ],
            },
        ];

        let router = worker
            .create_router(RouterOptions::new(codecs))
            .await
            .expect("Failed to create router");

        let listen_info = ListenInfo {
            protocol: Protocol::Udp,
            ip: IpAddr::from_str("127.0.0.1").unwrap(),
            announced_address: Some("127.0.0.1".to_string()),
            expose_internal_ip: false,
            port: None,
            port_range: Some(40000..=49999),
            flags: None,
            send_buffer_size: None,
            recv_buffer_size: None,
        };

        let mut transport_options =
            WebRtcTransportOptions::new(WebRtcTransportListenInfos::new(listen_info));
        transport_options.enable_sctp = true;

        let transport = router
            .create_webrtc_transport(transport_options)
            .await
            .expect("Failed to create transport");

        let _tid = transport.id();
        let ice_params = transport.ice_parameters();
        let _ufrag = &ice_params.username_fragment;
        let _pwd = &ice_params.password;
        let candidates = transport.ice_candidates();
        assert!(!candidates.is_empty());
        let dtls_params = transport.dtls_parameters();
        assert!(!dtls_params.fingerprints.is_empty());

        let new_ice = transport
            .restart_ice()
            .await
            .expect("Failed to restart ICE");
        assert_ne!(new_ice.username_fragment, ice_params.username_fragment);
    }
}
