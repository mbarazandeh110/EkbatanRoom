use metrics::{counter, gauge, histogram};

pub const METRIC_ACTIVE_ROUTERS: &str = "sfu_active_routers";
pub const METRIC_ACTIVE_TRANSPORTS: &str = "sfu_active_transports";
pub const METRIC_ACTIVE_PRODUCERS: &str = "sfu_active_producers";
pub const METRIC_ACTIVE_CONSUMERS: &str = "sfu_active_consumers";
pub const METRIC_GRPC_REQUESTS_TOTAL: &str = "sfu_grpc_requests_total";
pub const METRIC_GRPC_REQUEST_DURATION: &str = "sfu_grpc_request_duration_seconds";
pub const METRIC_IDEMPOTENCY_HITS_TOTAL: &str = "sfu_idempotency_hits_total";

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
        "aborted" => "aborted",
        "internal" => "internal",
        "unavailable" => "unavailable",
        _ => "other",
    };

    counter!(METRIC_GRPC_REQUESTS_TOTAL, "method" => method, "status" => normalized_status)
        .increment(1);
    histogram!(METRIC_GRPC_REQUEST_DURATION, "method" => method, "status" => normalized_status)
        .record(duration_secs);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_helpers_cardinality_guards() {
        // Ensure metric functions do not panic and accept standard labels
        inc_active_routers();
        dec_active_routers();
        inc_active_transports(true);
        dec_active_transports(true);
        inc_active_transports(false);
        dec_active_transports(false);
        inc_active_producers("audio");
        dec_active_producers("audio");
        inc_active_producers("video");
        dec_active_producers("video");
        inc_active_consumers("audio");
        dec_active_consumers("audio");
        record_idempotency_hit();
        record_grpc_request("CreateRouter", "ok", 0.012);
        record_grpc_request("CreateRouter", "invalid_argument", 0.001);
    }
}
