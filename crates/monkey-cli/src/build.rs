//! `monkey build` / `monkey run` — Connect IQ build orchestration.
//!
//! Wraps the official SDK tools exactly like the editor extensions do:
//! `monkeyc -f <jungle> -d <device> -o bin/<name>.prg -y <key> -w`, then for
//! `run`: launch `connectiq`, wait, and `monkeydo <prg> <device>`.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use monkey_project::{discover_from, Project};
use monkey_sdk::{find_developer_key, find_sdk};

/// Resolved inputs for a build.
pub struct BuildPlan {
    pub project: Project,
    pub sdk_root: PathBuf,
    pub device: String,
    pub key: PathBuf,
    /// `bin/<project-dir-name>.prg`
    pub prg: PathBuf,
}

/// Pick the target device: explicit flag → `MONKEYC_DEVICE` env → first
/// product in the manifest.
fn pick_device(project: &Project, flag: Option<&str>) -> Result<String, String> {
    if let Some(d) = flag {
        return Ok(d.to_string());
    }
    if let Ok(d) = env::var("MONKEYC_DEVICE") {
        if !d.is_empty() {
            return Ok(d);
        }
    }
    project
        .products()
        .first()
        .cloned()
        .ok_or_else(|| "manifest declares no <iq:product> devices".to_string())
}

/// Resolve project + SDK + key + device into a build plan.
pub fn plan(
    start: &str,
    sdk_flag: Option<&Path>,
    key_flag: Option<&Path>,
    device_flag: Option<&str>,
) -> Result<BuildPlan, String> {
    let project = discover_from(start)
        .ok_or_else(|| format!("no Connect IQ project found from {}", start))?;

    let sdk = find_sdk(sdk_flag).ok_or_else(|| {
        "Connect IQ SDK not found — install it or pass --sdk / MONKEYC_SDK_PATH".to_string()
    })?;
    let key = find_developer_key(key_flag).ok_or_else(|| {
        "developer key not found — pass --key or set MONKEYC_DEVELOPER_KEY".to_string()
    })?;
    let device = pick_device(&project, device_flag)?;

    let name = project
        .root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "app".to_string());
    let prg = project.root.join("bin").join(format!("{}.prg", name));

    Ok(BuildPlan {
        project,
        sdk_root: sdk.root,
        device,
        key,
        prg,
    })
}

/// Run `monkeyc` for the plan. Streams output; returns Err on failure.
pub fn compile(plan: &BuildPlan) -> Result<(), String> {
    compile_with(plan, false)
}

/// Compile; `unit_test` passes `monkeyc -t` so `Toybox.Test` tests are
/// embedded for `monkeydo -t` execution.
fn compile_with(plan: &BuildPlan, unit_test: bool) -> Result<(), String> {
    let monkeyc = plan.sdk_root.join("bin").join("monkeyc");
    if !monkeyc.is_file() {
        return Err(format!("monkeyc not found at {}", monkeyc.display()));
    }
    let jungle = plan.project.jungle_path.clone();
    let out_dir = plan.prg.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| format!("cannot create {}: {}", out_dir.display(), e))?;

    let mut args: Vec<String> = vec![
        "-f".into(),
        jungle.to_string_lossy().into_owned(),
        "-d".into(),
        plan.device.clone(),
        "-o".into(),
        plan.prg.to_string_lossy().into_owned(),
        "-y".into(),
        plan.key.to_string_lossy().into_owned(),
        "-w".into(),
    ];
    if unit_test {
        args.push("-t".into());
    }
    let status = Command::new(&monkeyc)
        .args(&args)
        .current_dir(&plan.project.root)
        .stdin(Stdio::null())
        .status()
        .map_err(|e| format!("failed to run {}: {}", monkeyc.display(), e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("monkeyc exited with {}", status))
    }
}

/// Launch the simulator (`connectiq`) without waiting, mirroring the
/// extensions' detached spawn.
fn launch_simulator(plan: &BuildPlan) {
    let connectiq = plan.sdk_root.join("bin").join("connectiq");
    if !connectiq.is_file() {
        eprintln!("monkey: connectiq not found — skipping simulator");
        return;
    }
    match Command::new(&connectiq)
        .current_dir(&plan.project.root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(_) => {}
        Err(e) => eprintln!("monkey: could not start simulator: {}", e),
    }
}

/// Run the PRG on the device via `monkeydo`.
pub fn launch(plan: &BuildPlan) -> Result<(), String> {
    let monkeydo = plan.sdk_root.join("bin").join("monkeydo");
    if !monkeydo.is_file() {
        return Err(format!("monkeydo not found at {}", monkeydo.display()));
    }
    let status = Command::new(&monkeydo)
        .arg(&plan.prg)
        .arg(&plan.device)
        .current_dir(&plan.project.root)
        .status()
        .map_err(|e| format!("failed to run {}: {}", monkeydo.display(), e))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("monkeydo exited with {}", status))
    }
}

/// `monkey build` — compile the project to `bin/<name>.prg`.
pub fn build(start: &str, sdk: Option<&Path>, key: Option<&Path>, device: Option<&str>) -> i32 {
    let plan = match plan(start, sdk, key, device) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("monkey: {}", e);
            return 1;
        }
    };
    eprintln!(
        "monkey: building {} ({}) with SDK {}",
        plan.project.describe(),
        plan.device,
        plan.sdk_root.display()
    );
    match compile(&plan) {
        Ok(()) => {
            println!("{}", plan.prg.display());
            0
        }
        Err(e) => {
            eprintln!("monkey: {}", e);
            1
        }
    }
}

/// `monkey run` — build, start the simulator, launch the app.
pub fn run(
    start: &str,
    sdk: Option<&Path>,
    key: Option<&Path>,
    device: Option<&str>,
    no_simulator: bool,
) -> i32 {
    let plan = match plan(start, sdk, key, device) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("monkey: {}", e);
            return 1;
        }
    };
    eprintln!(
        "monkey: building {} ({}) with SDK {}",
        plan.project.describe(),
        plan.device,
        plan.sdk_root.display()
    );
    if let Err(e) = compile(&plan) {
        eprintln!("monkey: {}", e);
        return 1;
    }
    println!("built {}", plan.prg.display());
    if !no_simulator {
        launch_simulator(&plan);
        // Give the simulator time to boot before monkeydo connects.
        thread::sleep(Duration::from_millis(2500));
    }
    match launch(&plan) {
        Ok(()) => {
            eprintln!("monkey: launched on {}", plan.device);
            0
        }
        Err(e) => {
            eprintln!("monkey: {}", e);
            1
        }
    }
}

/// Deploy the PRG and run `Toybox.Test` tests via `monkeydo -t`.
/// `test_name` selects a single test; `None` runs all tests the app exposes.
pub fn test_run(
    start: &str,
    sdk: Option<&Path>,
    key: Option<&Path>,
    device: Option<&str>,
    test_name: Option<&str>,
    no_simulator: bool,
) -> i32 {
    let plan = match plan(start, sdk, key, device) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("monkey: {}", e);
            return 1;
        }
    };
    eprintln!(
        "monkey: building {} ({}) with SDK {}",
        plan.project.describe(),
        plan.device,
        plan.sdk_root.display()
    );
    if let Err(e) = compile_with(&plan, true) {
        eprintln!("monkey: {}", e);
        return 1;
    }
    println!("built {} (unit-test mode)", plan.prg.display());
    if !no_simulator {
        launch_simulator(&plan);
        thread::sleep(Duration::from_millis(2500));
    }
    let monkeydo = plan.sdk_root.join("bin").join("monkeydo");
    let mut cmd = Command::new(&monkeydo);
    cmd.arg(&plan.prg).arg(&plan.device);
    match test_name {
        Some(name) => {
            cmd.arg("-t").arg(name);
        }
        None => {
            cmd.arg("-t");
        }
    }
    // monkeydo -t exits non-zero even when all tests pass (known SDK quirk),
    // so capture output and parse the RESULTS summary instead.
    let output = match cmd.current_dir(&plan.project.root).output() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("monkey: failed to run {}: {}", monkeydo.display(), e);
            return 1;
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    print!("{}", stdout);
    eprint!("{}", stderr);

    // Parse per-test results from the RESULTS block:
    //   Test:                               Status:
    //   TestAppTests.testBasic              PASS
    let mut results: Vec<(String, String)> = Vec::new();
    let mut in_results = false;
    for line in stdout.lines() {
        let t = line.trim();
        if t == "RESULTS" || t.starts_with("Test:") {
            in_results = true;
            continue;
        }
        if in_results {
            if t.starts_with("Ran ") || t.starts_with("PASSED") || t.starts_with("FAILED") {
                break;
            }
            // "Name<spaces>STATUS" — status is the last token.
            if let Some(status) = ["PASS", "FAIL", "ERROR", "SKIP"]
                .iter()
                .find(|s| t.ends_with(**s))
            {
                let name = t[..t.len() - status.len()].trim().to_string();
                if !name.is_empty() && name != "Test:" {
                    results.push((name, status.to_string()));
                }
            }
        }
    }

    // GitHub Actions annotations + step summary for CI.
    let failures: Vec<&(String, String)> = results
        .iter()
        .filter(|(_, s)| s != "PASS" && s != "SKIP")
        .collect();
    for (name, status) in &failures {
        println!("::error::Test {} {}", name, status);
    }
    if let Ok(summary) = std::env::var("GITHUB_STEP_SUMMARY") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(summary)
        {
            let _ = writeln!(f, "### monkey test — {}\n", plan.device);
            let _ = writeln!(f, "| Test | Status |");
            let _ = writeln!(f, "|---|---|");
            for (name, status) in &results {
                let _ = writeln!(f, "| `{}` | {} |", name, status);
            }
            let _ = writeln!(f);
        }
    }

    let ran_tests = stdout.contains("RESULTS") || stdout.contains("Executing test");
    // Fallback when per-test lines couldn't be parsed: any FAILED marker.
    let unparseable_failure = results.is_empty()
        && (stdout.contains("FAILED") || stdout.lines().any(|l| l.trim_end().ends_with("FAIL")));
    if ran_tests && (!failures.is_empty() || unparseable_failure) {
        eprintln!(
            "monkey: {} test(s) failed on {}",
            failures.len(),
            plan.device
        );
        1
    } else if ran_tests {
        eprintln!(
            "monkey: {} test(s) passed on {}",
            results.len(),
            plan.device
        );
        0
    } else {
        eprintln!(
            "monkey: monkeydo produced no test output (exit {}) — is the simulator running?",
            output.status
        );
        1
    }
}
