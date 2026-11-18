//! Toybox API member database for completion.
//!
//! Reads `bin/api.debug.xml` from the detected Connect IQ SDK — it maps
//! `parent="Toybox_Module_Class"` to member `symbol`s. Gives `Toybox.X.`
//! member completion without shipping a generated table.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

/// Dotted-owner → member names, e.g. `"Toybox.Math"` → `["cos", "sin", ...]`.
pub struct ToyboxDb {
    members: BTreeMap<String, BTreeSet<String>>,
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

impl ToyboxDb {
    /// Load from the detected SDK's `bin/api.debug.xml`.
    pub fn load() -> Option<Self> {
        let sdk = monkey_sdk::find_sdk(None)?;
        let path = sdk.bin().join("api.debug.xml");
        let text = fs::read_to_string(path).ok()?;
        Some(Self::parse(&text))
    }

    /// Parse `<entry parent="..." symbol="..."/>` records.
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
        ToyboxDb { members }
    }

    /// Member names of `owner` (e.g. `"Toybox.Math"`), sorted.
    pub fn members_of(&self, owner: &str) -> Vec<String> {
        self.members
            .get(owner)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
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
}
