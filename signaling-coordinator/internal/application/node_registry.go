package application

import (
	"context"
	"fmt"
	"log/slog"
	"sync"

	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

// RegisteredNode represents a connected media node in the cluster registry.
type RegisteredNode struct {
	mu           sync.RWMutex
	nodeID       string
	client       ports.NodeLifecyclePort
	capacityInfo *domain.NodeCapacityInfo
	cancelFunc   context.CancelFunc
}

// NodeID returns the identifier of the node.
func (n *RegisteredNode) NodeID() string {
	return n.nodeID
}

// Client returns the outbound lifecycle port for the node.
func (n *RegisteredNode) Client() ports.NodeLifecyclePort {
	return n.client
}

// CapacityInfo returns the latest reported capacity and load metrics.
func (n *RegisteredNode) CapacityInfo() *domain.NodeCapacityInfo {
	n.mu.RLock()
	defer n.mu.RUnlock()
	return n.capacityInfo
}

// updateCapacity safely updates the latest capacity info.
func (n *RegisteredNode) updateCapacity(info *domain.NodeCapacityInfo) {
	n.mu.Lock()
	defer n.mu.Unlock()
	n.capacityInfo = info
}

// NodeRegistry manages connected SFU media nodes and enforces cluster node limits.
type NodeRegistry struct {
	mu            sync.RWMutex
	coordinatorID string
	licenseMgr    *LicenseManager
	nodes         map[string]*RegisteredNode
}

// NewNodeRegistry initializes a new NodeRegistry.
func NewNodeRegistry(coordinatorID string, licenseMgr *LicenseManager) *NodeRegistry {
	if coordinatorID == "" {
		coordinatorID = "coordinator-default"
	}
	return &NodeRegistry{
		coordinatorID: coordinatorID,
		licenseMgr:    licenseMgr,
		nodes:         make(map[string]*RegisteredNode),
	}
}

// RegisterNode verifies cluster capacity limits, establishes event streaming with license token,
// and registers the node in the active cluster pool.
func (r *NodeRegistry) RegisterNode(ctx context.Context, nodeID string, client ports.NodeLifecyclePort) (*RegisteredNode, error) {
	r.mu.Lock()
	defer r.mu.Unlock()

	// 1. Check if node is already registered
	if existing, ok := r.nodes[nodeID]; ok {
		return existing, nil
	}

	// 2. Enforce cluster max_sfu_nodes quota via license manager
	if r.licenseMgr != nil {
		if err := r.licenseMgr.CheckCanAddNode(len(r.nodes)); err != nil {
			slog.Warn("cannot add media node to cluster: node limit exceeded",
				"node_id", nodeID,
				"current_nodes", len(r.nodes),
				"error", err,
			)
			return nil, err
		}
	}

	// 3. Initiate WatchNodeEvents with coordinator ID and license token
	licenseToken := ""
	if r.licenseMgr != nil {
		licenseToken = r.licenseMgr.GetToken()
	}

	nodeCtx, cancel := context.WithCancel(ctx)
	eventsChan, err := client.WatchNodeEvents(nodeCtx, r.coordinatorID, licenseToken)
	if err != nil {
		cancel()
		return nil, fmt.Errorf("failed to initiate watch node events for node %s: %w", nodeID, err)
	}

	node := &RegisteredNode{
		nodeID:     nodeID,
		client:     client,
		cancelFunc: cancel,
	}

	// Try fetching initial capacity
	if initCap, err := client.GetNodeCapacity(nodeCtx); err == nil && initCap != nil {
		node.capacityInfo = initCap
	}

	r.nodes[nodeID] = node

	slog.Info("media node registered successfully",
		"node_id", nodeID,
		"coordinator_id", r.coordinatorID,
		"has_license_token", licenseToken != "",
		"cluster_total_nodes", len(r.nodes),
	)

	// 4. Start background listener for node events
	go r.listenNodeEvents(nodeCtx, nodeID, node, eventsChan)

	return node, nil
}

func (r *NodeRegistry) listenNodeEvents(ctx context.Context, nodeID string, node *RegisteredNode, eventsChan <-chan *domain.NodeEvent) {
	for {
		select {
		case <-ctx.Done():
			return
		case evt, ok := <-eventsChan:
			if !ok {
				slog.Warn("media node event stream terminated", "node_id", nodeID)
				return
			}
			if evt != nil && evt.NodeCapacity != nil {
				node.updateCapacity(evt.NodeCapacity)
				slog.Debug("media node capacity updated from event",
					"node_id", nodeID,
					"effective_load", evt.NodeCapacity.EffectiveLoadPercent,
					"license_tier", evt.NodeCapacity.LicenseTier,
				)
			}
		}
	}
}

// UnregisterNode removes a node from the cluster and cancels its background listeners.
func (r *NodeRegistry) UnregisterNode(nodeID string) {
	r.mu.Lock()
	defer r.mu.Unlock()

	if node, ok := r.nodes[nodeID]; ok {
		if node.cancelFunc != nil {
			node.cancelFunc()
		}
		delete(r.nodes, nodeID)
		slog.Info("media node unregistered", "node_id", nodeID, "cluster_total_nodes", len(r.nodes))
	}
}

// GetNode returns the registered node if present.
func (r *NodeRegistry) GetNode(nodeID string) (*RegisteredNode, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	node, ok := r.nodes[nodeID]
	if !ok {
		return nil, fmt.Errorf("node %s not found in registry", nodeID)
	}
	return node, nil
}

// ActiveNodeCount returns the number of registered media nodes.
func (r *NodeRegistry) ActiveNodeCount() int {
	r.mu.RLock()
	defer r.mu.RUnlock()
	return len(r.nodes)
}

// ListNodes returns a snapshot slice of all registered nodes.
func (r *NodeRegistry) ListNodes() []*RegisteredNode {
	r.mu.RLock()
	defer r.mu.RUnlock()

	list := make([]*RegisteredNode, 0, len(r.nodes))
	for _, n := range r.nodes {
		list = append(list, n)
	}
	return list
}

// BroadcastLicense applies a new or renewed license token to all registered media nodes.
func (r *NodeRegistry) BroadcastLicense(ctx context.Context, licenseToken string) map[string]error {
	nodes := r.ListNodes()
	errorsMap := make(map[string]error)

	for _, n := range nodes {
		res, err := n.client.ApplyLicense(ctx, licenseToken)
		if err != nil {
			errorsMap[n.nodeID] = err
			slog.Error("failed to broadcast license to node", "node_id", n.nodeID, "error", err)
			continue
		}
		if !res.IsValid {
			errorsMap[n.nodeID] = fmt.Errorf("node rejected license: %s", res.ErrorMessage)
			slog.Warn("node rejected applied license", "node_id", n.nodeID, "reason", res.ErrorMessage)
			continue
		}
		slog.Info("license successfully applied to node",
			"node_id", n.nodeID,
			"tier", res.LicenseTier,
			"expires_at", res.ExpiresAtTimestamp,
		)
	}

	return errorsMap
}
