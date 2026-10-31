package application

import (
	"context"
	"fmt"
	"log/slog"
	"sync"
	"time"

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

// NodeEventHandler handles domain media events (ActiveSpeaker, MediaScore, LayerChanged, ResourceClosed) from media nodes.
type NodeEventHandler interface {
	HandleNodeEvent(ctx context.Context, nodeID string, evt *domain.NodeEvent)
}

// NodeEventHandlerFunc allows using a function as NodeEventHandler.
type NodeEventHandlerFunc func(ctx context.Context, nodeID string, evt *domain.NodeEvent)

// HandleNodeEvent calls the underlying function.
func (f NodeEventHandlerFunc) HandleNodeEvent(ctx context.Context, nodeID string, evt *domain.NodeEvent) {
	f(ctx, nodeID, evt)
}

// NodeRegistry manages connected SFU media nodes and enforces cluster node limits.
type NodeRegistry struct {
	mu            sync.RWMutex
	coordinatorID string
	licenseMgr    *LicenseManager
	nodes         map[string]*RegisteredNode
	eventHandler  NodeEventHandler
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

// SetEventHandler registers a handler for media node events (ActiveSpeaker, MediaScore, LayerChanged, ResourceClosed).
func (r *NodeRegistry) SetEventHandler(handler NodeEventHandler) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.eventHandler = handler
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
		slog.Warn("initial watch node events failed on registration, will retry in background",
			"node_id", nodeID,
			"error", err,
		)
		eventsChan = nil
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

	// 4. Start background listener for node events with reconnect backoff
	go r.listenNodeEvents(nodeCtx, nodeID, node, eventsChan)

	return node, nil
}

func (r *NodeRegistry) listenNodeEvents(ctx context.Context, nodeID string, node *RegisteredNode, initialChan <-chan *domain.NodeEvent) {
	eventsChan := initialChan
	backoff := 500 * time.Millisecond
	maxBackoff := 10 * time.Second

	for {
		if eventsChan == nil {
			select {
			case <-ctx.Done():
				return
			case <-time.After(backoff):
			}

			licenseToken := ""
			if r.licenseMgr != nil {
				licenseToken = r.licenseMgr.GetToken()
			}

			ch, err := node.Client().WatchNodeEvents(ctx, r.coordinatorID, licenseToken)
			if err != nil {
				slog.Debug("retrying watch node events connection", "node_id", nodeID, "error", err, "next_backoff", backoff)
				backoff *= 2
				if backoff > maxBackoff {
					backoff = maxBackoff
				}
				continue
			}

			// Successfully reconnected
			slog.Info("watch node events connection established", "node_id", nodeID)
			eventsChan = ch
			backoff = 500 * time.Millisecond

			// Refresh node capacity upon reconnect
			if capInfo, err := node.Client().GetNodeCapacity(ctx); err == nil && capInfo != nil {
				node.updateCapacity(capInfo)
			}
		}

		select {
		case <-ctx.Done():
			return
		case evt, ok := <-eventsChan:
			if !ok {
				slog.Warn("media node event stream terminated, scheduling reconnect", "node_id", nodeID)
				eventsChan = nil
				continue
			}
			if evt != nil {
				if evt.NodeCapacity != nil {
					node.updateCapacity(evt.NodeCapacity)
					slog.Debug("media node capacity updated from event",
						"node_id", nodeID,
						"effective_load", evt.NodeCapacity.EffectiveLoadPercent,
						"license_tier", evt.NodeCapacity.LicenseTier,
					)
				}
				r.mu.RLock()
				handler := r.eventHandler
				r.mu.RUnlock()
				if handler != nil && (evt.ActiveSpeaker != nil || evt.MediaScore != nil || evt.LayerChanged != nil || evt.ResourceClosed != nil) {
					handler.HandleNodeEvent(ctx, nodeID, evt)
				}
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

// Close gracefully cancels all node event streams and clears registered nodes.
func (r *NodeRegistry) Close() {
	r.mu.Lock()
	defer r.mu.Unlock()

	for id, node := range r.nodes {
		if node.cancelFunc != nil {
			node.cancelFunc()
		}
		delete(r.nodes, id)
	}
}
