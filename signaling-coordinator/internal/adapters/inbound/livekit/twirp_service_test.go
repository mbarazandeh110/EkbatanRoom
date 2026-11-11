package livekit_test

import (
	"context"
	"testing"

	livekitproto "github.com/livekit/protocol/livekit"

	"ekbatan-room/signaling-coordinator/internal/adapters/inbound/livekit"
	"ekbatan-room/signaling-coordinator/internal/application"
	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

type mockTwirpEngine struct {
	*mockCoordinator
}

func TestTwirpService_RoomLifecycle(t *testing.T) {
	ctx := context.Background()
	engine := &mockCoordinator{}
	roomMgr := application.NewRoomManager(engine, engine, engine, nil, nil)
	twirpSvc := livekit.NewRoomServiceServer(roomMgr)

	// 1. Create Room
	createReq := &livekitproto.CreateRoomRequest{
		Name:            "conf-twirp-1",
		EmptyTimeout:    300,
		MaxParticipants: 50,
		Metadata:        `{"subject":"math"}`,
	}
	room, err := twirpSvc.CreateRoom(ctx, createReq)
	if err != nil {
		t.Fatalf("CreateRoom failed: %v", err)
	}
	if room.Name != "conf-twirp-1" || room.Metadata != `{"subject":"math"}` {
		t.Fatalf("unexpected room: %+v", room)
	}

	// 2. List Rooms
	listRes, err := twirpSvc.ListRooms(ctx, &livekitproto.ListRoomsRequest{})
	if err != nil {
		t.Fatalf("ListRooms failed: %v", err)
	}
	if len(listRes.Rooms) != 1 || listRes.Rooms[0].Name != "conf-twirp-1" {
		t.Fatalf("expected 1 room, got %+v", listRes.Rooms)
	}

	// 3. Update Room Metadata
	updRoom, err := twirpSvc.UpdateRoomMetadata(ctx, &livekitproto.UpdateRoomMetadataRequest{
		Room:     "conf-twirp-1",
		Metadata: `{"subject":"physics"}`,
	})
	if err != nil {
		t.Fatalf("UpdateRoomMetadata failed: %v", err)
	}
	if updRoom.Metadata != `{"subject":"physics"}` {
		t.Fatalf("expected updated metadata, got %s", updRoom.Metadata)
	}

	// 4. Delete Room
	delRes, err := twirpSvc.DeleteRoom(ctx, &livekitproto.DeleteRoomRequest{
		Room: "conf-twirp-1",
	})
	if err != nil || delRes == nil {
		t.Fatalf("DeleteRoom failed: %v", err)
	}

	// Verify room is deleted
	listRes2, err := twirpSvc.ListRooms(ctx, &livekitproto.ListRoomsRequest{})
	if err != nil {
		t.Fatalf("ListRooms after delete failed: %v", err)
	}
	if len(listRes2.Rooms) != 0 {
		t.Fatalf("expected 0 rooms after delete, got %d", len(listRes2.Rooms))
	}
}

func (m *mockCoordinator) CreateRouter(ctx context.Context, roomID string) (*domain.RouterInfo, error) {
	return &domain.RouterInfo{
		RouterID:    "router-" + roomID,
		WorkerIndex: 0,
	}, nil
}

func (m *mockCoordinator) CloseRouter(ctx context.Context, routerID string) error {
	return nil
}

func (m *mockCoordinator) CreateTransport(ctx context.Context, cmd *domain.CreateTransportCommand) (*domain.TransportInfo, error) {
	return &domain.TransportInfo{
		TransportID: "tp-" + cmd.PeerID,
		IceUfrag:    "ufrag-test",
		IcePwd:      "pwd-test",
	}, nil
}

func (m *mockCoordinator) ConnectTransport(ctx context.Context, cmd *domain.ConnectTransportCommand) error {
	return nil
}

func (m *mockCoordinator) RestartIce(ctx context.Context, routerID, transportID string) (*domain.IceInfo, error) {
	return &domain.IceInfo{
		IceUfrag: "ufrag-new",
		IcePwd:   "pwd-new",
	}, nil
}

func (m *mockCoordinator) CloseTransport(ctx context.Context, routerID, transportID string) error {
	return nil
}

func (m *mockCoordinator) Produce(ctx context.Context, cmd *domain.ProduceCommand) (*domain.ProduceResult, error) {
	return &domain.ProduceResult{
		ProducerID: "prod-" + cmd.PeerID,
		Kind:       cmd.Kind,
	}, nil
}

func (m *mockCoordinator) Consume(ctx context.Context, cmd *domain.ConsumeCommand) (*domain.ConsumeResult, error) {
	return &domain.ConsumeResult{
		ConsumerID: "cons-" + cmd.ProducerID,
		ProducerID: cmd.ProducerID,
	}, nil
}

func (m *mockCoordinator) SetMediaPaused(ctx context.Context, cmd *domain.SetMediaPausedCommand) error {
	return nil
}

func (m *mockCoordinator) SetConsumerPreferredLayers(ctx context.Context, cmd *domain.SetPreferredLayersCommand) error {
	return nil
}

func (m *mockCoordinator) CloseResource(ctx context.Context, routerID string, resourceType domain.ResourceType, resourceID string) error {
	return nil
}

var (
	_ ports.RouterEnginePort    = (*mockCoordinator)(nil)
	_ ports.TransportEnginePort = (*mockCoordinator)(nil)
	_ ports.MediaStreamPort     = (*mockCoordinator)(nil)
)
