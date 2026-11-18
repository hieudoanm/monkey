//! Inline diagnostic suppression via `monkey-ignore` comments.
//!
//! Supported in `//` (Monkey C) and `#` (Jungle) comments:
//!
//! ```text
//! var x = 1; // monkey-ignore unused-variable
//! // monkey-ignore
//! var y = ;   // suppressed: whole-line comment covers the next line
//! ```
//!
//! `monkey-ignore` alone suppresses every rule; `monkey-ignore <code>[,<code>…]`
//! suppresses only the listed codes (space or comma separated). A trailing
//! comment suppresses diagnostics on its own line; a comment that is alone on
//! its line also suppresses the following line.

use monkey_core::{Diagnostic, SourceMap};

struct Suppression {
    /// 0-based line of the comment.
    line: usize,
    /// `None` = suppress all rules; `Some` = only these codes.
    codes: Option<Vec<String>>,
    /// Whether the comment is the only content on its line.
    alone: bool,
}

/// Scan source lines for `monkey-ignore` comments.
fn collect(src: &str) -> Vec<Suppression> {
    let mut out = Vec::new();
    for (idx, line) in src.lines().enumerate() {
        // Earliest comment marker on the line.
        let slash = line.find("//");
        let hash = line.find('#');
        let start = match (slash, hash) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        let Some(start) = start else { continue };
        let comment = &line[start..];
        let Some(pos) = comment.find("monkey-ignore") else {
            continue;
        };
        let rest = comment[pos + "monkey-ignore".len()..].trim();
        let codes: Vec<String> = rest
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .collect();
        out.push(Suppression {
            line: idx,
            codes: if codes.is_empty() { None } else { Some(codes) },
            alone: line[..start].trim().is_empty(),
        });
    }
    out
}

/// Remove suppressed diagnostics in place. `since` limits filtering to
/// entries appended after that index (used by `lint_text` which appends).
pub(crate) fn filter(src: &str, diags: &mut Vec<Diagnostic>, since: usize) {
    let suppressions = collect(src);
    if suppressions.is_empty() {
        return;
    }
    let map = SourceMap::new(src);
    let mut kept = Vec::with_capacity(diags.len());
    for (i, d) in diags.drain(..).enumerate() {
        if i >= since && is_suppressed(&d, &suppressions, &map, src) {
            continue;
        }
        kept.push(d);
    }
    *diags = kept;
}

fn is_suppressed(d: &Diagnostic, suppressions: &[Suppression], map: &SourceMap, src: &str) -> bool {
    let Some(span) = d.span else { return false };
    let (line1, _) = map.line_col(span.start, src);
    let line = (line1 - 1) as usize; // 0-based
    suppressions.iter().any(|s| {
        let on_line = s.line == line || (s.alone && s.line + 1 == line);
        if !on_line {
            return false;
        }
        match &s.codes {
            None => true,
            Some(codes) => codes.iter().any(|c| c == &d.code || c == "all"),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use monkey_core::Severity;

    fn diag_at(code: &str, src: &str, needle: &str) -> Diagnostic {
        let start = src.find(needle).unwrap() as u32;
        Diagnostic {
            code: code.to_string(),
            severity: Severity::Warning,
            message: code.to_string(),
            span: Some(monkey_core::Span::new(start, start + needle.len() as u32)),
            notes: vec![],
            help: None,
            suggestions: vec![],
        }
    }

    #[test]
    fn trailing_comment_suppresses_same_line() {
        let src = "var x = 1; // monkey-ignore unused-variable\nvar y = 2; // monkey-ignore\n";
        let mut diags = vec![
            diag_at("unused-variable", src, "x = 1"),
            diag_at("other-rule", src, "y = 2"),
        ];
        filter(src, &mut diags, 0);
        assert!(diags.is_empty());
    }

    #[test]
    fn whole_line_comment_suppresses_next_line() {
        let src = "// monkey-ignore\nvar x = 1;\nvar y = 2;\n";
        let mut diags = vec![
            diag_at("unused-variable", src, "x = 1"),
            diag_at("unused-variable", src, "y = 2"),
        ];
        filter(src, &mut diags, 0);
        assert_eq!(diags.len(), 1);
        assert_eq!(
            diags[0].span.unwrap().start as usize,
            src.find("y = 2").unwrap()
        );
    }

    #[test]
    fn code_list_only_suppresses_listed() {
        let src = "var x = 1; // monkey-ignore unused-variable\n";
        let mut diags = vec![
            diag_at("unused-variable", src, "x = 1"),
            diag_at("other-rule", src, "x = 1"),
        ];
        filter(src, &mut diags, 0);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "other-rule");
    }

    #[test]
    fn since_index_preserves_earlier_diags() {
        let src = "var x = 1; // monkey-ignore\n";
        let mut diags = vec![
            diag_at("pre-existing", src, "x = 1"), // index 0 — before `since`
            diag_at("unused-variable", src, "x = 1"), // index 1 — filtered
        ];
        filter(src, &mut diags, 1);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "pre-existing");
    }

    #[test]
    fn hash_comments_work_for_jungle() {
        let src = "key = value # monkey-ignore no-trailing-whitespace\n";
        let mut diags = vec![diag_at("no-trailing-whitespace", src, "value")];
        filter(src, &mut diags, 0);
        assert!(diags.is_empty());
    }
}
