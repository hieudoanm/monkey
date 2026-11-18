//! Recursive-descent parser for Monkey C with error recovery.
//!
//! The parser is permissive by design: it always returns a [`SourceFile`]
//! (possibly containing [`Item::Error`]/[`ClassMember::Error`]/
//! [`Stmt::Error`]/[`Expr::Missing`] nodes) plus a list of [`Diagnostic`]s,
//! so editor integrations can keep working on incomplete code.

use monkey_core::{Diagnostic, Span};
use monkey_syntax::{lex_full, Token, TokenKind};

use crate::ast::*;

/// Result of parsing a source file.
#[derive(Debug)]
pub struct ParseResult {
    pub file: SourceFile,
    /// Lexer + parser diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

/// Parse Monkey C source text into an AST plus diagnostics.
pub fn parse(src: &str) -> ParseResult {
    let lexed = lex_full(src);
    let tokens: Vec<Token> = lexed
        .tokens
        .into_iter()
        .filter(|t| !matches!(t.kind, TokenKind::LineComment | TokenKind::BlockComment))
        .collect();
    let mut p = Parser {
        tokens,
        pos: 0,
        diags: lexed.diagnostics,
    };
    let file = p.parse_file();
    ParseResult {
        file,
        diagnostics: p.diags,
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    diags: Vec<Diagnostic>,
}

// ---------- token helpers ----------

impl Parser {
    fn cur(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn nth(&self, n: usize) -> &Token {
        &self.tokens[(self.pos + n).min(self.tokens.len() - 1)]
    }

    fn at_eof(&self) -> bool {
        self.cur().kind == TokenKind::Eof
    }

    /// Whether the current token's text matches (keywords, idents, punct).
    fn at(&self, text: &str) -> bool {
        !self.at_eof() && self.cur().text == text
    }

    fn bump(&mut self) {
        if !self.at_eof() {
            self.pos += 1;
        }
    }

    /// Consume the current token and return its span.
    fn bump_span(&mut self) -> Span {
        let s = self.cur_span();
        self.bump();
        s
    }

    fn cur_span(&self) -> Span {
        Span::new(self.cur().start as u32, self.cur().end as u32)
    }

    /// Span from `start` (byte offset) to the end of the last consumed token.
    fn span_since(&self, start: u32) -> Span {
        let end = if self.pos == 0 {
            start
        } else {
            self.tokens[self.pos - 1].end as u32
        };
        Span::new(start, end.max(start))
    }

    fn error(&mut self, span: Span, message: impl Into<String>) {
        self.diags
            .push(Diagnostic::error("PARSE001", message.into()).with_span(span));
    }

    /// Expect a specific token text; on failure emit a diagnostic only —
    /// structural recovery is handled by the enclosing loop.
    fn expect(&mut self, text: &str, what: &str) -> bool {
        if self.at(text) {
            self.bump();
            true
        } else {
            let span = self.cur_span();
            self.error(
                span,
                format!("expected {}, found {}", what, self.describe_cur()),
            );
            false
        }
    }

    /// Expect `;`. If missing, diagnose and skip forward until the next
    /// statement/item boundary, `;`, `}`, or EOF.
    fn expect_semi(&mut self) {
        if self.at(";") {
            self.bump();
            return;
        }
        if self.at_eof() || self.at("}") {
            let span = self.cur_span();
            self.error(span, format!("expected ';', found {}", self.describe_cur()));
            return;
        }
        let span = self.cur_span();
        self.error(span, format!("expected ';', found {}", self.describe_cur()));
        while !self.at_eof() && !self.at(";") && !self.at("}") && !self.at_stmt_start() {
            self.bump();
        }
        if self.at(";") {
            self.bump();
        }
    }

    fn describe_cur(&self) -> String {
        let t = self.cur();
        if t.kind == TokenKind::Eof {
            "end of file".to_string()
        } else {
            format!("'{}'", t.text)
        }
    }
}

// ---------- top level ----------

impl Parser {
    fn parse_file(&mut self) -> SourceFile {
        let start = self.cur().start as u32;
        let mut items = Vec::new();
        while !self.at_eof() {
            let before = self.pos;
            items.push(self.parse_item());
            if self.pos == before {
                // Guaranteed progress fallback.
                self.bump();
            }
        }
        let end = self.tokens.last().map(|t| t.end as u32).unwrap_or(start);
        SourceFile {
            span: Span::new(start, end),
            items,
        }
    }

    fn parse_item(&mut self) -> Item {
        let item_start = self.cur().start as u32;
        let annotations = self.parse_annotations();

        let kind = if self.at("using") {
            ItemKind::Using
        } else if self.at("import") {
            ItemKind::Import
        } else if self.at("class") {
            ItemKind::Class
        } else if self.at("enum") {
            ItemKind::Enum
        } else if self.at("function") {
            ItemKind::Function
        } else if self.at("var") || self.at("const") {
            ItemKind::Var
        } else if self.at("public") || self.at("private") || self.at("protected") {
            // Member without an enclosing class — parse permissively.
            ItemKind::Memberish
        } else {
            let span = self.cur_span();
            self.error(
                span,
                format!("expected declaration, found {}", self.describe_cur()),
            );
            self.recover_item();
            return Item::Error {
                span: self.span_since(item_start),
            };
        };

        match kind {
            ItemKind::Using => Item::Using(self.parse_using(item_start, annotations)),
            ItemKind::Import => Item::Import(self.parse_import(item_start, annotations)),
            ItemKind::Class => Item::Class(self.parse_class(item_start, annotations)),
            ItemKind::Enum => Item::Enum(self.parse_enum(item_start, annotations)),
            ItemKind::Function => {
                Item::Function(self.parse_function(item_start, annotations, None))
            }
            ItemKind::Var => Item::VarDecl(self.parse_var_decl(item_start, annotations)),
            ItemKind::Memberish => {
                let visibility = self.parse_visibility();
                if self.at("function") {
                    Item::Function(self.parse_function(item_start, annotations, visibility))
                } else {
                    Item::VarDecl(self.parse_var_decl(item_start, annotations))
                }
            }
        }
    }

    /// Skip tokens until the start of the next plausible item or EOF.
    fn recover_item(&mut self) {
        let mut depth = 0i64;
        let mut guard = 0usize;
        while !self.at_eof() && guard < 10_000 {
            guard += 1;
            if depth == 0 && self.at_item_sync() {
                break;
            }
            match self.cur().text.as_str() {
                "{" => depth += 1,
                "}" => depth -= 1,
                _ => {}
            }
            self.bump();
        }
    }

    fn at_item_sync(&self) -> bool {
        matches!(
            self.cur().text.as_str(),
            "class"
                | "enum"
                | "function"
                | "var"
                | "const"
                | "using"
                | "import"
                | "public"
                | "private"
                | "protected"
        ) || self.at_annotation_start()
    }

    fn at_annotation_start(&self) -> bool {
        self.at("(") && self.nth(1).text == ":"
    }

    fn at_stmt_start(&self) -> bool {
        if self.at_eof() {
            return true;
        }
        match self.cur().kind {
            TokenKind::Keyword => matches!(
                self.cur().text.as_str(),
                "if" | "while"
                    | "do"
                    | "for"
                    | "return"
                    | "break"
                    | "continue"
                    | "throw"
                    | "try"
                    | "switch"
                    | "var"
                    | "const"
                    | "class"
                    | "enum"
                    | "function"
            ),
            TokenKind::Punct => matches!(
                self.cur().text.as_str(),
                "{" | ";" | "(" | "[" | "!" | "-" | "+" | "~" | "++" | "--"
            ),
            TokenKind::Number | TokenKind::Str | TokenKind::CharLit | TokenKind::Ident => true,
            _ => false,
        }
    }

    // ---------- annotations & small decls ----------

    fn parse_annotations(&mut self) -> Vec<Annotation> {
        let mut out = Vec::new();
        while self.at_annotation_start() {
            out.push(self.parse_annotation());
        }
        out
    }

    fn parse_annotation(&mut self) -> Annotation {
        let start = self.cur().start as u32;
        self.bump(); // (
        self.bump(); // :
        let name = if matches!(self.cur().kind, TokenKind::Ident | TokenKind::Keyword) {
            let id = Ident::new(self.cur_span(), self.cur().text.clone());
            self.bump();
            id
        } else {
            let span = self.cur_span();
            self.error(
                span,
                format!("expected annotation name, found {}", self.describe_cur()),
            );
            Ident::new(span, "")
        };
        let mut args = Vec::new();
        while !self.at_eof() && !self.at(")") {
            if self.at(",") {
                self.bump();
                continue;
            }
            args.push(self.parse_expr());
        }
        self.expect(")", "')'");
        Annotation {
            span: self.span_since(start),
            name,
            args,
        }
    }

    fn parse_using(&mut self, start: u32, annotations: Vec<Annotation>) -> UsingDecl {
        self.bump(); // using
        let path = self.parse_dotted_path();
        let alias = if self.at("as") {
            self.bump();
            if self.cur().kind == TokenKind::Ident {
                let id = Ident::new(self.cur_span(), self.cur().text.clone());
                self.bump();
                Some(id)
            } else {
                let span = self.cur_span();
                self.error(span, "expected alias name after 'as'");
                None
            }
        } else {
            None
        };
        self.expect_semi();
        UsingDecl {
            span: self.span_since(start),
            annotations,
            path,
            alias,
        }
    }

    fn parse_import(&mut self, start: u32, annotations: Vec<Annotation>) -> ImportDecl {
        self.bump(); // import
        let path = self.parse_dotted_path();
        self.expect_semi();
        ImportDecl {
            span: self.span_since(start),
            annotations,
            path,
        }
    }

    /// `A.B.C` → three Idents.
    fn parse_dotted_path(&mut self) -> Vec<Ident> {
        let mut path = Vec::new();
        if self.cur().kind == TokenKind::Ident {
            path.push(Ident::new(self.cur_span(), self.cur().text.clone()));
            self.bump();
            while self.at(".") {
                self.bump();
                if self.cur().kind == TokenKind::Ident {
                    path.push(Ident::new(self.cur_span(), self.cur().text.clone()));
                    self.bump();
                } else {
                    let span = self.cur_span();
                    self.error(
                        span,
                        format!("expected name after '.', found {}", self.describe_cur()),
                    );
                    break;
                }
            }
        } else {
            let span = self.cur_span();
            self.error(
                span,
                format!("expected path, found {}", self.describe_cur()),
            );
        }
        path
    }

    /// Type reference: dotted path + optional `?`.
    fn parse_type(&mut self) -> TypeRef {
        let start = self.cur().start as u32;
        let path = self.parse_dotted_path();
        let nullable = if self.at("?") {
            self.bump();
            true
        } else {
            false
        };
        TypeRef {
            span: self.span_since(start),
            path,
            nullable,
        }
    }

    // ---------- class / enum / function / field ----------

    fn parse_class(&mut self, start: u32, annotations: Vec<Annotation>) -> ClassDecl {
        self.bump(); // class
        let name = self.parse_ident_required("class name");
        let superclass = if self.at("extends") {
            self.bump();
            Some(self.parse_type())
        } else {
            None
        };
        let mut members = Vec::new();
        self.expect("{", "'{'");
        while !self.at_eof() && !self.at("}") {
            let before = self.pos;
            members.push(self.parse_class_member());
            if self.pos == before {
                self.bump();
            }
        }
        self.expect("}", "'}'");
        ClassDecl {
            span: self.span_since(start),
            annotations,
            name,
            superclass,
            members,
        }
    }

    fn parse_class_member(&mut self) -> ClassMember {
        let start = self.cur().start as u32;
        let annotations = self.parse_annotations();
        let visibility = self.parse_visibility();

        if self.at("function") {
            let f = self.parse_function_inner(start, annotations, visibility);
            return ClassMember::Function(f);
        }
        if self.at("var") || self.at("const") {
            let decl = self.parse_field_decl(start, annotations, visibility);
            return ClassMember::Field(decl);
        }

        let span = self.cur_span();
        self.error(
            span,
            format!("expected class member, found {}", self.describe_cur()),
        );
        self.recover_member();
        ClassMember::Error {
            span: self.span_since(start),
        }
    }

    /// Skip to the next plausible class member or `}`.
    fn recover_member(&mut self) {
        let mut depth = 0i64;
        let mut guard = 0usize;
        while !self.at_eof() && !self.at("}") && guard < 10_000 {
            guard += 1;
            if depth == 0 && self.at_member_sync() {
                break;
            }
            match self.cur().text.as_str() {
                "{" => depth += 1,
                "}" => depth -= 1,
                _ => {}
            }
            self.bump();
        }
    }

    fn at_member_sync(&self) -> bool {
        matches!(
            self.cur().text.as_str(),
            "function" | "var" | "const" | "public" | "private" | "protected"
        ) || self.at_annotation_start()
    }

    fn parse_visibility(&mut self) -> Option<Visibility> {
        let v = match self.cur().text.as_str() {
            "public" => Visibility::Public,
            "private" => Visibility::Private,
            "protected" => Visibility::Protected,
            _ => return None,
        };
        self.bump();
        Some(v)
    }

    /// `var name (as Type)? (= expr)? ;` or `const ...` as a class field.
    fn parse_field_decl(
        &mut self,
        start: u32,
        annotations: Vec<Annotation>,
        visibility: Option<Visibility>,
    ) -> FieldDecl {
        let mutable = self.at("var");
        self.bump(); // var | const
        let name = self.parse_ident_required("field name");
        let type_ann = if self.at("as") {
            self.bump();
            Some(self.parse_type())
        } else {
            None
        };
        let init = if self.at("=") {
            self.bump();
            Some(self.parse_expr())
        } else {
            None
        };
        self.expect_semi();
        FieldDecl {
            span: self.span_since(start),
            annotations,
            visibility,
            mutable,
            name,
            type_ann,
            init,
        }
    }

    fn parse_enum(&mut self, start: u32, annotations: Vec<Annotation>) -> EnumDecl {
        self.bump(); // enum
        let name = self.parse_ident_required("enum name");
        let mut members = Vec::new();
        self.expect("{", "'{'");
        while !self.at_eof() && !self.at("}") {
            if self.at(",") || self.at(";") {
                self.bump();
                continue;
            }
            let mstart = self.cur().start as u32;
            if self.cur().kind == TokenKind::Ident {
                let mname = Ident::new(self.cur_span(), self.cur().text.clone());
                self.bump();
                let value = if self.at("=") {
                    self.bump();
                    Some(self.parse_expr())
                } else {
                    None
                };
                members.push(EnumMember {
                    span: self.span_since(mstart),
                    name: mname,
                    value,
                });
                if self.at(",") {
                    self.bump();
                }
            } else {
                let span = self.cur_span();
                self.error(
                    span,
                    format!("expected enum member, found {}", self.describe_cur()),
                );
                self.bump();
            }
        }
        self.expect("}", "'}'");
        EnumDecl {
            span: self.span_since(start),
            annotations,
            name,
            members,
        }
    }

    /// `function name(params) (as Type)? { body }` at item level.
    fn parse_function(
        &mut self,
        start: u32,
        annotations: Vec<Annotation>,
        visibility: Option<Visibility>,
    ) -> FunctionDecl {
        self.parse_function_inner(start, annotations, visibility)
    }

    fn parse_function_inner(
        &mut self,
        start: u32,
        annotations: Vec<Annotation>,
        visibility: Option<Visibility>,
    ) -> FunctionDecl {
        self.bump(); // function
        let name = self.parse_ident_required("function name");
        let mut params = Vec::new();
        self.expect("(", "'('");
        while !self.at_eof() && !self.at(")") {
            if self.at(",") {
                self.bump();
                continue;
            }
            // Stop at tokens that clearly end the parameter list so a broken
            // signature doesn't swallow the function body.
            if self.cur().kind == TokenKind::Keyword
                || matches!(self.cur().text.as_str(), "{" | "}" | ";")
            {
                break;
            }
            let before = self.pos;
            params.push(self.parse_param());
            if self.pos == before {
                self.bump();
            }
            if !self.at(",") && !self.at(")") {
                let span = self.cur_span();
                self.error(
                    span,
                    format!("expected ',' or ')', found {}", self.describe_cur()),
                );
            }
        }
        self.expect(")", "')'");
        let return_type = if self.at("as") {
            self.bump();
            Some(self.parse_type())
        } else {
            None
        };
        let body = self.parse_block();
        FunctionDecl {
            span: self.span_since(start),
            annotations,
            visibility,
            name,
            params,
            return_type,
            body,
        }
    }

    fn parse_param(&mut self) -> Param {
        let start = self.cur().start as u32;
        let name = if self.cur().kind == TokenKind::Ident {
            let id = Ident::new(self.cur_span(), self.cur().text.clone());
            self.bump();
            id
        } else {
            let span = self.cur_span();
            self.error(
                span,
                format!("expected parameter name, found {}", self.describe_cur()),
            );
            self.bump();
            Ident::new(span, "")
        };
        let type_ann = if self.at("as") {
            self.bump();
            Some(self.parse_type())
        } else {
            None
        };
        Param {
            span: self.span_since(start),
            name,
            type_ann,
        }
    }

    /// `var`/`const name (as Type)? (= expr)? ;` — single declarator.
    fn parse_var_decl(&mut self, start: u32, annotations: Vec<Annotation>) -> VarDecl {
        let mutable = self.at("var");
        self.bump(); // var | const
        let name = self.parse_ident_required("variable name");
        let type_ann = if self.at("as") {
            self.bump();
            Some(self.parse_type())
        } else {
            None
        };
        let init = if self.at("=") {
            self.bump();
            Some(self.parse_expr())
        } else {
            None
        };
        self.expect_semi();
        VarDecl {
            span: self.span_since(start),
            annotations,
            mutable,
            name,
            type_ann,
            init,
        }
    }

    fn parse_ident_required(&mut self, what: &str) -> Ident {
        if self.cur().kind == TokenKind::Ident {
            let id = Ident::new(self.cur_span(), self.cur().text.clone());
            self.bump();
            id
        } else {
            let span = self.cur_span();
            self.error(
                span,
                format!("expected {}, found {}", what, self.describe_cur()),
            );
            Ident::new(span, "")
        }
    }

    // ---------- blocks & statements ----------

    fn parse_block(&mut self) -> Block {
        let start = self.cur().start as u32;
        if !self.at("{") {
            let span = self.cur_span();
            self.error(
                span,
                format!("expected '{{', found {}", self.describe_cur()),
            );
            return Block {
                span,
                stmts: Vec::new(),
            };
        }
        self.bump(); // {
        let mut stmts = Vec::new();
        while !self.at_eof() && !self.at("}") {
            let before = self.pos;
            stmts.push(self.parse_stmt());
            if self.pos == before {
                self.bump();
            }
        }
        self.expect("}", "'}'");
        Block {
            span: self.span_since(start),
            stmts,
        }
    }

    fn parse_stmt(&mut self) -> Stmt {
        let start = self.cur().start as u32;
        match self.cur().text.as_str() {
            "{" => return Stmt::Block(self.parse_block()),
            ";" => {
                let span = self.bump_span();
                return Stmt::Empty { span };
            }
            "var" | "const" => {
                let mutable = self.at("var");
                self.bump(); // var | const
                let mut decls = Vec::new();
                loop {
                    let dstart = self.cur().start as u32;
                    let name = self.parse_ident_required("variable name");
                    let type_ann = if self.at("as") {
                        self.bump();
                        Some(self.parse_type())
                    } else {
                        None
                    };
                    let init = if self.at("=") {
                        self.bump();
                        Some(self.parse_expr())
                    } else {
                        None
                    };
                    decls.push(VarDecl {
                        span: self.span_since(dstart),
                        annotations: Vec::new(),
                        mutable,
                        name,
                        type_ann,
                        init,
                    });
                    if self.at(",") {
                        self.bump();
                        continue;
                    }
                    break;
                }
                self.expect_semi();
                return Stmt::VarDecl {
                    span: self.span_since(start),
                    decls,
                };
            }
            "if" => return self.parse_if(start),
            "while" => return self.parse_while(start),
            "do" => return self.parse_do_while(start),
            "for" => return self.parse_for(start),
            "return" => {
                self.bump();
                let value = if self.at(";") || self.at("}") || self.at_eof() {
                    None
                } else {
                    Some(self.parse_expr())
                };
                self.expect_semi();
                return Stmt::Return {
                    span: self.span_since(start),
                    value,
                };
            }
            "break" => {
                self.bump();
                self.expect_semi();
                return Stmt::Break {
                    span: self.span_since(start),
                };
            }
            "continue" => {
                self.bump();
                self.expect_semi();
                return Stmt::Continue {
                    span: self.span_since(start),
                };
            }
            "throw" => {
                self.bump();
                let expr = self.parse_expr();
                self.expect_semi();
                return Stmt::Throw {
                    span: self.span_since(start),
                    expr,
                };
            }
            "try" => return self.parse_try(start),
            "switch" => return self.parse_switch(start),
            _ => {}
        }

        // Expression statement.
        let expr = self.parse_expr();
        self.expect_semi();
        Stmt::Expr {
            span: self.span_since(start),
            expr,
        }
    }

    /// `( expr )` — used by if/while/do-while/switch conditions.
    fn parse_paren_expr(&mut self) -> Expr {
        if self.at("(") {
            self.bump();
        } else {
            let span = self.cur_span();
            self.error(span, format!("expected '(', found {}", self.describe_cur()));
        }
        let e = self.parse_expr();
        self.expect(")", "')'");
        e
    }

    fn parse_if(&mut self, start: u32) -> Stmt {
        self.bump(); // if
        let mut branches = Vec::new();
        loop {
            let cond = self.parse_paren_expr();
            let body = self.parse_block();
            branches.push((cond, body));
            if self.at("else") {
                self.bump();
                if self.at("if") {
                    self.bump(); // inner `if`; loop parses its condition
                    continue;
                }
                let else_body = self.parse_block();
                return Stmt::If {
                    span: self.span_since(start),
                    branches,
                    else_branch: Some(else_body),
                };
            }
            break;
        }
        Stmt::If {
            span: self.span_since(start),
            branches,
            else_branch: None,
        }
    }

    fn parse_while(&mut self, start: u32) -> Stmt {
        self.bump(); // while
        let cond = self.parse_paren_expr();
        let body = self.parse_block();
        Stmt::While {
            span: self.span_since(start),
            cond,
            body,
        }
    }

    fn parse_do_while(&mut self, start: u32) -> Stmt {
        self.bump(); // do
        let body = self.parse_block();
        if self.at("while") {
            self.bump();
        } else {
            let span = self.cur_span();
            self.error(
                span,
                format!(
                    "expected 'while' after do-block, found {}",
                    self.describe_cur()
                ),
            );
        }
        let cond = self.parse_paren_expr();
        self.expect_semi();
        Stmt::DoWhile {
            span: self.span_since(start),
            body,
            cond,
        }
    }

    fn parse_for(&mut self, start: u32) -> Stmt {
        self.bump(); // for
        self.expect("(", "'('");
        let init = if self.at(";") {
            None
        } else if self.at("var") || self.at("const") {
            let mutable = self.at("var");
            self.bump();
            let dstart = self.cur().start as u32;
            let name = self.parse_ident_required("variable name");
            let type_ann = if self.at("as") {
                self.bump();
                Some(self.parse_type())
            } else {
                None
            };
            let init_e = if self.at("=") {
                self.bump();
                Some(self.parse_expr())
            } else {
                None
            };
            Some(ForInit::VarDecl(VarDecl {
                span: self.span_since(dstart),
                annotations: Vec::new(),
                mutable,
                name,
                type_ann,
                init: init_e,
            }))
        } else {
            Some(ForInit::Expr(self.parse_expr()))
        };
        self.expect(";", "';'");
        let cond = if self.at(";") {
            None
        } else {
            Some(self.parse_expr())
        };
        self.expect(";", "';'");
        let update = if self.at(")") {
            None
        } else {
            Some(self.parse_expr())
        };
        self.expect(")", "')'");
        let body = self.parse_block();
        Stmt::For {
            span: self.span_since(start),
            init,
            cond,
            update,
            body,
        }
    }

    fn parse_try(&mut self, start: u32) -> Stmt {
        self.bump(); // try
        let body = self.parse_block();
        let mut catches = Vec::new();
        while self.at("catch") {
            let cstart = self.cur().start as u32;
            self.bump(); // catch
            let mut param = None;
            if self.at("(") {
                self.bump();
                if self.cur().kind == TokenKind::Ident {
                    param = Some(Ident::new(self.cur_span(), self.cur().text.clone()));
                    self.bump();
                }
                self.expect(")", "')'");
            }
            let cbody = self.parse_block();
            catches.push(CatchClause {
                span: self.span_since(cstart),
                param,
                body: cbody,
            });
        }
        let finally = if self.at("finally") {
            self.bump();
            Some(self.parse_block())
        } else {
            None
        };
        Stmt::Try {
            span: self.span_since(start),
            body,
            catches,
            finally,
        }
    }

    fn parse_switch(&mut self, start: u32) -> Stmt {
        self.bump(); // switch
        let expr = self.parse_paren_expr();
        let mut cases = Vec::new();
        self.expect("{", "'{'");
        while !self.at_eof() && !self.at("}") {
            let cstart = self.cur().start as u32;
            if self.at("case") {
                self.bump();
                let label = self.parse_expr();
                self.expect(":", "':'");
                let stmts = self.parse_case_stmts();
                cases.push(SwitchCase {
                    span: self.span_since(cstart),
                    label: Some(label),
                    stmts,
                });
            } else if self.at("default") {
                self.bump();
                self.expect(":", "':'");
                let stmts = self.parse_case_stmts();
                cases.push(SwitchCase {
                    span: self.span_since(cstart),
                    label: None,
                    stmts,
                });
            } else {
                let span = self.cur_span();
                self.error(
                    span,
                    format!(
                        "expected 'case' or 'default', found {}",
                        self.describe_cur()
                    ),
                );
                self.bump();
            }
        }
        self.expect("}", "'}'");
        Stmt::Switch {
            span: self.span_since(start),
            expr,
            cases,
        }
    }

    /// Statements inside a switch case, up to the next `case`/`default`/`}`.
    fn parse_case_stmts(&mut self) -> Vec<Stmt> {
        let mut stmts = Vec::new();
        while !self.at_eof() && !self.at("}") && !self.at("case") && !self.at("default") {
            let before = self.pos;
            stmts.push(self.parse_stmt());
            if self.pos == before {
                self.bump();
            }
        }
        stmts
    }

    // ---------- expressions ----------

    /// Binding power for a binary operator at the current position.
    fn binop_bp(&self) -> Option<(BinOp, u8)> {
        let (op, lbp) = match self.cur().text.as_str() {
            "||" => (BinOp::OrOr, 5),
            "&&" => (BinOp::AndAnd, 7),
            "|" => (BinOp::BitOr, 9),
            "^" => (BinOp::BitXor, 11),
            "&" => (BinOp::BitAnd, 13),
            "==" => (BinOp::Eq, 15),
            "!=" => (BinOp::Ne, 15),
            "<" => (BinOp::Lt, 17),
            "<=" => (BinOp::Le, 17),
            ">" => (BinOp::Gt, 17),
            ">=" => (BinOp::Ge, 17),
            "instanceof" => (BinOp::InstanceOf, 17),
            "in" => (BinOp::In, 17),
            "<<" => (BinOp::Shl, 19),
            ">>" => (BinOp::Shr, 19),
            ">>>" => (BinOp::Ushr, 19),
            "+" => (BinOp::Add, 21),
            "-" => (BinOp::Sub, 21),
            "*" => (BinOp::Mul, 23),
            "/" => (BinOp::Div, 23),
            "%" => (BinOp::Mod, 23),
            _ => return None,
        };
        Some((op, lbp))
    }

    fn assign_op(&self) -> Option<AssignOp> {
        Some(match self.cur().text.as_str() {
            "=" => AssignOp::Assign,
            "+=" => AssignOp::Add,
            "-=" => AssignOp::Sub,
            "*=" => AssignOp::Mul,
            "/=" => AssignOp::Div,
            "%=" => AssignOp::Mod,
            "<<=" => AssignOp::Shl,
            ">>=" => AssignOp::Shr,
            ">>>=" => AssignOp::Ushr,
            "&=" => AssignOp::BitAnd,
            "|=" => AssignOp::BitOr,
            "^=" => AssignOp::BitXor,
            _ => return None,
        })
    }

    fn parse_expr(&mut self) -> Expr {
        self.parse_expr_bp(0)
    }

    fn parse_expr_bp(&mut self, min_bp: u8) -> Expr {
        let mut lhs = self.parse_prefix();
        loop {
            if self.at_eof() {
                break;
            }
            // Ternary `? :` (bp 3, right assoc).
            if self.at("?") && min_bp <= 3 {
                let start = lhs.span().start;
                self.bump(); // ?
                let then = self.parse_expr_bp(0);
                if self.at(":") {
                    self.bump();
                } else {
                    let span = self.cur_span();
                    self.error(
                        span,
                        format!("expected ':' in ternary, found {}", self.describe_cur()),
                    );
                }
                let else_ = self.parse_expr_bp(3);
                lhs = Expr::Ternary {
                    span: self.span_since(start),
                    cond: Box::new(lhs),
                    then: Box::new(then),
                    else_: Box::new(else_),
                };
                continue;
            }
            // Assignment (bp 1, right assoc).
            if let Some(op) = self.assign_op() {
                if min_bp > 1 {
                    break;
                }
                let start = lhs.span().start;
                self.bump();
                let value = self.parse_expr_bp(1);
                lhs = Expr::Assign {
                    span: self.span_since(start),
                    op,
                    target: Box::new(lhs),
                    value: Box::new(value),
                };
                continue;
            }
            let Some((op, lbp)) = self.binop_bp() else {
                break;
            };
            if lbp < min_bp {
                break;
            }
            let start = lhs.span().start;
            self.bump();
            let right = self.parse_expr_bp(lbp + 1);
            lhs = Expr::Binary {
                span: self.span_since(start),
                op,
                left: Box::new(lhs),
                right: Box::new(right),
            };
        }
        lhs
    }

    fn parse_prefix(&mut self) -> Expr {
        let start = self.cur().start as u32;
        match self.cur().text.as_str() {
            "!" | "-" | "+" | "~" | "++" | "--" => {
                let op = match self.cur().text.as_str() {
                    "!" => UnOp::Not,
                    "-" => UnOp::Neg,
                    "+" => UnOp::Pos,
                    "~" => UnOp::BitNot,
                    "++" => UnOp::PreInc,
                    "--" => UnOp::PreDec,
                    _ => unreachable!(),
                };
                self.bump();
                let expr = self.parse_prefix();
                Expr::Unary {
                    span: self.span_since(start),
                    op,
                    expr: Box::new(expr),
                }
            }
            "new" => {
                self.bump(); // new
                let type_ = self.parse_type();
                let args = if self.at("(") {
                    self.parse_args()
                } else {
                    Vec::new()
                };
                Expr::New {
                    span: self.span_since(start),
                    type_,
                    args,
                }
            }
            "(" => {
                self.bump(); // (
                let e = self.parse_expr();
                self.expect(")", "')'");
                Expr::Paren {
                    span: self.span_since(start),
                    expr: Box::new(e),
                }
            }
            "[" => {
                self.bump(); // [
                let mut elems = Vec::new();
                while !self.at_eof() && !self.at("]") {
                    if self.at(",") {
                        self.bump();
                        continue;
                    }
                    elems.push(self.parse_expr());
                    if self.at(",") {
                        self.bump();
                    } else {
                        break;
                    }
                }
                self.expect("]", "']'");
                Expr::Array {
                    span: self.span_since(start),
                    elems,
                }
            }
            "{" => {
                // Dictionary literal `{key: value, ...}` in expression position.
                self.bump(); // {
                let mut entries = Vec::new();
                while !self.at_eof() && !self.at("}") {
                    if self.at(",") {
                        self.bump();
                        continue;
                    }
                    let key = self.parse_expr();
                    if self.at(":") || self.at("=>") {
                        self.bump();
                    } else {
                        let span = self.cur_span();
                        self.error(
                            span,
                            format!(
                                "expected ':' in dictionary entry, found {}",
                                self.describe_cur()
                            ),
                        );
                    }
                    let value = self.parse_expr();
                    entries.push((key, value));
                    if self.at(",") {
                        self.bump();
                    } else {
                        break;
                    }
                }
                self.expect("}", "'}'");
                Expr::Dict {
                    span: self.span_since(start),
                    entries,
                }
            }
            "true" | "false" => {
                let is_true = self.at("true");
                let span = self.bump_span();
                Expr::Literal(Literal::Bool {
                    span,
                    value: is_true,
                })
            }
            "null" => {
                let span = self.bump_span();
                Expr::Literal(Literal::Null { span })
            }
            "this" => {
                let span = self.bump_span();
                Expr::This { span }
            }
            "super" => {
                let span = self.bump_span();
                Expr::Super { span }
            }
            _ => match self.cur().kind {
                TokenKind::Number | TokenKind::Str | TokenKind::CharLit => {
                    let span = self.bump_span();
                    let text = self.tokens[self.pos - 1].text.clone();
                    let lit = match self.tokens[self.pos - 1].kind {
                        TokenKind::Number => Literal::Number { span, text },
                        TokenKind::Str => Literal::Str { span, text },
                        _ => Literal::Char { span, text },
                    };
                    self.parse_postfix(Expr::Literal(lit))
                }
                TokenKind::Ident => {
                    let id = Ident::new(self.cur_span(), self.cur().text.clone());
                    self.bump();
                    self.parse_postfix(Expr::Ident(id))
                }
                TokenKind::Keyword => {
                    let span = self.bump_span();
                    self.error(
                        span,
                        format!(
                            "expected expression, found keyword '{}'",
                            self.tokens[self.pos - 1].text
                        ),
                    );
                    Expr::Missing { span }
                }
                _ => {
                    let span = self.cur_span();
                    self.error(
                        span,
                        format!("expected expression, found {}", self.describe_cur()),
                    );
                    // Don't consume closing delimiters — they belong to the
                    // enclosing construct and must survive for recovery.
                    if !matches!(self.cur().text.as_str(), ")" | "]" | "}" | "," | ";") {
                        self.bump();
                    }
                    Expr::Missing { span }
                }
            },
        }
    }

    /// Postfix loop: calls, indexing, member access, `as` casts, `++`/`--`.
    fn parse_postfix(&mut self, mut e: Expr) -> Expr {
        loop {
            if self.at("(") {
                let callee = collapse_path(std::mem::replace(
                    &mut e,
                    Expr::Missing {
                        span: Span::new(0, 0),
                    },
                ));
                let args = self.parse_args();
                let start = callee.span().start;
                e = Expr::Call {
                    span: self.span_since(start),
                    callee: Box::new(callee),
                    args,
                };
                continue;
            }
            if self.at("[") {
                self.bump(); // [
                let index = self.parse_expr();
                self.expect("]", "']'");
                let object = collapse_path(std::mem::replace(
                    &mut e,
                    Expr::Missing {
                        span: Span::new(0, 0),
                    },
                ));
                let start = object.span().start;
                e = Expr::Index {
                    span: self.span_since(start),
                    object: Box::new(object),
                    index: Box::new(index),
                };
                continue;
            }
            if self.at(".") {
                self.bump(); // .
                if self.cur().kind == TokenKind::Ident {
                    let name = Ident::new(self.cur_span(), self.cur().text.clone());
                    self.bump();
                    let start = e.span().start;
                    e = Expr::Member {
                        span: self.span_since(start),
                        object: Box::new(e),
                        name,
                    };
                    continue;
                }
                let span = self.cur_span();
                self.error(
                    span,
                    format!(
                        "expected member name after '.', found {}",
                        self.describe_cur()
                    ),
                );
                break;
            }
            if self.at("as") {
                self.bump(); // as
                let inner = collapse_path(std::mem::replace(
                    &mut e,
                    Expr::Missing {
                        span: Span::new(0, 0),
                    },
                ));
                let type_ = self.parse_type();
                let start = inner.span().start;
                e = Expr::Cast {
                    span: self.span_since(start),
                    expr: Box::new(inner),
                    type_,
                };
                continue;
            }
            if self.at("++") || self.at("--") {
                let op = if self.at("++") {
                    PostfixOp::Inc
                } else {
                    PostfixOp::Dec
                };
                self.bump();
                let start = e.span().start;
                e = Expr::Postfix {
                    span: self.span_since(start),
                    op,
                    expr: Box::new(e),
                };
                continue;
            }
            break;
        }
        collapse_path(e)
    }

    /// `(a, b, c)` argument list.
    fn parse_args(&mut self) -> Vec<Expr> {
        let mut args = Vec::new();
        self.bump(); // (
        while !self.at_eof() && !self.at(")") {
            if self.at(",") {
                self.bump();
                continue;
            }
            args.push(self.parse_expr());
            if self.at(",") {
                self.bump();
            } else {
                break;
            }
        }
        self.expect(")", "')'");
        args
    }
}

// ---------- free helpers ----------

#[derive(Clone, Copy)]
enum ItemKind {
    Using,
    Import,
    Class,
    Enum,
    Function,
    Var,
    Memberish,
}

/// Collapse `a.b.c` member chains rooted at an identifier into `Expr::Path`.
fn collapse_path(e: Expr) -> Expr {
    fn segments(e: &Expr, out: &mut Vec<Ident>) -> bool {
        match e {
            Expr::Ident(i) => {
                out.push(i.clone());
                true
            }
            Expr::Member { object, name, .. } => {
                let ok = segments(object, out);
                if ok {
                    out.push(name.clone());
                }
                ok
            }
            _ => false,
        }
    }
    let mut segs = Vec::new();
    if segments(&e, &mut segs) && segs.len() > 1 {
        let span = e.span();
        Expr::Path {
            span,
            segments: segs,
        }
    } else {
        e
    }
}
