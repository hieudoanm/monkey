//! Connect IQ project model and discovery.
//!
//! A project root contains `monkey.jungle` and `manifest.xml`. Discovery
//! walks up parent directories (same convention as the editor extensions:
//! up to 8 levels) so commands work from any file inside a project.

use std::fs;
use std::path::{Path, PathBuf};

use crate::jungle::Jungle;
use crate::manifest::Manifest;

/// How many parent directories to search when discovering a project.
pub const MAX_DEPTH: usize = 8;

/// A discovered Connect IQ project.
#[derive(Clone, Debug)]
pub struct Project {
    /// Project root directory.
    pub root: PathBuf,
    /// Path to `monkey.jungle`.
    pub jungle_path: PathBuf,
    /// Parsed `monkey.jungle`.
    pub jungle: Jungle,
    /// Path to `manifest.xml`.
    pub manifest_path: PathBuf,
    /// Parsed `manifest.xml`.
    pub manifest: Manifest,
}

impl Project {
    /// The directory holding the app's source, resolved from
    /// `base.sourcePath` in the jungle. Glob values (`source/**/*.mc`,
    /// `.\**.mc`) resolve to their static directory prefix; defaults to
    /// `source` when unset.
    pub fn source_dir(&self) -> PathBuf {
        let raw = self.jungle.get("base.sourcePath").unwrap_or("source");
        let prefix = raw
            .split(['*', '?'])
            .next()
            .unwrap_or(raw)
            .trim_end_matches(['/', '\\'])
            .trim_end_matches('.');
        let dir = if prefix.is_empty() { "." } else { prefix };
        self.root.join(dir)
    }

    /// Resource directory from `base.resourcePath`, defaulting to `resources`.
    pub fn resource_dir(&self) -> PathBuf {
        let rel = self.jungle.get("base.resourcePath").unwrap_or("resources");
        self.root.join(rel)
    }

    /// Target device ids declared in the manifest.
    pub fn products(&self) -> &[String] {
        self.manifest
            .application
            .as_ref()
            .map(|a| a.products.as_slice())
            .unwrap_or(&[])
    }

    /// One-line summary, e.g. `hands (watchface) → fr55, venu3`.
    pub fn describe(&self) -> String {
        let app = self.manifest.application.as_ref();
        let name = app.and_then(|a| a.name.as_deref()).unwrap_or_else(|| {
            self.root
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or("?")
        });
        let kind = app.and_then(|a| a.app_type.as_deref()).unwrap_or("app");
        let products = self.products();
        if products.is_empty() {
            format!("{} ({})", name, kind)
        } else {
            format!("{} ({}) → {}", name, kind, products.join(", "))
        }
    }
}

/// Load a project rooted at `root`. The manifest filename comes from the
/// `project.manifest` jungle key (default `manifest.xml`).
pub fn load(root: &Path) -> std::io::Result<Project> {
    let jungle_path = root.join("monkey.jungle");
    let jungle = crate::jungle::load(&jungle_path)?;
    let manifest_name = jungle.get("project.manifest").unwrap_or("manifest.xml");
    let manifest_path = root.join(manifest_name);
    let manifest = crate::manifest::load(&manifest_path)?;
    Ok(Project {
        root: root.to_path_buf(),
        jungle_path,
        jungle,
        manifest_path,
        manifest,
    })
}

/// Whether `dir` looks like a project root.
pub fn is_project_root(dir: &Path) -> bool {
    if !dir.join("monkey.jungle").is_file() {
        return false;
    }
    // Honor project.manifest when present.
    if let Ok(j) = crate::jungle::load(&dir.join("monkey.jungle")) {
        let name = j.get("project.manifest").unwrap_or("manifest.xml");
        dir.join(name).is_file()
    } else {
        dir.join("manifest.xml").is_file()
    }
}

/// Walk up from `start` (a file or directory) looking for a project root.
/// Returns `None` if nothing found within [`MAX_DEPTH`] levels.
pub fn discover(start: &Path) -> Option<Project> {
    let mut dir = if start.is_dir() {
        start.to_path_buf()
    } else {
        start.parent()?.to_path_buf()
    };
    for _ in 0..MAX_DEPTH {
        if is_project_root(&dir) {
            return load(&dir).ok();
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Find the project containing `path`, or `None`. `path` may be a file or
/// directory; relative paths are resolved against the current directory.
pub fn discover_from(path: &str) -> Option<Project> {
    let p = PathBuf::from(path);
    let abs = if p.is_absolute() {
        p
    } else {
        std::env::current_dir().ok()?.join(p)
    };
    discover(&abs)
}

/// Check that a project root's declared files exist; returns human-readable
/// problems (empty = healthy).
pub fn check_files(project: &Project) -> Vec<String> {
    let mut problems = Vec::new();
    let source = project.source_dir();
    if !source.is_dir() {
        problems.push(format!(
            "base.sourcePath `{}` does not exist",
            project.jungle.get("base.sourcePath").unwrap_or("source")
        ));
    } else {
        let has_mc = fs::read_dir(&source)
            .map(|rd| {
                rd.flatten()
                    .any(|e| e.path().extension().is_some_and(|x| x == "mc"))
            })
            .unwrap_or(false);
        if !has_mc {
            problems.push(format!("no .mc files under {}", source.display()));
        }
    }
    let resources = project.resource_dir();
    if !resources.is_dir() {
        problems.push(format!(
            "base.resourcePath `{}` does not exist",
            project
                .jungle
                .get("base.resourcePath")
                .unwrap_or("resources")
        ));
    }
    problems
}
