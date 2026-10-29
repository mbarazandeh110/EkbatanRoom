use opentelemetry::global;
use opentelemetry::trace::{SpanId, TraceContextExt, TraceId};
use tonic::metadata::MetadataMap;
use tracing_opentelemetry::OpenTelemetrySpanExt;

struct MetadataMapCarrier<'a>(&'a MetadataMap);

impl<'a> opentelemetry::propagation::Extractor for MetadataMapCarrier<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0
            .keys()
            .filter_map(|k| match k {
                tonic::metadata::KeyRef::Ascii(k) => Some(k.as_str()),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtractedTraceContext {
    pub trace_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub trace_flags: Option<String>,
    pub request_id: Option<String>,
}

impl ExtractedTraceContext {
    pub fn from_metadata(meta: &MetadataMap) -> Self {
        let mut ctx = Self::default();

        if let Some(val) = meta.get("traceparent").and_then(|v| v.to_str().ok()) {
            let parts: Vec<&str> = val.split('-').collect();
            if parts.len() == 4 && parts[0] == "00" {
                ctx.trace_id = Some(parts[1].to_string());
                ctx.parent_span_id = Some(parts[2].to_string());
                ctx.trace_flags = Some(parts[3].to_string());
            }
        }

        if let Some(req_id) = meta.get("x-request-id").and_then(|v| v.to_str().ok()) {
            ctx.request_id = Some(req_id.to_string());
        }

        ctx
    }
}

/// Extracts W3C traceparent and x-request-id from gRPC metadata and attaches context to the tracing span
pub fn attach_trace_context(span: &tracing::Span, metadata: &MetadataMap) -> ExtractedTraceContext {
    let carrier = MetadataMapCarrier(metadata);
    let parent_cx = global::get_text_map_propagator(|propagator| propagator.extract(&carrier));
    span.set_parent(parent_cx);

    let extracted = ExtractedTraceContext::from_metadata(metadata);
    let local_otel_trace_id = span.context().span().span_context().trace_id();
    if let Some(ref trace_id) = extracted.trace_id {
        span.record("trace_id", trace_id.as_str());
    } else if local_otel_trace_id != TraceId::INVALID {
        let local_trace_str = local_otel_trace_id.to_string();
        span.record("trace_id", local_trace_str.as_str());
    }

    let local_otel_span_id = span.context().span().span_context().span_id();
    if local_otel_span_id != SpanId::INVALID {
        let local_id_str = local_otel_span_id.to_string();
        span.record("span_id", local_id_str.as_str());
    } else if let Some(ref parent_span_id) = extracted.parent_span_id {
        span.record("span_id", parent_span_id.as_str());
    }

    if let Some(ref request_id) = extracted.request_id {
        span.record("request_id", request_id.as_str());
    }

    extracted
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::metadata::MetadataValue;

    #[test]
    fn test_extract_trace_context() {
        let mut meta = MetadataMap::new();
        meta.insert(
            "traceparent",
            MetadataValue::from_static("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"),
        );
        meta.insert("x-request-id", MetadataValue::from_static("req-123-abc"));

        let ctx = ExtractedTraceContext::from_metadata(&meta);
        assert_eq!(
            ctx.trace_id.as_deref(),
            Some("4bf92f3577b34da6a3ce929d0e0e4736")
        );
        assert_eq!(ctx.parent_span_id.as_deref(), Some("00f067aa0ba902b7"));
        assert_eq!(ctx.trace_flags.as_deref(), Some("01"));
        assert_eq!(ctx.request_id.as_deref(), Some("req-123-abc"));
    }

    #[test]
    fn test_extract_empty_metadata() {
        let meta = MetadataMap::new();
        let ctx = ExtractedTraceContext::from_metadata(&meta);
        assert_eq!(ctx.trace_id, None);
        assert_eq!(ctx.request_id, None);
    }
}
