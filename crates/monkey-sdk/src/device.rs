//! Device discovery via the Connect IQ `Devices/` directory.
//!
//! Each device is a directory containing `compiler.json` (capabilities,
//! display name) and `simulator.json` (simulator metadata).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Information about a discovered Connect IQ device.
#[derive(Clone, Debug)]
pub struct Device {
    /// Device id (directory name, e.g. `fr55`).
    pub id: String,
    /// Human-readable name from `compiler.json` (`displayName`), if present.
    pub display_name: Option<String>,
    /// Device family (e.g. `fr55`, `fenix7`) from `compiler.json`.
    pub family: Option<String>,
    /// `[width, height]` resolution from `compiler.json`, if present.
    pub resolution: Option<(u32, u32)>,
    /// App types the device supports (`appTypes[].type` in compiler.json,
    /// e.g. `watchFace`, `datafield`, `widget`).
    pub app_types: Vec<String>,
    /// API level from `deviceGroup` (e.g. `API level 3.4` → `3.4`).
    pub api_level: Option<String>,
    /// Root directory of the device data.
    pub dir: PathBuf,
}

/// Parse a `compiler.json` file into device fields.
fn read_compiler_json(dir: &Path, id: &str) -> Option<Device> {
    let text = fs::read_to_string(dir.join("compiler.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let resolution = v.get("resolution").and_then(|r| {
        let w = r.get("width").and_then(Value::as_u64)? as u32;
        let h = r.get("height").and_then(Value::as_u64)? as u32;
        Some((w, h))
    });
    let app_types = v
        .get("appTypes")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.get("type").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    // `deviceGroup` looks like `API level 3.4` — extract the version.
    let api_level = v
        .get("deviceGroup")
        .and_then(Value::as_str)
        .and_then(|g| g.rsplit(' ').next())
        .map(str::to_string);
    Some(Device {
        id: v
            .get("deviceId")
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_string(),
        display_name: v
            .get("displayName")
            .and_then(Value::as_str)
            .map(str::to_string),
        family: v
            .get("deviceFamily")
            .and_then(Value::as_str)
            .map(str::to_string),
        resolution,
        app_types,
        api_level,
        dir: dir.to_path_buf(),
    })
}

/// `Devices/` directories under each Connect IQ root that exist.
fn device_dirs() -> Vec<PathBuf> {
    super::connect_iq_dirs()
        .into_iter()
        .map(|d| d.join("Devices"))
        .filter(|d| d.is_dir())
        .collect()
}

/// List all discovered devices, sorted by id.
pub fn list_devices() -> Vec<Device> {
    let mut devices = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for root in device_dirs() {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().to_string();
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(dev) = read_compiler_json(&dir, &id) {
                devices.push(dev);
            }
        }
    }
    devices.sort_by(|a, b| a.id.cmp(&b.id));
    devices
}

/// The set of Toybox API symbols available on a specific device, parsed
/// from its `<id>.api.debug.xml` (same `<entry parent="..." symbol="..."/>`
/// format as the SDK-wide `api.debug.xml`).
#[derive(Clone, Debug, Default)]
pub struct ApiSurface {
    /// `"Toybox.Math.cos"`-style fully-qualified member names.
    pub symbols: std::collections::HashSet<String>,
    /// Dotted owner paths that exist on the device
    /// (`"Toybox"`, `"Toybox.Math"`, `"Toybox.WatchUi.Dc"`, ...).
    pub owners: std::collections::HashSet<String>,
}

fn xml_attr(tag: &str, name: &str) -> Option<String> {
    let pat = format!("{}=\"", name);
    let i = tag.find(&pat)? + pat.len();
    let rest = &tag[i..];
    let j = rest.find('"')?;
    Some(rest[..j].to_string())
}

impl Device {
    /// Parse this device's API surface from `<id>.api.debug.xml` in its
    /// device directory (falls back to any `*.api.debug.xml` present).
    ///
    /// The file mixes three relevant sections:
    /// - `dataEntryOffsetMappings` — the symbol tree (`symbolId`/`parentId`)
    ///   used to resolve unqualified scope names to `Toybox.A.B` paths.
    /// - `pcToLineNum` — `<entry parent="Toybox_A_B" symbol="m"/>` qualified
    ///   class/module members.
    /// - `functions` — `<functionEntry parent="Scope" name="m"/>` members
    ///   under unqualified scope names resolved via the symbol tree.
    pub fn api_surface(&self) -> Option<ApiSurface> {
        let named = self.dir.join(format!("{}.api.debug.xml", self.id));
        let path = if named.is_file() {
            named
        } else {
            fs::read_dir(&self.dir)
                .ok()?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .find(|p| {
                    p.extension().is_some_and(|x| x == "xml")
                        && p.file_name()
                            .is_some_and(|n| n.to_string_lossy().ends_with(".api.debug.xml"))
                })?
        };
        let text = fs::read_to_string(path).ok()?;
        let mut surface = ApiSurface::default();

        // 1. Symbol tree: symbolId → parentId.
        let mut parents: HashMap<String, String> = HashMap::new();
        for m in text.match_indices("<dataEntry ") {
            let tag = text[m.0..].split('>').next().unwrap_or("");
            let (Some(id), Some(pid)) = (xml_attr(tag, "symbolId"), xml_attr(tag, "parentId"))
            else {
                continue;
            };
            parents.insert(id, pid);
        }
        // Dotted path for a symbolId, e.g. "Dc" → "Toybox.Graphics.Dc".
        // `globals` is the synthetic root.
        fn path_of(
            id: &str,
            parents: &HashMap<String, String>,
            memo: &mut HashMap<String, String>,
            depth: usize,
        ) -> String {
            if let Some(p) = memo.get(id) {
                return p.clone();
            }
            // Depth cap guards against parentId cycles in the data.
            if id == "globals" || depth > 16 {
                return String::new();
            }
            let parent = parents.get(id).cloned().unwrap_or_default();
            let base = if parent.is_empty() {
                String::new()
            } else {
                path_of(&parent, parents, memo, depth + 1)
            };
            let full = if base.is_empty() {
                id.to_string()
            } else {
                format!("{}.{}", base, id)
            };
            memo.insert(id.to_string(), full.clone());
            full
        }
        let mut memo = HashMap::new();
        for id in parents.keys() {
            let p = path_of(id, &parents, &mut memo, 0);
            surface.owners.insert(p);
        }
        // "Toybox" itself is always an owner.
        surface.owners.insert("Toybox".to_string());

        // 2. Qualified members from pcToLineNum entries.
        for chunk in text.split("<entry ").skip(1) {
            let tag = chunk.split('>').next().unwrap_or("");
            let (Some(parent), Some(symbol)) = (xml_attr(tag, "parent"), xml_attr(tag, "symbol"))
            else {
                continue;
            };
            if symbol.contains("&lt;") || !parent.starts_with("Toybox") {
                continue;
            }
            let owner = parent.replace('_', ".");
            surface.owners.insert(owner.clone());
            surface.symbols.insert(format!("{}.{}", owner, symbol));
        }

        // 3. Members under unqualified scope names from <functions>.
        for m in text.match_indices("<functionEntry ") {
            let tag = text[m.0..].split('>').next().unwrap_or("");
            let (Some(scope), Some(name)) = (xml_attr(tag, "parent"), xml_attr(tag, "name")) else {
                continue;
            };
            if name.contains("&lt;") {
                continue;
            }
            let owner = path_of(&scope, &parents, &mut memo, 0);
            if owner.is_empty() {
                continue;
            }
            surface.symbols.insert(format!("{}.{}", owner, name));
        }
        Some(surface)
    }
}

/// Find a device by id (exact match; also matches `deviceFamily`/`displayName`
/// case-insensitively as a fallback).
pub fn find_device(id: &str) -> Option<Device> {
    let devices = list_devices();
    devices.iter().find(|d| d.id == id).cloned().or_else(|| {
        let needle = id.to_lowercase();
        devices
            .iter()
            .find(|d| {
                d.family
                    .as_deref()
                    .is_some_and(|f| f.eq_ignore_ascii_case(id))
                    || d.display_name
                        .as_deref()
                        .is_some_and(|n| n.to_lowercase().contains(&needle))
            })
            .cloned()
    })
}
