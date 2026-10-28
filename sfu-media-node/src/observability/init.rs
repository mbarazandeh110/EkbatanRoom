use crate::observability::config::ObservabilityConfig;
use opentelemetry::global;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::{BatchSpanProcessor, Sampler, TracerProvider};
use std::error::Error;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

fn build_otel_layer<S>(
    config: &ObservabilityConfig,
) -> Result<
    Option<tracing_opentelemetry::OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>>,
    Box<dyn Error>,
>
where
    S: tracing::Subscriber + for<'span> LookupSpan<'span>,
{
    if config.tracing_enabled {
        global::set_text_map_propagator(TraceContextPropagator::new());

        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(&config.tracing_endpoint)
            .build()
            .map_err(|e| Box::new(e) as Box<dyn Error>)?;

        let processor =
            BatchSpanProcessor::builder(exporter, opentelemetry_sdk::runtime::Tokio).build();

        let provider = TracerProvider::builder()
            .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
                config.tracing_sample_rate,
            ))))
            .with_span_processor(processor)
            .build();

        global::set_tracer_provider(provider.clone());
        let tracer = opentelemetry::trace::TracerProvider::tracer(&provider, "sfu-media-node");
        Ok(Some(tracing_opentelemetry::layer().with_tracer(tracer)))
    } else {
        Ok(None)
    }
}

pub fn shutdown_tracer_provider() {
    global::shutdown_tracer_provider();
}

/// Initializes structured logging, optional OpenTelemetry distributed tracing, and Prometheus metrics
pub fn init_observability(config: &ObservabilityConfig) -> Result<(), Box<dyn Error>> {
    let filter = EnvFilter::new(&config.log_level);

    if config.log_format == "json" {
        let otel_layer = build_otel_layer(config)?;
        let fmt_layer = tracing_subscriber::fmt::layer().json();
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .with(otel_layer)
            .try_init();
    } else if config.log_format == "pretty" {
        let otel_layer = build_otel_layer(config)?;
        let fmt_layer = tracing_subscriber::fmt::layer().pretty();
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .with(otel_layer)
            .try_init();
    } else {
        let otel_layer = build_otel_layer(config)?;
        let fmt_layer = tracing_subscriber::fmt::layer();
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .with(otel_layer)
            .try_init();
    }

    if config.metrics_enabled {
        let builder = metrics_exporter_prometheus::PrometheusBuilder::new()
            .with_http_listener(config.metrics_addr);
        if let Err(e) = builder.install() {
            tracing::warn!(error = %e, "Prometheus metrics recorder already initialized or failed to bind");
        } else {
            tracing::info!(metrics_addr = %config.metrics_addr, "Prometheus /metrics endpoint started");
        }
    }

    Ok(())
}
