//! JSON-RPC 2.0 response envelope types for CLI output.
//!
//! Provides [`wrap_success`] and [`wrap_error`] helpers that produce JSON
//! strings suitable for direct consumption by MCP-compatible clients and
//! agents.  The envelope format matches the JSON-RPC 2.0 specification and
//! is structurally identical to what the `rmcp` transport layer returns over
//! STDIO.
//!
//! CLI invocations have no request ID, so the `id` field is always `null`.

use serde::Serialize;
use serde_json::Value;

/// JSON-RPC error code for general server-side failures.
///
/// Per the JSON-RPC 2.0 specification, codes from `-32099` to `-32000` are
/// reserved for implementation-defined server errors.  `-32000` is the
/// conventional first entry.
pub const SERVER_ERROR: i32 = -32000;

/// A successful JSON-RPC 2.0 response envelope.
///
/// Serializes to `{"jsonrpc":"2.0","id":null,"result":{...}}`.
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    /// Protocol version.  Always `"2.0"`.
    pub jsonrpc: String,
    /// Request identifier.  Always `null` for CLI invocations.
    pub id: Option<Value>,
    /// The successful result payload.
    pub result: Value,
}

/// A JSON-RPC 2.0 error response envelope.
///
/// Serializes to `{"jsonrpc":"2.0","id":null,"error":{...}}`.
#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    /// Protocol version.  Always `"2.0"`.
    pub jsonrpc: String,
    /// Request identifier.  Always `null` for CLI invocations.
    pub id: Option<Value>,
    /// The error details.
    pub error: JsonRpcErrorObject,
}

/// The `error` object inside a JSON-RPC 2.0 error response.
#[derive(Debug, Serialize)]
pub struct JsonRpcErrorObject {
    /// Numeric error code (e.g. [`SERVER_ERROR`]).
    pub code: i32,
    /// Short human-readable error description.
    pub message: String,
    /// Optional additional error context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Wrap a serialisable value in a JSON-RPC 2.0 success envelope.
///
/// Returns a pretty-printed JSON string.  If serialisation fails (which
/// should not happen for well-formed `Serialize` types), returns a fallback
/// JSON-RPC error envelope string.
pub fn wrap_success(result: impl Serialize) -> String {
    let value = match serde_json::to_value(&result) {
        Ok(v) => v,
        Err(e) => return wrap_error(-32_603, format!("serialization error: {e}"), None),
    };
    let response = JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: None,
        result: value,
    };
    serde_json::to_string_pretty(&response).unwrap_or_else(|e| {
        format!(
            r#"{{"jsonrpc":"2.0","id":null,"error":{{"code":-32603,"message":"serialization error: {e}"}}}}"#
        )
    })
}

/// Wrap an error in a JSON-RPC 2.0 error envelope.
///
/// Returns a pretty-printed JSON string.  `code` should be a JSON-RPC 2.0
/// error code — use [`SERVER_ERROR`] (`-32000`) for general command failures.
/// Pass `data` to include additional error context.
pub fn wrap_error(code: i32, message: impl Into<String>, data: Option<Value>) -> String {
    let error = JsonRpcError {
        jsonrpc: "2.0".to_string(),
        id: None,
        error: JsonRpcErrorObject {
            code,
            message: message.into(),
            data,
        },
    };
    serde_json::to_string_pretty(&error).unwrap_or_else(|_| {
        format!(
            r#"{{"jsonrpc":"2.0","id":null,"error":{{"code":{code},"message":"serialization error"}}}}"#
        )
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn wrap_success_produces_jsonrpc_envelope() {
        let output = wrap_success(json!({"key": "value"}));
        let parsed: Value = serde_json::from_str(&output).expect("valid json");
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["id"], Value::Null);
        assert_eq!(parsed["result"]["key"], "value");
    }

    #[test]
    fn wrap_success_has_no_error_field() {
        let output = wrap_success(json!(42));
        let parsed: Value = serde_json::from_str(&output).expect("valid json");
        assert!(parsed.get("error").is_none());
    }

    #[test]
    fn wrap_error_produces_jsonrpc_error_envelope() {
        let output = wrap_error(SERVER_ERROR, "something failed", None);
        let parsed: Value = serde_json::from_str(&output).expect("valid json");
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["id"], Value::Null);
        assert_eq!(parsed["error"]["code"], SERVER_ERROR);
        assert_eq!(parsed["error"]["message"], "something failed");
        assert!(parsed["error"].get("data").is_none());
    }

    #[test]
    fn wrap_error_includes_data_when_provided() {
        let output = wrap_error(SERVER_ERROR, "msg", Some(json!({"detail": "x"})));
        let parsed: Value = serde_json::from_str(&output).expect("valid json");
        assert_eq!(parsed["error"]["data"]["detail"], "x");
    }

    #[test]
    fn wrap_error_has_no_result_field() {
        let output = wrap_error(-32_000, "err", None);
        let parsed: Value = serde_json::from_str(&output).expect("valid json");
        assert!(parsed.get("result").is_none());
    }

    #[test]
    fn wrap_success_null_result_is_valid() {
        let output = wrap_success(Value::Null);
        let parsed: Value = serde_json::from_str(&output).expect("valid json");
        assert_eq!(parsed["result"], Value::Null);
    }
}
