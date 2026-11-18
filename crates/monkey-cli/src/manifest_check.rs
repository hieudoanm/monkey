//! Manifest validation for `monkey project check`.
//!
//! Cross-checks `manifest.xml` against the Connect IQ SDK device profiles:
//! unknown product ids, unsupported app types, and `minApiLevel` vs each
//! device's API level.

use monkey_core::Diagnostic;
use monkey_project::Project;
use monkey_sdk::device::{find_device, Device};

/// Normalize an app-type or compiler.json type for comparison:
/// `watch-app`/`watchApp`/`WATCHFACE` all → `watchapp`/`watchface`.
fn norm_type(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Parse `major.minor[.patch]` into `(major, minor)` for comparison.
fn api_tuple(v: &str) -> Option<(u64, u64)> {
    let mut it = v.trim().split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next().unwrap_or("0").parse().ok()?;
    Some((major, minor))
}

fn device_supports_type(d: &Device, app_type: &str) -> bool {
    let want = norm_type(app_type);
    d.app_types.iter().any(|t| norm_type(t) == want)
}

/// Validate the project manifest; returns diagnostics (warnings mostly).
pub fn validate(project: &Project) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let Some(app) = &project.manifest.application else {
        diags.push(
            Diagnostic::error(
                "manifest-no-application",
                "manifest.xml has no <iq:application>",
            )
            .with_span_opt(None),
        );
        return diags;
    };

    if app.id.is_none() {
        diags.push(Diagnostic::warning(
            "manifest-missing-id",
            "<iq:application> has no id attribute",
        ));
    }
    let app_type = app.app_type.clone().unwrap_or_default();
    if app_type.is_empty() {
        diags.push(Diagnostic::warning(
            "manifest-missing-type",
            "<iq:application> has no type attribute",
        ));
    }
    if app.entry.is_none() {
        diags.push(Diagnostic::warning(
            "manifest-missing-entry",
            "<iq:application> has no entry attribute",
        ));
    }
    if app.products.is_empty() {
        diags.push(Diagnostic::warning(
            "manifest-no-products",
            "manifest declares no <iq:product> targets",
        ));
    }

    // Resolve product ids to device profiles.
    let mut devices = Vec::new();
    for pid in &app.products {
        if pid == "*" || pid.eq_ignore_ascii_case("all") {
            continue;
        }
        match find_device(pid) {
            Some(d) => devices.push(d),
            None => diags.push(Diagnostic::warning(
                "manifest-unknown-product",
                format!("product id '{}' not found in Connect IQ Devices", pid),
            )),
        }
    }

    // App type must be supported by at least one targeted device.
    if !app_type.is_empty() && !devices.is_empty() {
        let supported = devices.iter().any(|d| device_supports_type(d, &app_type));
        if !supported {
            let names: Vec<&str> = devices.iter().map(|d| d.id.as_str()).collect();
            diags.push(Diagnostic::warning(
                "manifest-unsupported-type",
                format!(
                    "app type '{}' is not supported by any target device ({})",
                    app_type,
                    names.join(", ")
                ),
            ));
        }
    }

    // minApiLevel must not exceed a device's API level.
    if let (Some(min), false) = (&app.min_api_level, devices.is_empty()) {
        if let Some(want) = api_tuple(min) {
            let mut incompatible = Vec::new();
            for d in &devices {
                if let Some(have) = d.api_level.as_deref().and_then(api_tuple) {
                    if have < want {
                        incompatible.push(format!(
                            "{} ({})",
                            d.id,
                            d.api_level.as_deref().unwrap_or("?")
                        ));
                    }
                }
            }
            if !incompatible.is_empty() {
                diags.push(Diagnostic::warning(
                    "manifest-api-level",
                    format!(
                        "minApiLevel {} exceeds the API level of: {}",
                        min,
                        incompatible.join(", ")
                    ),
                ));
            }
        }
    }

    diags
}
