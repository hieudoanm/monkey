//! Diagnostics for non-Monkey-C project files: `.mss` string tables (JSON)
//! and resource `*.xml` files. Zero-dependency validators — enough to catch
//! the errors Garmin's own extension reports (malformed JSON/XML).

use monkey_core::{Diagnostic, Severity, Span};

fn diag(code: &str, msg: impl Into<String>, start: usize, end: usize) -> Diagnostic {
    let mut d = Diagnostic::new(code, Severity::Error, msg);
    d.span = Some(Span {
        start: start as u32,
        end: end as u32,
    });
    d
}

/// Minimal JSON well-formedness check for `.mss` files. Reports the first
/// structural error with a byte span (editors show it inline).
pub fn check_mss(src: &str) -> Vec<Diagnostic> {
    let b = src.as_bytes();
    let mut i = 0usize;
    let mut stack: Vec<u8> = Vec::new(); // b'{' or b'['
    let mut out = Vec::new();

    fn ws(b: &[u8], i: &mut usize) {
        while *i < b.len() && b[*i].is_ascii_whitespace() {
            *i += 1;
        }
    }
    fn string_end(b: &[u8], mut i: usize) -> Result<usize, usize> {
        // i is just past the opening quote.
        while i < b.len() {
            match b[i] {
                b'\\' => i += 2,
                b'"' => return Ok(i + 1),
                _ => i += 1,
            }
        }
        Err(i)
    }
    // value parser via explicit recursion on a byte cursor.
    fn value(b: &[u8], i: &mut usize, stack: &mut Vec<u8>, out: &mut Vec<Diagnostic>) -> bool {
        ws(b, i);
        if *i >= b.len() {
            out.push(diag(
                "json-unexpected-eof",
                "unexpected end of file",
                *i,
                *i,
            ));
            return false;
        }
        match b[*i] {
            b'"' => match string_end(b, *i + 1) {
                Ok(e) => {
                    *i = e;
                    true
                }
                Err(e) => {
                    out.push(diag(
                        "json-unterminated-string",
                        "unterminated string",
                        *i,
                        e,
                    ));
                    *i = e;
                    false
                }
            },
            b'{' | b'[' => {
                let open = b[*i];
                stack.push(open);
                *i += 1;
                ws(b, i);
                if *i < b.len() && b[*i] == if open == b'{' { b'}' } else { b']' } {
                    stack.pop();
                    *i += 1;
                    return true;
                }
                loop {
                    ws(b, i);
                    if *i >= b.len() {
                        out.push(diag(
                            "json-unexpected-eof",
                            format!("unclosed '{}' — unexpected end of file", open as char),
                            *i,
                            *i,
                        ));
                        return false;
                    }
                    if open == b'{' {
                        if b[*i] != b'"' {
                            out.push(diag(
                                "json-expected-key",
                                "expected object key string",
                                *i,
                                *i + 1,
                            ));
                            return false;
                        }
                        match string_end(b, *i + 1) {
                            Ok(e) => *i = e,
                            Err(e) => {
                                out.push(diag(
                                    "json-unterminated-string",
                                    "unterminated string",
                                    *i,
                                    e,
                                ));
                                *i = e;
                                return false;
                            }
                        }
                        ws(b, i);
                        if *i >= b.len() || b[*i] != b':' {
                            out.push(diag(
                                "json-expected-colon",
                                "expected ':' after key",
                                *i,
                                (*i + 1).min(b.len()),
                            ));
                            return false;
                        }
                        *i += 1;
                    }
                    if !value(b, i, stack, out) {
                        return false;
                    }
                    // comma or close
                    ws(b, i);
                    if *i >= b.len() {
                        out.push(diag(
                            "json-unexpected-eof",
                            format!("unclosed '{}' — unexpected end of file", open as char),
                            *i,
                            *i,
                        ));
                        return false;
                    }
                    let close = if open == b'{' { b'}' } else { b']' };
                    if b[*i] == close {
                        stack.pop();
                        *i += 1;
                        return true;
                    }
                    if b[*i] != b',' {
                        out.push(diag(
                            "json-expected-comma",
                            "expected ',' or closing bracket",
                            *i,
                            *i + 1,
                        ));
                        return false;
                    }
                    *i += 1;
                }
            }
            b't' => lit(b, i, "true", out),
            b'f' => lit(b, i, "false", out),
            b'n' => lit(b, i, "null", out),
            b'-' | b'0'..=b'9' => number(b, i, out),
            _ => {
                out.push(diag(
                    "json-unexpected",
                    format!("unexpected character '{}'", b[*i] as char),
                    *i,
                    *i + 1,
                ));
                false
            }
        }
    }
    fn lit(b: &[u8], i: &mut usize, s: &str, out: &mut Vec<Diagnostic>) -> bool {
        if b[*i..].starts_with(s.as_bytes()) {
            *i += s.len();
            true
        } else {
            out.push(diag(
                "json-invalid-literal",
                format!("invalid literal, expected {}", s),
                *i,
                *i + s.len().min(b.len() - *i),
            ));
            false
        }
    }
    fn number(b: &[u8], i: &mut usize, out: &mut Vec<Diagnostic>) -> bool {
        let start = *i;
        if b[*i] == b'-' {
            *i += 1;
        }
        while *i < b.len()
            && (b[*i].is_ascii_digit() || matches!(b[*i], b'.' | b'e' | b'E' | b'+' | b'-'))
        {
            *i += 1;
        }
        let s = std::str::from_utf8(&b[start..*i]).unwrap_or("");
        if s.parse::<f64>().is_err() {
            out.push(diag(
                "json-invalid-number",
                format!("invalid number '{}'", s),
                start,
                *i,
            ));
            return false;
        }
        true
    }

    if !value(b, &mut i, &mut stack, &mut out) {
        return out;
    }
    ws(b, &mut i);
    if i < b.len() {
        out.push(diag(
            "json-trailing",
            "unexpected content after top-level value",
            i,
            i + 1,
        ));
    }
    out
}

/// XML well-formedness check for resource files (`resources/**/*.xml`,
/// `manifest.xml` is covered by `project check`).
pub fn check_xml(src: &str) -> Vec<Diagnostic> {
    let b = src.as_bytes();
    let mut i = 0usize;
    let mut stack: Vec<(String, usize)> = Vec::new(); // (qname, open pos)
    let mut out = Vec::new();
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let lt = i;
        if src[i..].starts_with("<!--") {
            match src[i + 4..].find("-->") {
                Some(e) => i += 4 + e + 3,
                None => {
                    out.push(diag(
                        "xml-unclosed-comment",
                        "unclosed comment",
                        lt,
                        b.len(),
                    ));
                    break;
                }
            }
            continue;
        }
        if src[i..].starts_with("<![CDATA[") {
            match src[i + 9..].find("]]>") {
                Some(e) => i += 9 + e + 3,
                None => {
                    out.push(diag(
                        "xml-unclosed-cdata",
                        "unclosed CDATA section",
                        lt,
                        b.len(),
                    ));
                    break;
                }
            }
            continue;
        }
        if src[i..].starts_with("<?") {
            match src[i + 2..].find("?>") {
                Some(e) => i += 2 + e + 2,
                None => {
                    out.push(diag(
                        "xml-unclosed-pi",
                        "unclosed processing instruction",
                        lt,
                        b.len(),
                    ));
                    break;
                }
            }
            continue;
        }
        if src[i..].starts_with("<!") {
            // DOCTYPE etc. — skip to '>'.
            match src[i..].find('>') {
                Some(e) => i += e + 1,
                None => {
                    out.push(diag(
                        "xml-unclosed-decl",
                        "unclosed declaration",
                        lt,
                        b.len(),
                    ));
                    break;
                }
            }
            continue;
        }
        i += 1;
        if i >= b.len() {
            break;
        }
        if b[i] == b'/' {
            // Close tag: read name, expect '>'.
            let ns = i + 1;
            let mut j = ns;
            while j < b.len()
                && (b[j].is_ascii_alphanumeric() || matches!(b[j], b'_' | b'-' | b'.' | b':'))
            {
                j += 1;
            }
            let name = &src[ns..j];
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j >= b.len() || b[j] != b'>' {
                out.push(diag(
                    "xml-malformed-close",
                    "malformed closing tag",
                    lt,
                    j.min(b.len()),
                ));
                i = j;
                continue;
            }
            match stack.pop() {
                Some((open, _)) if open == name => {}
                Some((open, _)) => {
                    // Mismatched close: if the name matches an ancestor,
                    // close through it; otherwise report and keep scanning.
                    out.push(diag(
                        "xml-mismatched-tag",
                        format!("expected closing tag for <{}>, found </{}>", open, name),
                        lt,
                        j + 1,
                    ));
                    if let Some(idx) = stack.iter().rposition(|(n, _)| *n == name) {
                        // Pop the mismatched opens above it (already popped
                        // `open`); elements between are implicitly closed.
                        stack.truncate(idx);
                    } else {
                        stack.push((open, lt));
                    }
                    if out.len() > 20 {
                        break;
                    }
                }
                None => {
                    out.push(diag(
                        "xml-unexpected-close",
                        format!("closing tag </{}> without open", name),
                        lt,
                        j + 1,
                    ));
                }
            }
            i = j + 1;
            continue;
        }
        // Open tag: read name.
        let ns = i;
        let mut j = ns;
        while j < b.len()
            && (b[j].is_ascii_alphanumeric() || matches!(b[j], b'_' | b'-' | b'.' | b':'))
        {
            j += 1;
        }
        if j == ns {
            out.push(diag("xml-malformed-tag", "malformed tag", lt, lt + 1));
            i += 1;
            continue;
        }
        let name = src[ns..j].to_string();
        // Scan to '>' honoring quotes.
        let mut k = j;
        let mut quote = 0u8;
        while k < b.len() {
            let c = b[k];
            if quote != 0 {
                if c == quote {
                    quote = 0;
                }
            } else if c == b'"' || c == b'\'' {
                quote = c;
            } else if c == b'>' {
                break;
            }
            k += 1;
        }
        if k >= b.len() {
            out.push(diag(
                "xml-unclosed-tag",
                format!("unclosed <{}>", name),
                lt,
                b.len(),
            ));
            break;
        }
        let self_closed = src[j..k].trim_end().ends_with('/');
        if !self_closed {
            stack.push((name, lt));
        }
        i = k + 1;
    }
    for (name, pos) in stack {
        out.push(diag(
            "xml-unclosed-element",
            format!("element <{}> was never closed", name),
            pos,
            pos + 1,
        ));
    }
    out
}
