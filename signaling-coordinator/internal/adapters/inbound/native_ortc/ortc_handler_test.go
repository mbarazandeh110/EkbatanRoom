package native_ortc_test

import (
	"encoding/json"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/gorilla/websocket"

	"ekbatan-room/signaling-coordinator/internal/adapters/inbound/native_ortc"
)

func TestNativeOrtcHandler_EchoAckPacket(t *testing.T) {
	handler := native_ortc.NewNativeOrtcHandler(nil)
	server := httptest.NewServer(handler)
	defer server.Close()

	wsURL := "ws" + strings.TrimPrefix(server.URL, "http") + "/ws/ortc"
	dialer := websocket.Dialer{HandshakeTimeout: 3 * time.Second}
	conn, _, err := dialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatalf("WebSocket dial to /ws/ortc failed: %v", err)
	}
	defer conn.Close()

	payload := "ping-payload-data"
	if err := conn.WriteMessage(websocket.TextMessage, []byte(payload)); err != nil {
		t.Fatalf("failed to write message: %v", err)
	}

	_ = conn.SetReadDeadline(time.Now().Add(2 * time.Second))
	msgType, data, err := conn.ReadMessage()
	if err != nil {
		t.Fatalf("failed to read ack message: %v", err)
	}
	if msgType != websocket.TextMessage {
		t.Fatalf("expected TextMessage, got %d", msgType)
	}

	var ack struct {
		Type string `json:"type"`
		Len  int    `json:"len"`
	}
	if err := json.Unmarshal(data, &ack); err != nil {
		t.Fatalf("failed to parse ack JSON: %v, raw: %s", err, string(data))
	}

	if ack.Type != "ack" {
		t.Fatalf("expected type 'ack', got %s", ack.Type)
	}
	if ack.Len != len(payload) {
		t.Fatalf("expected len %d, got %d", len(payload), ack.Len)
	}
}
