package application_test

import (
	"context"
	"errors"
	"sync"
	"testing"
	"time"

	"ekbatan-room/signaling-coordinator/internal/application"
	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

type mockNodeLifecycleClient struct {
	mu            sync.Mutex
	lastCoordID   string
	lastToken     string
	appliedToken  string
	watchEventsCh chan *domain.NodeEvent
	capacityInfo  *domain.NodeCapacityInfo
}

var _ ports.NodeLifecyclePort = (*mockNodeLifecycleClient)(nil)

func newMockNodeLifecycleClient() *mockNodeLifecycleClient {
	return &mockNodeLifecycleClient{
		watchEventsCh: make(chan *domain.NodeEvent, 10),
		capacityInfo: &domain.NodeCapacityInfo{
			TotalCapacityWeight:  1000,
			EffectiveLoadPercent: 15.0,
			LicenseTier:          "community",
		},
	}
}

func (m *mockNodeLifecycleClient) ApplyLicense(ctx context.Context, licenseToken string) (*domain.ApplyLicenseResult, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.appliedToken = licenseToken
	return &domain.ApplyLicenseResult{
		IsValid:     true,
		LicenseTier: "enterprise",
	}, nil
}

func (m *mockNodeLifecycleClient) GetNodeCapacity(ctx context.Context) (*domain.NodeCapacityInfo, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.capacityInfo, nil
}

func (m *mockNodeLifecycleClient) WatchNodeEvents(ctx context.Context, coordinatorID, licenseToken string) (<-chan *domain.NodeEvent, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.lastCoordID = coordinatorID
	m.lastToken = licenseToken
	return m.watchEventsCh, nil
}

func TestNodeRegistry_CommunityEnforcesSingleNode(t *testing.T) {
	licenseMgr, err := application.NewLicenseManager("")
	if err != nil {
		t.Fatalf("failed to create license manager: %v", err)
	}

	registry := application.NewNodeRegistry("coord-1", licenseMgr)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	client1 := newMockNodeLifecycleClient()
	node1, err := registry.RegisterNode(ctx, "sfu-node-1", client1)
	if err != nil {
		t.Fatalf("expected node 1 registration to succeed, got: %v", err)
	}
	if node1 == nil || node1.NodeID() != "sfu-node-1" {
		t.Fatalf("unexpected node 1 instance")
	}

	// Verify WatchNodeEvents received empty token for community
	if client1.lastToken != "" {
		t.Fatalf("expected empty license token for community, got: %s", client1.lastToken)
	}
	if client1.lastCoordID != "coord-1" {
		t.Fatalf("expected coordinator ID 'coord-1', got: %s", client1.lastCoordID)
	}

	// Registering a second node must fail with ErrMaxNodesExceeded
	client2 := newMockNodeLifecycleClient()
	_, err = registry.RegisterNode(ctx, "sfu-node-2", client2)
	if !errors.Is(err, domain.ErrMaxNodesExceeded) {
		t.Fatalf("expected ErrMaxNodesExceeded for 2nd node in community tier, got: %v", err)
	}

	// Unregister first node then re-register should succeed
	registry.UnregisterNode("sfu-node-1")
	if registry.ActiveNodeCount() != 0 {
		t.Fatalf("expected 0 active nodes after unregister")
	}

	_, err = registry.RegisterNode(ctx, "sfu-node-2", client2)
	if err != nil {
		t.Fatalf("expected node 2 to register after node 1 unregister, got: %v", err)
	}
}

func TestNodeRegistry_EnterprisePassesTokenAndCapacityEvent(t *testing.T) {
	now := time.Now().Unix()
	claims := domain.LicenseClaims{
		LicenseID:     "lic-ent-nodes",
		CustomerID:    "acme",
		Tier:          "enterprise",
		ClusterLimits: domain.ClusterLimits{MaxSfuNodes: 2},
		IssuedAt:      now,
		ExpiresAt:     now + 86400,
	}
	token := createTestToken(claims)

	licenseMgr, err := application.NewLicenseManager(token)
	if err != nil {
		t.Fatalf("failed to create license manager: %v", err)
	}

	registry := application.NewNodeRegistry("coord-enterprise", licenseMgr)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	client1 := newMockNodeLifecycleClient()
	node1, err := registry.RegisterNode(ctx, "sfu-node-1", client1)
	if err != nil {
		t.Fatalf("expected node 1 registration to succeed: %v", err)
	}

	// Token must be propagated to client1
	if client1.lastToken != token {
		t.Fatalf("expected license token %s, got: %s", token, client1.lastToken)
	}

	// Node 2 should also succeed
	client2 := newMockNodeLifecycleClient()
	_, err = registry.RegisterNode(ctx, "sfu-node-2", client2)
	if err != nil {
		t.Fatalf("expected node 2 registration to succeed: %v", err)
	}

	// Node 3 should fail
	client3 := newMockNodeLifecycleClient()
	_, err = registry.RegisterNode(ctx, "sfu-node-3", client3)
	if !errors.Is(err, domain.ErrMaxNodesExceeded) {
		t.Fatalf("expected ErrMaxNodesExceeded on node 3: %v", err)
	}

	// Push capacity update via event stream
	updatedCap := &domain.NodeCapacityInfo{
		TotalCapacityWeight:  2000,
		EffectiveLoadPercent: 72.5,
		LicenseTier:          "enterprise",
	}
	client1.watchEventsCh <- &domain.NodeEvent{
		RouterID:     "r-test",
		TimestampMS:  time.Now().UnixMilli(),
		NodeCapacity: updatedCap,
	}

	// Allow goroutine to process event
	time.Sleep(50 * time.Millisecond)

	curCap := node1.CapacityInfo()
	if curCap == nil || curCap.EffectiveLoadPercent != 72.5 || curCap.LicenseTier != "enterprise" {
		t.Fatalf("expected capacity to be updated via event, got: %+v", curCap)
	}

	// Broadcast updated license
	newClaims := claims
	newClaims.LicenseID = "lic-renewed"
	newToken := createTestToken(newClaims)

	errs := registry.BroadcastLicense(ctx, newToken)
	if len(errs) != 0 {
		t.Fatalf("expected no broadcast errors, got: %v", errs)
	}
	if client1.appliedToken != newToken || client2.appliedToken != newToken {
		t.Fatalf("expected token broadcast to both nodes")
	}
}
