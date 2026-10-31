//go:build enterprise

package application

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"strconv"
	"sync"

	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

var (
	// SoftThresholdPercent defines the load boundary (70%) after which a node stops receiving new rooms.
	SoftThresholdPercent float32 = 70.0
	// HardThresholdPercent defines the load boundary (85%) after which new transports on this node are blocked,
	// triggering inter-node PipeTransport spillover/cascading.
	HardThresholdPercent float32 = 85.0
)

func init() {
	if val := os.Getenv("SFU_SOFT_THRESHOLD_PERCENT"); val != "" {
		if parsed, err := strconv.ParseFloat(val, 32); err == nil && parsed > 0 && parsed <= 100 {
			SoftThresholdPercent = float32(parsed)
		}
	}
	if val := os.Getenv("SFU_HARD_THRESHOLD_PERCENT"); val != "" {
		if parsed, err := strconv.ParseFloat(val, 32); err == nil && parsed > 0 && parsed <= 100 {
			HardThresholdPercent = float32(parsed)
		}
	}
}

// EnterprisePlacementStrategy implements ports.RoomPlacementStrategy for the commercial Enterprise edition.
// It leverages real-time worker loads, capacity weights, and Soft/Hard thresholds for multi-node cascading.
type EnterprisePlacementStrategy struct {
	mu           sync.RWMutex
	nodeRegistry *NodeRegistry
	roomNodes    map[string]string
}

// NewDefaultPlacementStrategy creates an EnterprisePlacementStrategy for enterprise builds.
func NewDefaultPlacementStrategy(nr *NodeRegistry) ports.RoomPlacementStrategy {
	return &EnterprisePlacementStrategy{
		nodeRegistry: nr,
		roomNodes:    make(map[string]string),
	}
}

// availableWeightedScore calculates the remaining capacity score for a media node.
func (s *EnterprisePlacementStrategy) availableWeightedScore(node *RegisteredNode) float64 {
	capInfo := node.CapacityInfo()
	if capInfo == nil || capInfo.IsDraining {
		return 0.0
	}

	load := float64(capInfo.EffectiveLoadPercent)
	if load >= 100.0 {
		return 0.0
	}

	weight := float64(capInfo.TotalCapacityWeight)
	if weight <= 0 {
		weight = 100.0 // Default baseline weight if not configured
	}

	return weight * (1.0 - (load / 100.0))
}

// SelectNodeForNewRoom selects the optimal SFU node for placing a new room.
// Nodes above SoftThresholdPercent (70%) are excluded from hosting new rooms.
func (s *EnterprisePlacementStrategy) SelectNodeForNewRoom(ctx context.Context, roomID string) (string, error) {
	if s.nodeRegistry == nil {
		return "", domain.ErrNoNodesAvailable
	}

	nodes := s.nodeRegistry.ListNodes()
	if len(nodes) == 0 {
		return "", domain.ErrNoNodesAvailable
	}

	var bestNode *RegisteredNode
	var maxScore float64 = -1.0

	// 1. First pass: look for nodes in Normal Zone (load < SoftThreshold 70%)
	for _, n := range nodes {
		capInfo := n.CapacityInfo()
		if capInfo == nil || capInfo.IsDraining {
			continue
		}
		if capInfo.EffectiveLoadPercent < SoftThresholdPercent {
			score := s.availableWeightedScore(n)
			if score > maxScore {
				maxScore = score
				bestNode = n
			}
		}
	}

	// 2. Second pass: fallback to nodes between Soft and Hard threshold if all normal nodes are saturated
	if bestNode == nil {
		slog.Warn("all SFU nodes exceeded soft threshold (70%), selecting least loaded node under hard threshold (85%)",
			"room_id", roomID,
		)
		for _, n := range nodes {
			capInfo := n.CapacityInfo()
			if capInfo == nil || capInfo.IsDraining {
				continue
			}
			if capInfo.EffectiveLoadPercent < HardThresholdPercent {
				score := s.availableWeightedScore(n)
				if score > maxScore {
					maxScore = score
					bestNode = n
				}
			}
		}
	}

	if bestNode == nil {
		return "", fmt.Errorf("%w: all SFU nodes have reached or exceeded hard threshold (85%%)", domain.ErrNoNodesAvailable)
	}

	selectedNodeID := bestNode.NodeID()
	s.mu.Lock()
	s.roomNodes[roomID] = selectedNodeID
	s.mu.Unlock()

	slog.Info("enterprise scheduler assigned room to node",
		"room_id", roomID,
		"node_id", selectedNodeID,
		"score", maxScore,
	)

	return selectedNodeID, nil
}

// ResolveNodeForPeerJoin resolves which SFU node should host the joining peer.
// If the primary hosting node exceeds HardThresholdPercent (85%), the peer is spilled over
// to another available node in the cluster.
func (s *EnterprisePlacementStrategy) ResolveNodeForPeerJoin(ctx context.Context, roomID string, peerID string) (string, string, error) {
	s.mu.RLock()
	primaryNodeID, ok := s.roomNodes[roomID]
	s.mu.RUnlock()

	if !ok {
		// Room not yet placed, select initial node
		nodeID, err := s.SelectNodeForNewRoom(ctx, roomID)
		return nodeID, "", err
	}

	// Check primary node health and load
	primaryNode, err := s.nodeRegistry.GetNode(primaryNodeID)
	if err == nil {
		capInfo := primaryNode.CapacityInfo()
		// If primary node is healthy and load < HardThreshold (85%), keep peer on primary node
		if capInfo != nil && !capInfo.IsDraining && capInfo.EffectiveLoadPercent < HardThresholdPercent {
			return primaryNodeID, "", nil
		}
		var loadVal float32 = 0
		if capInfo != nil {
			loadVal = capInfo.EffectiveLoadPercent
		}
		slog.Warn("primary node for room reached hard threshold or is draining, evaluating spillover node",
			"room_id", roomID,
			"peer_id", peerID,
			"primary_node", primaryNodeID,
			"effective_load", loadVal,
		)
	}

	// Hard Threshold exceeded or primary node draining: Spillover to another node
	nodes := s.nodeRegistry.ListNodes()
	var spilloverNode *RegisteredNode
	var maxScore float64 = -1.0

	for _, n := range nodes {
		if n.NodeID() == primaryNodeID {
			continue // Do not spillover to the saturated primary node
		}
		capInfo := n.CapacityInfo()
		if capInfo == nil || capInfo.IsDraining {
			continue
		}
		if capInfo.EffectiveLoadPercent < HardThresholdPercent {
			score := s.availableWeightedScore(n)
			if score > maxScore {
				maxScore = score
				spilloverNode = n
			}
		}
	}

	if spilloverNode != nil {
		slog.Info("spilling over peer to secondary node via PipeTransport cascading",
			"room_id", roomID,
			"peer_id", peerID,
			"primary_node", primaryNodeID,
			"spillover_node", spilloverNode.NodeID(),
		)
		return spilloverNode.NodeID(), "", nil
	}

	// If no alternative node is under hard threshold, keep on primary node if alive, else fail
	if primaryNode != nil {
		return primaryNodeID, "", nil
	}

	return "", "", domain.ErrNoNodesAvailable
}

// RemoveRoom cleans up the room's placement record when the room is closed.
func (s *EnterprisePlacementStrategy) RemoveRoom(roomID string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.roomNodes, roomID)
}
