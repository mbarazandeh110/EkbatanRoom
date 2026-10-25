package livekit_test

import (
	"strings"
	"testing"

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
}

func TestSDPMapper_GeneratePublisherAnswer_ExplicitPassive(t *testing.T) {
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

	// Must contain a=setup:passive for DTLS encryption handshake
	if !strings.Contains(answer, "a=setup:passive") {
		t.Fatalf("expected a=setup:passive in answer, got:\n%s", answer)
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
			Index:          0,
			MID:            "0",
			Kind:           domain.TrackKindAudio,
			Active:         true,
			TrackSID:       "TR_audio_1",
			ProducerID:     "prod_audio",
			ConsumerID:     "cons_audio",
			SSRC:           50001,
			PayloadType:    111,
			ClockRate:      48000,
			MimeType:       "opus",
		},
		{
			Index:          1,
			MID:            "1",
			Kind:           domain.TrackKindVideo,
			Active:         false, // Inactivated slot for recycling
			PayloadType:    96,
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



