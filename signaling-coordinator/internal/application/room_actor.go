package application

import (
	"context"
	"fmt"
	"sync"

	"github.com/google/uuid"

	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

// defaultWebRTCRtpCapabilitiesJSON provides fallback standard WebRTC codecs (Opus, VP8, H264)
// so mediasoup router.can_consume() validation succeeds even when router caps are not pre-cached.
const defaultWebRTCRtpCapabilitiesJSON = `{"codecs":[{"mimeType":"audio/opus","kind":"audio","clockRate":48000,"channels":2,"preferredPayloadType":111},{"mimeType":"video/VP8","kind":"video","clockRate":90000,"preferredPayloadType":96},{"mimeType":"video/H264","kind":"video","clockRate":90000,"preferredPayloadType":125,"parameters":{"level-asymmetry-allowed":1,"packetization-mode":1,"profile-level-id":"42e01f"}}],"headerExtensions":[]}`

// RoomActor owns the room lifecycle, participant roster, and coordinates media resources.
type RoomActor struct {
	mu sync.Mutex

	room            *domain.Room
	sessions        map[string]*ParticipantSession
	routerPort      ports.RouterEnginePort
	transportPort   ports.TransportEnginePort
	mediaStreamPort ports.MediaStreamPort
	iceServers      []domain.IceServer
	closed          bool
}

func (a *RoomActor) getRtpCapabilitiesJSON() string {
	caps := a.room.GetRtpCapabilitiesJSON()
	if caps != "" && caps != `{"codecs":[]}` {
		return caps
	}
	return defaultWebRTCRtpCapabilitiesJSON
}

func NewRoomActor(
	roomID string,
	routerPort ports.RouterEnginePort,
	transportPort ports.TransportEnginePort,
	mediaStreamPort ports.MediaStreamPort,
	iceServers []domain.IceServer,
) *RoomActor {
	return &RoomActor{
		room:            domain.NewRoom(roomID, ""),
		sessions:        make(map[string]*ParticipantSession),
		routerPort:      routerPort,
		transportPort:   transportPort,
		mediaStreamPort: mediaStreamPort,
		iceServers:      iceServers,
	}
}

func (a *RoomActor) Join(
	ctx context.Context,
	participantID string,
	participantName string,
	responder ports.PeerResponderPort,
) (*domain.JoinResult, error) {
	a.mu.Lock()
	defer a.mu.Unlock()

	if a.closed || !a.room.IsActive() {
		return nil, domain.ErrRoomClosed
	}

	// 1. Lazy initialize media router on the SFU node if not yet created
	if a.room.RouterID == "" {
		routerInfo, err := a.routerPort.CreateRouter(ctx, a.room.ID)
		if err != nil {
			return nil, fmt.Errorf("failed to create media router for room %s: %w", a.room.ID, err)
		}
		a.room.RouterID = routerInfo.RouterID
		a.room.SetRtpCapabilitiesJSON(routerInfo.RtpCapabilitiesJSON)
	}

	// 2. Provision Publisher WebRTC transport on the SFU node
	pubTransport, err := a.transportPort.CreateTransport(ctx, &domain.CreateTransportCommand{
		RequestID:   uuid.NewString(),
		RouterID:    a.room.RouterID,
		PeerID:      participantID,
		IsPublisher: true,
		EnableSctp:  true,
	})
	if err != nil {
		return nil, fmt.Errorf("failed to create publisher transport: %w", err)
	}

	// 3. Provision Subscriber WebRTC transport on the SFU node
	subTransport, err := a.transportPort.CreateTransport(ctx, &domain.CreateTransportCommand{
		RequestID:   uuid.NewString(),
		RouterID:    a.room.RouterID,
		PeerID:      participantID,
		IsPublisher: false,
		EnableSctp:  true,
	})
	if err != nil {
		// Clean up publisher transport on failure
		_ = a.transportPort.CloseTransport(ctx, a.room.RouterID, pubTransport.TransportID)
		return nil, fmt.Errorf("failed to create subscriber transport: %w", err)
	}

	// 4. Create and enroll participant in domain
	sid := fmt.Sprintf("PA_%s", uuid.NewString()[:8])
	p := domain.NewParticipant(participantID, sid, participantName)
	p.PublisherTransportID = pubTransport.TransportID
	p.SubscriberTransportID = subTransport.TransportID
	p.SetConnected()

	if err := a.room.AddParticipant(p); err != nil {
		_ = a.transportPort.CloseTransport(ctx, a.room.RouterID, pubTransport.TransportID)
		_ = a.transportPort.CloseTransport(ctx, a.room.RouterID, subTransport.TransportID)
		return nil, err
	}

	session := NewParticipantSession(p, pubTransport, subTransport, responder)
	a.sessions[participantID] = session

	joinRes := &domain.JoinResult{
		Room:                a.room,
		Participant:         p,
		PublisherTransport:  pubTransport,
		SubscriberTransport: subTransport,
		IceServers:          a.iceServers,
	}

	// Notify new participant via responder
	if responder != nil {
		if err := responder.SendJoinResponse(joinRes); err != nil {
			_ = a.leaveLocked(ctx, participantID)
			return nil, fmt.Errorf("failed to send join response: %w", err)
		}
	}

	// Broadcast participant joined update to existing participants
	a.broadcastParticipantUpdateLocked()

	return joinRes, nil
}

func (a *RoomActor) Leave(ctx context.Context, participantID string) error {
	a.mu.Lock()
	defer a.mu.Unlock()

	return a.leaveLocked(ctx, participantID)
}

func (a *RoomActor) leaveLocked(ctx context.Context, participantID string) error {
	if a.closed {
		return nil // Ignore late leave after close
	}

	session, exists := a.sessions[participantID]
	if !exists {
		return domain.ErrParticipantNotFound
	}

	p := session.Participant()
	delete(a.sessions, participantID)
	_, _ = a.room.RemoveParticipant(participantID)
	_ = session.Close()

	// Inactivate all slots for this participant's tracks on remaining participants in a single batch (REQ-018)
	leavingTracks := p.ListTracks()
	for _, otherSession := range a.sessions {
		inactivatedCount := 0
		for _, track := range leavingTracks {
			if _, ok := otherSession.SlotTable().InactivateSlotByTrackSID(track.SID); ok {
				inactivatedCount++
			}
		}
		if inactivatedCount > 0 && otherSession.State() == domain.NegotiationStateStable {
			if err := otherSession.StartOfferTransition(); err == nil {
				if otherSession.Responder() != nil {
					_ = otherSession.Responder().SendSubscriberOffer(otherSession.SlotTable().ListSlots(), otherSession.Participant().SID)
				}
			}
		}
	}

	if a.room.RouterID != "" {
		if p.PublisherTransportID != "" {
			_ = a.transportPort.CloseTransport(ctx, a.room.RouterID, p.PublisherTransportID)
		}
		if p.SubscriberTransportID != "" {
			_ = a.transportPort.CloseTransport(ctx, a.room.RouterID, p.SubscriberTransportID)
		}
	}

	a.broadcastParticipantUpdateLocked()
	return nil
}

func (a *RoomActor) GetSession(participantID string) (*ParticipantSession, bool) {
	a.mu.Lock()
	defer a.mu.Unlock()

	session, ok := a.sessions[participantID]
	return session, ok
}

func (a *RoomActor) ListSessions() []*ParticipantSession {
	a.mu.Lock()
	defer a.mu.Unlock()

	sessions := make([]*ParticipantSession, 0, len(a.sessions))
	for _, s := range a.sessions {
		sessions = append(sessions, s)
	}
	return sessions
}

func (a *RoomActor) broadcastParticipantUpdateLocked() {
	participants := a.room.ListParticipants()
	for _, session := range a.sessions {
		if session.responder != nil {
			_ = session.responder.SendParticipantUpdate(participants)
		}
	}
}

func (a *RoomActor) PublishTracks(
	ctx context.Context,
	participantID string,
	dtlsRole string,
	fingerprints []domain.DtlsFingerprintInfo,
	tracks []domain.TrackPublishCommand,
) (*domain.TransportInfo, error) {
	a.mu.Lock()
	defer a.mu.Unlock()

	session, exists := a.sessions[participantID]
	if !exists {
		return nil, domain.ErrParticipantNotFound
	}
	p := session.Participant()

	// 1. Connect Publisher Transport on the SFU (once per transport lifecycle)
	if !session.PubTransportConnected() {
		err := a.transportPort.ConnectTransport(ctx, &domain.ConnectTransportCommand{
			RouterID:           a.room.RouterID,
			TransportID:        p.PublisherTransportID,
			DtlsRole:           dtlsRole,
			RemoteFingerprints: fingerprints,
		})
		if err != nil {
			return nil, fmt.Errorf("failed to connect publisher transport: %w", err)
		}
		session.SetPubTransportConnected(true)
	}

	publishedTracks := make([]*domain.Track, 0, len(tracks))

	// 2. Produce each NEW track on SFU node
	for _, tc := range tracks {
		var alreadyPublished *domain.Track
		if tc.SSRC > 0 {
			for _, existing := range p.ListTracks() {
				if existing.SSRC == tc.SSRC {
					alreadyPublished = existing
					break
				}
			}
		}
		if alreadyPublished != nil {
			continue
		}

		trackSID := fmt.Sprintf("TR_%s", uuid.NewString()[:8])
		reqID := uuid.NewString()

		prodRes, err := a.mediaStreamPort.Produce(ctx, &domain.ProduceCommand{
			RequestID:         reqID,
			RouterID:          a.room.RouterID,
			TransportID:       p.PublisherTransportID,
			PeerID:            participantID,
			Kind:              tc.Kind,
			RtpParametersJSON: tc.RtpParametersJSON,
			Paused:            false,
		})
		if err != nil {
			return nil, fmt.Errorf("failed to produce track %s: %w", tc.Kind, err)
		}

		domainTrack := &domain.Track{
			ID:                 prodRes.ProducerID,
			SID:                trackSID,
			Kind:               tc.Kind,
			SSRC:               tc.SSRC,
			RTXSSRC:            tc.RTXSSRC,
			PayloadType:        tc.PayloadType,
			RTXPayloadType:     tc.RTXPayloadType,
			MimeType:           tc.Codec,
			ClockRate:          tc.ClockRate,
			SimulcastEncodings: prodRes.Encodings,
		}
		_ = p.AddTrack(domainTrack)
		publishedTracks = append(publishedTracks, domainTrack)
	}

	if len(publishedTracks) > 0 {
		// Broadcast updated participant metadata (with newly published tracks) to all participants in the room
		a.broadcastParticipantUpdateLocked()

		// 3. Coordinate subscriptions on all OTHER participants in the room
		for _, otherSession := range a.sessions {
			if otherSession.Participant().ID == participantID {
				continue
			}

			if otherSession.State() == domain.NegotiationStateWaitingForAnswer {
				// Concurrency / Glare handling: queue tracks until current negotiation completes
				for _, t := range publishedTracks {
					otherSession.QueuePendingTrack(t)
				}
				continue
			}

			// Other session is Stable -> consume immediately
			for _, t := range publishedTracks {
				consRes, err := a.mediaStreamPort.Consume(ctx, &domain.ConsumeCommand{
					RequestID:             uuid.NewString(),
					RouterID:              a.room.RouterID,
					SubscriberTransportID: otherSession.Participant().SubscriberTransportID,
					ProducerID:            t.ID,
					RtpCapabilitiesJSON:   a.getRtpCapabilitiesJSON(),
					Paused:                false,
				})
				if err != nil {
					continue
				}

				otherSession.SlotTable().AllocOrRecycleSlot(
					t.Kind,
					t.SID,
					t.ID,
					consRes.ConsumerID,
					consRes.SSRC,
					consRes.RTXSSRC,
					consRes.PayloadType,
					consRes.RTXPayloadType,
					consRes.ClockRate,
					consRes.CodecMimeType,
					p.SID,
				)
				otherSession.Participant().AddSubscription(consRes.ConsumerID, t.ID)
			}

			if err := otherSession.StartOfferTransition(); err == nil {
				if otherSession.Responder() != nil {
					_ = otherSession.Responder().SendSubscriberOffer(otherSession.SlotTable().ListSlots(), p.SID)
				}
			}
		}
	}

	return session.PubTransport(), nil
}

func (a *RoomActor) HandleSubscriberAnswer(ctx context.Context, participantID string, dtlsRole string, fingerprints []domain.DtlsFingerprintInfo) error {
	a.mu.Lock()
	defer a.mu.Unlock()

	session, exists := a.sessions[participantID]
	if !exists {
		return domain.ErrParticipantNotFound
	}

	p := session.Participant()

	// Connect Subscriber Transport on the SFU (once per transport lifecycle)
	if len(fingerprints) > 0 && !session.SubTransportConnected() {
		err := a.transportPort.ConnectTransport(ctx, &domain.ConnectTransportCommand{
			RouterID:           a.room.RouterID,
			TransportID:        p.SubscriberTransportID,
			DtlsRole:           dtlsRole,
			RemoteFingerprints: fingerprints,
		})
		if err == nil {
			session.SetSubTransportConnected(true)
		}
	}

	if err := session.AnswerReceivedTransition(); err != nil {
		return err
	}

	// Drain any queued pending tracks
	pending := session.PopPendingTracks()
	if len(pending) > 0 {
		for _, t := range pending {
			consRes, err := a.mediaStreamPort.Consume(ctx, &domain.ConsumeCommand{
				RequestID:             uuid.NewString(),
				RouterID:              a.room.RouterID,
				SubscriberTransportID: p.SubscriberTransportID,
				ProducerID:            t.ID,
				RtpCapabilitiesJSON:   a.getRtpCapabilitiesJSON(),
				Paused:                false,
			})
			if err != nil {
				continue
			}

			session.SlotTable().AllocOrRecycleSlot(
				t.Kind,
				t.SID,
				t.ID,
				consRes.ConsumerID,
				consRes.SSRC,
				consRes.RTXSSRC,
				consRes.PayloadType,
				consRes.RTXPayloadType,
				consRes.ClockRate,
				consRes.CodecMimeType,
				p.SID,
			)
			p.AddSubscription(consRes.ConsumerID, t.ID)
		}

		if err := session.StartOfferTransition(); err == nil {
			if session.Responder() != nil {
				_ = session.Responder().SendSubscriberOffer(session.SlotTable().ListSlots(), p.SID)
			}
		}
	}

	return nil
}

func (a *RoomActor) HandleTrickle(ctx context.Context, participantID string, candidateJSON string, target int) error {
	a.mu.Lock()
	defer a.mu.Unlock()

	_, exists := a.sessions[participantID]
	if !exists {
		return domain.ErrParticipantNotFound
	}
	return nil
}

func (a *RoomActor) SetTrackMuted(ctx context.Context, participantID, trackSID string, muted bool) error {
	a.mu.Lock()
	defer a.mu.Unlock()

	session, exists := a.sessions[participantID]
	if !exists {
		return domain.ErrParticipantNotFound
	}

	p := session.Participant()
	if err := p.SetTrackMuted(trackSID, muted); err != nil {
		return err
	}

	track, ok := p.GetTrack(trackSID)
	if ok && track.ID != "" {
		_ = a.mediaStreamPort.SetMediaPaused(ctx, &domain.SetMediaPausedCommand{
			RouterID:     a.room.RouterID,
			ResourceType: domain.ResourceTypeProducer,
			ResourceID:   track.ID,
			Paused:       muted,
		})
	}

	// Notify other participants that this track was muted
	for _, s := range a.sessions {
		if s.Responder() != nil {
			_ = s.Responder().SendTrackMuted(trackSID, muted)
		}
	}
	return nil
}

func (a *RoomActor) SetConsumerLayers(ctx context.Context, participantID, consumerID string, spatial, temporal int32) error {
	a.mu.Lock()
	defer a.mu.Unlock()

	_, exists := a.sessions[participantID]
	if !exists {
		return domain.ErrParticipantNotFound
	}

	return a.mediaStreamPort.SetConsumerPreferredLayers(ctx, &domain.SetPreferredLayersCommand{
		RouterID:      a.room.RouterID,
		ConsumerID:    consumerID,
		SpatialLayer:  spatial,
		TemporalLayer: temporal,
	})
}

func (a *RoomActor) RestartPublisherIce(ctx context.Context, participantID string) (*domain.IceInfo, error) {
	a.mu.Lock()
	defer a.mu.Unlock()

	session, exists := a.sessions[participantID]
	if !exists {
		return nil, domain.ErrParticipantNotFound
	}

	p := session.Participant()
	return a.transportPort.RestartIce(ctx, a.room.RouterID, p.PublisherTransportID)
}

func (a *RoomActor) RestartSubscriberIce(ctx context.Context, participantID string) (*domain.IceInfo, error) {
	a.mu.Lock()
	defer a.mu.Unlock()

	session, exists := a.sessions[participantID]
	if !exists {
		return nil, domain.ErrParticipantNotFound
	}

	p := session.Participant()
	iceInfo, err := a.transportPort.RestartIce(ctx, a.room.RouterID, p.SubscriberTransportID)
	if err != nil {
		return nil, err
	}

	subTransport := session.SubTransport()
	if subTransport != nil {
		subTransport.IceUfrag = iceInfo.IceUfrag
		subTransport.IcePwd = iceInfo.IcePwd
	}

	// Send updated subscriber offer with new ICE credentials
	if session.State() == domain.NegotiationStateStable {
		if err := session.StartOfferTransition(); err == nil {
			if session.Responder() != nil {
				_ = session.Responder().SendSubscriberOffer(session.SlotTable().ListSlots(), p.SID)
			}
		}
	}

	return iceInfo, nil
}

func (a *RoomActor) Close(ctx context.Context) error {
	a.mu.Lock()
	defer a.mu.Unlock()

	if a.closed {
		return nil
	}
	a.closed = true

	for pid, session := range a.sessions {
		p := session.Participant()
		_ = session.Close()
		if a.room.RouterID != "" {
			if p.PublisherTransportID != "" {
				_ = a.transportPort.CloseTransport(ctx, a.room.RouterID, p.PublisherTransportID)
			}
			if p.SubscriberTransportID != "" {
				_ = a.transportPort.CloseTransport(ctx, a.room.RouterID, p.SubscriberTransportID)
			}
		}
		delete(a.sessions, pid)
	}

	_ = a.room.Close()
	if a.room.RouterID != "" {
		_ = a.routerPort.CloseRouter(ctx, a.room.RouterID)
	}
	return nil
}
