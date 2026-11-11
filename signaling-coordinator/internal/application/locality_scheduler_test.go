package application_test

import (
	"context"
	"fmt"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/alicebob/miniredis/v2"
	"github.com/redis/go-redis/v9"

	"ekbatan-room/signaling-coordinator/internal/adapters/outbound/placement_registry"
	"ekbatan-room/signaling-coordinator/internal/application"
	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

type mockRouterAndLifecycleClient struct {
	createRouterCount atomic.Int32
	closeRouterCount  atomic.Int32
}

var (
	_ ports.RouterEnginePort  = (*mockRouterAndLifecycleClient)(nil)
	_ ports.NodeLifecyclePort = (*mockRouterAndLifecycleClient)(nil)
)

func (m *mockRouterAndLifecycleClient) CreateRouter(ctx context.Context, roomID string) (*domain.RouterInfo, error) {
	m.createRouterCount.Add(1)
	// Simulate minor media engine initialization latency
	time.Sleep(10 * time.Millisecond)
	return &domain.RouterInfo{
		RouterID:            "router-" + roomID,
		WorkerIndex:         0,
		RtpCapabilitiesJSON: `{"codecs":[]}`,
	}, nil
}

func (m *mockRouterAndLifecycleClient) CloseRouter(ctx context.Context, routerID string) error {
	m.closeRouterCount.Add(1)
	return nil
}

func (m *mockRouterAndLifecycleClient) GetNodeCapacity(ctx context.Context) (*domain.NodeCapacityInfo, error) {
	return &domain.NodeCapacityInfo{
		TotalCapacityWeight:  1000,
		EffectiveLoadPercent: 20.0,
		LicenseTier:          "community",
	}, nil
}

func (m *mockRouterAndLifecycleClient) ApplyLicense(ctx context.Context, licenseToken string) (*domain.ApplyLicenseResult, error) {
	return &domain.ApplyLicenseResult{IsValid: true, LicenseTier: "enterprise"}, nil
}

func (m *mockRouterAndLifecycleClient) WatchNodeEvents(ctx context.Context, coordinatorID, licenseToken string) (<-chan *domain.NodeEvent, error) {
	return make(chan *domain.NodeEvent), nil
}

func setupTestScheduler(t *testing.T, reg ports.RoomPlacementRegistryPort) (*application.LocalityScheduler, *mockRouterAndLifecycleClient) {
	mockClient := &mockRouterAndLifecycleClient{}
	nodeRegistry := application.NewNodeRegistry("coord-test", nil)

	_, err := nodeRegistry.RegisterNode(context.Background(), "sfu-node-1", mockClient)
	if err != nil {
		t.Fatalf("failed to register test node: %v", err)
	}

	scheduler := application.NewLocalityScheduler(reg, nodeRegistry)
	return scheduler, mockClient
}

func testConcurrentAllocation(t *testing.T, reg ports.RoomPlacementRegistryPort) {
	scheduler, mockClient := setupTestScheduler(t, reg)
	ctx := context.Background()
	roomID := "room-concurrent-test"

	const concurrentCallers = 10
	var wg sync.WaitGroup
	wg.Add(concurrentCallers)

	results := make([]*domain.RoomPlacementTopology, concurrentCallers)
	errorsList := make([]error, concurrentCallers)

	startGate := make(chan struct{})

	for i := 0; i < concurrentCallers; i++ {
		go func(idx int) {
			defer wg.Done()
			<-startGate // Align all goroutines to fire at the exact same instant

			pID := fmt.Sprintf("participant-%d", idx)
			topo, err := scheduler.AllocateNodeForParticipant(ctx, roomID, pID)
			results[idx] = topo
			errorsList[idx] = err
		}(i)
	}

	// Release all 10 callers concurrently
	close(startGate)
	wg.Wait()

	// 1. Assert: All 10 callers succeeded without error
	for i, err := range errorsList {
		if err != nil {
			t.Fatalf("caller %d failed with error: %v", i, err)
		}
	}

	// 2. Assert: Exactly ONE CreateRouter call was dispatched to the SFU node (Zero Split-Brain)
	calls := mockClient.createRouterCount.Load()
	if calls != 1 {
		t.Fatalf("expected exactly 1 CreateRouter invocation on SFU node, but got %d", calls)
	}

	// 3. Assert: All 10 callers converged on the exact same placement topology
	expectedRouterID := "router-" + roomID
	expectedNodeID := "sfu-node-1"

	for i, topo := range results {
		if topo == nil {
			t.Fatalf("caller %d got nil topology", i)
		}
		if topo.RoomID != roomID {
			t.Fatalf("caller %d got roomID=%s, expected %s", i, topo.RoomID, roomID)
		}
		if topo.PrimaryNodeID != expectedNodeID {
			t.Fatalf("caller %d got PrimaryNodeID=%s, expected %s", i, topo.PrimaryNodeID, expectedNodeID)
		}
		if topo.PrimaryRouterID != expectedRouterID {
			t.Fatalf("caller %d got PrimaryRouterID=%s, expected %s", i, topo.PrimaryRouterID, expectedRouterID)
		}
		if len(topo.Routers) != 1 {
			t.Fatalf("caller %d expected 1 router entry, got %d", i, len(topo.Routers))
		}
		if topo.Routers[0].RouterID != expectedRouterID {
			t.Fatalf("caller %d router entry mismatch: %+v", i, topo.Routers[0])
		}
	}
}

// TestLocalityScheduler_ConcurrentAllocation_MemoryRegistry proves that 10 concurrent
// AllocateNodeForParticipant calls for a new room_id result in exactly ONE primary CreateRouter
// call on the SFU node and all 10 participants converge on the same placement topology.
func TestLocalityScheduler_ConcurrentAllocation_MemoryRegistry(t *testing.T) {
	reg := placement_registry.NewMemoryPlacementRegistry()
	testConcurrentAllocation(t, reg)
}

// TestLocalityScheduler_ConcurrentAllocation_RedisRegistry proves the exact same Zero Split-Brain
// invariant over Redis with SET NX PX distributed locking.
func TestLocalityScheduler_ConcurrentAllocation_RedisRegistry(t *testing.T) {
	mr, err := miniredis.Run()
	if err != nil {
		t.Fatalf("failed to start miniredis: %v", err)
	}
	defer mr.Close()

	rdb := redis.NewClient(&redis.Options{
		Addr: mr.Addr(),
	})
	defer rdb.Close()

	reg := placement_registry.NewRedisPlacementRegistry(rdb, 10*time.Minute)
	defer reg.Close()

	testConcurrentAllocation(t, reg)
}

func TestLocalityScheduler_RemoveRoom(t *testing.T) {
	reg := placement_registry.NewMemoryPlacementRegistry()
	scheduler, _ := setupTestScheduler(t, reg)
	ctx := context.Background()
	roomID := "room-to-remove"

	topo, err := scheduler.AllocateNodeForParticipant(ctx, roomID, "p-1")
	if err != nil || topo == nil {
		t.Fatalf("allocation failed: %v", err)
	}

	err = scheduler.RemoveRoom(ctx, roomID)
	if err != nil {
		t.Fatalf("remove room failed: %v", err)
	}

	_, err = reg.GetRoomPlacement(ctx, roomID)
	if err != domain.ErrRoomPlacementNotFound {
		t.Fatalf("expected ErrRoomPlacementNotFound after removal, got %v", err)
	}
}
