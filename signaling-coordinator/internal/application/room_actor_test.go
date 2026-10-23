package application_test

import (
	"context"
	"fmt"
	"sync"
	"testing"

	"ekbatan-room/signaling-coordinator/internal/application"
	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

type mockOutboundEngine struct {
	mu sync.Mutex

	createdRouters   []string
	closedRouters    []string
	createdTransports []*domain.CreateTransportCommand
	closedTransports []*closedTransportEntry
	produced         []*domain.ProduceCommand
	consumed         []*domain.ConsumeCommand
}

type closedTransportEntry struct {
	RouterID    string
	TransportID string
}

func newMockOutboundEngine() *mockOutboundEngine {
	return &mockOutboundEngine{
		createdRouters:    make([]string, 0),
		closedRouters:     make([]string, 0),
		createdTransports: make([]*domain.CreateTransportCommand, 0),
		closedTransports:  make([]*closedTransportEntry, 0),
		produced:          make([]*domain.ProduceCommand, 0),
		consumed:          make([]*domain.ConsumeCommand, 0),
	}
}

// RouterEnginePort
func (m *mockOutboundEngine) CreateRouter(ctx context.Context, roomID string) (*domain.RouterInfo, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	routerID := fmt.Sprintf("router-%s", roomID)
	m.createdRouters = append(m.createdRouters, routerID)
	return &domain.RouterInfo{
		RouterID:    routerID,
		WorkerIndex: 0,
	}, nil
}

func (m *mockOutboundEngine) CloseRouter(ctx context.Context, routerID string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.closedRouters = append(m.closedRouters, routerID)
	return nil
}

// TransportEnginePort
func (m *mockOutboundEngine) CreateTransport(ctx context.Context, cmd *domain.CreateTransportCommand) (*domain.TransportInfo, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.createdTransports = append(m.createdTransports, cmd)

	prefix := "sub"
	if cmd.IsPublisher {
		prefix = "pub"
	}
	transportID := fmt.Sprintf("%s-%s", prefix, cmd.PeerID)

	return &domain.TransportInfo{
		TransportID: transportID,
		IceUfrag:    "ufrag-test",
		IcePwd:      "pwd-test",
	}, nil
}

func (m *mockOutboundEngine) ConnectTransport(ctx context.Context, cmd *domain.ConnectTransportCommand) error {
	return nil
}

func (m *mockOutboundEngine) RestartIce(ctx context.Context, routerID, transportID string) (*domain.IceInfo, error) {
	return &domain.IceInfo{
		IceUfrag: "ufrag-new",
		IcePwd:   "pwd-new",
	}, nil
}

func (m *mockOutboundEngine) CloseTransport(ctx context.Context, routerID, transportID string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.closedTransports = append(m.closedTransports, &closedTransportEntry{
		RouterID:    routerID,
		TransportID: transportID,
	})
	return nil
}

// MediaStreamPort
func (m *mockOutboundEngine) Produce(ctx context.Context, cmd *domain.ProduceCommand) (*domain.ProduceResult, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.produced = append(m.produced, cmd)
	return &domain.ProduceResult{
		ProducerID: fmt.Sprintf("prod-%s", cmd.PeerID),
		Kind:       cmd.Kind,
	}, nil
}

func (m *mockOutboundEngine) Consume(ctx context.Context, cmd *domain.ConsumeCommand) (*domain.ConsumeResult, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.consumed = append(m.consumed, cmd)
	return &domain.ConsumeResult{
		ConsumerID: fmt.Sprintf("cons-%s", cmd.ProducerID),
		ProducerID: cmd.ProducerID,
	}, nil
}

func (m *mockOutboundEngine) SetMediaPaused(ctx context.Context, cmd *domain.SetMediaPausedCommand) error {
	return nil
}

func (m *mockOutboundEngine) SetConsumerPreferredLayers(ctx context.Context, cmd *domain.SetPreferredLayersCommand) error {
	return nil
}

func (m *mockOutboundEngine) CloseResource(ctx context.Context, routerID string, resourceType domain.ResourceType, resourceID string) error {
	return nil
}

var (
	_ ports.RouterEnginePort    = (*mockOutboundEngine)(nil)
	_ ports.TransportEnginePort = (*mockOutboundEngine)(nil)
	_ ports.MediaStreamPort     = (*mockOutboundEngine)(nil)
)

func TestRoomActor_JoinAndLeaveLifecycle(t *testing.T) {
	engine := newMockOutboundEngine()
	iceServers := []domain.IceServer{{URLs: []string{"stun:stun.example.com"}}}

	actor := application.NewRoomActor("test-room", engine, engine, engine, iceServers)

	resp1 := newMockResponder()
	ctx := context.Background()

	// 1. First participant joins (triggers Router creation)
	joinRes1, err := actor.Join(ctx, "user-1", "Alice", resp1)
	if err != nil {
		t.Fatalf("Join user-1 failed: %v", err)
	}

	if len(engine.createdRouters) != 1 {
		t.Fatalf("expected 1 router created, got %d", len(engine.createdRouters))
	}
	if joinRes1.PublisherTransport.TransportID != "pub-user-1" {
		t.Fatalf("expected pub-user-1, got %s", joinRes1.PublisherTransport.TransportID)
	}
	if joinRes1.SubscriberTransport.TransportID != "sub-user-1" {
		t.Fatalf("expected sub-user-1, got %s", joinRes1.SubscriberTransport.TransportID)
	}
	if resp1.joinResp == nil {
		t.Fatalf("expected join response sent to responder 1")
	}

	// 2. Second participant joins (reuses existing Router)
	resp2 := newMockResponder()
	joinRes2, err := actor.Join(ctx, "user-2", "Bob", resp2)
	if err != nil {
		t.Fatalf("Join user-2 failed: %v", err)
	}

	if len(engine.createdRouters) != 1 {
		t.Fatalf("expected router to be reused, still 1, got %d", len(engine.createdRouters))
	}
	if joinRes2.PublisherTransport.TransportID != "pub-user-2" {
		t.Fatalf("expected pub-user-2, got %s", joinRes2.PublisherTransport.TransportID)
	}

	// Verify participant updates were broadcast to both
	if len(resp1.partsUpdate) == 0 || len(resp2.partsUpdate) == 0 {
		t.Fatalf("expected participant updates broadcast")
	}

	// 3. User 1 leaves
	if err := actor.Leave(ctx, "user-1"); err != nil {
		t.Fatalf("Leave user-1 failed: %v", err)
	}

	if len(engine.closedTransports) != 2 {
		t.Fatalf("expected 2 transports closed for user-1, got %d", len(engine.closedTransports))
	}

	// 4. Close Room
	if err := actor.Close(ctx); err != nil {
		t.Fatalf("Close room failed: %v", err)
	}

	if len(engine.closedRouters) != 1 {
		t.Fatalf("expected 1 router closed, got %d", len(engine.closedRouters))
	}

	// 5. Late join after close returns ErrRoomClosed
	resp3 := newMockResponder()
	_, err = actor.Join(ctx, "user-3", "Charlie", resp3)
	if err == nil {
		t.Fatalf("expected error on late join, got nil")
	}
}

func TestRoomActor_TwoPeerPublishSubscribeSession(t *testing.T) {
	engine := newMockOutboundEngine()
	iceServers := []domain.IceServer{{URLs: []string{"stun:stun.example.com"}}}
	actor := application.NewRoomActor("conference-2p", engine, engine, engine, iceServers)
	ctx := context.Background()

	respAlice := newMockResponder()
	respBob := newMockResponder()

	// 1. Alice and Bob Join
	_, err := actor.Join(ctx, "alice", "Alice", respAlice)
	if err != nil {
		t.Fatalf("Alice join failed: %v", err)
	}
	_, err = actor.Join(ctx, "bob", "Bob", respBob)
	if err != nil {
		t.Fatalf("Bob join failed: %v", err)
	}

	bobSession, _ := actor.GetSession("bob")
	aliceSession, _ := actor.GetSession("alice")

	if bobSession.State() != domain.NegotiationStateStable {
		t.Fatalf("expected Bob initial state Stable, got %s", bobSession.State())
	}
	if aliceSession.State() != domain.NegotiationStateStable {
		t.Fatalf("expected Alice initial state Stable, got %s", aliceSession.State())
	}

	// 2. Alice publishes Audio and Video tracks
	fingerprints := []domain.DtlsFingerprintInfo{
		{Algorithm: "sha-256", Value: "AA:BB:CC:DD"},
	}
	aliceTracks := []domain.TrackPublishCommand{
		{
			Kind:              domain.TrackKindAudio,
			SSRC:              1111,
			PayloadType:       111,
			Codec:             "opus",
			ClockRate:         48000,
			RtpParametersJSON: `{"codecs":[{"mimeType":"audio/opus"}]}`,
		},
		{
			Kind:              domain.TrackKindVideo,
			SSRC:              2222,
			RTXSSRC:           2223,
			PayloadType:       96,
			RTXPayloadType:    97,
			Codec:             "VP8",
			ClockRate:         90000,
			RtpParametersJSON: `{"codecs":[{"mimeType":"video/VP8"}]}`,
		},
	}

	_, err = actor.PublishTracks(ctx, "alice", "client", fingerprints, aliceTracks)
	if err != nil {
		t.Fatalf("Alice PublishTracks failed: %v", err)
	}

	// Engine must have produced 2 tracks
	if len(engine.produced) != 2 {
		t.Fatalf("expected 2 tracks produced, got %d", len(engine.produced))
	}
	// Bob must have consumed 2 tracks
	if len(engine.consumed) != 2 {
		t.Fatalf("expected Bob to consume 2 tracks, got %d", len(engine.consumed))
	}
	for _, cmd := range engine.consumed {
		if cmd.RtpCapabilitiesJSON == "" || cmd.RtpCapabilitiesJSON == `{"codecs":[]}` {
			t.Fatalf("expected valid non-empty codecs in consume RtpCapabilitiesJSON, got: %s", cmd.RtpCapabilitiesJSON)
		}
	}

	// Bob must be in WaitingForAnswer state
	if bobSession.State() != domain.NegotiationStateWaitingForAnswer {
		t.Fatalf("expected Bob state WaitingForAnswer, got %s", bobSession.State())
	}
	// Bob responder must have received an offer
	if len(respBob.offers) != 1 {
		t.Fatalf("expected Bob to receive 1 subscriber offer, got %d", len(respBob.offers))
	}

	// 3. Bob responds with Subscriber Answer
	err = actor.HandleSubscriberAnswer(ctx, "bob", "client", fingerprints)
	if err != nil {
		t.Fatalf("Bob HandleSubscriberAnswer failed: %v", err)
	}
	if bobSession.State() != domain.NegotiationStateStable {
		t.Fatalf("expected Bob state to return to Stable, got %s", bobSession.State())
	}

	// 4. Bob publishes his Audio track
	bobTracks := []domain.TrackPublishCommand{
		{
			Kind:              domain.TrackKindAudio,
			SSRC:              3333,
			PayloadType:       111,
			Codec:             "opus",
			ClockRate:         48000,
			RtpParametersJSON: `{"codecs":[{"mimeType":"audio/opus"}]}`,
		},
	}
	_, err = actor.PublishTracks(ctx, "bob", "client", fingerprints, bobTracks)
	if err != nil {
		t.Fatalf("Bob PublishTracks failed: %v", err)
	}

	// Alice must now be in WaitingForAnswer
	if aliceSession.State() != domain.NegotiationStateWaitingForAnswer {
		t.Fatalf("expected Alice state WaitingForAnswer, got %s", aliceSession.State())
	}
	if len(respAlice.offers) != 1 {
		t.Fatalf("expected Alice to receive 1 subscriber offer, got %d", len(respAlice.offers))
	}

	// 5. Glare test: Bob publishes another track while Alice is still WaitingForAnswer
	bobVideoTrack := []domain.TrackPublishCommand{
		{
			Kind:              domain.TrackKindVideo,
			SSRC:              4444,
			PayloadType:       96,
			Codec:             "VP8",
			ClockRate:         90000,
			RtpParametersJSON: `{"codecs":[{"mimeType":"video/VP8"}]}`,
		},
	}
	_, err = actor.PublishTracks(ctx, "bob", "client", fingerprints, bobVideoTrack)
	if err != nil {
		t.Fatalf("Bob second publish failed: %v", err)
	}

	// Alice must NOT receive a conflicting offer yet (still 1 offer)
	if len(respAlice.offers) != 1 {
		t.Fatalf("expected Alice to still have 1 offer during glare, got %d", len(respAlice.offers))
	}

	// 6. Alice sends answer to first offer -> triggers automatic drain and second offer!
	err = actor.HandleSubscriberAnswer(ctx, "alice", "client", fingerprints)
	if err != nil {
		t.Fatalf("Alice HandleSubscriberAnswer failed: %v", err)
	}

	// Now Alice has received her 2nd offer after draining pending track!
	if len(respAlice.offers) != 2 {
		t.Fatalf("expected Alice to receive 2nd offer after answer drain, got %d", len(respAlice.offers))
	}

	// Alice answers 2nd offer -> returns to Stable
	err = actor.HandleSubscriberAnswer(ctx, "alice", "client", fingerprints)
	if err != nil {
		t.Fatalf("Alice HandleSubscriberAnswer 2 failed: %v", err)
	}
	if aliceSession.State() != domain.NegotiationStateStable {
		t.Fatalf("expected Alice to return to Stable, got %s", aliceSession.State())
	}
}

