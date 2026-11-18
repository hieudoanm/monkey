//! Language intelligence: document symbols, hover, definition, completion.
//!
//! Built on `monkey-parser`'s AST — declarations are collected via the
//! [`Visitor`] trait so every named node (classes, functions, fields, vars,
//! params, enum members) is indexed for lookups.

use monkey_core::Span;
use monkey_parser::ast::*;
use monkey_parser::visit::Visitor;
use serde_json::{json, Value};

use crate::position_at_byte;

// ---------- LSP SymbolKind / CompletionItemKind ----------
const KIND_METHOD: i64 = 6;
const KIND_FIELD: i64 = 8;
const KIND_CLASS: i64 = 5;
const KIND_ENUM: i64 = 10;
const KIND_FUNCTION: i64 = 12;
const KIND_VARIABLE: i64 = 13;
const KIND_ENUM_MEMBER: i64 = 22;
const KIND_KEYWORD: i64 = 14;

/// A named declaration found in the AST.
#[derive(Clone, Debug)]
pub struct Decl {
    pub name: String,
    /// "class" | "function" | "method" | "field" | "variable" | "param" | "enum" | "enum-member"
    pub kind: &'static str,
    /// One-line signature for hover/completion detail.
    pub detail: String,
    /// The identifier's own span.
    pub name_span: Span,
    /// Parent class name for methods/fields.
    pub parent: Option<String>,
}

fn type_text(t: &Option<TypeRef>) -> String {
    t.as_ref().map(type_ref_text).unwrap_or_default()
}

fn type_ref_text(t: &TypeRef) -> String {
    match t {
        TypeRef::Path { path, nullable, .. } => {
            let s: Vec<&str> = path.iter().map(|i| i.text.as_str()).collect();
            let mut out = s.join(".");
            if *nullable {
                out.push('?');
            }
            out
        }
        TypeRef::Array { elems, .. } => format!(
            "[{}]",
            elems
                .iter()
                .map(type_ref_text)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TypeRef::Union { variants, .. } => variants
            .iter()
            .map(type_ref_text)
            .collect::<Vec<_>>()
            .join(" or "),
    }
}

fn params_text(f: &FunctionDecl) -> String {
    let mut parts = Vec::new();
    for p in &f.params {
        let mut s = p.name.text.clone();
        if let Some(t) = &p.type_ann {
            s.push_str(" as ");
            s.push_str(&type_text(&Some(t.clone())));
        }
        parts.push(s);
    }
    parts.join(", ")
}

fn function_detail(f: &FunctionDecl, kind: &str) -> String {
    let mut s = format!("{} {}({})", kind, f.name.text, params_text(f));
    if let Some(r) = &f.return_type {
        s.push_str(" as ");
        s.push_str(&type_text(&Some(r.clone())));
    }
    s
}

struct Collector {
    decls: Vec<Decl>,
    class_stack: Vec<String>,
}

impl Collector {
    fn push(&mut self, name: &Ident, kind: &'static str, detail: String) {
        self.decls.push(Decl {
            name: name.text.clone(),
            kind,
            detail,
            name_span: name.span,
            parent: self.class_stack.last().cloned(),
        });
    }
}

impl Visitor for Collector {
    fn visit_class(&mut self, d: &ClassDecl) {
        self.push(&d.name, "class", format!("class {}", d.name.text));
        self.class_stack.push(d.name.text.clone());
        // Default recursion for members/annotations.
        for a in &d.annotations {
            self.visit_annotation(a);
        }
        if let Some(s) = &d.superclass {
            self.visit_type(s);
        }
        for m in &d.members {
            self.visit_class_member(m);
        }
        self.class_stack.pop();
    }

    fn visit_field(&mut self, f: &FieldDecl) {
        let ty = type_text(&f.type_ann);
        let detail = if ty.is_empty() {
            format!("var {}", f.name.text)
        } else {
            format!("var {}: {}", f.name.text, ty)
        };
        self.push(&f.name, "field", detail);
        for a in &f.annotations {
            self.visit_annotation(a);
        }
        if let Some(i) = &f.init {
            self.visit_expr(i);
        }
    }

    fn visit_function(&mut self, f: &FunctionDecl) {
        let in_class = self.class_stack.last().is_some();
        let kind = if in_class { "method" } else { "function" };
        self.push(&f.name, kind, function_detail(f, kind));
        for a in &f.annotations {
            self.visit_annotation(a);
        }
        for p in &f.params {
            self.visit_param(p);
        }
        if let Some(r) = &f.return_type {
            self.visit_type(r);
        }
        self.visit_block(&f.body);
    }

    fn visit_param(&mut self, p: &Param) {
        let ty = type_text(&p.type_ann);
        let detail = if ty.is_empty() {
            p.name.text.clone()
        } else {
            format!("{}: {}", p.name.text, ty)
        };
        self.push(&p.name, "param", detail);
    }

    fn visit_enum(&mut self, d: &EnumDecl) {
        self.push(&d.name, "enum", format!("enum {}", d.name.text));
        for m in &d.members {
            self.visit_enum_member(m);
        }
    }

    fn visit_enum_member(&mut self, m: &EnumMember) {
        self.push(&m.name, "enum-member", m.name.text.clone());
        if let Some(v) = &m.value {
            self.visit_expr(v);
        }
    }

    fn visit_var_decl(&mut self, d: &VarDecl) {
        let ty = type_text(&d.type_ann);
        let kw = if d.mutable { "var" } else { "const" };
        let detail = if ty.is_empty() {
            format!("{} {}", kw, d.name.text)
        } else {
            format!("{} {}: {}", kw, d.name.text, ty)
        };
        self.push(&d.name, "variable", detail);
        for a in &d.annotations {
            self.visit_annotation(a);
        }
        if let Some(i) = &d.init {
            self.visit_expr(i);
        }
    }
}

/// Collect every named declaration in `src` (parsed on demand).
pub fn collect_decls(src: &str) -> Vec<Decl> {
    let parsed = monkey_parser::parse(src);
    let mut c = Collector {
        decls: Vec::new(),
        class_stack: Vec::new(),
    };
    c.visit_source_file(&parsed.file);
    c.decls
}

// ---------- position helpers ----------

/// LSP position (0-based line, UTF-16 col) → UTF-8 byte offset.
pub fn offset_at(src: &str, line: usize, utf16_col: usize) -> usize {
    let mut cur_line = 0usize;
    let mut line_start = 0usize; // byte index of line start
    for (i, ch) in src.char_indices() {
        if cur_line == line {
            break;
        }
        if ch == '\n' {
            cur_line += 1;
            line_start = i + 1;
        }
    }
    if cur_line != line {
        return src.len();
    }
    // Walk the line counting UTF-16 units.
    let mut col = 0usize;
    for (i, ch) in src[line_start..].char_indices() {
        if col >= utf16_col {
            return line_start + i;
        }
        col += ch.len_utf16();
    }
    src.len()
}

fn span_contains(span: Span, offset: usize) -> bool {
    offset >= span.start as usize && offset < span.end as usize
}

fn range_json(src: &str, span: Span) -> Value {
    json!({
        "start": position_at_byte(src, span.start as usize),
        "end": position_at_byte(src, span.end as usize),
    })
}

fn ident_at(src: &str, offset: usize) -> Option<(usize, usize)> {
    let bytes = src.as_bytes();
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut start = offset;
    while start > 0 && is_ident(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = offset;
    while end < bytes.len() && is_ident(bytes[end]) {
        end += 1;
    }
    (start < end).then_some((start, end))
}

// ---------- features ----------

/// `textDocument/documentSymbol` → hierarchical DocumentSymbol[].
pub fn document_symbols(src: &str) -> Value {
    let parsed = monkey_parser::parse(src);
    let syms = symbols_for_items(&parsed.file.items);
    json!(syms.iter().map(|s| s.to_json(src)).collect::<Vec<_>>())
}

struct Sym {
    name: String,
    detail: String,
    kind: i64,
    span: Span,
    name_span: Span,
    children: Vec<Sym>,
}

impl Sym {
    fn to_json(&self, src: &str) -> Value {
        json!({
            "name": self.name,
            "detail": self.detail,
            "kind": self.kind,
            "range": range_json(src, self.span),
            "selectionRange": range_json(src, self.name_span),
            "children": self.children.iter().map(|c| c.to_json(src)).collect::<Vec<_>>(),
        })
    }
}

fn member_kind(f: &FieldDecl) -> i64 {
    if f.mutable {
        KIND_FIELD
    } else {
        KIND_VARIABLE
    }
}

fn symbols_for_items(items: &[Item]) -> Vec<Sym> {
    let mut out = Vec::new();
    for item in items {
        match item {
            Item::Class(d) => {
                let mut children = Vec::new();
                for m in &d.members {
                    match m {
                        ClassMember::Field(f) => children.push(Sym {
                            name: f.name.text.clone(),
                            detail: format!("var {}", f.name.text),
                            kind: member_kind(f),
                            span: f.span,
                            name_span: f.name.span,
                            children: vec![],
                        }),
                        ClassMember::Function(f) => children.push(Sym {
                            name: f.name.text.clone(),
                            detail: function_detail(f, "method"),
                            kind: KIND_METHOD,
                            span: f.span,
                            name_span: f.name.span,
                            children: vec![],
                        }),
                        ClassMember::Error { .. } => {}
                    }
                }
                out.push(Sym {
                    name: d.name.text.clone(),
                    detail: format!("class {}", d.name.text),
                    kind: KIND_CLASS,
                    span: d.span,
                    name_span: d.name.span,
                    children,
                });
            }
            Item::Enum(d) => {
                let children = d
                    .members
                    .iter()
                    .map(|m| Sym {
                        name: m.name.text.clone(),
                        detail: m.name.text.clone(),
                        kind: KIND_ENUM_MEMBER,
                        span: m.span,
                        name_span: m.name.span,
                        children: vec![],
                    })
                    .collect();
                out.push(Sym {
                    name: d.name.text.clone(),
                    detail: format!("enum {}", d.name.text),
                    kind: KIND_ENUM,
                    span: d.span,
                    name_span: d.name.span,
                    children,
                });
            }
            Item::Function(f) => out.push(Sym {
                name: f.name.text.clone(),
                detail: function_detail(f, "function"),
                kind: KIND_FUNCTION,
                span: f.span,
                name_span: f.name.span,
                children: vec![],
            }),
            Item::VarDecl(v) => out.push(Sym {
                name: v.name.text.clone(),
                detail: format!("var {}", v.name.text),
                kind: KIND_VARIABLE,
                span: v.span,
                name_span: v.name.span,
                children: vec![],
            }),
            _ => {}
        }
    }
    out
}

/// `textDocument/hover` — markdown info for the declaration under `offset`.
pub fn hover(src: &str, offset: usize) -> Option<Value> {
    let decls = collect_decls(src);
    // Innermost match: smallest name span containing the offset.
    let decl = decls
        .iter()
        .filter(|d| span_contains(d.name_span, offset))
        .min_by_key(|d| d.name_span.len())?;
    let kind_label = match decl.kind {
        "class" => "class",
        "enum" => "enum",
        "method" => "method",
        "function" => "function",
        "field" => "field",
        "variable" => "variable",
        "param" => "parameter",
        "enum-member" => "enum member",
        _ => decl.kind,
    };
    let prefix = format!("{} ", decl.kind);
    let sig = decl.detail.strip_prefix(&prefix).unwrap_or(&decl.detail);
    let mut text = format!("**{}** `{}`", kind_label, sig);
    if let Some(p) = &decl.parent {
        text.push_str(&format!(" — member of `{}`", p));
    }
    Some(json!({
        "contents": {"kind": "markdown", "value": text},
        "range": range_json(src, decl.name_span),
    }))
}

/// `textDocument/definition` — same-file jump to the declaration.
pub fn definition(src: &str, uri: &str, offset: usize) -> Value {
    let Some((start, end)) = ident_at(src, offset) else {
        return json!([]);
    };
    let word = &src[start..end];
    let decls = collect_decls(src);
    let mut locations: Vec<Value> = decls
        .iter()
        .filter(|d| d.name == word && d.name_span.start as usize != start)
        .map(|d| {
            json!({
                "uri": uri,
                "range": range_json(src, d.name_span),
            })
        })
        .collect();
    locations.dedup_by(|a, b| a == b);
    // If the cursor is on the declaration itself, still offer its location.
    if locations.is_empty() {
        if let Some(d) = decls.iter().find(|d| d.name == word) {
            locations.push(json!({
                "uri": uri,
                "range": range_json(src, d.name_span),
            }));
        }
    }
    json!(locations)
}

/// `textDocument/completion` — keywords + declared names in the file.
pub fn completion(src: &str) -> Value {
    let decls = collect_decls(src);
    let mut seen = std::collections::HashSet::new();
    let mut items: Vec<Value> = Vec::new();
    let mut push = |label: &str, kind: i64, detail: &str, items: &mut Vec<Value>| {
        if seen.insert(label.to_string()) {
            items.push(json!({
                "label": label,
                "kind": kind,
                "detail": detail,
                "insertText": label,
            }));
        }
    };
    for kw in monkey_syntax::KEYWORDS {
        push(kw, KIND_KEYWORD, "keyword", &mut items);
    }
    for d in &decls {
        let kind = match d.kind {
            "class" => KIND_CLASS,
            "enum" => KIND_ENUM,
            "method" => KIND_METHOD,
            "function" => KIND_FUNCTION,
            "field" => KIND_FIELD,
            "enum-member" => KIND_ENUM_MEMBER,
            _ => KIND_VARIABLE,
        };
        push(&d.name, kind, &d.detail, &mut items);
    }
    json!({"isIncomplete": false, "items": items})
}

// ---------- references ----------

/// Collects every identifier *use* (not declaration) matching `name`.
struct UseCollector {
    name: String,
    uses: Vec<Span>,
}

impl Visitor for UseCollector {
    fn visit_expr(&mut self, e: &Expr) {
        match e {
            Expr::Ident(i) if i.text == self.name => self.uses.push(i.span),
            _ => {}
        }
        // Default recursion into children.
        match e {
            Expr::Missing { .. }
            | Expr::Literal(_)
            | Expr::This { .. }
            | Expr::Super { .. }
            | Expr::Ident(_)
            | Expr::Path { .. } => {}
            Expr::Call { callee, args, .. } => {
                self.visit_expr(callee);
                for a in args {
                    self.visit_expr(a);
                }
            }
            Expr::Member { object, .. } => self.visit_expr(object),
            Expr::Index { object, index, .. } => {
                self.visit_expr(object);
                self.visit_expr(index);
            }
            Expr::Unary { expr, .. } | Expr::Postfix { expr, .. } | Expr::Paren { expr, .. } => {
                self.visit_expr(expr)
            }
            Expr::Binary { left, right, .. }
            | Expr::Assign {
                target: left,
                value: right,
                ..
            } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            Expr::Ternary {
                cond, then, else_, ..
            } => {
                self.visit_expr(cond);
                self.visit_expr(then);
                self.visit_expr(else_);
            }
            Expr::New { args, type_, .. } => {
                self.visit_type(type_);
                for a in args {
                    self.visit_expr(a);
                }
            }
            Expr::Cast { expr, type_, .. } => {
                self.visit_expr(expr);
                self.visit_type(type_);
            }
            Expr::Array { elems, .. } => {
                for e in elems {
                    self.visit_expr(e);
                }
            }
            Expr::Dict { entries, .. } => {
                for (k, v) in entries {
                    self.visit_expr(k);
                    self.visit_expr(v);
                }
            }
        }
    }

    fn visit_type(&mut self, t: &TypeRef) {
        // `new Foo()` / `as Foo` type paths count as uses of the type name.
        match t {
            TypeRef::Path { path, .. } => {
                if let Some(last) = path.last() {
                    if last.text == self.name {
                        self.uses.push(last.span);
                    }
                }
            }
            TypeRef::Array { elems, .. } => {
                for e in elems {
                    self.visit_type(e);
                }
            }
            TypeRef::Union { variants, .. } => {
                for v in variants {
                    self.visit_type(v);
                }
            }
        }
    }
}

/// `textDocument/references` — same-file uses of the symbol at `offset`.
/// Returns `Location[]`; includes the declaration when `include_decl`.
pub fn references(src: &str, uri: &str, offset: usize, include_decl: bool) -> Value {
    let Some((start, _)) = ident_at(src, offset) else {
        return json!([]);
    };
    let word = src[start..]
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>();
    let decls = collect_decls(src);
    let Some(decl) = decls.iter().find(|d| d.name == word) else {
        return json!([]);
    };
    let parsed = monkey_parser::parse(src);
    let mut c = UseCollector {
        name: word.clone(),
        uses: Vec::new(),
    };
    c.visit_source_file(&parsed.file);
    let mut spans: Vec<Span> = c.uses;
    spans.retain(|s| s.start as usize != decl.name_span.start as usize);
    spans.sort_by_key(|s| s.start);
    spans.dedup();
    let mut locations: Vec<Value> = spans
        .iter()
        .map(|s| json!({"uri": uri, "range": range_json(src, *s)}))
        .collect();
    if include_decl {
        locations.push(json!({"uri": uri, "range": range_json(src, decl.name_span)}));
    }
    json!(locations)
}

/// `textDocument/formatting` — full-document format as a single TextEdit.
/// Returns `null` when the text is already formatted.
pub fn formatting(src: &str, is_jungle: bool) -> Value {
    let formatted = if is_jungle {
        monkey_formatter::format_jungle(src, &Default::default())
    } else {
        monkey_formatter::format_monkeyc(src, &Default::default())
    };
    if formatted == src {
        return Value::Null;
    }
    let end = position_at_byte(src, src.len());
    json!([{
        "range": {"start": {"line": 0, "character": 0}, "end": end},
        "newText": formatted,
    }])
}

/// `textDocument/rename` — same-file rename as a WorkspaceEdit.
/// Returns `None` when the position isn't on a declared symbol or the new
/// name isn't a valid identifier.
pub fn rename(src: &str, uri: &str, offset: usize, new_name: &str) -> Option<Value> {
    if new_name.is_empty()
        || !new_name
            .chars()
            .next()
            .map(|c| c.is_alphabetic() || c == '_')
            .unwrap_or(false)
        || !new_name.chars().all(|c| c.is_alphanumeric() || c == '_')
    {
        return None;
    }
    let (start, _) = ident_at(src, offset)?;
    let word: String = src[start..]
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if word.is_empty() {
        return None;
    }
    let decls = collect_decls(src);
    let decl = decls.iter().find(|d| d.name == word)?;
    let parsed = monkey_parser::parse(src);
    let mut c = UseCollector {
        name: word.clone(),
        uses: Vec::new(),
    };
    c.visit_source_file(&parsed.file);
    let mut spans = c.uses;
    spans.retain(|s| s.start as usize != decl.name_span.start as usize);
    spans.push(decl.name_span);
    spans.sort_by_key(|s| s.start);
    spans.dedup();
    let edits: Vec<Value> = spans
        .iter()
        .map(|s| {
            json!({
                "range": range_json(src, *s),
                "newText": new_name,
            })
        })
        .collect();
    Some(json!({"changes": {uri: edits}}))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "class App extends WatchUi.WatchFace {\n  var counter = 0;\n  function onUpdate(dc as Dc) as Void {\n    counter += 1;\n    var local = counter;\n  }\n}\nfunction helper() as Void {\n  var x = 1;\n}\n";

    #[test]
    fn collects_all_decls() {
        let decls = collect_decls(SRC);
        let names: Vec<&str> = decls.iter().map(|d| d.name.as_str()).collect();
        for expected in ["App", "counter", "onUpdate", "dc", "local", "helper", "x"] {
            assert!(names.contains(&expected), "missing {}", expected);
        }
        let m = decls.iter().find(|d| d.name == "onUpdate").unwrap();
        assert_eq!(m.kind, "method");
        assert_eq!(m.parent.as_deref(), Some("App"));
    }

    #[test]
    fn offset_at_roundtrip() {
        let src = "class A {}\nfunction f() {}\n";
        let off = offset_at(src, 1, 0);
        assert_eq!(&src[off..off + 8], "function");
    }

    #[test]
    fn hover_finds_method() {
        let offset = SRC.find("onUpdate").unwrap();
        let h = hover(SRC, offset).unwrap();
        let text = h["contents"]["value"].as_str().unwrap();
        assert!(text.contains("method"), "got: {}", text);
        assert!(text.contains("onUpdate"));
        assert!(text.contains("App"));
    }

    #[test]
    fn hover_param() {
        let offset = SRC.find("dc as Dc").unwrap();
        let h = hover(SRC, offset).unwrap();
        assert!(h["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("parameter"));
    }

    #[test]
    fn definition_same_file() {
        // 'counter' usage inside onUpdate → field decl
        let use_offset = SRC.rfind("counter").unwrap();
        let defs = definition(SRC, "file:///a.mc", use_offset);
        let arr = defs.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["uri"], "file:///a.mc");
    }

    #[test]
    fn completion_has_keywords_and_decls() {
        let c = completion(SRC);
        let labels: Vec<&str> = c["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|i| i["label"].as_str())
            .collect();
        assert!(labels.contains(&"class"));
        assert!(labels.contains(&"App"));
        assert!(labels.contains(&"onUpdate"));
        assert!(labels.contains(&"counter"));
    }

    #[test]
    fn document_symbols_hierarchy() {
        let syms = document_symbols(SRC);
        let arr = syms.as_array().unwrap();
        assert_eq!(arr.len(), 2); // App class + helper function
        assert_eq!(arr[0]["name"], "App");
        assert_eq!(arr[0]["kind"], KIND_CLASS);
        let children = arr[0]["children"].as_array().unwrap();
        assert!(children
            .iter()
            .any(|c| c["name"] == "onUpdate" && c["kind"] == KIND_METHOD));
        assert!(children.iter().any(|c| c["name"] == "counter"));
    }

    #[test]
    fn references_finds_uses() {
        let src = "class A {\n  var c = 0;\n  function f() {\n    c = 1;\n    c = c + 1;\n  }\n}\n";
        let off = src.rfind("c + 1").unwrap();
        let refs = references(src, "file:///a.mc", off, true);
        let arr = refs.as_array().unwrap();
        // decl + 3 uses (c=1, c=c, and 'c' inside 'c + 1'... actually uses: 'c = 1', 'c =' lhs, 'c +' rhs)
        assert!(arr.len() >= 3, "got {} locations", arr.len());
    }

    #[test]
    fn formatting_returns_edit_or_null() {
        let messy = "class A{\nfunction f()as Void{var x=1;}\n}\n";
        let r = formatting(messy, false);
        assert!(r.is_array());
        assert!(r[0]["newText"].as_str().unwrap().contains("function f()"));
        let clean = "var x = 1;\n";
        assert!(formatting(clean, false).is_null());
    }

    #[test]
    fn rename_produces_workspace_edit() {
        let src = "class A {\n  var c = 0;\n  function f() {\n    c = 1;\n    c = c + 1;\n  }\n}\n";
        let off = src.rfind("c + 1").unwrap();
        let edit = rename(src, "file:///a.mc", off, "count").unwrap();
        let edits = edit["changes"]["file:///a.mc"].as_array().unwrap();
        // decl + uses
        assert!(edits.len() >= 4, "got {}", edits.len());
        assert!(edits.iter().all(|e| e["newText"] == "count"));
        assert!(rename(src, "file:///a.mc", off, "1bad").is_none());
        assert!(rename(src, "file:///a.mc", off, "has space").is_none());
    }
}
