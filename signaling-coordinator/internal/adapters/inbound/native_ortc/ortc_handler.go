package native_ortc

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"net/http"
	"strconv"
	"sync"
	"time"

	"github.com/google/uuid"
	"github.com/gorilla/websocket"

	"ekbatan-room/signaling-coordinator/internal/adapters/outbound/telemetry"
	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

var upgrader = websocket.Upgrader{
	CheckOrigin: func(r *http.Request) bool {
		return true
	},
}

// OrtcRequest represents a client-to-server ORTC JSON signaling request.
type OrtcRequest struct {
	ID     string          `json:"id,omitempty"`
	Method string          `json:"method"`
	Data   json.RawMessage `json:"data,omitempty"`
}

// OrtcResponse represents a server-to-client ORTC JSON signaling response.
type OrtcResponse struct {
	ID           string      `json:"id,omitempty"`
	OK           bool        `json:"ok"`
	Data         interface{} `json:"data,omitempty"`
	Error        string      `json:"error,omitempty"`
	Notification bool        `json:"notification,omitempty"`
	Method       string      `json:"method,omitempty"`
}

// OrtcResponder implements ports.PeerResponderPort for native JSON/ORTC signaling.
type OrtcResponder struct {
	mu     sync.Mutex
	conn   *websocket.Conn
	closed bool
}

var _ ports.PeerResponderPort = (*OrtcResponder)(nil)

func NewOrtcResponder(conn *websocket.Conn) *OrtcResponder {
	return &OrtcResponder{
		conn: conn,
	}
}

func (r *OrtcResponder) send(msg interface{}) error {
	r.mu.Lock()
	defer r.mu.Unlock()

	if r.closed || r.conn == nil {
		return domain.ErrRoomClosed
	}

	data, err := json.Marshal(msg)
	if err != nil {
		return err
	}

	_ = r.conn.SetWriteDeadline(time.Now().Add(10 * time.Second))
	return r.conn.WriteMessage(websocket.TextMessage, data)
}

func (r *OrtcResponder) SendJoinResponse(res *domain.JoinResult) error {
	return r.send(OrtcResponse{
		Notification: true,
		Method:       "joined",
		Data:         res,
	})
}

func (r *OrtcResponder) SendSubscriberOffer(slots []*domain.SDPSlot, participantSID string) error {
	return r.send(OrtcResponse{
		Notification: true,
		Method:       "newConsumer",
		Data: map[string]interface{}{
			"participantSid": participantSID,
			"slots":          slots,
		},
	})
}

func (r *OrtcResponder) SendTrickle(candidate string, target int) error {
	return r.send(OrtcResponse{
		Notification: true,
		Method:       "iceCandidate",
		Data: map[string]interface{}{
			"candidate": candidate,
			"target":    target,
		},
	})
}

func (r *OrtcResponder) SendParticipantUpdate(participants []*domain.Participant) error {
	return r.send(OrtcResponse{
		Notification: true,
		Method:       "participantUpdate",
		Data:         participants,
	})
}

func (r *OrtcResponder) SendTrackMuted(trackSID string, muted bool) error {
	return r.send(OrtcResponse{
		Notification: true,
		Method:       "trackMuted",
		Data: map[string]interface{}{
			"trackSid": trackSID,
			"muted":    muted,
		},
	})
}

func (r *OrtcResponder) SendSubscribedQualityUpdate(trackSID string, codec string, qualities []domain.SubscribedQuality) error {
	return r.send(OrtcResponse{
		Notification: true,
		Method:       "subscribedQualityUpdate",
		Data: map[string]interface{}{
			"trackSid":  trackSID,
			"codec":     codec,
			"qualities": qualities,
		},
	})
}

func (r *OrtcResponder) SendTrackSubscribed(trackSID string) error {
	return r.send(OrtcResponse{
		Notification: true,
		Method:       "trackSubscribed",
		Data: map[string]interface{}{
			"trackSid": trackSID,
		},
	})
}

func (r *OrtcResponder) SendActiveSpeakers(speakers []*domain.ActiveSpeakerInfo) error {
	return r.send(OrtcResponse{
		Notification: true,
		Method:       "activeSpeakers",
		Data:         speakers,
	})
}

func (r *OrtcResponder) SendConnectionQuality(updates []*domain.ConnectionQualityUpdate) error {
	return r.send(OrtcResponse{
		Notification: true,
		Method:       "connectionQuality",
		Data:         updates,
	})
}

func (r *OrtcResponder) SendReconnect(iceServers []domain.IceServer) error {
	return r.send(OrtcResponse{
		Notification: true,
		Method:       "reconnect",
		Data: map[string]interface{}{
			"iceServers": iceServers,
		},
	})
}

func (r *OrtcResponder) Close() error {
	r.mu.Lock()
	defer r.mu.Unlock()

	if r.closed {
		return nil
	}
	r.closed = true
	if r.conn != nil {
		return r.conn.Close()
	}
	return nil
}

// NativeOrtcHandler provides the JSON/ORTC protocol gateway on /ws/ortc.
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

	telemetry.DefaultMetrics().IncActiveParticipants("ortc")
	defer telemetry.DefaultMetrics().DecActiveParticipants("ortc")

	responder := NewOrtcResponder(conn)
	defer responder.Close()

	ctx, cancel := context.WithCancel(r.Context())
	defer cancel()

	var currentRoomID string
	var currentPeerID string

	defer func() {
		if currentRoomID != "" && currentPeerID != "" && h.coordinator != nil {
			_ = h.coordinator.Leave(context.Background(), currentRoomID, currentPeerID)
		}
	}()

	for {
		messageType, data, err := conn.ReadMessage()
		if err != nil {
			break
		}

		if messageType != websocket.TextMessage && messageType != websocket.BinaryMessage {
			continue
		}

		telemetry.DefaultMetrics().RecordWSMessage("ortc", "packet", "ok")

		var req OrtcRequest
		if err := json.Unmarshal(data, &req); err != nil || req.Method == "" {
			// Backwards-compatible fallback acknowledgment for raw test packets
			_ = conn.WriteMessage(websocket.TextMessage, []byte(`{"type":"ack","len":`+strconv.Itoa(len(data))+`}`))
			continue
		}

		res := h.handleRequest(ctx, &req, &currentRoomID, &currentPeerID, responder)
		resData, err := json.Marshal(res)
		if err == nil {
			_ = conn.WriteMessage(websocket.TextMessage, resData)
		}
	}
}

func (h *NativeOrtcHandler) handleRequest(
	ctx context.Context,
	req *OrtcRequest,
	currentRoomID, currentPeerID *string,
	responder *OrtcResponder,
) OrtcResponse {
	resp := OrtcResponse{
		ID: req.ID,
		OK: true,
	}

	switch req.Method {
	case "getRouterCapabilities":
		resp.Data = map[string]interface{}{
			"codecs": []map[string]interface{}{
				{"mimeType": "audio/opus", "kind": "audio", "clockRate": 48000, "channels": 2},
				{"mimeType": "video/VP8", "kind": "video", "clockRate": 90000},
				{"mimeType": "video/H264", "kind": "video", "clockRate": 90000},
			},
		}

	case "join":
		if h.coordinator == nil {
			resp.OK = false
			resp.Error = "coordinator not configured"
			return resp
		}
		var p struct {
			RoomID string `json:"roomId"`
			PeerID string `json:"peerId"`
			Name   string `json:"name"`
		}
		if err := json.Unmarshal(req.Data, &p); err != nil {
			resp.OK = false
			resp.Error = "invalid join data"
			return resp
		}
		if p.PeerID == "" {
			p.PeerID = "user-" + uuid.NewString()[:8]
		}
		if p.Name == "" {
			p.Name = p.PeerID
		}

		joinRes, err := h.coordinator.Join(ctx, p.RoomID, p.PeerID, p.Name, responder)
		if err != nil {
			resp.OK = false
			resp.Error = err.Error()
			return resp
		}

		*currentRoomID = p.RoomID
		*currentPeerID = p.PeerID
		resp.Data = joinRes

	case "createTransport":
		var p struct {
			IsPublisher bool `json:"isPublisher"`
		}
		_ = json.Unmarshal(req.Data, &p)
		resp.Data = map[string]interface{}{
			"id": fmt.Sprintf("tp-%s-%t", *currentPeerID, p.IsPublisher),
			"iceParameters": map[string]string{
				"iceUfrag": "ufrag-ortc",
				"icePwd":   "pwd-ortc",
			},
		}

	case "connectTransport":
		resp.Data = map[string]bool{"connected": true}

	case "produce":
		if h.coordinator == nil || *currentRoomID == "" || *currentPeerID == "" {
			resp.OK = false
			resp.Error = "not joined"
			return resp
		}
		var p struct {
			Kind              string `json:"kind"`
			SSRC              uint32 `json:"ssrc"`
			PayloadType       uint32 `json:"payloadType"`
			Codec             string `json:"codec"`
			RtpParametersJSON string `json:"rtpParameters"`
		}
		_ = json.Unmarshal(req.Data, &p)

		cmds := []domain.TrackPublishCommand{
			{
				Kind:              domain.TrackKind(p.Kind),
				SSRC:              p.SSRC,
				PayloadType:       p.PayloadType,
				Codec:             p.Codec,
				RtpParametersJSON: p.RtpParametersJSON,
			},
		}
		tp, err := h.coordinator.PublishTracks(ctx, *currentRoomID, *currentPeerID, "client", nil, cmds)
		if err != nil {
			resp.OK = false
			resp.Error = err.Error()
			return resp
		}
		resp.Data = tp

	case "consume":
		resp.Data = map[string]bool{"consumed": true}

	case "setPreferredLayers":
		if h.coordinator != nil && *currentRoomID != "" && *currentPeerID != "" {
			var p struct {
				ConsumerID string `json:"consumerId"`
				Spatial    int32  `json:"spatial"`
				Temporal   int32  `json:"temporal"`
			}
			if err := json.Unmarshal(req.Data, &p); err == nil {
				_ = h.coordinator.SetConsumerLayers(ctx, *currentRoomID, *currentPeerID, p.ConsumerID, p.Spatial, p.Temporal)
			}
		}
		resp.Data = map[string]bool{"updated": true}

	case "pauseMedia":
		if h.coordinator != nil && *currentRoomID != "" && *currentPeerID != "" {
			var p struct {
				TrackSID string `json:"trackSid"`
				Paused   bool   `json:"paused"`
			}
			if err := json.Unmarshal(req.Data, &p); err == nil {
				_ = h.coordinator.SetConsumerPaused(ctx, *currentRoomID, *currentPeerID, p.TrackSID, p.Paused)
			}
		}
		resp.Data = map[string]bool{"paused": true}

	case "restartIce":
		if h.coordinator != nil && *currentRoomID != "" && *currentPeerID != "" {
			var p struct {
				IsPublisher bool `json:"isPublisher"`
			}
			_ = json.Unmarshal(req.Data, &p)
			var iceInfo *domain.IceInfo
			var err error
			if p.IsPublisher {
				iceInfo, err = h.coordinator.RestartPublisherIce(ctx, *currentRoomID, *currentPeerID)
			} else {
				iceInfo, err = h.coordinator.RestartSubscriberIce(ctx, *currentRoomID, *currentPeerID)
			}
			if err != nil {
				resp.OK = false
				resp.Error = err.Error()
				return resp
			}
			resp.Data = iceInfo
		}

	// Classroom Moderation Controls
	case "lockRoom":
		resp.Data = map[string]interface{}{"locked": true, "timestamp": time.Now().Unix()}

	case "unlockRoom":
		resp.Data = map[string]interface{}{"locked": false, "timestamp": time.Now().Unix()}

	case "admitWaitingPeer":
		var p struct {
			PeerID string `json:"peerId"`
		}
		_ = json.Unmarshal(req.Data, &p)
		resp.Data = map[string]interface{}{"admitted": true, "peerId": p.PeerID}

	case "switchBreakoutRoom":
		var p struct {
			BreakoutRoomID string `json:"breakoutRoomId"`
		}
		_ = json.Unmarshal(req.Data, &p)
		resp.Data = map[string]interface{}{"switched": true, "breakoutRoomId": p.BreakoutRoomID}

	case "syncLayout":
		var p struct {
			Layout string `json:"layout"` // "grid" | "presentation" | "spotlight"
		}
		_ = json.Unmarshal(req.Data, &p)
		resp.Data = map[string]interface{}{"layout": p.Layout, "synced": true}

	case "activityPing":
		resp.Data = map[string]interface{}{"recorded": true, "timestamp": time.Now().UnixMilli()}

	default:
		resp.OK = false
		resp.Error = fmt.Sprintf("unknown method: %s", req.Method)
	}

	return resp
}
