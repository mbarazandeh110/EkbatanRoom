package livekit

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"net/http"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/gorilla/websocket"
	"github.com/livekit/protocol/auth"
	livekitproto "github.com/livekit/protocol/livekit"
	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/trace"
	"google.golang.org/protobuf/proto"

	"ekbatan-room/signaling-coordinator/internal/adapters/outbound/telemetry"
	"ekbatan-room/signaling-coordinator/internal/application"
	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

func parseTokenClaims(tokenStr, secret string) (identity, name, roomID, metadata string) {
	if tokenStr == "" {
		return "", "", "", ""
	}
	if secret != "" {
		if verifier, err := auth.ParseAPIToken(tokenStr); err == nil && verifier != nil {
			if _, grants, vErr := verifier.Verify([]byte(secret)); vErr == nil && grants != nil {
				room := ""
				if grants.Video != nil {
					room = grants.Video.Room
				}
				return grants.Identity, grants.Name, room, grants.Metadata
			}
		}
	}
	// Fallback unverified token decode for development / load testing
	parts := strings.Split(tokenStr, ".")
	if len(parts) >= 2 {
		payloadBytes, err := base64.RawURLEncoding.DecodeString(parts[1])
		if err == nil {
			var raw struct {
				Sub      string `json:"sub"`
				Name     string `json:"name"`
				Metadata string `json:"metadata"`
				Video    struct {
					Room string `json:"room"`
				} `json:"video"`
			}
			if err := json.Unmarshal(payloadBytes, &raw); err == nil {
				return raw.Sub, raw.Name, raw.Video.Room, raw.Metadata
			}
		}
	}
	return "", "", "", ""
}

var upgrader = websocket.Upgrader{
	CheckOrigin: func(r *http.Request) bool {
		return true // Allow all origins for signaling gateway
	},
}

// LiveKitWSHandler handles incoming WebSocket connections on /rtc.
type LiveKitWSHandler struct {
	coordinator ports.RoomCoordinatorPort
	cfg         ResponderConfig
}

// NewLiveKitWSHandler creates a new LiveKit signaling WebSocket handler.
func NewLiveKitWSHandler(coordinator ports.RoomCoordinatorPort, cfgs ...ResponderConfig) *LiveKitWSHandler {
	cfg := DefaultResponderConfig()
	if len(cfgs) > 0 {
		cfg = cfgs[0]
	}
	return &LiveKitWSHandler{
		coordinator: coordinator,
		cfg:         cfg,
	}
}

func (h *LiveKitWSHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	conn, err := upgrader.Upgrade(w, r, nil)
	if err != nil {
		slog.Error("livekit websocket upgrade failed", "error", err)
		telemetry.DefaultMetrics().RecordError("upgrade_failed")
		return
	}
	defer conn.Close()

	// Extract room, identity, token and query parameters
	q := r.URL.Query()
	tokenStr := q.Get("access_token")
	if tokenStr == "" {
		tokenStr = q.Get("token")
	}
	if tokenStr == "" {
		authHeader := r.Header.Get("Authorization")
		if strings.HasPrefix(strings.ToLower(authHeader), "bearer ") {
			tokenStr = strings.TrimSpace(authHeader[7:])
		}
	}

	tokenIdentity, tokenName, tokenRoom, _ := parseTokenClaims(tokenStr, h.cfg.ApiSecret)

	roomID := q.Get("room")
	if roomID == "" {
		roomID = q.Get("room_name")
	}
	if roomID == "" {
		roomID = tokenRoom
	}
	if roomID == "" {
		roomID = "default-room"
	}

	identity := q.Get("identity")
	if identity == "" {
		identity = tokenIdentity
	}
	if identity == "" {
		identity = "user-" + uuid.NewString()[:8]
	}

	name := q.Get("name")
	if name == "" {
		name = tokenName
	}
	if name == "" {
		name = identity
	}

	isReconnect := q.Get("reconnect") == "1" || strings.EqualFold(q.Get("reconnect"), "true")
	protocolVer := q.Get("protocol")
	sdk := q.Get("sdk")
	previousSID := q.Get("sid")
	targetNode := q.Get("node")
	if targetNode == "" {
		targetNode = q.Get("node_id")
	}

	tracer := otel.Tracer("livekit.gateway")
	ctx, span := tracer.Start(r.Context(), "LiveKitWS.Session",
		trace.WithAttributes(
			attribute.String("room_id", roomID),
			attribute.String("identity", identity),
			attribute.Bool("reconnect", isReconnect),
		),
	)
	defer span.End()

	if targetNode != "" {
		ctx = application.WithTargetNode(ctx, targetNode)
	}
	if previousSID != "" {
		ctx = application.WithPreviousSID(ctx, previousSID)
	}

	telemetry.DefaultMetrics().IncActiveParticipants("livekit")
	defer telemetry.DefaultMetrics().DecActiveParticipants("livekit")

	responder := NewLiveKitResponder(conn, h.cfg)
	defer responder.Close()

	ctx, cancel := context.WithCancel(ctx)
	defer cancel()

	slog.InfoContext(ctx, "livekit participant connecting",
		"room_id", roomID,
		"peer_id", identity,
		"identity", identity,
		"name", name,
		"reconnect", isReconnect,
		"protocol", protocolVer,
		"sdk", sdk,
		"previous_sid", previousSID,
		"target_node", targetNode,
	)

	// 1. Perform Join Handshake
	_, err = h.coordinator.Join(ctx, roomID, identity, name, responder)
	if err != nil {
		slog.ErrorContext(ctx, "livekit join handshake failed",
			"room_id", roomID,
			"peer_id", identity,
			"identity", identity,
			"error", err,
		)
		telemetry.DefaultMetrics().RecordError("join_failed")
		if errors.Is(err, domain.ErrParticipantEvicted) {
			_ = responder.SendLeave(false, domain.DisconnectReasonUserRejected)
			time.Sleep(10 * time.Millisecond)
		}
		return
	}
	slog.InfoContext(ctx, "livekit participant joined",
		"room_id", roomID,
		"peer_id", identity,
		"identity", identity,
	)
	defer func() {
		_ = h.coordinator.Leave(context.Background(), roomID, identity)
	}()

	type pendingAddTrackInfo struct {
		sid      string
		cid      string
		kind     domain.TrackKind
		name     string
		source   string
		width    uint32
		height   uint32
		mimeType string
	}
	var pendingAddTracks []pendingAddTrackInfo
	publishedMIDs := make(map[string]string)
	publishedSSRCs := make(map[uint32]string)

	// 2. Main incoming message read loop
	for {
		messageType, data, err := conn.ReadMessage()
		if err != nil {
			if websocket.IsUnexpectedCloseError(err, websocket.CloseGoingAway, websocket.CloseAbnormalClosure) {
				slog.WarnContext(ctx, "livekit websocket read error",
					"room_id", roomID,
					"peer_id", identity,
					"identity", identity,
					"error", err,
				)
			}
			break
		}

		if messageType != websocket.BinaryMessage {
			continue
		}

		telemetry.DefaultMetrics().RecordWSMessage("livekit", "signal_request", "inbound")

		var req livekitproto.SignalRequest
		if err := proto.Unmarshal(data, &req); err != nil {
			slog.ErrorContext(ctx, "livekit proto unmarshal error",
				"room_id", roomID,
				"peer_id", identity,
				"identity", identity,
				"error", err,
			)
			telemetry.DefaultMetrics().RecordError("unmarshal_failed")
			continue
		}

		switch msg := req.Message.(type) {
		case *livekitproto.SignalRequest_Offer:
			offerStart := time.Now()
			offerID := msg.Offer.GetId()
			slog.DebugContext(ctx, "received publisher offer",
				"room_id", roomID,
				"peer_id", identity,
				"identity", identity,
				"offer_id", offerID,
				"sdp", msg.Offer.GetSdp(),
			)

			parsed, err := ParsePublisherOffer(msg.Offer.GetSdp())
			if err != nil {
				slog.ErrorContext(ctx, "livekit offer parse error",
					"room_id", roomID,
					"identity", identity,
					"error", err,
				)
				telemetry.DefaultMetrics().RecordError("offer_parse_failed")
				continue
			}

			trackCmds := make([]domain.TrackPublishCommand, 0, len(parsed.Tracks))
			midToTrackId := make(map[string]string)
			for _, t := range parsed.Tracks {
				var existingSID string
				if t.MID != "" {
					existingSID = publishedMIDs[t.MID]
				}
				if existingSID == "" && t.SSRC != 0 {
					existingSID = publishedSSRCs[t.SSRC]
				}

				var assignedSID string
				var patInfo pendingAddTrackInfo

				if existingSID != "" {
					// Track was previously published; retain its existing SID without consuming new pendingAddTracks
					assignedSID = existingSID
				} else {
					// New track in renegotiation or initial publish; allocate from pendingAddTracks
					matchedIdx := -1
					if t.CID != "" {
						for i, pat := range pendingAddTracks {
							if pat.cid != "" && pat.cid == t.CID {
								matchedIdx = i
								break
							}
						}
					}
					if matchedIdx == -1 {
						for i, pat := range pendingAddTracks {
							if pat.kind == t.Kind {
								matchedIdx = i
								break
							}
						}
					}
					if matchedIdx >= 0 {
						pat := pendingAddTracks[matchedIdx]
						assignedSID = pat.sid
						patInfo = pat
						pendingAddTracks = append(pendingAddTracks[:matchedIdx], pendingAddTracks[matchedIdx+1:]...)
					}
				}

				trackCmd := domain.TrackPublishCommand{
					Kind:              t.Kind,
					SSRC:              t.SSRC,
					RTXSSRC:           t.RTXSSRC,
					PayloadType:       t.PayloadType,
					RTXPayloadType:    t.RTXPayloadType,
					Codec:             t.Codec,
					ClockRate:         t.ClockRate,
					CName:             t.CName,
					RtpParametersJSON: t.RtpParametersJSON,
					Name:              patInfo.name,
					Source:            patInfo.source,
					Width:             patInfo.width,
					Height:            patInfo.height,
				}
				if trackCmd.Codec == "" && patInfo.mimeType != "" {
					trackCmd.Codec = patInfo.mimeType
				}

				if assignedSID != "" {
					trackCmd.TrackSID = assignedSID
					if t.MID != "" {
						midToTrackId[t.MID] = assignedSID
						publishedMIDs[t.MID] = assignedSID
					}
					if t.SSRC != 0 {
						publishedSSRCs[t.SSRC] = assignedSID
					}
				} else {
					if t.MID != "" {
						midToTrackId[t.MID] = t.MID
					}
				}
				trackCmds = append(trackCmds, trackCmd)
			}

			pubTransport, err := h.coordinator.PublishTracks(ctx, roomID, identity, parsed.DTLSRole, parsed.RemoteFingerprints, trackCmds)
			if err != nil {
				slog.ErrorContext(ctx, "livekit publish tracks error",
					"room_id", roomID,
					"identity", identity,
					"error", err,
				)
				telemetry.DefaultMetrics().RecordError("publish_tracks_failed")
				continue
			}

			answerSDP, err := GeneratePublisherAnswer(msg.Offer.GetSdp(), pubTransport)
			if err != nil {
				slog.ErrorContext(ctx, "livekit generate publisher answer error",
					"room_id", roomID,
					"identity", identity,
					"error", err,
				)
				telemetry.DefaultMetrics().RecordError("generate_answer_failed")
				continue
			}

			slog.DebugContext(ctx, "sending publisher answer",
				"room_id", roomID,
				"peer_id", identity,
				"identity", identity,
				"offer_id", offerID,
				"sdp", answerSDP,
			)

			answerMsg := &livekitproto.SignalResponse{
				Message: &livekitproto.SignalResponse_Answer{
					Answer: &livekitproto.SessionDescription{
						Type:         "answer",
						Sdp:          answerSDP,
						Id:           offerID,
						MidToTrackId: midToTrackId,
					},
				},
			}
			_ = responder.send(answerMsg)
			telemetry.DefaultMetrics().RecordSDPNegotiation("publisher", time.Since(offerStart).Seconds())

		case *livekitproto.SignalRequest_AddTrack:
			addTrack := msg.AddTrack
			trackSID := addTrack.GetSid()
			if trackSID == "" {
				trackSID = fmt.Sprintf("TR_%s", uuid.NewString()[:12])
			}

			kind := domain.TrackKindAudio
			if addTrack.GetType() == livekitproto.TrackType_VIDEO {
				kind = domain.TrackKindVideo
			}
			sourceStr := "CAMERA"
			switch addTrack.GetSource() {
			case livekitproto.TrackSource_CAMERA:
				sourceStr = "CAMERA"
			case livekitproto.TrackSource_MICROPHONE:
				sourceStr = "MICROPHONE"
			case livekitproto.TrackSource_SCREEN_SHARE:
				sourceStr = "SCREEN_SHARE"
			case livekitproto.TrackSource_SCREEN_SHARE_AUDIO:
				sourceStr = "SCREEN_SHARE_AUDIO"
			}
			var mimeType string
			var codecs []*livekitproto.SimulcastCodecInfo

			if kind == domain.TrackKindVideo {
				if len(addTrack.GetSimulcastCodecs()) > 0 {
					for _, sc := range addTrack.GetSimulcastCodecs() {
						cMime := strings.TrimSpace(sc.GetCodec())
						if cMime != "" {
							if !strings.Contains(cMime, "/") {
								cMime = "video/" + strings.ToLower(cMime)
							} else {
								cMime = strings.ToLower(cMime)
							}
						} else {
							cMime = "video/vp8"
						}
						if mimeType == "" {
							mimeType = cMime
						}
						layers := sc.GetLayers()
						if len(layers) == 0 {
							layers = addTrack.GetLayers()
						}
						cid := sc.GetCid()
						if cid == "" {
							cid = addTrack.GetCid()
						}
						codecs = append(codecs, &livekitproto.SimulcastCodecInfo{
							MimeType:       cMime,
							Cid:            cid,
							Layers:         layers,
							VideoLayerMode: sc.GetVideoLayerMode(),
						})
					}
				}
				if mimeType == "" {
					mimeType = "video/vp8"
				}
				if len(codecs) == 0 {
					codecs = []*livekitproto.SimulcastCodecInfo{
						{
							MimeType: mimeType,
							Cid:      addTrack.GetCid(),
							Layers:   addTrack.GetLayers(),
						},
					}
				}
			} else {
				mimeType = "audio/opus"
				codecs = []*livekitproto.SimulcastCodecInfo{
					{
						MimeType: mimeType,
						Cid:      addTrack.GetCid(),
					},
				}
			}

			finalLayers := addTrack.GetLayers()
			if len(finalLayers) == 0 && len(codecs) > 0 && len(codecs[0].Layers) > 0 {
				finalLayers = codecs[0].Layers
			}

			pendingAddTracks = append(pendingAddTracks, pendingAddTrackInfo{
				sid:      trackSID,
				cid:      addTrack.GetCid(),
				kind:     kind,
				name:     addTrack.GetName(),
				source:   sourceStr,
				width:    addTrack.GetWidth(),
				height:   addTrack.GetHeight(),
				mimeType: mimeType,
			})

			slog.InfoContext(ctx, "received add track request",
				"room_id", roomID,
				"peer_id", identity,
				"identity", identity,
				"cid", addTrack.GetCid(),
				"track_sid", trackSID,
				"type", addTrack.GetType().String(),
				"mime_type", mimeType,
			)

			trackInfo := &livekitproto.TrackInfo{
				Sid:        trackSID,
				Name:       addTrack.GetName(),
				Type:       addTrack.GetType(),
				Muted:      addTrack.GetMuted(),
				Width:      addTrack.GetWidth(),
				Height:     addTrack.GetHeight(),
				Simulcast:  len(finalLayers) > 1 || len(addTrack.GetSimulcastCodecs()) > 0,
				DisableDtx: addTrack.GetDisableDtx(),
				Source:     addTrack.GetSource(),
				Layers:     finalLayers,
				MimeType:   mimeType,
				Codecs:     codecs,
			}

			trackPubMsg := &livekitproto.SignalResponse{
				Message: &livekitproto.SignalResponse_TrackPublished{
					TrackPublished: &livekitproto.TrackPublishedResponse{
						Cid:   addTrack.GetCid(),
						Track: trackInfo,
					},
				},
			}
			_ = responder.send(trackPubMsg)
			telemetry.DefaultMetrics().RecordWSMessage("livekit", "track_published", "ok")

		case *livekitproto.SignalRequest_Answer:
			ansStart := time.Now()
			slog.DebugContext(ctx, "received subscriber answer",
				"room_id", roomID,
				"peer_id", identity,
				"identity", identity,
				"sdp", msg.Answer.GetSdp(),
			)

			parsed, err := ParseSubscriberAnswer(msg.Answer.GetSdp())
			if err != nil {
				slog.ErrorContext(ctx, "livekit subscriber answer parse error",
					"room_id", roomID,
					"identity", identity,
					"error", err,
				)
				telemetry.DefaultMetrics().RecordError("subscriber_answer_parse_failed")
				continue
			}
			if err := h.coordinator.HandleSubscriberAnswer(ctx, roomID, identity, parsed.DTLSRole, parsed.RemoteFingerprints); err != nil {
				slog.ErrorContext(ctx, "livekit subscriber answer error",
					"room_id", roomID,
					"identity", identity,
					"error", err,
				)
				telemetry.DefaultMetrics().RecordError("subscriber_answer_failed")
			} else {
				telemetry.DefaultMetrics().RecordSDPNegotiation("subscriber", time.Since(ansStart).Seconds())
			}

		case *livekitproto.SignalRequest_Trickle:
			cand := msg.Trickle.GetCandidateInit()
			target := int(msg.Trickle.GetTarget())
			slog.DebugContext(ctx, "received trickle candidate",
				"room_id", roomID,
				"peer_id", identity,
				"identity", identity,
				"target", target,
			)
			if err := h.coordinator.HandleTrickle(ctx, roomID, identity, cand, target); err != nil {
				slog.ErrorContext(ctx, "livekit trickle error",
					"room_id", roomID,
					"identity", identity,
					"error", err,
				)
			}

		case *livekitproto.SignalRequest_Mute:
			slog.InfoContext(ctx, "livekit mute request",
				"room_id", roomID,
				"peer_id", identity,
				"identity", identity,
				"track_sid", msg.Mute.GetSid(),
				"muted", msg.Mute.GetMuted(),
			)
			if err := h.coordinator.SetTrackMuted(ctx, roomID, identity, msg.Mute.GetSid(), msg.Mute.GetMuted()); err != nil {
				slog.ErrorContext(ctx, "livekit mute track error",
					"room_id", roomID,
					"identity", identity,
					"error", err,
				)
				telemetry.DefaultMetrics().RecordError("mute_track_failed")
			}

		case *livekitproto.SignalRequest_TrackSetting:
			trackSettings := msg.TrackSetting
			slog.InfoContext(ctx, "livekit track setting request",
				"room_id", roomID,
				"peer_id", identity,
				"identity", identity,
				"quality", trackSettings.GetQuality(),
				"disabled", trackSettings.GetDisabled(),
			)
			spatial := int32(trackSettings.GetQuality())
			// REQ-002: Decouple temporal layer from spatial layer so SFU does not throttle framerate to 7.5 fps
			temporal := int32(-1)
			disabled := trackSettings.GetDisabled()
			for _, sid := range trackSettings.GetTrackSids() {
				if sid == "" {
					continue
				}
				// REQ-002: Pause consumer when disabled is true, resume when false
				if err := h.coordinator.SetConsumerPaused(ctx, roomID, identity, sid, disabled); err != nil {
					slog.ErrorContext(ctx, "livekit track setting pause error",
						"room_id", roomID,
						"identity", identity,
						"track_sid", sid,
						"disabled", disabled,
						"error", err,
					)
					telemetry.DefaultMetrics().RecordError("track_setting_pause_failed")
				}
				if !disabled {
					if err := h.coordinator.SetConsumerLayers(ctx, roomID, identity, sid, spatial, temporal); err != nil {
						slog.ErrorContext(ctx, "livekit track setting error",
							"room_id", roomID,
							"identity", identity,
							"track_sid", sid,
							"error", err,
						)
						telemetry.DefaultMetrics().RecordError("track_setting_failed")
					}
				}
			}

		case *livekitproto.SignalRequest_Ping:
			pongMsg := &livekitproto.SignalResponse{
				Message: &livekitproto.SignalResponse_Pong{
					Pong: msg.Ping,
				},
			}
			_ = responder.send(pongMsg)

		case *livekitproto.SignalRequest_Subscription:
			subReq := msg.Subscription
			for _, sid := range subReq.GetTrackSids() {
				if sid != "" {
					_ = h.coordinator.SetConsumerPaused(ctx, roomID, identity, sid, !subReq.GetSubscribe())
				}
			}
			for _, pt := range subReq.GetParticipantTracks() {
				for _, sid := range pt.GetTrackSids() {
					if sid != "" {
						_ = h.coordinator.SetConsumerPaused(ctx, roomID, identity, sid, !subReq.GetSubscribe())
					}
				}
			}
			subResp := &livekitproto.SignalResponse{
				Message: &livekitproto.SignalResponse_SubscriptionResponse{
					SubscriptionResponse: &livekitproto.SubscriptionResponse{},
				},
			}
			_ = responder.send(subResp)

		case *livekitproto.SignalRequest_SyncState:
			syncReq := msg.SyncState
			for _, disabledSID := range syncReq.GetTrackSidsDisabled() {
				_ = h.coordinator.SetConsumerPaused(ctx, roomID, identity, disabledSID, true)
			}
			if syncReq.GetSubscription() != nil {
				for _, sid := range syncReq.GetSubscription().GetTrackSids() {
					_ = h.coordinator.SetConsumerPaused(ctx, roomID, identity, sid, !syncReq.GetSubscription().GetSubscribe())
				}
			}

		case *livekitproto.SignalRequest_PingReq:
			pongMsg := &livekitproto.SignalResponse{
				Message: &livekitproto.SignalResponse_PongResp{
					PongResp: &livekitproto.Pong{
						LastPingTimestamp: msg.PingReq.GetTimestamp(),
						Timestamp:         time.Now().UnixMilli(),
					},
				},
			}
			_ = responder.send(pongMsg)

		case *livekitproto.SignalRequest_UpdateMetadata:
			slog.DebugContext(ctx, "livekit update metadata request", "identity", identity, "metadata", msg.UpdateMetadata.GetMetadata())

		case *livekitproto.SignalRequest_Simulate:
			slog.InfoContext(ctx, "livekit simulate scenario", "identity", identity, "scenario", msg.Simulate.String())

		case *livekitproto.SignalRequest_Leave:
			slog.InfoContext(ctx, "livekit participant leaving",
				"room_id", roomID,
				"peer_id", identity,
				"identity", identity,
			)
			return

		default:
			// Non-critical signal message ignored
			_ = strings.Clone("")
		}
	}
}
