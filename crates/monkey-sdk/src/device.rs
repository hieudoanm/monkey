//! Device discovery via the Connect IQ `Devices/` directory.
//!
//! Each device is a directory containing `compiler.json` (capabilities,
//! display name) and `simulator.json` (simulator metadata).

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
