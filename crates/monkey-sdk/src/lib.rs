//! Garmin Connect IQ SDK and developer-key discovery.
//!
//! Mirrors the lookup order used by the editor extensions
//! (`packages/extensions/*/runtime-preview`): explicit configuration first,
//! then `current-sdk.cfg`, then the `Sdks/` directory scan.

pub mod device;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// A discovered Connect IQ SDK installation.
#[derive(Clone, Debug)]
pub struct Sdk {
    /// SDK root directory (contains `bin/monkeyc`).
    pub root: PathBuf,
    /// SDK version string when known (e.g. `9.3.0`).
    pub version: Option<String>,
    /// Whether this SDK is pointed to by `current-sdk.cfg`.
    pub current: bool,
}

impl Sdk {
    /// Path to the SDK's `bin` directory.
    pub fn bin(&self) -> PathBuf {
        self.root.join("bin")
    }

    /// Whether a named SDK tool exists (`monkeyc`, `monkeydo`, `connectiq`, ...).
    pub fn has_tool(&self, name: &str) -> bool {
        self.bin().join(name).exists()
    }
}

/// Read the SDK version from `bin/version.txt`, falling back to
/// `<version>` inside `bin/compilerInfo.xml`.
pub fn sdk_version(root: &Path) -> Option<String> {
    let v = root.join("bin").join("version.txt");
    if let Ok(s) = fs::read_to_string(&v) {
        let s = s.trim();
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }
    let info = root.join("bin").join("compilerInfo.xml");
    if let Ok(xml) = fs::read_to_string(&info) {
        if let Some(start) = xml.find("<version>") {
            let rest = &xml[start + "<version>".len()..];
            if let Some(end) = rest.find("</version>") {
                let v = rest[..end].trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

fn sdk_at(root: PathBuf, current: bool) -> Sdk {
    Sdk {
        version: sdk_version(&root),
        root,
        current,
    }
}

/// Candidate Connect IQ root directories, in priority order.
///
/// `~/Library/Application Support/Garmin/ConnectIQ` (macOS),
/// `~/.Garmin/ConnectIQ` (Linux/macOS alt), `%LOCALAPPDATA%\Garmin\ConnectIQ`
/// and `%APPDATA%\Garmin\ConnectIQ` (Windows).
pub fn connect_iq_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE")) {
        let home = PathBuf::from(home);
        dirs.push(home.join("Library/Application Support/Garmin/ConnectIQ"));
        dirs.push(home.join(".Garmin/ConnectIQ"));
    }
    if let Some(local) = env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local).join("Garmin/ConnectIQ"));
    }
    if let Some(appdata) = env::var_os("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("Garmin/ConnectIQ"));
    }
    dirs.sort();
    dirs.dedup();
    dirs
}

/// Whether `path` looks like an SDK root (contains `bin/monkeyc`).
pub fn is_sdk_root(path: &Path) -> bool {
    path.join("bin").join("monkeyc").is_file() || path.join("bin").join("monkeyc.bat").is_file()
}

/// SDK roots pointed to by `current-sdk.cfg` files, if any.
fn current_sdk_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in connect_iq_dirs() {
        let cfg = dir.join("current-sdk.cfg");
        if let Ok(s) = fs::read_to_string(&cfg) {
            let p = s.trim();
            if !p.is_empty() {
                out.push(PathBuf::from(p));
            }
        }
    }
    out
}

/// All SDKs found under the Connect IQ `Sdks/` directories, newest first
/// (directory names sort by version/date).
pub fn list_sdks() -> Vec<Sdk> {
    let mut sdks = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // current-sdk.cfg entries first.
    for p in current_sdk_paths() {
        if is_sdk_root(&p) && seen.insert(p.clone()) {
            sdks.push(sdk_at(p, true));
        }
    }

    for dir in connect_iq_dirs() {
        let sdks_dir = dir.join("Sdks");
        let Ok(entries) = fs::read_dir(&sdks_dir) else {
            continue;
        };
        let mut candidates: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && is_sdk_root(p))
            .collect();
        candidates.sort();
        candidates.reverse();
        for p in candidates {
            if seen.insert(p.clone()) {
                sdks.push(sdk_at(p, false));
            }
        }
    }
    sdks
}

/// Resolve which SDK to use.
///
/// Order: explicit `configured` path → `MONKEYC_SDK_PATH`/`MONKEY_SDK` env →
/// `current-sdk.cfg` → newest SDK under `Sdks/`.
pub fn find_sdk(configured: Option<&Path>) -> Option<Sdk> {
    if let Some(p) = configured {
        let p = expand_home(p);
        return is_sdk_root(&p).then(|| sdk_at(p, false));
    }
    for key in ["MONKEYC_SDK_PATH", "MONKEY_SDK"] {
        if let Ok(v) = env::var(key) {
            if !v.is_empty() {
                return find_sdk(Some(Path::new(&v)));
            }
        }
    }
    list_sdks().into_iter().next()
}

/// Expand a leading `~` in a path.
pub fn expand_home(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE")) {
            return PathBuf::from(home).join(rest);
        }
    }
    path.to_path_buf()
}

/// Candidate developer-key locations, in priority order.
///
/// Explicit config/env first is handled by [`find_developer_key`]; these are
/// the standard on-disk locations (same set as the Zed extension).
pub fn developer_key_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(home) = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE")) {
        let home = PathBuf::from(home);
        out.push(home.join(".config/garmin/developer_key"));
        out.push(home.join(".config/garmin/developer_key.der"));
        out.push(home.join("Library/Application Support/Garmin/ConnectIQ/developer_key.der"));
        for editor in ["Code", "Cursor", "VSCodium", "Windsurf"] {
            out.push(home.join(format!(
                "Library/Application Support/{}/User/globalStorage/hieudoanm.monkey-c/developer_key.der",
                editor
            )));
        }
    }
    out
}

/// Resolve the developer key: explicit path → `MONKEYC_DEVELOPER_KEY`/`MONKEY_KEY`
/// env → standard locations.
pub fn find_developer_key(configured: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = configured {
        let p = expand_home(p);
        return p.is_file().then_some(p);
    }
    for key in ["MONKEYC_DEVELOPER_KEY", "MONKEY_KEY"] {
        if let Ok(v) = env::var(key) {
            if !v.is_empty() {
                let p = expand_home(Path::new(&v));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    developer_key_candidates().into_iter().find(|p| p.is_file())
}
