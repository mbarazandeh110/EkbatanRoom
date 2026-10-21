package telemetry

import (
	"context"

	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/propagation"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"
	"go.opentelemetry.io/otel/trace"
	"go.opentelemetry.io/otel/trace/noop"
)

const TracerName = "ekbatan-room/signaling-coordinator"

// SetupTracer initializes OpenTelemetry distributed tracing with W3C propagation.
func SetupTracer(ctx context.Context, cfg Config) (func(context.Context) error, error) {
	// 1. Register W3C TraceContext propagator globally
	otel.SetTextMapPropagator(propagation.NewCompositeTextMapPropagator(
		propagation.TraceContext{},
		propagation.Baggage{},
	))

	if !cfg.TracingEnabled {
		otel.SetTracerProvider(noop.NewTracerProvider())
		return func(context.Context) error { return nil }, nil
	}

	// 2. Configure Sampler with configurable sample rate
	sampler := sdktrace.ParentBased(sdktrace.TraceIDRatioBased(cfg.TraceSampleRate))

	tp := sdktrace.NewTracerProvider(
		sdktrace.WithSampler(sampler),
	)

	otel.SetTracerProvider(tp)

	shutdown := func(ctx context.Context) error {
		return tp.Shutdown(ctx)
	}

	return shutdown, nil
}

// Tracer returns the named OpenTelemetry tracer.
func Tracer() trace.Tracer {
	return otel.GetTracerProvider().Tracer(TracerName)
}

// StartSpan starts a new OpenTelemetry span.
func StartSpan(ctx context.Context, spanName string, opts ...trace.SpanStartOption) (context.Context, trace.Span) {
	return Tracer().Start(ctx, spanName, opts...)
}
