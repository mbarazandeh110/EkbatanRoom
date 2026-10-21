package ports

import (
	"context"

	"ekbatan-room/signaling-coordinator/internal/domain"
)

// RoomCoordinatorPort defines the inbound use cases for managing rooms and participants.
type RoomCoordinatorPort interface {
	// Join handles participant admission, router/transport provisioning and initial state synchronization.
	Join(ctx context.Context, roomID, participantID, participantName string, responder PeerResponderPort) (*domain.JoinResult, error)

	// Leave handles participant departure, tearing down transports and notifying peers.
	Leave(ctx context.Context, roomID, participantID string) error

	// PublishTracks processes published tracks, connects publisher transport, calls Produce on engine, and broadcasts to peers.
	PublishTracks(ctx context.Context, roomID, participantID string, dtlsRole string, fingerprints []domain.DtlsFingerprintInfo, tracks []domain.TrackPublishCommand) (*domain.TransportInfo, error)

	// HandleSubscriberAnswer processes client answer, connects subscriber transport, and finishes offer transition.
	HandleSubscriberAnswer(ctx context.Context, roomID, participantID string, dtlsRole string, fingerprints []domain.DtlsFingerprintInfo) error

	// HandleTrickle processes incoming ICE candidate from the client.
	HandleTrickle(ctx context.Context, roomID, participantID string, candidateJSON string, target int) error

	// SetTrackMuted toggles pause/mute state on a published track.
	SetTrackMuted(ctx context.Context, roomID, participantID, trackSID string, muted bool) error

	// SetConsumerLayers sets preferred spatial and temporal layers for a subscriber.
	SetConsumerLayers(ctx context.Context, roomID, participantID, consumerID string, spatial, temporal int32) error

	// RestartPublisherIce requests ICE restart on the publisher transport.
	RestartPublisherIce(ctx context.Context, roomID, participantID string) (*domain.IceInfo, error)

	// RestartSubscriberIce requests ICE restart on the subscriber transport.
	RestartSubscriberIce(ctx context.Context, roomID, participantID string) (*domain.IceInfo, error)
}

// PeerResponderPort defines the interface for sending signaling messages back to a connected peer.
// This is implemented by protocol-specific adapters (e.g. LiveKitAdapter, NativeOrtcAdapter).
type PeerResponderPort interface {
	SendJoinResponse(res *domain.JoinResult) error
	SendSubscriberOffer(slots []*domain.SDPSlot, participantSID string) error
	SendTrickle(candidate string, target int) error
	SendParticipantUpdate(participants []*domain.Participant) error
	SendTrackMuted(trackSID string, muted bool) error
	Close() error
}
