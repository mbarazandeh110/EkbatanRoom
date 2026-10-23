package application

import (
	"sync"

	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

// ParticipantSession manages the lifecycle and SDP negotiation state machine for a single participant.
// States: Stable -> WaitingForAnswer -> Stable (or Closed).
type ParticipantSession struct {
	mu sync.Mutex

	participant      *domain.Participant
	pubTransport     *domain.TransportInfo
	subTransport     *domain.TransportInfo
	responder        ports.PeerResponderPort
	negotiationState domain.NegotiationState
	slotTable             *domain.SDPSlotTable
	pendingTracks         []*domain.Track
	closed                bool
	pubTransportConnected bool
	subTransportConnected bool
}

func NewParticipantSession(
	p *domain.Participant,
	pubTransport *domain.TransportInfo,
	subTransport *domain.TransportInfo,
	responder ports.PeerResponderPort,
) *ParticipantSession {
	return &ParticipantSession{
		participant:      p,
		pubTransport:     pubTransport,
		subTransport:     subTransport,
		responder:        responder,
		negotiationState: domain.NegotiationStateStable,
		slotTable:        domain.NewSDPSlotTable(),
		pendingTracks:    make([]*domain.Track, 0),
	}
}

func (s *ParticipantSession) Participant() *domain.Participant {
	return s.participant
}

func (s *ParticipantSession) PubTransport() *domain.TransportInfo {
	return s.pubTransport
}

func (s *ParticipantSession) SubTransport() *domain.TransportInfo {
	return s.subTransport
}

func (s *ParticipantSession) Responder() ports.PeerResponderPort {
	return s.responder
}

func (s *ParticipantSession) SlotTable() *domain.SDPSlotTable {
	return s.slotTable
}

func (s *ParticipantSession) PubTransportConnected() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.pubTransportConnected
}

func (s *ParticipantSession) SetPubTransportConnected(connected bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.pubTransportConnected = connected
}

func (s *ParticipantSession) SubTransportConnected() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.subTransportConnected
}

func (s *ParticipantSession) SetSubTransportConnected(connected bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.subTransportConnected = connected
}

func (s *ParticipantSession) State() domain.NegotiationState {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.negotiationState
}

// StartOfferTransition transitions from Stable -> WaitingForAnswer.
// Returns ErrNegotiationGlare if already in WaitingForAnswer.
func (s *ParticipantSession) StartOfferTransition() error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.closed {
		return domain.ErrRoomClosed
	}

	if s.negotiationState == domain.NegotiationStateWaitingForAnswer {
		return domain.ErrNegotiationGlare
	}

	s.negotiationState = domain.NegotiationStateWaitingForAnswer
	return nil
}

// AnswerReceivedTransition transitions from WaitingForAnswer -> Stable.
func (s *ParticipantSession) AnswerReceivedTransition() error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.closed {
		return domain.ErrRoomClosed
	}

	if s.negotiationState != domain.NegotiationStateWaitingForAnswer {
		return domain.ErrInvalidNegotiationState
	}

	s.negotiationState = domain.NegotiationStateStable
	return nil
}

// QueuePendingTrack queues a track change when renegotiation is deferred due to glare.
func (s *ParticipantSession) QueuePendingTrack(t *domain.Track) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.pendingTracks = append(s.pendingTracks, t)
}

// PopPendingTracks drains the queued pending tracks once the state returns to Stable.
func (s *ParticipantSession) PopPendingTracks() []*domain.Track {
	s.mu.Lock()
	defer s.mu.Unlock()

	if len(s.pendingTracks) == 0 {
		return nil
	}
	tracks := s.pendingTracks
	s.pendingTracks = make([]*domain.Track, 0)
	return tracks
}

func (s *ParticipantSession) Close() error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if s.closed {
		return nil
	}
	s.closed = true
	s.negotiationState = domain.NegotiationStateClosed
	if s.responder != nil {
		_ = s.responder.Close()
	}
	return nil
}
