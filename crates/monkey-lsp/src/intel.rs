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
    /// Declared type name for vars/params/fields (last path segment).
    pub type_name: Option<String>,
    /// Parameter list for functions/methods (`"name as Type"` strings).
    pub params: Vec<String>,
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
        TypeRef::Generic { path, args, .. } => {
            let s: Vec<&str> = path.iter().map(|i| i.text.as_str()).collect();
            format!(
                "{}<{}>",
                s.join("."),
                args.iter()
                    .map(type_ref_text)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        TypeRef::Dict { entries, .. } => format!(
            "{{ {} }}",
            entries
                .iter()
                .map(|e| format!(":{} as {}", e.name.text, type_ref_text(&e.type_)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TypeRef::Method {
            path, params, ret, ..
        } => {
            let s: Vec<&str> = path.iter().map(|i| i.text.as_str()).collect();
            let mut out = format!(
                "{}({})",
                s.join("."),
                params
                    .iter()
                    .map(type_ref_text)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            if let Some(r) = ret {
                out.push_str(&format!(" as {}", type_ref_text(r)));
            }
            out
        }
    }
}

fn param_text(p: &Param) -> String {
    let mut s = p.name.text.clone();
    if let Some(t) = &p.type_ann {
        s.push_str(" as ");
        s.push_str(&type_ref_text(t));
    }
    s
}

fn params_text(f: &FunctionDecl) -> String {
    f.params
        .iter()
        .map(param_text)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Last path segment of a `TypeRef::Path`, e.g. `Dc` from `Toybox.Dc`.
fn type_name_of(t: &TypeRef) -> Option<String> {
    match t {
        TypeRef::Path { path, .. } => path.last().map(|i| i.text.clone()),
        _ => None,
    }
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
            type_name: None,
            params: Vec::new(),
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
        let n = self.decls.len();
        self.push(&f.name, "field", detail);
        self.decls[n].type_name =
            f.type_ann
                .as_ref()
                .and_then(type_name_of)
                .or_else(|| match &f.init {
                    Some(Expr::New { type_, .. }) => type_name_of(type_),
                    _ => None,
                });
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
        let n = self.decls.len();
        self.push(&f.name, kind, function_detail(f, kind));
        self.decls[n].params = f.params.iter().map(param_text).collect();
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
        let n = self.decls.len();
        self.push(&p.name, "param", detail);
        self.decls[n].type_name = p.type_ann.as_ref().and_then(type_name_of);
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
        let n = self.decls.len();
        self.push(&d.name, "variable", detail);
        self.decls[n].type_name =
            d.type_ann
                .as_ref()
                .and_then(type_name_of)
                .or_else(|| match &d.init {
                    Some(Expr::New { type_, .. }) => type_name_of(type_),
                    _ => None,
                });
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

fn fn_spans_item(item: &Item, out: &mut Vec<Span>) {
    match item {
        Item::Function(f) => out.push(f.span),
        Item::Class(c) => fn_spans_class(c, out),
        Item::Module(m) => {
            for i in &m.members {
                fn_spans_item(i, out);
            }
        }
        _ => {}
    }
}

fn fn_spans_class(c: &ClassDecl, out: &mut Vec<Span>) {
    for m in &c.members {
        match m {
            ClassMember::Function(f) => out.push(f.span),
            ClassMember::Class(inner) => fn_spans_class(inner, out),
            _ => {}
        }
    }
}

/// Span of the innermost function/method containing `inner`, if any.
/// Used to scope local variable/param references to their function body.
pub fn enclosing_fn_span(src: &str, inner: Span) -> Option<Span> {
    let parsed = monkey_parser::parse(src);
    let mut spans = Vec::new();
    for i in &parsed.file.items {
        fn_spans_item(i, &mut spans);
    }
    spans
        .into_iter()
        .filter(|s| inner.start >= s.start && inner.end <= s.end && s.end - s.start > 0)
        .min_by_key(|s| s.end - s.start)
}

fn range_json(src: &str, span: Span) -> Value {
    json!({
        "start": position_at_byte(src, span.start as usize),
        "end": position_at_byte(src, span.end as usize),
    })
}

pub(crate) fn ident_at(src: &str, offset: usize) -> Option<(usize, usize)> {
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
                        ClassMember::Enum(d) => children.push(Sym {
                            name: d.name.text.clone(),
                            detail: format!("enum {}", d.name.text),
                            kind: KIND_ENUM,
                            span: d.span,
                            name_span: d.name.span,
                            children: vec![],
                        }),
                        ClassMember::Class(c) => children.push(Sym {
                            name: c.name.text.clone(),
                            detail: format!("class {}", c.name.text),
                            kind: KIND_CLASS,
                            span: c.span,
                            name_span: c.name.span,
                            children: vec![],
                        }),
                        ClassMember::Typedef(t) => children.push(Sym {
                            name: t.name.text.clone(),
                            detail: format!("typedef {}", t.name.text),
                            kind: KIND_CLASS,
                            span: t.span,
                            name_span: t.name.span,
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
            Item::Module(d) => out.extend(symbols_for_items(&d.members)),
            _ => {}
        }
    }
    out
}

/// `textDocument/hover` — markdown info for the declaration under `offset`.
/// Falls back to resolving the identifier *use* at `offset` to a decl.
pub fn hover(src: &str, offset: usize) -> Option<Value> {
    let decls = collect_decls(src);
    // Innermost match: smallest name span containing the offset.
    if let Some(decl) = decls
        .iter()
        .filter(|d| span_contains(d.name_span, offset))
        .min_by_key(|d| d.name_span.len())
    {
        return Some(decl_hover(decl, decl.name_span, src, None));
    }
    // Usage position: resolve the identifier to a declaration by name.
    let (s, e) = ident_at(src, offset)?;
    let name = &src[s..e];
    let decl = decls
        .iter()
        .filter(|d| d.name == name)
        .min_by_key(|d| d.name_span.len())?;
    Some(decl_hover(decl, Span::new(s as u32, e as u32), src, None))
}

/// Hover JSON for an arbitrary declaration at `span` (usually a usage).
/// `extra` appends a suffix like ` — declared in `b.mc``.
pub fn decl_hover(decl: &Decl, span: Span, src: &str, extra: Option<&str>) -> Value {
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
    if let Some(x) = extra {
        text.push_str(x);
    }
    json!({
        "contents": {"kind": "markdown", "value": text},
        "range": range_json(src, span),
    })
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
///
/// `member` mode is for methods/fields/enum-members: `.name` member accesses
/// and path segments (`Enum.MEMBER`) count as uses, while bare identifiers
/// only count inside the declaring class body (self-calls like `draw()`).
struct UseCollector {
    name: String,
    member: bool,
    /// Span of the declaring class body; bare-ident uses outside it are
    /// ignored in member mode.
    class_span: Option<Span>,
    uses: Vec<Span>,
}

impl Visitor for UseCollector {
    fn visit_expr(&mut self, e: &Expr) {
        match e {
            Expr::Ident(i) if i.text == self.name => {
                // `class_span` doubles as a scope restriction: member mode
                // uses it for the declaring class; local mode for the
                // enclosing function. `None` means unrestricted.
                let inside = match self.class_span {
                    Some(cs) => span_contains(cs, i.span.start as usize),
                    None => true,
                };
                if inside {
                    self.uses.push(i.span);
                }
            }
            Expr::Path { segments, .. } => {
                // `Foo.bar` / `Enum.MEMBER` — any segment may be the use.
                // Same scope rules as bare idents (single-segment paths are
                // how plain identifiers often parse).
                for seg in segments {
                    if seg.text == self.name {
                        let inside = match self.class_span {
                            Some(cs) => span_contains(cs, seg.span.start as usize),
                            None => true,
                        };
                        if inside {
                            self.uses.push(seg.span);
                        }
                    }
                }
            }
            Expr::Member { name, .. } if self.member && name.text == self.name => {
                self.uses.push(name.span);
            }
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
            TypeRef::Generic { path, args, .. } => {
                if let Some(last) = path.last() {
                    if last.text == self.name {
                        self.uses.push(last.span);
                    }
                }
                for a in args {
                    self.visit_type(a);
                }
            }
            TypeRef::Dict { entries, .. } => {
                for e in entries {
                    self.visit_type(&e.type_);
                }
            }
            TypeRef::Method {
                path, params, ret, ..
            } => {
                if let Some(last) = path.last() {
                    if last.text == self.name {
                        self.uses.push(last.span);
                    }
                }
                for p in params {
                    self.visit_type(p);
                }
                if let Some(r) = ret {
                    self.visit_type(r);
                }
            }
        }
    }
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

/// Span of the class declaration named `name` in `src`, if any.
pub fn class_span(src: &str, name: &str) -> Option<Span> {
    let parsed = monkey_parser::parse(src);
    parsed.file.items.iter().find_map(|i| match i {
        Item::Class(c) if c.name.text == name => Some(c.span),
        _ => None,
    })
}

/// Uses of `name` with member-awareness: when `member` is set, `.name`
/// accesses and path segments count, and bare identifiers are restricted to
/// `class_span` (the declaring class body) when provided.
pub fn uses_in_mode(src: &str, name: &str, member: bool, class_span: Option<Span>) -> Vec<Span> {
    let parsed = monkey_parser::parse(src);
    let mut c = UseCollector {
        name: name.to_string(),
        member,
        class_span,
        uses: Vec::new(),
    };
    c.visit_source_file(&parsed.file);
    c.uses.sort_by_key(|s| s.start);
    c.uses.dedup();
    c.uses
}

/// The class declaration whose body contains `offset`, if any.
pub fn class_at(src: &str, offset: usize) -> Option<ClassDecl> {
    let parsed = monkey_parser::parse(src);
    parsed.file.items.iter().find_map(|i| match i {
        Item::Class(c) if offset >= c.span.start as usize && offset < c.span.end as usize => {
            Some(c.clone())
        }
        _ => None,
    })
}

/// Member names (fields + methods) of a class declared in `src`.
pub fn class_members(src: &str, class_name: &str) -> Vec<(String, &'static str)> {
    let parsed = monkey_parser::parse(src);
    for item in &parsed.file.items {
        if let Item::Class(c) = item {
            if c.name.text == class_name {
                return c
                    .members
                    .iter()
                    .filter_map(|m| match m {
                        ClassMember::Field(f) => Some((f.name.text.clone(), "field")),
                        ClassMember::Function(f) => Some((f.name.text.clone(), "method")),
                        ClassMember::Enum(_) | ClassMember::Class(_) | ClassMember::Typedef(_) => {
                            None
                        }
                        _ => None,
                    })
                    .collect();
            }
        }
    }
    Vec::new()
}

/// `textDocument/signatureHelp` — signature of the call enclosing `offset`.
pub fn signature_help(src: &str, offset: usize) -> Option<Value> {
    let parsed = monkey_parser::parse(src);
    // Innermost Call whose arg list contains the offset.
    struct Finder {
        offset: usize,
        best: Option<(Span, String)>,
    }
    impl Visitor for Finder {
        fn visit_expr(&mut self, e: &Expr) {
            if let Expr::Call { span, callee, .. } = e {
                let inside = offset_in(span, self.offset);
                if inside {
                    let name = callee_name(callee);
                    if !name.is_empty() {
                        let replace = match self.best {
                            Some((b, _)) => span.start >= b.start,
                            None => true,
                        };
                        if replace {
                            self.best = Some((*span, name));
                        }
                    }
                }
            }
            // default recursion
            match e {
                Expr::Missing { .. }
                | Expr::Literal(_)
                | Expr::Ident(_)
                | Expr::This { .. }
                | Expr::Super { .. }
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
                Expr::Unary { expr, .. }
                | Expr::Postfix { expr, .. }
                | Expr::Paren { expr, .. } => self.visit_expr(expr),
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
                Expr::New { args, .. } => {
                    for a in args {
                        self.visit_expr(a);
                    }
                }
                Expr::Cast { expr, .. } => self.visit_expr(expr),
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
    }
    let mut f = Finder { offset, best: None };
    f.visit_source_file(&parsed.file);
    let (call_span, name) = f.best?;
    let span = call_span;

    // Resolve the callee to a declaration for its parameter list.
    let decls = collect_decls(src);
    let decl = decls
        .iter()
        .find(|d| d.name == name && (d.kind == "function" || d.kind == "method"))
        .or_else(|| decls.iter().find(|d| d.name == name));
    let (label, params) = match decl {
        Some(d) => {
            let sig = d
                .detail
                .strip_prefix(&format!("{} ", d.kind))
                .unwrap_or(&d.detail);
            (sig.to_string(), d.params.clone())
        }
        None => (format!("{}()", name), Vec::new()),
    };

    // Active parameter: count top-level commas between '(' and offset.
    let active = active_parameter(src, span, offset);
    let parameters: Vec<Value> = params.iter().map(|p| json!({"label": p})).collect();
    Some(json!({
        "signatures": [{
            "label": label,
            "parameters": parameters,
        }],
        "activeSignature": 0,
        "activeParameter": active.min(parameters.len().saturating_sub(1)),
    }))
}

pub(crate) fn valid_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .next()
            .map(|c| c.is_alphabetic() || c == '_')
            .unwrap_or(false)
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn offset_in(span: &Span, offset: usize) -> bool {
    offset >= span.start as usize && offset <= span.end as usize
}

fn callee_name(callee: &Expr) -> String {
    match callee {
        Expr::Ident(i) => i.text.clone(),
        Expr::Member { name, .. } => name.text.clone(),
        Expr::Path { segments, .. } => segments.last().map(|i| i.text.clone()).unwrap_or_default(),
        _ => String::new(),
    }
}

/// Count commas at paren-depth 0 between the call's `(` and `offset`.
fn active_parameter(src: &str, call_span: Span, offset: usize) -> usize {
    let bytes = src.as_bytes();
    let start = call_span.start as usize;
    let end = (call_span.end as usize).min(src.len());
    // Find the '(' that opens the arg list.
    let mut open = None;
    for (i, &b) in bytes.iter().enumerate().take(end).skip(start) {
        if b == b'(' {
            open = Some(i);
            break;
        }
    }
    let Some(open) = open else { return 0 };
    let upto = offset.min(end);
    let mut depth = 0i32;
    let mut commas = 0usize;
    let mut i = open;
    while i < upto {
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 1 => commas += 1,
            b'"' | b'\'' => {
                // skip string/char literal
                let q = bytes[i];
                i += 1;
                while i < upto && bytes[i] != q {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    commas
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
    fn formatting_returns_edit_or_null() {
        let messy = "class A{\nfunction f()as Void{var x=1;}\n}\n";
        let r = formatting(messy, false);
        assert!(r.is_array());
        assert!(r[0]["newText"].as_str().unwrap().contains("function f()"));
        let clean = "var x = 1;\n";
        assert!(formatting(clean, false).is_null());
    }

    #[test]
    fn signature_help_finds_call() {
        let src = "function draw(x as Number, y as Number) as Void {\n}\nfunction go() as Void {\n  draw(1, 2);\n}\n";
        let off = src.rfind("2)").unwrap();
        let h = signature_help(src, off).unwrap();
        assert_eq!(h["activeSignature"], 0);
        assert_eq!(h["activeParameter"], 1);
        let label = h["signatures"][0]["label"].as_str().unwrap();
        assert!(label.contains("draw"), "got {}", label);
        assert_eq!(
            h["signatures"][0]["parameters"].as_array().unwrap().len(),
            2
        );
    }

    #[test]
    fn class_at_and_members() {
        let src =
            "class A {\n  var c = 0;\n  function m() as Void {}\n}\nfunction f() as Void {}\n";
        let off = src.find("c = 0").unwrap();
        let c = class_at(src, off).unwrap();
        assert_eq!(c.name.text, "A");
        let members = class_members(src, "A");
        assert!(members.iter().any(|(n, _)| n == "c"));
        assert!(members.iter().any(|(n, k)| n == "m" && *k == "method"));
    }
}
