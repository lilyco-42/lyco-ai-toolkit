//! The slice of JSON-RPC 2.0 that MCP needs over stdio.
//!
//! Only three request shapes matter for a tool server: initialize,
//! tools/list and tools/call. Everything else is answered with a
//! method-not-found error rather than a guess.

use serde_json::{json, Value};

pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

/// The MCP revision this server implements.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

pub fn success(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub fn error(id: Value, code: i32, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message.into() },
    })
}

/// A tool that succeeded. MCP carries the payload as content blocks.
pub fn tool_text(id: Value, text: impl Into<String>) -> Value {
    success(
        id,
        json!({
            "content": [{ "type": "text", "text": text.into() }],
            "isError": false,
        }),
    )
}

/// A tool that ran but failed. Per the spec this is a *successful* JSON-RPC
/// response carrying isError, not a protocol-level error, so the model can
/// read the message and try something else.
pub fn tool_error(id: Value, text: impl Into<String>) -> Value {
    success(
        id,
        json!({
            "content": [{ "type": "text", "text": text.into() }],
            "isError": true,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_carries_the_id_through() {
        let v = success(json!(7), json!({"ok": true}));
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["id"], 7);
        assert_eq!(v["result"]["ok"], true);
        assert!(v.get("error").is_none());
    }

    #[test]
    fn error_carries_code_and_message() {
        let v = error(json!(1), METHOD_NOT_FOUND, "nope");
        assert_eq!(v["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(v["error"]["message"], "nope");
        assert!(v.get("result").is_none());
    }

    #[test]
    fn tool_results_use_content_blocks_and_an_iserror_flag() {
        let ok = tool_text(json!(1), "done");
        assert_eq!(ok["result"]["content"][0]["type"], "text");
        assert_eq!(ok["result"]["content"][0]["text"], "done");
        assert_eq!(ok["result"]["isError"], false);

        let bad = tool_error(json!(1), "boom");
        assert_eq!(bad["result"]["isError"], true);
    }
}
