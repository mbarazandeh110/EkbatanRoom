package application_test

import (
	"context"
	"fmt"
	"sync"
	"testing"
	"time"

	"ekbatan-room/signaling-coordinator/internal/application"
	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

type mockCascadingNodeEngine struct {
	*mockOutboundEngine
	*mockNodeLifecycleClient
	mu sync.Mutex

	createdPipeTransports []*domain.CreatePipeTransportCommand
	connectedPipes        []*domain.ConnectPipeTransportCommand
	pipeProduces          []*domain.PipeProduceCommand
	pipeConsumes          []*domain.PipeConsumeCommand
	nodeID                string
}

func newMockCascadingNodeEngine(nodeID string) *mockCascadingNodeEngine {
	return &mockCascadingNodeEngine{
		mockOutboundEngine:      newMockOutboundEngine(),
		mockNodeLifecycleClient: newMockNodeLifecycleClient(),
		createdPipeTransports:   make([]*domain.CreatePipeTransportCommand, 0),
		connectedPipes:          make([]*domain.ConnectPipeTransportCommand, 0),
		pipeProduces:            make([]*domain.PipeProduceCommand, 0),
		pipeConsumes:            make([]*domain.PipeConsumeCommand, 0),
		nodeID:                  nodeID,
	}
}

func (m *mockCascadingNodeEngine) CreatePipeTransport(ctx context.Context, cmd *domain.CreatePipeTransportCommand) (*domain.PipeTransportInfo, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.createdPipeTransports = append(m.createdPipeTransports, cmd)
	return &domain.PipeTransportInfo{
		PipeTransportID:    fmt.Sprintf("pipe-%s-%s", m.nodeID, cmd.RouterID),
		IP:                 "10.0.0.1",
		Port:               40000,
		SrtpParametersJSON: `{"cryptoSuite":"AEAD_AES_256_GCM","keyBase64":"dGVzdA=="}`,
	}, nil
}

func (m *mockCascadingNodeEngine) ConnectPipeTransport(ctx context.Context, cmd *domain.ConnectPipeTransportCommand) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.connectedPipes = append(m.connectedPipes, cmd)
	return nil
}

func (m *mockCascadingNodeEngine) PipeProduce(ctx context.Context, cmd *domain.PipeProduceCommand) (*domain.ProduceResult, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.pipeProduces = append(m.pipeProduces, cmd)
	return &domain.ProduceResult{
		ProducerID: fmt.Sprintf("pipe-prod-%s", cmd.SourceProducerID),
		Kind:       cmd.Kind,
	}, nil
}

func (m *mockCascadingNodeEngine) PipeConsume(ctx context.Context, cmd *domain.PipeConsumeCommand) (*domain.ConsumeResult, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.pipeConsumes = append(m.pipeConsumes, cmd)
	return &domain.ConsumeResult{
		ConsumerID: fmt.Sprintf("pipe-cons-%s", cmd.ProducerID),
		ProducerID: cmd.ProducerID,
	}, nil
}

var (
	_ ports.PipeTransportPort = (*mockCascadingNodeEngine)(nil)
	_ ports.NodeLifecyclePort = (*mockCascadingNodeEngine)(nil)
	_ ports.RouterEnginePort  = (*mockCascadingNodeEngine)(nil)
	_ ports.MediaStreamPort   = (*mockCascadingNodeEngine)(nil)
)

func TestRoomActor_BiDirectionalMultiNodeCascading(t *testing.T) {
	ctx := context.Background()

	// 1. Setup Enterprise License with FeatureCascading
	now := time.Now().Unix()
	claims := domain.LicenseClaims{
		LicenseID:  "lic-cascading-test",
		CustomerID: "enterprise-customer",
		Tier:       "enterprise",
		Features:   []string{domain.FeatureCascading},
		ClusterLimits: domain.ClusterLimits{
			MaxConcurrentRooms: 10,
			MaxConcurrentPeers: 100,
			MaxSfuNodes:        10,
		},
		IssuedAt:  now,
		ExpiresAt: now + 86400,
	}
	token := createTestToken(claims)
	licenseMgr, err := application.NewLicenseManager(token)
	if err != nil {
		t.Fatalf("failed to create license manager: %v", err)
	}

	// 2. Setup NodeRegistry with Node-1 (Primary) and Node-2 (Secondary)
	nodeReg := application.NewNodeRegistry("coord-1", licenseMgr)
	node1Engine := newMockCascadingNodeEngine("node-1")
	node2Engine := newMockCascadingNodeEngine("node-2")

	_, err = nodeReg.RegisterNode(ctx, "node-1", node1Engine)
	if err != nil {
		t.Fatalf("failed to register node-1: %v", err)
	}
	_, err = nodeReg.RegisterNode(ctx, "node-2", node2Engine)
	if err != nil {
		t.Fatalf("failed to register node-2: %v", err)
	}

	// 3. Create RoomActor with Primary Node on node-1
	iceServers := []domain.IceServer{{URLs: []string{"stun:stun.example.com"}}}
	actor := application.NewRoomActor("room-cascaded", node1Engine, node1Engine, node1Engine, iceServers)
	actor.SetNodeRegistry(nodeReg)
	actor.SetPrimaryNodeID("node-1")
	actor.SetLicenseVerifier(licenseMgr)

	respAlice := newMockResponder()
	respBob := newMockResponder()

	// 4. Alice joins on Primary Node (node-1)
	joinResAlice, err := actor.Join(ctx, "alice", "Alice", respAlice, "node-1")
	if err != nil {
		t.Fatalf("Alice join failed: %v", err)
	}
	if joinResAlice == nil {
		t.Fatal("expected non-nil join response for Alice")
	}

	// 5. Bob joins on Secondary Node (node-2) -> triggers PipeTransport creation and connection
	joinResBob, err := actor.Join(ctx, "bob", "Bob", respBob, "node-2")
	if err != nil {
		t.Fatalf("Bob join on secondary node failed: %v", err)
	}
	if joinResBob == nil {
		t.Fatal("expected non-nil join response for Bob")
	}

	// Verify PipeTransports were created and connected on both nodes
	node1Engine.mu.Lock()
	node1Pipes := len(node1Engine.createdPipeTransports)
	node1Conns := len(node1Engine.connectedPipes)
	node1Engine.mu.Unlock()

	node2Engine.mu.Lock()
	node2Pipes := len(node2Engine.createdPipeTransports)
	node2Conns := len(node2Engine.connectedPipes)
	node2Engine.mu.Unlock()

	if node1Pipes != 1 || node1Conns != 1 {
		t.Fatalf("expected 1 pipe transport created and connected on node-1, got pipes=%d conns=%d", node1Pipes, node1Conns)
	}
	if node2Pipes != 1 || node2Conns != 1 {
		t.Fatalf("expected 1 pipe transport created and connected on node-2, got pipes=%d conns=%d", node2Pipes, node2Conns)
	}

	fingerprints := []domain.DtlsFingerprintInfo{
		{Algorithm: "sha-256", Value: "AA:BB:CC:DD"},
	}

	// 6. Alice publishes Audio Track on Node-1 (Primary)
	aliceTracks := []domain.TrackPublishCommand{
		{
			Kind:              domain.TrackKindAudio,
			SSRC:              11111,
			PayloadType:       111,
			Codec:             "opus",
			ClockRate:         48000,
			RtpParametersJSON: `{"codecs":[{"mimeType":"audio/opus"}]}`,
		},
	}
	_, err = actor.PublishTracks(ctx, "alice", "client", fingerprints, aliceTracks)
	if err != nil {
		t.Fatalf("Alice PublishTracks failed: %v", err)
	}

	// Verify track was piped from Node-1 to Node-2 via PipeProduce on Node-1
	node1Engine.mu.Lock()
	node1PipeProduces := len(node1Engine.pipeProduces)
	node1Engine.mu.Unlock()
	if node1PipeProduces != 1 {
		t.Fatalf("expected 1 pipe produce on node-1, got %d", node1PipeProduces)
	}

	// Verify Bob received Subscriber SDP Offer for Alice's track
	if len(respBob.offers) != 1 {
		t.Fatalf("expected Bob to receive 1 subscriber offer, got %d", len(respBob.offers))
	}

	// 7. Bob publishes Video Track on Node-2 (Secondary)
	bobTracks := []domain.TrackPublishCommand{
		{
			Kind:              domain.TrackKindVideo,
			SSRC:              22222,
			PayloadType:       96,
			Codec:             "VP8",
			ClockRate:         90000,
			RtpParametersJSON: `{"codecs":[{"mimeType":"video/VP8"}]}`,
		},
	}
	_, err = actor.PublishTracks(ctx, "bob", "client", fingerprints, bobTracks)
	if err != nil {
		t.Fatalf("Bob PublishTracks failed: %v", err)
	}

	// Verify track was piped from Node-2 to Node-1 via PipeProduce on Node-2
	node2Engine.mu.Lock()
	node2PipeProduces := len(node2Engine.pipeProduces)
	node2Engine.mu.Unlock()
	if node2PipeProduces != 1 {
		t.Fatalf("expected 1 pipe produce on node-2, got %d", node2PipeProduces)
	}

	// Verify Alice received Subscriber SDP Offer for Bob's track
	if len(respAlice.offers) != 1 {
		t.Fatalf("expected Alice to receive 1 subscriber offer, got %d", len(respAlice.offers))
	}

	// 8. Close Room and verify cleanup
	err = actor.Close(ctx)
	if err != nil {
		t.Fatalf("expected room close to succeed, got %v", err)
	}
}

func TestRoomActor_CascadingBlockedWhenNotLicensed(t *testing.T) {
	ctx := context.Background()

	// License without FeatureCascading
	now := time.Now().Unix()
	claims := domain.LicenseClaims{
		LicenseID:  "lic-no-cascading",
		CustomerID: "community-or-basic",
		Tier:       "community",
		Features:   []string{}, // no FeatureCascading
		ClusterLimits: domain.ClusterLimits{
			MaxConcurrentRooms: 10,
			MaxConcurrentPeers: 100,
			MaxSfuNodes:        2,
		},
		IssuedAt:  now,
		ExpiresAt: now + 86400,
	}
	token := createTestToken(claims)
	licenseMgr, err := application.NewLicenseManager(token)
	if err != nil {
		t.Fatalf("failed to create license manager: %v", err)
	}

	nodeReg := application.NewNodeRegistry("coord-1", licenseMgr)
	node1Engine := newMockCascadingNodeEngine("node-1")
	node2Engine := newMockCascadingNodeEngine("node-2")

	_, _ = nodeReg.RegisterNode(ctx, "node-1", node1Engine)

	iceServers := []domain.IceServer{{URLs: []string{"stun:stun.example.com"}}}
	actor := application.NewRoomActor("room-no-cascade", node1Engine, node1Engine, node1Engine, iceServers)
	actor.SetNodeRegistry(nodeReg)
	actor.SetPrimaryNodeID("node-1")
	actor.SetLicenseVerifier(licenseMgr)

	resp1 := newMockResponder()
	// Attempt join assigning to node-2 without cascading license
	// It should gracefully fallback to primary node (node-1) and succeed on node-1
	joinRes, err := actor.Join(ctx, "user-fallback", "User Fallback", resp1, "node-2")
	if err != nil {
		t.Fatalf("expected join with fallback to succeed, got: %v", err)
	}
	if joinRes == nil {
		t.Fatal("expected non-nil join response")
	}

	// Verify no pipe transports were created on node-1 or node-2
	node1Engine.mu.Lock()
	node1Pipes := len(node1Engine.createdPipeTransports)
	node1Engine.mu.Unlock()
	node2Engine.mu.Lock()
	node2Pipes := len(node2Engine.createdPipeTransports)
	node2Engine.mu.Unlock()

	if node1Pipes != 0 || node2Pipes != 0 {
		t.Fatalf("expected 0 pipe transports created when cascading not licensed, got node1=%d node2=%d", node1Pipes, node2Pipes)
	}
}
