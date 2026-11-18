//! Diagnostic rendering: terminal output and JSON.

use crate::diagnostic::Diagnostic;
use crate::map::SourceMap;

/// Render diagnostics in the classic `path:line:col: severity: message [code]`
/// format, one per line, followed by notes/help lines indented under each.
///
/// Diagnostics without a span render as `path: severity: message [code]`.
pub fn render_diagnostics(path: &str, src: &str, diags: &[Diagnostic]) -> String {
    let map = SourceMap::new(src);
    let mut out = String::new();
    for d in diags {
        match d.span {
            Some(span) => {
                let (line, col) = map.line_col(span.start, src);
                out.push_str(&format!(
                    "{}:{}:{}: {}: {} [{}]\n",
                    path,
                    line,
                    col,
                    d.severity.as_str(),
                    d.message,
                    d.code
                ));
            }
            None => {
                out.push_str(&format!(
                    "{}: {}: {} [{}]\n",
                    path,
                    d.severity.as_str(),
                    d.message,
                    d.code
                ));
            }
        }
        for note in &d.notes {
            out.push_str(&format!("  = note: {}\n", note));
        }
        if let Some(help) = &d.help {
            out.push_str(&format!("  = help: {}\n", help));
        }
    }
    out
}

/// Serialize diagnostics as a JSON array.
///
/// Shape per entry:
/// `{path, line, column, severity, code, message, span, notes, help, suggestions}`
/// where `line`/`column`/`span` are `null` when absent.
pub fn diagnostics_json(path: &str, src: &str, diags: &[Diagnostic]) -> String {
    let map = SourceMap::new(src);
    let mut out = String::from("[");
    for (i, d) in diags.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let (line, col, span_json) = match d.span {
            Some(s) => {
                let (l, c) = map.line_col(s.start, src);
                (
                    l.to_string(),
                    c.to_string(),
                    format!("{{\"start\":{},\"end\":{}}}", s.start, s.end),
                )
            }
            None => ("null".into(), "null".into(), "null".into()),
        };
        let suggestions: Vec<String> = d
            .suggestions
            .iter()
            .map(|s| {
                format!(
                    "{{\"span\":{{\"start\":{},\"end\":{}}},\"replacement\":{}}}",
                    s.span.start,
                    s.span.end,
                    json_string(&s.replacement)
                )
            })
            .collect();
        out.push_str(&format!(
            "{{\"path\":{},\"line\":{},\"column\":{},\"severity\":{},\"code\":{},\"message\":{},\"span\":{},\"notes\":[{}],\"help\":{},\"suggestions\":[{}]}}",
            json_string(path),
            line,
            col,
            json_string(d.severity.as_str()),
            json_string(&d.code),
            json_string(&d.message),
            span_json,
            d.notes
                .iter()
                .map(|n| json_string(n))
                .collect::<Vec<_>>()
                .join(","),
            d.help
                .as_ref()
                .map_or("null".to_string(), |h| json_string(h)),
            suggestions.join(","),
        ));
    }
    out.push(']');
    out
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::Span;
    use crate::Severity;

    #[test]
    fn terminal_format() {
        let src = "var x = 1;\nvar y = 2;\n";
        let diags = vec![Diagnostic::warning("rule-x", "careful").with_span(Span::new(15, 18))];
        let out = render_diagnostics("t.mc", src, &diags);
        assert_eq!(out, "t.mc:2:5: warning: careful [rule-x]\n");
    }

    #[test]
    fn json_is_valid_shape() {
        let src = "ab\n";
        let diags = vec![
            Diagnostic::error("E1", "bad")
                .with_span(Span::new(0, 2))
                .note("context")
                .help("fix it"),
            Diagnostic::new("E2", Severity::Hint, "fyi"),
        ];
        let json = diagnostics_json("t.mc", src, &diags);
        assert!(json.starts_with('[') && json.ends_with(']'));
        assert!(json.contains("\"line\":1"));
        assert!(json.contains("\"severity\":\"error\""));
        assert!(json.contains("\"code\":\"E1\""));
        assert!(json.contains("\"span\":{\"start\":0,\"end\":2}"));
        assert!(json.contains("\"help\":\"fix it\""));
        assert!(json.contains("\"line\":null"));
    }
}
