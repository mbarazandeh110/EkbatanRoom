package native_ortc_test

import (
	"context"
	"encoding/json"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/gorilla/websocket"

	"ekbatan-room/signaling-coordinator/internal/adapters/inbound/native_ortc"
	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

type mockOrtcCoordinator struct {
	ports.RoomCoordinatorPort

	mu       sync.Mutex
	lastRoom string
	lastPeer string
	lastRole string
}

func (m *mockOrtcCoordinator) Join(ctx context.Context, roomID, participantID, participantName string, responder ports.PeerResponderPort) (*domain.JoinResult, error) {
	m.mu.Lock()
	m.lastRoom = roomID
	m.lastPeer = participantID
	m.mu.Unlock()

	return &domain.JoinResult{
		Room:        domain.NewRoom(roomID, "router-1"),
		Participant: domain.NewParticipant(participantID, "PA_1", participantName),
		PublisherTransport: &domain.TransportInfo{
			TransportID: "pub-tp-1",
			IceUfrag:    "ufrag-pub",
		},
		SubscriberTransport: &domain.TransportInfo{
			TransportID: "sub-tp-1",
			IceUfrag:    "ufrag-sub",
		},
	}, nil
}

func (m *mockOrtcCoordinator) Leave(ctx context.Context, roomID, participantID string) error {
	return nil
}

func (m *mockOrtcCoordinator) PublishTracks(ctx context.Context, roomID, participantID string, dtlsRole string, fps []domain.DtlsFingerprintInfo, tracks []domain.TrackPublishCommand) (*domain.TransportInfo, error) {
	m.mu.Lock()
	m.lastRole = dtlsRole
	m.mu.Unlock()
	return &domain.TransportInfo{
		TransportID: "pub-tp-1",
	}, nil
}

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

func TestNativeOrtcHandler_GetRouterCapabilitiesAndJoin(t *testing.T) {
	coord := &mockOrtcCoordinator{}
	handler := native_ortc.NewNativeOrtcHandler(coord)
	server := httptest.NewServer(handler)
	defer server.Close()

	wsURL := "ws" + strings.TrimPrefix(server.URL, "http") + "/ws/ortc"
	dialer := websocket.Dialer{HandshakeTimeout: 3 * time.Second}
	conn, _, err := dialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatalf("WebSocket dial failed: %v", err)
	}
	defer conn.Close()

	// 1. Request getRouterCapabilities
	req1 := native_ortc.OrtcRequest{
		ID:     "req-1",
		Method: "getRouterCapabilities",
	}
	reqData, _ := json.Marshal(req1)
	if err := conn.WriteMessage(websocket.TextMessage, reqData); err != nil {
		t.Fatalf("failed to send getRouterCapabilities: %v", err)
	}

	_ = conn.SetReadDeadline(time.Now().Add(2 * time.Second))
	_, respData, err := conn.ReadMessage()
	if err != nil {
		t.Fatalf("failed to read response: %v", err)
	}

	var resp1 native_ortc.OrtcResponse
	if err := json.Unmarshal(respData, &resp1); err != nil {
		t.Fatalf("failed to unmarshal response: %v", err)
	}
	if !resp1.OK || resp1.ID != "req-1" {
		t.Fatalf("expected successful response for req-1, got %+v", resp1)
	}

	// 2. Request join
	joinData, _ := json.Marshal(map[string]string{
		"roomId": "classroom-101",
		"peerId": "student-alice",
		"name":   "Alice",
	})
	req2 := native_ortc.OrtcRequest{
		ID:     "req-2",
		Method: "join",
		Data:   joinData,
	}
	req2Data, _ := json.Marshal(req2)
	if err := conn.WriteMessage(websocket.TextMessage, req2Data); err != nil {
		t.Fatalf("failed to send join: %v", err)
	}

	_ = conn.SetReadDeadline(time.Now().Add(2 * time.Second))
	_, resp2Data, err := conn.ReadMessage()
	if err != nil {
		t.Fatalf("failed to read join response: %v", err)
	}

	var resp2 native_ortc.OrtcResponse
	if err := json.Unmarshal(resp2Data, &resp2); err != nil {
		t.Fatalf("failed to unmarshal join response: %v", err)
	}
	if !resp2.OK || resp2.ID != "req-2" {
		t.Fatalf("expected successful join response, got %+v", resp2)
	}

	coord.mu.Lock()
	joinedRoom := coord.lastRoom
	joinedPeer := coord.lastPeer
	coord.mu.Unlock()

	if joinedRoom != "classroom-101" || joinedPeer != "student-alice" {
		t.Fatalf("expected room classroom-101 and peer student-alice, got room=%s peer=%s", joinedRoom, joinedPeer)
	}

	// 3. Produce Track
	produceData, _ := json.Marshal(map[string]interface{}{
		"kind":        "audio",
		"ssrc":        12345,
		"payloadType": 111,
		"codec":       "opus",
	})
	req3 := native_ortc.OrtcRequest{
		ID:     "req-3",
		Method: "produce",
		Data:   produceData,
	}
	req3Data, _ := json.Marshal(req3)
	if err := conn.WriteMessage(websocket.TextMessage, req3Data); err != nil {
		t.Fatalf("failed to send produce: %v", err)
	}

	_ = conn.SetReadDeadline(time.Now().Add(2 * time.Second))
	_, resp3Data, err := conn.ReadMessage()
	if err != nil {
		t.Fatalf("failed to read produce response: %v", err)
	}
	var resp3 native_ortc.OrtcResponse
	if err := json.Unmarshal(resp3Data, &resp3); err != nil {
		t.Fatalf("failed to unmarshal produce response: %v", err)
	}
	if !resp3.OK || resp3.ID != "req-3" {
		t.Fatalf("expected successful produce response, got %+v", resp3)
	}
}

func TestNativeOrtcHandler_ClassroomModeration(t *testing.T) {
	coord := &mockOrtcCoordinator{}
	handler := native_ortc.NewNativeOrtcHandler(coord)
	server := httptest.NewServer(handler)
	defer server.Close()

	wsURL := "ws" + strings.TrimPrefix(server.URL, "http") + "/ws/ortc"
	dialer := websocket.Dialer{HandshakeTimeout: 3 * time.Second}
	conn, _, err := dialer.Dial(wsURL, nil)
	if err != nil {
		t.Fatalf("WebSocket dial failed: %v", err)
	}
	defer conn.Close()

	// 1. Lock Room
	reqLock := native_ortc.OrtcRequest{
		ID:     "lock-1",
		Method: "lockRoom",
	}
	data, _ := json.Marshal(reqLock)
	_ = conn.WriteMessage(websocket.TextMessage, data)
	_, respData, _ := conn.ReadMessage()
	var resLock native_ortc.OrtcResponse
	_ = json.Unmarshal(respData, &resLock)
	if !resLock.OK {
		t.Fatalf("expected lockRoom OK, got %+v", resLock)
	}

	// 2. Switch Breakout Room
	reqBreakout := native_ortc.OrtcRequest{
		ID:     "breakout-1",
		Method: "switchBreakoutRoom",
		Data:   json.RawMessage(`{"breakoutRoomId":"group-3"}`),
	}
	data, _ = json.Marshal(reqBreakout)
	_ = conn.WriteMessage(websocket.TextMessage, data)
	_, respData, _ = conn.ReadMessage()
	var resBreakout native_ortc.OrtcResponse
	_ = json.Unmarshal(respData, &resBreakout)
	if !resBreakout.OK {
		t.Fatalf("expected switchBreakoutRoom OK, got %+v", resBreakout)
	}

	// 3. Sync Layout
	reqLayout := native_ortc.OrtcRequest{
		ID:     "layout-1",
		Method: "syncLayout",
		Data:   json.RawMessage(`{"layout":"presentation"}`),
	}
	data, _ = json.Marshal(reqLayout)
	_ = conn.WriteMessage(websocket.TextMessage, data)
	_, respData, _ = conn.ReadMessage()
	var resLayout native_ortc.OrtcResponse
	_ = json.Unmarshal(respData, &resLayout)
	if !resLayout.OK {
		t.Fatalf("expected syncLayout OK, got %+v", resLayout)
	}

	// 4. Activity Ping
	reqPing := native_ortc.OrtcRequest{
		ID:     "ping-1",
		Method: "activityPing",
	}
	data, _ = json.Marshal(reqPing)
	_ = conn.WriteMessage(websocket.TextMessage, data)
	_, respData, _ = conn.ReadMessage()
	var resPing native_ortc.OrtcResponse
	_ = json.Unmarshal(respData, &resPing)
	if !resPing.OK {
		t.Fatalf("expected activityPing OK, got %+v", resPing)
	}
}
