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
use std::path::PathBuf;
use std::rc::Rc;

use serde_json::{json, Value};

mod index;
mod intel;
mod toybox;

use monkey_core::{Diagnostic, Severity, Span};
use monkey_linter::{lint_monkeyc, lint_text};

/// Per-document state the server needs to compute diagnostics.
#[derive(Default)]
pub struct Server {
    /// uri → (language id, latest text)
    docs: HashMap<String, (String, String)>,
    /// Set after `shutdown`; `exit` then terminates.
    shutdown_received: bool,
    /// Workspace root from `initialize` (workspaceFolders/rootUri).
    workspace_root: Option<PathBuf>,
    /// Lazily-built project symbol index.
    index: Option<Rc<index::ProjectIndex>>,
    /// Rebuild the index on next use (any doc change invalidates it).
    index_dirty: bool,
    /// Lazily-loaded Toybox API member database (from the SDK).
    toybox: Option<Rc<Option<toybox::ToyboxDb>>>,
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
                self.workspace_root = workspace_root(&params);
                out.response = Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "capabilities": {
                            "textDocumentSync": 2, // incremental
                            "diagnosticProvider": false,
                            "documentSymbolProvider": true,
                            "hoverProvider": true,
                            "definitionProvider": true,
                            "completionProvider": {"triggerCharacters": [".", " "]},
                            "documentFormattingProvider": true,
                            "rangeFormattingProvider": true,
                            "referencesProvider": true,
                            "renameProvider": {"prepareProvider": true},
                            "signatureHelpProvider": {"triggerCharacters": ["(", ","]}
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
                        self.index_dirty = true;
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
                // Incremental sync: apply each change in order. A change with
                // `range` is a ranged edit; one without is a full-text replace.
                let mut text = self
                    .docs
                    .get(uri)
                    .map(|(_, t)| t.clone())
                    .unwrap_or_default();
                if let Some(changes) = params.get("contentChanges").and_then(Value::as_array) {
                    for c in changes {
                        let new = c.get("text").and_then(Value::as_str).unwrap_or("");
                        let pos = |p: &str| -> Option<(u64, u64)> {
                            let line = c.pointer(&format!("/range/{}/line", p))?.as_u64()?;
                            let chr = c.pointer(&format!("/range/{}/character", p))?.as_u64()?;
                            Some((line, chr))
                        };
                        match (pos("start"), pos("end")) {
                            (Some((sl, sc)), Some((el, ec))) => {
                                if let Some(u) = apply_text_edit(&text, sl, sc, el, ec, new) {
                                    text = u;
                                }
                            }
                            _ => text = new.to_string(),
                        }
                    }
                }
                let lang = self
                    .docs
                    .get(uri)
                    .map(|(l, _)| l.clone())
                    .unwrap_or_default();
                self.docs
                    .insert(uri.to_string(), (lang.clone(), text.clone()));
                self.index_dirty = true;
                out.notifications.push(publish(uri, &lang, &text));
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
                    self.index_dirty = true;
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
            "textDocument/formatting" | "textDocument/rangeFormatting" => {
                // The formatter is whole-document; for range requests we return
                // the same single full-document edit (clients accept edits that
                // extend beyond the requested range).
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
            "textDocument/signatureHelp" => {
                let result = self.signature_help_result(&params);
                out.response = Some(json!({"jsonrpc": "2.0", "id": id, "result": result}));
            }
            "textDocument/prepareRename" => {
                let result = self.prepare_rename_result(&params);
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

    fn hover_result(&mut self, params: &Value) -> Value {
        let Some((uri, lang, src)) = self.doc_text(params) else {
            return Value::Null;
        };
        if lang == "jungle" {
            return Value::Null;
        }
        let Some(off) = self.offset_param(params, &src) else {
            return Value::Null;
        };
        if let Some(h) = intel::hover(&src, off) {
            return h;
        }
        // Cross-file fallback: resolve the identifier under the cursor to a
        // project-wide declaration (preferring same-file decls).
        let Some((word, a, b)) = self.word_at(&src, off) else {
            return Value::Null;
        };
        let idx = self.project_index();
        let decls = idx.find_decls(&word);
        let Some((f, d)) = decls
            .iter()
            .find(|(f, _)| f.uri == uri)
            .or_else(|| decls.first())
        else {
            return Value::Null;
        };
        let extra = if f.uri == uri {
            None
        } else {
            Some(format!(
                " — declared in `{}`",
                f.path
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default()
            ))
        };
        intel::decl_hover(d, Span::new(a as u32, b as u32), &src, extra.as_deref())
    }

    /// Lazily build (or reuse) the project symbol index, overlaying the
    /// in-memory text of open documents so unsaved buffers are reflected.
    fn project_index(&mut self) -> Rc<index::ProjectIndex> {
        if self.index_dirty || self.index.is_none() {
            let root = self
                .workspace_root
                .clone()
                .or_else(|| {
                    self.docs
                        .keys()
                        .find_map(|u| index::uri_to_path(u))
                        .and_then(|p| p.parent().map(PathBuf::from))
                        // Never scan the filesystem root.
                        .filter(|d| d != std::path::Path::new("/"))
                })
                .unwrap_or_else(|| PathBuf::from("."));
            let mut idx = index::ProjectIndex::build(&root);
            for f in &mut idx.files {
                if let Some((_, text)) = self.docs.get(&f.uri) {
                    f.src = text.clone();
                    f.decls = intel::collect_decls(text);
                }
            }
            self.index = Some(Rc::new(idx));
            self.index_dirty = false;
        }
        self.index.as_ref().expect("index just built").clone()
    }

    /// Identifier word at a byte offset in `src`.
    fn word_at(&self, src: &str, offset: usize) -> Option<(String, usize, usize)> {
        intel::ident_at(src, offset).map(|(a, b)| (src[a..b].to_string(), a, b))
    }

    fn definition_result(&mut self, params: &Value) -> Value {
        let Some((uri, lang, src)) = self.doc_text(params) else {
            return json!([]);
        };
        if lang == "jungle" {
            return json!([]);
        }
        let _ = uri;
        let Some(off) = self.offset_param(params, &src) else {
            return json!([]);
        };
        let Some((word, _, _)) = self.word_at(&src, off) else {
            return json!([]);
        };
        let idx = self.project_index();
        let mut locs: Vec<Value> = idx
            .find_decls(&word)
            .iter()
            .map(|(f, d)| {
                json!({
                    "uri": f.uri,
                    "range": {
                        "start": position_at_byte(&f.src, d.name_span.start as usize),
                        "end": position_at_byte(&f.src, d.name_span.end as usize),
                    },
                })
            })
            .collect();
        locs.dedup();
        json!(locs)
    }

    fn formatting_result(&self, params: &Value) -> Value {
        let Some((_, lang, src)) = self.doc_text(params) else {
            return Value::Null;
        };
        intel::formatting(&src, lang == "jungle")
    }

    fn references_result(&mut self, params: &Value) -> Value {
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
        let Some(off) = self.offset_param(params, &src) else {
            return json!([]);
        };
        let Some((word, _, _)) = self.word_at(&src, off) else {
            return json!([]);
        };
        let idx = self.project_index();
        // Resolve the target decl (prefer same-file) to detect member kinds.
        let target = idx
            .find_decls(&word)
            .into_iter()
            .find(|(f, _)| f.uri == uri)
            .or_else(|| idx.find_decls(&word).into_iter().next());
        let (member, parent, kind) = target
            .map(|(_, d)| {
                (
                    matches!(d.kind, "method" | "field" | "enum-member"),
                    d.parent.clone(),
                    d.kind,
                )
            })
            .unwrap_or((false, None, ""));
        let is_local = matches!(kind, "variable" | "param");
        let mut locs: Vec<Value> = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for f in &idx.files {
            if is_local {
                // Locals/params are function-scoped: only files that declare
                // this name as a local, and only uses inside each
                // declaration's enclosing function.
                for (_, d) in idx.find_decls_in(&f.uri, &word) {
                    if !matches!(d.kind, "variable" | "param") {
                        continue;
                    }
                    let scope = intel::enclosing_fn_span(&f.src, d.name_span);
                    for span in intel::uses_in_mode(&f.src, &word, false, scope) {
                        if seen.insert((f.uri.clone(), span.start, span.end)) {
                            locs.push(json!({
                                "uri": f.uri,
                                "range": {
                                    "start": position_at_byte(&f.src, span.start as usize),
                                    "end": position_at_byte(&f.src, span.end as usize),
                                },
                            }));
                        }
                    }
                }
                continue;
            }
            let cs = if member {
                // Bare-ident uses only count inside the declaring class;
                // files without it get an empty span (member/path uses
                // still count everywhere).
                Some(
                    parent
                        .as_deref()
                        .and_then(|p| intel::class_span(&f.src, p))
                        .unwrap_or(Span::new(0, 0)),
                )
            } else {
                None
            };
            for span in intel::uses_in_mode(&f.src, &word, member, cs) {
                locs.push(json!({
                    "uri": f.uri,
                    "range": {
                        "start": position_at_byte(&f.src, span.start as usize),
                        "end": position_at_byte(&f.src, span.end as usize),
                    },
                }));
            }
        }
        if include_decl {
            for (f, d) in idx.find_decls(&word) {
                if is_local {
                    // Only the declaration(s) whose scope contains the cursor
                    // are this symbol — same-named locals elsewhere are not.
                    let in_scope = if f.uri == uri {
                        match intel::enclosing_fn_span(&f.src, d.name_span) {
                            Some(scope) => off >= scope.start as usize && off < scope.end as usize,
                            None => true, // top-level decl: file scope
                        }
                    } else {
                        false
                    };
                    if !in_scope {
                        continue;
                    }
                }
                locs.push(json!({
                    "uri": f.uri,
                    "range": {
                        "start": position_at_byte(&f.src, d.name_span.start as usize),
                        "end": position_at_byte(&f.src, d.name_span.end as usize),
                    },
                }));
            }
        }
        json!(locs)
    }

    fn rename_result(&mut self, params: &Value) -> Value {
        let Some((uri, lang, src)) = self.doc_text(params) else {
            return Value::Null;
        };
        if lang == "jungle" {
            return Value::Null;
        }
        let new_name = params.get("newName").and_then(Value::as_str).unwrap_or("");
        if !intel::valid_ident(new_name) {
            return Value::Null;
        }
        let Some(off) = self.offset_param(params, &src) else {
            return Value::Null;
        };
        let Some((word, _, _)) = self.word_at(&src, off) else {
            return Value::Null;
        };
        let idx = self.project_index();
        let found = idx.find_decls(&word);
        if found.is_empty() {
            return Value::Null;
        }
        // Member-aware rename: `.name`/path uses + decl-site edits.
        let target = found
            .iter()
            .find(|(f, _)| f.uri == uri)
            .or_else(|| found.first());
        let (member, parent, kind) = target
            .map(|(_, d)| {
                (
                    matches!(d.kind, "method" | "field" | "enum-member"),
                    d.parent.clone(),
                    d.kind,
                )
            })
            .unwrap_or((false, None, ""));
        let is_local = matches!(kind, "variable" | "param");
        let mut changes = serde_json::Map::new();
        for f in &idx.files {
            let mut edits: Vec<Value> = Vec::new();
            if is_local {
                // Locals/params: only decls in scope of the cursor, and uses
                // restricted to each declaration's enclosing function.
                let mut seen = std::collections::BTreeSet::new();
                for (_, d) in idx.find_decls_in(&f.uri, &word) {
                    if !matches!(d.kind, "variable" | "param") {
                        continue;
                    }
                    let in_scope = if f.uri == uri {
                        match intel::enclosing_fn_span(&f.src, d.name_span) {
                            Some(scope) => off >= scope.start as usize && off < scope.end as usize,
                            None => true,
                        }
                    } else {
                        false
                    };
                    if !in_scope {
                        continue;
                    }
                    if seen.insert((d.name_span.start, d.name_span.end)) {
                        edits.push(json!({
                            "range": {
                                "start": position_at_byte(&f.src, d.name_span.start as usize),
                                "end": position_at_byte(&f.src, d.name_span.end as usize),
                            },
                            "newText": new_name,
                        }));
                    }
                    let scope = intel::enclosing_fn_span(&f.src, d.name_span);
                    for span in intel::uses_in_mode(&f.src, &word, false, scope) {
                        if seen.insert((span.start, span.end)) {
                            edits.push(json!({
                                "range": {
                                    "start": position_at_byte(&f.src, span.start as usize),
                                    "end": position_at_byte(&f.src, span.end as usize),
                                },
                                "newText": new_name,
                            }));
                        }
                    }
                }
            } else {
                let cs = if member {
                    Some(
                        parent
                            .as_deref()
                            .and_then(|p| intel::class_span(&f.src, p))
                            .unwrap_or(Span::new(0, 0)),
                    )
                } else {
                    None
                };
                for span in intel::uses_in_mode(&f.src, &word, member, cs) {
                    edits.push(json!({
                        "range": {
                            "start": position_at_byte(&f.src, span.start as usize),
                            "end": position_at_byte(&f.src, span.end as usize),
                        },
                        "newText": new_name,
                    }));
                }
                for (_, d) in idx.find_decls_in(&f.uri, &word) {
                    edits.push(json!({
                        "range": {
                            "start": position_at_byte(&f.src, d.name_span.start as usize),
                            "end": position_at_byte(&f.src, d.name_span.end as usize),
                        },
                        "newText": new_name,
                    }));
                }
            }
            if !edits.is_empty() {
                edits.sort_by(|a, b| {
                    let ka = format!(
                        "{}{}",
                        a["range"]["start"]["line"], a["range"]["start"]["character"]
                    );
                    let kb = format!(
                        "{}{}",
                        b["range"]["start"]["line"], b["range"]["start"]["character"]
                    );
                    ka.cmp(&kb)
                });
                changes.insert(f.uri.clone(), json!(edits));
            }
        }
        if changes.is_empty() {
            return Value::Null;
        }
        json!({"changes": Value::Object(changes)})
    }

    fn signature_help_result(&self, params: &Value) -> Value {
        let Some((_, lang, src)) = self.doc_text(params) else {
            return Value::Null;
        };
        if lang == "jungle" {
            return Value::Null;
        }
        match self.offset_param(params, &src) {
            Some(off) => intel::signature_help(&src, off).unwrap_or(Value::Null),
            None => Value::Null,
        }
    }

    fn prepare_rename_result(&mut self, params: &Value) -> Value {
        let Some((uri, lang, src)) = self.doc_text(params) else {
            return Value::Null;
        };
        if lang == "jungle" {
            return Value::Null;
        }
        let _ = uri;
        let Some(off) = self.offset_param(params, &src) else {
            return Value::Null;
        };
        let Some((word, a, b)) = self.word_at(&src, off) else {
            return Value::Null;
        };
        let idx = self.project_index();
        if idx.find_decls(&word).is_empty() {
            return Value::Null;
        }
        json!({
            "range": {
                "start": position_at_byte(&src, a),
                "end": position_at_byte(&src, b),
            },
            "placeholder": word,
        })
    }

    /// Base identifier of a `X.` member-access prefix ending at `offset`,
    /// e.g. `view.` → `view`, `this.` → `this`.
    fn member_base(src: &str, offset: usize) -> Option<String> {
        let upto = offset.min(src.len());
        let line_start = src[..upto].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let prefix = src[line_start..upto].trim_end();
        let dot = prefix.rfind('.')?;
        let before = &prefix[..dot];
        let ident: String = before
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        (!ident.is_empty()).then_some(ident)
    }

    fn completion_result(&mut self, params: &Value) -> Value {
        let Some((_, lang, src)) = self.doc_text(params) else {
            return json!({"isIncomplete": false, "items": []});
        };
        if lang == "jungle" {
            return json!({"isIncomplete": false, "items": []});
        }
        // Member completion after `X.` — resolve X's declared type.
        if let Some(off) = self.offset_param(params, &src) {
            if let Some(base) = Self::member_base(&src, off) {
                return self.member_completion(&src, off, &base);
            }
        }
        // Keywords + file decls + project decl names.
        let mut result = intel::completion(&src);
        let idx = self.project_index();
        let mut seen: std::collections::HashSet<String> = result["items"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|i| i["label"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        for f in &idx.files {
            for d in &f.decls {
                if seen.insert(d.name.clone()) {
                    let kind = match d.kind {
                        "class" => 7,
                        "enum" => 13,
                        "method" => 2,
                        "function" => 3,
                        "field" => 5,
                        "enum-member" => 22,
                        _ => 6,
                    };
                    result["items"].as_array_mut().unwrap().push(json!({
                        "label": d.name,
                        "kind": kind,
                        "detail": format!("{} ({})", d.detail, f.path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()),
                        "insertText": d.name,
                    }));
                }
            }
        }
        result
    }

    /// Completion items for `base.` — class members via type annotations or
    /// `this` inside a class body.
    fn member_completion(&mut self, src: &str, offset: usize, base: &str) -> Value {
        let mut members: Vec<(String, &'static str)> = Vec::new();
        let class_name = if base == "this" {
            intel::class_at(src, offset).map(|c| c.name.text)
        } else {
            // Find the decl for `base`; use its declared type name.
            let decls = intel::collect_decls(src);
            decls
                .iter()
                .find(|d| d.name == base)
                .and_then(|d| d.type_name.clone())
                .or_else(|| {
                    // Maybe `base` is itself a class name.
                    decls
                        .iter()
                        .any(|d| d.name == base && d.kind == "class")
                        .then(|| base.to_string())
                })
        };
        if let Some(cname) = class_name {
            // The class may live in another file — search the index.
            let idx = self.project_index();
            for f in &idx.files {
                let found = intel::class_members(&f.src, &cname);
                if !found.is_empty() {
                    members = found;
                    break;
                }
            }
        }
        if members.is_empty() {
            members = self.toybox_members(base);
        }
        let items: Vec<Value> = members
            .iter()
            .map(|(name, kind)| {
                json!({
                    "label": name,
                    "kind": if *kind == "method" { 2 } else { 5 },
                    "detail": kind,
                    "insertText": name,
                })
            })
            .collect();
        json!({"isIncomplete": false, "items": items})
    }

    /// Toybox API members for `base.` — handles `Toybox.`, `Toybox.Math.`,
    /// and bare `Math.` (post-`using`) forms.
    fn toybox_members(&mut self, base: &str) -> Vec<(String, &'static str)> {
        if self.toybox.is_none() {
            self.toybox = Some(Rc::new(toybox::ToyboxDb::load()));
        }
        let Some(rc) = self.toybox.as_ref() else {
            return Vec::new();
        };
        let Some(db) = rc.as_ref() else {
            return Vec::new();
        };
        // Candidate owner paths, most specific first.
        let candidates = [
            format!("Toybox.{}", base),
            base.to_string(),
            format!("Toybox.{}.{}", base, base),
        ];
        for owner in &candidates {
            let ms = db.members_of(owner);
            if !ms.is_empty() {
                return ms
                    .into_iter()
                    .map(|m| {
                        // Toybox constants are UPPER_CASE; the rest are methods.
                        let kind = if m.chars().all(|c| !c.is_lowercase()) {
                            "field"
                        } else {
                            "method"
                        };
                        (m, kind)
                    })
                    .collect();
            }
        }
        // `Toybox.` itself → module names.
        if base == "Toybox" {
            return db
                .children_of("Toybox")
                .into_iter()
                .map(|m| (m, "module"))
                .collect();
        }
        Vec::new()
    }
}

/// Extract the workspace root from `initialize` params
/// (`workspaceFolders[0].uri`, falling back to `rootUri`).
fn workspace_root(params: &Value) -> Option<PathBuf> {
    if let Some(folders) = params.get("workspaceFolders").and_then(Value::as_array) {
        if let Some(uri) = folders
            .first()
            .and_then(|f| f.get("uri"))
            .and_then(Value::as_str)
        {
            return index::uri_to_path(uri);
        }
    }
    params
        .get("rootUri")
        .and_then(Value::as_str)
        .and_then(index::uri_to_path)
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

/// Apply an incremental LSP text edit (UTF-16 line/character range) to `src`.
/// Returns `None` if offsets can't be resolved.
fn apply_text_edit(
    src: &str,
    sline: u64,
    schar: u64,
    eline: u64,
    echar: u64,
    new_text: &str,
) -> Option<String> {
    let start = intel::offset_at(src, sline as usize, schar as usize);
    let end = intel::offset_at(src, eline as usize, echar as usize);
    if start > end || end > src.len() {
        return None;
    }
    let mut out = String::with_capacity(src.len() + new_text.len());
    out.push_str(&src[..start]);
    out.push_str(new_text);
    out.push_str(&src[end..]);
    Some(out)
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
            2 // incremental
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
    fn definition_and_references_across_files() {
        let dir = std::env::temp_dir().join(format!("monkey-lsp-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let a = "class Shared {\n  var c = 0;\n  function f() {\n    c = 1;\n  }\n}\n";
        let b = "function useIt() as Void {\n  var s = new Shared();\n}\n";
        std::fs::write(dir.join("a.mc"), a).unwrap();
        std::fs::write(dir.join("b.mc"), b).unwrap();
        let uri_a = index::path_to_uri(&dir.join("a.mc"));
        let uri_b = index::path_to_uri(&dir.join("b.mc"));

        let mut s = Server::new();
        s.handle(&msg(
            "initialize",
            json!({"workspaceFolders": [{"uri": index::path_to_uri(&dir), "name": "t"}]}),
            Some(1),
        ));
        s.handle(&msg(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": uri_a, "languageId": "monkeyc", "version": 1, "text": a}}),
            None,
        ));
        s.handle(&msg(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": uri_b, "languageId": "monkeyc", "version": 1, "text": b}}),
            None,
        ));

        // 'c' usage inside f() at line 3 → decl at line 1
        let out = s.handle(&msg(
            "textDocument/definition",
            json!({"textDocument": {"uri": uri_a}, "position": {"line": 3, "character": 5}}),
            Some(2),
        ));
        let resp = out.response.unwrap();
        let locs = resp["result"].as_array().unwrap().clone();
        assert_eq!(locs.len(), 1);
        assert_eq!(locs[0]["uri"], uri_a);
        assert_eq!(locs[0]["range"]["start"]["line"], 1);

        // 'Shared' usage in b.mc → decl in a.mc (cross-file)
        let out = s.handle(&msg(
            "textDocument/definition",
            json!({"textDocument": {"uri": uri_b}, "position": {"line": 1, "character": 16}}),
            Some(3),
        ));
        let resp = out.response.unwrap();
        let locs = resp["result"].as_array().unwrap().clone();
        assert_eq!(locs.len(), 1);
        assert_eq!(locs[0]["uri"], uri_a);
        assert_eq!(locs[0]["range"]["start"]["line"], 0);

        // references on 'Shared' → decl + the `new Shared()` use
        let out = s.handle(&msg(
            "textDocument/references",
            json!({"textDocument": {"uri": uri_b}, "position": {"line": 1, "character": 16},
                   "context": {"includeDeclaration": true}}),
            Some(4),
        ));
        let resp = out.response.unwrap();
        let locs = resp["result"].as_array().unwrap().clone();
        assert!(locs.len() >= 2, "expected decl + use, got {}", locs.len());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_local_is_function_scoped() {
        let src = "class A {\n  function f() {\n    var tmp = 1;\n    var y = tmp;\n  }\n  function g() {\n    var tmp = 2;\n    var z = tmp;\n  }\n}\n";
        let dir = std::env::temp_dir().join(format!("monkey-lsp-scope-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("scope.mc"), src).unwrap();
        let uri = index::path_to_uri(&dir.join("scope.mc"));
        let mut s = Server::new();
        s.handle(&msg(
            "initialize",
            json!({"workspaceFolders": [{"uri": index::path_to_uri(&dir), "name": "t"}]}),
            Some(0),
        ));
        s.handle(&msg(
            "textDocument/didOpen",
            json!({"textDocument": {"uri": uri, "languageId": "monkeyc", "version": 1, "text": src}}),
            None,
        ));
        // Rename `tmp` declared in f() — line 2, char 9.
        let out = s.handle(&msg(
            "textDocument/rename",
            json!({"textDocument": {"uri": uri},
                   "position": {"line": 2, "character": 9},
                   "newName": "renamed"}),
            Some(1),
        ));
        let resp = out.response.unwrap();
        let edits = resp["result"]["changes"][uri].as_array().unwrap();
        // f()'s decl + use only; g()'s same-named local must be untouched.
        assert_eq!(edits.len(), 2, "got {:?}", resp["result"]);
        let lines: Vec<u64> = edits
            .iter()
            .map(|e| e["range"]["start"]["line"].as_u64().unwrap())
            .collect();
        assert!(lines.iter().all(|l| *l <= 3), "lines: {:?}", lines);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
