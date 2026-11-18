//! API-availability checking for `monkey project check`.
//!
//! Resolves dotted `Toybox.*` references (paths, `new`, `as` casts, and type
//! annotations) against each target device's `<id>.api.debug.xml` surface
//! and reports symbols that don't exist on a product.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use monkey_core::{Diagnostic, Span};
use monkey_parser::ast::*;
use monkey_parser::visit::Visitor;
use monkey_project::Project;
use monkey_sdk::device::{find_device, ApiSurface};

/// One file's API diagnostics, ready for `print_diagnostics`.
pub struct FileApiDiags {
    pub path: PathBuf,
    pub src: String,
    pub diags: Vec<Diagnostic>,
}

/// Dotted name candidates found in a file, with their spans.
struct ApiUseCollector {
    /// Bound name → dotted target, e.g. `Math` → `Toybox.Math`,
    /// `M` → `Toybox.Math` for `using Toybox.Math as M`.
    bound: HashMap<String, String>,
    /// `(resolved dotted name, span)` — only resolvable Toybox refs.
    uses: Vec<(String, Span)>,
}

impl ApiUseCollector {
    /// Resolve identifier segments to a dotted name, or `None` when the
    /// head isn't `Toybox` or a `using`-bound name.
    fn resolve(&self, segs: &[String]) -> Option<String> {
        let (head, rest) = segs.split_first()?;
        if rest.is_empty() {
            // Bare names can't be verified without type info.
            return None;
        }
        let base = if head == "Toybox" {
            "Toybox".to_string()
        } else {
            self.bound.get(head)?.clone()
        };
        let mut parts = vec![base];
        parts.extend(rest.iter().cloned());
        Some(parts.join("."))
    }

    fn record_path(&mut self, segs: &[Ident], span: Span) {
        let names: Vec<String> = segs.iter().map(|i| i.text.clone()).collect();
        if let Some(r) = self.resolve(&names) {
            self.uses.push((r, span));
        }
    }

    fn record_type(&mut self, t: &TypeRef) {
        let (path, span) = match t {
            TypeRef::Path { path, span, .. } | TypeRef::Generic { path, span, .. } => (path, *span),
            TypeRef::Method { path, span, .. } => (path, *span),
            _ => return,
        };
        self.record_path(path, span);
    }
}

impl Visitor for ApiUseCollector {
    fn visit_using(&mut self, d: &UsingDecl) {
        let target: Vec<String> = d.path.iter().map(|i| i.text.clone()).collect();
        if target.is_empty() {
            return;
        }
        let dotted = target.join(".");
        // `using A.B.C` binds `C`; `using A.B.C as X` binds `X`.
        let bound_name = d
            .alias
            .as_ref()
            .map(|a| a.text.clone())
            .or_else(|| target.last().cloned())
            .unwrap_or_default();
        if !bound_name.is_empty() {
            self.bound.insert(bound_name, dotted);
        }
    }

    fn visit_expr(&mut self, e: &Expr) {
        match e {
            Expr::Path { segments, span, .. } => self.record_path(segments, *span),
            Expr::New { type_, .. } | Expr::Cast { type_, .. } => self.record_type(type_),
            _ => {}
        }
        // Recurse so nested exprs are still visited.
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

    fn visit_type(&mut self, t: &TypeRef) {
        self.record_type(t);
        // Default recursion into nested type args.
        match t {
            TypeRef::Path { .. } => {}
            TypeRef::Method { params, ret, .. } => {
                for p in params {
                    self.visit_type(p);
                }
                if let Some(r) = ret {
                    self.visit_type(r);
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
            TypeRef::Generic { args, .. } => {
                for a in args {
                    self.visit_type(a);
                }
            }
            TypeRef::Dict { entries, .. } => {
                for e in entries {
                    self.visit_type(&e.type_);
                }
            }
        }
    }
}

/// Collect every resolvable Toybox reference in `src`.
fn api_uses(src: &str) -> Vec<(String, Span)> {
    let parsed = monkey_parser::parse(src);
    // Pre-pass for `using` bindings (they're top-level; collect first so
    // uses anywhere in the file resolve correctly).
    let mut bound = HashMap::new();
    for item in &parsed.file.items {
        if let Item::Using(u) = item {
            let target: Vec<String> = u.path.iter().map(|i| i.text.clone()).collect();
            if target.is_empty() {
                continue;
            }
            let dotted = target.join(".");
            let name = u
                .alias
                .as_ref()
                .map(|a| a.text.clone())
                .or_else(|| target.last().cloned())
                .unwrap_or_default();
            if !name.is_empty() {
                bound.insert(name, dotted);
            }
        }
    }
    let mut c = ApiUseCollector {
        bound,
        uses: Vec::new(),
    };
    c.visit_source_file(&parsed.file);
    c.uses
}

/// Check all `.mc` files under the project against each manifest product's
/// device API surface. Returns per-file diagnostic groups.
pub fn validate(project: &Project) -> Vec<FileApiDiags> {
    let mut out = Vec::new();
    let products = project
        .manifest
        .application
        .as_ref()
        .map(|a| a.products.clone())
        .unwrap_or_default();
    if products.is_empty() {
        return out;
    }

    // Load each product's API surface once.
    let mut surfaces: Vec<(String, ApiSurface)> = Vec::new();
    for pid in &products {
        if let Some(d) = find_device(pid) {
            if let Some(s) = d.api_surface() {
                surfaces.push((pid.clone(), s));
            }
        }
    }
    if surfaces.is_empty() {
        return out;
    }

    // Find .mc files under the project (same skip rules as the CLI).
    let files = crate::collect_files(&[project.root.to_string_lossy().to_string()], &["mc"]);
    for path in files {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let uses = api_uses(&src);
        if uses.is_empty() {
            continue;
        }
        let mut seen: HashSet<String> = HashSet::new();
        let mut diags = Vec::new();
        for (name, span) in uses {
            if !seen.insert(name.clone()) {
                continue;
            }
            for (pid, surface) in &surfaces {
                let known = surface.symbols.contains(&name) || surface.owners.contains(&name);
                if !known {
                    diags.push(
                        Diagnostic::error(
                            "api-unavailable",
                            format!("`{}` is not available on device '{}'", name, pid),
                        )
                        .with_span(span),
                    );
                }
            }
        }
        if !diags.is_empty() {
            out.push(FileApiDiags { path, src, diags });
        }
    }
    out
}
