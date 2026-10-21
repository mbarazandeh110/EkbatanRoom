pub const REDACTED: &str = "[REDACTED]";

/// Redacts sensitive strings (e.g. ice_pwd, tokens)
pub fn redact_secret(_secret: &str) -> &'static str {
    REDACTED
}

/// Redact ICE password from WebRTC transport options / parameters
pub fn redact_ice_pwd(pwd: &str) -> &'static str {
    if pwd.is_empty() {
        ""
    } else {
        REDACTED
    }
}

/// Helper to sanitize a JSON value containing potentially sensitive fields (such as iceParameters.password)
pub fn sanitize_json_value(val: &mut serde_json::Value) {
    match val {
        serde_json::Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                let lower = k.to_lowercase();
                if lower == "password"
                    || lower == "ice_pwd"
                    || lower == "token"
                    || lower == "access_token"
                    || lower == "secret"
                {
                    *v = serde_json::Value::String(REDACTED.to_string());
                } else {
                    sanitize_json_value(v);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                sanitize_json_value(item);
            }
        }
        _ => {}
    }
}

/// Logs raw SDP candidate line or SDP only if OBS_LOG_RAW_SDP=true at debug level
pub fn log_raw_sdp_if_enabled(enabled: bool, context: &str, sdp: &str) {
    if enabled {
        tracing::debug!(sdp_context = %context, raw_sdp = %sdp, "Raw SDP data");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_redaction() {
        assert_eq!(redact_secret("secret_token_123"), "[REDACTED]");
        assert_eq!(redact_ice_pwd("ice_pass_xyz"), "[REDACTED]");
        assert_eq!(redact_ice_pwd(""), "");

        let mut data = json!({
            "iceParameters": {
                "usernameFragment": "abcd",
                "password": "supersecretpassword"
            },
            "token": "jwt.header.payload"
        });

        sanitize_json_value(&mut data);
        assert_eq!(data["iceParameters"]["usernameFragment"], "abcd");
        assert_eq!(data["iceParameters"]["password"], "[REDACTED]");
        assert_eq!(data["token"], "[REDACTED]");
    }
}
