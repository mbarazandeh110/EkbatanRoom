package application_test

import (
	"errors"
	"testing"

	"ekbatan-room/signaling-coordinator/internal/application"
	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

type qualityUpdateRecord struct {
	TrackSID  string
	Codec     string
	Qualities []domain.SubscribedQuality
}

type mockResponder struct {
	ports.PeerResponderPort
	joinResp       *domain.JoinResult
	offers         []string
	trickles       []string
	partsUpdate    [][]*domain.Participant
	mutedTracks      map[string]bool
	qualityUpdates   []qualityUpdateRecord
	subscribedTracks []string
	closed           bool
}

func newMockResponder() *mockResponder {
	return &mockResponder{
		offers:           make([]string, 0),
		trickles:         make([]string, 0),
		partsUpdate:      make([][]*domain.Participant, 0),
		mutedTracks:      make(map[string]bool),
		qualityUpdates:   make([]qualityUpdateRecord, 0),
		subscribedTracks: make([]string, 0),
	}
}

func (m *mockResponder) SendJoinResponse(res *domain.JoinResult) error {
	m.joinResp = res
	return nil
}

func (m *mockResponder) SendSubscriberOffer(slots []*domain.SDPSlot, participantSID string) error {
	m.offers = append(m.offers, participantSID)
	return nil
}

func (m *mockResponder) SendTrickle(candidate string, target int) error {
	m.trickles = append(m.trickles, candidate)
	return nil
}

func (m *mockResponder) SendParticipantUpdate(participants []*domain.Participant) error {
	m.partsUpdate = append(m.partsUpdate, participants)
	return nil
}

func (m *mockResponder) SendTrackMuted(trackSID string, muted bool) error {
	m.mutedTracks[trackSID] = muted
	return nil
}

func (m *mockResponder) SendSubscribedQualityUpdate(trackSID string, codec string, qualities []domain.SubscribedQuality) error {
	m.qualityUpdates = append(m.qualityUpdates, qualityUpdateRecord{
		TrackSID:  trackSID,
		Codec:     codec,
		Qualities: qualities,
	})
	return nil
}

func (m *mockResponder) SendTrackSubscribed(trackSID string) error {
	m.subscribedTracks = append(m.subscribedTracks, trackSID)
	return nil
}

func (m *mockResponder) Close() error {
	m.closed = true
	return nil
}

func TestParticipantSession_StateTransitions(t *testing.T) {
	p := domain.NewParticipant("user-1", "PA_1", "Alice")
	responder := newMockResponder()
	session := application.NewParticipantSession(p, nil, nil, responder)

	if session.State() != domain.NegotiationStateStable {
		t.Fatalf("expected initial state Stable, got %s", session.State())
	}

	// 1. Transition to WaitingForAnswer
	if err := session.StartOfferTransition(); err != nil {
		t.Fatalf("failed transition to WaitingForAnswer: %v", err)
	}
	if session.State() != domain.NegotiationStateWaitingForAnswer {
		t.Fatalf("expected state WaitingForAnswer, got %s", session.State())
	}

	// 2. Glare check: cannot start offer while WaitingForAnswer
	err := session.StartOfferTransition()
	if !errors.Is(err, domain.ErrNegotiationGlare) {
		t.Fatalf("expected ErrNegotiationGlare, got %v", err)
	}

	// 3. Queue pending track during glare
	pendingTrack := &domain.Track{
		SID:  "TR_123",
		Kind: domain.TrackKindVideo,
	}
	session.QueuePendingTrack(pendingTrack)

	// 4. Receive answer, return to Stable
	if err := session.AnswerReceivedTransition(); err != nil {
		t.Fatalf("failed transition to Stable: %v", err)
	}
	if session.State() != domain.NegotiationStateStable {
		t.Fatalf("expected state Stable, got %s", session.State())
	}

	// 5. Drain pending tracks
	drained := session.PopPendingTracks()
	if len(drained) != 1 || drained[0].SID != "TR_123" {
		t.Fatalf("expected 1 drained track with SID TR_123, got %v", drained)
	}

	// 6. Close session
	if err := session.Close(); err != nil {
		t.Fatalf("failed to close session: %v", err)
	}
	if session.State() != domain.NegotiationStateClosed {
		t.Fatalf("expected state Closed, got %s", session.State())
	}
	if !responder.closed {
		t.Fatalf("expected responder to be closed")
	}
}
