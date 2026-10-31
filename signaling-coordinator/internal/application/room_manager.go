package application

import (
	"context"
	"sync"

	"ekbatan-room/signaling-coordinator/internal/adapters/outbound/telemetry"
	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

var _ ports.RoomCoordinatorPort = (*RoomManager)(nil)

// RoomManager coordinates room instances and dispatches incoming signaling requests to RoomActors.
type RoomManager struct {
	mu sync.RWMutex

	rooms           map[string]*RoomActor
	routerPort      ports.RouterEnginePort
	transportPort   ports.TransportEnginePort
	mediaStreamPort ports.MediaStreamPort
	iceServers      []domain.IceServer
	licenseMgr      *LicenseManager
	nodeRegistry    *NodeRegistry
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
	}
}

// SetNodeRegistry associates a NodeRegistry with the RoomManager.
func (m *RoomManager) SetNodeRegistry(nr *NodeRegistry) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.nodeRegistry = nr
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

	// 2. Count total active peers to enforce max_concurrent_peers (lock-free via atomic counts)
	totalPeers := 0
	for _, a := range m.rooms {
		totalPeers += a.ParticipantCount()
	}

	if isNewRoom {
		actor = NewRoomActor(
			roomID,
			m.routerPort,
			m.transportPort,
			m.mediaStreamPort,
			m.iceServers,
		)
		m.rooms[roomID] = actor
		telemetry.DefaultMetrics().IncActiveRooms()
	}
	m.mu.Unlock()

	// 3. Check peer limit without holding global m.mu lock
	if m.licenseMgr != nil {
		isNewPeer := true
		if !isNewRoom {
			if _, ok := actor.GetSession(participantID); ok {
				isNewPeer = false
			}
		}

		if isNewPeer {
			if err := m.licenseMgr.CheckCanJoinPeer(totalPeers); err != nil {
				if isNewRoom {
					m.mu.Lock()
					delete(m.rooms, roomID)
					telemetry.DefaultMetrics().DecActiveRooms()
					m.mu.Unlock()
					_ = actor.Close(ctx)
				}
				return nil, err
			}
		}
	}

	res, err := actor.Join(ctx, participantID, participantName, responder)
	if err != nil {
		m.mu.Lock()
		if isNewRoom && actor.ParticipantCount() == 0 {
			delete(m.rooms, roomID)
			telemetry.DefaultMetrics().DecActiveRooms()
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
	if actor.ParticipantCount() == 0 {
		delete(m.rooms, roomID)
		telemetry.DefaultMetrics().DecActiveRooms()
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
		telemetry.DefaultMetrics().DecActiveRooms()
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
