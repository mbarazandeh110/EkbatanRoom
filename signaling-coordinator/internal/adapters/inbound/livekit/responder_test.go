package livekit_test

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/gorilla/websocket"
	livekitproto "github.com/livekit/protocol/livekit"
	"google.golang.org/protobuf/proto"

	"ekbatan-room/signaling-coordinator/internal/adapters/inbound/livekit"
	"ekbatan-room/signaling-coordinator/internal/domain"
)

func TestLiveKitResponder_SendParticipantUpdate_MapsDisconnectedState(t *testing.T) {
	upgrader := websocket.Upgrader{CheckOrigin: func(r *http.Request) bool { return true }}
	serverConnChan := make(chan *websocket.Conn, 1)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		conn, err := upgrader.Upgrade(w, r, nil)
		if err != nil {
			t.Errorf("Upgrade failed: %v", err)
			return
		}
		serverConnChan <- conn
	}))
	defer server.Close()

	wsURL := "ws" + strings.TrimPrefix(server.URL, "http")
	clientConn, _, err := websocket.DefaultDialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatalf("client dial failed: %v", err)
	}
	defer clientConn.Close()

	serverConn := <-serverConnChan
	defer serverConn.Close()

	responder := livekit.NewLiveKitResponder(serverConn)

	p1 := domain.NewParticipant("user-1", "PA_1", "Alice")
	p1.SetConnected()

	p2 := domain.NewParticipant("user-2", "PA_2", "Bob")
	p2.SetDisconnected()

	err = responder.SendParticipantUpdate([]*domain.Participant{p1, p2})
	if err != nil {
		t.Fatalf("SendParticipantUpdate failed: %v", err)
	}

	_, msgBytes, err := clientConn.ReadMessage()
	if err != nil {
		t.Fatalf("ReadMessage failed: %v", err)
	}

	var resp livekitproto.SignalResponse
	if err := proto.Unmarshal(msgBytes, &resp); err != nil {
		t.Fatalf("proto.Unmarshal failed: %v", err)
	}

	update := resp.GetUpdate()
	if update == nil {
		t.Fatalf("expected ParticipantUpdate in SignalResponse, got: %+v", resp.Message)
	}

	if len(update.Participants) != 2 {
		t.Fatalf("expected 2 participants in update, got %d", len(update.Participants))
	}

	var aliceInfo, bobInfo *livekitproto.ParticipantInfo
	for _, p := range update.Participants {
		if p.Identity == "user-1" {
			aliceInfo = p
		} else if p.Identity == "user-2" {
			bobInfo = p
		}
	}

	if aliceInfo == nil || aliceInfo.State != livekitproto.ParticipantInfo_JOINED {
		t.Fatalf("expected Alice state to be JOINED, got: %v", aliceInfo)
	}

	if bobInfo == nil || bobInfo.State != livekitproto.ParticipantInfo_DISCONNECTED {
		t.Fatalf("expected Bob state to be DISCONNECTED, got: %v", bobInfo)
	}
}

func TestLiveKitResponder_SendSubscribedQualityUpdate(t *testing.T) {
	upgrader := websocket.Upgrader{CheckOrigin: func(r *http.Request) bool { return true }}
	serverConnChan := make(chan *websocket.Conn, 1)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		conn, err := upgrader.Upgrade(w, r, nil)
		if err != nil {
			t.Errorf("Upgrade failed: %v", err)
			return
		}
		serverConnChan <- conn
	}))
	defer server.Close()

	wsURL := "ws" + strings.TrimPrefix(server.URL, "http")
	clientConn, _, err := websocket.DefaultDialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatalf("client dial failed: %v", err)
	}
	defer clientConn.Close()

	serverConn := <-serverConnChan
	defer serverConn.Close()

	responder := livekit.NewLiveKitResponder(serverConn)

	qualities := []domain.SubscribedQuality{
		{Quality: domain.VideoQualityLow, Enabled: true},
		{Quality: domain.VideoQualityMedium, Enabled: true},
		{Quality: domain.VideoQualityHigh, Enabled: true},
	}

	err = responder.SendSubscribedQualityUpdate("TR_video_test", "video/VP8", qualities)
	if err != nil {
		t.Fatalf("SendSubscribedQualityUpdate failed: %v", err)
	}

	_, msgBytes, err := clientConn.ReadMessage()
	if err != nil {
		t.Fatalf("ReadMessage failed: %v", err)
	}

	var resp livekitproto.SignalResponse
	if err := proto.Unmarshal(msgBytes, &resp); err != nil {
		t.Fatalf("proto.Unmarshal failed: %v", err)
	}

	squ := resp.GetSubscribedQualityUpdate()
	if squ == nil {
		t.Fatalf("expected SubscribedQualityUpdate in SignalResponse, got: %+v", resp.Message)
	}

	if squ.TrackSid != "TR_video_test" {
		t.Fatalf("expected TrackSid == TR_video_test, got %s", squ.TrackSid)
	}

	if len(squ.SubscribedQualities) != 3 {
		t.Fatalf("expected 3 SubscribedQualities, got %d", len(squ.SubscribedQualities))
	}
	for _, q := range squ.SubscribedQualities {
		if !q.Enabled {
			t.Fatalf("expected quality %v to be enabled", q.Quality)
		}
	}

	if len(squ.SubscribedCodecs) != 1 {
		t.Fatalf("expected 1 SubscribedCodec, got %d", len(squ.SubscribedCodecs))
	}
	sc := squ.SubscribedCodecs[0]
	if sc.Codec != "vp8" {
		t.Fatalf("expected codec == vp8, got %s", sc.Codec)
	}
	if len(sc.Qualities) != 3 {
		t.Fatalf("expected 3 qualities in SubscribedCodec, got %d", len(sc.Qualities))
	}
}

func TestLiveKitResponder_MapDomainTracksToProto_REQ003(t *testing.T) {
	upgrader := websocket.Upgrader{CheckOrigin: func(r *http.Request) bool { return true }}
	serverConnChan := make(chan *websocket.Conn, 1)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		conn, err := upgrader.Upgrade(w, r, nil)
		if err != nil {
			t.Errorf("Upgrade failed: %v", err)
			return
		}
		serverConnChan <- conn
	}))
	defer server.Close()

	wsURL := "ws" + strings.TrimPrefix(server.URL, "http")
	clientConn, _, err := websocket.DefaultDialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatalf("client dial failed: %v", err)
	}
	defer clientConn.Close()

	serverConn := <-serverConnChan
	defer serverConn.Close()

	responder := livekit.NewLiveKitResponder(serverConn)

	p := domain.NewParticipant("user-1", "PA_1", "Alice")
	p.SetConnected()

	// 1. Audio track with empty MimeType (should default to audio/opus)
	audioTrack := &domain.Track{
		ID:       "PR_audio",
		SID:      "TR_audio_1",
		Kind:     domain.TrackKindAudio,
		Name:     "audio-mic",
		MimeType: "",
	}
	_ = p.AddTrack(audioTrack)

	// 2. Video track with simulcast and MimeType "VP8" (should normalize to video/vp8)
	videoTrack := &domain.Track{
		ID:       "PR_video",
		SID:      "TR_video_1",
		Kind:     domain.TrackKindVideo,
		Name:     "video-camera",
		Width:    1280,
		Height:   720,
		MimeType: "VP8",
		SimulcastEncodings: []domain.SimulcastLayer{
			{RID: "q", SSRC: 1001, MaxBitrate: 100000},
			{RID: "h", SSRC: 1002, MaxBitrate: 300000},
			{RID: "f", SSRC: 1003, MaxBitrate: 800000},
		},
	}
	_ = p.AddTrack(videoTrack)

	// 3. Single-layer video track without simulcast
	singleVideoTrack := &domain.Track{
		ID:       "PR_video_single",
		SID:      "TR_video_single",
		Kind:     domain.TrackKindVideo,
		Name:     "video-screen",
		Width:    1920,
		Height:   1080,
		MimeType: "video/vp8",
	}
	_ = p.AddTrack(singleVideoTrack)

	// 4. Simulcast video track with numeric RIDs ("0", "1", "2")
	numericSimulcastTrack := &domain.Track{
		ID:       "PR_video_num",
		SID:      "TR_video_num",
		Kind:     domain.TrackKindVideo,
		Name:     "video-numeric",
		Width:    1280,
		Height:   720,
		MimeType: "vp8",
		SimulcastEncodings: []domain.SimulcastLayer{
			{RID: "0", SSRC: 2001, MaxBitrate: 150000},
			{RID: "1", SSRC: 2002, MaxBitrate: 450000},
			{RID: "2", SSRC: 2003, MaxBitrate: 900000},
		},
	}
	_ = p.AddTrack(numericSimulcastTrack)

	err = responder.SendParticipantUpdate([]*domain.Participant{p})
	if err != nil {
		t.Fatalf("SendParticipantUpdate failed: %v", err)
	}

	_, msgBytes, err := clientConn.ReadMessage()
	if err != nil {
		t.Fatalf("ReadMessage failed: %v", err)
	}

	var resp livekitproto.SignalResponse
	if err := proto.Unmarshal(msgBytes, &resp); err != nil {
		t.Fatalf("proto.Unmarshal failed: %v", err)
	}

	update := resp.GetUpdate()
	if update == nil || len(update.Participants) != 1 {
		t.Fatalf("expected 1 participant in update, got %+v", resp.Message)
	}

	tracks := update.Participants[0].Tracks
	if len(tracks) != 4 {
		t.Fatalf("expected 4 tracks, got %d", len(tracks))
	}

	var protoAudio, protoVideo, protoSingleVideo, protoNumVideo *livekitproto.TrackInfo
	for _, tr := range tracks {
		switch tr.Sid {
		case "TR_audio_1":
			protoAudio = tr
		case "TR_video_1":
			protoVideo = tr
		case "TR_video_single":
			protoSingleVideo = tr
		case "TR_video_num":
			protoNumVideo = tr
		}
	}

	if protoAudio == nil || protoAudio.MimeType != "audio/opus" {
		t.Fatalf("expected audio track MimeType == audio/opus, got: %+v", protoAudio)
	}
	if len(protoAudio.Codecs) != 1 || protoAudio.Codecs[0].MimeType != "audio/opus" {
		t.Fatalf("expected audio track Codecs[0].MimeType == audio/opus, got: %+v", protoAudio.Codecs)
	}

	if protoVideo == nil || protoVideo.MimeType != "video/vp8" {
		t.Fatalf("expected video track MimeType == video/vp8, got: %+v", protoVideo)
	}
	if len(protoVideo.Codecs) != 1 || protoVideo.Codecs[0].MimeType != "video/vp8" {
		t.Fatalf("expected video track Codecs[0].MimeType == video/vp8, got: %+v", protoVideo.Codecs)
	}
	if len(protoVideo.Codecs[0].Layers) != 3 {
		t.Fatalf("expected 3 layers in video track Codecs[0], got %d", len(protoVideo.Codecs[0].Layers))
	}
	// Verify resolution scaling for q, h, f
	qLayer := protoVideo.Codecs[0].Layers[0]
	if qLayer.Quality != livekitproto.VideoQuality_LOW || qLayer.Width != 320 || qLayer.Height != 180 {
		t.Fatalf("expected qLayer LOW 320x180, got %v %dx%d", qLayer.Quality, qLayer.Width, qLayer.Height)
	}
	hLayer := protoVideo.Codecs[0].Layers[1]
	if hLayer.Quality != livekitproto.VideoQuality_MEDIUM || hLayer.Width != 640 || hLayer.Height != 360 {
		t.Fatalf("expected hLayer MEDIUM 640x360, got %v %dx%d", hLayer.Quality, hLayer.Width, hLayer.Height)
	}
	fLayer := protoVideo.Codecs[0].Layers[2]
	if fLayer.Quality != livekitproto.VideoQuality_HIGH || fLayer.Width != 1280 || fLayer.Height != 720 {
		t.Fatalf("expected fLayer HIGH 1280x720, got %v %dx%d", fLayer.Quality, fLayer.Width, fLayer.Height)
	}

	// Verify single-layer video has default HIGH layer and Simulcast == false
	if protoSingleVideo == nil || protoSingleVideo.MimeType != "video/vp8" {
		t.Fatalf("expected singleVideo MimeType == video/vp8, got: %+v", protoSingleVideo)
	}
	if protoSingleVideo.Simulcast {
		t.Fatalf("expected singleVideo Simulcast == false, got true")
	}
	if len(protoSingleVideo.Layers) != 1 || protoSingleVideo.Layers[0].Quality != livekitproto.VideoQuality_HIGH {
		t.Fatalf("expected 1 HIGH layer for single-layer video, got %+v", protoSingleVideo.Layers)
	}

	// Verify numeric RIDs ("0", "1", "2") map correctly to LOW, MEDIUM, HIGH
	if protoNumVideo == nil || protoNumVideo.MimeType != "video/vp8" {
		t.Fatalf("expected numVideo MimeType == video/vp8, got: %+v", protoNumVideo)
	}
	if len(protoNumVideo.Codecs[0].Layers) != 3 {
		t.Fatalf("expected 3 layers for numeric simulcast track, got %d", len(protoNumVideo.Codecs[0].Layers))
	}
	if protoNumVideo.Codecs[0].Layers[0].Quality != livekitproto.VideoQuality_LOW {
		t.Fatalf("expected RID 0 to map to LOW quality, got %v", protoNumVideo.Codecs[0].Layers[0].Quality)
	}
	if protoNumVideo.Codecs[0].Layers[1].Quality != livekitproto.VideoQuality_MEDIUM {
		t.Fatalf("expected RID 1 to map to MEDIUM quality, got %v", protoNumVideo.Codecs[0].Layers[1].Quality)
	}
	if protoNumVideo.Codecs[0].Layers[2].Quality != livekitproto.VideoQuality_HIGH {
		t.Fatalf("expected RID 2 to map to HIGH quality, got %v", protoNumVideo.Codecs[0].Layers[2].Quality)
	}
}

func TestLiveKitResponder_SendSubscribedQualityUpdate_EmptyCodecDefaultsToVP8_REQ003(t *testing.T) {
	upgrader := websocket.Upgrader{CheckOrigin: func(r *http.Request) bool { return true }}
	serverConnChan := make(chan *websocket.Conn, 1)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		conn, err := upgrader.Upgrade(w, r, nil)
		if err != nil {
			t.Errorf("Upgrade failed: %v", err)
			return
		}
		serverConnChan <- conn
	}))
	defer server.Close()

	wsURL := "ws" + strings.TrimPrefix(server.URL, "http")
	clientConn, _, err := websocket.DefaultDialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatalf("client dial failed: %v", err)
	}
	defer clientConn.Close()

	serverConn := <-serverConnChan
	defer serverConn.Close()

	responder := livekit.NewLiveKitResponder(serverConn)

	qualities := []domain.SubscribedQuality{
		{Quality: domain.VideoQualityHigh, Enabled: true},
	}

	// Empty codec should default to "vp8"
	err = responder.SendSubscribedQualityUpdate("TR_video_test", "", qualities)
	if err != nil {
		t.Fatalf("SendSubscribedQualityUpdate failed: %v", err)
	}

	_, msgBytes, err := clientConn.ReadMessage()
	if err != nil {
		t.Fatalf("ReadMessage failed: %v", err)
	}

	var resp livekitproto.SignalResponse
	if err := proto.Unmarshal(msgBytes, &resp); err != nil {
		t.Fatalf("proto.Unmarshal failed: %v", err)
	}

	squ := resp.GetSubscribedQualityUpdate()
	if squ == nil {
		t.Fatalf("expected SubscribedQualityUpdate in SignalResponse, got: %+v", resp.Message)
	}

	if len(squ.SubscribedCodecs) != 1 {
		t.Fatalf("expected 1 SubscribedCodec, got %d", len(squ.SubscribedCodecs))
	}
	if squ.SubscribedCodecs[0].Codec != "vp8" {
		t.Fatalf("expected SubscribedCodecs[0].Codec == vp8, got %s", squ.SubscribedCodecs[0].Codec)
	}
}
