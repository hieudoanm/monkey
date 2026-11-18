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
        TypeRef::Path { path, .. } | TypeRef::Generic { path, .. } => {
            path.last().map(|i| i.text.clone())
        }
        TypeRef::Array { .. } => Some("Array".to_string()),
        TypeRef::Union { variants, .. } => variants.first().and_then(type_name_of),
        TypeRef::Method { .. } => Some("Method".to_string()),
        TypeRef::Dict { .. } => Some("Dictionary".to_string()),
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
    /// Innermost enclosing class/enum name — parent for member decls.
    class_stack: Vec<String>,
    /// function name → return type text, for `var x = f()` inference.
    fn_returns: std::collections::HashMap<String, String>,
    /// `Owner.method` → return type, for `var x = o.m()` inference.
    method_returns: std::collections::HashMap<String, String>,
    /// var/field/param name → inferred type, for `var x = y` inference.
    known: std::collections::HashMap<String, String>,
}

fn collect_fn_returns_item(
    item: &Item,
    owner: Option<&str>,
    out: &mut std::collections::HashMap<String, String>,
    methods: &mut std::collections::HashMap<String, String>,
) {
    match item {
        Item::Function(f) => {
            let ty = type_text(&f.return_type);
            if !ty.is_empty() {
                out.insert(f.name.text.clone(), ty.clone());
                if let Some(o) = owner {
                    methods.insert(format!("{}.{}", o, f.name.text), ty);
                }
            }
        }
        Item::Class(c) => {
            for m in &c.members {
                match m {
                    ClassMember::Function(f) => {
                        let ty = type_text(&f.return_type);
                        if !ty.is_empty() {
                            out.insert(f.name.text.clone(), ty.clone());
                            methods.insert(format!("{}.{}", c.name.text, f.name.text), ty);
                        }
                    }
                    ClassMember::Class(inner) => collect_fn_returns_item(
                        &Item::Class((**inner).clone()),
                        Some(&inner.name.text),
                        out,
                        methods,
                    ),
                    _ => {}
                }
            }
        }
        Item::Module(m) => {
            for i in &m.members {
                collect_fn_returns_item(i, owner, out, methods);
            }
        }
        _ => {}
    }
}

/// Shared type-inference context: name→type maps plus the class-scope
/// stack (for `this`).
pub(crate) struct InferCtx<'a> {
    pub known: &'a std::collections::HashMap<String, String>,
    pub fn_returns: &'a std::collections::HashMap<String, String>,
    pub method_returns: &'a std::collections::HashMap<String, String>,
    /// Innermost enclosing class names, outermost last — `this` resolves
    /// to the last entry.
    pub scopes: &'a [String],
}

/// Infer an expression's type name where cheaply possible.
pub(crate) fn infer_expr(e: &Expr, ctx: &InferCtx) -> Option<String> {
    match e {
        Expr::New { type_, .. } => type_name_of(type_),
        Expr::Cast { type_, .. } => type_name_of(type_),
        Expr::This { .. } => ctx.scopes.last().cloned(),
        Expr::Ident(i) => ctx.known.get(&i.text).cloned(),
        Expr::Path { segments, .. } if segments.len() == 1 => {
            ctx.known.get(&segments[0].text).cloned()
        }
        Expr::Literal(Literal::Str { .. } | Literal::Char { .. }) => Some("String".to_string()),
        Expr::Literal(Literal::Number { .. }) => Some("Number".to_string()),
        Expr::Literal(Literal::Bool { .. }) => Some("Boolean".to_string()),
        Expr::Array { .. } => Some("Array".to_string()),
        Expr::Dict { .. } => Some("Dictionary".to_string()),
        Expr::Call { callee, .. } => match callee.as_ref() {
            Expr::Ident(i) => ctx.fn_returns.get(&i.text).cloned(),
            // `obj.m()` / `a.b.m()` — resolve the receiver's type, then
            // look up `Type.m` in the method-return table.
            Expr::Member { object, name, .. } => {
                let owner = infer_expr(object, ctx)?;
                ctx.method_returns
                    .get(&format!("{}.{}", owner, name.text))
                    .cloned()
            }
            Expr::Path { segments, .. } if segments.len() == 2 => {
                let owner = ctx.known.get(&segments[0].text)?;
                ctx.method_returns
                    .get(&format!("{}.{}", owner, segments[1].text))
                    .cloned()
            }
            _ => None,
        },
        Expr::Paren { expr, .. } | Expr::Postfix { expr, .. } | Expr::Unary { expr, .. } => {
            infer_expr(expr, ctx)
        }
        _ => None,
    }
}

impl Collector {
    fn infer(&self, e: &Expr) -> Option<String> {
        infer_expr(
            e,
            &InferCtx {
                known: &self.known,
                fn_returns: &self.fn_returns,
                method_returns: &self.method_returns,
                scopes: &self.class_stack,
            },
        )
    }

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
        let inferred = f
            .type_ann
            .as_ref()
            .and_then(type_name_of)
            .or_else(|| f.init.as_ref().and_then(|e| self.infer(e)));
        let ty = type_text(&f.type_ann);
        let shown = if ty.is_empty() {
            inferred.clone().unwrap_or_default()
        } else {
            ty
        };
        let detail = if shown.is_empty() {
            format!("var {}", f.name.text)
        } else {
            format!("var {}: {}", f.name.text, shown)
        };
        let n = self.decls.len();
        self.push(&f.name, "field", detail);
        self.decls[n].type_name = inferred;
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
        // Enum members' parent is the enum, not an enclosing class.
        self.class_stack.push(d.name.text.clone());
        for m in &d.members {
            self.visit_enum_member(m);
        }
        self.class_stack.pop();
    }

    fn visit_enum_member(&mut self, m: &EnumMember) {
        self.push(&m.name, "enum-member", m.name.text.clone());
        if let Some(v) = &m.value {
            self.visit_expr(v);
        }
    }

    fn visit_var_decl(&mut self, d: &VarDecl) {
        let inferred = d
            .type_ann
            .as_ref()
            .and_then(type_name_of)
            .or_else(|| d.init.as_ref().and_then(|e| self.infer(e)));
        let ty = type_text(&d.type_ann);
        let kw = if d.mutable { "var" } else { "const" };
        // Prefer the annotation text; fall back to the inferred type name.
        let shown = if ty.is_empty() {
            inferred.clone().unwrap_or_default()
        } else {
            ty
        };
        let detail = if shown.is_empty() {
            format!("{} {}", kw, d.name.text)
        } else {
            format!("{} {}: {}", kw, d.name.text, shown)
        };
        let n = self.decls.len();
        self.push(&d.name, "variable", detail);
        self.decls[n].type_name = inferred;
        for a in &d.annotations {
            self.visit_annotation(a);
        }
        if let Some(i) = &d.init {
            self.visit_expr(i);
        }
    }
}

/// Pre-pass: seed `name → type` from annotations and simple initializers so
/// `var b = a` / `var c = a.field()`-style chains resolve one level deep.
struct KnownTypes {
    known: std::collections::HashMap<String, String>,
    fn_returns: std::collections::HashMap<String, String>,
}

impl KnownTypes {
    fn record(&mut self, name: &Ident, type_ann: &Option<TypeRef>, init: &Option<Expr>) {
        let ty = type_ann.as_ref().and_then(type_name_of).or_else(|| {
            init.as_ref().and_then(|e| {
                infer_expr(
                    e,
                    &InferCtx {
                        known: &self.known,
                        fn_returns: &self.fn_returns,
                        method_returns: &std::collections::HashMap::new(),
                        scopes: &[],
                    },
                )
            })
        });
        if let Some(t) = ty {
            self.known.insert(name.text.clone(), t);
        }
    }
}

impl Visitor for KnownTypes {
    fn visit_var_decl(&mut self, d: &VarDecl) {
        self.record(&d.name, &d.type_ann, &d.init);
        if let Some(i) = &d.init {
            self.visit_expr(i);
        }
    }

    fn visit_field(&mut self, f: &FieldDecl) {
        self.record(&f.name, &f.type_ann, &f.init);
        if let Some(i) = &f.init {
            self.visit_expr(i);
        }
    }

    fn visit_param(&mut self, p: &Param) {
        if let Some(t) = p.type_ann.as_ref().and_then(type_name_of) {
            self.known.insert(p.name.text.clone(), t);
        }
    }
}

/// Post-pass: `var x; … x = new Foo()` — infer un-annotated locals from
/// later assignments inside the same function scope.
struct AssignTypes<'a> {
    ctx_maps: &'a InferMaps,
    scopes: Vec<String>,
    /// (name, inferred type, assign-ident span)
    assigns: Vec<(String, Option<String>, Span)>,
}

struct InferMaps {
    known: std::collections::HashMap<String, String>,
    fn_returns: std::collections::HashMap<String, String>,
    method_returns: std::collections::HashMap<String, String>,
}

impl<'a> AssignTypes<'a> {
    fn infer(&self, e: &Expr) -> Option<String> {
        infer_expr(
            e,
            &InferCtx {
                known: &self.ctx_maps.known,
                fn_returns: &self.ctx_maps.fn_returns,
                method_returns: &self.ctx_maps.method_returns,
                scopes: &self.scopes,
            },
        )
    }
}

impl Visitor for AssignTypes<'_> {
    fn visit_class(&mut self, d: &ClassDecl) {
        self.scopes.push(d.name.text.clone());
        for a in &d.annotations {
            self.visit_annotation(a);
        }
        if let Some(s) = &d.superclass {
            self.visit_type(s);
        }
        for m in &d.members {
            self.visit_class_member(m);
        }
        self.scopes.pop();
    }

    fn visit_expr(&mut self, e: &Expr) {
        if let Expr::Assign { target, value, .. } = e {
            if let Expr::Ident(i) = target.as_ref() {
                let ty = self.infer(value);
                self.assigns.push((i.text.clone(), ty, i.span));
            }
        }
        // Recurse — mirrors the default Visitor::visit_expr walk.
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
            Expr::New { type_, args, .. } => {
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
                for el in elems {
                    self.visit_expr(el);
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

/// Collect every named declaration in `src` (parsed on demand).
pub fn collect_decls(src: &str) -> Vec<Decl> {
    let parsed = monkey_parser::parse(src);
    // Pre-pass: function/method return types for `var x = f()` inference.
    let mut fn_returns = std::collections::HashMap::new();
    let mut method_returns = std::collections::HashMap::new();
    for i in &parsed.file.items {
        collect_fn_returns_item(i, None, &mut fn_returns, &mut method_returns);
    }
    // Seed name→type from annotations/simple inits.
    let mut kt = KnownTypes {
        known: std::collections::HashMap::new(),
        fn_returns: fn_returns.clone(),
    };
    kt.visit_source_file(&parsed.file);
    let known = kt.known;
    let maps = InferMaps {
        known: known.clone(),
        fn_returns: fn_returns.clone(),
        method_returns: method_returns.clone(),
    };
    let mut c = Collector {
        decls: Vec::new(),
        class_stack: Vec::new(),
        fn_returns,
        method_returns,
        known,
    };
    c.visit_source_file(&parsed.file);
    let mut decls = c.decls;

    // Assignment inference: `var x; … x = new Foo()` inside the same
    // function scope. Exact scope matches win; globals (scope `None`)
    // only fill decls still untyped after that.
    let mut at = AssignTypes {
        ctx_maps: &maps,
        scopes: Vec::new(),
        assigns: Vec::new(),
    };
    at.visit_source_file(&parsed.file);
    for d in decls.iter_mut() {
        if d.type_name.is_some() || d.kind != "variable" {
            continue;
        }
        let decl_scope = enclosing_fn_span(src, d.name_span);
        // Exact function-scope match first; file-scope decls then fall
        // back to any assignment of the same name (module globals).
        let mut found = None;
        for (name, ty, span) in &at.assigns {
            if name != &d.name {
                continue;
            }
            let Some(ty) = ty else { continue };
            if enclosing_fn_span(src, *span) == decl_scope {
                found = Some(ty.clone());
                break;
            }
        }
        if found.is_none() && decl_scope.is_none() {
            for (name, ty, _) in &at.assigns {
                if name == &d.name {
                    if let Some(t) = ty {
                        found = Some(t.clone());
                        break;
                    }
                }
            }
        }
        if let Some(ty) = found {
            d.type_name = Some(ty.clone());
            if !d.detail.contains(':') {
                d.detail = format!("{}: {}", d.detail, ty);
            }
        }
    }
    decls
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
    // Statement/decl snippets — LSP snippet syntax (${1:placeholder}, $0).
    const SNIPPETS: &[(&str, &str, &str)] = &[
        (
            "fn",
            "function ${1:name}(${2:params}) as ${3:Void} {\n\t$0\n}",
            "snippet: function",
        ),
        (
            "class",
            "class ${1:Name} {\n\tfunction initialize() {\n\t\t${2:}\n\t}\n\t$0\n}",
            "snippet: class",
        ),
        ("if", "if (${1:condition}) {\n\t$0\n}", "snippet: if"),
        (
            "for",
            "for (var ${1:i} = 0; ${1:i} < ${2:n}; ${1:i}++) {\n\t$0\n}",
            "snippet: for",
        ),
        (
            "while",
            "while (${1:condition}) {\n\t$0\n}",
            "snippet: while",
        ),
        (
            "try",
            "try {\n\t$0\n} catch (e) {\n\t\n}",
            "snippet: try/catch",
        ),
        ("var", "var ${1:name} = ${2:value};", "snippet: variable"),
        (
            "method",
            "function ${1:name}(${2:params}) as ${3:Void} {\n\t$0\n}",
            "snippet: method",
        ),
    ];
    for (label, insert, detail) in SNIPPETS {
        // No `seen` dedup: snippet labels intentionally shadow keyword
        // labels so both a plain keyword and a snippet variant are offered.
        items.push(json!({
            "label": label,
            "kind": 15, // Snippet
            "detail": detail,
            "insertText": insert,
            "insertTextFormat": 2,
        }));
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
/// `class_name → superclass name` for `super.x` owner resolution.
fn collect_supers(file: &SourceFile) -> std::collections::HashMap<String, String> {
    struct V {
        map: std::collections::HashMap<String, String>,
    }
    impl Visitor for V {
        fn visit_class(&mut self, d: &ClassDecl) {
            if let Some(s) = &d.superclass {
                if let Some(n) = type_name_of(s) {
                    self.map.insert(d.name.text.clone(), n);
                }
            }
            for a in &d.annotations {
                self.visit_annotation(a);
            }
            if let Some(s) = &d.superclass {
                self.visit_type(s);
            }
            for m in &d.members {
                self.visit_class_member(m);
            }
        }
    }
    let mut v = V {
        map: std::collections::HashMap::new(),
    };
    v.visit_source_file(file);
    v.map
}

struct UseCollector {
    name: String,
    member: bool,
    /// Span of the declaring class body; bare-ident uses outside it are
    /// ignored in member mode. Also reused as a function-scope restriction
    /// for local variable/param references.
    class_span: Option<Span>,
    /// Function spans containing a same-named local/param declaration.
    /// Bare-ident uses inside them bind to the local, not a member —
    /// excluded in member mode (fields/methods shadowed by locals).
    shadowed: Vec<Span>,
    /// Declaring class/enum name — owner check for `.name`/`Path` member
    /// uses so same-named members of other types don't collide.
    parent: Option<String>,
    /// var/field/param name → type name, for receiver resolution.
    types: std::collections::HashMap<String, String>,
    /// Names of classes/enums declared in this file — a path root matching
    /// one of these (but not `parent`) is a *different* owner.
    type_names: std::collections::HashSet<String>,
    /// class name → superclass name, for `super.x` resolution.
    supers: std::collections::HashMap<String, String>,
    /// Enclosing class names (innermost last) for `this` resolution.
    scopes: Vec<String>,
    uses: Vec<Span>,
}

impl UseCollector {
    /// True when `offset` falls inside a scope where a same-named local
    /// shadows the member being collected.
    fn shadowed_at(&self, offset: usize) -> bool {
        self.shadowed.iter().any(|s| span_contains(*s, offset))
    }

    /// Does identifier `name` (a receiver root like `v` in `v.member`)
    /// resolve to the declaring type `parent`?
    fn owner_matches(&self, name: &str, parent: &str) -> bool {
        if let Some(t) = self.types.get(name) {
            return t == parent;
        }
        // A known class/enum name that isn't the parent is a different
        // owner; anything else is unresolved → count conservatively.
        if self.type_names.contains(name) {
            return name == parent;
        }
        true
    }

    /// Does `super.member` inside the current class reach `parent`?
    fn super_matches(&self, parent: &str) -> bool {
        let Some(start) = self.scopes.last() else {
            return true;
        };
        let mut cur = start.clone();
        for _ in 0..32 {
            match self.supers.get(&cur) {
                Some(next) => {
                    if next == parent {
                        return true;
                    }
                    cur = next.clone();
                }
                None => return false,
            }
        }
        false
    }

    /// `Expr::Member` receiver check: `this.x`, `super.x`, `foo().x`, …
    fn receiver_matches(&self, object: &Expr) -> bool {
        let Some(parent) = self.parent.as_deref() else {
            return true;
        };
        match object {
            Expr::This { .. } => self.scopes.last().map(|s| s == parent).unwrap_or(true),
            Expr::Super { .. } => self.super_matches(parent),
            Expr::Ident(i) => self.owner_matches(&i.text, parent),
            Expr::Path { segments, .. } if segments.len() == 1 => {
                self.owner_matches(&segments[0].text, parent)
            }
            Expr::New { type_, .. } => type_name_of(type_).as_deref() == Some(parent),
            Expr::Paren { expr, .. } | Expr::Postfix { expr, .. } | Expr::Unary { expr, .. } => {
                self.receiver_matches(expr)
            }
            _ => true,
        }
    }

    /// `Path` segment check: segments after the first are member accesses
    /// (`v.count`, `Enum.MEMBER`); verify the owner when it's a single
    /// identifier. Deeper prefixes are left conservative.
    fn path_member_matches(&self, prefix: &[Ident]) -> bool {
        let Some(parent) = self.parent.as_deref() else {
            return true;
        };
        match prefix {
            [root] => self.owner_matches(&root.text, parent),
            _ => true,
        }
    }
}

impl Visitor for UseCollector {
    fn visit_class(&mut self, d: &ClassDecl) {
        self.scopes.push(d.name.text.clone());
        for a in &d.annotations {
            self.visit_annotation(a);
        }
        if let Some(s) = &d.superclass {
            self.visit_type(s);
        }
        for m in &d.members {
            self.visit_class_member(m);
        }
        self.scopes.pop();
    }

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
                if inside && !self.shadowed_at(i.span.start as usize) {
                    self.uses.push(i.span);
                }
            }
            Expr::Path { segments, .. } => {
                for (idx, seg) in segments.iter().enumerate() {
                    if seg.text != self.name {
                        continue;
                    }
                    if self.member && idx > 0 {
                        // Member segment (`v.name`, `Enum.NAME`) — check
                        // the owner prefix resolves to the declaring type.
                        if self.path_member_matches(&segments[..idx]) {
                            self.uses.push(seg.span);
                        }
                        continue;
                    }
                    // Root segment / non-member mode: bare-ident rules.
                    let inside = match self.class_span {
                        Some(cs) => span_contains(cs, seg.span.start as usize),
                        None => true,
                    };
                    if inside && !self.shadowed_at(seg.span.start as usize) {
                        self.uses.push(seg.span);
                    }
                }
            }
            Expr::Member { object, name, .. }
                if self.member && name.text == self.name && self.receiver_matches(object) =>
            {
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
/// accesses and path segments count, bare identifiers are restricted to
/// `class_span` (the declaring class body) when provided, and member
/// segments are only counted when their receiver resolves to `parent`
/// (the declaring class/enum) — unresolved receivers count conservatively.
pub fn uses_in_mode(
    src: &str,
    name: &str,
    member: bool,
    class_span: Option<Span>,
    parent: Option<&str>,
) -> Vec<Span> {
    let parsed = monkey_parser::parse(src);
    let decls = collect_decls(src);
    // In member mode, find functions where a same-named local/param shadows
    // the member — bare uses there belong to the local.
    let shadowed = if member {
        decls
            .iter()
            .filter(|d| d.name == name && matches!(d.kind, "variable" | "param"))
            .filter_map(|d| enclosing_fn_span(src, d.name_span))
            .collect()
    } else {
        Vec::new()
    };
    let (types, type_names) = if member {
        let mut types = std::collections::HashMap::new();
        let mut type_names = std::collections::HashSet::new();
        for d in &decls {
            match d.kind {
                "class" | "enum" => {
                    type_names.insert(d.name.clone());
                }
                _ => {
                    if let Some(t) = &d.type_name {
                        types.insert(d.name.clone(), t.clone());
                    }
                }
            }
        }
        (types, type_names)
    } else {
        (Default::default(), Default::default())
    };
    let supers = if member {
        collect_supers(&parsed.file)
    } else {
        Default::default()
    };
    let mut c = UseCollector {
        name: name.to_string(),
        member,
        class_span,
        shadowed,
        parent: parent.map(str::to_string),
        types,
        type_names,
        supers,
        scopes: Vec::new(),
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

/// If `name` is declared as `var name = <receiver>.<member>(...)`, return
/// `(receiver_path, member)` — e.g. `("Math", "cos")` for `Math.cos(0)` or
/// `("Toybox.Math", "cos")` for the fully-qualified form. Used to resolve
/// Toybox method return types for completion.
pub fn var_call_target(src: &str, name: &str) -> Option<(String, String)> {
    let parsed = monkey_parser::parse(src);
    struct V<'a> {
        name: &'a str,
        found: Option<(String, String)>,
    }
    impl Visitor for V<'_> {
        fn visit_var_decl(&mut self, d: &VarDecl) {
            if self.found.is_none() && d.name.text == self.name {
                if let Some(Expr::Call { callee, .. }) = &d.init {
                    match callee.as_ref() {
                        Expr::Path { segments, .. } if segments.len() >= 2 => {
                            if let Some(last) = segments.last() {
                                let member = last.text.clone();
                                let owner = segments[..segments.len() - 1]
                                    .iter()
                                    .map(|s| s.text.as_str())
                                    .collect::<Vec<_>>()
                                    .join(".");
                                self.found = Some((owner, member));
                            }
                        }
                        Expr::Member { name, .. } => {
                            self.found = Some((String::new(), name.text.clone()));
                        }
                        _ => {}
                    }
                }
            }
            if let Some(i) = &d.init {
                self.visit_expr(i);
            }
        }
    }
    let mut v = V { name, found: None };
    v.visit_source_file(&parsed.file);
    v.found
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

    #[test]
    fn enum_member_parent_is_enum() {
        let decls = collect_decls("enum Color {\n  RED,\n  BLUE\n}\n");
        let m = decls.iter().find(|d| d.name == "RED").unwrap();
        assert_eq!(m.kind, "enum-member");
        assert_eq!(m.parent.as_deref(), Some("Color"));
    }

    #[test]
    fn member_uses_filtered_by_receiver_type() {
        // `a.draw()` (a: A) counts for A.draw; `this.draw()` inside B and
        // `b.draw()` (b: B) do not.
        let src = "class A {\n  function draw() as Void {}\n}\nclass B {\n  var b = new B();\n  function draw() as Void {}\n  function f() as Void {\n    var a = new A();\n    a.draw();\n    this.draw();\n    b.draw();\n  }\n}\n";
        let uses = uses_in_mode(src, "draw", true, class_span(src, "A"), Some("A"));
        let texts: Vec<&str> = uses
            .iter()
            .map(|s| &src[s.start as usize..s.end as usize])
            .collect();
        assert_eq!(texts, vec!["draw"], "uses: {:?}", texts);
        assert!(src[uses[0].start as usize..].starts_with("draw();"));
    }

    #[test]
    fn enum_member_uses_filtered_by_owner() {
        let src =
            "enum Color {\n  RED,\n  BLUE\n}\nenum Size {\n  RED,\n  BIG\n}\nvar c = Color.RED;\nvar s = Size.RED;\n";
        let uses = uses_in_mode(src, "RED", true, Some(Span::new(0, 0)), Some("Color"));
        let texts: Vec<&str> = uses
            .iter()
            .map(|s| &src[s.start as usize..s.end as usize])
            .collect();
        assert_eq!(texts.len(), 1, "uses: {:?}", texts);
        // The counted use must be the one in `Color.RED` (line 8), not `Size.RED`.
        let line = src[..uses[0].start as usize].matches('\n').count();
        assert_eq!(line, 8, "wrong RED matched at line {}", line);
    }

    #[test]
    fn infers_types_through_vars_methods_and_assignments() {
        let src = "class Foo {\n  function bar() as Foo {\n    return new Foo();\n  }\n}\nfunction f() as Void {\n  var a = new Foo();\n  var b = a;\n  var c = a.bar();\n  var d = null;\n  d = new Foo();\n}\n";
        let decls = collect_decls(src);
        let ty = |n: &str| {
            decls
                .iter()
                .find(|d| d.name == n && d.kind == "variable")
                .and_then(|d| d.type_name.clone())
        };
        assert_eq!(ty("a").as_deref(), Some("Foo"));
        assert_eq!(ty("b").as_deref(), Some("Foo"), "var b = a");
        assert_eq!(ty("c").as_deref(), Some("Foo"), "var c = a.bar()");
        assert_eq!(ty("d").as_deref(), Some("Foo"), "d = new Foo()");
    }
}
