package application

import (
	"context"
	"sync"

	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

var _ ports.RoomCoordinatorPort = (*RoomManager)(nil)

type roomPlacementCleaner interface {
	RemoveRoom(roomID string)
}

// RoomManager coordinates room instances and dispatches incoming signaling requests to RoomActors.
type RoomManager struct {
	mu sync.RWMutex

	rooms             map[string]*RoomActor
	routerPort        ports.RouterEnginePort
	transportPort     ports.TransportEnginePort
	mediaStreamPort   ports.MediaStreamPort
	iceServers        []domain.IceServer
	licenseMgr        *LicenseManager
	nodeRegistry      *NodeRegistry
	placementStrategy ports.RoomPlacementStrategy
	metricsPort       ports.TelemetryMetricsPort
}

// NewRoomManager creates a new RoomManager with constructor-injected ports.
func NewRoomManager(
	routerPort ports.RouterEnginePort,
	transportPort ports.TransportEnginePort,
	mediaStreamPort ports.MediaStreamPort,
	iceServers []domain.IceServer,
	licenseMgr ...*LicenseManager,
) *RoomManager {
	var lm *LicenseManager
	if len(licenseMgr) > 0 && licenseMgr[0] != nil {
		lm = licenseMgr[0]
	} else {
		lm, _ = NewLicenseManager("")
	}
	return &RoomManager{
		rooms:           make(map[string]*RoomActor),
		routerPort:      routerPort,
		transportPort:   transportPort,
		mediaStreamPort: mediaStreamPort,
		iceServers:      iceServers,
		licenseMgr:      lm,
		metricsPort:     noopTelemetryMetrics{},
	}
}

// SetMetricsPort sets the TelemetryMetricsPort for metric reporting.
func (m *RoomManager) SetMetricsPort(mp ports.TelemetryMetricsPort) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if mp == nil {
		m.metricsPort = noopTelemetryMetrics{}
	} else {
		m.metricsPort = mp
	}
}

// SetPlacementStrategy overrides or sets the room placement strategy.
func (m *RoomManager) SetPlacementStrategy(ps ports.RoomPlacementStrategy) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.placementStrategy = ps
}

// SetNodeRegistry associates a NodeRegistry with the RoomManager, wires default placement, and registers event handler.
func (m *RoomManager) SetNodeRegistry(nr *NodeRegistry) {
	m.mu.Lock()
	m.nodeRegistry = nr
	if m.placementStrategy == nil && nr != nil {
		m.placementStrategy = NewDefaultPlacementStrategy(nr)
	}
	m.mu.Unlock()

	if nr != nil {
		nr.SetEventHandler(NodeEventHandlerFunc(func(ctx context.Context, nodeID string, evt *domain.NodeEvent) {
			if evt == nil || evt.RouterID == "" {
				return
			}
			m.mu.RLock()
			defer m.mu.RUnlock()
			for _, actor := range m.rooms {
				if actor.RouterID() == evt.RouterID {
					actor.HandleNodeEvent(ctx, evt)
					break
				}
			}
		}))
	}
}

// NodeRegistry returns the associated NodeRegistry, if any.
func (m *RoomManager) NodeRegistry() *NodeRegistry {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.nodeRegistry
}

func (m *RoomManager) getRoomActor(roomID string) (*RoomActor, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	actor, exists := m.rooms[roomID]
	if !exists {
		return nil, domain.ErrRoomNotFound
	}
	return actor, nil
}

// ActiveRoomsCount returns the current number of active rooms.
func (m *RoomManager) ActiveRoomsCount() int {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return len(m.rooms)
}

// ActivePeersCount returns the total number of connected peers across all rooms.
func (m *RoomManager) ActivePeersCount() int {
	m.mu.RLock()
	defer m.mu.RUnlock()

	total := 0
	for _, actor := range m.rooms {
		total += actor.ParticipantCount()
	}
	return total
}

func (m *RoomManager) cleanupRoomPlacementLocked(roomID string) {
	if cleaner, ok := m.placementStrategy.(roomPlacementCleaner); ok {
		cleaner.RemoveRoom(roomID)
	}
}

func (m *RoomManager) Join(
	ctx context.Context,
	roomID, participantID, participantName string,
	responder ports.PeerResponderPort,
) (*domain.JoinResult, error) {
	m.mu.Lock()
	actor, exists := m.rooms[roomID]
	isNewRoom := !exists

	// 1. Enforce max_concurrent_rooms if creating a new room
	if isNewRoom && m.licenseMgr != nil {
		if err := m.licenseMgr.CheckCanCreateRoom(len(m.rooms)); err != nil {
			m.mu.Unlock()
			return nil, err
		}
	}

	// 2. Room placement check
	var selectedNodeID string
	if isNewRoom && m.placementStrategy != nil {
		nodeID, err := m.placementStrategy.SelectNodeForNewRoom(ctx, roomID)
		if err != nil {
			m.mu.Unlock()
			return nil, err
		}
		selectedNodeID = nodeID
	} else if !isNewRoom && m.placementStrategy != nil {
		nodeID, _, err := m.placementStrategy.ResolveNodeForPeerJoin(ctx, roomID, participantID)
		if err != nil {
			m.mu.Unlock()
			return nil, err
		}
		selectedNodeID = nodeID
	}

	// 3. Count total active peers + in-flight joins to strictly enforce max_concurrent_peers
	totalPeers := 0
	for _, a := range m.rooms {
		totalPeers += a.ParticipantCount() + int(a.InFlightJoins())
	}

	if m.licenseMgr != nil {
		isNewPeer := true
		if !isNewRoom {
			if _, ok := actor.GetSession(participantID); ok {
				isNewPeer = false
			}
		}

		if isNewPeer {
			if err := m.licenseMgr.CheckCanJoinPeer(totalPeers); err != nil {
				m.mu.Unlock()
				return nil, err
			}
		}
	}

	if isNewRoom {
		routerPort := m.routerPort
		transportPort := m.transportPort
		mediaStreamPort := m.mediaStreamPort

		if selectedNodeID != "" && m.nodeRegistry != nil {
			if node, err := m.nodeRegistry.GetNode(selectedNodeID); err == nil && node != nil {
				client := node.Client()
				if rp, ok := client.(ports.RouterEnginePort); ok {
					routerPort = rp
				}
				if tp, ok := client.(ports.TransportEnginePort); ok {
					transportPort = tp
				}
				if mp, ok := client.(ports.MediaStreamPort); ok {
					mediaStreamPort = mp
				}
			}
		}

		actor = NewRoomActor(
			roomID,
			routerPort,
			transportPort,
			mediaStreamPort,
			m.iceServers,
			m.metricsPort,
		)
		actor.SetNodeRegistry(m.nodeRegistry)
		actor.SetPrimaryNodeID(selectedNodeID)
		m.rooms[roomID] = actor
		m.metricsPort.IncActiveRooms()
	}

	actor.BeginJoin()
	m.mu.Unlock()

	res, err := actor.Join(ctx, participantID, participantName, responder, selectedNodeID)
	actor.EndJoin()
	if err != nil {
		m.mu.Lock()
		if isNewRoom && actor.IsEmptyAndIdle() {
			delete(m.rooms, roomID)
			m.cleanupRoomPlacementLocked(roomID)
			m.metricsPort.DecActiveRooms()
			_ = actor.Close(ctx)
		}
		m.mu.Unlock()
		return nil, err
	}
	return res, nil
}

func (m *RoomManager) Leave(ctx context.Context, roomID, participantID string) error {
	actor, err := m.getRoomActor(roomID)
	if err != nil {
		return err
	}
	leaveErr := actor.Leave(ctx, participantID)

	// Clean up empty room so room quota is immediately reclaimed
	m.mu.Lock()
	if actor.IsEmptyAndIdle() {
		delete(m.rooms, roomID)
		m.cleanupRoomPlacementLocked(roomID)
		m.metricsPort.DecActiveRooms()
		_ = actor.Close(ctx)
	}
	m.mu.Unlock()

	return leaveErr
}

// Close gracefully closes all active rooms and releases all media router and transport resources on SFU nodes.
func (m *RoomManager) Close(ctx context.Context) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	for roomID, actor := range m.rooms {
		_ = actor.Close(ctx)
		delete(m.rooms, roomID)
		m.cleanupRoomPlacementLocked(roomID)
		m.metricsPort.DecActiveRooms()
	}
	return nil
}

func (m *RoomManager) PublishTracks(
	ctx context.Context,
	roomID, participantID string,
	dtlsRole string,
	fingerprints []domain.DtlsFingerprintInfo,
	tracks []domain.TrackPublishCommand,
) (*domain.TransportInfo, error) {
	actor, err := m.getRoomActor(roomID)
	if err != nil {
		return nil, err
	}
	return actor.PublishTracks(ctx, participantID, dtlsRole, fingerprints, tracks)
}

func (m *RoomManager) HandleSubscriberAnswer(ctx context.Context, roomID, participantID string, dtlsRole string, fingerprints []domain.DtlsFingerprintInfo) error {
	actor, err := m.getRoomActor(roomID)
	if err != nil {
		return err
	}
	return actor.HandleSubscriberAnswer(ctx, participantID, dtlsRole, fingerprints)
}

func (m *RoomManager) HandleTrickle(ctx context.Context, roomID, participantID string, candidateJSON string, target int) error {
	actor, err := m.getRoomActor(roomID)
	if err != nil {
		return err
	}
	return actor.HandleTrickle(ctx, participantID, candidateJSON, target)
}

func (m *RoomManager) SetTrackMuted(ctx context.Context, roomID, participantID, trackSID string, muted bool) error {
	actor, err := m.getRoomActor(roomID)
	if err != nil {
		return err
	}
	return actor.SetTrackMuted(ctx, participantID, trackSID, muted)
}

func (m *RoomManager) SetConsumerPaused(ctx context.Context, roomID, participantID, trackSID string, paused bool) error {
	actor, err := m.getRoomActor(roomID)
	if err != nil {
		return err
	}
	return actor.SetConsumerPaused(ctx, participantID, trackSID, paused)
}

func (m *RoomManager) SetConsumerLayers(ctx context.Context, roomID, participantID, consumerID string, spatial, temporal int32) error {
	actor, err := m.getRoomActor(roomID)
	if err != nil {
		return err
	}
	return actor.SetConsumerLayers(ctx, participantID, consumerID, spatial, temporal)
}

func (m *RoomManager) RestartPublisherIce(ctx context.Context, roomID, participantID string) (*domain.IceInfo, error) {
	actor, err := m.getRoomActor(roomID)
	if err != nil {
		return nil, err
	}
	return actor.RestartPublisherIce(ctx, participantID)
}

func (m *RoomManager) RestartSubscriberIce(ctx context.Context, roomID, participantID string) (*domain.IceInfo, error) {
	actor, err := m.getRoomActor(roomID)
	if err != nil {
		return nil, err
	}
	return actor.RestartSubscriberIce(ctx, participantID)
}
