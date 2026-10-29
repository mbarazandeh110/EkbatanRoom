package application_test

import (
	"crypto/ed25519"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"testing"
	"time"

	"ekbatan-room/signaling-coordinator/internal/application"
	"ekbatan-room/signaling-coordinator/internal/domain"
)

const testPrivKeySeed = "02dc0bfacd35d5f44baada453877d98d2dfadcfa175d67f211195a6eba102eb4"

func createTestToken(claims domain.LicenseClaims) string {
	privSeed, _ := hex.DecodeString(testPrivKeySeed)
	privKey := ed25519.NewKeyFromSeed(privSeed)

	claimsJSON, _ := json.Marshal(claims)
	payloadB64 := base64.RawURLEncoding.EncodeToString(claimsJSON)
	signedData := fmt.Sprintf("EKB1.%s", payloadB64)
	signature := ed25519.Sign(privKey, []byte(signedData))
	sigB64 := base64.RawURLEncoding.EncodeToString(signature)

	return fmt.Sprintf("EKB1.%s.%s", payloadB64, sigB64)
}

func TestLicenseManager_CommunityDefaults(t *testing.T) {
	mgr, err := application.NewLicenseManager("")
	if err != nil {
		t.Fatalf("expected NewLicenseManager to succeed with empty token, got: %v", err)
	}

	if mgr.GetTier() != domain.LicenseTierCommunity {
		t.Fatalf("expected community tier, got: %s", mgr.GetTier())
	}
	if mgr.GetToken() != "" {
		t.Fatalf("expected empty token for community, got: %s", mgr.GetToken())
	}

	limits := mgr.GetLimits()
	if limits.MaxSfuNodes != 1 || limits.MaxConcurrentPeers != 100 || limits.MaxConcurrentRooms != 10 {
		t.Fatalf("unexpected community limits: %+v", limits)
	}

	if err := mgr.CheckCanCreateRoom(9); err != nil {
		t.Fatalf("expected can create room under limit, got: %v", err)
	}
	if err := mgr.CheckCanCreateRoom(10); !errors.Is(err, domain.ErrMaxRoomsExceeded) {
		t.Fatalf("expected ErrMaxRoomsExceeded at limit, got: %v", err)
	}

	if err := mgr.CheckCanJoinPeer(99); err != nil {
		t.Fatalf("expected can join peer under limit, got: %v", err)
	}
	if err := mgr.CheckCanJoinPeer(100); !errors.Is(err, domain.ErrMaxPeersExceeded) {
		t.Fatalf("expected ErrMaxPeersExceeded at limit, got: %v", err)
	}

	if err := mgr.CheckCanAddNode(0); err != nil {
		t.Fatalf("expected can add node under limit, got: %v", err)
	}
	if err := mgr.CheckCanAddNode(1); !errors.Is(err, domain.ErrMaxNodesExceeded) {
		t.Fatalf("expected ErrMaxNodesExceeded at limit, got: %v", err)
	}
}

func TestLicenseManager_EnterpriseValidToken(t *testing.T) {
	now := time.Now().Unix()
	claims := domain.LicenseClaims{
		LicenseID:  "lic-enterprise-test-1",
		CustomerID: "acme-corp",
		Tier:       "enterprise",
		Features:   []string{"inter_node_cascading", "distributed_recording"},
		ClusterLimits: domain.ClusterLimits{
			MaxSfuNodes:        15,
			MaxConcurrentPeers: 10000,
			MaxConcurrentRooms: 500,
		},
		AllowedDomains:  []string{"*"},
		IssuedAt:        now,
		ExpiresAt:       now + 86400*30, // 30 days
		GracePeriodDays: 7,
	}

	token := createTestToken(claims)
	mgr, err := application.NewLicenseManager(token)
	if err != nil {
		t.Fatalf("expected valid token to load, got: %v", err)
	}

	if mgr.GetTier() != domain.LicenseTierEnterprise {
		t.Fatalf("expected enterprise tier, got: %s", mgr.GetTier())
	}
	if mgr.GetToken() != token {
		t.Fatalf("expected token to match")
	}
	if !mgr.HasFeature("inter_node_cascading") {
		t.Fatalf("expected feature inter_node_cascading to be present")
	}
	if mgr.HasFeature("unknown_feature") {
		t.Fatalf("expected unknown feature to be absent")
	}

	limits := mgr.GetLimits()
	if limits.MaxSfuNodes != 15 || limits.MaxConcurrentPeers != 10000 || limits.MaxConcurrentRooms != 500 {
		t.Fatalf("unexpected enterprise limits: %+v", limits)
	}

	if err := mgr.CheckCanCreateRoom(499); err != nil {
		t.Fatalf("expected can create room, got: %v", err)
	}
	if err := mgr.CheckCanCreateRoom(500); !errors.Is(err, domain.ErrMaxRoomsExceeded) {
		t.Fatalf("expected ErrMaxRoomsExceeded, got: %v", err)
	}
}

func TestLicenseManager_InvalidSignature(t *testing.T) {
	now := time.Now().Unix()
	claims := domain.LicenseClaims{
		LicenseID:  "lic-invalid-sig",
		CustomerID: "attacker",
		Tier:       "enterprise",
		ExpiresAt:  now + 86400,
	}

	token := createTestToken(claims)
	// Tamper token payload
	tamperedToken := token[:len(token)-5] + "AAAAA"

	_, err := application.NewLicenseManager(tamperedToken)
	if !errors.Is(err, domain.ErrLicenseInvalidSignature) && !errors.Is(err, domain.ErrLicenseInvalidToken) {
		t.Fatalf("expected ErrLicenseInvalidSignature or ErrLicenseInvalidToken, got: %v", err)
	}
}

func TestLicenseManager_InvalidTokenFormat(t *testing.T) {
	tests := []string{
		"not-a-token",
		"EKB1.onlyonepart",
		"EKB2.invalid.prefix",
	}

	for _, tt := range tests {
		_, err := application.NewLicenseManager(tt)
		if !errors.Is(err, domain.ErrLicenseInvalidToken) {
			t.Fatalf("expected ErrLicenseInvalidToken for %q, got: %v", tt, err)
		}
	}
}

func TestLicenseManager_ExpiredAndGracePeriod(t *testing.T) {
	baseTime := time.Date(2026, 10, 1, 12, 0, 0, 0, time.UTC)
	expiresAt := baseTime.Add(10 * 86400 * time.Second).Unix() // Expires Oct 11

	claims := domain.LicenseClaims{
		LicenseID:       "lic-grace-test",
		CustomerID:      "test-customer",
		Tier:            "enterprise",
		ClusterLimits:   domain.ClusterLimits{MaxConcurrentRooms: 50},
		ExpiresAt:       expiresAt,
		GracePeriodDays: 5, // Grace period lasts until Oct 16
	}
	token := createTestToken(claims)

	mgr, err := application.NewLicenseManager("")
	if err != nil {
		t.Fatalf("failed to create manager: %v", err)
	}

	// 1. Before expiration (Oct 5) -> Valid
	mgr.SetTimeFunc(func() time.Time {
		return baseTime.Add(5 * 86400 * time.Second)
	})
	if err := mgr.LoadToken(token); err != nil {
		t.Fatalf("expected load to succeed before expiration: %v", err)
	}
	if mgr.Status() != application.LicenseStatusValid || mgr.InGracePeriod() {
		t.Fatalf("expected valid before expiration")
	}

	// 2. In grace period (Oct 13: 2 days after expiration, within 5 days grace)
	mgr.SetTimeFunc(func() time.Time {
		return baseTime.Add(12 * 86400 * time.Second)
	})
	err = mgr.LoadToken(token)
	if err != nil {
		t.Fatalf("expected load to succeed during grace period, got: %v", err)
	}
	if !mgr.InGracePeriod() {
		t.Fatalf("expected InGracePeriod to be true")
	}
	// Operations still allowed during grace period
	if err := mgr.CheckCanCreateRoom(10); err != nil {
		t.Fatalf("expected room creation allowed in grace period, got: %v", err)
	}

	// 3. Past grace period (Oct 20: 9 days after expiration) -> Expired
	mgr.SetTimeFunc(func() time.Time {
		return baseTime.Add(19 * 86400 * time.Second)
	})
	err = mgr.LoadToken(token)
	if !errors.Is(err, domain.ErrLicenseExpired) {
		t.Fatalf("expected ErrLicenseExpired after grace period, got: %v", err)
	}
	if err := mgr.CheckCanCreateRoom(0); !errors.Is(err, domain.ErrLicenseExpired) {
		t.Fatalf("expected room creation blocked when expired, got: %v", err)
	}
}

func TestLicenseManager_PerpetualToken(t *testing.T) {
	claims := domain.LicenseClaims{
		LicenseID:     "lic-perpetual",
		CustomerID:    "on-prem-client",
		Tier:          "enterprise",
		ClusterLimits: domain.ClusterLimits{MaxConcurrentRooms: 100},
		ExpiresAt:     0, // Perpetual
	}
	token := createTestToken(claims)

	mgr, err := application.NewLicenseManager(token)
	if err != nil {
		t.Fatalf("failed to load perpetual token: %v", err)
	}
	if mgr.Status() != application.LicenseStatusValid {
		t.Fatalf("expected perpetual token to be valid")
	}
}
