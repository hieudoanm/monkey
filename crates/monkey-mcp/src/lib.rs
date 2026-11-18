//! MCP (Model Context Protocol) server for the Monkey toolchain.
//!
//! Speaks newline-delimited JSON-RPC 2.0 over stdio, protocol version
//! `2024-11-05` — the same shape `hieudoanm`'s `harness` MCP client expects:
//! `initialize` → `notifications/initialized` → `tools/list` → `tools/call`.
//!
//! The core is [`handle_message`], which maps one JSON-RPC message to an
//! optional response — testable without stdio. [`serve`] runs the stdio loop.

use std::io::{BufRead, Write};

use serde_json::{json, Value};

use monkey_core::{diagnostics_json_all, FileDiagnostics};
use monkey_formatter::{format_source, FormatOptions};
use monkey_linter::{lint_monkeyc, lint_text};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "monkey";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Tool definitions exposed via `tools/list`.
fn tools() -> Value {
    json!([
        {
            "name": "lint_source",
            "description": "Lint Monkey C or Jungle source. Returns diagnostics as a JSON array (path/line/column/severity/code/message). `path` selects the language (.jungle → Jungle rules, otherwise Monkey C).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {"type": "string", "description": "Source text to lint"},
                    "path": {"type": "string", "description": "File name used for language detection and diagnostic paths (default \"<stdin>.mc\")"}
                },
                "required": ["source"]
            }
        },
        {
            "name": "lint_file",
            "description": "Lint a Monkey C (.mc) or Jungle (.jungle) file on disk. Returns diagnostics as a JSON array.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the file to lint"}
                },
                "required": ["path"]
            }
        },
        {
            "name": "parse_source",
            "description": "Parse Monkey C source and return lexical/syntax diagnostics (LEX*/PARSE* codes) as a JSON array.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {"type": "string", "description": "Monkey C source text"}
                },
                "required": ["source"]
            }
        },
        {
            "name": "format_source",
            "description": "Format Monkey C or Jungle source (same rules as `monkey format`). Returns the formatted text.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {"type": "string", "description": "Source text to format"},
                    "path": {"type": "string", "description": "File name for language detection (default \"<stdin>.mc\")"},
                    "indent_spaces": {"type": "integer", "description": "Spaces per indent level (default 2)"},
                    "indent_tabs": {"type": "boolean", "description": "Indent with tabs instead of spaces"}
                },
                "required": ["source"]
            }
        },
        {
            "name": "project_info",
            "description": "Discover the Connect IQ project containing `path` (monkey.jungle + manifest.xml) and return its root, app type, entry point, minApiLevel, and target devices.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File or directory inside the project (default current directory)"}
                }
            }
        },
        {
            "name": "sdk_info",
            "description": "Report Connect IQ SDK discovery: resolved SDK path/version, installed SDKs, developer key, and device count.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "device_list",
            "description": "List Connect IQ device profiles (id, display name, resolution) from the SDK Devices directory.",
            "inputSchema": {"type": "object", "properties": {}}
        }
    ])
}

fn diagnostics_json(path: &str, src: &str, diags: &[monkey_core::Diagnostic]) -> String {
    diagnostics_json_all(&[FileDiagnostics {
        path,
        src,
        diagnostics: diags,
    }])
}

fn tool_text(text: String) -> Value {
    json!({
        "content": [{"type": "text", "text": text}],
        "isError": false
    })
}

fn tool_result(payload: Value) -> Value {
    json!({
        "content": [{"type": "text", "text": payload.to_string()}],
        "isError": false
    })
}

fn tool_error(message: &str) -> Value {
    json!({
        "content": [{"type": "text", "text": message}],
        "isError": true
    })
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn call_tool(name: &str, args: &Value) -> Value {
    match name {
        "lint_source" => {
            let Some(source) = arg_str(args, "source") else {
                return tool_error("missing required argument: source");
            };
            let path = arg_str(args, "path").unwrap_or("<stdin>.mc");
            let is_jungle = path.ends_with(".jungle");
            let mut diags = if is_jungle {
                let mut d = Vec::new();
                lint_text(source, &mut d);
                d
            } else {
                lint_monkeyc(source, path)
            };
            diags.sort_by_key(|d| d.span.map(|s| s.start).unwrap_or(u32::MAX));
            tool_text(diagnostics_json(path, source, &diags))
        }
        "lint_file" => {
            let Some(path) = arg_str(args, "path") else {
                return tool_error("missing required argument: path");
            };
            match std::fs::read_to_string(path) {
                Ok(src) => {
                    let is_jungle = path.ends_with(".jungle");
                    let mut diags = if is_jungle {
                        let mut d = Vec::new();
                        lint_text(&src, &mut d);
                        d
                    } else {
                        lint_monkeyc(&src, path)
                    };
                    diags.sort_by_key(|d| d.span.map(|s| s.start).unwrap_or(u32::MAX));
                    tool_text(diagnostics_json(path, &src, &diags))
                }
                Err(e) => tool_error(&format!("cannot read {}: {}", path, e)),
            }
        }
        "parse_source" => {
            let Some(source) = arg_str(args, "source") else {
                return tool_error("missing required argument: source");
            };
            let result = monkey_parser::parse(source);
            tool_text(diagnostics_json("<stdin>.mc", source, &result.diagnostics))
        }
        "format_source" => {
            let Some(source) = arg_str(args, "source") else {
                return tool_error("missing required argument: source");
            };
            let path = arg_str(args, "path").unwrap_or("<stdin>.mc");
            let mut opts = FormatOptions::default();
            if let Some(n) = args.get("indent_spaces").and_then(Value::as_u64) {
                opts.indent_width = Some(n as usize);
            }
            if args.get("indent_tabs").and_then(Value::as_bool) == Some(true) {
                opts.indent_width = None;
            }
            let formatted = format_source(source, path, &opts);
            tool_result(json!({"formatted": formatted}))
        }
        "project_info" => {
            let start = arg_str(args, "path").unwrap_or(".");
            match monkey_project::discover_from(start) {
                Some(p) => {
                    let app = p.manifest.application.as_ref();
                    tool_result(json!({
                        "root": p.root,
                        "jungle": p.jungle_path,
                        "manifest": p.manifest_path,
                        "describe": p.describe(),
                        "type": app.and_then(|a| a.app_type.clone()),
                        "name": app.and_then(|a| a.name.clone()),
                        "entry": app.and_then(|a| a.entry.clone()),
                        "minApiLevel": app.and_then(|a| a.min_api_level.clone()),
                        "products": p.products(),
                        "jungle": p.jungle.map(),
                        "problems": monkey_project::check_files(&p),
                    }))
                }
                None => tool_error(&format!("no Connect IQ project found from {}", start)),
            }
        }
        "sdk_info" => {
            let sdks: Vec<Value> = monkey_sdk::list_sdks()
                .iter()
                .map(|s| {
                    json!({
                        "root": s.root,
                        "version": s.version,
                        "current": s.current,
                    })
                })
                .collect();
            tool_result(json!({
                "sdk": monkey_sdk::find_sdk(None).map(|s| json!({
                    "root": s.root,
                    "version": s.version,
                })),
                "sdks": sdks,
                "developerKey": monkey_sdk::find_developer_key(None),
                "deviceCount": monkey_sdk::device::list_devices().len(),
            }))
        }
        "device_list" => {
            let devices: Vec<Value> = monkey_sdk::device::list_devices()
                .iter()
                .map(|d| {
                    json!({
                        "id": d.id,
                        "displayName": d.display_name,
                        "family": d.family,
                        "resolution": d.resolution.map(|(w, h)| format!("{}x{}", w, h)),
                    })
                })
                .collect();
            tool_result(json!(devices))
        }
        other => tool_error(&format!("unknown tool: {}", other)),
    }
}

/// Handle one JSON-RPC message. Returns `Some(response)` for requests,
/// `None` for notifications (and malformed input).
pub fn handle_message(msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str)?;
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));

    let result = match method {
        "initialize" => json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {"tools": {}},
            "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION}
        }),
        "ping" => json!({}),
        "tools/list" => json!({"tools": tools()}),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            call_tool(name, &args)
        }
        // notifications/initialized and anything else: no response.
        _ if id.is_none() => return None,
        _ => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": format!("method not found: {}", method)}
            }));
        }
    };

    Some(json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    }))
}

/// Run the MCP stdio server loop (newline-delimited JSON-RPC).
pub fn serve() -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in stdin.lock().lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
            continue; // ignore malformed lines
        };
        if let Some(resp) = handle_message(&msg) {
            writeln!(out, "{}", resp)?;
            out.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rpc(method: &str, params: Value, id: u64) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
    }

    #[test]
    fn initialize_handshake() {
        let resp = handle_message(&rpc("initialize", json!({}), 1)).unwrap();
        assert_eq!(resp["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(resp["result"]["serverInfo"]["name"], "monkey");
    }

    #[test]
    fn notifications_get_no_response() {
        let msg = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        assert!(handle_message(&msg).is_none());
    }

    #[test]
    fn tools_list_has_expected_tools() {
        let resp = handle_message(&rpc("tools/list", json!({}), 2)).unwrap();
        let names: Vec<&str> = resp["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        for expected in [
            "lint_source",
            "lint_file",
            "parse_source",
            "format_source",
            "project_info",
            "sdk_info",
            "device_list",
        ] {
            assert!(names.contains(&expected), "missing tool {}", expected);
        }
    }

    #[test]
    fn lint_source_flags_unused_variable() {
        let resp = handle_message(&rpc(
            "tools/call",
            json!({"name": "lint_source", "arguments": {
                "source": "class A {\n  function x() {\n    var unused = 1;\n  }\n}\n",
                "path": "a.mc"
            }}),
            3,
        ))
        .unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("unused-variable"), "got: {}", text);
        assert_eq!(resp["result"]["isError"], false);
    }

    #[test]
    fn format_source_roundtrip() {
        let resp = handle_message(&rpc(
            "tools/call",
            json!({"name": "format_source", "arguments": {
                "source": "class A{function x(){return 1;}}"
            }}),
            4,
        ))
        .unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        assert!(parsed["formatted"].as_str().unwrap().contains("class A {"));
    }

    #[test]
    fn unknown_method_errors() {
        let resp = handle_message(&rpc("bogus/method", json!({}), 5)).unwrap();
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[test]
    fn lint_source_jungle_uses_text_rules() {
        let resp = handle_message(&rpc(
            "tools/call",
            json!({"name": "lint_source", "arguments": {
                "source": "key = value   \n",
                "path": "app.jungle"
            }}),
            6,
        ))
        .unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("no-trailing-whitespace"), "got: {}", text);
    }
}
