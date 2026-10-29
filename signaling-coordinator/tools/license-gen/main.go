package main

import (
	"crypto/ed25519"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"strings"
	"time"
)

type ClusterLimits struct {
	MaxSfuNodes         int `json:"max_sfu_nodes"`
	MaxConcurrentPeers int `json:"max_concurrent_peers"`
	MaxConcurrentRooms int `json:"max_concurrent_rooms"`
}

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

func main() {
	genKeys := flag.Bool("gen-keys", false, "Generate new Ed25519 keypair")
	privHex := flag.String("priv-key", "02dc0bfacd35d5f44baada453877d98d2dfadcfa175d67f211195a6eba102eb4", "Private key in hex")
	customer := flag.String("customer", "enterprise-client", "Customer identifier")
	tier := flag.String("tier", "enterprise", "License tier (community/enterprise)")
	days := flag.Int("days", 365, "Validity duration in days (0 for perpetual)")
	graceDays := flag.Int("grace-days", 14, "Grace period in days after expiration")
	maxNodes := flag.Int("max-nodes", 10, "Maximum allowed SFU media nodes")
	maxPeers := flag.Int("max-peers", 5000, "Maximum concurrent peers")
	maxRooms := flag.Int("max-rooms", 200, "Maximum concurrent rooms")
	features := flag.String("features", "inter_node_cascading,distributed_recording", "Comma-separated feature flags")
	flag.Parse()

	if *genKeys {
		pub, priv, err := ed25519.GenerateKey(nil)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error generating keypair: %v\n", err)
			os.Exit(1)
		}
		fmt.Printf("PUBLIC_KEY_HEX:  %s\n", hex.EncodeToString(pub))
		fmt.Printf("PRIVATE_KEY_HEX: %s\n", hex.EncodeToString(priv.Seed()))
		return
	}

	privSeed, err := hex.DecodeString(*privHex)
	if err != nil || len(privSeed) != 32 {
		fmt.Fprintf(os.Stderr, "Invalid private key seed: must be 32-byte hex\n")
		os.Exit(1)
	}
	privKey := ed25519.NewKeyFromSeed(privSeed)

	now := time.Now().Unix()
	var expiresAt int64 = 0
	if *days > 0 {
		expiresAt = now + int64(*days*86400)
	}

	var featureList []string
	for _, f := range strings.Split(*features, ",") {
		trimmed := strings.TrimSpace(f)
		if trimmed != "" {
			featureList = append(featureList, trimmed)
		}
	}

	claims := LicenseClaims{
		LicenseID:       fmt.Sprintf("lic-%s-%d", *customer, now),
		CustomerID:      *customer,
		Tier:            *tier,
		Features:        featureList,
		ClusterLimits: ClusterLimits{
			MaxSfuNodes:         *maxNodes,
			MaxConcurrentPeers: *maxPeers,
			MaxConcurrentRooms: *maxRooms,
		},
		AllowedDomains:  []string{"*"},
		IssuedAt:        now,
		ExpiresAt:       expiresAt,
		GracePeriodDays: *graceDays,
	}

	claimsJSON, err := json.Marshal(claims)
	if err != nil {
		fmt.Fprintf(os.Stderr, "JSON marshal error: %v\n", err)
		os.Exit(1)
	}

	payloadB64 := base64.RawURLEncoding.EncodeToString(claimsJSON)
	signedData := fmt.Sprintf("EKB1.%s", payloadB64)
	signature := ed25519.Sign(privKey, []byte(signedData))
	sigB64 := base64.RawURLEncoding.EncodeToString(signature)

	token := fmt.Sprintf("EKB1.%s.%s", payloadB64, sigB64)

	fmt.Printf("\n--- EkbatanRoom License Token Generated ---\n")
	fmt.Printf("Customer:   %s\n", claims.CustomerID)
	fmt.Printf("Tier:       %s\n", claims.Tier)
	fmt.Printf("Expires At: %s (Grace: %d days)\n", time.Unix(expiresAt, 0).Format(time.RFC3339), *graceDays)
	fmt.Printf("Features:   %v\n", claims.Features)
	fmt.Printf("Limits:     Nodes=%d, Peers=%d, Rooms=%d\n", *maxNodes, *maxPeers, *maxRooms)
	fmt.Printf("\nTOKEN:\n%s\n\n", token)
}
