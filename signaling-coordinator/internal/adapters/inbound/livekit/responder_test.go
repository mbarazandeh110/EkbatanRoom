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

