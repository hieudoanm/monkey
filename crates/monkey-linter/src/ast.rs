//! AST-backed lint rules.
//!
//! One visitor pass collects everything: unreachable/empty blocks, identical
//! comparison operands, `requestUpdate` inside `onUpdate`, and unused local
//! variables (scoped per function).

use std::collections::{HashMap, HashSet};

use monkey_core::{Diagnostic, Severity, Span};
use monkey_parser::ast::*;
use monkey_parser::visit::Visitor;

/// Run all AST rules over a parsed file.
pub fn lint_ast(src: &str, file: &SourceFile, diags: &mut Vec<Diagnostic>) {
    let mut linter = AstLinter {
        src,
        diags: Vec::new(),
        scope: None,
        in_onupdate: false,
    };
    linter.visit_source_file(file);
    diags.append(&mut linter.diags);
}

/// Per-function state for the unused-variable rule.
struct FnScope {
    /// `(name, span)` for each `var`/`const` declarator, in order.
    decls: Vec<(String, Span)>,
    /// Number of declarations per name (shadowing makes a name ambiguous).
    decl_counts: HashMap<String, usize>,
    /// Identifier texts referenced anywhere in the function.
    uses: HashSet<String>,
}

struct AstLinter<'a> {
    src: &'a str,
    diags: Vec<Diagnostic>,
    /// `Some` while inside a function body.
    scope: Option<FnScope>,
    /// `true` while inside a function named `onUpdate`.
    in_onupdate: bool,
}

impl<'a> AstLinter<'a> {
    fn expr_text(&self, e: &Expr) -> &str {
        let s = e.span();
        self.src.get(s.start as usize..s.end as usize).unwrap_or("")
    }

    /// Whether `expr`'s callee path ends in `requestUpdate`.
    fn is_request_update_call(&self, e: &Expr) -> bool {
        match e {
            Expr::Call { callee, .. } => match callee.as_ref() {
                Expr::Path { segments, .. } => {
                    segments.last().is_some_and(|s| s.text == "requestUpdate")
                }
                Expr::Member { name, .. } => name.text == "requestUpdate",
                Expr::Ident(i) => i.text == "requestUpdate",
                _ => false,
            },
            _ => false,
        }
    }

    fn check_unused_vars(&mut self) {
        let Some(scope) = self.scope.take() else {
            return;
        };
        for (name, span) in &scope.decls {
            if scope.decl_counts.get(name) == Some(&1) && !scope.uses.contains(name) {
                self.diags.push(
                    Diagnostic::warning("unused-variable", format!("unused variable `{}`", name))
                        .with_span(*span),
                );
            }
        }
    }
}

impl Visitor for AstLinter<'_> {
    fn visit_function(&mut self, f: &FunctionDecl) {
        let outer_scope = self.scope.take();
        let outer_onupdate = self.in_onupdate;
        self.scope = Some(FnScope {
            decls: Vec::new(),
            decl_counts: HashMap::new(),
            uses: HashSet::new(),
        });
        self.in_onupdate = f.name.text == "onUpdate";

        for a in &f.annotations {
            self.visit_annotation(a);
        }
        for p in &f.params {
            self.visit_param(p);
        }
        if let Some(t) = &f.return_type {
            self.visit_type(t);
        }
        self.visit_block(&f.body);

        self.check_unused_vars();
        self.scope = outer_scope;
        self.in_onupdate = outer_onupdate;
    }

    fn visit_var_decl(&mut self, d: &VarDecl) {
        if let Some(scope) = &mut self.scope {
            let name = d.name.text.clone();
            *scope.decl_counts.entry(name.clone()).or_insert(0) += 1;
            scope.decls.push((name, d.name.span));
        }
        for a in &d.annotations {
            self.visit_annotation(a);
        }
        if let Some(t) = &d.type_ann {
            self.visit_type(t);
        }
        if let Some(e) = &d.init {
            self.visit_expr(e);
        }
    }

    fn visit_block(&mut self, b: &Block) {
        if b.stmts.is_empty() {
            self.diags.push(
                Diagnostic::new("empty-block", Severity::Hint, "empty block").with_span(b.span),
            );
        }
        let mut terminated = false;
        for s in &b.stmts {
            if terminated {
                self.diags.push(
                    Diagnostic::warning("unreachable-code", "unreachable statement")
                        .with_span(s.span()),
                );
            }
            if matches!(
                s,
                Stmt::Return { .. }
                    | Stmt::Throw { .. }
                    | Stmt::Break { .. }
                    | Stmt::Continue { .. }
            ) {
                terminated = true;
            }
            self.visit_stmt(s);
        }
    }

    fn visit_expr(&mut self, e: &Expr) {
        // unused-variable: record identifier usages inside functions.
        if let Some(scope) = &mut self.scope {
            match e {
                Expr::Ident(i) => {
                    scope.uses.insert(i.text.clone());
                }
                Expr::Path { segments, .. } => {
                    if let Some(first) = segments.first() {
                        scope.uses.insert(first.text.clone());
                    }
                }
                _ => {}
            }
        }

        // no-request-update-in-onupdate.
        if self.in_onupdate && self.is_request_update_call(e) {
            self.diags.push(
                Diagnostic::error(
                    "no-request-update-in-onupdate",
                    "requestUpdate() inside onUpdate causes a continuous redraw loop; request updates in onShow instead",
                )
                .with_span(e.span()),
            );
        }

        // identical-operands on comparisons.
        if let Expr::Binary {
            op, left, right, ..
        } = e
        {
            if matches!(
                op,
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
            ) && self.expr_text(left) == self.expr_text(right)
                && !self.expr_text(left).is_empty()
            {
                self.diags.push(
                    Diagnostic::warning(
                        "identical-operands",
                        format!("both sides of '{}' are identical", op.as_str()),
                    )
                    .with_span(e.span()),
                );
            }
        }

        // Default recursion (mirrors visit::Visitor::visit_expr).
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
            Expr::Unary { expr, .. } | Expr::Postfix { expr, .. } => self.visit_expr(expr),
            Expr::Binary { left, right, .. } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            Expr::Assign { target, value, .. } => {
                self.visit_expr(target);
                self.visit_expr(value);
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
            Expr::Paren { expr, .. } => self.visit_expr(expr),
        }
    }
}
