//! `manifest.xml` parsing for Connect IQ projects.

use std::fs;
use std::path::Path;

use crate::xml::{self, Element};

/// The `<iq:application>` element of a manifest.
#[derive(Clone, Debug, Default)]
pub struct Application {
    /// Application UUID.
    pub id: Option<String>,
    /// App type: `watchface`, `watch-app`, `data-field`, `widget`, etc.
    pub app_type: Option<String>,
    /// Display name (often a `@Strings.*` resource reference).
    pub name: Option<String>,
    /// Entry-point class name.
    pub entry: Option<String>,
    /// Launcher icon resource reference.
    pub launcher_icon: Option<String>,
    /// Minimum API level (e.g. `1.2.0`, `3.0.0`).
    pub min_api_level: Option<String>,
    /// Target device/product ids from `<iq:products>`.
    pub products: Vec<String>,
    /// Requested permissions.
    pub permissions: Vec<String>,
    /// Supported language codes.
    pub languages: Vec<String>,
}

impl Application {
    /// Whether this app type is a watch face.
    pub fn is_watchface(&self) -> bool {
        self.app_type.as_deref() == Some("watchface")
    }
}

/// A parsed `manifest.xml`.
#[derive(Clone, Debug, Default)]
pub struct Manifest {
    /// The `<iq:application>` element, if present.
    pub application: Option<Application>,
}

fn collect_ids(parent: Option<&Element>, tag: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(p) = parent {
        let mut found = Vec::new();
        p.descendants_named(tag, &mut found);
        for el in found {
            if let Some(id) = el.attr("id") {
                out.push(id.to_string());
            }
        }
    }
    out
}

fn collect_text(parent: Option<&Element>, tag: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(p) = parent {
        let mut found = Vec::new();
        p.descendants_named(tag, &mut found);
        for el in found {
            let t = el.text.trim();
            if !t.is_empty() {
                out.push(t.to_string());
            }
        }
    }
    out
}

/// Parse manifest XML text.
pub fn parse(source: &str) -> Manifest {
    let roots = xml::parse(source);
    let mut application = None;
    for root in &roots {
        let mut apps = Vec::new();
        root.descendants_named("application", &mut apps);
        // The outermost <application> is the manifest's application element;
        // nested matches (unlikely) would appear later in document order.
        if let Some(app_el) = apps.first() {
            application = Some(Application {
                id: app_el.attr("id").map(str::to_string),
                app_type: app_el.attr("type").map(str::to_string),
                name: app_el.attr("name").map(str::to_string),
                entry: app_el.attr("entry").map(str::to_string),
                launcher_icon: app_el.attr("launcherIcon").map(str::to_string),
                min_api_level: app_el.attr("minApiLevel").map(str::to_string),
                products: collect_ids(Some(app_el), "product"),
                permissions: collect_ids(Some(app_el), "uses-permission"),
                languages: collect_text(Some(app_el), "language"),
            });
            break;
        }
    }
    Manifest { application }
}

/// Read and parse `manifest.xml` at `path`.
pub fn load(path: &Path) -> std::io::Result<Manifest> {
    Ok(parse(&fs::read_to_string(path)?))
}
