package media_node_grpc_test

import (
	"context"
	"errors"
	"testing"
	"time"

	"google.golang.org/grpc"

	sfuv1 "ekbatan-room/signaling-coordinator/gen/proto/sfu/v1"
	"ekbatan-room/signaling-coordinator/internal/adapters/outbound/media_node_grpc"
	"ekbatan-room/signaling-coordinator/internal/domain"
)

type mockSfuMediaEngineServiceClient struct {
	sfuv1.SfuMediaEngineServiceClient

	CreateRouterFunc             func(ctx context.Context, in *sfuv1.CreateRouterRequest, opts ...grpc.CallOption) (*sfuv1.CreateRouterResponse, error)
	CreateTransportFunc          func(ctx context.Context, in *sfuv1.CreateTransportRequest, opts ...grpc.CallOption) (*sfuv1.CreateTransportResponse, error)
	ConnectTransportFunc         func(ctx context.Context, in *sfuv1.ConnectTransportRequest, opts ...grpc.CallOption) (*sfuv1.ConnectTransportResponse, error)
	RestartIceFunc               func(ctx context.Context, in *sfuv1.RestartIceRequest, opts ...grpc.CallOption) (*sfuv1.RestartIceResponse, error)
	ProduceFunc                  func(ctx context.Context, in *sfuv1.ProduceRequest, opts ...grpc.CallOption) (*sfuv1.ProduceResponse, error)
	ConsumeFunc                  func(ctx context.Context, in *sfuv1.ConsumeRequest, opts ...grpc.CallOption) (*sfuv1.ConsumeResponse, error)
	SetMediaPausedFunc           func(ctx context.Context, in *sfuv1.SetMediaPausedRequest, opts ...grpc.CallOption) (*sfuv1.SetMediaPausedResponse, error)
	SetConsumerPreferredLayersFunc func(ctx context.Context, in *sfuv1.SetConsumerPreferredLayersRequest, opts ...grpc.CallOption) (*sfuv1.SetConsumerPreferredLayersResponse, error)
	CloseResourceFunc            func(ctx context.Context, in *sfuv1.CloseResourceRequest, opts ...grpc.CallOption) (*sfuv1.CloseResourceResponse, error)
}

func (m *mockSfuMediaEngineServiceClient) CreateRouter(ctx context.Context, in *sfuv1.CreateRouterRequest, opts ...grpc.CallOption) (*sfuv1.CreateRouterResponse, error) {
	if m.CreateRouterFunc != nil {
		return m.CreateRouterFunc(ctx, in, opts...)
	}
	return nil, errors.New("not implemented")
}

func (m *mockSfuMediaEngineServiceClient) CreateTransport(ctx context.Context, in *sfuv1.CreateTransportRequest, opts ...grpc.CallOption) (*sfuv1.CreateTransportResponse, error) {
	if m.CreateTransportFunc != nil {
		return m.CreateTransportFunc(ctx, in, opts...)
	}
	return nil, errors.New("not implemented")
}

func (m *mockSfuMediaEngineServiceClient) ConnectTransport(ctx context.Context, in *sfuv1.ConnectTransportRequest, opts ...grpc.CallOption) (*sfuv1.ConnectTransportResponse, error) {
	if m.ConnectTransportFunc != nil {
		return m.ConnectTransportFunc(ctx, in, opts...)
	}
	return nil, errors.New("not implemented")
}

func (m *mockSfuMediaEngineServiceClient) RestartIce(ctx context.Context, in *sfuv1.RestartIceRequest, opts ...grpc.CallOption) (*sfuv1.RestartIceResponse, error) {
	if m.RestartIceFunc != nil {
		return m.RestartIceFunc(ctx, in, opts...)
	}
	return nil, errors.New("not implemented")
}

func (m *mockSfuMediaEngineServiceClient) Produce(ctx context.Context, in *sfuv1.ProduceRequest, opts ...grpc.CallOption) (*sfuv1.ProduceResponse, error) {
	if m.ProduceFunc != nil {
		return m.ProduceFunc(ctx, in, opts...)
	}
	return nil, errors.New("not implemented")
}

func (m *mockSfuMediaEngineServiceClient) Consume(ctx context.Context, in *sfuv1.ConsumeRequest, opts ...grpc.CallOption) (*sfuv1.ConsumeResponse, error) {
	if m.ConsumeFunc != nil {
		return m.ConsumeFunc(ctx, in, opts...)
	}
	return nil, errors.New("not implemented")
}

func (m *mockSfuMediaEngineServiceClient) SetMediaPaused(ctx context.Context, in *sfuv1.SetMediaPausedRequest, opts ...grpc.CallOption) (*sfuv1.SetMediaPausedResponse, error) {
	if m.SetMediaPausedFunc != nil {
		return m.SetMediaPausedFunc(ctx, in, opts...)
	}
	return nil, errors.New("not implemented")
}

func (m *mockSfuMediaEngineServiceClient) SetConsumerPreferredLayers(ctx context.Context, in *sfuv1.SetConsumerPreferredLayersRequest, opts ...grpc.CallOption) (*sfuv1.SetConsumerPreferredLayersResponse, error) {
	if m.SetConsumerPreferredLayersFunc != nil {
		return m.SetConsumerPreferredLayersFunc(ctx, in, opts...)
	}
	return nil, errors.New("not implemented")
}

func (m *mockSfuMediaEngineServiceClient) CloseResource(ctx context.Context, in *sfuv1.CloseResourceRequest, opts ...grpc.CallOption) (*sfuv1.CloseResourceResponse, error) {
	if m.CloseResourceFunc != nil {
		return m.CloseResourceFunc(ctx, in, opts...)
	}
	return nil, errors.New("not implemented")
}

func TestMediaNodeClient_CreateRouter_Success(t *testing.T) {
	mockClient := &mockSfuMediaEngineServiceClient{
		CreateRouterFunc: func(ctx context.Context, in *sfuv1.CreateRouterRequest, opts ...grpc.CallOption) (*sfuv1.CreateRouterResponse, error) {
			if in.RoomId != "room-123" {
				t.Fatalf("expected room-123, got %s", in.RoomId)
			}
			if in.RequestId == "" {
				t.Fatalf("expected non-empty request_id")
			}
			deadline, ok := ctx.Deadline()
			if !ok || time.Until(deadline) <= 0 {
				t.Fatalf("expected context to have a positive deadline")
			}
			return &sfuv1.CreateRouterResponse{
				RouterId:                 "router-xyz",
				WorkerIndex:              1,
				RouterRtpCapabilitiesJson: `{"codecs":[]}`,
			}, nil
		},
	}

	adapter := media_node_grpc.NewMediaNodeClient(mockClient, 2*time.Second)
	info, err := adapter.CreateRouter(context.Background(), "room-123")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if info.RouterID != "router-xyz" || info.WorkerIndex != 1 {
		t.Fatalf("unexpected info: %+v", info)
	}
}

func TestMediaNodeClient_CreateTransport_Success(t *testing.T) {
	mockClient := &mockSfuMediaEngineServiceClient{
		CreateTransportFunc: func(ctx context.Context, in *sfuv1.CreateTransportRequest, opts ...grpc.CallOption) (*sfuv1.CreateTransportResponse, error) {
			if in.RouterId != "r-1" || in.PeerId != "p-1" || !in.IsPublisher {
				t.Fatalf("unexpected request params: %+v", in)
			}
			if in.RequestId != "custom-req-id" {
				t.Fatalf("expected custom-req-id, got %s", in.RequestId)
			}
			return &sfuv1.CreateTransportResponse{
				TransportId: "t-1",
				IceUfrag:    "ufrag-123",
				IcePwd:      "pwd-123",
				IceCandidates: []*sfuv1.IceCandidateInfo{
					{
						Foundation:       "1",
						Priority:         2130706431,
						Ip:               "192.168.1.100",
						Protocol:         "udp",
						Port:             50000,
						Type:             "host",
						SdpCandidateLine: "candidate:1 1 udp 2130706431 192.168.1.100 50000 typ host",
					},
				},
				DtlsFingerprints: []*sfuv1.DtlsFingerprintInfo{
					{
						Algorithm: "sha-256",
						Value:     "AA:BB:CC",
					},
				},
			}, nil
		},
	}

	adapter := media_node_grpc.NewMediaNodeClient(mockClient, 2*time.Second)
	info, err := adapter.CreateTransport(context.Background(), &domain.CreateTransportCommand{
		RequestID:   "custom-req-id",
		RouterID:    "r-1",
		PeerID:      "p-1",
		IsPublisher: true,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if info.TransportID != "t-1" || info.IceUfrag != "ufrag-123" {
		t.Fatalf("unexpected transport info: %+v", info)
	}
	if len(info.IceCandidates) != 1 || len(info.DtlsFingerprints) != 1 {
		t.Fatalf("expected 1 candidate and 1 fingerprint, got %d, %d", len(info.IceCandidates), len(info.DtlsFingerprints))
	}
}

func TestMediaNodeClient_Produce_And_Consume(t *testing.T) {
	mockClient := &mockSfuMediaEngineServiceClient{
		ProduceFunc: func(ctx context.Context, in *sfuv1.ProduceRequest, opts ...grpc.CallOption) (*sfuv1.ProduceResponse, error) {
			return &sfuv1.ProduceResponse{
				ProducerId: "prod-1",
				Kind:       "audio",
			}, nil
		},
		ConsumeFunc: func(ctx context.Context, in *sfuv1.ConsumeRequest, opts ...grpc.CallOption) (*sfuv1.ConsumeResponse, error) {
			return &sfuv1.ConsumeResponse{
				ConsumerId:    "cons-1",
				ProducerId:    in.ProducerId,
				Kind:          "audio",
				Ssrc:          12345,
				PayloadType:   111,
				CodecMimeType: "audio/opus",
				ClockRate:     48000,
			}, nil
		},
	}

	adapter := media_node_grpc.NewMediaNodeClient(mockClient, 2*time.Second)
	prod, err := adapter.Produce(context.Background(), &domain.ProduceCommand{
		RouterID:    "r-1",
		TransportID: "t-1",
		PeerID:      "p-1",
		Kind:        domain.TrackKindAudio,
	})
	if err != nil {
		t.Fatalf("Produce failed: %v", err)
	}
	if prod.ProducerID != "prod-1" || prod.Kind != domain.TrackKindAudio {
		t.Fatalf("unexpected produce result: %+v", prod)
	}

	cons, err := adapter.Consume(context.Background(), &domain.ConsumeCommand{
		RouterID:              "r-1",
		SubscriberTransportID: "t-2",
		ProducerID:            prod.ProducerID,
	})
	if err != nil {
		t.Fatalf("Consume failed: %v", err)
	}
	if cons.ConsumerID != "cons-1" || cons.SSRC != 12345 {
		t.Fatalf("unexpected consume result: %+v", cons)
	}
}

func TestMediaNodeClient_ConnectTransport_IdempotentAlreadyCalled(t *testing.T) {
	mockClient := &mockSfuMediaEngineServiceClient{
		ConnectTransportFunc: func(ctx context.Context, in *sfuv1.ConnectTransportRequest, opts ...grpc.CallOption) (*sfuv1.ConnectTransportResponse, error) {
			// Simulate mediasoup returning already called error
			return nil, errors.New("rpc error: code = Internal desc = Mediasoup error: Failed to connect transport: Received response error: connect() already called")
		},
	}

	adapter := media_node_grpc.NewMediaNodeClient(mockClient, 2*time.Second)
	err := adapter.ConnectTransport(context.Background(), &domain.ConnectTransportCommand{
		RouterID:    "r-1",
		TransportID: "t-1",
		DtlsRole:    "client",
		RemoteFingerprints: []domain.DtlsFingerprintInfo{
			{Algorithm: "sha-256", Value: "AA:BB:CC"},
		},
	})

	if err != nil {
		t.Fatalf("expected connect() already called to be handled as idempotent success, got error: %v", err)
	}
}

