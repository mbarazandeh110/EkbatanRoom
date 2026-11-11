package livekit

import (
	"context"
	"errors"

	livekitproto "github.com/livekit/protocol/livekit"
	"github.com/twitchtv/twirp"

	"ekbatan-room/signaling-coordinator/internal/application"
	"ekbatan-room/signaling-coordinator/internal/domain"
)

// RoomServiceServer implements livekitproto.RoomService Twirp interface.
type RoomServiceServer struct {
	roomMgr *application.RoomManager
}

// NewRoomServiceServer instantiates the Twirp RoomService handler.
func NewRoomServiceServer(roomMgr *application.RoomManager) *RoomServiceServer {
	return &RoomServiceServer{
		roomMgr: roomMgr,
	}
}

var _ livekitproto.RoomService = (*RoomServiceServer)(nil)

func (s *RoomServiceServer) CreateRoom(ctx context.Context, req *livekitproto.CreateRoomRequest) (*livekitproto.Room, error) {
	if req.GetName() == "" {
		return nil, twirp.RequiredArgumentError("name")
	}

	room, err := s.roomMgr.CreateRoom(ctx, req.GetName(), req.GetMetadata(), req.GetEmptyTimeout(), req.GetMaxParticipants())
	if err != nil {
		if errors.Is(err, domain.ErrMaxRoomsExceeded) {
			return nil, twirp.NewError(twirp.ResourceExhausted, "maximum rooms limit reached")
		}
		return nil, twirp.InternalErrorWith(err)
	}

	return s.mapRoomToProto(room), nil
}

func (s *RoomServiceServer) ListRooms(ctx context.Context, req *livekitproto.ListRoomsRequest) (*livekitproto.ListRoomsResponse, error) {
	rooms := s.roomMgr.ListRooms(req.GetNames())
	protoRooms := make([]*livekitproto.Room, 0, len(rooms))
	for _, r := range rooms {
		protoRooms = append(protoRooms, s.mapRoomToProto(r))
	}
	return &livekitproto.ListRoomsResponse{
		Rooms: protoRooms,
	}, nil
}

func (s *RoomServiceServer) DeleteRoom(ctx context.Context, req *livekitproto.DeleteRoomRequest) (*livekitproto.DeleteRoomResponse, error) {
	if req.GetRoom() == "" {
		return nil, twirp.RequiredArgumentError("room")
	}

	err := s.roomMgr.DeleteRoom(ctx, req.GetRoom())
	if err != nil && !errors.Is(err, domain.ErrRoomNotFound) {
		return nil, twirp.InternalErrorWith(err)
	}

	return &livekitproto.DeleteRoomResponse{}, nil
}

func (s *RoomServiceServer) ListParticipants(ctx context.Context, req *livekitproto.ListParticipantsRequest) (*livekitproto.ListParticipantsResponse, error) {
	if req.GetRoom() == "" {
		return nil, twirp.RequiredArgumentError("room")
	}

	parts, err := s.roomMgr.ListParticipants(req.GetRoom())
	if err != nil {
		if errors.Is(err, domain.ErrRoomNotFound) {
			return nil, twirp.NotFoundError("room not found")
		}
		return nil, twirp.InternalErrorWith(err)
	}

	protoParts := make([]*livekitproto.ParticipantInfo, 0, len(parts))
	for _, p := range parts {
		protoParts = append(protoParts, s.mapParticipantToProto(p))
	}

	return &livekitproto.ListParticipantsResponse{
		Participants: protoParts,
	}, nil
}

func (s *RoomServiceServer) GetParticipant(ctx context.Context, req *livekitproto.RoomParticipantIdentity) (*livekitproto.ParticipantInfo, error) {
	if req.GetRoom() == "" {
		return nil, twirp.RequiredArgumentError("room")
	}
	if req.GetIdentity() == "" {
		return nil, twirp.RequiredArgumentError("identity")
	}

	p, err := s.roomMgr.GetParticipant(req.GetRoom(), req.GetIdentity())
	if err != nil {
		if errors.Is(err, domain.ErrParticipantNotFound) || errors.Is(err, domain.ErrRoomNotFound) {
			return nil, twirp.NotFoundError("participant not found")
		}
		return nil, twirp.InternalErrorWith(err)
	}

	return s.mapParticipantToProto(p), nil
}

func (s *RoomServiceServer) RemoveParticipant(ctx context.Context, req *livekitproto.RoomParticipantIdentity) (*livekitproto.RemoveParticipantResponse, error) {
	if req.GetRoom() == "" {
		return nil, twirp.RequiredArgumentError("room")
	}
	if req.GetIdentity() == "" {
		return nil, twirp.RequiredArgumentError("identity")
	}

	err := s.roomMgr.Leave(ctx, req.GetRoom(), req.GetIdentity())
	if err != nil && !errors.Is(err, domain.ErrParticipantNotFound) && !errors.Is(err, domain.ErrRoomNotFound) {
		return nil, twirp.InternalErrorWith(err)
	}

	return &livekitproto.RemoveParticipantResponse{}, nil
}

func (s *RoomServiceServer) MutePublishedTrack(ctx context.Context, req *livekitproto.MuteRoomTrackRequest) (*livekitproto.MuteRoomTrackResponse, error) {
	if req.GetRoom() == "" {
		return nil, twirp.RequiredArgumentError("room")
	}
	if req.GetIdentity() == "" {
		return nil, twirp.RequiredArgumentError("identity")
	}
	if req.GetTrackSid() == "" {
		return nil, twirp.RequiredArgumentError("track_sid")
	}

	err := s.roomMgr.SetTrackMuted(ctx, req.GetRoom(), req.GetIdentity(), req.GetTrackSid(), req.GetMuted())
	if err != nil {
		return nil, twirp.InternalErrorWith(err)
	}

	p, err := s.roomMgr.GetParticipant(req.GetRoom(), req.GetIdentity())
	var trackInfo *livekitproto.TrackInfo
	if err == nil && p != nil {
		for _, t := range p.ListTracks() {
			if t.SID == req.GetTrackSid() {
				protoTracks := mapDomainTracksToProto([]*domain.Track{t})
				if len(protoTracks) > 0 {
					trackInfo = protoTracks[0]
				}
				break
			}
		}
	}

	return &livekitproto.MuteRoomTrackResponse{
		Track: trackInfo,
	}, nil
}

func (s *RoomServiceServer) UpdateParticipant(ctx context.Context, req *livekitproto.UpdateParticipantRequest) (*livekitproto.ParticipantInfo, error) {
	if req.GetRoom() == "" {
		return nil, twirp.RequiredArgumentError("room")
	}
	if req.GetIdentity() == "" {
		return nil, twirp.RequiredArgumentError("identity")
	}

	if req.GetMetadata() != "" {
		if err := s.roomMgr.UpdateParticipantMetadata(req.GetRoom(), req.GetIdentity(), req.GetMetadata()); err != nil {
			return nil, twirp.InternalErrorWith(err)
		}
	}

	p, err := s.roomMgr.GetParticipant(req.GetRoom(), req.GetIdentity())
	if err != nil {
		return nil, twirp.NotFoundError("participant not found")
	}

	return s.mapParticipantToProto(p), nil
}

func (s *RoomServiceServer) UpdateSubscriptions(ctx context.Context, req *livekitproto.UpdateSubscriptionsRequest) (*livekitproto.UpdateSubscriptionsResponse, error) {
	if req.GetRoom() == "" {
		return nil, twirp.RequiredArgumentError("room")
	}
	if req.GetIdentity() == "" {
		return nil, twirp.RequiredArgumentError("identity")
	}

	for _, sid := range req.GetTrackSids() {
		_ = s.roomMgr.SetConsumerPaused(ctx, req.GetRoom(), req.GetIdentity(), sid, !req.GetSubscribe())
	}
	for _, pt := range req.GetParticipantTracks() {
		for _, sid := range pt.GetTrackSids() {
			_ = s.roomMgr.SetConsumerPaused(ctx, req.GetRoom(), req.GetIdentity(), sid, !req.GetSubscribe())
		}
	}

	return &livekitproto.UpdateSubscriptionsResponse{}, nil
}

func (s *RoomServiceServer) SendData(ctx context.Context, req *livekitproto.SendDataRequest) (*livekitproto.SendDataResponse, error) {
	if req.GetRoom() == "" {
		return nil, twirp.RequiredArgumentError("room")
	}
	return &livekitproto.SendDataResponse{}, nil
}

func (s *RoomServiceServer) UpdateRoomMetadata(ctx context.Context, req *livekitproto.UpdateRoomMetadataRequest) (*livekitproto.Room, error) {
	if req.GetRoom() == "" {
		return nil, twirp.RequiredArgumentError("room")
	}

	if err := s.roomMgr.UpdateRoomMetadata(req.GetRoom(), req.GetMetadata()); err != nil {
		return nil, twirp.InternalErrorWith(err)
	}

	room, err := s.roomMgr.GetRoom(req.GetRoom())
	if err != nil {
		return nil, twirp.NotFoundError("room not found")
	}

	return s.mapRoomToProto(room), nil
}

func (s *RoomServiceServer) ForwardParticipant(ctx context.Context, req *livekitproto.ForwardParticipantRequest) (*livekitproto.ForwardParticipantResponse, error) {
	return &livekitproto.ForwardParticipantResponse{}, nil
}

func (s *RoomServiceServer) MoveParticipant(ctx context.Context, req *livekitproto.MoveParticipantRequest) (*livekitproto.MoveParticipantResponse, error) {
	return &livekitproto.MoveParticipantResponse{}, nil
}

func (s *RoomServiceServer) PerformRpc(ctx context.Context, req *livekitproto.PerformRpcRequest) (*livekitproto.PerformRpcResponse, error) {
	return &livekitproto.PerformRpcResponse{}, nil
}

func (s *RoomServiceServer) mapRoomToProto(r *domain.Room) *livekitproto.Room {
	if r == nil {
		return nil
	}
	parts := r.ListParticipants()
	return &livekitproto.Room{
		Sid:             r.ID,
		Name:            r.ID,
		EmptyTimeout:    r.EmptyTimeout,
		MaxParticipants: r.MaxParticipants,
		CreationTime:    r.CreatedAt.Unix(),
		Metadata:        r.GetMetadata(),
		NumParticipants: uint32(len(parts)),
		ActiveRecording: false,
	}
}

func (s *RoomServiceServer) mapParticipantToProto(p *domain.Participant) *livekitproto.ParticipantInfo {
	if p == nil {
		return nil
	}
	return &livekitproto.ParticipantInfo{
		Sid:        p.SID,
		Identity:   p.ID,
		Name:       p.Name,
		State:      mapDomainParticipantStateToProto(p.GetState()),
		Tracks:     mapDomainTracksToProto(p.ListTracks()),
		JoinedAt:   p.JoinedAt.Unix(),
		Metadata:   p.GetMetadata(),
		Permission: defaultParticipantPermission(),
	}
}
