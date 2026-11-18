use monkey_sdk::{device, is_sdk_root};
use std::fs;
use std::path::PathBuf;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("monkey-sdk-test-{}", name));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn is_sdk_root_requires_monkeyc() {
    let dir = temp_dir("sdkroot");
    assert!(!is_sdk_root(&dir));
    fs::create_dir_all(dir.join("bin")).unwrap();
    fs::write(dir.join("bin/monkeyc"), "#!/bin/sh\n").unwrap();
    assert!(is_sdk_root(&dir));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn find_sdk_accepts_explicit_valid_root() {
    let dir = temp_dir("findsdk");
    fs::create_dir_all(dir.join("bin")).unwrap();
    fs::write(dir.join("bin/monkeyc"), "x").unwrap();
    fs::write(dir.join("bin/version.txt"), "9.9.9\n").unwrap();

    let sdk = monkey_sdk::find_sdk(Some(&dir)).expect("sdk found");
    assert_eq!(sdk.root, dir);
    assert_eq!(sdk.version.as_deref(), Some("9.9.9"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn find_sdk_rejects_invalid_explicit_root() {
    let dir = temp_dir("bad-sdk");
    assert!(monkey_sdk::find_sdk(Some(&dir)).is_none());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn find_sdk_expands_tilde() {
    // Only checks the function doesn't panic on tilde paths.
    let _ = monkey_sdk::expand_home(std::path::Path::new("~/foo"));
}

#[test]
fn devices_listing_is_stable() {
    // Depends on machine state — just verify the call doesn't panic and ids
    // are unique when devices exist.
    let devices = device::list_devices();
    let ids: std::collections::HashSet<_> = devices.iter().map(|d| &d.id).collect();
    assert_eq!(ids.len(), devices.len());
}
