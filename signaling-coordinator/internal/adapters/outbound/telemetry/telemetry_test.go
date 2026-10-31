package telemetry_test

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"go.opentelemetry.io/otel"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"

	"ekbatan-room/signaling-coordinator/internal/adapters/outbound/telemetry"
)

func TestLogger_SecurityRedaction(t *testing.T) {
	var buf bytes.Buffer
	cfg := telemetry.Config{
		LogLevel:  "info",
		LogFormat: "json",
		LogRawSDP: false,
	}

	logger := telemetry.SetupLogger(cfg, &buf)

	logger.Info("user join attempt",
		"token", "super-secret-jwt-token-12345",
		"access_token", "secret-token-xyz",
		"license_token", "lic-secret-token",
		"license_key", "ed25519-secret-key",
		"ice_pwd", "secret-ice-password",
		"sdp", "v=0\r\no=- 123 2 IN IP4 127.0.0.1\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111",
		"room_id", "room-101",
	)

	logOutput := buf.String()

	// Check token redaction
	if strings.Contains(logOutput, "super-secret-jwt-token-12345") {
		t.Fatalf("expected token to be redacted, got:\n%s", logOutput)
	}
	if strings.Contains(logOutput, "secret-token-xyz") {
		t.Fatalf("expected access_token to be redacted, got:\n%s", logOutput)
	}
	if strings.Contains(logOutput, "lic-secret-token") {
		t.Fatalf("expected license_token to be redacted, got:\n%s", logOutput)
	}
	if strings.Contains(logOutput, "ed25519-secret-key") {
		t.Fatalf("expected license_key to be redacted, got:\n%s", logOutput)
	}
	if strings.Contains(logOutput, "secret-ice-password") {
		t.Fatalf("expected ice_pwd to be redacted, got:\n%s", logOutput)
	}

	// Check SDP redaction when LogRawSDP is false
	if strings.Contains(logOutput, "v=0\r\no=-") {
		t.Fatalf("expected raw SDP to be redacted when LogRawSDP=false, got:\n%s", logOutput)
	}
	if !strings.Contains(logOutput, "[SDP len=") || !strings.Contains(logOutput, "m_lines=") {
		t.Fatalf("expected [SDP len=... m_lines=...] placeholder, got:\n%s", logOutput)
	}

	// Verify it's valid JSON
	var parsed map[string]interface{}
	if err := json.Unmarshal(buf.Bytes(), &parsed); err != nil {
		t.Fatalf("expected valid JSON log record: %v", err)
	}
	if parsed["token"] != "[REDACTED]" {
		t.Fatalf("expected [REDACTED] for token, got %v", parsed["token"])
	}
	if parsed["license_token"] != "[REDACTED]" {
		t.Fatalf("expected [REDACTED] for license_token, got %v", parsed["license_token"])
	}
	if parsed["license_key"] != "[REDACTED]" {
		t.Fatalf("expected [REDACTED] for license_key, got %v", parsed["license_key"])
	}
	if parsed["ice_pwd"] != "[REDACTED]" {
		t.Fatalf("expected [REDACTED] for ice_pwd, got %v", parsed["ice_pwd"])
	}
}

func TestLogger_RawSDPAllowedInDebugMode(t *testing.T) {
	var buf bytes.Buffer
	cfg := telemetry.Config{
		LogLevel:  "debug",
		LogFormat: "json",
		LogRawSDP: true,
	}

	logger := telemetry.SetupLogger(cfg, &buf)
	rawSDP := "v=0\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111"
	logger.Debug("received offer", "sdp", rawSDP)

	var parsed map[string]interface{}
	if err := json.Unmarshal(buf.Bytes(), &parsed); err != nil {
		t.Fatalf("expected valid JSON: %v", err)
	}
	if parsed["sdp"] != rawSDP {
		t.Fatalf("expected raw SDP %q, got %q", rawSDP, parsed["sdp"])
	}
}

func TestLogger_TraceContextInjection(t *testing.T) {
	tp := sdktrace.NewTracerProvider()
	otel.SetTracerProvider(tp)
	tracer := tp.Tracer("test-tracer")

	ctx, span := tracer.Start(context.Background(), "test-span")
	defer span.End()

	var buf bytes.Buffer
	cfg := telemetry.Config{
		LogLevel:  "info",
		LogFormat: "json",
	}

	logger := telemetry.SetupLogger(cfg, &buf)
	logger.InfoContext(ctx, "span active log message")

	var parsed map[string]interface{}
	if err := json.Unmarshal(buf.Bytes(), &parsed); err != nil {
		t.Fatalf("expected valid JSON log: %v", err)
	}

	traceID, hasTrace := parsed["trace_id"]
	spanID, hasSpan := parsed["span_id"]

	if !hasTrace || traceID == "" {
		t.Fatalf("expected trace_id injected from active span, got none in:\n%s", buf.String())
	}
	if !hasSpan || spanID == "" {
		t.Fatalf("expected span_id injected from active span, got none in:\n%s", buf.String())
	}
}

func TestMetrics_LowCardinalityAndExport(t *testing.T) {
	metrics := telemetry.NewMetrics()

	metrics.IncActiveRooms()
	metrics.IncActiveParticipants("livekit")
	metrics.RecordWSMessage("livekit", "offer", "ok")
	metrics.RecordSDPNegotiation("publisher", 0.025)
	metrics.IncGlareQueued()
	metrics.RecordGRPC("CreateRouter", "ok", 0.005)

	req := httptest.NewRequest(http.MethodGet, "/metrics", nil)
	w := httptest.NewRecorder()
	metrics.Handler().ServeHTTP(w, req)

	respBody := w.Body.String()

	// Verify expected metrics are present
	if !strings.Contains(respBody, "signaling_active_rooms 1") {
		t.Fatalf("expected signaling_active_rooms 1, got:\n%s", respBody)
	}
	if !strings.Contains(respBody, "signaling_sdp_glare_queued_total 1") {
		t.Fatalf("expected signaling_sdp_glare_queued_total 1, got:\n%s", respBody)
	}
	if !strings.Contains(respBody, `signaling_active_participants{protocol="livekit"} 1`) {
		t.Fatalf("expected signaling_active_participants with protocol label, got:\n%s", respBody)
	}

	// Verify strict cardinality guard: NO room_id, peer_id, or transport_id anywhere in metrics!
	forbiddenLabels := []string{"room_id", "peer_id", "participant_id", "transport_id", "request_id"}
	for _, lbl := range forbiddenLabels {
		if strings.Contains(respBody, lbl+"=") {
			t.Fatalf("cardinality guard violation: found forbidden label %s in /metrics output:\n%s", lbl, respBody)
		}
	}
}

func TestLogger_PrettyFormat(t *testing.T) {
	var buf bytes.Buffer
	cfg := telemetry.Config{
		LogLevel:  "info",
		LogFormat: "pretty",
	}

	logger := telemetry.SetupLogger(cfg, &buf)
	logger.Info("pretty log test", "key", "value")

	out := buf.String()
	if !strings.Contains(out, "pretty log test") || !strings.Contains(out, "key=value") {
		t.Fatalf("expected text-formatted pretty log, got: %s", out)
	}
}

func TestLogger_TraceLogLevel(t *testing.T) {
	var buf bytes.Buffer
	cfg := telemetry.Config{
		LogLevel:  "trace",
		LogFormat: "json",
	}

	logger := telemetry.SetupLogger(cfg, &buf)
	logger.Log(context.Background(), -4, "fine-grained trace message")

	out := buf.String()
	if !strings.Contains(out, "fine-grained trace message") {
		t.Fatalf("expected trace message to be logged when LogLevel=trace, got: %s", out)
	}
}

func TestTracer_SetupShutdown(t *testing.T) {
	cfg := telemetry.Config{
		TracingEnabled:  false,
		TraceSampleRate: 1.0,
		OtlpEndpoint:    "localhost:4317",
	}

	shutdown, err := telemetry.SetupTracer(context.Background(), cfg)
	if err != nil {
		t.Fatalf("SetupTracer failed: %v", err)
	}
	defer shutdown(context.Background())
}
