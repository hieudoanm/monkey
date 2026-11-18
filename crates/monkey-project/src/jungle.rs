//! `monkey.jungle` parsing.
//!
//! Jungle files are simple `key = value` property files with `#` comments,
//! e.g. `base.sourcePath = source`. Entries keep file order; lookups are
//! case-sensitive like the SDK.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// A parsed `monkey.jungle` file.
#[derive(Clone, Debug, Default)]
pub struct Jungle {
    /// Entries in file order.
    pub entries: Vec<(String, String)>,
}

impl Jungle {
    /// Look up a property value. Later duplicate keys win, matching typical
    /// property-file semantics.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Whether the jungle declares any source path (`base.sourcePath`).
    pub fn has_source_path(&self) -> bool {
        self.get("base.sourcePath").is_some()
    }

    /// All known key → value pairs (later duplicates win).
    pub fn map(&self) -> BTreeMap<&str, &str> {
        self.entries
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }
}

/// Parse jungle text. Lines without `=` are ignored; blank lines and `#`
/// comments are skipped. Inline `#` inside values is preserved.
pub fn parse(source: &str) -> Jungle {
    let mut entries = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(eq) = line.find('=') else {
            continue;
        };
        let key = line[..eq].trim();
        if key.is_empty() {
            continue;
        }
        let mut value = line[eq + 1..].trim();
        // Strip one layer of matching surrounding quotes.
        if value.len() >= 2 {
            let bytes = value.as_bytes();
            if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
                || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
            {
                value = &value[1..value.len() - 1];
            }
        }
        entries.push((key.to_string(), value.to_string()));
    }
    Jungle { entries }
}

/// Read and parse `monkey.jungle` at `path`.
pub fn load(path: &Path) -> std::io::Result<Jungle> {
    Ok(parse(&fs::read_to_string(path)?))
}
