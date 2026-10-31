package domain

import "errors"

var (
	// ErrRoomNotFound indicates the requested room does not exist.
	ErrRoomNotFound = errors.New("room not found")

	// ErrRoomClosed indicates the room has been closed and cannot accept new actions.
	ErrRoomClosed = errors.New("room is closed")

	// ErrParticipantNotFound indicates the participant does not exist in the room.
	ErrParticipantNotFound = errors.New("participant not found")

	// ErrParticipantAlreadyExists indicates a participant with this identity already exists.
	ErrParticipantAlreadyExists = errors.New("participant already exists in room")

	// ErrInvalidNegotiationState indicates an SDP renegotiation was attempted in an invalid state.
	ErrInvalidNegotiationState = errors.New("invalid negotiation state")

	// ErrTransportNotFound indicates the specified WebRTC transport does not exist.
	ErrTransportNotFound = errors.New("transport not found")

	// ErrTrackNotFound indicates the specified track does not exist.
	ErrTrackNotFound = errors.New("track not found")

	// ErrTrackAlreadyPublished indicates the track has already been published.
	ErrTrackAlreadyPublished = errors.New("track already published")

	// ErrNegotiationGlare indicates a renegotiation collision occurred.
	ErrNegotiationGlare = errors.New("negotiation glare: waiting for answer")

	// Licensing and Cluster Limit Errors
	ErrLicenseInvalidToken     = errors.New("invalid license token format")
	ErrLicenseInvalidSignature = errors.New("invalid license signature")
	ErrLicenseExpired          = errors.New("license has expired")
	ErrMaxRoomsExceeded        = errors.New("cluster limit exceeded: maximum concurrent rooms reached")
	ErrMaxPeersExceeded        = errors.New("cluster limit exceeded: maximum concurrent peers reached")
	ErrMaxNodesExceeded        = errors.New("cluster limit exceeded: maximum SFU nodes reached")
	ErrNoNodesAvailable        = errors.New("no SFU media nodes available in cluster")
)
