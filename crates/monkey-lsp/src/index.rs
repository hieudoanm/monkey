//! Project-wide symbol index for cross-file language features.
//!
//! Scans a directory tree for `*.mc` files (skipping `node_modules`, `target`,
//! `.git`), parses each, and collects declarations. Used by definition,
//! references, and rename to resolve symbols across files.

use std::fs;
use std::path::{Path, PathBuf};

use crate::intel::{collect_decls, Decl};

/// One indexed source file.
pub struct IndexedFile {
    pub path: PathBuf,
    pub uri: String,
    pub src: String,
    pub decls: Vec<Decl>,
}

/// All `*.mc` files under a root with their declarations.
pub struct ProjectIndex {
    pub files: Vec<IndexedFile>,
}

const SKIP_DIRS: &[&str] = &["node_modules", "target", ".git"];

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !SKIP_DIRS.contains(&name.as_ref()) {
                walk(&path, out);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("mc") {
            out.push(path);
        }
    }
}

/// `file:///x/y z.mc` style URI for a path (minimal percent-encoding).
pub fn path_to_uri(path: &Path) -> String {
    let s = path.to_string_lossy();
    let mut out = String::from("file://");
    for b in s.bytes() {
        match b {
            b' ' => out.push_str("%20"),
            b'%' => out.push_str("%25"),
            b'#' => out.push_str("%23"),
            b'?' => out.push_str("%3F"),
            _ => out.push(b as char),
        }
    }
    out
}

/// Inverse of [`path_to_uri`] for the common case.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h: String = chars.by_ref().take(2).collect();
            out.push(char::from_u32(u32::from_str_radix(&h, 16).ok()?)?);
        } else {
            out.push(c);
        }
    }
    Some(PathBuf::from(out))
}

impl ProjectIndex {
    /// Build an index by scanning `root` for `*.mc` files.
    pub fn build(root: &Path) -> Self {
        let mut paths = Vec::new();
        walk(root, &mut paths);
        paths.sort();
        let files = paths
            .into_iter()
            .filter_map(|p| {
                let src = fs::read_to_string(&p).ok()?;
                let decls = collect_decls(&src);
                Some(IndexedFile {
                    uri: path_to_uri(&p),
                    path: p,
                    src,
                    decls,
                })
            })
            .collect();
        ProjectIndex { files }
    }

    /// Declarations named `name` in the file with `uri`.
    pub fn find_decls_in<'a>(&'a self, uri: &str, name: &str) -> Vec<(&'a IndexedFile, &'a Decl)> {
        self.files
            .iter()
            .filter(|f| f.uri == uri)
            .flat_map(|f| f.decls.iter().map(move |d| (f, d)))
            .filter(|(_, d)| d.name == name)
            .collect()
    }

    /// All declarations named `name` across the project.
    pub fn find_decls<'a>(&'a self, name: &str) -> Vec<(&'a IndexedFile, &'a Decl)> {
        self.files
            .iter()
            .flat_map(|f| f.decls.iter().map(move |d| (f, d)))
            .filter(|(_, d)| d.name == name)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_roundtrip() {
        let p = PathBuf::from("/tmp/a b/c#d.mc");
        let uri = path_to_uri(&p);
        assert_eq!(uri, "file:///tmp/a%20b/c%23d.mc");
        assert_eq!(uri_to_path(&uri).unwrap(), p);
    }

    #[test]
    fn index_finds_decls_across_files() {
        let dir = std::env::temp_dir().join(format!("monkey-idx-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("src/a.mc"),
            "class Shared {\n  function hello() as Void {}\n}\n",
        )
        .unwrap();
        fs::write(dir.join("src/b.mc"), "var other = 1;\n").unwrap();
        let idx = ProjectIndex::build(&dir);
        assert_eq!(idx.files.len(), 2);
        let decls = idx.find_decls("Shared");
        assert_eq!(decls.len(), 1);
        assert!(decls[0].0.path.ends_with("a.mc"));
        assert!(idx.find_decls("other")[0].0.path.ends_with("b.mc"));
        assert!(idx.find_decls("missing").is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
