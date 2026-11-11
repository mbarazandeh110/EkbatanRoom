package application

import (
	"crypto/ed25519"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"strings"
	"sync"
	"time"

	"ekbatan-room/signaling-coordinator/internal/domain"
	"ekbatan-room/signaling-coordinator/internal/ports"
)

// DefaultLicensePublicKeyHex is the official EkbatanRoom Ed25519 master public key.
const DefaultLicensePublicKeyHex = "23a1aa3536a3863f3bd6b468d2537192d9ec9cdcb127486738e145fe64eb5436"

type LicenseStatus = string

const (
	LicenseStatusValid       LicenseStatus = "valid"
	LicenseStatusGracePeriod LicenseStatus = "grace_period"
	LicenseStatusExpired     LicenseStatus = "expired"
)

var _ ports.LicenseVerifierPort = (*LicenseManager)(nil)

// LicenseManager manages Ed25519 license validation and cluster quota enforcement.
type LicenseManager struct {
	mu       sync.RWMutex
	pubKey   ed25519.PublicKey
	token    string
	claims   domain.LicenseClaims
	tier     domain.LicenseTier
	limits   domain.ClusterLimits
	status   LicenseStatus
	timeFunc func() time.Time
}

// NewLicenseManager creates and initializes a LicenseManager with the provided license token and optional public key.
func NewLicenseManager(tokenStr string, pubKeyHex ...string) (*LicenseManager, error) {
	keyHex := DefaultLicensePublicKeyHex
	if len(pubKeyHex) > 0 && strings.TrimSpace(pubKeyHex[0]) != "" {
		keyHex = strings.TrimSpace(pubKeyHex[0])
	}

	pubBytes, err := hex.DecodeString(keyHex)
	if err != nil || len(pubBytes) != ed25519.PublicKeySize {
		return nil, fmt.Errorf("invalid Ed25519 public key hex: %w", err)
	}

	mgr := &LicenseManager{
		pubKey:   ed25519.PublicKey(pubBytes),
		timeFunc: time.Now,
	}

	if err := mgr.LoadToken(tokenStr); err != nil {
		return nil, err
	}
	return mgr, nil
}

// LoadToken verifies and applies a license token, or sets Community defaults if token is empty.
func (m *LicenseManager) LoadToken(tokenStr string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	trimmed := strings.TrimSpace(tokenStr)
	if trimmed == "" || strings.EqualFold(trimmed, "community") {
		// Community Edition defaults
		m.token = ""
		m.tier = domain.LicenseTierCommunity
		m.claims = domain.LicenseClaims{
			LicenseID:  "community",
			CustomerID: "community",
			Tier:       string(domain.LicenseTierCommunity),
			Features:   []string{},
			ClusterLimits: domain.ClusterLimits{
				MaxSfuNodes:        1,
				MaxConcurrentPeers: 100,
				MaxConcurrentRooms: 10,
			},
			AllowedDomains: []string{"*"},
			IssuedAt:       m.now().Unix(),
			ExpiresAt:      0, // Perpetual
		}
		m.limits = m.claims.ClusterLimits
		m.status = LicenseStatusValid
		slog.Info("running in Community Edition",
			"tier", m.tier,
			"max_nodes", m.limits.MaxSfuNodes,
			"max_peers", m.limits.MaxConcurrentPeers,
			"max_rooms", m.limits.MaxConcurrentRooms,
		)
		return nil
	}

	// Enterprise token validation
	claims, status, err := m.verifyTokenLocked(trimmed)
	if err != nil {
		if errors.Is(err, domain.ErrLicenseExpired) {
			m.status = LicenseStatusExpired
			m.token = trimmed
			if claims != nil {
				m.claims = *claims
				m.tier = domain.LicenseTier(claims.Tier)
				m.limits = claims.ClusterLimits
			}
		}
		return err
	}

	m.token = trimmed
	m.claims = *claims
	m.tier = domain.LicenseTier(claims.Tier)
	m.limits = claims.ClusterLimits
	m.status = status

	slog.Info("license verified and loaded",
		"license_id", claims.LicenseID,
		"customer", claims.CustomerID,
		"tier", m.tier,
		"status", m.status,
		"max_nodes", m.limits.MaxSfuNodes,
		"max_peers", m.limits.MaxConcurrentPeers,
		"max_rooms", m.limits.MaxConcurrentRooms,
		"expires_at", claims.ExpiresAt,
	)

	return nil
}

func (m *LicenseManager) verifyTokenLocked(token string) (*domain.LicenseClaims, LicenseStatus, error) {
	if !strings.HasPrefix(token, "EKB1.") {
		return nil, "", domain.ErrLicenseInvalidToken
	}

	parts := strings.Split(token, ".")
	if len(parts) != 3 || parts[1] == "" || parts[2] == "" {
		return nil, "", domain.ErrLicenseInvalidToken
	}

	payloadB64 := parts[1]
	sigB64 := parts[2]

	payloadBytes, err := decodeBase64URL(payloadB64)
	if err != nil {
		return nil, "", fmt.Errorf("%w: invalid payload encoding", domain.ErrLicenseInvalidToken)
	}

	sigBytes, err := decodeBase64URL(sigB64)
	if err != nil || len(sigBytes) != ed25519.SignatureSize {
		return nil, "", domain.ErrLicenseInvalidSignature
	}

	signedData := fmt.Sprintf("EKB1.%s", payloadB64)
	if !ed25519.Verify(m.pubKey, []byte(signedData), sigBytes) {
		return nil, "", domain.ErrLicenseInvalidSignature
	}

	var claims domain.LicenseClaims
	if err := json.Unmarshal(payloadBytes, &claims); err != nil {
		return nil, "", fmt.Errorf("%w: invalid claims JSON: %v", domain.ErrLicenseInvalidToken, err)
	}

	now := m.now().Unix()
	status := LicenseStatusValid

	if claims.ExpiresAt > 0 {
		if now > claims.ExpiresAt {
			graceEnd := claims.ExpiresAt + int64(claims.GracePeriodDays*86400)
			if now > graceEnd {
				return &claims, LicenseStatusExpired, domain.ErrLicenseExpired
			}
			status = LicenseStatusGracePeriod
			slog.Warn("license is operating within grace period",
				"license_id", claims.LicenseID,
				"expired_at", claims.ExpiresAt,
				"grace_end", graceEnd,
				"days_remaining", (graceEnd-now)/86400,
			)
		}
	}

	return &claims, status, nil
}

func decodeBase64URL(s string) ([]byte, error) {
	if b, err := base64.RawURLEncoding.DecodeString(s); err == nil {
		return b, nil
	}
	return base64.URLEncoding.DecodeString(s)
}

func (m *LicenseManager) now() time.Time {
	if m.timeFunc != nil {
		return m.timeFunc()
	}
	return time.Now()
}

// SetTimeFunc overrides the clock for testing time/expiration behaviors.
func (m *LicenseManager) SetTimeFunc(fn func() time.Time) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.timeFunc = fn
}

func (m *LicenseManager) GetToken() string {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.token
}

func (m *LicenseManager) GetTier() domain.LicenseTier {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.tier
}

func (m *LicenseManager) GetLimits() domain.ClusterLimits {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.limits
}

func (m *LicenseManager) GetClaims() domain.LicenseClaims {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.claims
}

func (m *LicenseManager) InGracePeriod() bool {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.status == LicenseStatusGracePeriod
}

func (m *LicenseManager) Status() LicenseStatus {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.status
}

func (m *LicenseManager) HasFeature(feature string) bool {
	m.mu.RLock()
	defer m.mu.RUnlock()
	for _, f := range m.claims.Features {
		if strings.EqualFold(f, feature) {
			return true
		}
	}
	return false
}

func (m *LicenseManager) checkExpiredRLocked() error {
	if m.status == LicenseStatusExpired {
		return domain.ErrLicenseExpired
	}
	if m.claims.ExpiresAt > 0 {
		now := m.now().Unix()
		graceEnd := m.claims.ExpiresAt + int64(m.claims.GracePeriodDays*86400)
		if now > graceEnd {
			return domain.ErrLicenseExpired
		}
	}
	return nil
}

// CheckCanCreateRoom verifies if a new room can be created without exceeding cluster limits.
func (m *LicenseManager) CheckCanCreateRoom(currentRooms int) error {
	m.mu.RLock()
	defer m.mu.RUnlock()

	if err := m.checkExpiredRLocked(); err != nil {
		return err
	}

	if m.limits.MaxConcurrentRooms > 0 && currentRooms >= m.limits.MaxConcurrentRooms {
		slog.Warn("cluster room limit reached",
			"current_rooms", currentRooms,
			"max_allowed", m.limits.MaxConcurrentRooms,
		)
		return domain.ErrMaxRoomsExceeded
	}
	return nil
}

// CheckCanJoinPeer verifies if a new participant can join without exceeding cluster limits.
func (m *LicenseManager) CheckCanJoinPeer(currentPeers int) error {
	m.mu.RLock()
	defer m.mu.RUnlock()

	if err := m.checkExpiredRLocked(); err != nil {
		return err
	}

	if m.limits.MaxConcurrentPeers > 0 && currentPeers >= m.limits.MaxConcurrentPeers {
		slog.Warn("cluster peer limit reached",
			"current_peers", currentPeers,
			"max_allowed", m.limits.MaxConcurrentPeers,
		)
		return domain.ErrMaxPeersExceeded
	}
	return nil
}

// CheckCanAddNode verifies if a new SFU media node can join without exceeding cluster limits.
func (m *LicenseManager) CheckCanAddNode(currentNodes int) error {
	m.mu.RLock()
	defer m.mu.RUnlock()

	if err := m.checkExpiredRLocked(); err != nil {
		return err
	}

	if m.limits.MaxSfuNodes > 0 && currentNodes >= m.limits.MaxSfuNodes {
		slog.Warn("cluster SFU node limit reached",
			"current_nodes", currentNodes,
			"max_allowed", m.limits.MaxSfuNodes,
		)
		return domain.ErrMaxNodesExceeded
	}
	return nil
}

// CheckCanJoinParticipantToRoom checks whether adding another participant to this room exceeds max_participants_per_room.
func (m *LicenseManager) CheckCanJoinParticipantToRoom(currentRoomParticipants int) error {
	m.mu.RLock()
	defer m.mu.RUnlock()

	if err := m.checkExpiredRLocked(); err != nil {
		return err
	}

	if m.limits.MaxParticipantsPerRoom > 0 && currentRoomParticipants >= m.limits.MaxParticipantsPerRoom {
		slog.Warn("room participant limit reached",
			"current_participants", currentRoomParticipants,
			"max_allowed", m.limits.MaxParticipantsPerRoom,
		)
		return domain.ErrMaxParticipantsPerRoomExceeded
	}
	return nil
}

// CheckFeatureAllowed verifies whether a specific feature is permitted by the license.
func (m *LicenseManager) CheckFeatureAllowed(feature string) error {
	m.mu.RLock()
	defer m.mu.RUnlock()

	if err := m.checkExpiredRLocked(); err != nil {
		return err
	}

	if m.tier == domain.LicenseTierCommunity {
		if strings.EqualFold(feature, domain.FeatureCascading) || strings.EqualFold(feature, domain.FeatureRecording) {
			return domain.ErrFeatureNotLicensed
		}
	}

	for _, f := range m.claims.Features {
		if f == "*" || strings.EqualFold(f, feature) {
			return nil
		}
	}

	if len(m.claims.Features) == 0 && m.tier == domain.LicenseTierEnterprise {
		// Enterprise defaults allow cascading & recording unless explicitly restricted
		return nil
	}

	return domain.ErrFeatureNotLicensed
}
