//! Toybox API member database for completion.
//!
//! Reads `bin/api.debug.xml` from the detected Connect IQ SDK — it maps
//! `parent="Toybox_Module_Class"` to member `symbol`s. Gives `Toybox.X.`
//! member completion without shipping a generated table.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

/// Dotted-owner → member names, e.g. `"Toybox.Math"` → `["cos", "sin", ...]`.
/// Plus `Parent.member` → return type name for `var x = Math.cos()` inference.
pub struct ToyboxDb {
    members: BTreeMap<String, BTreeSet<String>>,
    /// `"Math.cos"`-style key (functionEntry parent is unqualified) → type.
    returns: BTreeMap<String, String>,
    /// Bare member name → type (first wins) for unqualified calls.
    return_names: BTreeMap<String, String>,
    /// `"Toybox.Math.cos"`-style key → markdown doc text from CDATA.
    docs: BTreeMap<String, String>,
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let pat = format!("{}=\"", name);
    let i = tag.find(&pat)? + pat.len();
    let rest = &tag[i..];
    let j = rest.find('"')?;
    Some(rest[..j].to_string())
}

/// `Toybox_Math_FirFilter` → `Toybox.Math.FirFilter`; non-Toybox parents
/// are kept as-is with `_` → `.`.
fn owner_of(parent: &str) -> String {
    parent.replace('_', ".")
}

/// Extract a return type name from a `<functionEntry>` chunk's
/// `<div class="returns">Returns [Toybox::Lang::Float, ...]</div>` doc.
/// Multiple types collapse to the first mapped canonical name.
fn return_type_of(chunk: &str) -> Option<String> {
    let i = chunk.find("class=\"returns\"")?;
    let rest = &chunk[i..];
    let j = rest.find("Returns")? + "Returns".len();
    let rest = rest[j..].trim_start();
    let inner = rest.strip_prefix('[')?;
    let end = inner.find(']')?;
    let first = inner[..end].split(',').next()?.trim();
    // `Toybox::Lang::Float` / `Toybox.Lang.Float` → last segment.
    let seg = first
        .rsplit("::")
        .next()
        .unwrap_or(first)
        .rsplit('.')
        .next()
        .unwrap_or(first);
    Some(match seg {
        "Number" | "Float" | "Long" | "Double" => "Number".to_string(),
        "String" | "Boolean" | "Array" | "Dictionary" | "Method" => seg.to_string(),
        other => other.to_string(),
    })
}

/// Extract the `<documentation><![CDATA[...]]></documentation>` body of a
/// `<functionEntry>` chunk and flatten its HTML to readable text.
fn doc_text_of(chunk: &str) -> Option<String> {
    let i = chunk.find("<![CDATA[")? + "<![CDATA[".len();
    let j = chunk[i..].find("]]>")? + i;
    let html = &chunk[i..j];
    let text = html_to_text(html);
    (!text.is_empty()).then_some(text)
}

/// Flatten the doc HTML (`<div class="param">`, `<br/>`, entities) into
/// plain lines suitable for markdown hover.
fn html_to_text(html: &str) -> String {
    // `<br>` variants become newlines; every other tag is dropped.
    let mut s = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find('<') {
        s.push_str(&rest[..i]);
        rest = &rest[i..];
        if rest.starts_with("<br") {
            // Skip to the closing '>' of the <br ...> tag.
            if let Some(j) = rest.find('>') {
                s.push('\n');
                rest = &rest[j + 1..];
                continue;
            }
        }
        if let Some(j) = rest.find('>') {
            rest = &rest[j + 1..];
        } else {
            break;
        }
    }
    s.push_str(rest);
    let s = s
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    let lines: Vec<&str> = s.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let mut out = lines.join("\n");
    const MAX: usize = 4000;
    if out.len() > MAX {
        out.truncate(MAX);
    }
    out
}

impl ToyboxDb {
    /// Load from the detected SDK's `bin/api.debug.xml`.
    pub fn load() -> Option<Self> {
        let sdk = monkey_sdk::find_sdk(None)?;
        let path = sdk.bin().join("api.debug.xml");
        let text = fs::read_to_string(path).ok()?;
        Some(Self::parse(&text))
    }

    /// Parse `<entry parent="..." symbol="..."/>` records plus
    /// `<functionEntry>` return types from their CDATA docs.
    pub fn parse(xml: &str) -> Self {
        let mut members: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for chunk in xml.split("<entry ").skip(1) {
            let tag = chunk.split('>').next().unwrap_or("");
            let (Some(parent), Some(symbol)) = (attr(tag, "parent"), attr(tag, "symbol")) else {
                continue;
            };
            // XML-escaped forms: `&lt;init&gt;`.
            if symbol.contains("&lt;") {
                continue;
            }
            members.entry(owner_of(&parent)).or_default().insert(symbol);
        }
        // `<functionEntry>` carries the fuller API but with *unqualified*
        // parents (`parent="String"`). Resolve each to the qualified owner
        // (`Toybox.Lang.String`) using the `<entry>`-built map, falling
        // back to the raw parent name.
        // (name, parent, return type, doc text)
        type FnRow = (String, String, Option<String>, Option<String>);
        let mut fns: Vec<FnRow> = Vec::new();
        for chunk in xml.split("<functionEntry ").skip(1) {
            let tag_end = chunk.find('>').unwrap_or(0);
            let tag = &chunk[..tag_end];
            let (Some(name), Some(parent)) = (attr(tag, "name"), attr(tag, "parent")) else {
                continue;
            };
            if name.contains("&lt;") {
                continue;
            }
            fns.push((name, parent, return_type_of(chunk), doc_text_of(chunk)));
        }
        let resolve = |parent: &str| -> String {
            let suffix = format!(".{}", parent);
            members
                .keys()
                .find(|k| k.starts_with("Toybox.") && k.ends_with(&suffix))
                .or_else(|| members.keys().find(|k| k.ends_with(&suffix)))
                .cloned()
                .unwrap_or_else(|| owner_of(parent))
        };
        // (name, owner, raw parent, return type, doc text)
        let resolved: Vec<_> = fns
            .iter()
            .map(|(name, parent, ty, doc)| {
                (
                    name.clone(),
                    resolve(parent),
                    parent.clone(),
                    ty.clone(),
                    doc.clone(),
                )
            })
            .collect();
        let mut returns = BTreeMap::new();
        let mut return_names = BTreeMap::new();
        let mut docs = BTreeMap::new();
        for (name, owner, raw_parent, ty, doc) in resolved {
            members
                .entry(owner.clone())
                .or_default()
                .insert(name.clone());
            // Keep the unqualified parent key too (`Math.cos` lookups).
            members
                .entry(owner_of(&raw_parent))
                .or_default()
                .insert(name.clone());
            if let Some(ty) = ty {
                returns
                    .entry(format!("{}.{}", owner, name))
                    .or_insert_with(|| ty.clone());
                returns
                    .entry(format!("{}.{}", owner_of(&raw_parent), name))
                    .or_insert_with(|| ty.clone());
                return_names.entry(name.clone()).or_insert(ty);
            }
            if let Some(doc) = doc {
                docs.entry(format!("{}.{}", owner, name))
                    .or_insert_with(|| doc.clone());
                docs.entry(format!("{}.{}", owner_of(&raw_parent), name))
                    .or_insert(doc);
            }
        }
        ToyboxDb {
            members,
            returns,
            return_names,
            docs,
        }
    }

    /// Return type of `owner_hint.member` (e.g. `"Math"`, `"cos"`), falling
    /// back to the bare member name. `None` when unknown.
    pub fn return_type(&self, owner_hint: &str, member: &str) -> Option<String> {
        self.returns
            .get(&format!("{}.{}", owner_hint, member))
            .or_else(|| self.return_names.get(member))
            .cloned()
    }

    /// Resolve a bare type/module name to a dotted owner path.
    /// `"String"` → `"Toybox.Lang.String"`, `"Dc"` → `"Toybox.WatchUi.Dc"`.
    /// `Toybox.X` exact matches win, then `Toybox.*` suffix matches
    /// (preferred — the API XML also has sparse unqualified `parent`
    /// groups like `"Number"` that would shadow them), then bare `X`,
    /// then any suffix match.
    pub fn find_owner(&self, base: &str) -> Option<String> {
        let toybox = format!("Toybox.{}", base);
        if self.members.contains_key(&toybox) {
            return Some(toybox);
        }
        let suffix = format!(".{}", base);
        if let Some(k) = self
            .members
            .keys()
            .find(|k| k.starts_with("Toybox.") && k.ends_with(&suffix))
        {
            return Some(k.clone());
        }
        if self.members.contains_key(base) {
            return Some(base.to_string());
        }
        self.members.keys().find(|k| k.ends_with(&suffix)).cloned()
    }

    /// Member names of `owner` (e.g. `"Toybox.Math"`), sorted.
    pub fn members_of(&self, owner: &str) -> Vec<String> {
        self.members
            .get(owner)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Markdown doc text for `owner.member` (e.g. `"Toybox.Math"`, `"cos"`).
    /// `None` when the API has no documentation for the member.
    pub fn doc(&self, owner: &str, member: &str) -> Option<String> {
        self.docs.get(&format!("{}.{}", owner, member)).cloned()
    }

    /// Child names one level below `prefix` — e.g. `children_of("Toybox")`
    /// → `["Ant", "Math", ...]` (module names), `children_of("Toybox.Math")`
    /// → `["FirFilter", ...]` (class names in that module).
    pub fn children_of(&self, prefix: &str) -> Vec<String> {
        let mut out = BTreeSet::new();
        let head = format!("{}.", prefix);
        for key in self.members.keys() {
            if let Some(rest) = key.strip_prefix(&head) {
                if let Some(first) = rest.split('.').next() {
                    out.insert(first.to_string());
                }
            }
        }
        out.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_entries() {
        let xml = r#"<debugInfo>
<entry parent="Toybox_Math" symbol="cos" pc="1"/>
<entry parent="Toybox_Math" symbol="sin" pc="2"/>
<entry parent="Toybox_Math_FirFilter" symbol="reset" pc="3"/>
<entry parent="Toybox_Test" symbol="assert" pc="4"/>
<entry parent="Toybox_Test" symbol="&lt;init&gt;" pc="5"/>
</debugInfo>"#;
        let db = ToyboxDb::parse(xml);
        assert_eq!(db.members_of("Toybox.Math"), vec!["cos", "sin"]);
        assert_eq!(db.members_of("Toybox.Math.FirFilter"), vec!["reset"]);
        assert_eq!(db.members_of("Toybox.Test"), vec!["assert"]);
        assert_eq!(db.children_of("Toybox"), vec!["Math", "Test"]);
        assert_eq!(db.children_of("Toybox.Math"), vec!["FirFilter"]);
    }

    #[test]
    fn parses_return_types() {
        let xml = r#"<functionEntry accessMode="public" name="cos" parent="Math">
<documentation><![CDATA[<div class="returns">Returns  [Toybox::Lang::Float, Toybox::Lang::Double]</div>]]></documentation>
</functionEntry>
<functionEntry accessMode="public" name="format" parent="Lang_Number">
<documentation><![CDATA[<div class="returns">Returns  [Toybox::Lang::String]</div>]]></documentation>
</functionEntry>"#;
        let db = ToyboxDb::parse(xml);
        assert_eq!(db.return_type("Math", "cos").as_deref(), Some("Number"));
        assert_eq!(
            db.return_type("Lang.Number", "format").as_deref(),
            Some("String")
        );
        // Bare-name fallback.
        assert_eq!(db.return_type("Whatever", "cos").as_deref(), Some("Number"));
        assert_eq!(db.return_type("Math", "missing"), None);
    }

    #[test]
    fn parses_doc_text() {
        let xml = r#"<entry parent="Toybox_Math" symbol="cos" pc="1"/>
<functionEntry accessMode="public" name="cos" parent="Math">
<param id="x"/>
<documentation><![CDATA[<div class="description"> Get the cosine of an angle.<br/> <div class="param"> x [Toybox::Lang::Number]</div><br/>  The angle in radians<br/> <div class="returns">Returns  [Toybox::Lang::Float]</div><br/> @since 1.0.0<br/></div>]]></documentation>
</functionEntry>"#;
        let db = ToyboxDb::parse(xml);
        let doc = db.doc("Toybox.Math", "cos").expect("doc for Math.cos");
        assert!(doc.contains("cosine of an angle"));
        assert!(doc.contains("angle in radians"));
        assert!(doc.contains("Returns"));
        assert!(!doc.contains("<div"));
        assert!(!doc.contains("<br"));
        // Unqualified parent key also resolves.
        assert!(db.doc("Math", "cos").is_some());
        assert!(db.doc("Toybox.Math", "missing").is_none());
    }
}
