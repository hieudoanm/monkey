//! AST visitor with default recursion.
//!
//! Implement [`Visitor`] and override the methods you care about; every
//! default method already walks into child nodes, so a minimal visitor only
//! implements the hooks it needs.

use crate::ast::*;

/// Depth-first AST visitor.
pub trait Visitor {
    fn visit_source_file(&mut self, file: &SourceFile) {
        for item in &file.items {
            self.visit_item(item);
        }
    }

    fn visit_item(&mut self, item: &Item) {
        match item {
            Item::Using(d) => self.visit_using(d),
            Item::Import(d) => self.visit_import(d),
            Item::Class(d) => self.visit_class(d),
            Item::Enum(d) => self.visit_enum(d),
            Item::Function(d) => self.visit_function(d),
            Item::VarDecl(d) => self.visit_var_decl(d),
            Item::Error { .. } => {}
        }
    }

    fn visit_using(&mut self, d: &UsingDecl) {
        for a in &d.annotations {
            self.visit_annotation(a);
        }
    }

    fn visit_import(&mut self, d: &ImportDecl) {
        for a in &d.annotations {
            self.visit_annotation(a);
        }
    }

    fn visit_class(&mut self, d: &ClassDecl) {
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

    fn visit_class_member(&mut self, m: &ClassMember) {
        match m {
            ClassMember::Field(f) => self.visit_field(f),
            ClassMember::Function(f) => self.visit_function(f),
            ClassMember::Error { .. } => {}
        }
    }

    fn visit_field(&mut self, f: &FieldDecl) {
        for a in &f.annotations {
            self.visit_annotation(a);
        }
        if let Some(t) = &f.type_ann {
            self.visit_type(t);
        }
        if let Some(e) = &f.init {
            self.visit_expr(e);
        }
    }

    fn visit_function(&mut self, f: &FunctionDecl) {
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
    }

    fn visit_param(&mut self, p: &Param) {
        if let Some(t) = &p.type_ann {
            self.visit_type(t);
        }
    }

    fn visit_enum(&mut self, d: &EnumDecl) {
        for a in &d.annotations {
            self.visit_annotation(a);
        }
        for m in &d.members {
            self.visit_enum_member(m);
        }
    }

    fn visit_enum_member(&mut self, m: &EnumMember) {
        if let Some(v) = &m.value {
            self.visit_expr(v);
        }
    }

    fn visit_annotation(&mut self, a: &Annotation) {
        for arg in &a.args {
            self.visit_expr(arg);
        }
    }

    fn visit_type(&mut self, _t: &TypeRef) {}

    fn visit_block(&mut self, b: &Block) {
        for s in &b.stmts {
            self.visit_stmt(s);
        }
    }

    fn visit_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Block(b) => self.visit_block(b),
            Stmt::VarDecl { decls, .. } => {
                for d in decls {
                    self.visit_var_decl(d);
                }
            }
            Stmt::Expr { expr, .. } => self.visit_expr(expr),
            Stmt::If {
                branches,
                else_branch,
                ..
            } => {
                for (cond, body) in branches {
                    self.visit_expr(cond);
                    self.visit_block(body);
                }
                if let Some(b) = else_branch {
                    self.visit_block(b);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.visit_expr(cond);
                self.visit_block(body);
            }
            Stmt::DoWhile { body, cond, .. } => {
                self.visit_block(body);
                self.visit_expr(cond);
            }
            Stmt::For {
                init,
                cond,
                update,
                body,
                ..
            } => {
                if let Some(i) = init {
                    match i {
                        ForInit::VarDecl(d) => self.visit_var_decl(d),
                        ForInit::Expr(e) => self.visit_expr(e),
                    }
                }
                if let Some(c) = cond {
                    self.visit_expr(c);
                }
                if let Some(u) = update {
                    self.visit_expr(u);
                }
                self.visit_block(body);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.visit_expr(v);
                }
            }
            Stmt::Throw { expr, .. } => self.visit_expr(expr),
            Stmt::Try {
                body,
                catches,
                finally,
                ..
            } => {
                self.visit_block(body);
                for c in catches {
                    self.visit_block(&c.body);
                }
                if let Some(f) = finally {
                    self.visit_block(f);
                }
            }
            Stmt::Switch { expr, cases, .. } => {
                self.visit_expr(expr);
                for c in cases {
                    if let Some(l) = &c.label {
                        self.visit_expr(l);
                    }
                    for s in &c.stmts {
                        self.visit_stmt(s);
                    }
                }
            }
            Stmt::Break { .. }
            | Stmt::Continue { .. }
            | Stmt::Empty { .. }
            | Stmt::Error { .. } => {}
        }
    }

    fn visit_var_decl(&mut self, d: &VarDecl) {
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

    fn visit_expr(&mut self, e: &Expr) {
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
            Expr::Paren { expr, .. } => self.visit_expr(expr),
        }
    }
}
