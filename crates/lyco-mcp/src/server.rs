//! Request dispatch and the stdio transport.
//!
//! The transport is newline-delimited JSON on stdin/stdout, which is what MCP
//! clients speak to a local server. Dispatch is a pure function so the protocol
//! can be tested without spawning a process.

use anyhow::Result;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

use crate::jsonrpc;
use crate::tools::Registry;

pub const SERVER_NAME: &str = "lyco";

/// Handle one input line.
///
/// Returns the response to write, or `None` when the message was a
/// notification. Per JSON-RPC a message without an `id` is a notification and
/// must not be answered, so `notifications/initialized` is silently accepted.
pub fn handle_line(registry: &Registry, line: &str) -> Option<Value> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let request: Value = match serde_json::from_str(trimmed) {
        Ok(value) => value,
        Err(err) => {
            return Some(jsonrpc::error(
                Value::Null,
                jsonrpc::PARSE_ERROR,
                format!("invalid JSON: {err}"),
            ));
        }
    };

    // A message without an id is a notification and must not be answered.
    let id = request.get("id").cloned()?;
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return Some(jsonrpc::error(
            id,
            jsonrpc::INVALID_REQUEST,
            "request has no 'method'",
        ));
    };
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    Some(dispatch(registry, id, method, &params))
}

fn dispatch(registry: &Registry, id: Value, method: &str, params: &Value) -> Value {
    match method {
        "initialize" => jsonrpc::success(
            id,
            json!({
                "protocolVersion": jsonrpc::PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": env!("CARGO_PKG_VERSION"),
                },
            }),
        ),
        "ping" => jsonrpc::success(id, json!({})),
        "tools/list" => jsonrpc::success(id, registry.list_json()),
        "tools/call" => call_tool(registry, id, params),
        other => jsonrpc::error(
            id,
            jsonrpc::METHOD_NOT_FOUND,
            format!("unknown method '{other}'"),
        ),
    }
}

fn call_tool(registry: &Registry, id: Value, params: &Value) -> Value {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return jsonrpc::error(id, jsonrpc::INVALID_PARAMS, "tools/call needs a 'name'");
    };
    let Some(tool) = registry.find(name) else {
        return jsonrpc::error(
            id,
            jsonrpc::INVALID_PARAMS,
            format!("unknown tool '{name}'"),
        );
    };
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    // A failing tool is reported inside the result with isError, not as a
    // protocol error, so the model can read the message and adapt.
    match (tool.handler)(&args) {
        Ok(text) => jsonrpc::tool_text(id, text),
        Err(err) => jsonrpc::tool_error(id, format!("{err:#}")),
    }
}

/// Read newline-delimited JSON from stdin and answer on stdout.
pub fn serve_stdio(registry: &Registry) -> Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if let Some(response) = handle_line(registry, &line) {
            serde_json::to_writer(&mut out, &response)?;
            out.write_all(b"\n")?;
            out.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> Registry {
        Registry::standard()
    }

    fn call(line: &str) -> Value {
        handle_line(&registry(), line).expect("expected a response")
    }

    #[test]
    fn initialize_reports_the_protocol_and_server_info() {
        let response = call(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#);
        assert_eq!(
            response["result"]["protocolVersion"],
            jsonrpc::PROTOCOL_VERSION
        );
        assert_eq!(response["result"]["serverInfo"]["name"], SERVER_NAME);
        assert!(response["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn a_notification_gets_no_reply() {
        assert!(handle_line(
            &registry(),
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
        )
        .is_none());
        assert!(handle_line(&registry(), "").is_none());
        assert!(handle_line(&registry(), "   ").is_none());
    }

    #[test]
    fn tools_list_advertises_every_tool_with_a_schema() {
        let response = call(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
        let tools = response["result"]["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), registry().len());
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        for expected in [
            "skin_template",
            "model_from_glb",
            "lyrics_generate",
            "bundle",
        ] {
            assert!(names.contains(&expected), "missing {expected} in {names:?}");
        }
        for tool in tools {
            assert!(tool["description"].as_str().is_some_and(|d| !d.is_empty()));
            assert_eq!(tool["inputSchema"]["type"], "object");
            assert!(tool["inputSchema"]["properties"].is_object());
        }
    }

    #[test]
    fn malformed_json_is_a_parse_error_with_a_null_id() {
        let response = call("this is not json");
        assert_eq!(response["error"]["code"], jsonrpc::PARSE_ERROR);
        assert!(response["id"].is_null());
    }

    #[test]
    fn an_unknown_method_is_method_not_found() {
        let response = call(r#"{"jsonrpc":"2.0","id":3,"method":"resources/list"}"#);
        assert_eq!(response["error"]["code"], jsonrpc::METHOD_NOT_FOUND);
    }

    #[test]
    fn an_unknown_tool_is_invalid_params() {
        let line = r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"nope"}}"#;
        let response = call(line);
        assert_eq!(response["error"]["code"], jsonrpc::INVALID_PARAMS);
    }

    #[test]
    fn a_tool_that_fails_reports_iserror_rather_than_a_protocol_error() {
        let line = r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"skin_validate","arguments":{"path":"definitely-absent.png"}}}"#;
        let response = call(line);
        assert!(
            response.get("error").is_none(),
            "must not be a protocol error"
        );
        assert_eq!(response["result"]["isError"], true);
        let text = response["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or("");
        assert!(!text.is_empty(), "the failure needs a readable message");
    }

    #[test]
    fn missing_required_arguments_fail_the_tool_not_the_protocol() {
        let line = r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"skin_template","arguments":{}}}"#;
        let response = call(line);
        assert_eq!(response["result"]["isError"], true);
        let text = response["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or("");
        assert!(
            text.contains("out"),
            "should name the missing argument: {text}"
        );
    }

    #[test]
    fn ping_is_answered() {
        let response = call(r#"{"jsonrpc":"2.0","id":7,"method":"ping"}"#);
        assert!(response.get("result").is_some());
    }
}
