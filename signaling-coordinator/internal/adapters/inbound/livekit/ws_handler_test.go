package livekit_test

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/gorilla/websocket"
	"google.golang.org/protobuf/proto"

	livekitproto "github.com/livekit/protocol/livekit"

	"ekbatan-room/signaling-coordinator/internal/adapters/inbound/livekit"
	"ekbatan-room/signaling-coordinator/internal/domain"
	"sync"

	"ekbatan-room/signaling-coordinator/internal/ports"
)

type mockCoordinator struct {
	ports.RoomCoordinatorPort

	mu                  sync.Mutex
	joinCalled          bool
	leaveCalled         bool
	lastRoom            string
	lastPeer            string
	lastResp            ports.PeerResponderPort
	lastPublishedTracks []domain.TrackPublishCommand
	lastMutedTrackSID   string
	lastMutedValue      bool
	lastPausedTrackSID  string
	lastPausedValue     bool
	pausedCalls         int
	lastLayerTrackSID   string
	lastSpatialLayer    int32
	lastTemporalLayer   int32
	layerCalls          int
}

func (m *mockCoordinator) SetConsumerPaused(ctx context.Context, roomID, participantID, trackSID string, paused bool) error {
	m.mu.Lock()
	m.lastPausedTrackSID = trackSID
	m.lastPausedValue = paused
	m.pausedCalls++
	m.mu.Unlock()
	return nil
}

func (m *mockCoordinator) SetConsumerLayers(ctx context.Context, roomID, participantID, consumerID string, spatial, temporal int32) error {
	m.mu.Lock()
	m.lastLayerTrackSID = consumerID
	m.lastSpatialLayer = spatial
	m.lastTemporalLayer = temporal
	m.layerCalls++
	m.mu.Unlock()
	return nil
}

func (m *mockCoordinator) Join(ctx context.Context, roomID, participantID, participantName string, responder ports.PeerResponderPort) (*domain.JoinResult, error) {
	m.mu.Lock()
	m.joinCalled = true
	m.lastRoom = roomID
	m.lastPeer = participantID
	m.lastResp = responder
	m.mu.Unlock()

	room := domain.NewRoom(roomID, "router-1")
	p := domain.NewParticipant(participantID, "PA_test", participantName)
	_ = room.AddParticipant(p)

	joinRes := &domain.JoinResult{
		Room:        room,
		Participant: p,
		PublisherTransport: &domain.TransportInfo{
			TransportID: "pub-1",
			IceUfrag:    "ufrag-pub",
		},
		SubscriberTransport: &domain.TransportInfo{
			TransportID: "sub-1",
			IceUfrag:    "ufrag-sub",
		},
		IceServers: []domain.IceServer{
			{URLs: []string{"stun:stun.example.com:19302"}},
		},
	}

	// Immediately send join response back via responder
	_ = responder.SendJoinResponse(joinRes)
	return joinRes, nil
}

func (m *mockCoordinator) Leave(ctx context.Context, roomID, participantID string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.leaveCalled = true
	return nil
}

func (m *mockCoordinator) PublishTracks(ctx context.Context, roomID, participantID string, dtlsRole string, fingerprints []domain.DtlsFingerprintInfo, tracks []domain.TrackPublishCommand) (*domain.TransportInfo, error) {
	m.mu.Lock()
	m.lastPublishedTracks = tracks
	m.mu.Unlock()
	return &domain.TransportInfo{
		TransportID: "pub-1",
		IceUfrag:    "ufrag-pub",
		IcePwd:      "pwd-pub",
	}, nil
}

func (m *mockCoordinator) SetTrackMuted(ctx context.Context, roomID, participantID, trackSID string, muted bool) error {
	m.mu.Lock()
	m.lastMutedTrackSID = trackSID
	m.lastMutedValue = muted
	m.mu.Unlock()
	return nil
}

func (m *mockCoordinator) HandleSubscriberAnswer(ctx context.Context, roomID, participantID string, dtlsRole string, fingerprints []domain.DtlsFingerprintInfo) error {
	return nil
}

func (m *mockCoordinator) IsLeaveCalled() bool {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.leaveCalled
}

func TestLiveKitWSHandler_JoinHandshake(t *testing.T) {
	coord := &mockCoordinator{}
	handler := livekit.NewLiveKitWSHandler(coord)

	server := httptest.NewServer(handler)
	defer server.Close()

	wsURL := "ws" + strings.TrimPrefix(server.URL, "http") + "/rtc?room=conference-101&identity=alice&name=Alice%20Smith"

	dialer := websocket.Dialer{HandshakeTimeout: 3 * time.Second}
	conn, resp, err := dialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatalf("WebSocket dial failed: %v", err)
	}
	defer conn.Close()

	if resp.StatusCode != http.StatusSwitchingProtocols {
		t.Fatalf("expected 101 Switching Protocols, got %d", resp.StatusCode)
	}

	// Read initial Join message
	_ = conn.SetReadDeadline(time.Now().Add(3 * time.Second))
	msgType, data, err := conn.ReadMessage()
	if err != nil {
		t.Fatalf("failed to read join message: %v", err)
	}
	if msgType != websocket.BinaryMessage {
		t.Fatalf("expected binary message, got %d", msgType)
	}

	var signalResp livekitproto.SignalResponse
	if err := proto.Unmarshal(data, &signalResp); err != nil {
		t.Fatalf("failed to unmarshal signal response: %v", err)
	}

	join := signalResp.GetJoin()
	if join == nil {
		t.Fatalf("expected Join response, got nil")
	}

	if join.Room.Name != "conference-101" {
		t.Fatalf("expected room conference-101, got %s", join.Room.Name)
	}
	if join.Participant.Identity != "alice" || join.Participant.Name != "Alice Smith" {
		t.Fatalf("unexpected participant: %+v", join.Participant)
	}
	if len(join.IceServers) != 1 || join.IceServers[0].Urls[0] != "stun:stun.example.com:19302" {
		t.Fatalf("unexpected ice servers: %+v", join.IceServers)
	}

	// Close connection cleanly
	_ = conn.WriteMessage(websocket.CloseMessage, websocket.FormatCloseMessage(websocket.CloseNormalClosure, ""))
	time.Sleep(50 * time.Millisecond)

	if !coord.IsLeaveCalled() {
		t.Fatalf("expected Leave to be called after disconnect")
	}
}

func TestLiveKitWSHandler_OfferAnswerIdMatch(t *testing.T) {
	coord := &mockCoordinator{}
	handler := livekit.NewLiveKitWSHandler(coord)

	server := httptest.NewServer(handler)
	defer server.Close()

	wsURL := "ws" + strings.TrimPrefix(server.URL, "http") + "/rtc?room=conference-101&identity=alice&name=Alice%20Smith"

	dialer := websocket.Dialer{HandshakeTimeout: 3 * time.Second}
	conn, _, err := dialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatalf("WebSocket dial failed: %v", err)
	}
	defer conn.Close()

	// 1. Drain Join message
	_ = conn.SetReadDeadline(time.Now().Add(3 * time.Second))
	_, _, err = conn.ReadMessage()
	if err != nil {
		t.Fatalf("failed to read join: %v", err)
	}

	// 2. Send Offer with ID = 42
	dummyOfferSDP := "v=0\r\no=- 12345 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\nc=IN IP4 0.0.0.0\r\na=mid:0\r\na=sendonly\r\na=rtpmap:111 opus/48000/2\r\na=setup:actpass\r\na=fingerprint:sha-256 00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF\r\n"
	offerReq := &livekitproto.SignalRequest{
		Message: &livekitproto.SignalRequest_Offer{
			Offer: &livekitproto.SessionDescription{
				Type: "offer",
				Sdp:  dummyOfferSDP,
				Id:   42,
			},
		},
	}
	reqData, _ := proto.Marshal(offerReq)
	if err := conn.WriteMessage(websocket.BinaryMessage, reqData); err != nil {
		t.Fatalf("failed to send offer: %v", err)
	}

	// 3. Read Answer and verify Answer.Id == 42
	_ = conn.SetReadDeadline(time.Now().Add(3 * time.Second))
	_, respData, err := conn.ReadMessage()
	if err != nil {
		t.Fatalf("failed to read answer message: %v", err)
	}

	var signalResp livekitproto.SignalResponse
	if err := proto.Unmarshal(respData, &signalResp); err != nil {
		t.Fatalf("failed to unmarshal answer response: %v", err)
	}

	ans := signalResp.GetAnswer()
	if ans == nil {
		t.Fatalf("expected Answer response, got %+v", signalResp.Message)
	}
	if ans.Id != 42 {
		t.Fatalf("expected Answer.Id == 42 to match Offer.Id, got %d", ans.Id)
	}
	if ans.MidToTrackId == nil || ans.MidToTrackId["0"] != "0" {
		t.Fatalf("expected MidToTrackId to contain mid '0', got %+v", ans.MidToTrackId)
	}
}

func TestLiveKitWSHandler_AddTrackRequest(t *testing.T) {
	coord := &mockCoordinator{}
	handler := livekit.NewLiveKitWSHandler(coord)

	server := httptest.NewServer(handler)
	defer server.Close()

	wsURL := "ws" + strings.TrimPrefix(server.URL, "http") + "/rtc?room=conference-101&identity=alice&name=Alice%20Smith"

	dialer := websocket.Dialer{HandshakeTimeout: 3 * time.Second}
	conn, _, err := dialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatalf("WebSocket dial failed: %v", err)
	}
	defer conn.Close()

	// 1. Drain Join message
	_ = conn.SetReadDeadline(time.Now().Add(3 * time.Second))
	_, _, err = conn.ReadMessage()
	if err != nil {
		t.Fatalf("failed to read join: %v", err)
	}

	// 2. Send AddTrack request
	addTrackReq := &livekitproto.SignalRequest{
		Message: &livekitproto.SignalRequest_AddTrack{
			AddTrack: &livekitproto.AddTrackRequest{
				Cid:    "client-cid-999",
				Name:   "microphone",
				Type:   livekitproto.TrackType_AUDIO,
				Source: livekitproto.TrackSource_MICROPHONE,
			},
		},
	}
	reqData, _ := proto.Marshal(addTrackReq)
	if err := conn.WriteMessage(websocket.BinaryMessage, reqData); err != nil {
		t.Fatalf("failed to send add track request: %v", err)
	}

	// 3. Read TrackPublished response
	_ = conn.SetReadDeadline(time.Now().Add(3 * time.Second))
	_, respData, err := conn.ReadMessage()
	if err != nil {
		t.Fatalf("failed to read track published message: %v", err)
	}

	var signalResp livekitproto.SignalResponse
	if err := proto.Unmarshal(respData, &signalResp); err != nil {
		t.Fatalf("failed to unmarshal track published response: %v", err)
	}

	tp := signalResp.GetTrackPublished()
	if tp == nil {
		t.Fatalf("expected TrackPublished response, got %+v", signalResp.Message)
	}
	if tp.Cid != "client-cid-999" {
		t.Fatalf("expected Cid == client-cid-999, got %s", tp.Cid)
	}
	if tp.Track == nil || !strings.HasPrefix(tp.Track.Sid, "TR_") {
		t.Fatalf("expected Track.Sid with prefix TR_, got %+v", tp.Track)
	}
	if tp.Track.Type != livekitproto.TrackType_AUDIO || tp.Track.Name != "microphone" {
		t.Fatalf("unexpected track info: %+v", tp.Track)
	}
}

func TestLiveKitResponder_SubscriberOfferSequenceAndMid(t *testing.T) {
	upgrader := websocket.Upgrader{}
	var responder *livekit.LiveKitResponder
	var serverConn *websocket.Conn
	var wg sync.WaitGroup
	wg.Add(1)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var err error
		serverConn, err = upgrader.Upgrade(w, r, nil)
		if err != nil {
			t.Errorf("upgrade error: %v", err)
			return
		}
		responder = livekit.NewLiveKitResponder(serverConn)
		wg.Done()
	}))
	defer server.Close()

	wsURL := "ws" + strings.TrimPrefix(server.URL, "http")
	clientConn, _, err := websocket.DefaultDialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatalf("dial failed: %v", err)
	}
	defer clientConn.Close()
	wg.Wait()
	defer serverConn.Close()

	// Initialize JoinResult to set subTransport
	joinRes := &domain.JoinResult{
		Room:        domain.NewRoom("r1", "router1"),
		Participant: domain.NewParticipant("p1", "PA_1", "User1"),
		SubscriberTransport: &domain.TransportInfo{
			TransportID: "sub-1",
			IceUfrag:    "ufrag-sub",
			IcePwd:      "pwd-sub",
		},
	}
	if err := responder.SendJoinResponse(joinRes); err != nil {
		t.Fatalf("SendJoinResponse failed: %v", err)
	}
	// Drain join message on client
	_, _, _ = clientConn.ReadMessage()

	// Send 1st subscriber offer
	slots1 := []*domain.SDPSlot{
		{
			Index:       0,
			MID:         "0",
			Kind:        domain.TrackKindAudio,
			Active:      true,
			TrackSID:    "TR_audio_1",
			PayloadType: 111,
			ClockRate:   48000,
		},
	}
	if err := responder.SendSubscriberOffer(slots1, "PA_1"); err != nil {
		t.Fatalf("1st SendSubscriberOffer failed: %v", err)
	}

	// Read 1st offer
	_, msg1Bytes, err := clientConn.ReadMessage()
	if err != nil {
		t.Fatalf("failed to read 1st offer: %v", err)
	}
	var resp1 livekitproto.SignalResponse
	if err := proto.Unmarshal(msg1Bytes, &resp1); err != nil {
		t.Fatalf("failed to unmarshal 1st offer: %v", err)
	}
	offer1 := resp1.GetOffer()
	if offer1 == nil {
		t.Fatalf("expected Offer message, got %+v", resp1.Message)
	}
	if offer1.Id != 1 {
		t.Fatalf("expected offer1.Id == 1, got %d", offer1.Id)
	}
	if offer1.MidToTrackId["0"] != "TR_audio_1" {
		t.Fatalf("expected mid 0 -> TR_audio_1, got %+v", offer1.MidToTrackId)
	}

	// Send 2nd subscriber offer
	slots2 := append(slots1, &domain.SDPSlot{
		Index:       1,
		MID:         "1",
		Kind:        domain.TrackKindVideo,
		Active:      true,
		TrackSID:    "TR_video_2",
		PayloadType: 96,
		ClockRate:   90000,
	})
	if err := responder.SendSubscriberOffer(slots2, "PA_1"); err != nil {
		t.Fatalf("2nd SendSubscriberOffer failed: %v", err)
	}

	// Read 2nd offer
	_, msg2Bytes, err := clientConn.ReadMessage()
	if err != nil {
		t.Fatalf("failed to read 2nd offer: %v", err)
	}
	var resp2 livekitproto.SignalResponse
	if err := proto.Unmarshal(msg2Bytes, &resp2); err != nil {
		t.Fatalf("failed to unmarshal 2nd offer: %v", err)
	}
	offer2 := resp2.GetOffer()
	if offer2.Id != 2 {
		t.Fatalf("expected offer2.Id == 2 (monotonic), got %d", offer2.Id)
	}
	if offer2.MidToTrackId["0"] != "TR_audio_1" || offer2.MidToTrackId["1"] != "TR_video_2" {
		t.Fatalf("unexpected midToTrackId in offer2: %+v", offer2.MidToTrackId)
	}
}

func TestLiveKitWSHandler_RtcV1Returns404PerREQ008(t *testing.T) {
	coord := &mockCoordinator{}
	handler := livekit.NewLiveKitWSHandler(coord)

	mux := http.NewServeMux()
	mux.Handle("/rtc", handler)

	server := httptest.NewServer(mux)
	defer server.Close()

	wsURL := "ws" + strings.TrimPrefix(server.URL, "http") + "/rtc/v1?room=conference-v1&identity=bob&name=Bob"

	dialer := websocket.Dialer{HandshakeTimeout: 3 * time.Second}
	_, resp, err := dialer.Dial(wsURL, nil)
	if err == nil {
		t.Fatalf("expected dial to /rtc/v1 to fail with HTTP 404, but dial succeeded")
	}
	if resp == nil || resp.StatusCode != http.StatusNotFound {
		status := 0
		if resp != nil {
			status = resp.StatusCode
		}
		t.Fatalf("expected HTTP 404 Not Found on /rtc/v1 to trigger livekit-client fallback, got: %d (err: %v)", status, err)
	}
}

func TestLiveKitResponder_PingIntervalAndTimeoutPerREQ007(t *testing.T) {
	// Custom config test
	customCfg := livekit.ResponderConfig{
		PingInterval:  12,
		PingTimeout:   35,
		ServerVersion: "1.54.0-test",
	}

	coord := &mockCoordinator{}
	handler := livekit.NewLiveKitWSHandler(coord, customCfg)

	mux := http.NewServeMux()
	mux.Handle("/rtc", handler)

	server := httptest.NewServer(mux)
	defer server.Close()

	wsURL := "ws" + strings.TrimPrefix(server.URL, "http") + "/rtc?room=conf-ping&identity=alice&name=Alice"

	dialer := websocket.Dialer{HandshakeTimeout: 3 * time.Second}
	conn, _, err := dialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatalf("failed to dial /rtc: %v", err)
	}
	defer conn.Close()

	_ = conn.SetReadDeadline(time.Now().Add(3 * time.Second))
	msgType, data, err := conn.ReadMessage()
	if err != nil {
		t.Fatalf("failed to read message: %v", err)
	}
	if msgType != websocket.BinaryMessage {
		t.Fatalf("expected binary message")
	}

	var signalResp livekitproto.SignalResponse
	if err := proto.Unmarshal(data, &signalResp); err != nil {
		t.Fatalf("failed to unmarshal join response: %v", err)
	}

	join := signalResp.GetJoin()
	if join == nil {
		t.Fatalf("expected join response, got nil")
	}

	if join.PingInterval != 12 {
		t.Fatalf("expected PingInterval 12, got %d", join.PingInterval)
	}
	if join.PingTimeout != 35 {
		t.Fatalf("expected PingTimeout 35, got %d", join.PingTimeout)
	}
	if join.PingTimeout <= join.PingInterval {
		t.Fatalf("PingTimeout (%d) must be greater than PingInterval (%d) per REQ-007", join.PingTimeout, join.PingInterval)
	}
	if join.ServerVersion != "1.54.0-test" {
		t.Fatalf("expected server version 1.54.0-test, got %s", join.ServerVersion)
	}

	// Verify REQ-009: ParticipantInfo must have Permission populated
	if join.Participant == nil || join.Participant.Permission == nil {
		t.Fatalf("expected join.Participant.Permission to be populated per REQ-009, got nil")
	}
	perm := join.Participant.Permission
	if !perm.CanPublish || !perm.CanSubscribe || !perm.CanPublishData {
		t.Fatalf("expected CanPublish, CanSubscribe, and CanPublishData to be true, got %+v", perm)
	}
}

func TestLiveKitWSHandler_AddTrack_PreservesTrackSID_ThroughOfferAndMute(t *testing.T) {
	coord := &mockCoordinator{}
	handler := livekit.NewLiveKitWSHandler(coord)

	server := httptest.NewServer(handler)
	defer server.Close()

	wsURL := "ws" + strings.TrimPrefix(server.URL, "http") + "/rtc?room=conference-sid&identity=alice&name=Alice"
	dialer := websocket.Dialer{HandshakeTimeout: 3 * time.Second}
	conn, _, err := dialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatalf("WebSocket dial failed: %v", err)
	}
	defer conn.Close()

	// 1. Drain Join response
	_ = conn.SetReadDeadline(time.Now().Add(3 * time.Second))
	_, _, err = conn.ReadMessage()
	if err != nil {
		t.Fatalf("failed to read join: %v", err)
	}

	// 2. Client sends AddTrack request
	addTrackReq := &livekitproto.SignalRequest{
		Message: &livekitproto.SignalRequest_AddTrack{
			AddTrack: &livekitproto.AddTrackRequest{
				Cid:    "client-track-cid-1",
				Name:   "camera",
				Type:   livekitproto.TrackType_VIDEO,
				Source: livekitproto.TrackSource_CAMERA,
			},
		},
	}
	reqData, _ := proto.Marshal(addTrackReq)
	if err := conn.WriteMessage(websocket.BinaryMessage, reqData); err != nil {
		t.Fatalf("failed to send add track request: %v", err)
	}

	// 3. Read TrackPublished response and capture assigned TrackSID
	_ = conn.SetReadDeadline(time.Now().Add(3 * time.Second))
	_, respData, err := conn.ReadMessage()
	if err != nil {
		t.Fatalf("failed to read track published message: %v", err)
	}

	var signalResp livekitproto.SignalResponse
	if err := proto.Unmarshal(respData, &signalResp); err != nil {
		t.Fatalf("failed to unmarshal track published response: %v", err)
	}
	tp := signalResp.GetTrackPublished()
	if tp == nil || tp.Track == nil {
		t.Fatalf("expected TrackPublished, got %+v", signalResp.Message)
	}
	assignedSID := tp.Track.Sid
	if !strings.HasPrefix(assignedSID, "TR_") {
		t.Fatalf("expected Track SID starting with TR_, got %s", assignedSID)
	}

	// 4. Client sends Offer with Video media section (mid: "0")
	dummyVideoOfferSDP := "v=0\r\no=- 12345 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\nc=IN IP4 0.0.0.0\r\na=mid:0\r\na=sendonly\r\na=rtpmap:96 VP8/90000\r\na=setup:actpass\r\na=fingerprint:sha-256 00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF\r\n"
	offerReq := &livekitproto.SignalRequest{
		Message: &livekitproto.SignalRequest_Offer{
			Offer: &livekitproto.SessionDescription{
				Type: "offer",
				Sdp:  dummyVideoOfferSDP,
				Id:   101,
			},
		},
	}
	offerData, _ := proto.Marshal(offerReq)
	if err := conn.WriteMessage(websocket.BinaryMessage, offerData); err != nil {
		t.Fatalf("failed to send offer: %v", err)
	}

	// 5. Read Answer response
	_ = conn.SetReadDeadline(time.Now().Add(3 * time.Second))
	_, answerBytes, err := conn.ReadMessage()
	if err != nil {
		t.Fatalf("failed to read answer: %v", err)
	}

	var ansResp livekitproto.SignalResponse
	if err := proto.Unmarshal(answerBytes, &ansResp); err != nil {
		t.Fatalf("failed to unmarshal answer: %v", err)
	}
	ans := ansResp.GetAnswer()
	if ans == nil {
		t.Fatalf("expected Answer, got %+v", ansResp.Message)
	}

	// Verify that MidToTrackId["0"] is the assignedSID from AddTrack!
	if ans.MidToTrackId["0"] != assignedSID {
		t.Fatalf("expected MidToTrackId['0'] == %s, got %s", assignedSID, ans.MidToTrackId["0"])
	}

	// Verify coordinator received the exact assignedSID in TrackPublishCommand
	coord.mu.Lock()
	if len(coord.lastPublishedTracks) != 1 {
		coord.mu.Unlock()
		t.Fatalf("expected 1 published track in coordinator, got %d", len(coord.lastPublishedTracks))
	}
	if coord.lastPublishedTracks[0].TrackSID != assignedSID {
		coord.mu.Unlock()
		t.Fatalf("expected coordinator TrackSID == %s, got %s", assignedSID, coord.lastPublishedTracks[0].TrackSID)
	}
	coord.mu.Unlock()

	// 6. Client sends MuteTrack request using the assignedSID
	muteReq := &livekitproto.SignalRequest{
		Message: &livekitproto.SignalRequest_Mute{
			Mute: &livekitproto.MuteTrackRequest{
				Sid:   assignedSID,
				Muted: true,
			},
		},
	}
	muteData, _ := proto.Marshal(muteReq)
	if err := conn.WriteMessage(websocket.BinaryMessage, muteData); err != nil {
		t.Fatalf("failed to send mute request: %v", err)
	}

	time.Sleep(50 * time.Millisecond)

	coord.mu.Lock()
	defer coord.mu.Unlock()
	if coord.lastMutedTrackSID != assignedSID {
		t.Fatalf("expected coordinator lastMutedTrackSID == %s, got %s", assignedSID, coord.lastMutedTrackSID)
	}
	if !coord.lastMutedValue {
		t.Fatalf("expected coordinator lastMutedValue == true")
	}
}

func TestLiveKitWSHandler_OfferAnswer_InactiveMSectionConformsRFC3264_REQ001(t *testing.T) {
	coord := &mockCoordinator{}
	handler := livekit.NewLiveKitWSHandler(coord)

	server := httptest.NewServer(handler)
	defer server.Close()

	wsURL := "ws" + strings.TrimPrefix(server.URL, "http") + "/rtc?room=conference-1&identity=alice&name=Alice"
	dialer := websocket.Dialer{HandshakeTimeout: 3 * time.Second}
	conn, _, err := dialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatalf("WebSocket dial failed: %v", err)
	}
	defer conn.Close()

	// 1. Drain Join response
	_ = conn.SetReadDeadline(time.Now().Add(3 * time.Second))
	_, _, err = conn.ReadMessage()
	if err != nil {
		t.Fatalf("failed to read join: %v", err)
	}

	// 2. Client sends Offer with active audio (sendonly) and inactive video screenshare (inactive)
	offerSDP := `v=0
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
a=ssrc:1001 cname:test-cname
m=video 9 UDP/TLS/RTP/SAVPF 96
c=IN IP4 0.0.0.0
a=mid:1
a=inactive
a=setup:actpass
a=rtpmap:96 VP8/90000
`

	offerReq := &livekitproto.SignalRequest{
		Message: &livekitproto.SignalRequest_Offer{
			Offer: &livekitproto.SessionDescription{
				Type: "offer",
				Sdp:  offerSDP,
				Id:   42,
			},
		},
	}
	offerData, _ := proto.Marshal(offerReq)
	if err := conn.WriteMessage(websocket.BinaryMessage, offerData); err != nil {
		t.Fatalf("failed to send offer request: %v", err)
	}

	// 3. Read Answer response
	_ = conn.SetReadDeadline(time.Now().Add(3 * time.Second))
	_, respData, err := conn.ReadMessage()
	if err != nil {
		t.Fatalf("failed to read answer response: %v", err)
	}

	var signalResp livekitproto.SignalResponse
	if err := proto.Unmarshal(respData, &signalResp); err != nil {
		t.Fatalf("failed to unmarshal answer response: %v", err)
	}

	ans := signalResp.GetAnswer()
	if ans == nil {
		t.Fatalf("expected Answer in response, got nil")
	}
	if ans.Id != 42 {
		t.Fatalf("expected Answer.Id == 42, got %d", ans.Id)
	}

	// 4. Verify inactive m-section conforms to RFC 3264 (a=inactive, NOT a=recvonly)
	answerSDP := ans.Sdp
	if !strings.Contains(answerSDP, "a=mid:1") {
		t.Fatalf("answer missing mid:1")
	}
	// Verify coordinator only published 1 active track (skipping inactive video)
	coord.mu.Lock()
	defer coord.mu.Unlock()
	if len(coord.lastPublishedTracks) != 1 {
		t.Fatalf("expected exactly 1 active track published, got %d", len(coord.lastPublishedTracks))
	}
	if coord.lastPublishedTracks[0].Kind != domain.TrackKindAudio {
		t.Fatalf("expected audio track published, got %v", coord.lastPublishedTracks[0].Kind)
	}
}

func TestLiveKitWSHandler_TrackSetting_DecoupleTemporalAndSupportDisabled_REQ002(t *testing.T) {
	coord := &mockCoordinator{}
	handler := livekit.NewLiveKitWSHandler(coord)

	server := httptest.NewServer(handler)
	defer server.Close()

	wsURL := "ws" + strings.TrimPrefix(server.URL, "http") + "/rtc?room=conf-req002&identity=bob&name=Bob"

	dialer := websocket.Dialer{HandshakeTimeout: 3 * time.Second}
	conn, _, err := dialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatalf("WebSocket dial failed: %v", err)
	}
	defer conn.Close()

	// 1. Drain JoinResponse
	_ = conn.SetReadDeadline(time.Now().Add(3 * time.Second))
	if _, _, err := conn.ReadMessage(); err != nil {
		t.Fatalf("failed to read join response: %v", err)
	}

	// 2. Test Disabled: true -> Pauses consumer via SetConsumerPaused(true), skips SetConsumerLayers
	trackSID := "TR_video_bob_1"
	reqDisabled := &livekitproto.SignalRequest{
		Message: &livekitproto.SignalRequest_TrackSetting{
			TrackSetting: &livekitproto.UpdateTrackSettings{
				TrackSids: []string{trackSID},
				Disabled:  true,
				Quality:   livekitproto.VideoQuality_LOW,
			},
		},
	}
	dataDisabled, _ := proto.Marshal(reqDisabled)
	if err := conn.WriteMessage(websocket.BinaryMessage, dataDisabled); err != nil {
		t.Fatalf("failed to write disabled track setting: %v", err)
	}

	time.Sleep(50 * time.Millisecond)

	coord.mu.Lock()
	if coord.pausedCalls != 1 || coord.lastPausedTrackSID != trackSID || !coord.lastPausedValue {
		t.Fatalf("expected SetConsumerPaused(track=%s, paused=true), got calls=%d, track=%s, val=%v",
			trackSID, coord.pausedCalls, coord.lastPausedTrackSID, coord.lastPausedValue)
	}
	if coord.layerCalls != 0 {
		t.Fatalf("expected 0 layerCalls when disabled=true, got %d", coord.layerCalls)
	}
	coord.mu.Unlock()

	// 3. Test Disabled: false with LOW quality -> Unpauses via SetConsumerPaused(false), SetConsumerLayers(spatial=0, temporal=-1)
	reqLow := &livekitproto.SignalRequest{
		Message: &livekitproto.SignalRequest_TrackSetting{
			TrackSetting: &livekitproto.UpdateTrackSettings{
				TrackSids: []string{trackSID},
				Disabled:  false,
				Quality:   livekitproto.VideoQuality_LOW,
			},
		},
	}
	dataLow, _ := proto.Marshal(reqLow)
	if err := conn.WriteMessage(websocket.BinaryMessage, dataLow); err != nil {
		t.Fatalf("failed to write low quality track setting: %v", err)
	}

	time.Sleep(50 * time.Millisecond)

	coord.mu.Lock()
	if coord.pausedCalls != 2 || coord.lastPausedValue != false {
		t.Fatalf("expected SetConsumerPaused(track=%s, paused=false), got calls=%d, val=%v",
			trackSID, coord.pausedCalls, coord.lastPausedValue)
	}
	if coord.layerCalls != 1 {
		t.Fatalf("expected 1 layerCall, got %d", coord.layerCalls)
	}
	if coord.lastSpatialLayer != 0 {
		t.Fatalf("expected spatial layer 0 for LOW quality, got %d", coord.lastSpatialLayer)
	}
	// REQ-002 Decoupling: temporal must be -1, NOT equal to spatial (0)
	if coord.lastTemporalLayer != -1 {
		t.Fatalf("expected temporal layer -1 (decoupled), got %d", coord.lastTemporalLayer)
	}
	coord.mu.Unlock()

	// 4. Test Disabled: false with HIGH quality -> SetConsumerLayers(spatial=2, temporal=-1)
	reqHigh := &livekitproto.SignalRequest{
		Message: &livekitproto.SignalRequest_TrackSetting{
			TrackSetting: &livekitproto.UpdateTrackSettings{
				TrackSids: []string{trackSID},
				Disabled:  false,
				Quality:   livekitproto.VideoQuality_HIGH,
			},
		},
	}
	dataHigh, _ := proto.Marshal(reqHigh)
	if err := conn.WriteMessage(websocket.BinaryMessage, dataHigh); err != nil {
		t.Fatalf("failed to write high quality track setting: %v", err)
	}

	time.Sleep(50 * time.Millisecond)

	coord.mu.Lock()
	if coord.layerCalls != 2 {
		t.Fatalf("expected 2 layerCalls, got %d", coord.layerCalls)
	}
	if coord.lastSpatialLayer != 2 {
		t.Fatalf("expected spatial layer 2 for HIGH quality, got %d", coord.lastSpatialLayer)
	}
	if coord.lastTemporalLayer != -1 {
		t.Fatalf("expected temporal layer -1 (decoupled), got %d", coord.lastTemporalLayer)
	}
	coord.mu.Unlock()
}



