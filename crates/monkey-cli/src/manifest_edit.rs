//! Surgical `manifest.xml` editing for `monkey project` subcommands.
//!
//! Works on the raw XML text (the project XML parser is read-only and does
//! not retain source positions). Element lookup is namespace-prefix
//! agnostic: `<iq:products>` and `<products>` both match `products`.

use std::fs;
use std::path::Path;

/// Span of an XML element in source text.
struct El {
    /// Byte offset of the `<` opening the element.
    start: usize,
    /// Byte offset one past the matching `>` of the open tag.
    open_end: usize,
    /// Byte offset of the `<` of the close tag (or of `/>` when self-closed).
    close_start: usize,
    /// Byte offset one past the element (close tag `>` or `/>`).
    end: usize,
    /// Qualified tag name as written (`iq:products`).
    qname: String,
    self_closed: bool,
}

/// Find `<prefix:local ...>` / `<local ...>` element `local` in `src`,
/// searching from `from`. Returns the element span.
fn find_element(src: &str, local: &str, from: usize) -> Option<El> {
    let bytes = src.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] != b'<' || i + 1 >= bytes.len() {
            i += 1;
            continue;
        }
        // Skip comments, PIs, doctype, close tags.
        let rest = &src[i + 1..];
        if rest.starts_with('!') || rest.starts_with('?') || rest.starts_with('/') {
            i += 1;
            continue;
        }
        // Read the qualified name.
        let name_end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-' || c == ':' || c == '.'))
            .map(|o| i + 1 + o)
            .unwrap_or(src.len());
        let qname = src[i + 1..name_end].to_string();
        let lname = qname.rsplit(':').next().unwrap_or(&qname);
        if lname == local {
            // Find the end of the open tag, honoring quoted `>` inside attrs.
            let mut j = name_end;
            let mut quote: Option<u8> = None;
            while j < bytes.len() {
                let b = bytes[j];
                match quote {
                    Some(q) => {
                        if b == q {
                            quote = None;
                        }
                    }
                    None => match b {
                        b'"' | b'\'' => quote = Some(b),
                        b'>' => break,
                        _ => {}
                    },
                }
                j += 1;
            }
            if j >= bytes.len() {
                return None;
            }
            let open_end = j + 1;
            let self_closed = src[..open_end].trim_end().ends_with("/>");
            if self_closed {
                return Some(El {
                    start: i,
                    open_end,
                    close_start: open_end - 2,
                    end: open_end,
                    qname,
                    self_closed: true,
                });
            }
            // Find the matching close tag — sections we edit never nest the
            // same tag, so the first `</qname>` wins.
            let close_tag = format!("</{}", qname);
            if let Some(rel) = src[open_end..].find(&close_tag) {
                let cs = open_end + rel;
                let gt = src[cs..].find('>').map(|o| cs + o + 1).unwrap_or(src.len());
                return Some(El {
                    start: i,
                    open_end,
                    close_start: cs,
                    end: gt,
                    qname,
                    self_closed: false,
                });
            }
            return None;
        }
        i = name_end.max(i + 1);
    }
    None
}

/// Whitespace indent of the line containing byte offset `pos`.
fn line_indent(src: &str, pos: usize) -> String {
    let line_start = src[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
    src[line_start..pos]
        .chars()
        .take_while(|c| c.is_whitespace())
        .collect()
}

/// Inner content span (between open and close tags) of element `local`,
/// searching the whole document. `None` for self-closed/missing elements.
fn inner_span(src: &str, local: &str) -> Option<(usize, usize)> {
    let el = find_element(src, local, 0)?;
    if el.self_closed {
        return None;
    }
    Some((el.open_end, el.close_start))
}

/// Replace the children of element `local` with `items` (each a complete
/// element string without indentation), creating the section inside
/// `parent_local` when missing. Indentation follows the surrounding file's
/// two-space convention derived from the element's own line.
fn set_items(
    src: &str,
    local: &str,
    parent_local: &str,
    items: &[String],
) -> Result<String, String> {
    let item_lines = |indent: &str| -> String {
        if items.is_empty() {
            String::new()
        } else {
            let child = format!("{}  ", indent);
            let body: Vec<String> = items.iter().map(|i| format!("{}{}", child, i)).collect();
            format!("\n{}\n{}", body.join("\n"), indent)
        }
    };

    if let Some(el) = find_element(src, local, 0) {
        if el.self_closed {
            // `<x/>` → `<x>\n  items\n</x>` keeping the element's indent.
            let indent = line_indent(src, el.start);
            let inner = item_lines(&indent);
            let mut out = String::with_capacity(src.len() + inner.len() + 8);
            out.push_str(&src[..el.start]);
            out.push_str(&src[el.start..el.close_start]); // `<qname`
            out.push('>');
            out.push_str(&inner);
            out.push_str("</");
            out.push_str(&el.qname);
            out.push('>');
            out.push_str(&src[el.end..]);
            return Ok(out);
        }
        // Non-self-closed: replace content between open_end and close_start.
        let indent = line_indent(src, el.start);
        let inner = item_lines(&indent);
        let mut out = String::with_capacity(src.len() + inner.len());
        out.push_str(&src[..el.open_end]);
        out.push_str(&inner);
        out.push_str(&src[el.close_start..]);
        return Ok(out);
    }

    // Section missing — insert before the parent's close tag.
    let parent = find_element(src, parent_local, 0)
        .ok_or_else(|| format!("manifest has no <{}> element", parent_local))?;
    if parent.self_closed {
        return Err(format!(
            "<{}> is self-closed; add <{}> manually",
            parent_local, local
        ));
    }
    let close_line_start = src[..parent.close_start]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);
    let parent_indent = line_indent(src, parent.close_start);
    let sec_indent = format!("{}  ", parent_indent);
    // Section open, items, section close, then newline before the parent's
    // close-tag line. Match the parent's namespace prefix (`iq:` etc.).
    let pfx = if parent.qname.contains(':') {
        format!("{}:", parent.qname.split(':').next().unwrap_or(""))
    } else {
        String::new()
    };
    let qlocal = format!("{}{}", pfx, local);
    let body = if items.is_empty() {
        format!("{}{}/>", sec_indent, qlocal)
    } else {
        let child = format!("{}  ", sec_indent);
        let lines: Vec<String> = items.iter().map(|i| format!("{}{}", child, i)).collect();
        format!(
            "{}<{}>\n{}\n{}</{}>",
            sec_indent,
            qlocal,
            lines.join("\n"),
            sec_indent,
            qlocal
        )
    };
    let mut out = String::with_capacity(src.len() + body.len() + 8);
    out.push_str(&src[..close_line_start]);
    out.push_str(&body);
    out.push('\n');
    out.push_str(&src[close_line_start..]);
    Ok(out)
}

/// List child item ids/text inside element `local` of `parent_local`.
/// `attr` selects an attribute (`id`, `name`) or `None` for text content.
fn list_items(src: &str, parent_local: &str, child_local: &str, attr: Option<&str>) -> Vec<String> {
    let Some((s, e)) = inner_span(src, parent_local) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut pos = s;
    while pos < e {
        let Some(el) = find_element(&src[pos..e], child_local, 0) else {
            break;
        };
        let abs_start = pos + el.start;
        let abs_end = pos + el.end;
        let tag = &src[abs_start..pos + el.open_end];
        let val = match attr {
            Some(a) => attr_value(tag, a).unwrap_or_default(),
            None => {
                if el.self_closed {
                    String::new()
                } else {
                    src[pos + el.open_end..pos + el.close_start]
                        .trim()
                        .to_string()
                }
            }
        };
        if !val.is_empty() {
            out.push(val);
        }
        pos = abs_end;
    }
    out
}

/// Extract `attr="value"` or `attr='value'` from a tag string.
fn attr_value(tag: &str, attr: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let pat = format!("{}={}", attr, quote);
        if let Some(i) = tag.find(&pat) {
            let rest = &tag[i + pat.len()..];
            if let Some(end) = rest.find(quote) {
                return Some(rest[..end].to_string());
            }
        }
    }
    // Unquoted (rare but legal XML).
    let pat = format!("{}=", attr);
    if let Some(i) = tag.find(&pat) {
        let rest = tag[i + pat.len()..].trim_start();
        let end = rest
            .find(|c: char| c == '>' || c == '/' || c.is_whitespace())
            .unwrap_or(rest.len());
        return Some(rest[..end].to_string());
    }
    None
}

/// Add or replace an attribute on the first `<local ...>` open tag.
fn set_attr(src: &str, local: &str, attr: &str, value: &str) -> Result<String, String> {
    let el = find_element(src, local, 0).ok_or_else(|| format!("no <{}> element", local))?;
    let tag = &src[el.start..el.open_end];
    let new_tag = if attr_value(tag, attr).is_some() {
        // Replace the existing attribute value in place.
        let mut out = String::new();
        let bytes = tag.as_bytes();
        let mut i = 0;
        let mut done = false;
        while i < bytes.len() {
            if !done
                && tag[i..].starts_with(attr)
                && (i == 0 || bytes[i - 1] == b' ')
                && tag[i + attr.len()..].starts_with('=')
            {
                let eq = i + attr.len();
                let vstart = tag[eq + 1..]
                    .find(|c: char| !c.is_whitespace())
                    .map(|o| eq + 1 + o)
                    .unwrap_or(eq + 1);
                let quote = bytes[vstart];
                if quote == b'"' || quote == b'\'' {
                    let vend = tag[vstart + 1..]
                        .find(quote as char)
                        .map(|o| vstart + 1 + o + 1)
                        .unwrap_or(tag.len());
                    // Prefix through `attr` is already in `out` byte-by-byte;
                    // but we haven't emitted `attr` itself yet — emit it now.
                    // Roll back: `out` currently ends right before `attr`.
                    out.push_str(attr);
                    out.push_str("=\"");
                    out.push_str(value);
                    out.push('"');
                    i = vend;
                    done = true;
                    continue;
                }
            }
            // Copy the byte (tags are ASCII-safe for our purposes).
            out.push(bytes[i] as char);
            i += 1;
        }
        if !done {
            return Err(format!("attribute {} not replaceable", attr));
        }
        out
    } else {
        // Insert before the `>` of the open tag.
        let gt = tag.rfind('>').unwrap();
        let mut t = tag[..gt].trim_end().to_string();
        t.push(' ');
        t.push_str(attr);
        t.push_str("=\"");
        t.push_str(value);
        t.push('"');
        t.push_str(&tag[gt..]);
        t
    };
    let mut out = String::with_capacity(src.len() + new_tag.len());
    out.push_str(&src[..el.start]);
    out.push_str(&new_tag);
    out.push_str(&src[el.open_end..]);
    Ok(out)
}

/// Regenerate a UUIDv4 (8-4-4-4-12 hex) without external crates.
fn new_uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    let mut x = nanos ^ (pid << 64) ^ 0x9e3779b97f4a7c15u128;
    let mut bytes = [0u8; 16];
    for b in bytes.iter_mut() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *b = (x & 0xff) as u8;
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // variant
    let h: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

/// Format one child element for a section.
fn item_xml(child: &str, attr: Option<&str>, value: &str) -> String {
    match (child, attr) {
        ("language", None) => format!("<iq:language>{}</iq:language>", value),
        ("annotation", None) => format!("<iq:annotation>{}</iq:annotation>", value),
        (_, Some(a)) => format!("<iq:{} {}=\"{}\"/>", child, a, value),
        _ => format!("<iq:{}>{}</iq:{}>", child, value, child),
    }
}

/// Apply a `list|set|add|remove` operation to a manifest section.
/// `section` is the parent element (`products`, `permissions`, `languages`,
/// `barrels`, `annotations`); `child`/`attr` describe its items.
pub fn edit_section(
    manifest_path: &Path,
    section: &str,
    child: &str,
    attr: Option<&str>,
    parent: &str,
    op: &str,
    values: &[String],
) -> Result<String, String> {
    let src = fs::read_to_string(manifest_path)
        .map_err(|e| format!("cannot read {}: {}", manifest_path.display(), e))?;
    if op == "list" {
        let items = list_items(&src, section, child, attr);
        return Ok(if items.is_empty() {
            "(none)".to_string()
        } else {
            items.join("\n")
        });
    }
    let current = list_items(&src, section, child, attr);
    let mut next: Vec<String> = match op {
        "set" => values.to_vec(),
        "add" => {
            let mut v = current;
            for x in values {
                if !v.contains(x) {
                    v.push(x.clone());
                }
            }
            v
        }
        "remove" => current
            .into_iter()
            .filter(|x| !values.contains(x))
            .collect(),
        _ => return Err(format!("unknown op '{}' (list|set|add|remove)", op)),
    };
    next.sort();
    next.dedup();
    let items: Vec<String> = next.iter().map(|v| item_xml(child, attr, v)).collect();
    // Sections live inside `<iq:application>` for apps and `<iq:barrel>`
    // for Monkey Barrels — resolve whichever the manifest declares.
    let container = if find_element(&src, parent, 0).is_some() {
        parent.to_string()
    } else if parent == "application" && find_element(&src, "barrel", 0).is_some() {
        "barrel".to_string()
    } else if parent == "barrel" && find_element(&src, "application", 0).is_some() {
        "application".to_string()
    } else {
        parent.to_string()
    };
    if section == "annotations" && container == "application" {
        return Err(
            "<iq:annotations> is only valid inside <iq:barrel> (Monkey Barrel manifests)"
                .to_string(),
        );
    }
    let out = set_items(&src, section, &container, &items)?;
    fs::write(manifest_path, out)
        .map_err(|e| format!("cannot write {}: {}", manifest_path.display(), e))?;
    Ok(format!("{}: {}", section, next.join(", ")))
}

/// Set attributes on `<iq:application>`.
pub fn edit_application(manifest_path: &Path, sets: &[(String, String)]) -> Result<String, String> {
    let mut src = fs::read_to_string(manifest_path)
        .map_err(|e| format!("cannot read {}: {}", manifest_path.display(), e))?;
    for (k, v) in sets {
        src = set_attr(&src, "application", k, v)?;
    }
    fs::write(manifest_path, &src)
        .map_err(|e| format!("cannot write {}: {}", manifest_path.display(), e))?;
    Ok(sets
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join(", "))
}

/// Regenerate the application `id` UUID.
pub fn regenerate_uuid(manifest_path: &Path) -> Result<String, String> {
    let uuid = new_uuid();
    edit_application(manifest_path, &[("id".to_string(), uuid.clone())])?;
    Ok(uuid)
}

/// Current `<iq:application>` attributes.
pub fn application_attrs(manifest_path: &Path) -> Result<Vec<(String, String)>, String> {
    let src = fs::read_to_string(manifest_path)
        .map_err(|e| format!("cannot read {}: {}", manifest_path.display(), e))?;
    let el = find_element(&src, "application", 0).ok_or("no <application> element")?;
    let tag = &src[el.start..el.open_end];
    let mut out = Vec::new();
    for attr in ["id", "type", "name", "entry", "launcherIcon", "minApiLevel"] {
        if let Some(v) = attr_value(tag, attr) {
            out.push((attr.to_string(), v));
        }
    }
    Ok(out)
}
