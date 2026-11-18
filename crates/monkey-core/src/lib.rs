//! Shared source spans, positions, and diagnostics for the Monkey toolchain.
//!
//! Every language-facing crate (`monkey-syntax`, `monkey-parser`,
//! `monkey-linter`, `monkey-formatter`, and future analyzer/LSP crates)
//! reports problems through [`Diagnostic`] so that CLI, editor, and CI
//! integrations see consistent codes, severities, and locations.
//!
//! The crate is dependency-free by design.

mod diagnostic;
mod map;
mod render;
mod span;

pub use diagnostic::{CodeSuggestion, Diagnostic, Severity};
pub use map::SourceMap;
pub use render::{
    diagnostics_json, diagnostics_json_all, diagnostics_sarif, render_diagnostics, FileDiagnostics,
};
pub use span::Span;
