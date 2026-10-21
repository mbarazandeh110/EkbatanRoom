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
