//go:build !enterprise

package application

import (
	"context"
	"sync"

	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

// CommunityPlacementStrategy implements ports.RoomPlacementStrategy for the Open-Source Community edition.
// It assigns rooms and peers to the primary active media node without multi-node cascading.
type CommunityPlacementStrategy struct {
	mu           sync.RWMutex
	nodeRegistry *NodeRegistry
	roomNodes    map[string]string
}

// NewDefaultPlacementStrategy creates a CommunityPlacementStrategy for non-enterprise builds.
func NewDefaultPlacementStrategy(nr *NodeRegistry) ports.RoomPlacementStrategy {
	return &CommunityPlacementStrategy{
		nodeRegistry: nr,
		roomNodes:    make(map[string]string),
	}
}

// SelectNodeForNewRoom picks the active media node for a new room in single-node Community edition.
func (s *CommunityPlacementStrategy) SelectNodeForNewRoom(ctx context.Context, roomID string) (string, error) {
	if s.nodeRegistry == nil {
		return "", domain.ErrNoNodesAvailable
	}

	nodes := s.nodeRegistry.ListNodes()
	if len(nodes) == 0 {
		return "", domain.ErrNoNodesAvailable
	}

	// In Community edition, select the first available node
	selectedNode := nodes[0].NodeID()

	s.mu.Lock()
	s.roomNodes[roomID] = selectedNode
	s.mu.Unlock()

	return selectedNode, nil
}

// ResolveNodeForPeerJoin resolves the hosting node for peer join.
func (s *CommunityPlacementStrategy) ResolveNodeForPeerJoin(ctx context.Context, roomID string, peerID string) (string, string, error) {
	s.mu.RLock()
	nodeID, ok := s.roomNodes[roomID]
	s.mu.RUnlock()

	if ok {
		return nodeID, "", nil
	}

	// Fallback to SelectNodeForNewRoom if room has no recorded node yet
	nodeID, err := s.SelectNodeForNewRoom(ctx, roomID)
	return nodeID, "", err
}

// RemoveRoom cleans up the room's placement record when the room is closed.
func (s *CommunityPlacementStrategy) RemoveRoom(roomID string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.roomNodes, roomID)
}
