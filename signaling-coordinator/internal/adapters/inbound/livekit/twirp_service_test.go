package livekit_test

import (
	"context"
	"sync"
	"testing"

	livekitproto "github.com/livekit/protocol/livekit"
	"google.golang.org/protobuf/proto"

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

func (m *mockCoordinator) ProduceData(ctx context.Context, cmd *domain.ProduceDataCommand) (*domain.ProduceDataResult, error) {
	return &domain.ProduceDataResult{
		DataProducerID: "dp-" + cmd.Label,
		Label:          cmd.Label,
	}, nil
}

func (m *mockCoordinator) ConsumeData(ctx context.Context, cmd *domain.ConsumeDataCommand) (*domain.ConsumeDataResult, error) {
	return &domain.ConsumeDataResult{
		DataConsumerID: "dc-" + cmd.DataProducerID,
		DataProducerID: cmd.DataProducerID,
	}, nil
}

func (m *mockCoordinator) SendDirectData(ctx context.Context, cmd *domain.SendDirectDataCommand) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.lastDirectData = cmd
	return nil
}

var (
	_ ports.RouterEnginePort    = (*mockCoordinator)(nil)
	_ ports.TransportEnginePort = (*mockCoordinator)(nil)
	_ ports.MediaStreamPort     = (*mockCoordinator)(nil)
)

type mockTwirpResponder struct {
	mu         sync.Mutex
	updates    [][]*domain.Participant
	leaveCalls []struct {
		canReconnect bool
		reason       domain.DisconnectReason
	}
}

func (r *mockTwirpResponder) SendJoinResponse(res *domain.JoinResult) error { return nil }
func (r *mockTwirpResponder) SendSubscriberOffer(slots []*domain.SDPSlot, sid string) error { return nil }
func (r *mockTwirpResponder) SendTrickle(candidate string, target int) error { return nil }
func (r *mockTwirpResponder) SendParticipantUpdate(participants []*domain.Participant) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.updates = append(r.updates, participants)
	return nil
}
func (r *mockTwirpResponder) SendTrackMuted(trackSID string, muted bool) error { return nil }
func (r *mockTwirpResponder) SendSubscribedQualityUpdate(trackSID, codec string, qualities []domain.SubscribedQuality) error { return nil }
func (r *mockTwirpResponder) SendTrackSubscribed(trackSID string) error { return nil }
func (r *mockTwirpResponder) SendActiveSpeakers(speakers []*domain.ActiveSpeakerInfo) error { return nil }
func (r *mockTwirpResponder) SendConnectionQuality(updates []*domain.ConnectionQualityUpdate) error { return nil }
func (r *mockTwirpResponder) SendReconnect(iceServers []domain.IceServer) error { return nil }
func (r *mockTwirpResponder) SendLeave(canReconnect bool, reason domain.DisconnectReason) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.leaveCalls = append(r.leaveCalls, struct {
		canReconnect bool
		reason       domain.DisconnectReason
	}{canReconnect: canReconnect, reason: reason})
	return nil
}
func (r *mockTwirpResponder) Close() error { return nil }

func TestTwirpService_UpdateParticipant_BroadcastsParticipantUpdate_REQ026(t *testing.T) {
	ctx := context.Background()
	engine := &mockCoordinator{}
	roomMgr := application.NewRoomManager(engine, engine, engine, nil, nil)
	twirpSvc := livekit.NewRoomServiceServer(roomMgr)

	roomID := "room-upd-test"
	_, err := twirpSvc.CreateRoom(ctx, &livekitproto.CreateRoomRequest{Name: roomID})
	if err != nil {
		t.Fatalf("CreateRoom failed: %v", err)
	}

	aliceResp := &mockTwirpResponder{}
	bobResp := &mockTwirpResponder{}

	_, err = roomMgr.Join(ctx, roomID, "peer-alice", "Alice", aliceResp)
	if err != nil {
		t.Fatalf("Alice join failed: %v", err)
	}
	_, err = roomMgr.Join(ctx, roomID, "peer-bob", "Bob", bobResp)
	if err != nil {
		t.Fatalf("Bob join failed: %v", err)
	}

	// Reset recorded updates before UpdateParticipant call
	bobResp.mu.Lock()
	bobResp.updates = nil
	bobResp.mu.Unlock()

	// Update Alice's metadata and name
	info, err := twirpSvc.UpdateParticipant(ctx, &livekitproto.UpdateParticipantRequest{
		Room:     roomID,
		Identity: "peer-alice",
		Metadata: `{"role":"speaker"}`,
		Name:     "Alice In Wonderland",
	})
	if err != nil {
		t.Fatalf("UpdateParticipant failed: %v", err)
	}
	if info.Metadata != `{"role":"speaker"}` || info.Name != "Alice In Wonderland" {
		t.Fatalf("unexpected ParticipantInfo: %+v", info)
	}

	// Verify Bob received the broadcasted update
	bobResp.mu.Lock()
	updates := bobResp.updates
	bobResp.mu.Unlock()

	if len(updates) == 0 {
		t.Fatalf("expected Bob to receive participant update broadcast, got 0")
	}

	lastBatch := updates[len(updates)-1]
	var foundAlice *domain.Participant
	for _, p := range lastBatch {
		if p.ID == "peer-alice" {
			foundAlice = p
			break
		}
	}
	if foundAlice == nil {
		t.Fatalf("Alice not found in broadcast update to Bob")
	}
	if foundAlice.GetMetadata() != `{"role":"speaker"}` {
		t.Fatalf("expected Alice metadata '{\"role\":\"speaker\"}', got %q", foundAlice.GetMetadata())
	}
	if foundAlice.GetName() != "Alice In Wonderland" {
		t.Fatalf("expected Alice name 'Alice In Wonderland', got %q", foundAlice.GetName())
	}
}

func TestTwirpService_RemoveParticipant_SendsLeaveRequestAndRejectsReconnect_REQ027(t *testing.T) {
	ctx := context.Background()
	engine := &mockCoordinator{}
	roomMgr := application.NewRoomManager(engine, engine, engine, nil, nil)
	twirpSvc := livekit.NewRoomServiceServer(roomMgr)

	roomID := "room-evict-test"
	_, err := twirpSvc.CreateRoom(ctx, &livekitproto.CreateRoomRequest{Name: roomID})
	if err != nil {
		t.Fatalf("CreateRoom failed: %v", err)
	}

	aliceResp := &mockTwirpResponder{}
	bobResp := &mockTwirpResponder{}

	_, err = roomMgr.Join(ctx, roomID, "peer-alice", "Alice", aliceResp)
	if err != nil {
		t.Fatalf("Alice join failed: %v", err)
	}
	bobRes, err := roomMgr.Join(ctx, roomID, "peer-bob", "Bob", bobResp)
	if err != nil {
		t.Fatalf("Bob join failed: %v", err)
	}

	// Remove Bob via Twirp
	_, err = twirpSvc.RemoveParticipant(ctx, &livekitproto.RoomParticipantIdentity{
		Room:     roomID,
		Identity: "peer-bob",
	})
	if err != nil {
		t.Fatalf("RemoveParticipant failed: %v", err)
	}

	// Verify Bob received SendLeave with CanReconnect = false and Reason = UserRejected
	bobResp.mu.Lock()
	leaveCalls := bobResp.leaveCalls
	bobResp.mu.Unlock()

	if len(leaveCalls) == 0 {
		t.Fatalf("expected Bob to receive SendLeave, got 0 calls")
	}
	if leaveCalls[0].canReconnect != false {
		t.Errorf("expected canReconnect = false, got true")
	}
	if leaveCalls[0].reason != domain.DisconnectReasonUserRejected {
		t.Errorf("expected reason = DisconnectReasonUserRejected, got %v", leaveCalls[0].reason)
	}

	// Verify Bob is marked as evicted in RoomManager
	if !roomMgr.IsParticipantEvicted(roomID, "peer-bob", bobRes.Participant.SID) {
		t.Errorf("expected peer-bob to be marked as evicted in room")
	}

	// Subsequent join with evicted identity should be rejected
	_, err = roomMgr.Join(ctx, roomID, "peer-bob", "Bob", &mockTwirpResponder{})
	if err == nil {
		t.Fatalf("expected join to fail with ErrParticipantEvicted, got nil")
	}

	// Subsequent join with previous_sid set to evicted SID should be rejected
	reconnectCtx := application.WithPreviousSID(ctx, bobRes.Participant.SID)
	_, err = roomMgr.Join(reconnectCtx, roomID, "bob-new-identity", "Bob", &mockTwirpResponder{})
	if err == nil {
		t.Fatalf("expected join with evicted SID to fail, got nil")
	}
}

func TestTwirpService_SendData_EncodesDataPacket_REQ028(t *testing.T) {
	ctx := context.Background()
	engine := &mockCoordinator{}
	roomMgr := application.NewRoomManager(engine, engine, engine, nil, nil)
	twirpSvc := livekit.NewRoomServiceServer(roomMgr)

	roomID := "room-send-data"
	_, err := twirpSvc.CreateRoom(ctx, &livekitproto.CreateRoomRequest{Name: roomID})
	if err != nil {
		t.Fatalf("CreateRoom failed: %v", err)
	}

	payload := []byte("chat message payload")
	destIdentities := []string{"peer-bob", "peer-charlie"}
	topic := "lk-chat"

	_, err = twirpSvc.SendData(ctx, &livekitproto.SendDataRequest{
		Room:                  roomID,
		Data:                  payload,
		Kind:                  livekitproto.DataPacket_RELIABLE,
		Topic:                 &topic,
		DestinationIdentities: destIdentities,
	})
	if err != nil {
		t.Fatalf("SendData failed: %v", err)
	}

	// Verify engine received direct data packet
	lastCmd := engine.LastDirectData()
	if lastCmd == nil {
		t.Fatalf("expected engine.SendDirectData to be called, got nil")
	}

	var dp livekitproto.DataPacket
	if err := proto.Unmarshal(lastCmd.Payload, &dp); err != nil {
		t.Fatalf("failed to unmarshal DataPacket: %v", err)
	}

	if dp.Kind != livekitproto.DataPacket_RELIABLE {
		t.Errorf("expected kind RELIABLE, got %v", dp.Kind)
	}
	userPkt := dp.GetUser()
	if userPkt == nil {
		t.Fatalf("expected user packet inside DataPacket, got nil")
	}
	if string(userPkt.Payload) != string(payload) {
		t.Errorf("expected payload %q, got %q", string(payload), string(userPkt.Payload))
	}
	if userPkt.GetTopic() != topic {
		t.Errorf("expected topic %q, got %q", topic, userPkt.GetTopic())
	}
	if len(userPkt.DestinationIdentities) != len(destIdentities) {
		t.Errorf("expected destIdentities %v, got %v", destIdentities, userPkt.DestinationIdentities)
	}
}
