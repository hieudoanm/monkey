//! AST for Monkey C.
//!
//! Every node carries a [`Span`] of byte offsets into the source file.
//! The AST is deliberately typed (enums/structs, no trait objects) and
//! preserves enough structure for formatting-adjacent tooling, lint rules,
//! and future language intelligence.

use monkey_core::Span;

/// A source file: a sequence of top-level items.
#[derive(Clone, Debug)]
pub struct SourceFile {
    pub span: Span,
    pub items: Vec<Item>,
}

/// An identifier with its source span.
#[derive(Clone, Debug)]
pub struct Ident {
    pub span: Span,
    pub text: String,
}

impl Ident {
    pub fn new(span: Span, text: impl Into<String>) -> Self {
        Self {
            span,
            text: text.into(),
        }
    }
}

/// A type reference: `Lang.Number`, `Lang.Number?`, `[View]`, or a union
/// like `[Views] or [Views, InputDelegates]`.
#[derive(Clone, Debug)]
pub enum TypeRef {
    /// Dotted path + optional `?`.
    Path {
        span: Span,
        /// Dotted path segments, e.g. `["Lang", "Number"]`.
        path: Vec<Ident>,
        /// Whether the type is nullable (`Type?`).
        nullable: bool,
    },
    /// `[T]` or `[T, U, ...]` array/tuple type.
    Array {
        span: Span,
        elems: Vec<TypeRef>,
        /// Whether the type is nullable (`[T]?`).
        nullable: bool,
    },
    /// `A or B or ...` union type.
    Union { span: Span, variants: Vec<TypeRef> },
    /// `Foo<Bar, Baz>` generic type (e.g. `Array<Number>`).
    Generic {
        span: Span,
        /// Dotted path of the generic type, e.g. `["Array"]`.
        path: Vec<Ident>,
        args: Vec<TypeRef>,
        /// Whether the type is nullable (`Array<T>?`).
        nullable: bool,
    },
    /// `Method(A, B) as R` callable/method type.
    Method {
        span: Span,
        /// Dotted path of the method type (usually `["Method"]`).
        path: Vec<Ident>,
        params: Vec<TypeRef>,
        /// Return type after `as`, if present.
        ret: Option<Box<TypeRef>>,
        /// Whether the type is nullable.
        nullable: bool,
    },
    /// `{ :key as Type, ... }` dictionary type literal.
    Dict {
        span: Span,
        entries: Vec<DictTypeEntry>,
        /// Whether the type is nullable.
        nullable: bool,
    },
}

/// One entry in a dictionary type literal: `:name as Type`.
#[derive(Clone, Debug)]
pub struct DictTypeEntry {
    pub span: Span,
    pub name: Ident,
    pub type_: TypeRef,
}

impl TypeRef {
    /// Source span of the type.
    pub fn span(&self) -> Span {
        match self {
            TypeRef::Path { span, .. }
            | TypeRef::Array { span, .. }
            | TypeRef::Union { span, .. }
            | TypeRef::Generic { span, .. }
            | TypeRef::Method { span, .. }
            | TypeRef::Dict { span, .. } => *span,
        }
    }

    /// Mark the type nullable (`?` suffix). For unions the marker is
    /// applied to the last variant.
    pub fn set_nullable(&mut self) {
        match self {
            TypeRef::Path { nullable, .. }
            | TypeRef::Array { nullable, .. }
            | TypeRef::Generic { nullable, .. }
            | TypeRef::Method { nullable, .. }
            | TypeRef::Dict { nullable, .. } => *nullable = true,
            TypeRef::Union { variants, .. } => {
                if let Some(last) = variants.last_mut() {
                    last.set_nullable();
                }
            }
        }
    }
}

/// An annotation `(:name)` or `(:name, arg, ...)`.
#[derive(Clone, Debug)]
pub struct Annotation {
    pub span: Span,
    pub name: Ident,
    pub args: Vec<Expr>,
}

/// Access modifier on a member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
    Protected,
}

/// A top-level item.
#[derive(Clone, Debug)]
pub enum Item {
    /// `using Foo.Bar;` or `using Foo.Bar as Baz;`
    Using(UsingDecl),
    /// `import some.path;` (permissive; rarely used in Monkey C).
    Import(ImportDecl),
    Class(ClassDecl),
    Enum(EnumDecl),
    /// Top-level `function` — not idiomatic Monkey C, parsed for recovery.
    Function(FunctionDecl),
    /// `var`/`const` at top level — parsed for recovery.
    VarDecl(VarDecl),
    /// `module Name { ... }` namespace/module declaration.
    Module(ModuleDecl),
    /// `typedef Name as Type | interface { ... } | { dict-type }`.
    Typedef(TypedefDecl),
    /// An item the parser could not understand; kept for tooling.
    Error {
        span: Span,
    },
}

#[derive(Clone, Debug)]
pub struct UsingDecl {
    pub span: Span,
    pub annotations: Vec<Annotation>,
    /// `using A.B.C` → `["A","B","C"]`.
    pub path: Vec<Ident>,
    /// `as Alias` if present.
    pub alias: Option<Ident>,
}

#[derive(Clone, Debug)]
pub struct ImportDecl {
    pub span: Span,
    pub annotations: Vec<Annotation>,
    pub path: Vec<Ident>,
}

/// A `module Name { ... }` declaration. Members are regular items
/// (`var`/`const`/`function`, possibly `hidden`).
#[derive(Clone, Debug)]
pub struct ModuleDecl {
    pub span: Span,
    pub annotations: Vec<Annotation>,
    pub name: Ident,
    pub members: Vec<Item>,
}

/// A `typedef` declaration.
#[derive(Clone, Debug)]
pub struct TypedefDecl {
    pub span: Span,
    pub annotations: Vec<Annotation>,
    pub name: Ident,
    pub target: TypedefTarget,
}

/// What a `typedef` aliases.
#[derive(Clone, Debug)]
pub enum TypedefTarget {
    /// `typedef X as SomeType;` — any type expression.
    Type(TypeRef),
    /// `typedef X as interface { function sig(...); ... };`
    Interface {
        span: Span,
        /// Signature-only function declarations (empty bodies).
        members: Vec<FunctionDecl>,
    },
}

#[derive(Clone, Debug)]
pub struct ClassDecl {
    pub span: Span,
    pub annotations: Vec<Annotation>,
    pub name: Ident,
    /// `extends Super` type, if present.
    pub superclass: Option<TypeRef>,
    pub members: Vec<ClassMember>,
}

#[derive(Clone, Debug)]
pub enum ClassMember {
    Field(FieldDecl),
    Function(FunctionDecl),
    /// Nested `enum` declaration inside a class.
    Enum(EnumDecl),
    /// Nested `class` declaration inside a class.
    Class(Box<ClassDecl>),
    /// `typedef` inside a class body.
    Typedef(TypedefDecl),
    Error {
        span: Span,
    },
}

#[derive(Clone, Debug)]
pub struct FieldDecl {
    pub span: Span,
    pub annotations: Vec<Annotation>,
    pub visibility: Option<Visibility>,
    /// `true` for `var`, `false` for `const`.
    pub mutable: bool,
    pub name: Ident,
    pub type_ann: Option<TypeRef>,
    pub init: Option<Expr>,
}

#[derive(Clone, Debug)]
pub struct FunctionDecl {
    pub span: Span,
    pub annotations: Vec<Annotation>,
    pub visibility: Option<Visibility>,
    pub name: Ident,
    pub params: Vec<Param>,
    /// Return type after `as`.
    pub return_type: Option<TypeRef>,
    pub body: Block,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub span: Span,
    pub name: Ident,
    /// Type after `as`, e.g. `dc as Dc`.
    pub type_ann: Option<TypeRef>,
}

#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub span: Span,
    pub annotations: Vec<Annotation>,
    pub name: Ident,
    pub members: Vec<EnumMember>,
}

#[derive(Clone, Debug)]
pub struct EnumMember {
    pub span: Span,
    pub name: Ident,
    /// Optional `= value`.
    pub value: Option<Expr>,
}

/// A `{ ... }` block of statements.
#[derive(Clone, Debug)]
pub struct Block {
    pub span: Span,
    pub stmts: Vec<Stmt>,
}

/// A variable declaration statement or class field initializer list.
/// `var a = 1, b = 2;` produces two `VarDecl`s under one `Stmt::VarDecl`.
#[derive(Clone, Debug)]
pub struct VarDecl {
    pub span: Span,
    pub annotations: Vec<Annotation>,
    /// `true` for `var`, `false` for `const`.
    pub mutable: bool,
    pub name: Ident,
    pub type_ann: Option<TypeRef>,
    pub init: Option<Expr>,
}

/// Statements.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum Stmt {
    Block(Block),
    /// `var`/`const` declaration, possibly multiple declarators.
    VarDecl {
        span: Span,
        decls: Vec<VarDecl>,
    },
    /// Expression statement (call, assignment, etc.).
    Expr {
        span: Span,
        expr: Expr,
    },
    If {
        span: Span,
        /// `(condition, body)` per branch; first is `if`, rest are `else if`.
        branches: Vec<(Expr, Block)>,
        else_branch: Option<Block>,
    },
    While {
        span: Span,
        cond: Expr,
        body: Block,
    },
    DoWhile {
        span: Span,
        body: Block,
        cond: Expr,
    },
    For {
        span: Span,
        init: Option<ForInit>,
        cond: Option<Expr>,
        update: Option<Expr>,
        body: Block,
    },
    Return {
        span: Span,
        value: Option<Expr>,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    Throw {
        span: Span,
        expr: Expr,
    },
    Try {
        span: Span,
        body: Block,
        catches: Vec<CatchClause>,
        finally: Option<Block>,
    },
    Switch {
        span: Span,
        expr: Expr,
        cases: Vec<SwitchCase>,
    },
    /// Lone `;`.
    Empty {
        span: Span,
    },
    /// Statement the parser could not understand.
    Error {
        span: Span,
    },
}

#[derive(Clone, Debug)]
pub enum ForInit {
    /// `var i = 0` inside the for-parens.
    VarDecl(VarDecl),
    Expr(Expr),
}

#[derive(Clone, Debug)]
pub struct CatchClause {
    pub span: Span,
    /// Exception variable name, if declared (`catch (e)`).
    pub param: Option<Ident>,
    /// `catch (e instanceof MyException)` type filter, if present.
    pub exception_type: Option<TypeRef>,
    pub body: Block,
}

#[derive(Clone, Debug)]
pub struct SwitchCase {
    pub span: Span,
    /// `None` for `default:`.
    pub label: Option<Expr>,
    pub stmts: Vec<Stmt>,
}

/// Expressions.
// Enum variants differ in size (e.g. `Dict` holds a Vec); boxing every
// large variant would add allocation noise for little gain in practice.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum Expr {
    /// Could not parse an expression; span covers skipped tokens.
    Missing {
        span: Span,
    },
    Literal(Literal),
    /// Single identifier.
    Ident(Ident),
    /// `this`.
    This {
        span: Span,
    },
    /// `super` (used as `super.foo()`).
    Super {
        span: Span,
    },
    /// `a.b.c` dotted path used as a value (e.g. `Graphics.COLOR_WHITE`).
    Path {
        span: Span,
        segments: Vec<Ident>,
    },
    /// `expr(args)`.
    Call {
        span: Span,
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    /// `expr.name`.
    Member {
        span: Span,
        object: Box<Expr>,
        name: Ident,
    },
    /// `expr[index]`.
    Index {
        span: Span,
        object: Box<Expr>,
        index: Box<Expr>,
    },
    /// `op expr` — `!`, `-`, `+`, `~`, `++`, `--`.
    Unary {
        span: Span,
        op: UnOp,
        expr: Box<Expr>,
    },
    /// `expr++` / `expr--`.
    Postfix {
        span: Span,
        op: PostfixOp,
        expr: Box<Expr>,
    },
    /// `left op right`.
    Binary {
        span: Span,
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    /// `target op= value` or `target = value`.
    Assign {
        span: Span,
        op: AssignOp,
        target: Box<Expr>,
        value: Box<Expr>,
    },
    /// `cond ? then : else`.
    Ternary {
        span: Span,
        cond: Box<Expr>,
        then: Box<Expr>,
        else_: Box<Expr>,
    },
    /// `new Type(args)`.
    New {
        span: Span,
        type_: TypeRef,
        args: Vec<Expr>,
    },
    /// `expr as Type`.
    Cast {
        span: Span,
        expr: Box<Expr>,
        type_: TypeRef,
    },
    /// `[a, b, c]` or byte array `[a, b]b`.
    Array {
        span: Span,
        elems: Vec<Expr>,
        /// `true` for the `b` suffix (`[0x01, 0x00]b`).
        byte_array: bool,
    },
    /// `{"k": v, ...}` — permissive; Monkey C dictionary literals.
    Dict {
        span: Span,
        entries: Vec<(Expr, Expr)>,
    },
    /// `(expr)`.
    Paren {
        span: Span,
        expr: Box<Expr>,
    },
}

#[derive(Clone, Debug)]
pub enum Literal {
    /// Raw literal text (keeps original formatting, e.g. `0xFF`, `60.0`).
    Number {
        span: Span,
        text: String,
    },
    Str {
        span: Span,
        text: String,
    },
    Char {
        span: Span,
        text: String,
    },
    Bool {
        span: Span,
        value: bool,
    },
    Null {
        span: Span,
    },
    /// `:name` symbol literal (used by `method()`, `has`, dict keys).
    Symbol {
        span: Span,
        text: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Not,
    Neg,
    Pos,
    BitNot,
    PreInc,
    PreDec,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostfixOp {
    Inc,
    Dec,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    OrOr,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Ushr,
    InstanceOf,
    In,
    Has,
}

impl BinOp {
    pub fn as_str(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::AndAnd => "&&",
            BinOp::OrOr => "||",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
            BinOp::Ushr => ">>>",
            BinOp::InstanceOf => "instanceof",
            BinOp::In => "in",
            BinOp::Has => "has",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Shl,
    Shr,
    Ushr,
    BitAnd,
    BitOr,
    BitXor,
}

impl AssignOp {
    pub fn as_str(self) -> &'static str {
        match self {
            AssignOp::Assign => "=",
            AssignOp::Add => "+=",
            AssignOp::Sub => "-=",
            AssignOp::Mul => "*=",
            AssignOp::Div => "/=",
            AssignOp::Mod => "%=",
            AssignOp::Shl => "<<=",
            AssignOp::Shr => ">>=",
            AssignOp::Ushr => ">>>=",
            AssignOp::BitAnd => "&=",
            AssignOp::BitOr => "|=",
            AssignOp::BitXor => "^=",
        }
    }
}

impl Expr {
    /// Source span of the expression.
    pub fn span(&self) -> Span {
        match self {
            Expr::Missing { span }
            | Expr::This { span }
            | Expr::Super { span }
            | Expr::Path { span, .. }
            | Expr::Call { span, .. }
            | Expr::Member { span, .. }
            | Expr::Index { span, .. }
            | Expr::Unary { span, .. }
            | Expr::Postfix { span, .. }
            | Expr::Binary { span, .. }
            | Expr::Assign { span, .. }
            | Expr::Ternary { span, .. }
            | Expr::New { span, .. }
            | Expr::Cast { span, .. }
            | Expr::Array { span, .. }
            | Expr::Dict { span, .. }
            | Expr::Paren { span, .. } => *span,
            Expr::Ident(i) => i.span,
            Expr::Literal(l) => match l {
                Literal::Number { span, .. }
                | Literal::Str { span, .. }
                | Literal::Char { span, .. }
                | Literal::Bool { span, .. }
                | Literal::Null { span }
                | Literal::Symbol { span, .. } => *span,
            },
        }
    }
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::Block(b) => b.span,
            Stmt::VarDecl { span, .. }
            | Stmt::Expr { span, .. }
            | Stmt::If { span, .. }
            | Stmt::While { span, .. }
            | Stmt::DoWhile { span, .. }
            | Stmt::For { span, .. }
            | Stmt::Return { span, .. }
            | Stmt::Break { span }
            | Stmt::Continue { span }
            | Stmt::Throw { span, .. }
            | Stmt::Try { span, .. }
            | Stmt::Switch { span, .. }
            | Stmt::Empty { span }
            | Stmt::Error { span } => *span,
        }
    }
}

impl Item {
    pub fn span(&self) -> Span {
        match self {
            Item::Using(d) => d.span,
            Item::Import(d) => d.span,
            Item::Class(d) => d.span,
            Item::Enum(d) => d.span,
            Item::Function(d) => d.span,
            Item::VarDecl(d) => d.span,
            Item::Module(d) => d.span,
            Item::Typedef(d) => d.span,
            Item::Error { span } => *span,
        }
    }
}
