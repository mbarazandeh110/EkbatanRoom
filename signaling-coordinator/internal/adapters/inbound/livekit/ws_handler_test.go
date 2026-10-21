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

	mu          sync.Mutex
	joinCalled  bool
	leaveCalled bool
	lastRoom    string
	lastPeer    string
	lastResp    ports.PeerResponderPort
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
	return &domain.TransportInfo{
		TransportID: "pub-1",
		IceUfrag:    "ufrag-pub",
		IcePwd:      "pwd-pub",
	}, nil
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
