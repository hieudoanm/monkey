//! Language-agnostic text rules (also applied to `.jungle` files).

use monkey_core::{Diagnostic, SourceMap, Span};

fn span_of(line_start: u32, start_col_bytes: usize, end_col_bytes: usize) -> Span {
    Span::new(
        line_start + start_col_bytes as u32,
        line_start + end_col_bytes as u32,
    )
}

/// Text-level rules that also apply to Jungle files.
pub fn lint_text(src: &str, diags: &mut Vec<Diagnostic>) {
    let map = SourceMap::new(src);
    let mut blank_run = 0usize;
    let lines: Vec<&str> = src.split('\n').collect();
    for (idx, line) in lines.iter().enumerate() {
        let line_no = (idx + 1) as u32;
        let line_start = map.line_start(line_no);
        // Strip a trailing '\r' so CRLF files aren't flagged.
        let content = line.strip_suffix('\r').unwrap_or(line);
        let trimmed = content.trim_end_matches([' ', '\t']);

        if trimmed.len() != content.len() {
            diags.push(
                Diagnostic::warning("no-trailing-whitespace", "trailing whitespace")
                    .with_span(span_of(line_start, trimmed.len(), content.len())),
            );
        }

        if content.trim().is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                diags.push(
                    Diagnostic::warning(
                        "no-multiple-empty-lines",
                        "multiple consecutive blank lines",
                    )
                    .with_span(span_of(line_start, 0, content.len())),
                );
            }
        } else {
            blank_run = 0;
        }

        if content.starts_with('\t') {
            diags.push(
                Diagnostic::warning("use-space-indent", "tab indentation; use spaces")
                    .with_span(span_of(line_start, 0, 1)),
            );
        }
    }
    if !src.is_empty() && !src.ends_with('\n') {
        let end = src.len() as u32;
        diags.push(
            Diagnostic::warning("eof-newline", "file does not end with a newline")
                .with_span(Span::new(end, end)),
        );
    }
}
