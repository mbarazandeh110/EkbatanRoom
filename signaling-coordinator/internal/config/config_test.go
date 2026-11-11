package config_test

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"ekbatan-room/signaling-coordinator/internal/config"
)

func TestConfig_DefaultConfigValid(t *testing.T) {
	cfg := config.DefaultConfig()
	if err := cfg.Validate(); err != nil {
		t.Fatalf("expected DefaultConfig to be valid, got: %v", err)
	}

	if cfg.LiveKit.PingInterval != 10 {
		t.Fatalf("expected default ping interval 10, got %d", cfg.LiveKit.PingInterval)
	}
	if cfg.LiveKit.PingTimeout != 20 {
		t.Fatalf("expected default ping timeout 20, got %d", cfg.LiveKit.PingTimeout)
	}
	if cfg.LiveKit.PingTimeout <= cfg.LiveKit.PingInterval {
		t.Fatalf("PingTimeout must exceed PingInterval per REQ-007")
	}
}

func TestConfig_ValidatePingTimeoutRequirement(t *testing.T) {
	cfg := config.DefaultConfig()
	cfg.LiveKit.PingInterval = 20
	cfg.LiveKit.PingTimeout = 10

	err := cfg.Validate()
	if err == nil {
		t.Fatalf("expected validation failure when PingTimeout <= PingInterval")
	}
}

func TestConfig_LoadFromYAMLFile(t *testing.T) {
	yamlContent := `
server:
  port: "9000"
  read_timeout: 30s
  write_timeout: 30s
  shutdown_timeout: 10s

sfu:
  grpc_addr: "10.0.0.5:50051"
  request_timeout: 8s

livekit:
  ping_interval: 15
  ping_timeout: 45
  server_version: "2.0.0"

rtc:
  ice_servers:
    - urls:
        - "stun:stun1.example.com"
        - "stun:stun2.example.com"
      username: "user"
      credential: "pwd"

telemetry:
  log_level: "debug"
  log_format: "text"
  log_raw_sdp: true
  tracing_enabled: true
  trace_sample_rate: 0.5
  otlp_endpoint: "collector:4317"
  metrics_enabled: false
`
	tmpDir := t.TempDir()
	filePath := filepath.Join(tmpDir, "config.yaml")
	if err := os.WriteFile(filePath, []byte(yamlContent), 0644); err != nil {
		t.Fatalf("failed to write temp config file: %v", err)
	}

	cfg, err := config.Load(filePath)
	if err != nil {
		t.Fatalf("failed to load config from yaml: %v", err)
	}

	if cfg.Server.Port != "9000" {
		t.Fatalf("expected port 9000, got %s", cfg.Server.Port)
	}
	if cfg.Server.ReadTimeout != 30*time.Second {
		t.Fatalf("expected ReadTimeout 30s, got %v", cfg.Server.ReadTimeout)
	}
	if cfg.SFU.GRPCAddr != "10.0.0.5:50051" {
		t.Fatalf("expected GRPCAddr 10.0.0.5:50051, got %s", cfg.SFU.GRPCAddr)
	}
	if cfg.LiveKit.PingInterval != 15 || cfg.LiveKit.PingTimeout != 45 {
		t.Fatalf("unexpected livekit ping config: %+v", cfg.LiveKit)
	}
	if len(cfg.RTC.IceServers) != 1 || len(cfg.RTC.IceServers[0].URLs) != 2 {
		t.Fatalf("unexpected ICE servers: %+v", cfg.RTC.IceServers)
	}
	if cfg.Telemetry.LogLevel != "debug" || cfg.Telemetry.LogFormat != "text" || !cfg.Telemetry.LogRawSDP {
		t.Fatalf("unexpected telemetry config: %+v", cfg.Telemetry)
	}
}

func TestConfig_EnvOverrides(t *testing.T) {
	t.Setenv("PORT", "8888")
	t.Setenv("SFU_GRPC_ADDR", "sfu.internal:50051")
	t.Setenv("LIVEKIT_PING_INTERVAL", "5")
	t.Setenv("LIVEKIT_PING_TIMEOUT", "15")
	t.Setenv("LIVEKIT_SERVER_VERSION", "1.99.0")
	t.Setenv("RTC_ICE_SERVERS", "stun:custom1.org,stun:custom2.org")
	t.Setenv("OBS_LOG_LEVEL", "warn")
	t.Setenv("OBS_LOG_RAW_SDP", "true")

	cfg, err := config.Load("")
	if err != nil {
		t.Fatalf("failed to load config with env: %v", err)
	}

	if cfg.Server.Port != "8888" {
		t.Fatalf("expected port 8888 from env, got %s", cfg.Server.Port)
	}
	if cfg.SFU.GRPCAddr != "sfu.internal:50051" {
		t.Fatalf("expected sfu addr from env, got %s", cfg.SFU.GRPCAddr)
	}
	if cfg.LiveKit.PingInterval != 5 || cfg.LiveKit.PingTimeout != 15 {
		t.Fatalf("expected ping interval 5 and timeout 15 from env, got %+v", cfg.LiveKit)
	}
	if cfg.LiveKit.ServerVersion != "1.99.0" {
		t.Fatalf("expected server version 1.99.0, got %s", cfg.LiveKit.ServerVersion)
	}
	if len(cfg.RTC.IceServers) != 1 || len(cfg.RTC.IceServers[0].URLs) != 2 {
		t.Fatalf("expected 2 ICE URLs from comma-separated env, got %+v", cfg.RTC.IceServers)
	}
	if cfg.Telemetry.LogLevel != "warn" || !cfg.Telemetry.LogRawSDP {
		t.Fatalf("unexpected telemetry env values: %+v", cfg.Telemetry)
	}
}

func TestConfig_MultiNodeParsing_REQ029(t *testing.T) {
	// 1. Comma-separated addresses without names
	t.Setenv("SFU_GRPC_ADDR", "127.0.0.1:50051,127.0.0.1:50052")
	cfg1, err := config.Load("")
	if err != nil {
		t.Fatalf("failed to load config: %v", err)
	}
	nodes1 := cfg1.SFU.GetNodes()
	if len(nodes1) != 2 {
		t.Fatalf("expected 2 nodes, got %d", len(nodes1))
	}
	if nodes1[0].ID != "sfu-primary" || nodes1[0].GRPCAddr != "127.0.0.1:50051" {
		t.Errorf("unexpected node 0: %+v", nodes1[0])
	}
	if nodes1[1].ID != "sfu-node-2" || nodes1[1].GRPCAddr != "127.0.0.1:50052" {
		t.Errorf("unexpected node 1: %+v", nodes1[1])
	}

	// 2. Comma-separated with explicit node IDs via SFU_NODES
	t.Setenv("SFU_NODES", "node-a=10.0.0.1:50051,node-b=10.0.0.2:50051")
	cfg2, err := config.Load("")
	if err != nil {
		t.Fatalf("failed to load config with SFU_NODES: %v", err)
	}
	nodes2 := cfg2.SFU.GetNodes()
	if len(nodes2) != 2 {
		t.Fatalf("expected 2 nodes from SFU_NODES, got %d", len(nodes2))
	}
	if nodes2[0].ID != "node-a" || nodes2[0].GRPCAddr != "10.0.0.1:50051" {
		t.Errorf("unexpected node-a: %+v", nodes2[0])
	}
	if nodes2[1].ID != "node-b" || nodes2[1].GRPCAddr != "10.0.0.2:50051" {
		t.Errorf("unexpected node-b: %+v", nodes2[1])
	}
}
