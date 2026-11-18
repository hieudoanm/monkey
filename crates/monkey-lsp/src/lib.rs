//! Minimal Monkey C language server.
//!
//! Speaks LSP over stdio (Content-Length framed JSON-RPC) and publishes lint
//! diagnostics for `monkeyc`/`jungle` documents on open/change/save/close.
//! Diagnostics come from `monkey-linter` (text rules, lexer/parser, AST
//! rules) — the same engine as `monkey lint`.
//!
//! The core is [`Server::handle`], which maps one JSON-RPC message to an
//! optional response plus any `textDocument/publishDiagnostics` notifications
//! — testable without stdio. [`serve`] runs the framed stdio loop.

use std::collections::HashMap;
use std::io::{Read, Write};

use serde_json::{json, Value};

mod intel;

use monkey_core::{Diagnostic, Severity};
use monkey_linter::{lint_monkeyc, lint_text};

/// Per-document state the server needs to compute diagnostics.
#[derive(Default)]
pub struct Server {
    /// uri → (language id, latest text)
    docs: HashMap<String, (String, String)>,
    /// Set after `shutdown`; `exit` then terminates.
    shutdown_received: bool,
}

/// What [`Server::handle`] produced for one message.
pub struct Output {
    /// JSON-RPC response for requests; `None` for notifications.
    pub response: Option<Value>,
    /// `textDocument/publishDiagnostics` notifications to emit.
    pub notifications: Vec<Value>,
    /// Whether the server should exit (`exit` notification after shutdown).
    pub exit: bool,
}

impl Server {
    pub fn new() -> Self {
        Self::default()
    }

    /// Handle one decoded JSON-RPC message.
    pub fn handle(&mut self, msg: &Value) -> Output {
        let mut out = Output {
            response: None,
            notifications: Vec::new(),
            exit: false,
        };
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let is_request = id.is_some();
        let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));

        match method {
            "initialize" => {
                out.response = Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "capabilities": {
                            "textDocumentSync": 1, // full sync
                            "diagnosticProvider": false,
                            "documentSymbolProvider": true,
                            "hoverProvider": true,
                            "definitionProvider": true,
                            "completionProvider": {"triggerCharacters": [".", " "]},
                            "documentFormattingProvider": true,
                            "referencesProvider": true,
                            "renameProvider": {"prepareProvider": false}
                        },
                        "serverInfo": {"name": "monkey", "version": env!("CARGO_PKG_VERSION")}
                    }
                }));
            }
            "initialized" | "exit" => {
                if method == "exit" {
                    out.exit = self.shutdown_received || !is_request;
                }
            }
            "shutdown" => {
                self.shutdown_received = true;
                out.response = Some(json!({"jsonrpc": "2.0", "id": id, "result": null}));
            }
            "textDocument/didOpen" => {
                if let Some(doc) = params.get("textDocument") {
                    let uri = doc.get("uri").and_then(Value::as_str).unwrap_or("");
                    let lang = doc.get("languageId").and_then(Value::as_str).unwrap_or("");
                    let text = doc.get("text").and_then(Value::as_str).unwrap_or("");
                    if !uri.is_empty() {
                        self.docs
                            .insert(uri.to_string(), (lang.to_string(), text.to_string()));
                        out.notifications.push(publish(uri, lang, text));
                    }
                }
            }
            "textDocument/didChange" => {
                let uri = params
                    .get("textDocument")
                    .and_then(|d| d.get("uri"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                // Full sync: take the last content change's full text.
                let text = params
                    .get("contentChanges")
                    .and_then(Value::as_array)
                    .and_then(|cs| cs.last())
                    .and_then(|c| c.get("text"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if let Some(text) = text {
                    let lang = self
                        .docs
                        .get(uri)
                        .map(|(l, _)| l.clone())
                        .unwrap_or_default();
                    self.docs
                        .insert(uri.to_string(), (lang.clone(), text.clone()));
                    out.notifications.push(publish(uri, &lang, &text));
                }
            }
            "textDocument/didSave" | "textDocument/didClose" => {
                let uri = params
                    .get("textDocument")
                    .and_then(|d| d.get("uri"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if method == "textDocument/didSave" {
                    // Re-lint from the latest known text (or read the file for
                    // on-disk saves of unopened docs — keep it simple: use
                    // stored text, else skip).
                    if let Some((lang, text)) = self.docs.get(uri) {
                        out.notifications.push(publish(uri, lang, text));
                    }
                } else {
                    self.docs.remove(uri);
                    out.notifications.push(json!({
                        "jsonrpc": "2.0",
                        "method": "textDocument/publishDiagnostics",
                        "params": {"uri": uri, "diagnostics": []}
                    }));
                }
            }
            "textDocument/documentSymbol" => {
                let result = self
                    .doc_text(&params)
                    .filter(|(_, lang, _)| lang != "jungle")
                    .map(|(_, _, src)| intel::document_symbols(&src))
                    .unwrap_or_else(|| json!([]));
                out.response = Some(json!({"jsonrpc": "2.0", "id": id, "result": result}));
            }
            "textDocument/hover" => {
                let result = self.hover_result(&params);
                out.response = Some(json!({"jsonrpc": "2.0", "id": id, "result": result}));
            }
            "textDocument/definition" => {
                let result = self.definition_result(&params);
                out.response = Some(json!({"jsonrpc": "2.0", "id": id, "result": result}));
            }
            "textDocument/completion" => {
                let result = self.completion_result(&params);
                out.response = Some(json!({"jsonrpc": "2.0", "id": id, "result": result}));
            }
            "textDocument/formatting" => {
                let result = self.formatting_result(&params);
                out.response = Some(json!({"jsonrpc": "2.0", "id": id, "result": result}));
            }
            "textDocument/references" => {
                let result = self.references_result(&params);
                out.response = Some(json!({"jsonrpc": "2.0", "id": id, "result": result}));
            }
            "textDocument/rename" => {
                let result = self.rename_result(&params);
                out.response = Some(json!({"jsonrpc": "2.0", "id": id, "result": result}));
            }
            _ if is_request => {
                out.response = Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {"code": -32601, "message": format!("method not found: {}", method)}
                }));
            }
            _ => {}
        }
        out
    }

    /// uri + language + text for a `textDocument/*` params payload.
    fn doc_text(&self, params: &Value) -> Option<(String, String, String)> {
        let uri = params
            .get("textDocument")
            .and_then(|d| d.get("uri"))
            .and_then(Value::as_str)?;
        self.docs
            .get(uri)
            .map(|(lang, text)| (uri.to_string(), lang.clone(), text.clone()))
    }

    /// Byte offset from LSP `position` params.
    fn offset_param(&self, params: &Value, src: &str) -> Option<usize> {
        let pos = params.get("position")?;
        let line = pos.get("line")?.as_u64()? as usize;
        let col = pos.get("character")?.as_u64()? as usize;
        Some(intel::offset_at(src, line, col))
    }

    fn hover_result(&self, params: &Value) -> Value {
        let Some((_, lang, src)) = self.doc_text(params) else {
            return Value::Null;
        };
        if lang == "jungle" {
            return Value::Null;
        }
        match self.offset_param(params, &src) {
            Some(off) => intel::hover(&src, off).unwrap_or(Value::Null),
            None => Value::Null,
        }
    }

    fn definition_result(&self, params: &Value) -> Value {
        let Some((uri, lang, src)) = self.doc_text(params) else {
            return json!([]);
        };
        if lang == "jungle" {
            return json!([]);
        }
        match self.offset_param(params, &src) {
            Some(off) => intel::definition(&src, &uri, off),
            None => json!([]),
        }
    }

    fn completion_result(&self, params: &Value) -> Value {
        let Some((_, lang, src)) = self.doc_text(params) else {
            return json!({"isIncomplete": false, "items": []});
        };
        if lang == "jungle" {
            return json!({"isIncomplete": false, "items": []});
        }
        intel::completion(&src)
    }

    fn formatting_result(&self, params: &Value) -> Value {
        let Some((_, lang, src)) = self.doc_text(params) else {
            return Value::Null;
        };
        intel::formatting(&src, lang == "jungle")
    }

    fn references_result(&self, params: &Value) -> Value {
        let Some((uri, lang, src)) = self.doc_text(params) else {
            return json!([]);
        };
        if lang == "jungle" {
            return json!([]);
        }
        let include_decl = params
            .get("context")
            .and_then(|c| c.get("includeDeclaration"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        match self.offset_param(params, &src) {
            Some(off) => intel::references(&src, &uri, off, include_decl),
            None => json!([]),
        }
    }

    fn rename_result(&self, params: &Value) -> Value {
        let Some((uri, lang, src)) = self.doc_text(params) else {
            return Value::Null;
        };
        if lang == "jungle" {
            return Value::Null;
        }
        let new_name = params.get("newName").and_then(Value::as_str).unwrap_or("");
        match self.offset_param(params, &src) {
            Some(off) => intel::rename(&src, &uri, off, new_name).unwrap_or(Value::Null),
            None => Value::Null,
        }
    }
}

fn publish(uri: &str, lang: &str, src: &str) -> Value {
    let diagnostics = lint_diagnostics(lang, src);
    json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "diagnostics": diagnostics
        }
    })
}

fn lint_diagnostics(lang: &str, src: &str) -> Vec<Value> {
    let is_jungle = lang == "jungle";
    let mut diags = if is_jungle {
        let mut d = Vec::new();
        lint_text(src, &mut d);
        d
    } else {
        lint_monkeyc(src, "<document>.mc")
    };
    diags.sort_by_key(|d| d.span.map(|s| s.start).unwrap_or(u32::MAX));
    diags.iter().map(|d| to_lsp(src, d)).collect()
}

fn severity_lsp(s: Severity) -> i64 {
    match s {
        Severity::Error => 1,
        Severity::Warning => 2,
        Severity::Information => 3,
        Severity::Hint => 4,
    }
}

/// Convert a byte-span diagnostic to an LSP diagnostic (UTF-16 positions).
fn to_lsp(src: &str, d: &Diagnostic) -> Value {
    let (range, code) = match d.span {
        Some(span) => {
            let start = position_at_byte(src, span.start as usize);
            let end = position_at_byte(src, span.end as usize);
            (json!({"start": start, "end": end}), json!(d.code))
        }
        None => (
            json!({"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}}),
            json!(d.code),
        ),
    };
    json!({
        "range": range,
        "severity": severity_lsp(d.severity),
        "code": code,
        "source": "monkey",
        "message": d.message,
    })
}

/// UTF-8 byte offset → LSP position (0-based line, UTF-16 character).
pub(crate) fn position_at_byte(src: &str, byte_offset: usize) -> Value {
    let target = byte_offset.min(src.len());
    let mut line = 0usize;
    let mut col = 0usize; // UTF-16 units on the current line
    for (byte_i, ch) in src.char_indices() {
        if byte_i >= target {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf16();
        }
    }
    json!({"line": line, "character": col})
}

/// Run the LSP stdio loop.
pub fn serve() -> std::io::Result<()> {
    use std::io::BufRead;
    let stdin = std::io::stdin();
    let mut reader = stdin.lock();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut server = Server::new();

    loop {
        // Read headers.
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 {
                return Ok(()); // EOF
            }
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                break;
            }
            if let Some(v) = trimmed.strip_prefix("Content-Length:") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
        if content_length == 0 {
            continue;
        }
        let mut buf = vec![0u8; content_length];
        reader.read_exact(&mut buf)?;
        let Ok(msg) = serde_json::from_slice::<Value>(&buf) else {
            continue;
        };
        let output = server.handle(&msg);
        for n in output.notifications {
            write_framed(&mut out, &n)?;
        }
        if let Some(resp) = output.response {
            write_framed(&mut out, &resp)?;
        }
        out.flush()?;
        if output.exit {
            return Ok(());
        }
    }
}

fn write_framed(out: &mut impl Write, msg: &Value) -> std::io::Result<()> {
    let body = serde_json::to_vec(msg).unwrap_or_default();
    write!(out, "Content-Length: {}\r\n\r\n", body.len())?;
    out.write_all(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(method: &str, params: Value, id: Option<u64>) -> Value {
        let mut m = json!({"jsonrpc": "2.0", "method": method});
        if let Some(i) = id {
            m["id"] = json!(i);
        }
        m["params"] = params;
        m
    }

    #[test]
    fn initialize_returns_sync_capability() {
        let mut s = Server::new();
        let out = s.handle(&msg("initialize", json!({}), Some(1)));
        assert_eq!(
            out.response.unwrap()["result"]["capabilities"]["textDocumentSync"],
            1
        );
    }

    #[test]
    fn did_open_publishes_diagnostics() {
        let mut s = Server::new();
        let out = s.handle(&msg(
            "textDocument/didOpen",
            json!({"textDocument": {
                "uri": "file:///a.mc",
                "languageId": "monkeyc",
                "version": 1,
                "text": "class A {\n  function x() {\n    var unused = 1;\n  }\n}\n"
            }}),
            None,
        ));
        assert_eq!(out.notifications.len(), 1);
        let diags = out.notifications[0]["params"]["diagnostics"]
            .as_array()
            .unwrap();
        assert!(diags.iter().any(|d| d["code"] == "unused-variable"));
        assert_eq!(diags[0]["source"], "monkey");
    }

    #[test]
    fn did_change_updates_diagnostics() {
        let mut s = Server::new();
        s.handle(&msg(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": "file:///a.jungle", "languageId": "jungle", "version": 1, "text": "a=b\n"}}),
            None,
        ));
        let out = s.handle(&msg(
            "textDocument/didChange",
            json!({"textDocument": {"uri": "file:///a.jungle", "version": 2},
                   "contentChanges": [{"text": "a=b   \n"}]}),
            None,
        ));
        let diags = out.notifications[0]["params"]["diagnostics"]
            .as_array()
            .unwrap();
        assert!(diags.iter().any(|d| d["code"] == "no-trailing-whitespace"));
    }

    #[test]
    fn did_close_clears_diagnostics() {
        let mut s = Server::new();
        s.handle(&msg(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": "file:///a.mc", "languageId": "monkeyc", "version": 1, "text": "x"}}),
            None,
        ));
        let out = s.handle(&msg(
            "textDocument/didClose",
            json!({"textDocument": {"uri": "file:///a.mc", "version": 2}}),
            None,
        ));
        assert_eq!(
            out.notifications[0]["params"]["diagnostics"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn shutdown_then_exit() {
        let mut s = Server::new();
        s.handle(&msg("shutdown", json!({}), Some(9)));
        let out = s.handle(&msg("exit", json!({}), None));
        assert!(out.exit);
    }

    #[test]
    fn unknown_request_errors() {
        let mut s = Server::new();
        let out = s.handle(&msg("bogus", json!({}), Some(3)));
        assert_eq!(out.response.unwrap()["error"]["code"], -32601);
    }

    #[test]
    fn positions_are_utf16() {
        // 'é' is 1 UTF-16 unit but 2 UTF-8 bytes; a diagnostic after it must
        // report character offset in UTF-16 units.
        let src = "// é comment\nclass A {}\n";
        let pos = position_at_byte(src, src.find("class").unwrap());
        assert_eq!(pos["line"], 1);
        assert_eq!(pos["character"], 0);
    }

    #[test]
    fn document_symbol_and_hover_and_completion() {
        let mut s = Server::new();
        s.handle(&msg(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": "file:///a.mc", "languageId": "monkeyc", "version": 1,
                "text": "class A {\n  function f() as Void {}\n}\n"}}),
            None,
        ));
        let out = s.handle(&msg(
            "textDocument/documentSymbol",
            json!({"textDocument": {"uri": "file:///a.mc"}}),
            Some(2),
        ));
        let resp = out.response.unwrap();
        let syms = resp["result"].as_array().unwrap();
        assert_eq!(syms[0]["name"], "A");
        assert_eq!(syms[0]["children"][0]["name"], "f");

        let out = s.handle(&msg(
            "textDocument/hover",
            json!({"textDocument": {"uri": "file:///a.mc"},
                   "position": {"line": 1, "character": 11}}),
            Some(3),
        ));
        let resp = out.response.unwrap();
        assert!(resp["result"]["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("f("));

        let out = s.handle(&msg(
            "textDocument/completion",
            json!({"textDocument": {"uri": "file:///a.mc"},
                   "position": {"line": 0, "character": 0}}),
            Some(4),
        ));
        let resp = out.response.unwrap();
        let items = resp["result"]["items"].as_array().unwrap();
        assert!(items.iter().any(|i| i["label"] == "class"));
        assert!(items.iter().any(|i| i["label"] == "A"));
    }

    #[test]
    fn definition_jumps_to_decl() {
        let mut s = Server::new();
        s.handle(&msg(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": "file:///a.mc", "languageId": "monkeyc", "version": 1,
                "text": "class A {\n  var c = 0;\n  function f() {\n    c = 1;\n  }\n}\n"}}),
            None,
        ));
        // 'c' usage at line 3
        let out = s.handle(&msg(
            "textDocument/definition",
            json!({"textDocument": {"uri": "file:///a.mc"},
                   "position": {"line": 3, "character": 5}}),
            Some(5),
        ));
        let resp = out.response.unwrap();
        let locs = resp["result"].as_array().unwrap();
        assert_eq!(locs.len(), 1);
        assert_eq!(locs[0]["range"]["start"]["line"], 1);
    }
}
