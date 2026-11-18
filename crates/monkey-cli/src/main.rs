//! `monkey` — Monkey C toolchain CLI.
//!
//! Commands:
//!   monkey format [--check|--write] [--stdin] [--indent-spaces N|--indent-tabs] [paths...]
//!   monkey lint   [paths...]
//!   monkey check  [paths...]
//!   monkey tokens <file>
//!   monkey parse  [--ast] [--stdin] <file>
//!   monkey doctor [--sdk <path>] [--key <path>] [path]
//!   monkey sdk    list|detect|info [--sdk <path>]
//!   monkey device list|info <id>
//!   monkey project info [path]
//!   monkey build  [--device <id>] [--sdk <path>] [--key <path>] [path]
//!   monkey run    [--device <id>] [--sdk <path>] [--key <path>] [--no-simulator] [path]
//!
//! Paths may be files or directories; directories are searched recursively
//! for `*.mc` and `*.jungle` files.

use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::ExitCode;

use monkey_core::{
    diagnostics_json_all, diagnostics_sarif, render_diagnostics, Diagnostic, FileDiagnostics,
};

mod build;
use monkey_formatter::{format_source, FormatOptions, LineEnding};
use monkey_linter::{lint_monkeyc, lint_text};

const USAGE: &str = "\
monkey — Monkey C toolchain

USAGE:
    monkey <command> [options] [paths...]

COMMANDS:
    format    Format Monkey C (.mc) and Jungle (.jungle) files
    lint      Check Monkey C files for issues
    check     Format-check + lint
    tokens    Dump the token stream for a file (debug)
    parse     Parse Monkey C and report syntax/lexical diagnostics
    doctor    Check SDK, developer key, and project setup
    sdk       Manage/connect-info: list | detect | info
    device    List Connect IQ devices or show device info
    project   Project info: info [path]
    build     Compile a Connect IQ project to bin/<name>.prg
    run       Build, start the simulator, and launch the app

FORMAT OPTIONS:
    --check            Report which files would change (default for files)
    --write            Write formatted output in place
    --stdin            Read source from stdin, write result to stdout
    --indent-spaces N  Spaces per indent level (default: 2)
    --indent-tabs      Indent with tabs

LINT/CHECK:
    Diagnostics are printed as `path:line:col: severity: message [code]`.
    --json emits a JSON array; --sarif emits a SARIF 2.1.0 log.
    `lint --stdin [label]` reads source from stdin and always emits JSON
    (SARIF with --sarif) for editor integrations.
    Exits non-zero when any diagnostic or format diff is found.

ENVIRONMENT:
    MONKEYC_SDK_PATH / MONKEY_SDK      SDK location override
    MONKEYC_DEVELOPER_KEY / MONKEY_KEY Developer key location
";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        eprint!("{}", USAGE);
        return ExitCode::from(2);
    };
    match cmd.as_str() {
        "format" => cmd_format(&args[1..]),
        "lint" => cmd_lint(&args[1..], false),
        "check" => cmd_lint(&args[1..], true),
        "tokens" => cmd_tokens(&args[1..]),
        "parse" => cmd_parse(&args[1..]),
        "doctor" => cmd_doctor(&args[1..]),
        "sdk" => cmd_sdk(&args[1..]),
        "device" => cmd_device(&args[1..]),
        "project" => cmd_project(&args[1..]),
        "build" => cmd_build(&args[1..], false),
        "run" => cmd_build(&args[1..], true),
        "-h" | "--help" | "help" => {
            print!("{}", USAGE);
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("monkey: unknown command '{}'\n", other);
            eprint!("{}", USAGE);
            ExitCode::from(2)
        }
    }
}

// ---------- shared helpers ----------

fn collect_files(paths: &[String], extensions: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    if stack.is_empty() {
        stack.push(PathBuf::from("."));
    }
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            if let Ok(entries) = fs::read_dir(&p) {
                for e in entries.flatten() {
                    let path = e.path();
                    if path.is_dir() {
                        if !path.components().any(|c| {
                            c.as_os_str() == "node_modules"
                                || c.as_os_str() == "target"
                                || c.as_os_str() == ".git"
                        }) {
                            stack.push(path);
                        }
                    } else if path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| extensions.contains(&e))
                    {
                        out.push(path);
                    }
                }
            }
        } else if p.is_file() {
            out.push(p);
        } else {
            eprintln!("monkey: no such file or directory: {}", p.display());
        }
    }
    out.sort();
    out
}

fn read_stdin() -> String {
    let mut buf = String::new();
    let _ = io::stdin().read_to_string(&mut buf);
    buf
}

fn print_diagnostics(path: &str, src: &str, diags: &[Diagnostic]) {
    print!("{}", render_diagnostics(path, src, diags));
}

fn format_options(args: &[String]) -> FormatOptions {
    let mut opts = FormatOptions::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--indent-tabs" => opts.indent_width = None,
            "--indent-spaces" => {
                if let Some(v) = args.get(i + 1).and_then(|s| s.parse::<usize>().ok()) {
                    opts.indent_width = Some(v);
                    i += 1;
                }
            }
            "--crlf" => opts.line_ending = LineEnding::Crlf,
            _ => {}
        }
        i += 1;
    }
    opts
}

fn positional(args: &[String]) -> Vec<String> {
    args.iter()
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .collect()
}

// ---------- commands ----------

fn cmd_format(args: &[String]) -> ExitCode {
    let opts = format_options(args);
    let write = args.iter().any(|a| a == "--write");
    let check = args.iter().any(|a| a == "--check");
    let use_stdin = args.iter().any(|a| a == "--stdin");

    if use_stdin {
        let src = read_stdin();
        print!("{}", format_source(&src, "stdin.mc", &opts));
        return ExitCode::SUCCESS;
    }

    let files = collect_files(&positional(args), &["mc", "jungle"]);
    if files.is_empty() {
        eprintln!("monkey: no .mc or .jungle files found");
        return ExitCode::from(2);
    }

    let mut changed = 0usize;
    let mut errors = 0usize;
    for path in &files {
        let Ok(src) = fs::read_to_string(path) else {
            eprintln!("monkey: cannot read {}", path.display());
            errors += 1;
            continue;
        };
        let formatted = format_source(&src, &path.to_string_lossy(), &opts);
        if formatted == src {
            continue;
        }
        changed += 1;
        if write {
            if fs::write(path, &formatted).is_err() {
                eprintln!("monkey: cannot write {}", path.display());
                errors += 1;
            }
        } else if check {
            println!("would reformat: {}", path.display());
        } else {
            // Default: print formatted source to stdout.
            if files.len() > 1 {
                println!("===== {} =====", path.display());
            }
            print!("{}", formatted);
        }
    }
    if check && changed > 0 {
        eprintln!("monkey: {} file(s) need formatting", changed);
    }
    let failed = if check { changed > 0 } else { errors > 0 };
    ExitCode::from(if failed { 1 } else { 0 })
}

fn cmd_lint(args: &[String], with_format_check: bool) -> ExitCode {
    let opts = format_options(args);
    let check_fmt = with_format_check || args.iter().any(|a| a == "--check");
    let json_out = args.iter().any(|a| a == "--json");
    let sarif_out = args.iter().any(|a| a == "--sarif");
    let use_stdin = args.iter().any(|a| a == "--stdin");

    // `lint --stdin [--json|--sarif] [label]` — lint source from stdin so
    // editor integrations can check unsaved buffers. The first positional
    // arg labels the pseudo-file (used for language detection + output path).
    if use_stdin {
        let label = positional(args)
            .first()
            .cloned()
            .unwrap_or_else(|| "<stdin>".to_string());
        let src = read_stdin();
        let is_monkeyc = !label.ends_with(".jungle");
        let mut diags = if is_monkeyc {
            lint_monkeyc(&src, &label)
        } else {
            let mut d = Vec::new();
            lint_text(&src, &mut d);
            d
        };
        diags.sort_by_key(|d| d.span.map(|s| s.start).unwrap_or(u32::MAX));
        let files = [FileDiagnostics {
            path: &label,
            src: &src,
            diagnostics: &diags,
        }];
        // stdin mode emits machine-readable output for editor integrations:
        // JSON by default, SARIF with --sarif.
        if sarif_out {
            print!("{}", diagnostics_sarif(&files));
        } else {
            print!("{}", diagnostics_json_all(&files));
        }
        let findings = diags.len();
        if findings > 0 {
            eprintln!("monkey: {} finding(s)", findings);
            return ExitCode::from(1);
        }
        return ExitCode::SUCCESS;
    }

    let files = collect_files(&positional(args), &["mc", "jungle"]);
    if files.is_empty() {
        eprintln!("monkey: no .mc or .jungle files found");
        return ExitCode::from(2);
    }

    // (path, src, diagnostics) per file, plus format-drift paths.
    let mut results: Vec<(String, String, Vec<Diagnostic>)> = Vec::new();
    let mut drift: Vec<String> = Vec::new();
    let mut findings = 0usize;

    for path in &files {
        let Ok(src) = fs::read_to_string(path) else {
            eprintln!("monkey: cannot read {}", path.display());
            findings += 1;
            continue;
        };
        let pstr = path.to_string_lossy().to_string();
        let is_monkeyc = matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("mc") | Some("mb") | Some("mcgen")
        );
        // lint_monkeyc already includes the text rules.
        let mut diags = if is_monkeyc {
            lint_monkeyc(&src, &pstr)
        } else {
            let mut d = Vec::new();
            lint_text(&src, &mut d);
            d
        };
        diags.sort_by_key(|d| d.span.map(|s| s.start).unwrap_or(u32::MAX));
        findings += diags.len();

        if check_fmt {
            let formatted = format_source(&src, &pstr, &opts);
            if formatted != src {
                drift.push(pstr.clone());
                findings += 1;
            }
        }
        results.push((pstr, src, diags));
    }

    if json_out || sarif_out {
        // Attach a synthesized diagnostic for each format-drifted file so
        // machine formats see it too.
        let mut owned = results;
        for p in &drift {
            if let Some((_, _, ds)) = owned.iter_mut().find(|(path, _, _)| path == p) {
                ds.push(Diagnostic::warning("format-drift", "would reformat"));
            }
        }
        let refs: Vec<FileDiagnostics> = owned
            .iter()
            .map(|(p, s, d)| FileDiagnostics {
                path: p,
                src: s,
                diagnostics: d,
            })
            .collect();
        if sarif_out {
            print!("{}", diagnostics_sarif(&refs));
        } else {
            print!("{}", diagnostics_json_all(&refs));
        }
    } else {
        for (pstr, src, diags) in &results {
            print_diagnostics(pstr, src, diags);
        }
        for p in &drift {
            println!("{}: would reformat", p);
        }
    }

    if findings > 0 {
        eprintln!("monkey: {} finding(s)", findings);
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn cmd_tokens(args: &[String]) -> ExitCode {
    let path = match args.iter().find(|a| !a.starts_with("--")) {
        Some(p) => p.clone(),
        None => {
            eprintln!("monkey: tokens requires a file path");
            return ExitCode::from(2);
        }
    };
    let src = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("monkey: cannot read {}", path);
            return ExitCode::from(2);
        }
    };
    for t in monkey_syntax::lex(&src) {
        let kind = format!("{:?}", t.kind);
        println!("{:>4}:{:<4} {:<12} {:?}", t.line, t.column, kind, t.text);
    }
    ExitCode::SUCCESS
}

/// `monkey parse [--ast] [--stdin] <file>` — run the Monkey C parser and
/// print lexical + syntax diagnostics. `--ast` dumps the AST instead.
fn cmd_parse(args: &[String]) -> ExitCode {
    let use_stdin = args.iter().any(|a| a == "--stdin");
    let show_ast = args.iter().any(|a| a == "--ast");
    let path = args.iter().find(|a| !a.starts_with("--")).cloned();

    let (src, label) = if use_stdin {
        (read_stdin(), "<stdin>".to_string())
    } else {
        let Some(p) = path else {
            eprintln!("monkey: parse requires a file path (or --stdin)");
            return ExitCode::from(2);
        };
        match fs::read_to_string(&p) {
            Ok(s) => (s, p),
            Err(_) => {
                eprintln!("monkey: cannot read {}", p);
                return ExitCode::from(2);
            }
        }
    };

    let result = monkey_parser::parse(&src);
    if show_ast {
        println!("{:#?}", result.file);
    }
    let diags = &result.diagnostics;
    if diags.is_empty() {
        println!("{}: ok", label);
        return ExitCode::SUCCESS;
    }
    print!("{}", monkey_core::render_diagnostics(&label, &src, diags));
    let has_error = diags
        .iter()
        .any(|d| d.severity == monkey_core::Severity::Error);
    ExitCode::from(if has_error { 1 } else { 0 })
}

// ---------- project / sdk / device commands ----------

/// Extract `--flag <value>` pairs; returns (flag → value) and the args with
/// both the flag and its value removed.
fn take_flags(args: &[String], flags: &[&str]) -> (Vec<(String, String)>, Vec<String>) {
    let mut found = Vec::new();
    let mut rest = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if flags.iter().any(|f| a == f) {
            if let Some(v) = args.get(i + 1) {
                found.push((a.clone(), v.clone()));
                i += 2;
                continue;
            }
        }
        if flags.iter().any(|f| a.starts_with(&format!("{}=", f))) {
            let v = a.split_once('=').map(|(_, v)| v.to_string());
            if let Some(v) = v {
                found.push((a[..a.find('=').unwrap()].to_string(), v));
                i += 1;
                continue;
            }
        }
        rest.push(a.clone());
        i += 1;
    }
    (found, rest)
}

fn flag_value<'a>(flags: &'a [(String, String)], name: &str) -> Option<&'a str> {
    flags
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

/// `monkey doctor [--sdk <path>] [--key <path>] [path]` — verify the local
/// toolchain setup: SDK, developer key, and project discovery.
fn cmd_doctor(args: &[String]) -> ExitCode {
    let (flags, rest) = take_flags(args, &["--sdk", "--key"]);
    let start = rest.first().cloned().unwrap_or_else(|| ".".to_string());
    let mut ok = true;

    // SDK
    let sdk_arg = flag_value(&flags, "--sdk").map(PathBuf::from);
    match monkey_sdk::find_sdk(sdk_arg.as_deref()) {
        Some(sdk) => {
            let ver = sdk.version.as_deref().unwrap_or("unknown version");
            let tools = ["monkeyc", "monkeydo", "connectiq"]
                .iter()
                .filter(|t| sdk.has_tool(t))
                .count();
            println!(
                "ok   sdk        {} ({}) — {}/3 tools",
                sdk.root.display(),
                ver,
                tools
            );
            if tools < 3 {
                println!(
                    "warn sdk        missing tools under {}",
                    sdk.bin().display()
                );
            }
        }
        None => {
            println!("FAIL sdk        no Connect IQ SDK found");
            println!("     install one or set --sdk / MONKEYC_SDK_PATH");
            ok = false;
        }
    }

    // Developer key
    let key_arg = flag_value(&flags, "--key").map(PathBuf::from);
    match monkey_sdk::find_developer_key(key_arg.as_deref()) {
        Some(k) => println!("ok   key        {}", k.display()),
        None => {
            println!("FAIL key        no developer key found");
            println!("     pass --key or set MONKEYC_DEVELOPER_KEY");
            ok = false;
        }
    }

    // Devices
    let devices = monkey_sdk::device::list_devices();
    if devices.is_empty() {
        println!("warn devices    none found under ConnectIQ/Devices");
    } else {
        println!("ok   devices    {} device profile(s)", devices.len());
    }

    // Project
    match monkey_project::discover_from(&start) {
        Some(p) => {
            println!("ok   project    {}", p.describe());
            println!("     root        {}", p.root.display());
            for problem in monkey_project::check_files(&p) {
                println!("warn project    {}", problem);
            }
        }
        None => {
            println!(
                "warn project    no monkey.jungle + manifest.xml found from {}",
                start
            );
        }
    }

    if ok {
        println!("doctor: ready");
        ExitCode::SUCCESS
    } else {
        println!("doctor: setup incomplete");
        ExitCode::from(1)
    }
}

/// `monkey sdk list|detect|info` — SDK discovery commands.
fn cmd_sdk(args: &[String]) -> ExitCode {
    let (flags, rest) = take_flags(args, &["--sdk"]);
    let sub = rest.first().map(String::as_str).unwrap_or("list");
    match sub {
        "list" => {
            let sdks = monkey_sdk::list_sdks();
            if sdks.is_empty() {
                println!("no SDKs found");
                return ExitCode::from(1);
            }
            for sdk in &sdks {
                let marker = if sdk.current { " (current)" } else { "" };
                let ver = sdk.version.as_deref().unwrap_or("?");
                println!("{}  v{}{}", sdk.root.display(), ver, marker);
            }
            ExitCode::SUCCESS
        }
        "detect" => {
            match monkey_sdk::find_sdk(flag_value(&flags, "--sdk").map(PathBuf::from).as_deref()) {
                Some(sdk) => {
                    println!("{}", sdk.root.display());
                    if let Some(v) = &sdk.version {
                        println!("version: {}", v);
                    }
                    ExitCode::SUCCESS
                }
                None => {
                    eprintln!("monkey: no SDK found (install one or set --sdk/MONKEYC_SDK_PATH)");
                    ExitCode::from(1)
                }
            }
        }
        "info" => {
            let arg = flag_value(&flags, "--sdk").map(PathBuf::from);
            match monkey_sdk::find_sdk(arg.as_deref()) {
                Some(sdk) => {
                    println!("root:    {}", sdk.root.display());
                    println!("version: {}", sdk.version.as_deref().unwrap_or("unknown"));
                    println!("current: {}", sdk.current);
                    for tool in ["monkeyc", "monkeydo", "connectiq", "monkeydoc"] {
                        println!(
                            "  {:<10} {}",
                            tool,
                            if sdk.has_tool(tool) { "yes" } else { "no" }
                        );
                    }
                    ExitCode::SUCCESS
                }
                None => {
                    eprintln!("monkey: no SDK found");
                    ExitCode::from(1)
                }
            }
        }
        other => {
            eprintln!(
                "monkey: unknown sdk subcommand '{}' (list|detect|info)",
                other
            );
            ExitCode::from(2)
        }
    }
}

/// `monkey device list|info <id>` — list or inspect Connect IQ devices.
fn cmd_device(args: &[String]) -> ExitCode {
    let rest: Vec<String> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .collect();
    let sub = rest.first().map(String::as_str).unwrap_or("list");
    match sub {
        "list" => {
            let devices = monkey_sdk::device::list_devices();
            if devices.is_empty() {
                println!("no devices found (expected <ConnectIQ>/Devices/*/compiler.json)");
                return ExitCode::from(1);
            }
            for d in &devices {
                let name = d.display_name.as_deref().unwrap_or("-");
                let res = d
                    .resolution
                    .map(|(w, h)| format!("{}x{}", w, h))
                    .unwrap_or_else(|| "-".to_string());
                println!("{:<16} {:<28} {}", d.id, name, res);
            }
            ExitCode::SUCCESS
        }
        "info" => {
            let Some(id) = rest.get(1) else {
                eprintln!("monkey: device info requires a device id");
                return ExitCode::from(2);
            };
            match monkey_sdk::device::find_device(id) {
                Some(d) => {
                    println!("id:          {}", d.id);
                    println!("displayName: {}", d.display_name.as_deref().unwrap_or("-"));
                    println!("family:      {}", d.family.as_deref().unwrap_or("-"));
                    if let Some((w, h)) = d.resolution {
                        println!("resolution:  {}x{}", w, h);
                    }
                    println!("dir:         {}", d.dir.display());
                    ExitCode::SUCCESS
                }
                None => {
                    eprintln!("monkey: device '{}' not found", id);
                    ExitCode::from(1)
                }
            }
        }
        other => {
            eprintln!("monkey: unknown device subcommand '{}' (list|info)", other);
            ExitCode::from(2)
        }
    }
}

/// `monkey project info [path]` — show discovered project details.
fn cmd_project(args: &[String]) -> ExitCode {
    let (flags, rest) = take_flags(args, &["--sdk", "--key"]);
    let _ = flags;
    let sub = rest.first().map(String::as_str).unwrap_or("info");
    let start = rest.get(1).cloned().unwrap_or_else(|| ".".to_string());
    match sub {
        "info" => match monkey_project::discover_from(&start) {
            Some(p) => {
                println!("root:     {}", p.root.display());
                println!("jungle:   {}", p.jungle_path.display());
                println!("manifest: {}", p.manifest_path.display());
                println!("describe: {}", p.describe());
                if let Some(app) = &p.manifest.application {
                    if let Some(t) = &app.app_type {
                        println!("type:     {}", t);
                    }
                    if let Some(m) = &app.min_api_level {
                        println!("minApi:   {}", m);
                    }
                    if let Some(e) = &app.entry {
                        println!("entry:    {}", e);
                    }
                }
                for (k, v) in p.jungle.map() {
                    println!("jungle    {} = {}", k, v);
                }
                for problem in monkey_project::check_files(&p) {
                    println!("warn      {}", problem);
                }
                ExitCode::SUCCESS
            }
            None => {
                eprintln!("monkey: no project found from {}", start);
                ExitCode::from(1)
            }
        },
        other => {
            eprintln!("monkey: unknown project subcommand '{}' (info)", other);
            ExitCode::from(2)
        }
    }
}

/// `monkey build|run [--device <id>] [--sdk <path>] [--key <path>] [--no-simulator] [path]`
fn cmd_build(args: &[String], do_run: bool) -> ExitCode {
    let (flags, rest) = take_flags(args, &["--sdk", "--key", "--device"]);
    let start = positional(&rest)
        .first()
        .cloned()
        .unwrap_or_else(|| ".".to_string());
    let sdk = flag_value(&flags, "--sdk").map(PathBuf::from);
    let key = flag_value(&flags, "--key").map(PathBuf::from);
    let device = flag_value(&flags, "--device");
    let no_simulator = args.iter().any(|a| a == "--no-simulator");
    let code = if do_run {
        build::run(&start, sdk.as_deref(), key.as_deref(), device, no_simulator)
    } else {
        build::build(&start, sdk.as_deref(), key.as_deref(), device)
    };
    ExitCode::from(code as u8)
}
