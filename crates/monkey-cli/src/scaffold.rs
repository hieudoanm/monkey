//! `monkey new` — scaffold a minimal Connect IQ project.
//!
//! Generates `monkey.jungle`, `manifest.xml`, `resources/strings/strings.xml`,
//! and a two-file `source/` app (`<Name>App.mc` + `<Name>View.mc`) following
//! the `Application.AppBase` + `WatchUi.View` structure used by real projects.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// App types accepted by `--type` (manifest `type` attribute values).
pub const APP_TYPES: &[&str] = &["watchface", "datafield", "watchapp", "widget"];

pub struct ScaffoldOptions {
    /// Target directory (created if missing).
    pub dir: PathBuf,
    /// App name used for the entry class (`<Name>App`). Derived from the
    /// directory name when empty.
    pub name: Option<String>,
    /// Manifest `type` attribute.
    pub app_type: String,
    /// Device id to put in `<iq:products>` (may be repeated via caller).
    pub devices: Vec<String>,
    /// `minApiLevel` attribute.
    pub min_api: String,
}

/// PascalCase a directory/file name into a valid Monkey C identifier stem.
pub fn pascal_case(input: &str) -> String {
    let mut out = String::new();
    let mut capitalize = true;
    for ch in input.chars() {
        if ch.is_alphanumeric() {
            if capitalize {
                out.extend(ch.to_uppercase());
                capitalize = false;
            } else {
                out.push(ch);
            }
        } else {
            capitalize = true;
        }
    }
    if out.is_empty() {
        "MyApp".to_string()
    } else {
        out
    }
}

/// Generate a UUID v4-shaped id from time + pid (uniqueness is best-effort;
/// the manifest only needs a stable unique-looking id).
fn generate_uuid() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut state = nanos ^ ((std::process::id() as u64) << 32) ^ 0x9e3779b97f4a7c15;
    let mut next = || {
        // xorshift64*
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state.wrapping_mul(0x2545F4914F6CDD1D)
    };
    let a = next();
    let b = next();
    let c = next();
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (a >> 32) as u32,
        (a >> 16) as u16,
        (a & 0xfff) as u16,
        ((b >> 48) as u16 & 0x3fff) | 0x8000,
        ((b as u128) << 64 | c as u128) & 0xffffffffffff
    )
}

fn manifest_xml(opts: &ScaffoldOptions, app_name: &str) -> String {
    let products = if opts.devices.is_empty() {
        "    <iq:products/>\n".to_string()
    } else {
        let items: String = opts
            .devices
            .iter()
            .map(|d| format!("      <iq:product id=\"{}\"/>\n", d))
            .collect();
        format!("    <iq:products>\n{}    </iq:products>\n", items)
    };
    format!(
        r#"<?xml version="1.0"?>
<iq:manifest version="3" xmlns:iq="http://www.garmin.com/xml/connectiq">
  <iq:application id="{}" type="{}" name="@Strings.AppName" entry="{}App" launcherIcon="@Drawables.LauncherIcon" minApiLevel="{}">
{}
    <iq:permissions/>
    <iq:languages><iq:language>eng</iq:language></iq:languages>
    <iq:barrels/>
  </iq:application>
</iq:manifest>
"#,
        generate_uuid(),
        opts.app_type,
        app_name,
        opts.min_api,
        products
    )
}

fn app_mc(app_name: &str, view_name: &str) -> String {
    format!(
        r#"import Toybox.Application;
import Toybox.Lang;
import Toybox.WatchUi;

class {app} extends Application.AppBase {{
  function initialize() {{
    AppBase.initialize();
  }}

  function getInitialView() as [Views] or [Views, InputDelegates] {{
    return [new {view}()];
  }}
}}

function getApp() as {app} {{
  return Application.getApp() as {app};
}}
"#,
        app = format!("{}App", app_name),
        view = view_name,
    )
}

fn view_mc(app_name: &str, view_name: &str) -> String {
    format!(
        r#"import Toybox.Graphics;
import Toybox.Lang;
import Toybox.WatchUi;

class {view} extends WatchUi.View {{
  function initialize() {{
    View.initialize();
  }}

  function onUpdate(dc as Dc) as Void {{
    dc.setColor(Graphics.COLOR_BLACK, Graphics.COLOR_BLACK);
    dc.clear();
    dc.setColor(Graphics.COLOR_WHITE, Graphics.COLOR_BLACK);
    dc.drawText(
      dc.getWidth() / 2,
      dc.getHeight() / 2,
      Graphics.FONT_MEDIUM,
      "{label}",
      Graphics.TEXT_JUSTIFY_CENTER
    );
  }}
}}
"#,
        view = view_name,
        label = app_name,
    )
}

/// Create the project. Returns the list of files written.
pub fn scaffold(opts: &ScaffoldOptions) -> std::io::Result<Vec<PathBuf>> {
    let dir_name = opts
        .dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let app_name = opts
        .name
        .clone()
        .map(|n| pascal_case(&n))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| pascal_case(&dir_name));
    let view_name = format!("{}View", app_name);

    let jungle = opts.dir.join("monkey.jungle");
    let manifest = opts.dir.join("manifest.xml");
    if jungle.exists() || manifest.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!(
                "{} already contains a Connect IQ project",
                opts.dir.display()
            ),
        ));
    }

    let files: Vec<(PathBuf, String)> = vec![
        (jungle, "project.manifest = manifest.xml\n".to_string()),
        (manifest, manifest_xml(opts, &app_name)),
        (
            opts.dir.join("resources/strings/strings.xml"),
            format!(
                r#"<strings xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="https://developer.garmin.com/downloads/connect-iq/resources.xsd">
  <string id="AppName">{}</string>
</strings>
"#,
                app_name
            ),
        ),
        (
            opts.dir.join(format!("source/{}App.mc", app_name)),
            app_mc(&app_name, &view_name),
        ),
        (
            opts.dir.join("resources/drawables/drawables.xml"),
            r#"<drawables xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="https://developer.garmin.com/downloads/connect-iq/resources.xsd">
  <bitmap id="LauncherIcon" filename="launcher_icon.svg" dithering="none" />
</drawables>
"#
            .to_string(),
        ),
        (
            opts.dir.join("resources/drawables/launcher_icon.svg"),
            r##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 35 35" width="35" height="35"><rect width="35" height="35" rx="8" fill="#1b1b1d"/><circle cx="17.5" cy="17.5" r="6" fill="#d71921"/></svg>
"##
            .to_string(),
        ),
        (
            opts.dir.join(format!("source/{}.mc", view_name)),
            view_mc(&app_name, &view_name),
        ),
    ];

    let mut written = Vec::new();
    for (path, contents) in files {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        // Emit formatter-clean output so `monkey check` passes immediately.
        let contents = match path.extension().and_then(|e| e.to_str()) {
            Some("mc") => monkey_formatter::format_monkeyc(&contents, &Default::default()),
            Some("jungle") => monkey_formatter::format_jungle(&contents, &Default::default()),
            _ => contents,
        };
        fs::write(&path, contents)?;
        written.push(path);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pascal_case_basic() {
        assert_eq!(pascal_case("my-watch-face"), "MyWatchFace");
        assert_eq!(pascal_case("hands"), "Hands");
        assert_eq!(pascal_case(""), "MyApp");
        assert_eq!(pascal_case("123app"), "123app");
    }

    #[test]
    fn scaffold_writes_project() {
        let dir = std::env::temp_dir().join(format!("monkey-new-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let opts = ScaffoldOptions {
            dir: dir.clone(),
            name: Some("testapp".to_string()),
            app_type: "watchface".to_string(),
            devices: vec!["fr55".to_string()],
            min_api: "1.2.0".to_string(),
        };
        let files = scaffold(&opts).unwrap();
        assert_eq!(files.len(), 7);
        let manifest = fs::read_to_string(dir.join("manifest.xml")).unwrap();
        assert!(manifest.contains("type=\"watchface\""));
        assert!(manifest.contains("entry=\"TestappApp\""));
        assert!(manifest.contains("id=\"fr55\""));
        let app = fs::read_to_string(dir.join("source/TestappApp.mc")).unwrap();
        assert!(app.contains("class TestappApp extends Application.AppBase"));
        // Second scaffold into same dir must fail.
        assert!(scaffold(&opts).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
