package application

import (
	"context"
	"sync"

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
}

// NewRoomManager creates a new RoomManager with constructor-injected ports.
func NewRoomManager(
	routerPort ports.RouterEnginePort,
	transportPort ports.TransportEnginePort,
	mediaStreamPort ports.MediaStreamPort,
	iceServers []domain.IceServer,
) *RoomManager {
	return &RoomManager{
		rooms:           make(map[string]*RoomActor),
		routerPort:      routerPort,
		transportPort:   transportPort,
		mediaStreamPort: mediaStreamPort,
		iceServers:      iceServers,
	}
}

func (m *RoomManager) getOrCreateRoomActor(roomID string) *RoomActor {
	m.mu.Lock()
	defer m.mu.Unlock()

	actor, exists := m.rooms[roomID]
	if !exists {
		actor = NewRoomActor(
			roomID,
			m.routerPort,
			m.transportPort,
			m.mediaStreamPort,
			m.iceServers,
		)
		m.rooms[roomID] = actor
	}
	return actor
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

func (m *RoomManager) Join(
	ctx context.Context,
	roomID, participantID, participantName string,
	responder ports.PeerResponderPort,
) (*domain.JoinResult, error) {
	actor := m.getOrCreateRoomActor(roomID)
	return actor.Join(ctx, participantID, participantName, responder)
}

func (m *RoomManager) Leave(ctx context.Context, roomID, participantID string) error {
	actor, err := m.getRoomActor(roomID)
	if err != nil {
		return err
	}
	return actor.Leave(ctx, participantID)
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
