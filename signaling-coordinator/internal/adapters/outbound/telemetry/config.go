package telemetry

import (
	"os"
	"strconv"
	"strings"
)

// Config encapsulates all observability settings configured via OBS_* environment variables.
type Config struct {
	LogLevel         string  // OBS_LOG_LEVEL: debug, info, warn, error (default: info)
	LogFormat        string  // OBS_LOG_FORMAT: json, text (default: json)
	LogRawSDP        bool    // OBS_LOG_RAW_SDP: bool (default: false)
	TracingEnabled   bool    // OBS_TRACING_ENABLED: bool (default: false)
	TraceSampleRate  float64 // OBS_TRACE_SAMPLE_RATE: float (default: 1.0)
	OtlpEndpoint     string  // OBS_OTLP_ENDPOINT: string (e.g. localhost:4317)
	MetricsEnabled   bool    // OBS_METRICS_ENABLED: bool (default: true)
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
