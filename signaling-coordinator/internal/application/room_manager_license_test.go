package application_test

import (
	"context"
	"errors"
	"testing"
	"time"

	"ekbatan-room/signaling-coordinator/internal/application"
	"ekbatan-room/signaling-coordinator/internal/domain"
)

func TestRoomManager_LicenseMaxRoomsEnforcement(t *testing.T) {
	now := time.Now().Unix()
	claims := domain.LicenseClaims{
		LicenseID:     "lic-max-rooms",
		CustomerID:    "test",
		Tier:          "enterprise",
		ClusterLimits: domain.ClusterLimits{MaxConcurrentRooms: 2, MaxConcurrentPeers: 100},
		IssuedAt:      now,
		ExpiresAt:     now + 86400,
	}
	token := createTestToken(claims)
	licenseMgr, err := application.NewLicenseManager(token)
	if err != nil {
		t.Fatalf("failed to create license manager: %v", err)
	}

	engine := newMockOutboundEngine()
	roomMgr := application.NewRoomManager(
		engine,
		engine,
		engine,
		nil,
		licenseMgr,
	)

	ctx := context.Background()

	// 1. Join Room 1 -> should succeed
	res1, err := roomMgr.Join(ctx, "room-1", "user-1", "User One", newMockResponder())
	if err != nil || res1 == nil {
		t.Fatalf("expected room 1 join to succeed, got: %v", err)
	}

	// 2. Join Room 2 -> should succeed
	res2, err := roomMgr.Join(ctx, "room-2", "user-2", "User Two", newMockResponder())
	if err != nil || res2 == nil {
		t.Fatalf("expected room 2 join to succeed, got: %v", err)
	}

	// 3. Join Room 3 -> should fail with ErrMaxRoomsExceeded
	_, err = roomMgr.Join(ctx, "room-3", "user-3", "User Three", newMockResponder())
	if !errors.Is(err, domain.ErrMaxRoomsExceeded) {
		t.Fatalf("expected ErrMaxRoomsExceeded for room 3, got: %v", err)
	}

	// 4. Joining existing room (room-1) with new participant -> should succeed (room already exists)
	res1b, err := roomMgr.Join(ctx, "room-1", "user-1-b", "User One B", newMockResponder())
	if err != nil || res1b == nil {
		t.Fatalf("expected joining existing room to succeed, got: %v", err)
	}

	// 5. Leave room-2 completely -> room should be cleaned up and slot reclaimed
	err = roomMgr.Leave(ctx, "room-2", "user-2")
	if err != nil {
		t.Fatalf("expected leave room-2 to succeed: %v", err)
	}

	if roomMgr.ActiveRoomsCount() != 1 {
		t.Fatalf("expected active rooms count to be 1 after room-2 became empty, got: %d", roomMgr.ActiveRoomsCount())
	}

	// 6. Now creating room-3 must succeed
	res3, err := roomMgr.Join(ctx, "room-3", "user-3", "User Three", newMockResponder())
	if err != nil || res3 == nil {
		t.Fatalf("expected room-3 join to succeed after room-2 was freed, got: %v", err)
	}
}

func TestRoomManager_LicenseMaxPeersEnforcement(t *testing.T) {
	now := time.Now().Unix()
	claims := domain.LicenseClaims{
		LicenseID:     "lic-max-peers",
		CustomerID:    "test",
		Tier:          "enterprise",
		ClusterLimits: domain.ClusterLimits{MaxConcurrentRooms: 10, MaxConcurrentPeers: 3},
		IssuedAt:      now,
		ExpiresAt:     now + 86400,
	}
	token := createTestToken(claims)
	licenseMgr, err := application.NewLicenseManager(token)
	if err != nil {
		t.Fatalf("failed to create license manager: %v", err)
	}

	engine := newMockOutboundEngine()
	roomMgr := application.NewRoomManager(
		engine,
		engine,
		engine,
		nil,
		licenseMgr,
	)

	ctx := context.Background()

	// Add 3 peers across 2 rooms
	peers := []struct {
		roomID string
		peerID string
	}{
		{"room-1", "user-1"},
		{"room-1", "user-2"},
		{"room-2", "user-3"},
	}

	for _, p := range peers {
		_, err := roomMgr.Join(ctx, p.roomID, p.peerID, p.peerID, newMockResponder())
		if err != nil {
			t.Fatalf("expected peer %s to join room %s, got: %v", p.peerID, p.roomID, err)
		}
	}

	if roomMgr.ActivePeersCount() != 3 {
		t.Fatalf("expected 3 active peers, got: %d", roomMgr.ActivePeersCount())
	}

	// 4th peer must be rejected
	_, err = roomMgr.Join(ctx, "room-1", "user-4", "User Four", newMockResponder())
	if !errors.Is(err, domain.ErrMaxPeersExceeded) {
		t.Fatalf("expected ErrMaxPeersExceeded for 4th peer, got: %v", err)
	}

	// One peer leaves room-1
	if err := roomMgr.Leave(ctx, "room-1", "user-1"); err != nil {
		t.Fatalf("expected leave to succeed: %v", err)
	}
	if roomMgr.ActivePeersCount() != 2 {
		t.Fatalf("expected 2 active peers after leave, got: %d", roomMgr.ActivePeersCount())
	}

	// Now 4th peer can join
	_, err = roomMgr.Join(ctx, "room-1", "user-4", "User Four", newMockResponder())
	if err != nil {
		t.Fatalf("expected user-4 to join after user-1 left, got: %v", err)
	}
}

func TestRoomManager_LicenseExpiredBlocksNewJoins(t *testing.T) {
	baseTime := time.Date(2026, 10, 1, 12, 0, 0, 0, time.UTC)
	expiresAt := baseTime.Add(5 * 86400 * time.Second).Unix()

	claims := domain.LicenseClaims{
		LicenseID:       "lic-expired-blocks",
		CustomerID:      "test",
		Tier:            "enterprise",
		ClusterLimits:   domain.ClusterLimits{MaxConcurrentRooms: 10, MaxConcurrentPeers: 100},
		ExpiresAt:       expiresAt,
		GracePeriodDays: 2,
	}
	token := createTestToken(claims)

	licenseMgr, _ := application.NewLicenseManager("")
	licenseMgr.SetTimeFunc(func() time.Time {
		// 10 days after baseTime -> 3 days after grace period has ended!
		return baseTime.Add(10 * 86400 * time.Second)
	})
	_ = licenseMgr.LoadToken(token)

	engine := newMockOutboundEngine()
	roomMgr := application.NewRoomManager(
		engine,
		engine,
		engine,
		nil,
		licenseMgr,
	)

	_, err := roomMgr.Join(context.Background(), "room-1", "user-1", "User One", newMockResponder())
	if !errors.Is(err, domain.ErrLicenseExpired) {
		t.Fatalf("expected ErrLicenseExpired for expired license, got: %v", err)
	}
}

func TestRoomManager_LicenseMaxParticipantsPerRoomEnforcement(t *testing.T) {
	now := time.Now().Unix()
	claims := domain.LicenseClaims{
		LicenseID:  "lic-max-parts-room",
		CustomerID: "test-parts",
		Tier:       "enterprise",
		ClusterLimits: domain.ClusterLimits{
			MaxConcurrentRooms:     10,
			MaxConcurrentPeers:     100,
			MaxParticipantsPerRoom: 2,
		},
		IssuedAt:  now,
		ExpiresAt: now + 86400,
	}
	token := createTestToken(claims)
	licenseMgr, err := application.NewLicenseManager(token)
	if err != nil {
		t.Fatalf("failed to create license manager: %v", err)
	}

	engine := newMockOutboundEngine()
	roomMgr := application.NewRoomManager(
		engine,
		engine,
		engine,
		nil,
		licenseMgr,
	)

	ctx := context.Background()

	// 1. Join user-1 -> should succeed
	res1, err := roomMgr.Join(ctx, "room-part-limit", "user-1", "User One", newMockResponder())
	if err != nil || res1 == nil {
		t.Fatalf("expected user-1 to join, got: %v", err)
	}

	// 2. Join user-2 -> should succeed (count reaches 2)
	res2, err := roomMgr.Join(ctx, "room-part-limit", "user-2", "User Two", newMockResponder())
	if err != nil || res2 == nil {
		t.Fatalf("expected user-2 to join, got: %v", err)
	}

	// 3. Join user-3 -> should fail with ErrMaxParticipantsPerRoomExceeded
	_, err = roomMgr.Join(ctx, "room-part-limit", "user-3", "User Three", newMockResponder())
	if !errors.Is(err, domain.ErrMaxParticipantsPerRoomExceeded) {
		t.Fatalf("expected ErrMaxParticipantsPerRoomExceeded for user-3, got: %v", err)
	}

	// 4. User-1 leaves room -> room now has 1 participant
	err = roomMgr.Leave(ctx, "room-part-limit", "user-1")
	if err != nil {
		t.Fatalf("expected leave to succeed: %v", err)
	}

	// 5. User-3 now joins successfully
	res3, err := roomMgr.Join(ctx, "room-part-limit", "user-3", "User Three", newMockResponder())
	if err != nil || res3 == nil {
		t.Fatalf("expected user-3 to join after slot freed, got: %v", err)
	}
}

