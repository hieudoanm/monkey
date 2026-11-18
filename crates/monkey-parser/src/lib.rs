//! Recursive-descent parser and typed AST for Monkey C.
//!
//! [`parse`] always succeeds: it returns a [`SourceFile`] plus diagnostics,
//! using [`Item::Error`]/[`Stmt::Error`]/[`Expr::Missing`] nodes for
//! unrecoverable fragments so tooling can operate on broken code.

pub mod ast;
mod parser;
pub mod visit;

pub use parser::{parse, ParseResult};
