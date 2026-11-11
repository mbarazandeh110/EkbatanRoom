package application

import (
	"context"
	"fmt"
	"sync"
	"time"

	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

// LocalityScheduler coordinates room placement, distributed locking, and router lifecycle
// across coordinator pods to eliminate split-brain scenarios and optimize SFU node locality.
type LocalityScheduler struct {
	mu                sync.RWMutex
	registry          ports.RoomPlacementRegistryPort
	nodeRegistry      *NodeRegistry
	placementStrategy ports.RoomPlacementStrategy
}

// NewLocalityScheduler constructs a new LocalityScheduler.
func NewLocalityScheduler(
	registry ports.RoomPlacementRegistryPort,
	nodeRegistry *NodeRegistry,
	placementStrategy ...ports.RoomPlacementStrategy,
) *LocalityScheduler {
	var ps ports.RoomPlacementStrategy
	if len(placementStrategy) > 0 && placementStrategy[0] != nil {
		ps = placementStrategy[0]
	} else if nodeRegistry != nil {
		ps = NewDefaultPlacementStrategy(nodeRegistry)
	}

	return &LocalityScheduler{
		registry:          registry,
		nodeRegistry:      nodeRegistry,
		placementStrategy: ps,
	}
}

// Registry returns the underlying RoomPlacementRegistryPort.
func (s *LocalityScheduler) Registry() ports.RoomPlacementRegistryPort {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.registry
}

// SetPlacementStrategy overrides the placement strategy.
func (s *LocalityScheduler) SetPlacementStrategy(ps ports.RoomPlacementStrategy) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.placementStrategy = ps
}

// AllocateNodeForParticipant safely determines and allocates the hosting SFU node and primary router
// for a participant joining a room. It utilizes a distributed atomic lock to guarantee Zero Split-Brain
// (strictly ONE primary router creation across concurrent joiners or coordinator pods).
func (s *LocalityScheduler) AllocateNodeForParticipant(
	ctx context.Context,
	roomID string,
	participantID string,
) (*domain.RoomPlacementTopology, error) {
	if s.registry == nil {
		return nil, domain.ErrRoomPlacementNotFound
	}

	// 1. Fast path: check if room placement topology already exists
	if topo, err := s.registry.GetRoomPlacement(ctx, roomID); err == nil && topo != nil && topo.PrimaryRouterID != "" {
		return topo, nil
	}

	// 2. Acquisition loop with backoff to prevent duplicate creation
	var unlock func()
	var err error
	for attempt := 0; attempt < 50; attempt++ {
		unlock, err = s.registry.AcquireRoomCreationLock(ctx, roomID, 5*time.Second)
		if err == nil {
			break
		}

		// Wait briefly while winner initializes room
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-time.After(15 * time.Millisecond):
		}

		// Check if another pod has finalized placement
		if topo, err := s.registry.GetRoomPlacement(ctx, roomID); err == nil && topo != nil && topo.PrimaryRouterID != "" {
			return topo, nil
		}
	}

	if err != nil {
		return nil, fmt.Errorf("failed to acquire room creation lock: %w", err)
	}
	defer unlock()

	// 3. Double-check under lock
	if topo, err := s.registry.GetRoomPlacement(ctx, roomID); err == nil && topo != nil && topo.PrimaryRouterID != "" {
		return topo, nil
	}

	// 4. Select optimal SFU media node for the new room
	var selectedNodeID string
	if s.placementStrategy != nil {
		selectedNodeID, err = s.placementStrategy.SelectNodeForNewRoom(ctx, roomID)
		if err != nil {
			return nil, err
		}
	} else if s.nodeRegistry != nil {
		nodes := s.nodeRegistry.ListNodes()
		if len(nodes) == 0 {
			return nil, domain.ErrNoNodesAvailable
		}
		selectedNodeID = nodes[0].NodeID()
	} else {
		return nil, domain.ErrNoNodesAvailable
	}

	// 5. Retrieve node client and create router on the SFU node (EXACTLY ONCE)
	if s.nodeRegistry == nil {
		return nil, domain.ErrNoNodesAvailable
	}
	node, err := s.nodeRegistry.GetNode(selectedNodeID)
	if err != nil || node == nil {
		return nil, domain.ErrNoNodesAvailable
	}

	routerPort, ok := node.Client().(ports.RouterEnginePort)
	if !ok || routerPort == nil {
		return nil, fmt.Errorf("node %s does not implement RouterEnginePort", selectedNodeID)
	}

	routerInfo, err := routerPort.CreateRouter(ctx, roomID)
	if err != nil {
		return nil, fmt.Errorf("failed to create media router on node %s: %w", selectedNodeID, err)
	}

	// 6. Persist placement in distributed registry
	if err := s.registry.SavePrimaryPlacement(ctx, roomID, selectedNodeID, routerInfo.RouterID, routerInfo.WorkerIndex); err != nil {
		return nil, fmt.Errorf("failed to save primary placement: %w", err)
	}

	// 7. Return converged topology
	return s.registry.GetRoomPlacement(ctx, roomID)
}

// RemoveRoom cleans up all placement and router entries for a closed room.
func (s *LocalityScheduler) RemoveRoom(ctx context.Context, roomID string) error {
	if s.registry == nil {
		return nil
	}
	return s.registry.RemoveRoomPlacement(ctx, roomID)
}
