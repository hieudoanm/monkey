//! Lexer and token model for Monkey C.
//!
//! [`lex`] is the permissive entry point (tokens only); [`lex_full`]
//! additionally returns lexical diagnostics with source spans.

mod lexer;
mod token;

pub use lexer::{is_control_keyword, lex, lex_full, LexResult};
pub use token::{Token, TokenKind, CONTROL_KEYWORDS, KEYWORDS};
