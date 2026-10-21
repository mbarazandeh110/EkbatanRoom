package native_ortc

import (
	"log/slog"
	"net/http"

	"github.com/gorilla/websocket"

	"ekbatan-room/signaling-coordinator/internal/adapters/outbound/telemetry"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

var upgrader = websocket.Upgrader{
	CheckOrigin: func(r *http.Request) bool {
		return true
	},
}

// NativeOrtcHandler provides the clean skeleton adapter for the native ORTC protocol on /ws/ortc.
// This proves the dual-gateway capability of the hexagonal architecture.
type NativeOrtcHandler struct {
	coordinator ports.RoomCoordinatorPort
}

// NewNativeOrtcHandler constructs the NativeOrtcHandler with dependency injection.
func NewNativeOrtcHandler(coordinator ports.RoomCoordinatorPort) *NativeOrtcHandler {
	return &NativeOrtcHandler{
		coordinator: coordinator,
	}
}

func (h *NativeOrtcHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	conn, err := upgrader.Upgrade(w, r, nil)
	if err != nil {
		slog.Error("native ortc websocket upgrade failed", "error", err)
		telemetry.DefaultMetrics().RecordError("ortc_upgrade_failed")
		return
	}
	defer conn.Close()

	telemetry.DefaultMetrics().IncActiveParticipants("native_ortc")
	defer telemetry.DefaultMetrics().DecActiveParticipants("native_ortc")

	// Skeleton message loop for native ORTC protocol
	for {
		messageType, data, err := conn.ReadMessage()
		if err != nil {
			break
		}
		if messageType == websocket.TextMessage || messageType == websocket.BinaryMessage {
			telemetry.DefaultMetrics().RecordWSMessage("native_ortc", "packet", "ok")
			// Acknowledge received ORTC packet
			_ = conn.WriteMessage(websocket.TextMessage, []byte(`{"type":"ack","len":`+string(rune(len(data)))+`}`))
		}
	}
}
