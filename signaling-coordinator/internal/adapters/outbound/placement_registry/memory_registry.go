package placement_registry

import (
	"context"
	"sync"
	"time"

	"github.com/google/uuid"

	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

var _ ports.RoomPlacementRegistryPort = (*MemoryPlacementRegistry)(nil)

type memLockEntry struct {
	token     string
	expiresAt time.Time
}

// MemoryPlacementRegistry provides an in-memory thread-safe implementation of RoomPlacementRegistryPort.
// Suitable for single-pod deployments, fallback mode, and unit tests.
type MemoryPlacementRegistry struct {
	mu         sync.RWMutex
	placements map[string]*domain.RoomPlacementTopology

	lockMu sync.Mutex
	locks  map[string]*memLockEntry
}

// NewMemoryPlacementRegistry creates a new in-memory placement registry.
func NewMemoryPlacementRegistry() *MemoryPlacementRegistry {
	return &MemoryPlacementRegistry{
		placements: make(map[string]*domain.RoomPlacementTopology),
		locks:      make(map[string]*memLockEntry),
	}
}

// AcquireRoomCreationLock acquires a mutual exclusion lock for creating a room topology.
func (m *MemoryPlacementRegistry) AcquireRoomCreationLock(ctx context.Context, roomID string, ttl time.Duration) (func(), error) {
	m.lockMu.Lock()
	defer m.lockMu.Unlock()

	now := time.Now()
	entry, exists := m.locks[roomID]
	if exists && entry.expiresAt.After(now) {
		return nil, domain.ErrRoomLockAcquisitionFailed
	}

	token := uuid.NewString()
	m.locks[roomID] = &memLockEntry{
		token:     token,
		expiresAt: now.Add(ttl),
	}

	var once sync.Once
	unlockFunc := func() {
		once.Do(func() {
			m.lockMu.Lock()
			defer m.lockMu.Unlock()
			if curr, ok := m.locks[roomID]; ok && curr.token == token {
				delete(m.locks, roomID)
			}
		})
	}

	return unlockFunc, nil
}

// GetRoomPlacement retrieves the current room placement topology from memory.
func (m *MemoryPlacementRegistry) GetRoomPlacement(ctx context.Context, roomID string) (*domain.RoomPlacementTopology, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	topo, exists := m.placements[roomID]
	if !exists || topo == nil {
		return nil, domain.ErrRoomPlacementNotFound
	}

	return copyTopology(topo), nil
}

// SavePrimaryPlacement saves the primary SFU node and router for a room.
func (m *MemoryPlacementRegistry) SavePrimaryPlacement(ctx context.Context, roomID, primaryNodeID, primaryRouterID string, workerIdx uint32) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	now := time.Now().UTC()
	primaryEntry := domain.RouterPlacementEntry{
		NodeID:          primaryNodeID,
		RouterID:        primaryRouterID,
		WorkerIndex:     workerIdx,
		Role:            domain.RouterRolePrimary,
		ActiveConsumers: 0,
	}

	topo := &domain.RoomPlacementTopology{
		RoomID:           roomID,
		PrimaryNodeID:    primaryNodeID,
		PrimaryRouterID:  primaryRouterID,
		PrimaryWorkerIdx: workerIdx,
		Routers:          []domain.RouterPlacementEntry{primaryEntry},
		CreatedAt:        now,
		UpdatedAt:        now,
	}

	m.placements[roomID] = topo
	return nil
}

// AddCascadedRouter registers a cascaded router for multi-worker or multi-node topologies.
func (m *MemoryPlacementRegistry) AddCascadedRouter(ctx context.Context, roomID, nodeID string, workerIdx uint32, routerID, pipeRole string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	topo, exists := m.placements[roomID]
	if !exists {
		return domain.ErrRoomPlacementNotFound
	}

	for i, r := range topo.Routers {
		if r.NodeID == nodeID && r.RouterID == routerID {
			topo.Routers[i].WorkerIndex = workerIdx
			topo.Routers[i].Role = pipeRole
			topo.UpdatedAt = time.Now().UTC()
			return nil
		}
	}

	topo.Routers = append(topo.Routers, domain.RouterPlacementEntry{
		NodeID:          nodeID,
		RouterID:        routerID,
		WorkerIndex:     workerIdx,
		Role:            pipeRole,
		ActiveConsumers: 0,
	})
	topo.UpdatedAt = time.Now().UTC()
	return nil
}

// UpdateRouterConsumerCount updates the active consumer count on a specific router in the topology.
func (m *MemoryPlacementRegistry) UpdateRouterConsumerCount(ctx context.Context, roomID, nodeID, routerID string, activeConsumers uint32) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	topo, exists := m.placements[roomID]
	if !exists {
		return domain.ErrRoomPlacementNotFound
	}

	found := false
	for i, r := range topo.Routers {
		if r.NodeID == nodeID && r.RouterID == routerID {
			topo.Routers[i].ActiveConsumers = activeConsumers
			found = true
			break
		}
	}

	if !found {
		return domain.ErrRoomPlacementNotFound
	}

	topo.UpdatedAt = time.Now().UTC()
	return nil
}

// RemoveRoomPlacement removes all placement and routing state for a room.
func (m *MemoryPlacementRegistry) RemoveRoomPlacement(ctx context.Context, roomID string) error {
	m.mu.Lock()
	delete(m.placements, roomID)
	m.mu.Unlock()

	m.lockMu.Lock()
	delete(m.locks, roomID)
	m.lockMu.Unlock()

	return nil
}

func copyTopology(t *domain.RoomPlacementTopology) *domain.RoomPlacementTopology {
	if t == nil {
		return nil
	}
	cp := *t
	cp.Routers = make([]domain.RouterPlacementEntry, len(t.Routers))
	copy(cp.Routers, t.Routers)
	return &cp
}
