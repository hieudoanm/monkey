//! Minimal XML parser for Connect IQ manifests.
//!
//! Tolerant, non-validating: handles elements, attributes (single/double
//! quotes), text, comments, CDATA, processing instructions, and
//! self-closing tags. Namespace prefixes are kept in names (`iq:application`);
//! use [`local_name`] to match on the local part.

/// An XML element.
#[derive(Clone, Debug)]
pub struct Element {
    /// Qualified tag name, e.g. `iq:application`.
    pub name: String,
    /// Attributes in document order.
    pub attributes: Vec<(String, String)>,
    /// Direct text children concatenated.
    pub text: String,
    /// Child elements.
    pub children: Vec<Element>,
}

impl Element {
    /// Local name without namespace prefix (`iq:application` → `application`).
    pub fn local_name(&self) -> &str {
        self.name.rsplit(':').next().unwrap_or(&self.name)
    }

    /// First attribute value by qualified or local name.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| k == name || k.rsplit(':').next() == Some(name))
            .map(|(_, v)| v.as_str())
    }

    /// First child element matching qualified or local name.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children
            .iter()
            .find(|c| c.name == name || c.local_name() == name.rsplit(':').next().unwrap_or(name))
    }

    /// All descendant-or-self elements matching a local name, depth-first.
    pub fn descendants_named<'a>(&'a self, name: &'a str, out: &mut Vec<&'a Element>) {
        if self.local_name() == name || self.name == name {
            out.push(self);
        }
        for c in &self.children {
            c.descendants_named(name, out);
        }
    }
}

#[derive(Debug)]
struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

/// Parse XML text into a forest of elements (usually one root).
/// Returns elements successfully parsed before the first hard error;
/// malformed tails are skipped rather than failing the whole document.
pub fn parse(src: &str) -> Vec<Element> {
    let mut p = Parser {
        src: src.as_bytes(),
        pos: 0,
    };
    let mut roots = Vec::new();
    while let Some(elem) = p.element() {
        roots.push(elem);
    }
    roots
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

impl<'a> Parser<'a> {
    fn skip_misc(&mut self) {
        loop {
            self.skip_ws();
            if self.src[self.pos..].starts_with(b"<!--") {
                if let Some(end) = find(&self.src[self.pos + 4..], b"-->") {
                    self.pos += 4 + end + 3;
                    continue;
                }
                self.pos = self.src.len();
                return;
            }
            if self.src[self.pos..].starts_with(b"<![CDATA[") {
                if let Some(end) = find(&self.src[self.pos + 9..], b"]]>") {
                    self.pos += 9 + end + 3;
                    continue;
                }
                self.pos = self.src.len();
                return;
            }
            if self.src[self.pos..].starts_with(b"<?") {
                if let Some(end) = find(&self.src[self.pos + 2..], b"?>") {
                    self.pos += 2 + end + 2;
                    continue;
                }
                self.pos = self.src.len();
                return;
            }
            if self.src[self.pos..].starts_with(b"<!") {
                // DOCTYPE or other declaration: skip to '>'.
                if let Some(end) = find(&self.src[self.pos..], b">") {
                    self.pos += end + 1;
                    continue;
                }
                self.pos = self.src.len();
                return;
            }
            return;
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.src.len() && self.src[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn element(&mut self) -> Option<Element> {
        self.skip_misc();
        if self.pos >= self.src.len() || self.src[self.pos] != b'<' {
            return None;
        }
        self.pos += 1; // consume '<'
        if self.pos >= self.src.len() || self.src[self.pos] == b'/' {
            return None; // stray closing tag
        }
        let name = self.name()?;
        let mut attributes = Vec::new();
        loop {
            self.skip_ws();
            if self.pos >= self.src.len() {
                return None;
            }
            match self.src[self.pos] {
                b'>' => {
                    self.pos += 1;
                    break;
                }
                b'/' if self.src.get(self.pos + 1) == Some(&b'>') => {
                    self.pos += 2;
                    return Some(Element {
                        name,
                        attributes,
                        text: String::new(),
                        children: Vec::new(),
                    });
                }
                _ => {
                    let key = self.name()?;
                    self.skip_ws();
                    if self.pos < self.src.len() && self.src[self.pos] == b'=' {
                        self.pos += 1;
                        self.skip_ws();
                        if let Some(value) = self.attr_value() {
                            attributes.push((key, value));
                            continue;
                        }
                    }
                    // Attribute without value (or malformed): keep key with empty value.
                    attributes.push((key, String::new()));
                }
            }
        }
        let mut text = String::new();
        let mut children = Vec::new();
        loop {
            if self.pos >= self.src.len() {
                return None; // unterminated element
            }
            if self.src[self.pos..].starts_with(b"</") {
                self.pos += 2;
                let _ = self.name(); // closing tag name
                self.skip_ws();
                if self.pos < self.src.len() && self.src[self.pos] == b'>' {
                    self.pos += 1;
                }
                break;
            }
            if self.src[self.pos] == b'<' {
                match self.element() {
                    Some(child) => children.push(child),
                    None => {
                        // Malformed child: skip one char and retry to avoid loops.
                        self.pos += 1;
                    }
                }
            } else {
                let start = self.pos;
                while self.pos < self.src.len() && self.src[self.pos] != b'<' {
                    self.pos += 1;
                }
                if let Ok(s) = std::str::from_utf8(&self.src[start..self.pos]) {
                    text.push_str(s);
                }
            }
        }
        Some(Element {
            name,
            attributes,
            text: unescape(text.trim()),
            children,
        })
    }

    /// Read a tag/attribute name: letters, digits, `_`, `-`, `.`, `:`.
    fn name(&mut self) -> Option<String> {
        let start = self.pos;
        while self.pos < self.src.len() {
            let b = self.src[self.pos];
            if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':') {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            return None;
        }
        std::str::from_utf8(&self.src[start..self.pos])
            .ok()
            .map(str::to_string)
    }

    fn attr_value(&mut self) -> Option<String> {
        if self.pos >= self.src.len() {
            return None;
        }
        let quote = self.src[self.pos];
        if quote != b'"' && quote != b'\'' {
            return None;
        }
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.src.len() && self.src[self.pos] != quote {
            self.pos += 1;
        }
        let raw = std::str::from_utf8(&self.src[start..self.pos]).ok()?;
        if self.pos < self.src.len() {
            self.pos += 1; // closing quote
        }
        Some(unescape(raw))
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest_like_doc() {
        let src = r#"<?xml version="1.0"?>
<iq:manifest xmlns:iq="http://x">
  <iq:application id="abc" type="watchface" name="App" entry="App">
    <iq:products><iq:product id="fr55"/><iq:product id="venu3"/></iq:products>
    <!-- comment -->
    <iq:languages><iq:language>eng</iq:language></iq:languages>
  </iq:application>
</iq:manifest>"#;
        let roots = parse(src);
        assert_eq!(roots.len(), 1);
        let app = roots[0].child("application").unwrap();
        assert_eq!(app.attr("id"), Some("abc"));
        assert_eq!(app.attr("type"), Some("watchface"));
        let products: Vec<&Element> = {
            let mut v = Vec::new();
            app.descendants_named("product", &mut v);
            v
        };
        assert_eq!(products.len(), 2);
        assert_eq!(products[0].attr("id"), Some("fr55"));
    }

    #[test]
    fn tolerates_malformed_tail() {
        let roots = parse("<a><b>x</b></a><broken");
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].children.len(), 1);
    }
}
