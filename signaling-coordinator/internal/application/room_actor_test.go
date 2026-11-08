package application_test

import (
	"context"
	"fmt"
	"strings"
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
	connectedTransports []*domain.ConnectTransportCommand
	closedTransports    []*closedTransportEntry
	produced            []*domain.ProduceCommand
	consumed            []*domain.ConsumeCommand
	preferredLayers     []*domain.SetPreferredLayersCommand
	pausedMedia         []*domain.SetMediaPausedCommand
}

type closedTransportEntry struct {
	RouterID    string
	TransportID string
}

func newMockOutboundEngine() *mockOutboundEngine {
	return &mockOutboundEngine{
		createdRouters:      make([]string, 0),
		closedRouters:       make([]string, 0),
		createdTransports:   make([]*domain.CreateTransportCommand, 0),
		connectedTransports: make([]*domain.ConnectTransportCommand, 0),
		closedTransports:    make([]*closedTransportEntry, 0),
		produced:            make([]*domain.ProduceCommand, 0),
		consumed:            make([]*domain.ConsumeCommand, 0),
		preferredLayers:     make([]*domain.SetPreferredLayersCommand, 0),
		pausedMedia:         make([]*domain.SetMediaPausedCommand, 0),
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
	m.mu.Lock()
	defer m.mu.Unlock()
	m.connectedTransports = append(m.connectedTransports, cmd)
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
		ProducerID: fmt.Sprintf("prod-%s-%s", cmd.PeerID, cmd.Kind),
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
	m.mu.Lock()
	defer m.mu.Unlock()
	m.pausedMedia = append(m.pausedMedia, cmd)
	return nil
}

func (m *mockOutboundEngine) SetConsumerPreferredLayers(ctx context.Context, cmd *domain.SetPreferredLayersCommand) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.preferredLayers = append(m.preferredLayers, cmd)
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

	// Verify user-2 received participant update with user-1 as Disconnected
	if len(resp2.partsUpdate) < 2 {
		t.Fatalf("expected user-2 to receive at least 2 participant updates, got %d", len(resp2.partsUpdate))
	}
	latestUpdate := resp2.partsUpdate[len(resp2.partsUpdate)-1]
	foundUser1Disconnected := false
	for _, p := range latestUpdate {
		if p.ID == "user-1" && p.GetState() == domain.ParticipantStateDisconnected {
			foundUser1Disconnected = true
			break
		}
	}
	if !foundUser1Disconnected {
		t.Fatalf("expected user-2 to receive participant update with user-1 disconnected, got: %+v", latestUpdate)
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
		// REQ-020: Verify rtcpFeedback with PLI for video keyframe requests
		if !strings.Contains(cmd.RtpCapabilitiesJSON, `"rtcpFeedback"`) {
			t.Fatalf("expected consume RtpCapabilitiesJSON to contain rtcpFeedback, got: %s", cmd.RtpCapabilitiesJSON)
		}
		if !strings.Contains(cmd.RtpCapabilitiesJSON, `"parameter":"pli"`) {
			t.Fatalf("expected consume RtpCapabilitiesJSON to contain PLI feedback, got: %s", cmd.RtpCapabilitiesJSON)
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

	// Verify REQ-016: Bob must have received a ParticipantUpdate where Alice has 2 published tracks
	if len(respBob.partsUpdate) == 0 {
		t.Fatalf("expected Bob to receive participant updates, got none")
	}
	latestUpdate := respBob.partsUpdate[len(respBob.partsUpdate)-1]
	var aliceInUpdate *domain.Participant
	for _, p := range latestUpdate {
		if p.ID == "alice" {
			aliceInUpdate = p
			break
		}
	}
	if aliceInUpdate == nil {
		t.Fatalf("expected Alice in Bob's participant update")
	}
	if len(aliceInUpdate.ListTracks()) != 2 {
		t.Fatalf("expected Alice to have 2 published tracks in participant update, got %d", len(aliceInUpdate.ListTracks()))
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

	// Verify REQ-011: ConnectTransport must be called at most once per transport lifecycle
	// Bob published twice, Alice answered twice, but their transports should each only connect once
	engine.mu.Lock()
	bobPubConnects := 0
	alicePubConnects := 0
	bobSubConnects := 0
	aliceSubConnects := 0
	for _, c := range engine.connectedTransports {
		switch c.TransportID {
		case "pub-bob":
			bobPubConnects++
		case "pub-alice":
			alicePubConnects++
		case "sub-bob":
			bobSubConnects++
		case "sub-alice":
			aliceSubConnects++
		}
	}
	engine.mu.Unlock()

	if bobPubConnects != 1 {
		t.Fatalf("expected pub-bob to connect exactly once, connected %d times", bobPubConnects)
	}
	if alicePubConnects != 1 {
		t.Fatalf("expected pub-alice to connect exactly once, connected %d times", alicePubConnects)
	}
	if bobSubConnects != 1 {
		t.Fatalf("expected sub-bob to connect exactly once, connected %d times", bobSubConnects)
	}
	if aliceSubConnects != 1 {
		t.Fatalf("expected sub-alice to connect exactly once, connected %d times", aliceSubConnects)
	}
}

func TestRoomManager_LifecycleAndRouting(t *testing.T) {
	ctx := context.Background()
	engine := newMockOutboundEngine()
	iceServers := []domain.IceServer{{URLs: []string{"stun:stun.l.google.com:19302"}}}

	manager := application.NewRoomManager(engine, engine, engine, iceServers)

	respAlice := newMockResponder()
	joinRes, err := manager.Join(ctx, "room-mgr-1", "alice", "Alice", respAlice)
	if err != nil {
		t.Fatalf("manager.Join failed: %v", err)
	}
	if joinRes.Participant.ID != "alice" {
		t.Fatalf("expected participant alice, got %s", joinRes.Participant.ID)
	}

	fingerprints := []domain.DtlsFingerprintInfo{
		{Algorithm: "sha-256", Value: "11:22:33:44:55"},
	}

	tracks := []domain.TrackPublishCommand{
		{
			Kind:              domain.TrackKindAudio,
			SSRC:              1234,
			PayloadType:       111,
			Codec:             "opus",
			ClockRate:         48000,
			RtpParametersJSON: `{"codecs":[{"mimeType":"audio/opus"}]}`,
		},
	}

	pubTransport, err := manager.PublishTracks(ctx, "room-mgr-1", "alice", "client", fingerprints, tracks)
	if err != nil {
		t.Fatalf("manager.PublishTracks failed: %v", err)
	}
	if pubTransport == nil {
		t.Fatal("expected non-nil pubTransport")
	}

	aliceTracks := joinRes.Participant.ListTracks()
	if len(aliceTracks) > 0 {
		err = manager.SetTrackMuted(ctx, "room-mgr-1", "alice", aliceTracks[0].SID, true)
		if err != nil {
			t.Fatalf("manager.SetTrackMuted failed: %v", err)
		}
	}

	err = manager.Leave(ctx, "room-mgr-1", "alice")
	if err != nil {
		t.Fatalf("manager.Leave failed: %v", err)
	}

	// Operations on non-existent room should return ErrRoomNotFound
	err = manager.Leave(ctx, "non-existent-room", "alice")
	if err != domain.ErrRoomNotFound {
		t.Fatalf("expected ErrRoomNotFound, got %v", err)
	}
}

func TestRoomActor_PublishTracks_UnifiedPlanRenegotiationDuplicateSSRCFiltered(t *testing.T) {
	ctx := context.Background()
	engine := newMockOutboundEngine()
	iceServers := []domain.IceServer{{URLs: []string{"stun:stun.example.com"}}}
	actor := application.NewRoomActor("room-reneg", engine, engine, engine, iceServers)

	respAlice := newMockResponder()
	fingerprints := []domain.DtlsFingerprintInfo{
		{Algorithm: "sha-256", Value: "AA:BB:CC"},
	}

	_, err := actor.Join(ctx, "alice", "Alice", respAlice)
	if err != nil {
		t.Fatalf("Join failed: %v", err)
	}

	// 1. Initial Offer: Audio track only (SSRC 1001)
	offer1 := []domain.TrackPublishCommand{
		{
			Kind:              domain.TrackKindAudio,
			SSRC:              1001,
			PayloadType:       111,
			Codec:             "opus",
			ClockRate:         48000,
			RtpParametersJSON: `{"codecs":[{"mimeType":"audio/opus","payloadType":111,"clockRate":48000,"channels":2}]}`,
		},
	}
	_, err = actor.PublishTracks(ctx, "alice", "client", fingerprints, offer1)
	if err != nil {
		t.Fatalf("Offer 1 PublishTracks failed: %v", err)
	}

	engine.mu.Lock()
	produceCount1 := len(engine.produced)
	engine.mu.Unlock()
	if produceCount1 != 1 {
		t.Fatalf("expected 1 produce call after offer 1, got %d", produceCount1)
	}

	// 2. Unified Plan Renegotiation: Browser sends BOTH Audio (SSRC 1001) and Video (SSRC 2001)
	offer2 := []domain.TrackPublishCommand{
		{
			Kind:              domain.TrackKindAudio,
			SSRC:              1001,
			PayloadType:       111,
			Codec:             "opus",
			ClockRate:         48000,
			RtpParametersJSON: `{"codecs":[{"mimeType":"audio/opus","payloadType":111,"clockRate":48000,"channels":2}]}`,
		},
		{
			Kind:              domain.TrackKindVideo,
			SSRC:              2001,
			PayloadType:       96,
			Codec:             "VP8",
			ClockRate:         90000,
			RtpParametersJSON: `{"codecs":[{"mimeType":"video/VP8","payloadType":96,"clockRate":90000}]}`,
		},
	}
	_, err = actor.PublishTracks(ctx, "alice", "client", fingerprints, offer2)
	if err != nil {
		t.Fatalf("Offer 2 PublishTracks failed: %v", err)
	}

	engine.mu.Lock()
	produceCount2 := len(engine.produced)
	lastProduced := engine.produced[produceCount2-1]
	engine.mu.Unlock()

	// Crucial REQ-014 assertion: only 1 new track should be produced, not 2! Total produced should be 2.
	if produceCount2 != 2 {
		t.Fatalf("expected total 2 produce calls after offer 2, got %d (audio should have been filtered out!)", produceCount2)
	}
	if lastProduced.Kind != domain.TrackKindVideo {
		t.Fatalf("expected 2nd produce to be video, got %s", lastProduced.Kind)
	}

	// 3. Subsequent Renegotiation with identical tracks (no new tracks)
	_, err = actor.PublishTracks(ctx, "alice", "client", fingerprints, offer2)
	if err != nil {
		t.Fatalf("Offer 3 PublishTracks failed: %v", err)
	}

	engine.mu.Lock()
	produceCount3 := len(engine.produced)
	engine.mu.Unlock()
	if produceCount3 != 2 {
		t.Fatalf("expected total produce calls to remain 2 after duplicate offer, got %d", produceCount3)
	}
}

func TestRoomActor_PublishTracks_ExplicitTrackSID(t *testing.T) {
	ctx := context.Background()
	engine := newMockOutboundEngine()
	iceServers := []domain.IceServer{{URLs: []string{"stun:stun.example.com"}}}
	actor := application.NewRoomActor("conf-tracksid", engine, engine, engine, iceServers)

	resp := newMockResponder()
	_, err := actor.Join(ctx, "alice", "Alice", resp)
	if err != nil {
		t.Fatalf("Join failed: %v", err)
	}

	fingerprints := []domain.DtlsFingerprintInfo{{Algorithm: "sha-256", Value: "AA:BB:CC"}}
	explicitSID := "TR_explicit_mic_123"

	tracks := []domain.TrackPublishCommand{
		{
			TrackSID:          explicitSID,
			Kind:              domain.TrackKindAudio,
			SSRC:              5555,
			PayloadType:       111,
			Codec:             "opus",
			ClockRate:         48000,
			RtpParametersJSON: `{"codecs":[{"mimeType":"audio/opus"}]}`,
		},
	}

	_, err = actor.PublishTracks(ctx, "alice", "client", fingerprints, tracks)
	if err != nil {
		t.Fatalf("PublishTracks failed: %v", err)
	}

	session, ok := actor.GetSession("alice")
	if !ok {
		t.Fatalf("expected session for alice")
	}

	track, ok := session.Participant().GetTrack(explicitSID)
	if !ok {
		t.Fatalf("expected track with explicit SID %s to be registered in participant", explicitSID)
	}
	if track.SID != explicitSID {
		t.Fatalf("expected track.SID == %s, got %s", explicitSID, track.SID)
	}
	if track.PublisherSID != session.Participant().SID {
		t.Fatalf("expected track.PublisherSID == %s, got %s", session.Participant().SID, track.PublisherSID)
	}

	// Verify Mute succeeds using explicit TrackSID
	if err := actor.SetTrackMuted(ctx, "alice", explicitSID, true); err != nil {
		t.Fatalf("SetTrackMuted failed with explicit TrackSID: %v", err)
	}
	mutedTrack, ok := session.Participant().GetTrack(explicitSID)
	if !ok || !mutedTrack.Paused {
		t.Fatalf("expected track to be paused/muted")
	}
}

func TestRoomActor_SetConsumerLayers_ResolvesTrackSIDToConsumerID(t *testing.T) {
	ctx := context.Background()
	engine := newMockOutboundEngine()
	iceServers := []domain.IceServer{{URLs: []string{"stun:stun.example.com"}}}
	actor := application.NewRoomActor("conf-layers", engine, engine, engine, iceServers)

	respAlice := newMockResponder()
	respBob := newMockResponder()

	_, _ = actor.Join(ctx, "alice", "Alice", respAlice)
	_, _ = actor.Join(ctx, "bob", "Bob", respBob)

	fingerprints := []domain.DtlsFingerprintInfo{{Algorithm: "sha-256", Value: "AA:BB:CC"}}
	explicitVideoSID := "TR_video_custom_99"

	tracks := []domain.TrackPublishCommand{
		{
			TrackSID:          explicitVideoSID,
			Kind:              domain.TrackKindVideo,
			SSRC:              7777,
			PayloadType:       96,
			Codec:             "VP8",
			ClockRate:         90000,
			RtpParametersJSON: `{"codecs":[{"mimeType":"video/VP8"}]}`,
		},
	}

	_, err := actor.PublishTracks(ctx, "alice", "client", fingerprints, tracks)
	if err != nil {
		t.Fatalf("PublishTracks failed: %v", err)
	}

	bobSession, _ := actor.GetSession("bob")
	slots := bobSession.SlotTable().ListSlots()
	if len(slots) != 1 {
		t.Fatalf("expected 1 slot on Bob, got %d", len(slots))
	}
	actualConsumerID := slots[0].ConsumerID
	if actualConsumerID == "" {
		t.Fatalf("expected slot to have valid ConsumerID")
	}

	// Bob requests layer update passing TrackSID instead of internal ConsumerID (LiveKit protocol behavior)
	err = actor.SetConsumerLayers(ctx, "bob", explicitVideoSID, 2, 2)
	if err != nil {
		t.Fatalf("SetConsumerLayers failed: %v", err)
	}

	engine.mu.Lock()
	defer engine.mu.Unlock()
	if len(engine.preferredLayers) != 1 {
		t.Fatalf("expected 1 preferredLayers command, got %d", len(engine.preferredLayers))
	}
	cmd := engine.preferredLayers[0]
	if cmd.ConsumerID != actualConsumerID {
		t.Fatalf("expected SetPreferredLayersCommand to resolve TrackSID %s to actual ConsumerID %s, got %s",
			explicitVideoSID, actualConsumerID, cmd.ConsumerID)
	}
	if cmd.SpatialLayer != 2 || cmd.TemporalLayer != 2 {
		t.Fatalf("expected layers 2/2, got spatial=%d temporal=%d", cmd.SpatialLayer, cmd.TemporalLayer)
	}
}

func TestRoomActor_SetConsumerPaused_ResolvesTrackSIDToConsumerID_REQ002(t *testing.T) {
	ctx := context.Background()
	engine := newMockOutboundEngine()
	iceServers := []domain.IceServer{{URLs: []string{"stun:stun.example.com"}}}
	actor := application.NewRoomActor("conf-pause-req002", engine, engine, engine, iceServers)

	respAlice := newMockResponder()
	respBob := newMockResponder()

	_, _ = actor.Join(ctx, "alice", "Alice", respAlice)
	_, _ = actor.Join(ctx, "bob", "Bob", respBob)

	fingerprints := []domain.DtlsFingerprintInfo{{Algorithm: "sha-256", Value: "AA:BB:CC"}}
	explicitVideoSID := "TR_video_custom_88"

	tracks := []domain.TrackPublishCommand{
		{
			TrackSID:          explicitVideoSID,
			Kind:              domain.TrackKindVideo,
			SSRC:              8888,
			PayloadType:       96,
			Codec:             "VP8",
			ClockRate:         90000,
			RtpParametersJSON: `{"codecs":[{"mimeType":"video/VP8"}]}`,
		},
	}

	_, err := actor.PublishTracks(ctx, "alice", "client", fingerprints, tracks)
	if err != nil {
		t.Fatalf("PublishTracks failed: %v", err)
	}

	bobSession, _ := actor.GetSession("bob")
	slots := bobSession.SlotTable().ListSlots()
	if len(slots) != 1 {
		t.Fatalf("expected 1 slot on Bob, got %d", len(slots))
	}
	actualConsumerID := slots[0].ConsumerID
	if actualConsumerID == "" {
		t.Fatalf("expected slot to have valid ConsumerID")
	}

	// 1. Bob pauses the incoming track using TrackSID (disabled: true)
	err = actor.SetConsumerPaused(ctx, "bob", explicitVideoSID, true)
	if err != nil {
		t.Fatalf("SetConsumerPaused(true) failed: %v", err)
	}

	// Verify domain slot paused state
	slots = bobSession.SlotTable().ListSlots()
	if !slots[0].Paused {
		t.Fatalf("expected slot.Paused to be true")
	}

	// Verify outbound gRPC call received ResourceTypeConsumer with actualConsumerID and Paused: true
	engine.mu.Lock()
	if len(engine.pausedMedia) != 1 {
		t.Fatalf("expected 1 pausedMedia command, got %d", len(engine.pausedMedia))
	}
	cmd1 := engine.pausedMedia[0]
	if cmd1.ResourceType != domain.ResourceTypeConsumer {
		t.Fatalf("expected ResourceTypeConsumer, got %v", cmd1.ResourceType)
	}
	if cmd1.ResourceID != actualConsumerID {
		t.Fatalf("expected ResourceID %s, got %s", actualConsumerID, cmd1.ResourceID)
	}
	if !cmd1.Paused {
		t.Fatalf("expected cmd1.Paused == true")
	}
	engine.mu.Unlock()

	// 2. Bob unpauses the incoming track (disabled: false)
	err = actor.SetConsumerPaused(ctx, "bob", explicitVideoSID, false)
	if err != nil {
		t.Fatalf("SetConsumerPaused(false) failed: %v", err)
	}

	// Verify domain slot unpaused
	slots = bobSession.SlotTable().ListSlots()
	if slots[0].Paused {
		t.Fatalf("expected slot.Paused to be false")
	}

	engine.mu.Lock()
	if len(engine.pausedMedia) != 2 {
		t.Fatalf("expected 2 pausedMedia commands, got %d", len(engine.pausedMedia))
	}
	cmd2 := engine.pausedMedia[1]
	if cmd2.ResourceType != domain.ResourceTypeConsumer || cmd2.ResourceID != actualConsumerID || cmd2.Paused {
		t.Fatalf("expected second command to unpause consumer %s, got: %+v", actualConsumerID, cmd2)
	}
	engine.mu.Unlock()
}

func TestRoomManager_CloseGracefullyReleasesAllRooms(t *testing.T) {
	ctx := context.Background()
	engine := newMockOutboundEngine()
	iceServers := []domain.IceServer{{URLs: []string{"stun:stun.example.com"}}}
	mgr := application.NewRoomManager(engine, engine, engine, iceServers)

	_, err := mgr.Join(ctx, "room-close-1", "user-1", "User 1", newMockResponder())
	if err != nil {
		t.Fatalf("Join room 1 failed: %v", err)
	}
	_, err = mgr.Join(ctx, "room-close-2", "user-2", "User 2", newMockResponder())
	if err != nil {
		t.Fatalf("Join room 2 failed: %v", err)
	}

	if mgr.ActiveRoomsCount() != 2 {
		t.Fatalf("expected 2 active rooms, got %d", mgr.ActiveRoomsCount())
	}

	// Graceful close of manager
	if err := mgr.Close(ctx); err != nil {
		t.Fatalf("manager.Close failed: %v", err)
	}

	if mgr.ActiveRoomsCount() != 0 {
		t.Fatalf("expected 0 active rooms after close, got %d", mgr.ActiveRoomsCount())
	}

	engine.mu.Lock()
	closedRouters := len(engine.closedRouters)
	engine.mu.Unlock()
	if closedRouters != 2 {
		t.Fatalf("expected 2 routers closed on SFU node, got %d", closedRouters)
	}
}

func TestRoomActor_ProducerResourceClosedEvent_InactivatesSlotAndRenegotiates(t *testing.T) {
	ctx := context.Background()
	engine := newMockOutboundEngine()
	iceServers := []domain.IceServer{{URLs: []string{"stun:stun.example.com"}}}
	actor := application.NewRoomActor("room-prod-close", engine, engine, engine, iceServers)

	respAlice := newMockResponder()
	_, err := actor.Join(ctx, "alice", "Alice", respAlice)
	if err != nil {
		t.Fatalf("Alice Join failed: %v", err)
	}

	respBob := newMockResponder()
	_, err = actor.Join(ctx, "bob", "Bob", respBob)
	if err != nil {
		t.Fatalf("Bob Join failed: %v", err)
	}

	_, err = actor.PublishTracks(ctx, "alice", "client", nil, []domain.TrackPublishCommand{
		{
			Kind:     domain.TrackKindVideo,
			TrackSID: "TR_alice_cam",
			Codec:    "VP8",
		},
	})
	if err != nil {
		t.Fatalf("PublishTracks failed: %v", err)
	}

	// Verify Bob has an active slot
	sessBob, _ := actor.GetSession("bob")
	activeSlots := sessBob.SlotTable().ActiveSlots()
	if len(activeSlots) != 1 || activeSlots[0].TrackSID != "TR_alice_cam" {
		t.Fatalf("expected Bob to have active slot for TR_alice_cam, got %+v", activeSlots)
	}

	sessAlice, _ := actor.GetSession("alice")
	aliceTracks := sessAlice.Participant().ListTracks()
	if len(aliceTracks) == 0 {
		t.Fatalf("expected Alice to have published tracks")
	}
	actualProducerID := aliceTracks[0].ID

	// Bob answers initial offer to transition to Stable
	if err := actor.HandleSubscriberAnswer(ctx, "bob", "client", nil); err != nil {
		t.Fatalf("Bob HandleSubscriberAnswer failed: %v", err)
	}

	respBob.offers = nil

	// Simulate SFU node sending ResourceClosed event for Alice's producer
	actor.HandleNodeEvent(ctx, &domain.NodeEvent{
		RouterID: actor.RouterID(),
		ResourceClosed: &domain.ResourceClosedEvent{
			ResourceType: domain.ResourceTypeProducer,
			ResourceID:   actualProducerID,
			Reason:       "network timeout",
		},
	})

	// Bob's slot should now be inactivated
	activeAfter := sessBob.SlotTable().ActiveSlots()
	if len(activeAfter) != 0 {
		t.Fatalf("expected Bob to have 0 active slots after producer closed, got %d", len(activeAfter))
	}

	// Bob should have received a renegotiation subscriber offer
	if len(respBob.offers) < 1 {
		t.Fatalf("expected Bob to receive subscriber offer after producer closed, got %d", len(respBob.offers))
	}

	// Alice's published tracks should now be removed from Participant (verifying RemoveTrack by SID)
	if len(sessAlice.Participant().ListTracks()) != 0 {
		t.Fatalf("expected Alice to have 0 published tracks after producer closed, got %d", len(sessAlice.Participant().ListTracks()))
	}
}

func TestRoomActor_LeaveDuringWaitingForAnswer_PendingRenegotiation(t *testing.T) {
	ctx := context.Background()
	engine := newMockOutboundEngine()
	iceServers := []domain.IceServer{{URLs: []string{"stun:stun.example.com"}}}
	actor := application.NewRoomActor("room-pending-reneg", engine, engine, engine, iceServers)

	respAlice := newMockResponder()
	_, _ = actor.Join(ctx, "alice", "Alice", respAlice)
	respBob := newMockResponder()
	_, _ = actor.Join(ctx, "bob", "Bob", respBob)

	// Alice publishes track
	_, _ = actor.PublishTracks(ctx, "alice", "client", nil, []domain.TrackPublishCommand{
		{Kind: domain.TrackKindAudio, TrackSID: "TR_alice_mic"},
	})

	sessBob, _ := actor.GetSession("bob")
	// Bob is now in WaitingForAnswer because SendSubscriberOffer was sent
	if sessBob.State() != domain.NegotiationStateWaitingForAnswer {
		t.Fatalf("expected Bob to be WaitingForAnswer, got %v", sessBob.State())
	}

	// Now Alice leaves while Bob is WaitingForAnswer
	respBob.offers = nil
	_ = actor.Leave(ctx, "alice")

	// Since Bob was WaitingForAnswer, offer was not sent immediately, but pending renegotiation marked
	if len(respBob.offers) != 0 {
		t.Fatalf("expected no new offer while Bob is waiting for answer, got %d", len(respBob.offers))
	}

	// Bob finishes answering the previous offer
	err := actor.HandleSubscriberAnswer(ctx, "bob", "client", nil)
	if err != nil {
		t.Fatalf("HandleSubscriberAnswer failed: %v", err)
	}

	// Now Bob should have received the deferred renegotiation offer!
	if len(respBob.offers) != 1 {
		t.Fatalf("expected Bob to receive deferred renegotiation offer upon answer, got %d", len(respBob.offers))
	}
	if sessBob.State() != domain.NegotiationStateWaitingForAnswer {
		t.Fatalf("expected Bob to enter WaitingForAnswer for the new offer, got %v", sessBob.State())
	}
}

func TestRoomActor_SubscribedQualityUpdate_REQ002(t *testing.T) {
	ctx := context.Background()
	engine := newMockOutboundEngine()
	actor := application.NewRoomActor("room-squ-req002", engine, engine, engine, nil)

	respAlice := newMockResponder()
	_, err := actor.Join(ctx, "alice", "Alice", respAlice)
	if err != nil {
		t.Fatalf("Alice join failed: %v", err)
	}

	// 1. Alice publishes a video track
	videoTrackSID := "TR_alice_cam"
	_, err = actor.PublishTracks(ctx, "alice", "client", nil, []domain.TrackPublishCommand{
		{
			Kind:              domain.TrackKindVideo,
			TrackSID:          videoTrackSID,
			Codec:             "video/VP8",
			ClockRate:         90000,
			RtpParametersJSON: `{"codecs":[{"mimeType":"video/VP8"}]}`,
		},
	})
	if err != nil {
		t.Fatalf("Alice PublishTracks failed: %v", err)
	}

	// Alice should receive SubscribedQualityUpdate immediately upon publishing
	if len(respAlice.qualityUpdates) < 1 {
		t.Fatalf("expected Alice to receive SubscribedQualityUpdate on publish, got %d", len(respAlice.qualityUpdates))
	}
	qu1 := respAlice.qualityUpdates[len(respAlice.qualityUpdates)-1]
	if qu1.TrackSID != videoTrackSID {
		t.Fatalf("expected TrackSID %s, got %s", videoTrackSID, qu1.TrackSID)
	}
	if qu1.Codec != "video/VP8" {
		t.Fatalf("expected Codec video/VP8, got %s", qu1.Codec)
	}

	// 2. Bob joins the room (late joiner) subscribing to Alice's video track
	respBob := newMockResponder()
	initialUpdates := len(respAlice.qualityUpdates)
	_, err = actor.Join(ctx, "bob", "Bob", respBob)
	if err != nil {
		t.Fatalf("Bob join failed: %v", err)
	}

	// Alice must receive SubscribedQualityUpdate when Bob joins and subscribes
	if len(respAlice.qualityUpdates) <= initialUpdates {
		t.Fatalf("expected Alice to receive SubscribedQualityUpdate when Bob joins, got %d (was %d)",
			len(respAlice.qualityUpdates), initialUpdates)
	}

	// 3. Bob handles subscriber answer
	beforeAnswerUpdates := len(respAlice.qualityUpdates)
	err = actor.HandleSubscriberAnswer(ctx, "bob", "client", nil)
	if err != nil {
		t.Fatalf("Bob HandleSubscriberAnswer failed: %v", err)
	}

	// Alice must receive SubscribedQualityUpdate when Bob completes answer
	if len(respAlice.qualityUpdates) <= beforeAnswerUpdates {
		t.Fatalf("expected Alice to receive SubscribedQualityUpdate when Bob answers, got %d (was %d)",
			len(respAlice.qualityUpdates), beforeAnswerUpdates)
	}
}




