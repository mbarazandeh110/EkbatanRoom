use crate::ports::outbound::MetricsPort;
use metrics::{counter, gauge, histogram};

pub const METRIC_ACTIVE_ROUTERS: &str = "sfu_active_routers";
pub const METRIC_ACTIVE_TRANSPORTS: &str = "sfu_active_transports";
pub const METRIC_ACTIVE_PRODUCERS: &str = "sfu_active_producers";
pub const METRIC_ACTIVE_CONSUMERS: &str = "sfu_active_consumers";
pub const METRIC_GRPC_REQUESTS_TOTAL: &str = "sfu_grpc_requests_total";
pub const METRIC_GRPC_REQUEST_DURATION: &str = "sfu_grpc_request_duration_seconds";
pub const METRIC_IDEMPOTENCY_HITS_TOTAL: &str = "sfu_idempotency_hits_total";
pub const METRIC_NODE_CAPACITY_WEIGHT: &str = "sfu_node_capacity_weight";
pub const METRIC_NODE_LOGICAL_LOAD_PERCENT: &str = "sfu_node_logical_load_percent";
pub const METRIC_NODE_CPU_USAGE_PERCENT: &str = "sfu_node_cpu_usage_percent";
pub const METRIC_NODE_EFFECTIVE_LOAD_PERCENT: &str = "sfu_node_effective_load_percent";

#[derive(Clone, Default)]
pub struct PrometheusMetrics;

impl MetricsPort for PrometheusMetrics {
    fn inc_active_routers(&self) {
        inc_active_routers();
    }
    fn dec_active_routers(&self) {
        dec_active_routers();
    }
    fn inc_active_transports(&self, is_publisher: bool) {
        inc_active_transports(is_publisher);
    }
    fn dec_active_transports(&self, is_publisher: bool) {
        dec_active_transports(is_publisher);
    }
    fn inc_active_producers(&self, kind: &str) {
        inc_active_producers(kind);
    }
    fn dec_active_producers(&self, kind: &str) {
        dec_active_producers(kind);
    }
    fn inc_active_consumers(&self, kind: &str) {
        inc_active_consumers(kind);
    }
    fn dec_active_consumers(&self, kind: &str) {
        dec_active_consumers(kind);
    }
    fn record_idempotency_hit(&self) {
        record_idempotency_hit();
    }
    fn record_grpc_request(&self, method: &'static str, status: &str, duration_secs: f64) {
        record_grpc_request(method, status, duration_secs);
    }
    fn update_node_capacity(
        &self,
        total_capacity_weight: u32,
        logical_load_percent: f32,
        cpu_usage_percent: f32,
        effective_load_percent: f32,
    ) {
        update_node_capacity(
            total_capacity_weight,
            logical_load_percent,
            cpu_usage_percent,
            effective_load_percent,
        );
    }
}

/// Increment active routers gauge
pub fn inc_active_routers() {
    gauge!(METRIC_ACTIVE_ROUTERS).increment(1.0);
}

/// Decrement active routers gauge
pub fn dec_active_routers() {
    gauge!(METRIC_ACTIVE_ROUTERS).decrement(1.0);
}

/// Increment active transports with direction label ("pub" or "sub")
pub fn inc_active_transports(is_publisher: bool) {
    let direction = if is_publisher { "pub" } else { "sub" };
    gauge!(METRIC_ACTIVE_TRANSPORTS, "direction" => direction).increment(1.0);
}

/// Decrement active transports with direction label ("pub" or "sub")
pub fn dec_active_transports(is_publisher: bool) {
    let direction = if is_publisher { "pub" } else { "sub" };
    gauge!(METRIC_ACTIVE_TRANSPORTS, "direction" => direction).decrement(1.0);
}

/// Increment active producers with kind label ("audio" or "video")
pub fn inc_active_producers(kind: &str) {
    let normalized_kind = match kind.to_lowercase().as_str() {
        "audio" => "audio",
        "video" => "video",
        _ => "unknown",
    };
    gauge!(METRIC_ACTIVE_PRODUCERS, "kind" => normalized_kind).increment(1.0);
}

/// Decrement active producers with kind label ("audio" or "video")
pub fn dec_active_producers(kind: &str) {
    let normalized_kind = match kind.to_lowercase().as_str() {
        "audio" => "audio",
        "video" => "video",
        _ => "unknown",
    };
    gauge!(METRIC_ACTIVE_PRODUCERS, "kind" => normalized_kind).decrement(1.0);
}

/// Increment active consumers with kind label ("audio" or "video")
pub fn inc_active_consumers(kind: &str) {
    let normalized_kind = match kind.to_lowercase().as_str() {
        "audio" => "audio",
        "video" => "video",
        _ => "unknown",
    };
    gauge!(METRIC_ACTIVE_CONSUMERS, "kind" => normalized_kind).increment(1.0);
}

/// Decrement active consumers with kind label ("audio" or "video")
pub fn dec_active_consumers(kind: &str) {
    let normalized_kind = match kind.to_lowercase().as_str() {
        "audio" => "audio",
        "video" => "video",
        _ => "unknown",
    };
    gauge!(METRIC_ACTIVE_CONSUMERS, "kind" => normalized_kind).decrement(1.0);
}

/// Record idempotency cache hit
pub fn record_idempotency_hit() {
    counter!(METRIC_IDEMPOTENCY_HITS_TOTAL).increment(1);
}

/// Record gRPC request count and duration with cardinality-safe labels
pub fn record_grpc_request(method: &'static str, status: &str, duration_secs: f64) {
    let normalized_status = match status {
        "ok" => "ok",
        "invalid_argument" => "invalid_argument",
        "not_found" => "not_found",
        "already_exists" => "already_exists",
        "failed_precondition" => "failed_precondition",
        "aborted" => "aborted",
        "internal" => "internal",
        "unavailable" => "unavailable",
        "unimplemented" => "unimplemented",
        _ => "other",
    };

    counter!(METRIC_GRPC_REQUESTS_TOTAL, "method" => method, "status" => normalized_status)
        .increment(1);
    histogram!(METRIC_GRPC_REQUEST_DURATION, "method" => method, "status" => normalized_status)
        .record(duration_secs);
}

/// Update node capacity gauges without high-cardinality labels
pub fn update_node_capacity(
    total_capacity_weight: u32,
    logical_load_percent: f32,
    cpu_usage_percent: f32,
    effective_load_percent: f32,
) {
    gauge!(METRIC_NODE_CAPACITY_WEIGHT).set(total_capacity_weight as f64);
    gauge!(METRIC_NODE_LOGICAL_LOAD_PERCENT).set(logical_load_percent as f64);
    gauge!(METRIC_NODE_CPU_USAGE_PERCENT).set(cpu_usage_percent as f64);
    gauge!(METRIC_NODE_EFFECTIVE_LOAD_PERCENT).set(effective_load_percent as f64);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_helpers_cardinality_guards() {
        let m = PrometheusMetrics;
        m.inc_active_routers();
        m.dec_active_routers();
        m.inc_active_transports(true);
        m.dec_active_transports(true);
        m.inc_active_transports(false);
        m.dec_active_transports(false);
        m.inc_active_producers("audio");
        m.dec_active_producers("audio");
        m.inc_active_producers("video");
        m.dec_active_producers("video");
        m.inc_active_consumers("audio");
        m.dec_active_consumers("audio");
        m.record_idempotency_hit();
        m.record_grpc_request("CreateRouter", "ok", 0.012);
        m.record_grpc_request("CreateRouter", "invalid_argument", 0.001);
        m.record_grpc_request("Consume", "failed_precondition", 0.005);
    }
}
