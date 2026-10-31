package livekit

import (
	"sync"
	"time"

	"github.com/gorilla/websocket"
	"google.golang.org/protobuf/proto"

	livekitproto "github.com/livekit/protocol/livekit"

	"ekbatan-room/signaling-coordinator/internal/adapters/outbound/telemetry"
	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

var _ ports.PeerResponderPort = (*LiveKitResponder)(nil)

// ResponderConfig configures LiveKit protocol parameters sent in JoinResponse.
type ResponderConfig struct {
	PingInterval  uint32
	PingTimeout   uint32
	ServerVersion string
}

// DefaultResponderConfig returns recommended default settings matching the LiveKit protocol.
func DefaultResponderConfig() ResponderConfig {
	return ResponderConfig{
		PingInterval:  10,
		PingTimeout:   20,
		ServerVersion: "1.53.0",
	}
}

// LiveKitResponder handles sending LiveKit protobuf signal responses over a WebSocket connection.
type LiveKitResponder struct {
	mu           sync.Mutex
	conn         *websocket.Conn
	subTransport *domain.TransportInfo
	offerSeq     uint32
	closed       bool
	cfg          ResponderConfig
}

func NewLiveKitResponder(conn *websocket.Conn, cfgs ...ResponderConfig) *LiveKitResponder {
	cfg := DefaultResponderConfig()
	if len(cfgs) > 0 {
		cfg = cfgs[0]
	}
	return &LiveKitResponder{
		conn: conn,
		cfg:  cfg,
	}
}

func (r *LiveKitResponder) send(msg *livekitproto.SignalResponse) error {
	r.mu.Lock()
	defer r.mu.Unlock()

	if r.closed {
		return domain.ErrRoomClosed
	}

	data, err := proto.Marshal(msg)
	if err != nil {
		return err
	}

	writeTimeout := time.Duration(r.cfg.PingTimeout) * time.Second
	if writeTimeout <= 0 {
		writeTimeout = 10 * time.Second
	}
	_ = r.conn.SetWriteDeadline(time.Now().Add(writeTimeout))

	telemetry.DefaultMetrics().RecordWSMessage("livekit", "signal_response", "ok")
	return r.conn.WriteMessage(websocket.BinaryMessage, data)
}

func defaultParticipantPermission() *livekitproto.ParticipantPermission {
	return &livekitproto.ParticipantPermission{
		CanSubscribe:      true,
		CanPublish:        true,
		CanPublishData:    true,
		CanUpdateMetadata: true,
	}
}

func mapDomainTracksToProto(tracks []*domain.Track) []*livekitproto.TrackInfo {
	if len(tracks) == 0 {
		return nil
	}
	protoTracks := make([]*livekitproto.TrackInfo, 0, len(tracks))
	for _, t := range tracks {
		trackType := livekitproto.TrackType_AUDIO
		src := livekitproto.TrackSource_MICROPHONE
		if t.Kind == domain.TrackKindVideo {
			trackType = livekitproto.TrackType_VIDEO
			src = livekitproto.TrackSource_CAMERA
		}
		switch t.Source {
		case "CAMERA":
			src = livekitproto.TrackSource_CAMERA
		case "MICROPHONE":
			src = livekitproto.TrackSource_MICROPHONE
		case "SCREEN_SHARE":
			src = livekitproto.TrackSource_SCREEN_SHARE
		case "SCREEN_SHARE_AUDIO":
			src = livekitproto.TrackSource_SCREEN_SHARE_AUDIO
		}

		name := t.Name
		if name == "" {
			name = t.SID
		}

		protoTracks = append(protoTracks, &livekitproto.TrackInfo{
			Sid:       t.SID,
			Type:      trackType,
			Name:      name,
			Muted:     t.Paused,
			Width:     t.Width,
			Height:    t.Height,
			Source:    src,
			Simulcast: len(t.SimulcastEncodings) > 0,
		})
	}
	return protoTracks
}

func (r *LiveKitResponder) SendJoinResponse(res *domain.JoinResult) error {
	r.mu.Lock()
	r.subTransport = res.SubscriberTransport
	r.mu.Unlock()

	otherParticipants := make([]*livekitproto.ParticipantInfo, 0)
	for _, p := range res.Room.ListParticipants() {
		if p.ID == res.Participant.ID {
			continue
		}
		otherParticipants = append(otherParticipants, &livekitproto.ParticipantInfo{
			Sid:        p.SID,
			Identity:   p.ID,
			Name:       p.Name,
			State:      livekitproto.ParticipantInfo_JOINED,
			Tracks:     mapDomainTracksToProto(p.ListTracks()),
			Permission: defaultParticipantPermission(),
		})
	}

	iceServers := make([]*livekitproto.ICEServer, 0, len(res.IceServers))
	for _, is := range res.IceServers {
		iceServers = append(iceServers, &livekitproto.ICEServer{
			Urls:       is.URLs,
			Username:   is.Username,
			Credential: is.Credential,
		})
	}

	joinMsg := &livekitproto.SignalResponse{
		Message: &livekitproto.SignalResponse_Join{
			Join: &livekitproto.JoinResponse{
				Room: &livekitproto.Room{
					Sid:  res.Room.ID,
					Name: res.Room.ID,
				},
				Participant: &livekitproto.ParticipantInfo{
					Sid:        res.Participant.SID,
					Identity:   res.Participant.ID,
					Name:       res.Participant.Name,
					State:      livekitproto.ParticipantInfo_JOINED,
					Tracks:     mapDomainTracksToProto(res.Participant.ListTracks()),
					Permission: defaultParticipantPermission(),
				},
				OtherParticipants: otherParticipants,
				ServerVersion:     r.cfg.ServerVersion,
				IceServers:        iceServers,
				SubscriberPrimary: false,
				ServerInfo: &livekitproto.ServerInfo{
					Edition:  livekitproto.ServerInfo_Standard,
					Version:  r.cfg.ServerVersion,
					Protocol: 12,
				},
				PingTimeout:  int32(r.cfg.PingTimeout),
				PingInterval: int32(r.cfg.PingInterval),
			},
		},
	}

	return r.send(joinMsg)
}

func (r *LiveKitResponder) SendSubscriberOffer(slots []*domain.SDPSlot, participantSID string) error {
	r.mu.Lock()
	subTransport := r.subTransport
	r.offerSeq++
	offerID := r.offerSeq
	r.mu.Unlock()

	if subTransport == nil {
		return domain.ErrTransportNotFound
	}

	sdpOffer, err := GenerateSubscriberOffer(subTransport, slots, participantSID, uint64(offerID))
	if err != nil {
		return err
	}

	midToTrackId := make(map[string]string)
	for _, slot := range slots {
		if slot.Active && slot.TrackSID != "" && slot.MID != "" {
			midToTrackId[slot.MID] = slot.TrackSID
		}
	}

	msg := &livekitproto.SignalResponse{
		Message: &livekitproto.SignalResponse_Offer{
			Offer: &livekitproto.SessionDescription{
				Type:         "offer",
				Sdp:          sdpOffer,
				Id:           offerID,
				MidToTrackId: midToTrackId,
			},
		},
	}
	return r.send(msg)
}

func (r *LiveKitResponder) SendTrickle(candidate string, target int) error {
	msg := &livekitproto.SignalResponse{
		Message: &livekitproto.SignalResponse_Trickle{
			Trickle: &livekitproto.TrickleRequest{
				CandidateInit: candidate,
				Target:        livekitproto.SignalTarget(target),
			},
		},
	}
	return r.send(msg)
}

func (r *LiveKitResponder) SendParticipantUpdate(participants []*domain.Participant) error {
	protoParts := make([]*livekitproto.ParticipantInfo, 0, len(participants))
	for _, p := range participants {
		protoParts = append(protoParts, &livekitproto.ParticipantInfo{
			Sid:        p.SID,
			Identity:   p.ID,
			Name:       p.Name,
			State:      livekitproto.ParticipantInfo_JOINED,
			Tracks:     mapDomainTracksToProto(p.ListTracks()),
			Permission: defaultParticipantPermission(),
		})
	}

	msg := &livekitproto.SignalResponse{
		Message: &livekitproto.SignalResponse_Update{
			Update: &livekitproto.ParticipantUpdate{
				Participants: protoParts,
			},
		},
	}
	return r.send(msg)
}

func (r *LiveKitResponder) SendTrackMuted(trackSID string, muted bool) error {
	msg := &livekitproto.SignalResponse{
		Message: &livekitproto.SignalResponse_Mute{
			Mute: &livekitproto.MuteTrackRequest{
				Sid:   trackSID,
				Muted: muted,
			},
		},
	}
	return r.send(msg)
}

func (r *LiveKitResponder) Close() error {
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
