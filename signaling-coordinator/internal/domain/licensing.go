package domain

type LicenseTier string

const (
	LicenseTierCommunity  LicenseTier = "community"
	LicenseTierEnterprise LicenseTier = "enterprise"
)

// ClusterLimits defines resource limits authorized by a license.
type ClusterLimits struct {
	MaxSfuNodes        int `json:"max_sfu_nodes"`
	MaxConcurrentPeers int `json:"max_concurrent_peers"`
	MaxConcurrentRooms int `json:"max_concurrent_rooms"`
}

// LicenseClaims represents the cryptographically signed claims payload.
type LicenseClaims struct {
	LicenseID       string        `json:"license_id"`
	CustomerID      string        `json:"customer_id"`
	Tier            string        `json:"tier"` // "enterprise" | "community"
	Features        []string      `json:"features"`
	ClusterLimits   ClusterLimits `json:"cluster_limits"`
	AllowedDomains  []string      `json:"allowed_domains"`
	IssuedAt        int64         `json:"issued_at"`
	ExpiresAt       int64         `json:"expires_at"` // 0 = Perpetual
	GracePeriodDays int           `json:"grace_period_days"`
}
