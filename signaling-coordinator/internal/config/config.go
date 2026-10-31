package config

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"strconv"
	"strings"
	"time"

	"gopkg.in/yaml.v3"

	"ekbatan-room/signaling-coordinator/internal/adapters/outbound/telemetry"
	"ekbatan-room/signaling-coordinator/internal/domain"
)

// Config holds all configuration for the signaling-coordinator service.
type Config struct {
	Server    ServerConfig     `json:"server" yaml:"server"`
	SFU       SFUConfig        `json:"sfu" yaml:"sfu"`
	LiveKit   LiveKitConfig    `json:"livekit" yaml:"livekit"`
	RTC       RTCConfig        `json:"rtc" yaml:"rtc"`
	License   LicenseConfig    `json:"license" yaml:"license"`
	Telemetry telemetry.Config `json:"telemetry" yaml:"telemetry"`
}

// LicenseConfig holds licensing keys and public key overrides.
type LicenseConfig struct {
	Key       string `json:"key" yaml:"key"`
	PublicKey string `json:"public_key" yaml:"public_key"`
}

// ServerConfig defines HTTP and WebSocket server parameters.
type ServerConfig struct {
	Port            string        `json:"port" yaml:"port"`
	ReadTimeout     time.Duration `json:"read_timeout" yaml:"read_timeout"`
	WriteTimeout    time.Duration `json:"write_timeout" yaml:"write_timeout"`
	ShutdownTimeout time.Duration `json:"shutdown_timeout" yaml:"shutdown_timeout"`
}

// SFUConfig defines client parameters for communicating with sfu-media-node over gRPC.
type SFUConfig struct {
	GRPCAddr       string        `json:"grpc_addr" yaml:"grpc_addr"`
	RequestTimeout time.Duration `json:"request_timeout" yaml:"request_timeout"`
}

// LiveKitConfig defines protocol parameters for the LiveKit signaling gateway.
type LiveKitConfig struct {
	PingInterval  uint32 `json:"ping_interval" yaml:"ping_interval"`
	PingTimeout   uint32 `json:"ping_timeout" yaml:"ping_timeout"`
	ServerVersion string `json:"server_version" yaml:"server_version"`
}

// RTCConfig defines WebRTC ICE parameters.
type RTCConfig struct {
	IceServers []domain.IceServer `json:"ice_servers" yaml:"ice_servers"`
}

// DefaultConfig provides recommended defaults for all parameters.
func DefaultConfig() Config {
	return Config{
		Server: ServerConfig{
			Port:            "7880",
			ReadTimeout:     15 * time.Second,
			WriteTimeout:    15 * time.Second,
			ShutdownTimeout: 5 * time.Second,
		},
		SFU: SFUConfig{
			GRPCAddr:       "127.0.0.1:50051",
			RequestTimeout: 5 * time.Second,
		},
		LiveKit: LiveKitConfig{
			PingInterval:  10,
			PingTimeout:   20,
			ServerVersion: "1.53.0",
		},
		RTC: RTCConfig{
			IceServers: []domain.IceServer{
				{
					URLs: []string{"stun:stun.l.google.com:19302"},
				},
			},
		},
		License: LicenseConfig{
			Key:       "",
			PublicKey: "",
		},
		Telemetry: telemetry.Config{
			LogLevel:        "info",
			LogFormat:       "json",
			LogRawSDP:       false,
			TracingEnabled:  false,
			TraceSampleRate: 1.0,
			OtlpEndpoint:    "",
			MetricsEnabled:  true,
		},
	}
}

// Load loads configuration from an optional YAML file, overlaid with environment variables.
func Load(filePath string) (*Config, error) {
	cfg := DefaultConfig()

	// 1. Resolve configuration file path
	if filePath == "" {
		filePath = os.Getenv("CONFIG_FILE")
	}

	if filePath != "" {
		data, err := os.ReadFile(filePath)
		if err != nil {
			return nil, fmt.Errorf("failed to read config file %q: %w", filePath, err)
		}
		if err := yaml.Unmarshal(data, &cfg); err != nil {
			return nil, fmt.Errorf("failed to parse yaml config file %q: %w", filePath, err)
		}
	} else if _, err := os.Stat("config.yaml"); err == nil {
		// Auto-discover config.yaml if present in working directory
		data, err := os.ReadFile("config.yaml")
		if err == nil {
			_ = yaml.Unmarshal(data, &cfg)
		}
	}

	// 2. Overlay Environment Variables (Twelve-Factor App Factor III: Config)
	applyEnvOverrides(&cfg)

	// 3. Validate Configuration
	if err := cfg.Validate(); err != nil {
		return nil, fmt.Errorf("config validation failed: %w", err)
	}

	return &cfg, nil
}

func applyEnvOverrides(cfg *Config) {
	// Server
	if v := os.Getenv("PORT"); v != "" {
		cfg.Server.Port = strings.TrimSpace(v)
	} else if v := os.Getenv("CONFIG_SERVER_PORT"); v != "" {
		cfg.Server.Port = strings.TrimSpace(v)
	}
	if v := os.Getenv("SERVER_READ_TIMEOUT"); v != "" {
		if d, err := time.ParseDuration(v); err == nil {
			cfg.Server.ReadTimeout = d
		}
	}
	if v := os.Getenv("SERVER_WRITE_TIMEOUT"); v != "" {
		if d, err := time.ParseDuration(v); err == nil {
			cfg.Server.WriteTimeout = d
		}
	}
	if v := os.Getenv("SERVER_SHUTDOWN_TIMEOUT"); v != "" {
		if d, err := time.ParseDuration(v); err == nil {
			cfg.Server.ShutdownTimeout = d
		}
	}

	// SFU gRPC
	if v := os.Getenv("SFU_GRPC_ADDR"); v != "" {
		cfg.SFU.GRPCAddr = strings.TrimSpace(v)
	}
	if v := os.Getenv("SFU_REQUEST_TIMEOUT"); v != "" {
		if d, err := time.ParseDuration(v); err == nil {
			cfg.SFU.RequestTimeout = d
		}
	}

	// LiveKit Protocol
	if v := os.Getenv("LIVEKIT_PING_INTERVAL"); v != "" {
		if u, err := strconv.ParseUint(strings.TrimSpace(v), 10, 32); err == nil {
			cfg.LiveKit.PingInterval = uint32(u)
		}
	}
	if v := os.Getenv("LIVEKIT_PING_TIMEOUT"); v != "" {
		if u, err := strconv.ParseUint(strings.TrimSpace(v), 10, 32); err == nil {
			cfg.LiveKit.PingTimeout = uint32(u)
		}
	}
	if v := os.Getenv("LIVEKIT_SERVER_VERSION"); v != "" {
		cfg.LiveKit.ServerVersion = strings.TrimSpace(v)
	}

	// WebRTC ICE Servers
	if v := os.Getenv("RTC_ICE_SERVERS"); v != "" {
		trimmed := strings.TrimSpace(v)
		if strings.HasPrefix(trimmed, "[") {
			var parsed []domain.IceServer
			if err := json.Unmarshal([]byte(trimmed), &parsed); err == nil && len(parsed) > 0 {
				cfg.RTC.IceServers = parsed
			}
		} else {
			// Comma-separated list of URLs
			parts := strings.Split(trimmed, ",")
			urls := make([]string, 0, len(parts))
			for _, p := range parts {
				p = strings.TrimSpace(p)
				if p != "" {
					urls = append(urls, p)
				}
			}
			if len(urls) > 0 {
				cfg.RTC.IceServers = []domain.IceServer{{URLs: urls}}
			}
		}
	}

	// License (EKBATAN_LICENSE_*)
	if v := os.Getenv("EKBATAN_LICENSE_KEY"); v != "" {
		cfg.License.Key = strings.TrimSpace(v)
	}
	if v := os.Getenv("EKBATAN_LICENSE_PUBLIC_KEY"); v != "" {
		cfg.License.PublicKey = strings.TrimSpace(v)
	}

	// Telemetry (OBS_*)
	if v := os.Getenv("OBS_LOG_LEVEL"); v != "" {
		cfg.Telemetry.LogLevel = strings.TrimSpace(v)
	}
	if v := os.Getenv("OBS_LOG_FORMAT"); v != "" {
		cfg.Telemetry.LogFormat = strings.TrimSpace(v)
	}
	if v := os.Getenv("OBS_LOG_RAW_SDP"); v != "" {
		if b, err := strconv.ParseBool(strings.TrimSpace(v)); err == nil {
			cfg.Telemetry.LogRawSDP = b
		}
	}
	if v := os.Getenv("OBS_TRACING_ENABLED"); v != "" {
		if b, err := strconv.ParseBool(strings.TrimSpace(v)); err == nil {
			cfg.Telemetry.TracingEnabled = b
		}
	}
	if v := os.Getenv("OBS_TRACE_SAMPLE_RATE"); v != "" {
		if f, err := strconv.ParseFloat(strings.TrimSpace(v), 64); err == nil {
			cfg.Telemetry.TraceSampleRate = f
		}
	}
	if v := os.Getenv("OBS_OTLP_ENDPOINT"); v != "" {
		cfg.Telemetry.OtlpEndpoint = strings.TrimSpace(v)
	}
	if v := os.Getenv("OBS_METRICS_ENABLED"); v != "" {
		if b, err := strconv.ParseBool(strings.TrimSpace(v)); err == nil {
			cfg.Telemetry.MetricsEnabled = b
		}
	}
	if v := os.Getenv("OBS_METRICS_ADDR"); v != "" {
		cfg.Telemetry.MetricsAddr = strings.TrimSpace(v)
	}
}

// Validate checks that all required configuration settings are valid and consistent.
func (c *Config) Validate() error {
	if strings.TrimSpace(c.Server.Port) == "" {
		return errors.New("server.port must not be empty")
	}
	if strings.TrimSpace(c.SFU.GRPCAddr) == "" {
		return errors.New("sfu.grpc_addr must not be empty")
	}
	if c.LiveKit.PingInterval == 0 {
		return errors.New("livekit.ping_interval must be greater than 0")
	}
	if c.LiveKit.PingTimeout <= c.LiveKit.PingInterval {
		return fmt.Errorf("invalid livekit ping config: ping_timeout (%d) must be greater than ping_interval (%d)",
			c.LiveKit.PingTimeout, c.LiveKit.PingInterval)
	}
	if len(c.RTC.IceServers) == 0 {
		return errors.New("rtc.ice_servers must contain at least one ICE server")
	}
	for i, is := range c.RTC.IceServers {
		if len(is.URLs) == 0 {
			return fmt.Errorf("rtc.ice_servers[%d] must contain at least one URL", i)
		}
	}
	return nil
}
