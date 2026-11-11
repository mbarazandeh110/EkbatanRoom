package livekit_test

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/pion/sdp/v3"

	"ekbatan-room/signaling-coordinator/internal/adapters/inbound/livekit"
	"ekbatan-room/signaling-coordinator/internal/domain"
)

const sampleClientOffer = `v=0
o=- 70837583765872365 2 IN IP4 127.0.0.1
s=-
t=0 0
a=group:BUNDLE 0 1
a=msid-semantic: WMS
m=audio 9 UDP/TLS/RTP/SAVPF 111
c=IN IP4 0.0.0.0
a=mid:0
a=sendonly
a=setup:actpass
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=rtpmap:111 opus/48000/2
a=ssrc:1001 cname:test-cname
m=video 9 UDP/TLS/RTP/SAVPF 96 97
c=IN IP4 0.0.0.0
a=mid:1
a=sendonly
a=setup:actpass
a=rtpmap:96 VP8/90000
a=rtpmap:97 rtx/90000
a=fmtp:97 apt=96
a=ssrc:2001 cname:test-cname
a=ssrc:2002 cname:test-cname
a=ssrc-group:FID 2001 2002
`

func TestSDPMapper_ParsePublisherOffer(t *testing.T) {
	parsed, err := livekit.ParsePublisherOffer(sampleClientOffer)
	if err != nil {
		t.Fatalf("ParsePublisherOffer failed: %v", err)
	}

	if parsed.DTLSRole != "client" {
		t.Fatalf("expected client DTLS role from actpass, got %s", parsed.DTLSRole)
	}
	if len(parsed.RemoteFingerprints) == 0 {
		t.Fatalf("expected remote fingerprints, got none")
	}
	if len(parsed.Tracks) != 2 {
		t.Fatalf("expected 2 tracks (audio + video), got %d", len(parsed.Tracks))
	}

	audio := parsed.Tracks[0]
	if audio.Kind != domain.TrackKindAudio || audio.SSRC != 1001 || audio.PayloadType != 111 || audio.Codec != "opus" || audio.Channels != 2 {
		t.Fatalf("unexpected audio track: %+v", audio)
	}
	if !strings.Contains(audio.RtpParametersJSON, `"channels":2`) {
		t.Fatalf("expected audio RtpParametersJSON to contain \"channels\":2, got: %s", audio.RtpParametersJSON)
	}
	if !strings.Contains(audio.RtpParametersJSON, `"mid":"0"`) {
		t.Fatalf("expected audio RtpParametersJSON to contain \"mid\":\"0\", got: %s", audio.RtpParametersJSON)
	}
	if !strings.Contains(audio.RtpParametersJSON, `"headerExtensions":[]`) {
		t.Fatalf("expected audio RtpParametersJSON to contain \"headerExtensions\":[], got: %s", audio.RtpParametersJSON)
	}
	if !strings.Contains(audio.RtpParametersJSON, `"reducedSize":true`) {
		t.Fatalf("expected audio RtpParametersJSON to contain \"reducedSize\":true, got: %s", audio.RtpParametersJSON)
	}
	if !strings.Contains(audio.RtpParametersJSON, `"cname":"test-cname"`) {
		t.Fatalf("expected audio RtpParametersJSON to contain \"cname\":\"test-cname\", got: %s", audio.RtpParametersJSON)
	}

	video := parsed.Tracks[1]
	if video.Kind != domain.TrackKindVideo || video.SSRC != 2001 || video.RTXSSRC != 2002 || video.PayloadType != 96 {
		t.Fatalf("unexpected video track: %+v", video)
	}
	if strings.Contains(video.RtpParametersJSON, "channels") {
		t.Fatalf("video RtpParametersJSON must NOT contain channels, got: %s", video.RtpParametersJSON)
	}
	if !strings.Contains(video.RtpParametersJSON, `"mid":"1"`) {
		t.Fatalf("expected video RtpParametersJSON to contain \"mid\":\"1\", got: %s", video.RtpParametersJSON)
	}
	if !strings.Contains(video.RtpParametersJSON, `"headerExtensions":[]`) {
		t.Fatalf("expected video RtpParametersJSON to contain \"headerExtensions\":[], got: %s", video.RtpParametersJSON)
	}
	if !strings.Contains(video.RtpParametersJSON, `"reducedSize":true`) {
		t.Fatalf("expected video RtpParametersJSON to contain \"reducedSize\":true, got: %s", video.RtpParametersJSON)
	}
	// REQ-019: RTX codec registration
	if !strings.Contains(video.RtpParametersJSON, `"mimeType":"video/rtx"`) {
		t.Fatalf("expected video RtpParametersJSON to register RTX codec, got: %s", video.RtpParametersJSON)
	}
	if !strings.Contains(video.RtpParametersJSON, `"payloadType":97`) {
		t.Fatalf("expected video RtpParametersJSON to contain RTX payloadType 97, got: %s", video.RtpParametersJSON)
	}
	if !strings.Contains(video.RtpParametersJSON, `"apt":96`) {
		t.Fatalf("expected video RtpParametersJSON to contain apt:96, got: %s", video.RtpParametersJSON)
	}

	// REQ-020: Standard video RTCP feedback fallback
	if !strings.Contains(video.RtpParametersJSON, `"rtcpFeedback"`) {
		t.Fatalf("expected video RtpParametersJSON to contain rtcpFeedback, got: %s", video.RtpParametersJSON)
	}
	if !strings.Contains(video.RtpParametersJSON, `"parameter":"pli"`) {
		t.Fatalf("expected video RtpParametersJSON to contain PLI feedback, got: %s", video.RtpParametersJSON)
	}
	if !strings.Contains(video.RtpParametersJSON, `"parameter":"fir"`) {
		t.Fatalf("expected video RtpParametersJSON to contain FIR feedback, got: %s", video.RtpParametersJSON)
	}
}

func TestSDPMapper_ParsePublisherOffer_ExplicitVideoRtcpFeedback(t *testing.T) {
	sdpWithRtcpFb := `v=0
o=- 12345 2 IN IP4 127.0.0.1
s=-
t=0 0
m=video 9 UDP/TLS/RTP/SAVPF 96
c=IN IP4 0.0.0.0
a=mid:video-0
a=sendonly
a=setup:actpass
a=rtpmap:96 VP8/90000
a=rtcp-fb:96 nack
a=rtcp-fb:96 nack pli
a=rtcp-fb:96 ccm fir
a=rtcp-fb:96 goog-remb
a=ssrc:5555 cname:explicit-fb
`
	parsed, err := livekit.ParsePublisherOffer(sdpWithRtcpFb)
	if err != nil {
		t.Fatalf("ParsePublisherOffer failed: %v", err)
	}
	if len(parsed.Tracks) != 1 {
		t.Fatalf("expected 1 track, got %d", len(parsed.Tracks))
	}
	video := parsed.Tracks[0]
	if !strings.Contains(video.RtpParametersJSON, `"parameter":"pli"`) {
		t.Fatalf("expected parsed PLI feedback, got: %s", video.RtpParametersJSON)
	}
	if !strings.Contains(video.RtpParametersJSON, `"parameter":"fir"`) {
		t.Fatalf("expected parsed FIR feedback, got: %s", video.RtpParametersJSON)
	}
	if !strings.Contains(video.RtpParametersJSON, `"type":"goog-remb"`) {
		t.Fatalf("expected parsed goog-remb feedback, got: %s", video.RtpParametersJSON)
	}
}

func TestSDPMapper_GeneratePublisherAnswer_ActiveDTLSRole_REQ005(t *testing.T) {
	transport := &domain.TransportInfo{
		TransportID: "pub-transport",
		IceUfrag:    "server-ufrag",
		IcePwd:      "server-pwd",
		DtlsFingerprints: []domain.DtlsFingerprintInfo{
			{Algorithm: "sha-256", Value: "FF:EE:DD:CC:BB:AA"},
		},
		IceCandidates: []domain.IceCandidateInfo{
			{SDPCandidateLine: "candidate:1 1 udp 2130706431 127.0.0.1 50000 typ host"},
		},
	}

	answer, err := livekit.GeneratePublisherAnswer(sampleClientOffer, transport)
	if err != nil {
		t.Fatalf("GeneratePublisherAnswer failed: %v", err)
	}

	// REQ-005: Must contain a=setup:active at session and media levels to prevent DTLS deadlock
	if !strings.Contains(answer, "a=setup:active") {
		t.Fatalf("expected a=setup:active in answer, got:\n%s", answer)
	}

	var ans sdp.SessionDescription
	if err := ans.Unmarshal([]byte(answer)); err != nil {
		t.Fatalf("failed to unmarshal publisher answer: %v", err)
	}

	// Verify session level setup:active
	sessionSetup, ok := ans.Attribute("setup")
	if !ok || sessionSetup != "active" {
		t.Fatalf("expected session-level a=setup:active, got %s (found: %v)", sessionSetup, ok)
	}

	// Verify media-level setup:active in every media section (audio, video)
	if len(ans.MediaDescriptions) == 0 {
		t.Fatalf("expected media descriptions in answer, got 0")
	}
	for i, md := range ans.MediaDescriptions {
		mediaSetup, ok := md.Attribute("setup")
		if !ok || mediaSetup != "active" {
			t.Fatalf("expected media[%d] (%s) to have a=setup:active, got %s (found: %v)", i, md.MediaName.Media, mediaSetup, ok)
		}
	}

	// REQ-017: ice-lite attribute
	if !strings.Contains(answer, "a=ice-lite") {
		t.Fatalf("expected a=ice-lite in answer for Mediasoup compatibility, got:\n%s", answer)
	}
	if !strings.Contains(answer, "a=ice-ufrag:server-ufrag") {
		t.Fatalf("expected server ice-ufrag, got:\n%s", answer)
	}
	if !strings.Contains(answer, "a=recvonly") {
		t.Fatalf("expected a=recvonly in answer")
	}
}

func TestSDPMapper_GeneratePublisherAnswer_SimulcastNegotiation_REQ002(t *testing.T) {
	offerWithSimulcast := `v=0
o=- 12345 2 IN IP4 127.0.0.1
s=-
t=0 0
m=video 9 UDP/TLS/RTP/SAVPF 96
c=IN IP4 0.0.0.0
a=mid:video-0
a=sendonly
a=setup:actpass
a=rtpmap:96 VP8/90000
a=rtcp-fb:96 nack
a=rtcp-fb:96 nack pli
a=rtcp-fb:96 ccm fir
a=rid:q send
a=rid:h send
a=rid:f send
a=simulcast:send q;h;f
`
	transport := &domain.TransportInfo{
		TransportID: "pub-simulcast",
		IceUfrag:    "ufrag-sim",
		IcePwd:      "pwd-sim",
		DtlsFingerprints: []domain.DtlsFingerprintInfo{
			{Algorithm: "sha-256", Value: "11:22:33:44"},
		},
	}

	answer, err := livekit.GeneratePublisherAnswer(offerWithSimulcast, transport)
	if err != nil {
		t.Fatalf("GeneratePublisherAnswer failed: %v", err)
	}

	// Verify that simulcast attributes are negotiated to recv per RFC 8853 / REQ-002
	if !strings.Contains(answer, "a=simulcast:recv q;h;f") {
		t.Fatalf("expected a=simulcast:recv q;h;f in answer per REQ-002, got:\n%s", answer)
	}
	if !strings.Contains(answer, "a=rid:q recv") {
		t.Fatalf("expected a=rid:q recv in answer, got:\n%s", answer)
	}
	if !strings.Contains(answer, "a=rid:h recv") {
		t.Fatalf("expected a=rid:h recv in answer, got:\n%s", answer)
	}
	if !strings.Contains(answer, "a=rid:f recv") {
		t.Fatalf("expected a=rid:f recv in answer, got:\n%s", answer)
	}
	if strings.Contains(answer, "a=simulcast:send") {
		t.Fatalf("unexpected a=simulcast:send in answer (should be recv)")
	}
	if strings.Contains(answer, "a=rid:q send") {
		t.Fatalf("unexpected a=rid:q send in answer (should be recv)")
	}
}

func TestSDPMapper_GenerateSubscriberOffer_ActiveAndInactiveSlots(t *testing.T) {
	transport := &domain.TransportInfo{
		TransportID: "sub-transport",
		IceUfrag:    "sub-ufrag",
		IcePwd:      "sub-pwd",
		DtlsFingerprints: []domain.DtlsFingerprintInfo{
			{Algorithm: "sha-256", Value: "AA:BB:CC"},
		},
	}

	slots := []*domain.SDPSlot{
		{
			Index:       0,
			MID:         "0",
			Kind:        domain.TrackKindAudio,
			Active:      true,
			TrackSID:    "TR_audio_1",
			ProducerID:  "prod_audio",
			ConsumerID:  "cons_audio",
			SSRC:        50001,
			PayloadType: 111,
			ClockRate:   48000,
			MimeType:    "opus",
		},
		{
			Index:       1,
			MID:         "1",
			Kind:        domain.TrackKindVideo,
			Active:      false, // Inactivated slot for recycling
			PayloadType: 96,
		},
	}

	offer, err := livekit.GenerateSubscriberOffer(transport, slots, "PA_alice")
	if err != nil {
		t.Fatalf("GenerateSubscriberOffer failed: %v", err)
	}

	// Offer must contain a=setup:actpass for subscriber
	if !strings.Contains(offer, "a=setup:actpass") {
		t.Fatalf("expected a=setup:actpass in subscriber offer")
	}
	// REQ-017: ice-lite attribute in subscriber offer for Mediasoup compatibility
	if !strings.Contains(offer, "a=ice-lite") {
		t.Fatalf("expected a=ice-lite in subscriber offer, got:\n%s", offer)
	}

	// REQ-018: RFC 8843 BUNDLE compliance: only active slots are in BUNDLE
	if !strings.Contains(offer, "a=group:BUNDLE 0") || strings.Contains(offer, "a=group:BUNDLE 0 1") {
		t.Fatalf("expected BUNDLE to only contain active slot 0, got:\n%s", offer)
	}

	// Active slot 0 must have port 9, sendonly, and msid
	if !strings.Contains(offer, "a=mid:0") || !strings.Contains(offer, "a=sendonly") {
		t.Fatalf("expected active mid:0 sendonly")
	}
	if !strings.Contains(offer, "a=msid:PA_alice TR_audio_1") {
		t.Fatalf("expected msid for active track")
	}

	// REQ-001 (rtc-load-suite): Subscriber offer must NOT contain explicit a=ssrc or a=ssrc-group
	// to conform with Unified Plan and prevent Pion WebRTC / livekit-cli from dropping incoming packets.
	if strings.Contains(offer, "a=ssrc") {
		t.Fatalf("subscriber offer must NOT contain explicit 'a=ssrc' attributes, got:\n%s", offer)
	}
	if strings.Contains(offer, "a=ssrc-group") {
		t.Fatalf("subscriber offer must NOT contain explicit 'a=ssrc-group' attributes, got:\n%s", offer)
	}

	// Inactive slot 1 must have port 0 and a=inactive
	if !strings.Contains(offer, "a=mid:1") || !strings.Contains(offer, "a=inactive") || !strings.Contains(offer, "m=video 0 ") {
		t.Fatalf("expected inactive video slot with port 0, got:\n%s", offer)
	}

	// RFC 4566 / RFC 7587: a=rtpmap:111 opus/48000/2 and fmtp
	if !strings.Contains(offer, "a=rtpmap:111 opus/48000/2") {
		t.Fatalf("expected a=rtpmap:111 opus/48000/2 in offer, got:\n%s", offer)
	}
	if !strings.Contains(offer, "a=fmtp:111 minptime=10;useinbandfec=1") {
		t.Fatalf("expected a=fmtp:111 minptime=10;useinbandfec=1 in offer, got:\n%s", offer)
	}

	// REQ-018: If all slots are inactive, no BUNDLE attribute should be emitted
	allInactiveSlots := []*domain.SDPSlot{
		{
			Index:       0,
			MID:         "0",
			Kind:        domain.TrackKindAudio,
			Active:      false,
			PayloadType: 111,
		},
	}
	offerAllInactive, err := livekit.GenerateSubscriberOffer(transport, allInactiveSlots, "PA_alice")
	if err != nil {
		t.Fatalf("GenerateSubscriberOffer failed for all inactive slots: %v", err)
	}
	if strings.Contains(offerAllInactive, "a=group:BUNDLE") {
		t.Fatalf("expected no BUNDLE attribute when all slots are inactive, got:\n%s", offerAllInactive)
	}
}

func TestSDPMapper_GenerateSubscriberOffer_RFC4566MimeTypeStrippingAndVideoFeedback(t *testing.T) {
	transport := &domain.TransportInfo{
		TransportID: "sub-transport-rfc",
		IceUfrag:    "sub-ufrag",
		IcePwd:      "sub-pwd",
		DtlsFingerprints: []domain.DtlsFingerprintInfo{
			{Algorithm: "sha-256", Value: "AA:BB:CC"},
		},
	}

	// Mediasoup returns MimeType with media prefix: "audio/opus" and "video/VP8"
	slots := []*domain.SDPSlot{
		{
			Index:       0,
			MID:         "0",
			Kind:        domain.TrackKindAudio,
			Active:      true,
			TrackSID:    "TR_aud",
			SSRC:        60001,
			PayloadType: 100,
			ClockRate:   48000,
			MimeType:    "audio/opus",
		},
		{
			Index:       1,
			MID:         "1",
			Kind:        domain.TrackKindVideo,
			Active:      true,
			TrackSID:    "TR_vid",
			SSRC:        60002,
			PayloadType: 101,
			ClockRate:   90000,
			MimeType:    "video/VP8",
		},
	}

	offer, err := livekit.GenerateSubscriberOffer(transport, slots, "PA_bob")
	if err != nil {
		t.Fatalf("GenerateSubscriberOffer failed: %v", err)
	}

	// Must NOT contain media prefix in rtpmap
	if strings.Contains(offer, "audio/opus") {
		t.Fatalf("subscriber offer rtpmap must NOT contain media prefix 'audio/opus', got:\n%s", offer)
	}
	if strings.Contains(offer, "video/VP8") {
		t.Fatalf("subscriber offer rtpmap must NOT contain media prefix 'video/VP8', got:\n%s", offer)
	}

	// Must contain pure subtype rtpmap
	if !strings.Contains(offer, "a=rtpmap:100 opus/48000/2") {
		t.Fatalf("expected 'a=rtpmap:100 opus/48000/2', got:\n%s", offer)
	}
	if !strings.Contains(offer, "a=fmtp:100 minptime=10;useinbandfec=1") {
		t.Fatalf("expected 'a=fmtp:100 minptime=10;useinbandfec=1', got:\n%s", offer)
	}
	if !strings.Contains(offer, "a=rtpmap:101 VP8/90000") {
		t.Fatalf("expected 'a=rtpmap:101 VP8/90000', got:\n%s", offer)
	}

	// Video rtcp-fb feedback attributes
	if !strings.Contains(offer, "a=rtcp-fb:101 goog-remb") ||
		!strings.Contains(offer, "a=rtcp-fb:101 transport-cc") ||
		!strings.Contains(offer, "a=rtcp-fb:101 nack pli") {
		t.Fatalf("expected video rtcp-fb lines in offer, got:\n%s", offer)
	}
}

func TestSDPMapper_GenerateSubscriberOffer_NoExplicitSSRC_REQ001(t *testing.T) {
	transport := &domain.TransportInfo{
		TransportID: "sub-transport-req001",
		IceUfrag:    "test-ufrag",
		IcePwd:      "test-pwd",
		DtlsFingerprints: []domain.DtlsFingerprintInfo{
			{Algorithm: "sha-256", Value: "11:22:33:44:55"},
		},
		IceCandidates: []domain.IceCandidateInfo{
			{SDPCandidateLine: "candidate:1 1 udp 2130706431 127.0.0.1 50000 typ host"},
		},
	}

	slots := []*domain.SDPSlot{
		{
			Index:        0,
			MID:          "0",
			Kind:         domain.TrackKindAudio,
			Active:       true,
			TrackSID:     "TR_audio_abc",
			ProducerID:   "prod_audio",
			ConsumerID:   "cons_audio",
			SSRC:         11111,
			PayloadType:  111,
			ClockRate:    48000,
			MimeType:     "audio/opus",
			PublisherSID: "PA_pub_1",
		},
		{
			Index:          1,
			MID:            "1",
			Kind:           domain.TrackKindVideo,
			Active:         true,
			TrackSID:       "TR_video_xyz",
			ProducerID:     "prod_video",
			ConsumerID:     "cons_video",
			SSRC:           22222,
			RTXSSRC:        33333,
			PayloadType:    96,
			RTXPayloadType: 97,
			ClockRate:      90000,
			MimeType:       "video/VP8",
			PublisherSID:   "PA_pub_1",
		},
	}

	offer, err := livekit.GenerateSubscriberOffer(transport, slots, "PA_sub_1")
	if err != nil {
		t.Fatalf("GenerateSubscriberOffer failed: %v", err)
	}

	// Must NOT contain a=ssrc or a=ssrc-group per REQ-001
	if strings.Contains(offer, "a=ssrc") {
		t.Fatalf("offer MUST NOT contain explicit a=ssrc, got:\n%s", offer)
	}
	if strings.Contains(offer, "a=ssrc-group") {
		t.Fatalf("offer MUST NOT contain explicit a=ssrc-group, got:\n%s", offer)
	}

	// Must contain standard Unified Plan MID and msid
	if !strings.Contains(offer, "a=mid:0") || !strings.Contains(offer, "a=mid:1") {
		t.Fatalf("offer must contain a=mid for Unified Plan demultiplexing, got:\n%s", offer)
	}
	if !strings.Contains(offer, "a=msid:PA_pub_1 TR_audio_abc") {
		t.Fatalf("offer must contain audio msid, got:\n%s", offer)
	}
	if !strings.Contains(offer, "a=msid:PA_pub_1 TR_video_xyz") {
		t.Fatalf("offer must contain video msid, got:\n%s", offer)
	}
	if !strings.Contains(offer, "a=group:BUNDLE 0 1") {
		t.Fatalf("offer must BUNDLE all active slots, got:\n%s", offer)
	}
}

func TestSDPMapper_ApplicationDataChannelOffer(t *testing.T) {
	dataChannelOffer := `v=0
o=- 70837583765872365 2 IN IP4 127.0.0.1
s=-
t=0 0
a=group:BUNDLE 0
m=application 9 UDP/DTLS/SCTP webrtc-datachannel
c=IN IP4 0.0.0.0
a=mid:0
a=setup:actpass
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=sctp-port:5000
a=max-message-size:262144
`
	parsed, err := livekit.ParsePublisherOffer(dataChannelOffer)
	if err != nil {
		t.Fatalf("ParsePublisherOffer failed for data channel offer: %v", err)
	}

	if parsed.DTLSRole != "client" {
		t.Fatalf("expected client DTLS role from actpass in application m-line, got %s", parsed.DTLSRole)
	}
	if len(parsed.RemoteFingerprints) != 1 {
		t.Fatalf("expected 1 remote fingerprint from application m-line, got %d", len(parsed.RemoteFingerprints))
	}
	if parsed.RemoteFingerprints[0].Algorithm != "sha-256" {
		t.Fatalf("unexpected algorithm: %s", parsed.RemoteFingerprints[0].Algorithm)
	}

	transport := &domain.TransportInfo{
		TransportID: "pub-transport",
		IceUfrag:    "ufrag-test",
		IcePwd:      "pwd-test",
		DtlsFingerprints: []domain.DtlsFingerprintInfo{
			{Algorithm: "sha-256", Value: "00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF"},
		},
	}

	answer, err := livekit.GeneratePublisherAnswer(dataChannelOffer, transport)
	if err != nil {
		t.Fatalf("GeneratePublisherAnswer failed: %v", err)
	}

	if !strings.Contains(answer, "m=application 9 UDP/DTLS/SCTP webrtc-datachannel") {
		t.Fatalf("expected preserved UDP/DTLS/SCTP protos in application answer, got:\n%s", answer)
	}
	if strings.Contains(answer, "recvonly") || strings.Contains(answer, "rtcp-mux") {
		t.Fatalf("application answer must not contain RTP recvonly or rtcp-mux attributes, got:\n%s", answer)
	}
	if !strings.Contains(answer, "a=sctp-port:5000") {
		t.Fatalf("expected sctp-port retained in answer, got:\n%s", answer)
	}
}

func TestSDPMapper_SingleFingerprintPerLevelRFC8122(t *testing.T) {
	// Simulate sfu-media-node providing multiple DTLS fingerprints
	transport := &domain.TransportInfo{
		TransportID: "multi-fp-transport",
		IceUfrag:    "ufrag-test",
		IcePwd:      "pwd-test",
		DtlsFingerprints: []domain.DtlsFingerprintInfo{
			{Algorithm: "sha-224", Value: "11:22:33"},
			{Algorithm: "sha-256", Value: "AA:BB:CC:DD"},
			{Algorithm: "sha-384", Value: "44:55:66"},
			{Algorithm: "sha-512", Value: "77:88:99"},
		},
	}

	// 1. Publisher Answer check
	answer, err := livekit.GeneratePublisherAnswer(sampleClientOffer, transport)
	if err != nil {
		t.Fatalf("GeneratePublisherAnswer failed: %v", err)
	}

	// RFC 8122: Must have at most ONE fingerprint at session level
	fpCount := strings.Count(answer, "a=fingerprint:")
	if fpCount != 1 {
		t.Fatalf("expected exactly 1 a=fingerprint line per RFC 8122, got %d in answer:\n%s", fpCount, answer)
	}
	if !strings.Contains(answer, "a=fingerprint:sha-256 AA:BB:CC:DD") {
		t.Fatalf("expected sha-256 fingerprint prioritized, got:\n%s", answer)
	}

	// 2. Subscriber Offer check
	slots := []*domain.SDPSlot{
		{
			Index:       0,
			MID:         "0",
			Kind:        domain.TrackKindAudio,
			Active:      true,
			PayloadType: 111,
			MimeType:    "opus",
			ClockRate:   48000,
			SSRC:        12345,
			TrackSID:    "TR_123",
		},
	}
	offer, err := livekit.GenerateSubscriberOffer(transport, slots, "PA_alice")
	if err != nil {
		t.Fatalf("GenerateSubscriberOffer failed: %v", err)
	}

	offerFpCount := strings.Count(offer, "a=fingerprint:")
	if offerFpCount != 1 {
		t.Fatalf("expected exactly 1 a=fingerprint line per RFC 8122, got %d in offer:\n%s", offerFpCount, offer)
	}
	if !strings.Contains(offer, "a=fingerprint:sha-256 AA:BB:CC:DD") {
		t.Fatalf("expected sha-256 fingerprint prioritized in offer, got:\n%s", offer)
	}
}

func TestSDPMapper_AudioChannelsDefaultFallback(t *testing.T) {
	offerWithoutChannels := `v=0
o=- 70837583765872365 2 IN IP4 127.0.0.1
s=-
t=0 0
m=audio 9 UDP/TLS/RTP/SAVPF 111
c=IN IP4 0.0.0.0
a=mid:audio-mid
a=sendonly
a=setup:actpass
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=rtpmap:111 opus/48000
a=ssrc:5555 cname:test-cname
`
	parsed, err := livekit.ParsePublisherOffer(offerWithoutChannels)
	if err != nil {
		t.Fatalf("ParsePublisherOffer failed: %v", err)
	}

	if len(parsed.Tracks) != 1 {
		t.Fatalf("expected 1 track, got %d", len(parsed.Tracks))
	}

	audio := parsed.Tracks[0]
	if audio.Channels != 2 {
		t.Fatalf("expected default 2 channels for audio when not specified, got %d", audio.Channels)
	}
	if !strings.Contains(audio.RtpParametersJSON, `"channels":2`) {
		t.Fatalf("expected RtpParametersJSON to contain \"channels\":2, got: %s", audio.RtpParametersJSON)
	}
	if !strings.Contains(audio.RtpParametersJSON, `"mid":"audio-mid"`) {
		t.Fatalf("expected RtpParametersJSON to contain \"mid\":\"audio-mid\", got: %s", audio.RtpParametersJSON)
	}
	if !strings.Contains(audio.RtpParametersJSON, `"headerExtensions":[]`) {
		t.Fatalf("expected RtpParametersJSON to contain \"headerExtensions\":[], got: %s", audio.RtpParametersJSON)
	}
	if !strings.Contains(audio.RtpParametersJSON, `"reducedSize":true`) {
		t.Fatalf("expected RtpParametersJSON to contain \"reducedSize\":true, got: %s", audio.RtpParametersJSON)
	}
	if !strings.Contains(audio.RtpParametersJSON, `"cname":"test-cname"`) {
		t.Fatalf("expected RtpParametersJSON to contain \"cname\":\"test-cname\", got: %s", audio.RtpParametersJSON)
	}
}

func TestSDPMapper_PreservePrimaryMediaSSRC_REQ021(t *testing.T) {
	offerWithFID := `v=0
o=- 70837583765872365 2 IN IP4 127.0.0.1
s=-
t=0 0
m=video 9 UDP/TLS/RTP/SAVPF 96 97
c=IN IP4 0.0.0.0
a=mid:video-0
a=sendonly
a=setup:actpass
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=rtpmap:96 VP8/90000
a=rtpmap:97 rtx/90000
a=fmtp:97 apt=96
a=ssrc-group:FID 814331902 3655270929
a=ssrc:814331902 cname:BOx/v2pfypsuv0kf
a=ssrc:814331902 msid:4da05d30-0011-4571-86af-1f2f009d6bfd fdacb0d6-9dc0-4d87-b0e5-a58aca45b299
a=ssrc:3655270929 cname:BOx/v2pfypsuv0kf
a=ssrc:3655270929 msid:4da05d30-0011-4571-86af-1f2f009d6bfd fdacb0d6-9dc0-4d87-b0e5-a58aca45b299
`

	parsed, err := livekit.ParsePublisherOffer(offerWithFID)
	if err != nil {
		t.Fatalf("ParsePublisherOffer failed: %v", err)
	}

	if len(parsed.Tracks) != 1 {
		t.Fatalf("expected 1 video track, got %d", len(parsed.Tracks))
	}

	track := parsed.Tracks[0]
	if track.Kind != domain.TrackKindVideo {
		t.Fatalf("expected video track, got %s", track.Kind)
	}

	// SSRC must be the primary media SSRC (814331902), NOT the RTX SSRC (3655270929)
	if track.SSRC != 814331902 {
		t.Fatalf("expected primary media SSRC 814331902, got %d", track.SSRC)
	}
	if track.RTXSSRC != 3655270929 {
		t.Fatalf("expected RTX SSRC 3655270929, got %d", track.RTXSSRC)
	}
	if track.CName != "BOx/v2pfypsuv0kf" {
		t.Fatalf("expected CName BOx/v2pfypsuv0kf, got %s", track.CName)
	}

	// Verify encodings in RtpParametersJSON
	expectedMediaSSRCJSON := `"ssrc":814331902`
	expectedRtxSSRCJSON := `"rtx":{"ssrc":3655270929}`
	if !strings.Contains(track.RtpParametersJSON, expectedMediaSSRCJSON) {
		t.Fatalf("expected RtpParametersJSON to contain %s, got: %s", expectedMediaSSRCJSON, track.RtpParametersJSON)
	}
	if !strings.Contains(track.RtpParametersJSON, expectedRtxSSRCJSON) {
		t.Fatalf("expected RtpParametersJSON to contain %s, got: %s", expectedRtxSSRCJSON, track.RtpParametersJSON)
	}
}

func TestSDPMapper_PreservePrimaryMediaSSRC_OrderIndependent_REQ022(t *testing.T) {
	// SDP matching Chromium livekit-client where individual a=ssrc lines precede a=ssrc-group:FID
	offerWithFIDTrailing := `v=0
o=- 2045496112200814096 4 IN IP4 127.0.0.1
s=-
t=0 0
m=video 9 UDP/TLS/RTP/SAVPF 96 97
c=IN IP4 0.0.0.0
a=mid:video-0
a=sendonly
a=setup:actpass
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=rtpmap:96 VP8/90000
a=rtpmap:97 rtx/90000
a=fmtp:97 apt=96
a=ssrc:1524406117 cname:YgDESFDBW4n2R3kX
a=ssrc:1524406117 msid:baee7bae-2ce2-400a-91d7-920e4889cc9e adb5c000-4718-4ca9-9722-b46e96c85b64
a=ssrc:1988226342 cname:YgDESFDBW4n2R3kX
a=ssrc:1988226342 msid:baee7bae-2ce2-400a-91d7-920e4889cc9e adb5c000-4718-4ca9-9722-b46e96c85b64
a=ssrc-group:FID 1524406117 1988226342
`

	parsed, err := livekit.ParsePublisherOffer(offerWithFIDTrailing)
	if err != nil {
		t.Fatalf("ParsePublisherOffer failed: %v", err)
	}

	if len(parsed.Tracks) != 1 {
		t.Fatalf("expected 1 video track, got %d", len(parsed.Tracks))
	}

	track := parsed.Tracks[0]
	if track.Kind != domain.TrackKindVideo {
		t.Fatalf("expected video track, got %s", track.Kind)
	}

	// Must be primary media SSRC (1524406117), NOT RTX SSRC (1988226342) even when a=ssrc lines precede a=ssrc-group:FID
	if track.SSRC != 1524406117 {
		t.Fatalf("expected primary media SSRC 1524406117, got %d", track.SSRC)
	}
	if track.RTXSSRC != 1988226342 {
		t.Fatalf("expected RTX SSRC 1988226342, got %d", track.RTXSSRC)
	}
	if track.CName != "YgDESFDBW4n2R3kX" {
		t.Fatalf("expected CName YgDESFDBW4n2R3kX, got %s", track.CName)
	}

	// Verify encodings in RtpParametersJSON
	expectedMediaSSRCJSON := `"ssrc":1524406117`
	expectedRtxSSRCJSON := `"rtx":{"ssrc":1988226342}`
	if !strings.Contains(track.RtpParametersJSON, expectedMediaSSRCJSON) {
		t.Fatalf("expected RtpParametersJSON to contain %s, got: %s", expectedMediaSSRCJSON, track.RtpParametersJSON)
	}
	if !strings.Contains(track.RtpParametersJSON, expectedRtxSSRCJSON) {
		t.Fatalf("expected RtpParametersJSON to contain %s, got: %s", expectedRtxSSRCJSON, track.RtpParametersJSON)
	}
	if track.CID != "adb5c000-4718-4ca9-9722-b46e96c85b64" {
		t.Fatalf("expected CID adb5c000-4718-4ca9-9722-b46e96c85b64, got %s", track.CID)
	}
}

func TestSDPMapper_H264_FMTP_Parameters_Parsed(t *testing.T) {
	sdpH264 := `v=0
o=- 2045496112200814096 2 IN IP4 127.0.0.1
s=-
t=0 0
m=video 9 UDP/TLS/RTP/SAVPF 125 126
c=IN IP4 0.0.0.0
a=mid:video-h264
a=sendonly
a=setup:actpass
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=rtpmap:125 H264/90000
a=fmtp:125 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f
a=rtpmap:126 rtx/90000
a=fmtp:126 apt=125
a=msid:stream-id-123 track-cid-456
a=ssrc:333444555 cname:cname-test-h264
`

	parsed, err := livekit.ParsePublisherOffer(sdpH264)
	if err != nil {
		t.Fatalf("ParsePublisherOffer failed: %v", err)
	}

	if len(parsed.Tracks) != 1 {
		t.Fatalf("expected 1 track, got %d", len(parsed.Tracks))
	}

	track := parsed.Tracks[0]
	if track.CID != "track-cid-456" {
		t.Fatalf("expected track CID track-cid-456, got %s", track.CID)
	}
	if track.CName != "cname-test-h264" {
		t.Fatalf("expected CName cname-test-h264, got %s", track.CName)
	}

	if !strings.Contains(track.RtpParametersJSON, `"profile-level-id":"42e01f"`) {
		t.Fatalf("expected profile-level-id 42e01f in RtpParametersJSON, got: %s", track.RtpParametersJSON)
	}
	if !strings.Contains(track.RtpParametersJSON, `"packetization-mode":1`) {
		t.Fatalf("expected packetization-mode 1 in RtpParametersJSON, got: %s", track.RtpParametersJSON)
	}
	if !strings.Contains(track.RtpParametersJSON, `"level-asymmetry-allowed":1`) {
		t.Fatalf("expected level-asymmetry-allowed 1 in RtpParametersJSON, got: %s", track.RtpParametersJSON)
	}
}

func TestSDPMapper_ParsePublisherOffer_RtxAptAndHeaderExtensions_REQ023(t *testing.T) {
	// Chromium SDP offer with multiple RTX codecs (VP8 apt=96 and RED apt=102) and header extensions
	chromiumOffer := `v=0
o=- 70837583765872365 2 IN IP4 127.0.0.1
s=-
t=0 0
a=group:BUNDLE video-0
m=video 9 UDP/TLS/RTP/SAVPF 96 97 102 103
c=IN IP4 0.0.0.0
a=mid:video-0
a=sendonly
a=setup:actpass
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=rtpmap:96 VP8/90000
a=rtpmap:97 rtx/90000
a=fmtp:97 apt=96
a=rtpmap:102 red/90000
a=rtpmap:103 rtx/90000
a=fmtp:103 apt=102
a=extmap:4 urn:ietf:params:rtp-hdrext:sdes:mid
a=extmap:2 http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time
a=extmap:3 http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01
a=ssrc:918035962 cname:1PoCmU0GQ07gag4o
a=ssrc:3504424756 cname:1PoCmU0GQ07gag4o
a=ssrc-group:FID 918035962 3504424756
`

	parsed, err := livekit.ParsePublisherOffer(chromiumOffer)
	if err != nil {
		t.Fatalf("ParsePublisherOffer failed: %v", err)
	}

	if len(parsed.Tracks) != 1 {
		t.Fatalf("expected 1 track, got %d", len(parsed.Tracks))
	}

	track := parsed.Tracks[0]
	if track.Kind != domain.TrackKindVideo {
		t.Fatalf("expected video track, got %s", track.Kind)
	}

	// SSRC and RTX SSRC
	if track.SSRC != 918035962 {
		t.Fatalf("expected primary media SSRC 918035962, got %d", track.SSRC)
	}
	if track.RTXSSRC != 3504424756 {
		t.Fatalf("expected RTX SSRC 3504424756, got %d", track.RTXSSRC)
	}

	// Primary codec and RTX matching
	if track.PayloadType != 96 {
		t.Fatalf("expected primary payloadType 96, got %d", track.PayloadType)
	}
	if track.RTXPayloadType != 97 {
		t.Fatalf("expected RTX payloadType 97 (matching apt=96), got %d (must NOT be 103 from secondary RED codec)", track.RTXPayloadType)
	}

	// Verify RtpParametersJSON structure
	var rtpParams struct {
		Codecs []struct {
			MimeType    string                 `json:"mimeType"`
			PayloadType uint32                 `json:"payloadType"`
			ClockRate   uint32                 `json:"clockRate"`
			Parameters  map[string]interface{} `json:"parameters"`
		} `json:"codecs"`
		HeaderExtensions []struct {
			ID      int    `json:"id"`
			URI     string `json:"uri"`
			Encrypt bool   `json:"encrypt"`
		} `json:"headerExtensions"`
	}

	if err := json.Unmarshal([]byte(track.RtpParametersJSON), &rtpParams); err != nil {
		t.Fatalf("failed to unmarshal RtpParametersJSON: %v", err)
	}

	// Verify codecs
	var foundVP8, foundRTX97, foundRTX103 bool
	for _, c := range rtpParams.Codecs {
		if c.MimeType == "video/VP8" && c.PayloadType == 96 {
			foundVP8 = true
		}
		if c.MimeType == "video/rtx" {
			if c.PayloadType == 97 {
				foundRTX97 = true
				if apt, ok := c.Parameters["apt"].(float64); !ok || uint32(apt) != 96 {
					t.Fatalf("expected RTX codec apt to be 96, got %v", c.Parameters["apt"])
				}
			}
			if c.PayloadType == 103 {
				foundRTX103 = true
			}
		}
	}
	if !foundVP8 {
		t.Fatalf("expected video/VP8 codec with PT 96 in codecs")
	}
	if !foundRTX97 {
		t.Fatalf("expected video/rtx codec with PT 97 in codecs")
	}
	if foundRTX103 {
		t.Fatalf("did NOT expect secondary RED rtx PT 103 in primary video codecs")
	}

	// Verify headerExtensions with required encrypt: false (RFC 5285 / Mediasoup Rust requirement)
	if len(rtpParams.HeaderExtensions) != 3 {
		t.Fatalf("expected 3 header extensions, got %d: %+v", len(rtpParams.HeaderExtensions), rtpParams.HeaderExtensions)
	}

	extMap := make(map[int]struct {
		URI     string
		Encrypt bool
	})
	for _, ext := range rtpParams.HeaderExtensions {
		extMap[ext.ID] = struct {
			URI     string
			Encrypt bool
		}{
			URI:     ext.URI,
			Encrypt: ext.Encrypt,
		}
		if ext.Encrypt != false {
			t.Fatalf("expected header extension ID %d to have encrypt: false, got: %v", ext.ID, ext.Encrypt)
		}
	}

	if midExt, ok := extMap[4]; !ok || midExt.URI != "urn:ietf:params:rtp-hdrext:sdes:mid" {
		t.Fatalf("expected header extension 4 for sdes:mid, got: %+v", midExt)
	}
	if absExt, ok := extMap[2]; !ok || absExt.URI != "http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time" {
		t.Fatalf("expected header extension 2 for abs-send-time, got: %+v", absExt)
	}
	if twccExt, ok := extMap[3]; !ok || twccExt.URI != "http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01" {
		t.Fatalf("expected header extension 3 for transport-wide-cc, got: %+v", twccExt)
	}

	// Verify raw JSON string explicitly contains "encrypt":false
	if !strings.Contains(track.RtpParametersJSON, `"encrypt":false`) {
		t.Fatalf("expected RtpParametersJSON to contain \"encrypt\":false, got: %s", track.RtpParametersJSON)
	}
}

func TestSDPMapper_FilterUnsupportedHeaderExtensions_REQ024(t *testing.T) {
	// SDP matching Chromium publisher offer with unsupported WebRTC extension URIs
	sdpOffer := `v=0
o=- 70837583765872365 2 IN IP4 127.0.0.1
s=-
t=0 0
a=group:BUNDLE video-0
m=video 9 UDP/TLS/RTP/SAVPF 96
c=IN IP4 0.0.0.0
a=mid:video-0
a=sendonly
a=setup:actpass
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=rtpmap:96 VP8/90000
a=extmap:14 urn:ietf:params:rtp-hdrext:toffset
a=extmap:2 http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time
a=extmap:13 urn:3gpp:video-orientation
a=extmap:3 http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01
a=extmap:5 http://www.webrtc.org/experiments/rtp-hdrext/playout-delay
a=extmap:6 http://www.webrtc.org/experiments/rtp-hdrext/video-content-type
a=extmap:7 http://www.webrtc.org/experiments/rtp-hdrext/video-timing
a=extmap:8 http://www.webrtc.org/experiments/rtp-hdrext/color-space
a=extmap:4 urn:ietf:params:rtp-hdrext:sdes:mid
a=extmap:10 urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id
a=extmap:11 urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id
a=ssrc:918035962 cname:1PoCmU0GQ07gag4o
`

	parsed, err := livekit.ParsePublisherOffer(sdpOffer)
	if err != nil {
		t.Fatalf("ParsePublisherOffer failed: %v", err)
	}

	if len(parsed.Tracks) != 1 {
		t.Fatalf("expected 1 track, got %d", len(parsed.Tracks))
	}

	track := parsed.Tracks[0]

	var rtpParams struct {
		HeaderExtensions []struct {
			ID      int    `json:"id"`
			URI     string `json:"uri"`
			Encrypt bool   `json:"encrypt"`
		} `json:"headerExtensions"`
	}

	if err := json.Unmarshal([]byte(track.RtpParametersJSON), &rtpParams); err != nil {
		t.Fatalf("failed to unmarshal RtpParametersJSON: %v", err)
	}

	// Unsupported URIs must be excluded
	unsupportedURIs := []string{
		"urn:ietf:params:rtp-hdrext:toffset",
		"http://www.webrtc.org/experiments/rtp-hdrext/video-content-type",
		"http://www.webrtc.org/experiments/rtp-hdrext/video-timing",
		"http://www.webrtc.org/experiments/rtp-hdrext/color-space",
	}

	for _, unsupp := range unsupportedURIs {
		if strings.Contains(track.RtpParametersJSON, unsupp) {
			t.Fatalf("RtpParametersJSON must NOT contain unsupported extension URI: %s", unsupp)
		}
	}

	// Supported URIs must be retained
	expectedIDs := map[int]string{
		2:  "http://www.webrtc.org/experiments/rtp-hdrext/abs-send-time",
		13: "urn:3gpp:video-orientation",
		3:  "http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01",
		5:  "http://www.webrtc.org/experiments/rtp-hdrext/playout-delay",
		4:  "urn:ietf:params:rtp-hdrext:sdes:mid",
		10: "urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id",
		11: "urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id",
	}

	if len(rtpParams.HeaderExtensions) != len(expectedIDs) {
		t.Fatalf("expected %d supported header extensions, got %d: %+v", len(expectedIDs), len(rtpParams.HeaderExtensions), rtpParams.HeaderExtensions)
	}

	for _, ext := range rtpParams.HeaderExtensions {
		expURI, found := expectedIDs[ext.ID]
		if !found {
			t.Fatalf("unexpected header extension ID %d with URI %s", ext.ID, ext.URI)
		}
		if ext.URI != expURI {
			t.Fatalf("expected extension ID %d to have URI %s, got %s", ext.ID, expURI, ext.URI)
		}
		if ext.Encrypt != false {
			t.Fatalf("expected extension ID %d to have encrypt: false, got: %v", ext.ID, ext.Encrypt)
		}
	}
}

func TestSDPMapper_GeneratePublisherAnswer_ExcludeRedAndUlpfec_REQ025(t *testing.T) {
	offerWithRedAndUlpfec := `v=0
o=- 70837583765872365 2 IN IP4 127.0.0.1
s=-
t=0 0
a=group:BUNDLE 0 1
m=audio 9 UDP/TLS/RTP/SAVPF 111
c=IN IP4 0.0.0.0
a=mid:0
a=sendonly
a=setup:actpass
a=rtpmap:111 opus/48000/2
m=video 9 UDP/TLS/RTP/SAVPF 96 97 39 40 98 99 100 101 102 103 104
c=IN IP4 0.0.0.0
a=mid:1
a=sendonly
a=setup:actpass
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=rtpmap:96 VP8/90000
a=rtpmap:97 rtx/90000
a=fmtp:97 apt=96
a=rtpmap:39 H264/90000
a=rtpmap:40 rtx/90000
a=fmtp:40 apt=39
a=rtpmap:98 VP9/90000
a=rtpmap:99 rtx/90000
a=fmtp:99 apt=98
a=rtpmap:100 AV1/90000
a=rtpmap:101 rtx/90000
a=fmtp:101 apt=100
a=rtpmap:102 red/90000
a=rtpmap:103 rtx/90000
a=fmtp:103 apt=102
a=rtpmap:104 ulpfec/90000
a=rtcp-fb:96 nack
a=rtcp-fb:96 nack pli
a=rtcp-fb:102 nack
a=rtcp-fb:104 nack
a=rtcp-fb:* goog-remb
a=extmap:4 urn:ietf:params:rtp-hdrext:sdes:mid
a=ssrc:918035962 cname:test-cname
a=ssrc:3504424756 cname:test-cname
a=ssrc-group:FID 918035962 3504424756
`

	transport := &domain.TransportInfo{
		TransportID: "pub-transport-req025",
		IceUfrag:    "server-ufrag",
		IcePwd:      "server-pwd",
		DtlsFingerprints: []domain.DtlsFingerprintInfo{
			{Algorithm: "sha-256", Value: "11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00"},
		},
	}

	answer, err := livekit.GeneratePublisherAnswer(offerWithRedAndUlpfec, transport)
	if err != nil {
		t.Fatalf("GeneratePublisherAnswer failed: %v", err)
	}

	// 1. Audio m-line must be retained
	if !strings.Contains(answer, "m=audio 9 UDP/TLS/RTP/SAVPF 111") {
		t.Fatalf("expected audio m-line retained in answer, got:\n%s", answer)
	}
	if !strings.Contains(answer, "a=rtpmap:111 opus/48000/2") {
		t.Fatalf("expected audio rtpmap in answer, got:\n%s", answer)
	}

	// 2. Video m-line must NOT contain excluded payload types 102 (red), 103 (rtx for red), 104 (ulpfec)
	expectedVideoMLine := "m=video 9 UDP/TLS/RTP/SAVPF 96 97 39 40 98 99 100 101"
	if !strings.Contains(answer, expectedVideoMLine) {
		t.Fatalf("expected video m-line with formats 96 97 39 40 98 99 100 101, got:\n%s", answer)
	}

	excludedSubstrings := []string{
		" 102",
		" 103",
		" 104",
		"a=rtpmap:102",
		"a=rtpmap:103",
		"a=fmtp:103",
		"a=rtpmap:104",
		"a=rtcp-fb:102",
		"a=rtcp-fb:104",
		"red/90000",
		"ulpfec/90000",
	}
	for _, excl := range excludedSubstrings {
		if strings.Contains(answer, excl) {
			t.Fatalf("answer must NOT contain excluded pattern %q, got:\n%s", excl, answer)
		}
	}

	// 3. Supported video codecs and RTX pairs must be retained
	requiredSubstrings := []string{
		"a=rtpmap:96 VP8/90000",
		"a=rtpmap:97 rtx/90000",
		"a=fmtp:97 apt=96",
		"a=rtpmap:39 H264/90000",
		"a=rtpmap:40 rtx/90000",
		"a=fmtp:40 apt=39",
		"a=rtpmap:98 VP9/90000",
		"a=rtpmap:99 rtx/90000",
		"a=fmtp:99 apt=98",
		"a=rtpmap:100 AV1/90000",
		"a=rtpmap:101 rtx/90000",
		"a=fmtp:101 apt=100",
		"a=rtcp-fb:96 nack",
		"a=rtcp-fb:96 nack pli",
		"a=rtcp-fb:* goog-remb",
		"a=extmap:4 urn:ietf:params:rtp-hdrext:sdes:mid",
	}
	for _, req := range requiredSubstrings {
		if !strings.Contains(answer, req) {
			t.Fatalf("answer must contain required pattern %q, got:\n%s", req, answer)
		}
	}
}

func TestSDPMapper_ParsePublisherOffer_SkipsRedAndUlpfec(t *testing.T) {
	// Offer where red (102) and ulpfec (104) appear before VP8 (96)
	offerRedFirst := `v=0
o=- 70837583765872365 2 IN IP4 127.0.0.1
s=-
t=0 0
m=video 9 UDP/TLS/RTP/SAVPF 102 103 104 96 97
c=IN IP4 0.0.0.0
a=mid:video-0
a=sendonly
a=setup:actpass
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=rtpmap:102 red/90000
a=rtpmap:103 rtx/90000
a=fmtp:103 apt=102
a=rtpmap:104 ulpfec/90000
a=rtpmap:96 VP8/90000
a=rtpmap:97 rtx/90000
a=fmtp:97 apt=96
a=ssrc:918035962 cname:test-cname
a=ssrc:3504424756 cname:test-cname
a=ssrc-group:FID 918035962 3504424756
`

	parsed, err := livekit.ParsePublisherOffer(offerRedFirst)
	if err != nil {
		t.Fatalf("ParsePublisherOffer failed: %v", err)
	}

	if len(parsed.Tracks) != 1 {
		t.Fatalf("expected 1 track, got %d", len(parsed.Tracks))
	}

	track := parsed.Tracks[0]
	// Primary codec must be VP8 (PT 96), NOT RED (PT 102) or ULPFEC (PT 104)
	if track.PayloadType != 96 {
		t.Fatalf("expected primary payloadType 96 (VP8), got %d (%s)", track.PayloadType, track.Codec)
	}
	if track.Codec != "VP8" {
		t.Fatalf("expected codec name VP8, got %s", track.Codec)
	}
	if track.RTXPayloadType != 97 {
		t.Fatalf("expected RTX payloadType 97, got %d", track.RTXPayloadType)
	}
}

func TestSDPMapper_GeneratePublisherAnswer_InactiveMSection_ConformsRFC3264_REQ001(t *testing.T) {
	// Offer containing:
	// - audio (sendonly)
	// - video active camera (sendonly)
	// - video inactive screenshare (inactive) per RFC 3264 Section 6.1
	offerWithInactive := `v=0
o=- 70837583765872365 2 IN IP4 127.0.0.1
s=-
t=0 0
a=group:BUNDLE 0 1 2
a=msid-semantic: WMS
m=audio 9 UDP/TLS/RTP/SAVPF 111
c=IN IP4 0.0.0.0
a=mid:0
a=sendonly
a=setup:actpass
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=rtpmap:111 opus/48000/2
a=ssrc:1001 cname:test-cname
m=video 9 UDP/TLS/RTP/SAVPF 96
c=IN IP4 0.0.0.0
a=mid:1
a=sendonly
a=setup:actpass
a=rtpmap:96 VP8/90000
a=ssrc:2001 cname:test-cname
m=video 9 UDP/TLS/RTP/SAVPF 96
c=IN IP4 0.0.0.0
a=mid:2
a=inactive
a=setup:actpass
a=rtpmap:96 VP8/90000
`

	transport := &domain.TransportInfo{
		TransportID: "pub-transport",
		IceUfrag:    "server-ufrag",
		IcePwd:      "server-pwd",
		DtlsFingerprints: []domain.DtlsFingerprintInfo{
			{Algorithm: "sha-256", Value: "FF:EE:DD:CC:BB:AA"},
		},
		IceCandidates: []domain.IceCandidateInfo{
			{SDPCandidateLine: "candidate:1 1 udp 2130706431 127.0.0.1 50000 typ host"},
		},
	}

	// 1. Verify ParsePublisherOffer skips the inactive track (only returns 2 active tracks)
	parsed, err := livekit.ParsePublisherOffer(offerWithInactive)
	if err != nil {
		t.Fatalf("ParsePublisherOffer failed: %v", err)
	}
	if len(parsed.Tracks) != 2 {
		t.Fatalf("expected 2 active tracks, got %d", len(parsed.Tracks))
	}
	if parsed.Tracks[0].MID != "0" || parsed.Tracks[1].MID != "1" {
		t.Fatalf("unexpected active track MIDs: %v, %v", parsed.Tracks[0].MID, parsed.Tracks[1].MID)
	}

	// 2. Verify GeneratePublisherAnswer sets a=inactive for mid:2 and a=recvonly for mid:0, mid:1
	answerSDPStr, err := livekit.GeneratePublisherAnswer(offerWithInactive, transport)
	if err != nil {
		t.Fatalf("GeneratePublisherAnswer failed: %v", err)
	}

	var ans sdp.SessionDescription
	if err := ans.Unmarshal([]byte(answerSDPStr)); err != nil {
		t.Fatalf("failed to unmarshal answer SDP: %v", err)
	}

	if len(ans.MediaDescriptions) != 3 {
		t.Fatalf("expected 3 media descriptions in answer, got %d", len(ans.MediaDescriptions))
	}

	// Media 0 (audio): must be recvonly
	md0 := ans.MediaDescriptions[0]
	if _, ok := md0.Attribute("recvonly"); !ok {
		t.Fatalf("expected mid:0 to have a=recvonly")
	}
	if _, ok := md0.Attribute("inactive"); ok {
		t.Fatalf("mid:0 must NOT have a=inactive")
	}

	// Media 1 (video camera): must be recvonly
	md1 := ans.MediaDescriptions[1]
	if _, ok := md1.Attribute("recvonly"); !ok {
		t.Fatalf("expected mid:1 to have a=recvonly")
	}
	if _, ok := md1.Attribute("inactive"); ok {
		t.Fatalf("mid:1 must NOT have a=inactive")
	}

	// Media 2 (video screenshare inactive): MUST be inactive per RFC 3264 Section 6.1
	md2 := ans.MediaDescriptions[2]
	if _, ok := md2.Attribute("inactive"); !ok {
		t.Fatalf("expected mid:2 to have a=inactive per RFC 3264, got attributes: %v", md2.Attributes)
	}
	if _, ok := md2.Attribute("recvonly"); ok {
		t.Fatalf("mid:2 must NOT have a=recvonly, would cause browser Incompatible send direction rejection")
	}
}

func TestSDPMapper_GeneratePublisherAnswer_AllInactive_REQ001(t *testing.T) {
	allInactiveOffer := `v=0
o=- 70837583765872365 2 IN IP4 127.0.0.1
s=-
t=0 0
a=group:BUNDLE 0
m=video 9 UDP/TLS/RTP/SAVPF 96
c=IN IP4 0.0.0.0
a=mid:0
a=inactive
a=setup:actpass
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=rtpmap:96 VP8/90000
`

	transport := &domain.TransportInfo{
		TransportID: "pub-transport",
		IceUfrag:    "server-ufrag",
		IcePwd:      "server-pwd",
		DtlsFingerprints: []domain.DtlsFingerprintInfo{
			{Algorithm: "sha-256", Value: "FF:EE:DD:CC:BB:AA"},
		},
	}

	parsed, err := livekit.ParsePublisherOffer(allInactiveOffer)
	if err != nil {
		t.Fatalf("ParsePublisherOffer failed: %v", err)
	}
	if len(parsed.Tracks) != 0 {
		t.Fatalf("expected 0 tracks for all-inactive offer, got %d", len(parsed.Tracks))
	}

	answer, err := livekit.GeneratePublisherAnswer(allInactiveOffer, transport)
	if err != nil {
		t.Fatalf("GeneratePublisherAnswer failed: %v", err)
	}

	if strings.Contains(answer, "a=recvonly") {
		t.Fatalf("answer for all-inactive offer must not contain a=recvonly, got:\n%s", answer)
	}
	if !strings.Contains(answer, "a=inactive") {
		t.Fatalf("answer for all-inactive offer must contain a=inactive, got:\n%s", answer)
	}
}

func TestSDPMapper_GeneratePublisherAnswer_PortZeroRejectedMSection_REQ001(t *testing.T) {
	offerWithPortZero := `v=0
o=- 70837583765872365 2 IN IP4 127.0.0.1
s=-
t=0 0
a=group:BUNDLE 0 1
m=audio 9 UDP/TLS/RTP/SAVPF 111
c=IN IP4 0.0.0.0
a=mid:0
a=sendonly
a=setup:actpass
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=rtpmap:111 opus/48000/2
m=video 0 UDP/TLS/RTP/SAVPF 96
c=IN IP4 0.0.0.0
a=mid:1
a=rtpmap:96 VP8/90000
`

	transport := &domain.TransportInfo{
		TransportID: "pub-transport",
		IceUfrag:    "server-ufrag",
		IcePwd:      "server-pwd",
		DtlsFingerprints: []domain.DtlsFingerprintInfo{
			{Algorithm: "sha-256", Value: "FF:EE:DD:CC:BB:AA"},
		},
	}

	answerSDPStr, err := livekit.GeneratePublisherAnswer(offerWithPortZero, transport)
	if err != nil {
		t.Fatalf("GeneratePublisherAnswer failed: %v", err)
	}

	var ans sdp.SessionDescription
	if err := ans.Unmarshal([]byte(answerSDPStr)); err != nil {
		t.Fatalf("failed to unmarshal answer SDP: %v", err)
	}

	if len(ans.MediaDescriptions) != 2 {
		t.Fatalf("expected 2 media descriptions, got %d", len(ans.MediaDescriptions))
	}

	// m=video port must be 0 per RFC 3264 Section 8.2
	if ans.MediaDescriptions[1].MediaName.Port.Value != 0 {
		t.Fatalf("expected rejected m-section port to be 0, got %d", ans.MediaDescriptions[1].MediaName.Port.Value)
	}
	if _, ok := ans.MediaDescriptions[1].Attribute("inactive"); !ok {
		t.Fatalf("expected rejected m-section to have a=inactive")
	}

	// BUNDLE must only contain active mid "0", excluding rejected mid "1" per RFC 8843 Section 7.3
	groupAttr, ok := ans.Attribute("group")
	if !ok {
		t.Fatalf("expected group attribute in answer")
	}
	if strings.Contains(groupAttr, "1") {
		t.Fatalf("rejected mid 1 must be excluded from BUNDLE group, got: %s", groupAttr)
	}
}

func TestSDPMapper_ParsePublisherOffer_PrioritizeVP8OverH264_REQ003(t *testing.T) {
	// Offer lists H264 (39) before VP8 (96) in Formats and rtpmap
	offerH264First := `v=0
o=- 123456789 2 IN IP4 127.0.0.1
s=-
t=0 0
a=group:BUNDLE video
m=video 9 UDP/TLS/RTP/SAVPF 39 40 96 97
c=IN IP4 0.0.0.0
a=mid:video
a=sendonly
a=setup:actpass
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=rtpmap:39 H264/90000
a=fmtp:39 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f
a=rtpmap:40 rtx/90000
a=fmtp:40 apt=39
a=rtpmap:96 VP8/90000
a=rtpmap:97 rtx/90000
a=fmtp:97 apt=96
a=ssrc:555666 cname:test-user
`

	parsed, err := livekit.ParsePublisherOffer(offerH264First)
	if err != nil {
		t.Fatalf("ParsePublisherOffer failed: %v", err)
	}
	if len(parsed.Tracks) != 1 {
		t.Fatalf("expected 1 track, got %d", len(parsed.Tracks))
	}

	track := parsed.Tracks[0]
	// VP8 must be prioritized over H264 per REQ-003 to prevent router "Unsupported codec" errors
	if track.PayloadType != 96 {
		t.Fatalf("expected prioritized PayloadType 96 (VP8), got %d", track.PayloadType)
	}
	if !strings.EqualFold(track.Codec, "VP8") {
		t.Fatalf("expected prioritized Codec VP8, got %s", track.Codec)
	}
	if track.RTXPayloadType != 97 {
		t.Fatalf("expected RTXPayloadType 97 for VP8, got %d", track.RTXPayloadType)
	}

	// Offer with only H264: H264 must still be selected
	offerOnlyH264 := `v=0
o=- 123456789 2 IN IP4 127.0.0.1
s=-
t=0 0
a=group:BUNDLE video
m=video 9 UDP/TLS/RTP/SAVPF 39 40
c=IN IP4 0.0.0.0
a=mid:video
a=sendonly
a=setup:actpass
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=rtpmap:39 H264/90000
a=fmtp:39 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f
a=rtpmap:40 rtx/90000
a=fmtp:40 apt=39
a=ssrc:555666 cname:test-user
`

	parsedH264Only, err := livekit.ParsePublisherOffer(offerOnlyH264)
	if err != nil {
		t.Fatalf("ParsePublisherOffer failed: %v", err)
	}
	if len(parsedH264Only.Tracks) != 1 {
		t.Fatalf("expected 1 track, got %d", len(parsedH264Only.Tracks))
	}
	trackH264 := parsedH264Only.Tracks[0]
	if trackH264.PayloadType != 39 {
		t.Fatalf("expected PayloadType 39 (H264), got %d", trackH264.PayloadType)
	}
	if !strings.EqualFold(trackH264.Codec, "H264") {
		t.Fatalf("expected Codec H264, got %s", trackH264.Codec)
	}
	if trackH264.RTXPayloadType != 40 {
		t.Fatalf("expected RTXPayloadType 40 for H264, got %d", trackH264.RTXPayloadType)
	}

	// Offer with H264 (39) + H264 RTX (40) and VP8 (96) with NO RTX
	// VP8 must be chosen, and RTXPayloadType must be 0 (must NOT be set to 40)
	offerVP8WithoutRTX := `v=0
o=- 123456789 2 IN IP4 127.0.0.1
s=-
t=0 0
a=group:BUNDLE video
m=video 9 UDP/TLS/RTP/SAVPF 39 40 96
c=IN IP4 0.0.0.0
a=mid:video
a=sendonly
a=setup:actpass
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=rtpmap:39 H264/90000
a=fmtp:39 level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f
a=rtpmap:40 rtx/90000
a=fmtp:40 apt=39
a=rtpmap:96 VP8/90000
a=ssrc:555666 cname:test-user
`
	parsedVP8NoRTX, err := livekit.ParsePublisherOffer(offerVP8WithoutRTX)
	if err != nil {
		t.Fatalf("ParsePublisherOffer failed: %v", err)
	}
	trackVP8NoRTX := parsedVP8NoRTX.Tracks[0]
	if trackVP8NoRTX.PayloadType != 96 {
		t.Fatalf("expected PayloadType 96 (VP8), got %d", trackVP8NoRTX.PayloadType)
	}
	if trackVP8NoRTX.RTXPayloadType != 0 {
		t.Fatalf("expected RTXPayloadType 0 for VP8 when no apt matched, got %d (disparate H264 RTX was incorrectly assigned)", trackVP8NoRTX.RTXPayloadType)
	}
}

func TestSDPMapper_DTLSRole_DeadlockResolution_REQ005(t *testing.T) {
	// 1. Client offers actpass -> SFU must adopt "client" role to initiate DTLS ClientHello
	offerActpass := `v=0
o=- 112233 2 IN IP4 127.0.0.1
s=-
t=0 0
m=video 9 UDP/TLS/RTP/SAVPF 96
c=IN IP4 0.0.0.0
a=mid:video-1
a=sendonly
a=setup:actpass
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=rtpmap:96 VP8/90000
`
	parsed, err := livekit.ParsePublisherOffer(offerActpass)
	if err != nil {
		t.Fatalf("ParsePublisherOffer failed: %v", err)
	}
	if parsed.DTLSRole != "client" {
		t.Fatalf("expected SFU DTLSRole 'client' when client sends actpass, got '%s'", parsed.DTLSRole)
	}

	// 2. Client offers passive -> SFU must adopt "client" role
	offerPassive := strings.ReplaceAll(offerActpass, "a=setup:actpass", "a=setup:passive")
	parsedPassive, err := livekit.ParsePublisherOffer(offerPassive)
	if err != nil {
		t.Fatalf("ParsePublisherOffer failed: %v", err)
	}
	if parsedPassive.DTLSRole != "client" {
		t.Fatalf("expected SFU DTLSRole 'client' when client sends passive, got '%s'", parsedPassive.DTLSRole)
	}

	// 3. Client offers active -> SFU must adopt "server" role
	offerActive := strings.ReplaceAll(offerActpass, "a=setup:actpass", "a=setup:active")
	parsedActive, err := livekit.ParsePublisherOffer(offerActive)
	if err != nil {
		t.Fatalf("ParsePublisherOffer failed: %v", err)
	}
	if parsedActive.DTLSRole != "server" {
		t.Fatalf("expected SFU DTLSRole 'server' when client sends active, got '%s'", parsedActive.DTLSRole)
	}
}

func TestSDPMapper_GeneratePublisherAnswer_MediaLevelSetupActive_AllSections_REQ005(t *testing.T) {
	// Offer containing application (DataChannel), audio, and video sections
	offerThreeMedia := `v=0
o=- 998877 2 IN IP4 127.0.0.1
s=-
t=0 0
a=group:BUNDLE datachannel audio video
m=application 9 UDP/DTLS/SCTP webrtc-datachannel
c=IN IP4 0.0.0.0
a=mid:datachannel
a=sctp-port:5000
a=setup:actpass
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
m=audio 9 UDP/TLS/RTP/SAVPF 111
c=IN IP4 0.0.0.0
a=mid:audio
a=sendonly
a=setup:actpass
a=rtpmap:111 opus/48000/2
a=ssrc:1111 cname:alice
m=video 9 UDP/TLS/RTP/SAVPF 96
c=IN IP4 0.0.0.0
a=mid:video
a=sendonly
a=setup:actpass
a=rtpmap:96 VP8/90000
a=ssrc:2222 cname:alice
`
	transport := &domain.TransportInfo{
		TransportID: "pub-transport",
		IceUfrag:    "server-ufrag",
		IcePwd:      "server-pwd",
		DtlsFingerprints: []domain.DtlsFingerprintInfo{
			{Algorithm: "sha-256", Value: "11:22:33:44:55:66"},
		},
	}

	answer, err := livekit.GeneratePublisherAnswer(offerThreeMedia, transport)
	if err != nil {
		t.Fatalf("GeneratePublisherAnswer failed: %v", err)
	}

	var ans sdp.SessionDescription
	if err := ans.Unmarshal([]byte(answer)); err != nil {
		t.Fatalf("failed to unmarshal answer: %v", err)
	}

	// Session level must have a=setup:active
	sessionSetup, ok := ans.Attribute("setup")
	if !ok || sessionSetup != "active" {
		t.Fatalf("expected session level a=setup:active, got '%s' (found: %v)", sessionSetup, ok)
	}

	// Every media section (application, audio, video) MUST have a=setup:active directly
	if len(ans.MediaDescriptions) != 3 {
		t.Fatalf("expected 3 media sections in answer, got %d", len(ans.MediaDescriptions))
	}

	expectedMids := []string{"datachannel", "audio", "video"}
	for i, md := range ans.MediaDescriptions {
		mid, _ := md.Attribute("mid")
		if mid != expectedMids[i] {
			t.Fatalf("expected mid %s at index %d, got %s", expectedMids[i], i, mid)
		}
		setupVal, hasSetup := md.Attribute("setup")
		if !hasSetup {
			t.Fatalf("media section %s (%s) is missing a=setup attribute!", mid, md.MediaName.Media)
		}
		if setupVal != "active" {
			t.Fatalf("media section %s (%s) has a=setup:%s, expected a=setup:active", mid, md.MediaName.Media, setupVal)
		}
	}
}

func TestSDPMapper_HeaderExtensions_MidRidTransportCC_REQ005(t *testing.T) {
	// Offer matching browser / Pion client declaring MID, RID (Simulcast), and Transport-CC
	offerWithExt := `v=0
o=- 445566 2 IN IP4 127.0.0.1
s=-
t=0 0
a=group:BUNDLE video
m=video 9 UDP/TLS/RTP/SAVPF 96
c=IN IP4 0.0.0.0
a=mid:video
a=sendonly
a=setup:actpass
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=rtpmap:96 VP8/90000
a=extmap:1 urn:ietf:params:rtp-hdrext:sdes:mid
a=extmap:2 urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id
a=extmap:3 http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01
a=rid:f send
a=rid:h send
a=rid:q send
a=simulcast:send f;h;q
a=ssrc:3001 cname:test
`
	parsed, err := livekit.ParsePublisherOffer(offerWithExt)
	if err != nil {
		t.Fatalf("ParsePublisherOffer failed: %v", err)
	}

	if len(parsed.Tracks) != 1 {
		t.Fatalf("expected 1 track, got %d", len(parsed.Tracks))
	}

	track := parsed.Tracks[0]

	// Verify both headerExtensions and header_extensions are present in RtpParametersJSON
	var rawMap map[string]interface{}
	if err := json.Unmarshal([]byte(track.RtpParametersJSON), &rawMap); err != nil {
		t.Fatalf("failed to unmarshal track.RtpParametersJSON: %v", err)
	}

	if _, ok := rawMap["headerExtensions"]; !ok {
		t.Fatalf("expected headerExtensions key in RtpParametersJSON")
	}
	if _, ok := rawMap["header_extensions"]; !ok {
		t.Fatalf("expected header_extensions key in RtpParametersJSON per REQ-005")
	}

	var rtpParams struct {
		HeaderExtensions []struct {
			ID      int    `json:"id"`
			URI     string `json:"uri"`
			Encrypt bool   `json:"encrypt"`
		} `json:"headerExtensions"`
		HeaderExtensionsSnake []struct {
			ID      int    `json:"id"`
			URI     string `json:"uri"`
			Encrypt bool   `json:"encrypt"`
		} `json:"header_extensions"`
	}

	if err := json.Unmarshal([]byte(track.RtpParametersJSON), &rtpParams); err != nil {
		t.Fatalf("failed to unmarshal typed RtpParametersJSON: %v", err)
	}

	if len(rtpParams.HeaderExtensions) != 3 {
		t.Fatalf("expected 3 header extensions, got %d: %+v", len(rtpParams.HeaderExtensions), rtpParams.HeaderExtensions)
	}
	if len(rtpParams.HeaderExtensionsSnake) != 3 {
		t.Fatalf("expected 3 header_extensions (snake_case), got %d", len(rtpParams.HeaderExtensionsSnake))
	}

	extByURI := make(map[string]int)
	for _, ext := range rtpParams.HeaderExtensions {
		extByURI[ext.URI] = ext.ID
		if ext.Encrypt != false {
			t.Fatalf("expected encrypt: false for extension %s", ext.URI)
		}
	}

	// Verify MID
	if id, ok := extByURI["urn:ietf:params:rtp-hdrext:sdes:mid"]; !ok || id != 1 {
		t.Fatalf("expected MID extension id 1, got id=%d (found=%v)", id, ok)
	}
	// Verify RID
	if id, ok := extByURI["urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id"]; !ok || id != 2 {
		t.Fatalf("expected RID extension id 2, got id=%d (found=%v)", id, ok)
	}
	// Verify Transport-CC
	if id, ok := extByURI["http://www.ietf.org/id/draft-holmer-rmcat-transport-wide-cc-extensions-01"]; !ok || id != 3 {
		t.Fatalf("expected Transport-CC extension id 3, got id=%d (found=%v)", id, ok)
	}
}

func TestSDPMapper_ParseSubscriberAnswer_DTLSRole_DeadlockResolution_REQ006(t *testing.T) {
	// Base subscriber answer with media-level setup
	sampleSubscriberAnswer := `v=0
o=- 234567 2 IN IP4 127.0.0.1
s=-
t=0 0
a=group:BUNDLE 0
m=video 9 UDP/TLS/RTP/SAVPF 96
c=IN IP4 0.0.0.0
a=mid:0
a=recvonly
a=setup:passive
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=rtpmap:96 VP8/90000
`

	// 1. Client answers "passive" (Pion WebRTC standard answer to actpass offer)
	// Mediasoup must adopt "client" role to actively send ClientHello and resolve deadlock
	parsed, err := livekit.ParseSubscriberAnswer(sampleSubscriberAnswer)
	if err != nil {
		t.Fatalf("ParseSubscriberAnswer failed: %v", err)
	}
	if parsed.DTLSRole != "client" {
		t.Fatalf("expected SFU DTLSRole 'client' when client answers passive, got '%s'", parsed.DTLSRole)
	}
	if len(parsed.RemoteFingerprints) != 1 {
		t.Fatalf("expected 1 remote fingerprint, got %d", len(parsed.RemoteFingerprints))
	}
	if parsed.RemoteFingerprints[0].Algorithm != "sha-256" {
		t.Fatalf("expected sha-256 fingerprint, got %s", parsed.RemoteFingerprints[0].Algorithm)
	}

	// 2. Client answers "passive" at session level
	sessionPassive := `v=0
o=- 234567 2 IN IP4 127.0.0.1
s=-
t=0 0
a=setup:passive
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
m=audio 9 UDP/TLS/RTP/SAVPF 111
c=IN IP4 0.0.0.0
a=mid:0
a=recvonly
a=rtpmap:111 opus/48000/2
`
	parsedSessPassive, err := livekit.ParseSubscriberAnswer(sessionPassive)
	if err != nil {
		t.Fatalf("ParseSubscriberAnswer failed: %v", err)
	}
	if parsedSessPassive.DTLSRole != "client" {
		t.Fatalf("expected SFU DTLSRole 'client' when client answers passive at session level, got '%s'", parsedSessPassive.DTLSRole)
	}

	// 3. Client answers "active" -> SFU must adopt "server" role
	answerActive := strings.ReplaceAll(sampleSubscriberAnswer, "a=setup:passive", "a=setup:active")
	parsedActive, err := livekit.ParseSubscriberAnswer(answerActive)
	if err != nil {
		t.Fatalf("ParseSubscriberAnswer failed: %v", err)
	}
	if parsedActive.DTLSRole != "server" {
		t.Fatalf("expected SFU DTLSRole 'server' when client answers active, got '%s'", parsedActive.DTLSRole)
	}

	// 4. Client answers "actpass" -> SFU defaults to "client" role
	answerActpass := strings.ReplaceAll(sampleSubscriberAnswer, "a=setup:passive", "a=setup:actpass")
	parsedActpass, err := livekit.ParseSubscriberAnswer(answerActpass)
	if err != nil {
		t.Fatalf("ParseSubscriberAnswer failed: %v", err)
	}
	if parsedActpass.DTLSRole != "client" {
		t.Fatalf("expected SFU DTLSRole 'client' when client answers actpass, got '%s'", parsedActpass.DTLSRole)
	}

	// 5. Client answer omits a=setup entirely -> SFU defaults to "client" role
	answerNoSetup := strings.ReplaceAll(sampleSubscriberAnswer, "a=setup:passive\n", "")
	parsedNoSetup, err := livekit.ParseSubscriberAnswer(answerNoSetup)
	if err != nil {
		t.Fatalf("ParseSubscriberAnswer failed: %v", err)
	}
	if parsedNoSetup.DTLSRole != "client" {
		t.Fatalf("expected default SFU DTLSRole 'client' when a=setup is omitted, got '%s'", parsedNoSetup.DTLSRole)
	}

	// 6. Case-insensitivity & whitespace trimming
	answerCase := strings.ReplaceAll(sampleSubscriberAnswer, "a=setup:passive", "a=SETUP:  PASSIVE  ")
	parsedCase, err := livekit.ParseSubscriberAnswer(answerCase)
	if err != nil {
		t.Fatalf("ParseSubscriberAnswer failed: %v", err)
	}
	if parsedCase.DTLSRole != "client" {
		t.Fatalf("expected SFU DTLSRole 'client' for 'a=SETUP:  PASSIVE  ', got '%s'", parsedCase.DTLSRole)
	}

	// 7. Duplicate fingerprints at session and media level are deduplicated
	answerDupFP := `v=0
o=- 234567 2 IN IP4 127.0.0.1
s=-
t=0 0
a=setup:passive
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
m=video 9 UDP/TLS/RTP/SAVPF 96
c=IN IP4 0.0.0.0
a=mid:0
a=fingerprint:sha-256 11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00
a=setup:passive
`
	parsedDupFP, err := livekit.ParseSubscriberAnswer(answerDupFP)
	if err != nil {
		t.Fatalf("ParseSubscriberAnswer failed: %v", err)
	}
	if len(parsedDupFP.RemoteFingerprints) != 1 {
		t.Fatalf("expected deduplication to 1 fingerprint, got %d", len(parsedDupFP.RemoteFingerprints))
	}
}

func TestSDPMapper_GenerateSubscriberOffer_MediaLevelSetupActpass_REQ006(t *testing.T) {
	transport := &domain.TransportInfo{
		TransportID: "sub-transport-req006",
		IceUfrag:    "ufrag-req006",
		IcePwd:      "pwd-req006",
		DtlsFingerprints: []domain.DtlsFingerprintInfo{
			{Algorithm: "sha-256", Value: "00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF"},
		},
	}
	slots := []*domain.SDPSlot{
		{
			Index:          0,
			MID:            "0",
			Kind:           domain.TrackKindVideo,
			Active:         true,
			PayloadType:    96,
			RTXPayloadType: 97,
			MimeType:       "video/vp8",
			ClockRate:      90000,
			TrackSID:       "TR_video_1",
			PublisherSID:   "PA_pub_1",
		},
		{
			Index:       1,
			MID:         "1",
			Kind:        domain.TrackKindAudio,
			Active:      false, // recycled inactive slot
			PayloadType: 111,
			MimeType:    "audio/opus",
			ClockRate:   48000,
			TrackSID:    "TR_audio_1",
			PublisherSID: "PA_pub_1",
		},
	}

	offer, err := livekit.GenerateSubscriberOffer(transport, slots, "PA_sub_1")
	if err != nil {
		t.Fatalf("GenerateSubscriberOffer failed: %v", err)
	}

	// 1. Session level must have a=setup:actpass
	if !strings.Contains(offer, "a=setup:actpass") {
		t.Fatalf("expected a=setup:actpass in subscriber offer, got:\n%s", offer)
	}

	// 2. Count a=setup:actpass occurrences: 1 at session level + 1 in active m-line + 1 in inactive m-line = 3
	setupCount := strings.Count(offer, "a=setup:actpass")
	if setupCount < 3 {
		t.Fatalf("expected a=setup:actpass at session level and within each media description, got count=%d in:\n%s", setupCount, offer)
	}
}

func TestSDPMapper_GenerateSubscriberOffer_IncludesDataChannel_REQ028(t *testing.T) {
	transportWithSctp := &domain.TransportInfo{
		TransportID: "sub-sctp-transport",
		IceUfrag:    "ufrag-sub",
		IcePwd:      "pwd-sub",
		SctpPort:    5000,
		DtlsFingerprints: []domain.DtlsFingerprintInfo{
			{Algorithm: "sha-256", Value: "11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00"},
		},
		IceCandidates: []domain.IceCandidateInfo{
			{SDPCandidateLine: "candidate:udpcand 1 udp 2130706431 127.0.0.1 50001 typ host"},
		},
	}

	slots := []*domain.SDPSlot{
		{
			Index:        0,
			MID:          "0",
			Kind:         domain.TrackKindAudio,
			Active:       true,
			PayloadType:  111,
			MimeType:     "audio/opus",
			ClockRate:    48000,
			TrackSID:     "TR_audio_1",
			PublisherSID: "PA_pub_1",
		},
	}

	// 1. With audio slot and SCTP port: should contain datachannel and audio m-line bundled together
	offer, err := livekit.GenerateSubscriberOffer(transportWithSctp, slots, "PA_sub_1")
	if err != nil {
		t.Fatalf("GenerateSubscriberOffer failed: %v", err)
	}

	if !strings.Contains(offer, "m=application 9 UDP/DTLS/SCTP webrtc-datachannel") {
		t.Fatalf("expected m=application in subscriber offer, got:\n%s", offer)
	}
	if !strings.Contains(offer, "a=mid:datachannel") {
		t.Fatalf("expected a=mid:datachannel, got:\n%s", offer)
	}
	if !strings.Contains(offer, "a=sctp-port:5000") {
		t.Fatalf("expected a=sctp-port:5000, got:\n%s", offer)
	}
	if !strings.Contains(offer, "a=max-message-size:262144") {
		t.Fatalf("expected a=max-message-size:262144, got:\n%s", offer)
	}
	if !strings.Contains(offer, "a=group:BUNDLE datachannel 0") {
		t.Fatalf("expected BUNDLE datachannel 0, got:\n%s", offer)
	}

	// 2. Empty slots (DataChannel only offer)
	emptyOffer, err := livekit.GenerateSubscriberOffer(transportWithSctp, nil, "PA_sub_1")
	if err != nil {
		t.Fatalf("GenerateSubscriberOffer with nil slots failed: %v", err)
	}
	if !strings.Contains(emptyOffer, "m=application 9 UDP/DTLS/SCTP webrtc-datachannel") {
		t.Fatalf("expected m=application in empty slot offer, got:\n%s", emptyOffer)
	}
	if !strings.Contains(emptyOffer, "a=group:BUNDLE datachannel") {
		t.Fatalf("expected a=group:BUNDLE datachannel, got:\n%s", emptyOffer)
	}

	// 3. Transport without SCTP (SctpPort == 0)
	transportNoSctp := &domain.TransportInfo{
		TransportID: "sub-no-sctp",
		IceUfrag:    "ufrag-sub",
		IcePwd:      "pwd-sub",
		SctpPort:    0,
		DtlsFingerprints: []domain.DtlsFingerprintInfo{
			{Algorithm: "sha-256", Value: "11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00"},
		},
	}
	noSctpOffer, err := livekit.GenerateSubscriberOffer(transportNoSctp, slots, "PA_sub_1")
	if err != nil {
		t.Fatalf("GenerateSubscriberOffer failed: %v", err)
	}
	if strings.Contains(noSctpOffer, "m=application") {
		t.Fatalf("expected no m=application when SctpPort == 0, got:\n%s", noSctpOffer)
	}
	if strings.Contains(noSctpOffer, "datachannel") {
		t.Fatalf("expected no datachannel MID when SctpPort == 0, got:\n%s", noSctpOffer)
	}
}

