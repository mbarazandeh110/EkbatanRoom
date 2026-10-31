package telemetry

import (
	"os"
	"strconv"
	"strings"
)

// Config encapsulates all observability settings configured via OBS_* environment variables.
type Config struct {
	LogLevel        string  `json:"log_level" yaml:"log_level"`
	LogFormat       string  `json:"log_format" yaml:"log_format"`
	LogRawSDP       bool    `json:"log_raw_sdp" yaml:"log_raw_sdp"`
	TracingEnabled  bool    `json:"tracing_enabled" yaml:"tracing_enabled"`
	TraceSampleRate float64 `json:"trace_sample_rate" yaml:"trace_sample_rate"`
	OtlpEndpoint    string  `json:"otlp_endpoint" yaml:"otlp_endpoint"`
	MetricsEnabled  bool    `json:"metrics_enabled" yaml:"metrics_enabled"`
	MetricsAddr     string  `json:"metrics_addr" yaml:"metrics_addr"`
}

func LoadConfigFromEnv() Config {
	cfg := Config{
		LogLevel:        getEnv("OBS_LOG_LEVEL", "info"),
		LogFormat:       getEnv("OBS_LOG_FORMAT", "json"),
		LogRawSDP:       getEnvBool("OBS_LOG_RAW_SDP", false),
		TracingEnabled:  getEnvBool("OBS_TRACING_ENABLED", false),
		TraceSampleRate: getEnvFloat("OBS_TRACE_SAMPLE_RATE", 1.0),
		OtlpEndpoint:    getEnv("OBS_OTLP_ENDPOINT", ""),
		MetricsEnabled:  getEnvBool("OBS_METRICS_ENABLED", true),
		MetricsAddr:     getEnv("OBS_METRICS_ADDR", ""),
	}
	return cfg
}

func getEnv(key, defaultVal string) string {
	if val := os.Getenv(key); val != "" {
		return val
	}
	return defaultVal
}

func getEnvBool(key string, defaultVal bool) bool {
	val := os.Getenv(key)
	if val == "" {
		return defaultVal
	}
	b, err := strconv.ParseBool(strings.TrimSpace(val))
	if err != nil {
		return defaultVal
	}
	return b
}

func getEnvFloat(key string, defaultVal float64) float64 {
	val := os.Getenv(key)
	if val == "" {
		return defaultVal
	}
	f, err := strconv.ParseFloat(strings.TrimSpace(val), 64)
	if err != nil {
		return defaultVal
	}
	return f
}
