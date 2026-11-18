//! Diagnostic model shared by lexer, parser, linter, and analyzer.

use crate::span::Span;

/// How severe a diagnostic is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Something is definitely wrong.
    Error,
    /// Something is probably wrong.
    Warning,
    /// Worth knowing, not a problem.
    Information,
    /// Extra context or stylistic guidance.
    Hint,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Information => "information",
            Severity::Hint => "hint",
        }
    }
}

/// A concrete edit attached to a diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeSuggestion {
    /// Source range to replace.
    pub span: Span,
    /// Text that should replace the spanned source.
    pub replacement: String,
}

/// A single diagnostic with a stable code, severity, message, and optional
/// source location.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    /// Stable machine-readable identifier, e.g. `no-trailing-whitespace`,
    /// `LEX001`, `PARSE001`.
    pub code: String,
    pub severity: Severity,
    pub message: String,
    /// Primary source location, if any.
    pub span: Option<Span>,
    /// Additional context lines rendered under the primary message.
    pub notes: Vec<String>,
    /// Optional remediation hint.
    pub help: Option<String>,
    /// Optional concrete edits.
    pub suggestions: Vec<CodeSuggestion>,
}

impl Diagnostic {
    pub fn new(code: impl Into<String>, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity,
            message: message.into(),
            span: None,
            notes: Vec::new(),
            help: None,
            suggestions: Vec::new(),
        }
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Error, message)
    }

    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Warning, message)
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn with_span_opt(mut self, span: Option<Span>) -> Self {
        self.span = span;
        self
    }

    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn suggest(mut self, suggestion: CodeSuggestion) -> Self {
        self.suggestions.push(suggestion);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_works() {
        let d = Diagnostic::warning("rule-x", "something is off")
            .with_span(Span::new(0, 3))
            .note("extra context")
            .help("try this")
            .suggest(CodeSuggestion {
                span: Span::new(0, 1),
                replacement: "y".into(),
            });
        assert_eq!(d.code, "rule-x");
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.span, Some(Span::new(0, 3)));
        assert_eq!(d.notes.len(), 1);
        assert!(d.help.is_some());
        assert_eq!(d.suggestions.len(), 1);
    }
}
