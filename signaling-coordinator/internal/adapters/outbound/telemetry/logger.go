package telemetry

import (
	"context"
	"fmt"
	"io"
	"log/slog"
	"os"
	"strings"

	"go.opentelemetry.io/otel/trace"
)

var sensitiveKeys = map[string]bool{
	"token":        true,
	"access_token": true,
	"ice_pwd":      true,
	"secret":       true,
	"password":     true,
	"credential":   true,
}

// otelContextHandler wraps slog.Handler to inject active trace_id and span_id into log records.
type otelContextHandler struct {
	slog.Handler
}

func (h *otelContextHandler) Handle(ctx context.Context, r slog.Record) error {
	span := trace.SpanFromContext(ctx)
	if span != nil && span.SpanContext().IsValid() {
		r.AddAttrs(
			slog.String("trace_id", span.SpanContext().TraceID().String()),
			slog.String("span_id", span.SpanContext().SpanID().String()),
		)
	}
	return h.Handler.Handle(ctx, r)
}

func (h *otelContextHandler) WithAttrs(attrs []slog.Attr) slog.Handler {
	return &otelContextHandler{Handler: h.Handler.WithAttrs(attrs)}
}

func (h *otelContextHandler) WithGroup(name string) slog.Handler {
	return &otelContextHandler{Handler: h.Handler.WithGroup(name)}
}

// SetupLogger configures log/slog with level filtering, security redaction, and trace correlation.
func SetupLogger(cfg Config, w io.Writer) *slog.Logger {
	if w == nil {
		w = os.Stdout
	}

	var level slog.Level
	switch strings.ToLower(cfg.LogLevel) {
	case "debug":
		level = slog.LevelDebug
	case "warn", "warning":
		level = slog.LevelWarn
	case "error":
		level = slog.LevelError
	default:
		level = slog.LevelInfo
	}

	replaceAttr := func(groups []string, a slog.Attr) slog.Attr {
		keyLower := strings.ToLower(a.Key)

		// 1. Redact credentials
		if sensitiveKeys[keyLower] {
			return slog.String(a.Key, "[REDACTED]")
		}

		// 2. Redact SDP unless debug level AND OBS_LOG_RAW_SDP=true
		if keyLower == "sdp" || keyLower == "raw_sdp" || keyLower == "sdp_offer" || keyLower == "sdp_answer" {
			if level == slog.LevelDebug && cfg.LogRawSDP {
				return a
			}
			strVal := a.Value.String()
			return slog.String(a.Key, fmt.Sprintf("[SDP len=%d]", len(strVal)))
		}

		return a
	}

	opts := &slog.HandlerOptions{
		Level:       level,
		ReplaceAttr: replaceAttr,
	}

	var baseHandler slog.Handler
	if strings.ToLower(cfg.LogFormat) == "text" {
		baseHandler = slog.NewTextHandler(w, opts)
	} else {
		baseHandler = slog.NewJSONHandler(w, opts)
	}

	handler := &otelContextHandler{Handler: baseHandler}
	logger := slog.New(handler)
	slog.SetDefault(logger)
	return logger
}
