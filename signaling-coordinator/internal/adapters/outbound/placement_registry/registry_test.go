package placement_registry

import (
	"context"
	"sync"
	"testing"
	"time"

	"github.com/alicebob/miniredis/v2"
	"github.com/redis/go-redis/v9"

	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

func testRegistryContract(t *testing.T, reg ports.RoomPlacementRegistryPort) {
	ctx := context.Background()
	roomID := "room-test-123"

	// 1. Initial lookup should return ErrRoomPlacementNotFound
	_, err := reg.GetRoomPlacement(ctx, roomID)
	if err != domain.ErrRoomPlacementNotFound {
		t.Fatalf("expected ErrRoomPlacementNotFound, got %v", err)
	}

	// 2. Acquire lock
	unlock, err := reg.AcquireRoomCreationLock(ctx, roomID, 2*time.Second)
	if err != nil {
		t.Fatalf("failed to acquire lock: %v", err)
	}

	// 3. Second lock acquisition for same room while held must fail
	_, err = reg.AcquireRoomCreationLock(ctx, roomID, 2*time.Second)
	if err != domain.ErrRoomLockAcquisitionFailed {
		t.Fatalf("expected ErrRoomLockAcquisitionFailed, got %v", err)
	}

	// 4. Release lock
	unlock()

	// 5. Save primary placement
	err = reg.SavePrimaryPlacement(ctx, roomID, "node-1", "router-primary-1", 0)
	if err != nil {
		t.Fatalf("failed to save primary placement: %v", err)
	}

	// 6. Get placement and verify
	topo, err := reg.GetRoomPlacement(ctx, roomID)
	if err != nil {
		t.Fatalf("failed to get placement: %v", err)
	}
	if topo.RoomID != roomID || topo.PrimaryNodeID != "node-1" || topo.PrimaryRouterID != "router-primary-1" {
		t.Fatalf("unexpected topology metadata: %+v", topo)
	}
	if len(topo.Routers) != 1 {
		t.Fatalf("expected 1 router in topology, got %d", len(topo.Routers))
	}
	if topo.Routers[0].Role != domain.RouterRolePrimary || topo.Routers[0].RouterID != "router-primary-1" {
		t.Fatalf("unexpected primary router entry: %+v", topo.Routers[0])
	}

	// 7. Add cascaded router
	err = reg.AddCascadedRouter(ctx, roomID, "node-2", 1, "router-sec-2", domain.RouterRoleInterNodePipe)
	if err != nil {
		t.Fatalf("failed to add cascaded router: %v", err)
	}

	topo, err = reg.GetRoomPlacement(ctx, roomID)
	if err != nil {
		t.Fatalf("failed to get placement after cascade: %v", err)
	}
	if len(topo.Routers) != 2 {
		t.Fatalf("expected 2 routers, got %d", len(topo.Routers))
	}

	// 8. Update consumer count
	err = reg.UpdateRouterConsumerCount(ctx, roomID, "node-2", "router-sec-2", 42)
	if err != nil {
		t.Fatalf("failed to update router consumer count: %v", err)
	}

	topo, err = reg.GetRoomPlacement(ctx, roomID)
	if err != nil {
		t.Fatalf("failed to get placement after consumer count update: %v", err)
	}
	var secEntry *domain.RouterPlacementEntry
	for _, r := range topo.Routers {
		if r.RouterID == "router-sec-2" {
			secEntry = &r
			break
		}
	}
	if secEntry == nil || secEntry.ActiveConsumers != 42 {
		t.Fatalf("expected active consumers 42, got %+v", secEntry)
	}

	// 9. Remove room placement
	err = reg.RemoveRoomPlacement(ctx, roomID)
	if err != nil {
		t.Fatalf("failed to remove room placement: %v", err)
	}

	_, err = reg.GetRoomPlacement(ctx, roomID)
	if err != domain.ErrRoomPlacementNotFound {
		t.Fatalf("expected ErrRoomPlacementNotFound after removal, got %v", err)
	}
}

func TestMemoryPlacementRegistry(t *testing.T) {
	reg := NewMemoryPlacementRegistry()
	testRegistryContract(t, reg)
}

func TestRedisPlacementRegistry(t *testing.T) {
	mr, err := miniredis.Run()
	if err != nil {
		t.Fatalf("failed to start miniredis: %v", err)
	}
	defer mr.Close()

	rdb := redis.NewClient(&redis.Options{
		Addr: mr.Addr(),
	})
	defer rdb.Close()

	reg := NewRedisPlacementRegistry(rdb, 10*time.Minute)
	defer reg.Close()

	testRegistryContract(t, reg)
}

func TestRedisPlacementRegistry_PubSubSync(t *testing.T) {
	mr, err := miniredis.Run()
	if err != nil {
		t.Fatalf("failed to start miniredis: %v", err)
	}
	defer mr.Close()

	rdb1 := redis.NewClient(&redis.Options{Addr: mr.Addr()})
	defer rdb1.Close()
	rdb2 := redis.NewClient(&redis.Options{Addr: mr.Addr()})
	defer rdb2.Close()

	reg1 := NewRedisPlacementRegistry(rdb1, 10*time.Minute)
	defer reg1.Close()
	reg2 := NewRedisPlacementRegistry(rdb2, 10*time.Minute)
	defer reg2.Close()

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	reg2.StartPubSubSubscriber(ctx)
	// Allow subscriber to connect
	time.Sleep(50 * time.Millisecond)

	roomID := "room-sync-pubsub"

	// Reg1 saves primary placement
	err = reg1.SavePrimaryPlacement(ctx, roomID, "node-1", "router-primary-1", 0)
	if err != nil {
		t.Fatalf("reg1 failed to save placement: %v", err)
	}

	// Reg2 reads placement (fetches from Redis and caches locally)
	topo2, err := reg2.GetRoomPlacement(ctx, roomID)
	if err != nil {
		t.Fatalf("reg2 failed to get placement: %v", err)
	}
	if topo2.PrimaryRouterID != "router-primary-1" {
		t.Fatalf("unexpected primary router on reg2: %s", topo2.PrimaryRouterID)
	}

	// Reg1 adds cascaded router and broadcasts update via PubSub
	err = reg1.AddCascadedRouter(ctx, roomID, "node-2", 1, "router-sec-2", domain.RouterRoleInterNodePipe)
	if err != nil {
		t.Fatalf("reg1 failed to add cascaded router: %v", err)
	}

	// Give subscriber a few milliseconds to process PubSub event and invalidate local cache
	time.Sleep(50 * time.Millisecond)

	// Reg2 should now see updated topology from Redis due to cache invalidation
	topo2Updated, err := reg2.GetRoomPlacement(ctx, roomID)
	if err != nil {
		t.Fatalf("reg2 failed to get updated placement: %v", err)
	}
	if len(topo2Updated.Routers) != 2 {
		t.Fatalf("expected 2 routers on reg2 after PubSub invalidation, got %d", len(topo2Updated.Routers))
	}
}

func TestMemoryPlacementRegistry_Concurrency(t *testing.T) {
	reg := NewMemoryPlacementRegistry()
	ctx := context.Background()
	roomID := "concurrent-lock-room"

	numGoroutines := 10
	var wg sync.WaitGroup
	wg.Add(numGoroutines)

	lockSuccesses := 0
	var mu sync.Mutex

	for i := 0; i < numGoroutines; i++ {
		go func(idx int) {
			defer wg.Done()
			unlock, err := reg.AcquireRoomCreationLock(ctx, roomID, 1*time.Second)
			if err == nil {
				mu.Lock()
				lockSuccesses++
				mu.Unlock()
				// Hold lock briefly
				time.Sleep(10 * time.Millisecond)
				unlock()
			}
		}(i)
	}

	wg.Wait()

	if lockSuccesses == 0 {
		t.Fatal("expected at least one lock acquisition to succeed")
	}
}
