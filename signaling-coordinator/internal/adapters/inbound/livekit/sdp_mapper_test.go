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
	if audio.Kind != domain.TrackKindAudio || audio.SSRC != 1001 || audio.PayloadType != 111 || audio.Codec != "opus" {
		t.Fatalf("unexpected audio track: %+v", audio)
	}

	video := parsed.Tracks[1]
	if video.Kind != domain.TrackKindVideo || video.SSRC != 2001 || video.RTXSSRC != 2002 || video.PayloadType != 96 {
		t.Fatalf("unexpected video track: %+v", video)
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
