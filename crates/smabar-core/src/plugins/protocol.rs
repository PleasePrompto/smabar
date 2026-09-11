//! JSON-RPC 2.0 NDJSON codec: classify incoming stdout lines and build
//! outgoing lines. Pure string/JSON handling, no IO.

use serde_json::{Value, json};

/// A plugin stdout line, classified.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum IncomingLine {
    /// Response to one of the core's requests.
    Response {
        id: u64,
        result: Result<Value, String>,
    },
    /// Request from the plugin; must be answered with the same id.
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    /// Notification from the plugin.
    Notification { method: String, params: Value },
    /// Not valid JSON-RPC — auto-captured into the plugin log.
    NotRpc,
}

/// Classifies one stdout line. Anything that is not a JSON object with
/// `"jsonrpc": "2.0"` and a request/response shape is [`IncomingLine::NotRpc`].
pub(crate) fn classify_line(line: &str) -> IncomingLine {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return IncomingLine::NotRpc;
    };
    let Some(object) = value.as_object() else {
        return IncomingLine::NotRpc;
    };
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return IncomingLine::NotRpc;
    }
    if let Some(method) = object.get("method").and_then(Value::as_str) {
        let method = method.to_string();
        let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
        return match object.get("id") {
            Some(id) if !id.is_null() => IncomingLine::Request {
                id: id.clone(),
                method,
                params,
            },
            _ => IncomingLine::Notification { method, params },
        };
    }
    let Some(id) = object.get("id").and_then(Value::as_u64) else {
        return IncomingLine::NotRpc;
    };
    if let Some(error) = object.get("error") {
        return IncomingLine::Response {
            id,
            result: Err(format_error(error)),
        };
    }
    match object.get("result") {
        Some(result) => IncomingLine::Response {
            id,
            result: Ok(result.clone()),
        },
        None => IncomingLine::NotRpc,
    }
}

/// Human-readable form of a JSON-RPC error object.
fn format_error(error: &Value) -> String {
    let code = error.get("code").and_then(Value::as_i64);
    let message = error.get("message").and_then(Value::as_str);
    match (code, message) {
        (Some(code), Some(message)) => format!("{message} (code {code})"),
        (None, Some(message)) => message.to_string(),
        _ => error.to_string(),
    }
}

pub(crate) fn request_line(id: u64, method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

pub(crate) fn notification_line(method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "method": method, "params": params}).to_string()
}

pub(crate) fn response_ok_line(id: &Value, result: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
}

pub(crate) fn response_error_line(id: &Value, code: i64, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garbage_lines_are_not_rpc() {
        for line in [
            "hello from print()",
            "[1, 2, 3]",
            "42",
            r#"{"foo": 1}"#,
            r#"{"method": "x", "params": {}}"#,
            r#"{"jsonrpc": "1.0", "method": "x"}"#,
            // id present, but neither result nor error.
            r#"{"jsonrpc": "2.0", "id": 5}"#,
        ] {
            assert_eq!(classify_line(line), IncomingLine::NotRpc, "line: {line}");
        }
    }

    #[test]
    fn notifications_classify_with_and_without_params() {
        assert_eq!(
            classify_line(r#"{"jsonrpc": "2.0", "method": "log", "params": {"level": "info"}}"#),
            IncomingLine::Notification {
                method: "log".to_string(),
                params: json!({"level": "info"}),
            }
        );
        // Missing params default to an empty object; a null id still means
        // notification.
        assert_eq!(
            classify_line(r#"{"jsonrpc": "2.0", "id": null, "method": "ui.render"}"#),
            IncomingLine::Notification {
                method: "ui.render".to_string(),
                params: json!({}),
            }
        );
    }

    #[test]
    fn plugin_requests_keep_their_id_verbatim() {
        assert_eq!(
            classify_line(r#"{"jsonrpc": "2.0", "id": 7, "method": "settings.get", "params": {}}"#),
            IncomingLine::Request {
                id: json!(7),
                method: "settings.get".to_string(),
                params: json!({}),
            }
        );
    }

    #[test]
    fn responses_classify_success_and_error() {
        assert_eq!(
            classify_line(r#"{"jsonrpc": "2.0", "id": 3, "result": {"ok": true}}"#),
            IncomingLine::Response {
                id: 3,
                result: Ok(json!({"ok": true})),
            }
        );
        assert_eq!(
            classify_line(
                r#"{"jsonrpc": "2.0", "id": 4, "error": {"code": -32000, "message": "boom"}}"#
            ),
            IncomingLine::Response {
                id: 4,
                result: Err("boom (code -32000)".to_string()),
            }
        );
    }

    #[test]
    fn built_lines_are_single_line_json_that_round_trips() {
        let lines = [
            request_line(1, "initialize", json!({"protocolVersion": 1})),
            notification_line("event", json!({"tileId": "w", "action": "click"})),
            response_ok_line(&json!(9), json!({})),
            response_error_line(&json!(9), -32601, "unknown method"),
        ];
        for line in &lines {
            assert!(!line.contains('\n'), "must be one NDJSON line: {line}");
            let value: Value = serde_json::from_str(line).expect("valid JSON");
            assert_eq!(value["jsonrpc"], "2.0");
        }
    }

    #[test]
    fn built_request_classifies_back_as_response_pair() {
        // A request the core sends and the matching response a plugin echoes.
        let request = request_line(12, "ping", json!({}));
        let parsed: Value = serde_json::from_str(&request).expect("valid JSON");
        assert_eq!(parsed["id"], 12);
        assert_eq!(parsed["method"], "ping");

        let response = response_ok_line(&parsed["id"], json!({}));
        assert_eq!(
            classify_line(&response),
            IncomingLine::Response {
                id: 12,
                result: Ok(json!({})),
            }
        );
    }
}
