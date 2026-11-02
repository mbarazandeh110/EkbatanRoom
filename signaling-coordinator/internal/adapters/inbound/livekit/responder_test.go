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
