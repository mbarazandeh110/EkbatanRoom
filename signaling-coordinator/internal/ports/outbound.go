package ports

import (
	"context"

	"ekbatan-room/signaling-coordinator/internal/domain"
)

// RouterEnginePort defines the outbound operations for media router lifecycle.
type RouterEnginePort interface {
	CreateRouter(ctx context.Context, roomID string) (*domain.RouterInfo, error)
	CloseRouter(ctx context.Context, routerID string) error
}

// TransportEnginePort defines the outbound operations for WebRTC transport lifecycle.
type TransportEnginePort interface {
	CreateTransport(ctx context.Context, cmd *domain.CreateTransportCommand) (*domain.TransportInfo, error)
	ConnectTransport(ctx context.Context, cmd *domain.ConnectTransportCommand) error
	RestartIce(ctx context.Context, routerID, transportID string) (*domain.IceInfo, error)
	CloseTransport(ctx context.Context, routerID, transportID string) error
}

// MediaStreamPort defines the outbound operations for media streams (producers & consumers).
type MediaStreamPort interface {
	Produce(ctx context.Context, cmd *domain.ProduceCommand) (*domain.ProduceResult, error)
	Consume(ctx context.Context, cmd *domain.ConsumeCommand) (*domain.ConsumeResult, error)
	SetMediaPaused(ctx context.Context, cmd *domain.SetMediaPausedCommand) error
	SetConsumerPreferredLayers(ctx context.Context, cmd *domain.SetPreferredLayersCommand) error
	CloseResource(ctx context.Context, routerID string, resourceType domain.ResourceType, resourceID string) error
}

// SFUMediaPort is an alias for MediaStreamPort for backward and cross-service architectural consistency.
type SFUMediaPort = MediaStreamPort

// NodeLifecyclePort defines the outbound operations for media node monitoring and licensing.
type NodeLifecyclePort interface {
	ApplyLicense(ctx context.Context, licenseToken string) (*domain.ApplyLicenseResult, error)
	GetNodeCapacity(ctx context.Context) (*domain.NodeCapacityInfo, error)
	WatchNodeEvents(ctx context.Context, coordinatorID, licenseToken string) (<-chan *domain.NodeEvent, error)
}

// PipeTransportPort defines the enterprise inter-node PipeTransport cascading operations.
type PipeTransportPort interface {
	CreatePipeTransport(ctx context.Context, cmd *domain.CreatePipeTransportCommand) (*domain.PipeTransportInfo, error)
	ConnectPipeTransport(ctx context.Context, cmd *domain.ConnectPipeTransportCommand) error
	PipeProduce(ctx context.Context, cmd *domain.PipeProduceCommand) (*domain.ProduceResult, error)
	PipeConsume(ctx context.Context, cmd *domain.PipeConsumeCommand) (*domain.ConsumeResult, error)
}

// RoomPlacementStrategy defines the Open-Core strategy port for SFU node placement and cascading.
type RoomPlacementStrategy interface {
	SelectNodeForNewRoom(ctx context.Context, roomID string) (string, error)
	ResolveNodeForPeerJoin(ctx context.Context, roomID string, peerID string) (string, string, error)
}

// TelemetryMetricsPort decouples application logic from concrete Prometheus/VictoriaMetrics adapters.
type TelemetryMetricsPort interface {
	IncActiveRooms()
	DecActiveRooms()
	IncGlareQueued()
}
