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
//!   monkey mcp    Run the MCP server (stdio JSON-RPC) for editor/agent tools
//!   monkey lsp    Run the language server (stdio LSP) publishing lint diagnostics
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

mod api_check;
mod build;
mod manifest_check;
mod manifest_edit;
mod scaffold;
use monkey_formatter::{format_source, FormatOptions, LineEnding};
use monkey_linter::{lint_monkeyc_config, lint_text_config, LintConfig};

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
    sdk       Manage/connect-info: list | detect | info | open <tool>
    device    List Connect IQ devices or show device info
    project   Project info/check and manifest editing:
              info|check|products|permissions|languages|barrels|app|uuid
    test      Verify a project: lint + format check + PRG build
    new       Scaffold a new Connect IQ project
    build     Compile a Connect IQ project to bin/<name>.prg
    run       Build, start the simulator, and launch the app
    debug     Build and launch the SDK debugger (mdd)
    export    Package the app as bin/<name>.iq (monkeyc -e)
    clean     Remove the project's bin/ build output
    mcp       MCP server over stdio (tools for agents/editors)
    lsp       Language server over stdio (lint diagnostics)
    version   Print version and build timestamp (--version, -V)

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
        "new" => cmd_new(&args[1..]),
        "clean" => cmd_clean(&args[1..]),
        "export" => cmd_export(&args[1..]),
        "test" => cmd_test(&args[1..]),
        "build" => cmd_build(&args[1..], false),
        "run" => cmd_build(&args[1..], true),
        "debug" => cmd_debug(&args[1..]),
        "mcp" => cmd_mcp(),
        "lsp" => cmd_lsp(),
        "-h" | "--help" | "help" => {
            print!("{}", USAGE);
            ExitCode::SUCCESS
        }
        "-V" | "--version" | "version" => {
            println!(
                "monkey {} (build {})",
                env!("CARGO_PKG_VERSION"),
                env!("MONKEY_BUILD_TS")
            );
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

/// Load `lint.*` configuration from the `monkey.jungle` of the project
/// containing `start` (or the current directory's project).
fn lint_config(start: Option<&str>) -> LintConfig {
    let project = start
        .and_then(monkey_project::discover_from)
        .or_else(|| monkey_project::discover_from("."));
    project
        .map(|p| LintConfig::from_jungle(p.jungle.map().into_iter()))
        .unwrap_or_default()
}

/// Persistent stdin lint loop for `lint --serve`.
fn lint_serve() -> ExitCode {
    use std::io::{BufRead, Write};
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    loop {
        let mut label = String::new();
        if reader.read_line(&mut label).unwrap_or(0) == 0 {
            return ExitCode::SUCCESS; // EOF
        }
        let label = label.trim_end_matches(['\n', '\r']).to_string();
        let mut len_line = String::new();
        if reader.read_line(&mut len_line).unwrap_or(0) == 0 {
            return ExitCode::SUCCESS;
        }
        let Ok(len) = len_line.trim().parse::<usize>() else {
            return ExitCode::from(2);
        };
        let mut buf = vec![0u8; len];
        if reader.read_exact(&mut buf).is_err() {
            return ExitCode::SUCCESS;
        }
        let src = String::from_utf8_lossy(&buf).into_owned();

        let config = lint_config(None);
        let is_monkeyc = !label.ends_with(".jungle");
        let mut diags = if label.ends_with(".mss") {
            monkey_lsp::data_files::check_mss(&src)
        } else if label.ends_with(".xml") {
            monkey_lsp::data_files::check_xml(&src)
        } else if is_monkeyc {
            lint_monkeyc_config(&src, &label, &config)
        } else {
            let mut d = Vec::new();
            lint_text_config(&src, &mut d, &config);
            d
        };
        diags.sort_by_key(|d| d.span.map(|s| s.start).unwrap_or(u32::MAX));
        let files = [FileDiagnostics {
            path: &label,
            src: &src,
            diagnostics: &diags,
        }];
        let json = diagnostics_json_all(&files);
        if writeln!(out, "{}", json).is_err() || out.flush().is_err() {
            return ExitCode::SUCCESS;
        }
    }
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

    let pos = positional(args);
    let files = collect_files(&pos, &["mc", "jungle"]);
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
    let serve = args.iter().any(|a| a == "--serve");

    // `lint --serve` — persistent lint server for editor integrations.
    // Frame protocol (all LF-terminated headers, then raw bytes):
    //   <label>\n<len>\n<source bytes>   →   <json array>\n
    // One JSON diagnostics array per request, same shape as `lint --stdin`.
    // Keeps the process alive so editors don't spawn-per-keystroke.
    if serve {
        return lint_serve();
    }

    // `lint --stdin [--json|--sarif] [label]` — lint source from stdin so
    // editor integrations can check unsaved buffers. The first positional
    // arg labels the pseudo-file (used for language detection + output path).
    if use_stdin {
        let label = positional(args)
            .first()
            .cloned()
            .unwrap_or_else(|| "<stdin>".to_string());
        let src = read_stdin();
        let config = lint_config(None);
        let is_monkeyc = !label.ends_with(".jungle");
        let mut diags = if label.ends_with(".mss") {
            monkey_lsp::data_files::check_mss(&src)
        } else if label.ends_with(".xml") {
            monkey_lsp::data_files::check_xml(&src)
        } else if is_monkeyc {
            lint_monkeyc_config(&src, &label, &config)
        } else {
            let mut d = Vec::new();
            lint_text_config(&src, &mut d, &config);
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

    let pos = positional(args);
    let config = lint_config(pos.first().map(String::as_str));
    let files = collect_files(&pos, &["mc", "jungle", "mss", "xml"]);
    if files.is_empty() {
        eprintln!("monkey: no .mc, .jungle, .mss, or .xml files found");
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
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        // Resource/data files: structural well-formedness only (no lint
        // rules, no format check — the formatter is Monkey-C-only).
        if ext == "mss" || ext == "xml" {
            let mut diags = if ext == "mss" {
                monkey_lsp::data_files::check_mss(&src)
            } else {
                monkey_lsp::data_files::check_xml(&src)
            };
            diags.sort_by_key(|d| d.span.map(|s| s.start).unwrap_or(u32::MAX));
            findings += diags.len();
            results.push((pstr, src, diags));
            continue;
        }
        let is_monkeyc = matches!(ext, "mc" | "mb" | "mcgen");
        // lint_monkeyc already includes the text rules.
        let mut diags = if is_monkeyc {
            lint_monkeyc_config(&src, &pstr, &config)
        } else {
            let mut d = Vec::new();
            lint_text_config(&src, &mut d, &config);
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
        "open" => {
            // `monkey sdk open <tool|samples|docs>` — launch SDK GUI tools
            // and folders, mirroring the extension's convenience commands.
            let tool = rest.get(1).map(String::as_str).unwrap_or("");
            let sdk = match monkey_sdk::find_sdk(
                flag_value(&flags, "--sdk").map(PathBuf::from).as_deref(),
            ) {
                Some(s) => s,
                None => {
                    eprintln!("monkey: no SDK found");
                    return ExitCode::from(1);
                }
            };
            cmd_sdk_open(&sdk, tool)
        }
        other => {
            eprintln!(
                "monkey: unknown sdk subcommand '{}' (list|detect|info|open)",
                other
            );
            ExitCode::from(2)
        }
    }
}

/// Launch an SDK tool or open an SDK folder in the file manager/browser.
fn cmd_sdk_open(sdk: &monkey_sdk::Sdk, tool: &str) -> ExitCode {
    use std::process::Command;
    let bin = sdk.bin();
    // tool name → executable under bin/ (or .app for GUI tools)
    let exe = match tool {
        "graph" | "monkeygraph" => bin.join("monkeygraph"),
        "motion" | "monkeymotion" => bin.join("MonkeyMotion.app"),
        "era" => bin.join("era"),
        "doc" | "docs" => sdk.root.join("doc").join("index.html"),
        "samples" => sdk.root.join("samples"),
        "manager" | "sdkmanager" => {
            // SDK Manager is a separate Garmin app; look for it next to the SDK.
            let candidate = sdk
                .root
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.join("ConnectIQ SDK Manager.app"));
            match candidate {
                Some(c) if c.exists() => c,
                _ => {
                    eprintln!("monkey: SDK Manager app not found");
                    return ExitCode::from(1);
                }
            }
        }
        "" => {
            eprintln!("monkey: sdk open requires a tool (graph|motion|era|doc|samples|manager)");
            return ExitCode::from(2);
        }
        other => {
            eprintln!(
                "monkey: unknown sdk tool '{}' (graph|motion|era|doc|samples|manager)",
                other
            );
            return ExitCode::from(2);
        }
    };
    if !exe.exists() {
        eprintln!("monkey: {} not found at {}", tool, exe.display());
        return ExitCode::from(1);
    }
    // `open` handles .app bundles, directories, and files on macOS;
    // fall back to executing the binary directly on other platforms.
    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg(&exe).status();
    #[cfg(not(target_os = "macos"))]
    let status = if exe.is_dir() {
        eprintln!(
            "monkey: cannot open directory {} on this platform",
            exe.display()
        );
        return ExitCode::from(1);
    } else {
        Command::new(&exe).status()
    };
    match status {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(s) => ExitCode::from(s.code().unwrap_or(1) as u8),
        Err(e) => {
            eprintln!("monkey: failed to launch {}: {}", exe.display(), e);
            ExitCode::from(1)
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
        "check" => match monkey_project::discover_from(&start) {
            Some(p) => {
                let src = fs::read_to_string(&p.manifest_path).unwrap_or_default();
                let mstr = p.manifest_path.to_string_lossy().to_string();
                let diags = manifest_check::validate(&p);
                // API-availability check against each product's device
                // surface (`<id>.api.debug.xml`), reported per .mc file.
                let api_groups = api_check::validate(&p);
                let api_count: usize = api_groups.iter().map(|g| g.diags.len()).sum();
                let manifest_count = diags.len();
                if diags.is_empty() && api_groups.is_empty() {
                    println!("{}: manifest OK", p.describe());
                    ExitCode::SUCCESS
                } else {
                    print_diagnostics(&mstr, &src, &diags[..manifest_count]);
                    for g in &api_groups {
                        print_diagnostics(&g.path.to_string_lossy(), &g.src, &g.diags);
                    }
                    eprintln!("monkey: {} finding(s)", manifest_count + api_count);
                    ExitCode::from(1)
                }
            }
            None => {
                eprintln!("monkey: no project found from {}", start);
                ExitCode::from(1)
            }
        },
        "products" => {
            cmd_project_edit(&rest[1..], "products", "product", Some("id"), "application")
        }
        "permissions" => cmd_project_edit(
            &rest[1..],
            "permissions",
            "uses-permission",
            Some("id"),
            "application",
        ),
        "languages" => cmd_project_edit(&rest[1..], "languages", "language", None, "application"),
        "barrels" => cmd_project_edit(
            &rest[1..],
            "barrels",
            "depends",
            Some("name"),
            "application",
        ),
        // `<iq:annotations>` is only valid inside `<iq:barrel>` (Monkey
        // Barrel manifests) — edit_section rejects it for app manifests.
        "annotations" => cmd_project_edit(&rest[1..], "annotations", "annotation", None, "barrel"),
        "app" => cmd_project_app(&rest[1..]),
        "uuid" => cmd_project_uuid(&rest[1..]),
        other => {
            eprintln!(
                "monkey: unknown project subcommand '{}' (info|check|products|permissions|languages|app|uuid|barrels|annotations)",
                other
            );
            ExitCode::from(2)
        }
    }
}

/// `monkey project <section> <op> [values...] [--path <dir>]` — manifest.xml
/// editing. Sections: products|permissions|languages|barrels|annotations.
/// Ops: list (default) | set | add | remove. The project is discovered from
/// `--path` or the current directory.
fn cmd_project_edit(
    args: &[String],
    section: &str,
    child: &str,
    attr: Option<&str>,
    parent: &str,
) -> ExitCode {
    let (flags, rest) = take_flags(args, &["--path"]);
    let path_str = flag_value(&flags, "--path")
        .map(str::to_string)
        .unwrap_or_else(|| ".".to_string());
    let op = rest.first().cloned().unwrap_or_else(|| "list".to_string());
    let values: Vec<String> = rest.get(1..).unwrap_or(&[]).to_vec();
    let Some(p) = monkey_project::discover_from(&path_str) else {
        eprintln!("monkey: no project found from {}", path_str);
        return ExitCode::from(1);
    };
    match manifest_edit::edit_section(&p.manifest_path, section, child, attr, parent, &op, &values)
    {
        Ok(msg) => {
            println!("{}", msg);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("monkey: {}", e);
            ExitCode::from(1)
        }
    }
}

/// `monkey project app show|set k=v...` and `monkey project uuid`.
fn cmd_project_app(args: &[String]) -> ExitCode {
    let rest: Vec<String> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .collect();
    let sub = rest.first().map(String::as_str).unwrap_or("show");
    let path_str = rest.get(1).cloned().unwrap_or_else(|| ".".to_string());
    let Some(p) = monkey_project::discover_from(&path_str) else {
        eprintln!("monkey: no project found from {}", path_str);
        return ExitCode::from(1);
    };
    match sub {
        "show" => match manifest_edit::application_attrs(&p.manifest_path) {
            Ok(attrs) => {
                for (k, v) in attrs {
                    println!("{:<14} {}", k, v);
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("monkey: {}", e);
                ExitCode::from(1)
            }
        },
        "set" => {
            let mut sets = Vec::new();
            for kv in &rest[2..] {
                match kv.split_once('=') {
                    Some((k, v)) => sets.push((k.to_string(), v.to_string())),
                    None => {
                        eprintln!("monkey: expected key=value, got '{}'", kv);
                        return ExitCode::from(2);
                    }
                }
            }
            if sets.is_empty() {
                eprintln!("monkey: project app set requires key=value pairs");
                return ExitCode::from(2);
            }
            match manifest_edit::edit_application(&p.manifest_path, &sets) {
                Ok(msg) => {
                    println!("{}", msg);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("monkey: {}", e);
                    ExitCode::from(1)
                }
            }
        }
        other => {
            eprintln!("monkey: unknown app subcommand '{}' (show|set)", other);
            ExitCode::from(2)
        }
    }
}

/// `monkey project uuid [path]` — regenerate the application UUID.
fn cmd_project_uuid(args: &[String]) -> ExitCode {
    let path_str = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| ".".to_string());
    let Some(p) = monkey_project::discover_from(&path_str) else {
        eprintln!("monkey: no project found from {}", path_str);
        return ExitCode::from(1);
    };
    match manifest_edit::regenerate_uuid(&p.manifest_path) {
        Ok(uuid) => {
            println!("{}", uuid);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("monkey: {}", e);
            ExitCode::from(1)
        }
    }
}

/// `monkey clean [path]` — remove the project's `bin/` build output.
fn cmd_clean(args: &[String]) -> ExitCode {
    let path_str = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| ".".to_string());
    let Some(p) = monkey_project::discover_from(&path_str) else {
        eprintln!("monkey: no project found from {}", path_str);
        return ExitCode::from(1);
    };
    let bin = p.root.join("bin");
    if !bin.exists() {
        println!("{}: nothing to clean", bin.display());
        return ExitCode::SUCCESS;
    }
    match fs::remove_dir_all(&bin) {
        Ok(()) => {
            println!("removed {}", bin.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("monkey: cannot remove {}: {}", bin.display(), e);
            ExitCode::from(1)
        }
    }
}

/// `monkey export [--sdk <path>] [--key <path>] [--device <id>] [path]` —
/// package the app as `bin/<name>.iq` via `monkeyc -e`.
fn cmd_export(args: &[String]) -> ExitCode {
    let (flags, rest) = take_flags(args, &["--sdk", "--key", "--device"]);
    let start = positional(&rest)
        .first()
        .cloned()
        .unwrap_or_else(|| ".".to_string());
    let sdk = flag_value(&flags, "--sdk").map(PathBuf::from);
    let key = flag_value(&flags, "--key").map(PathBuf::from);
    let device = flag_value(&flags, "--device");
    let code = build::export(&start, sdk.as_deref(), key.as_deref(), device);
    ExitCode::from(code as u8)
}

/// `monkey new <dir> [--name X] [--type watchface|datafield|watchapp|widget]
///   [--device id]... [--min-api x.y.z]`
fn cmd_new(args: &[String]) -> ExitCode {
    let (flags, rest) = take_flags(args, &["--name", "--type", "--device", "--min-api"]);
    let pos = positional(&rest);
    let Some(dir) = pos.first() else {
        eprintln!("monkey new: missing directory");
        eprintln!("usage: monkey new <dir> [--name Name] [--type watchface] [--device id] [--min-api 1.2.0]");
        return ExitCode::from(2);
    };
    let app_type = flag_value(&flags, "--type")
        .unwrap_or("watchface")
        .to_string();
    if !scaffold::APP_TYPES.contains(&app_type.as_str()) {
        eprintln!(
            "monkey new: unknown --type '{}' (expected: {})",
            app_type,
            scaffold::APP_TYPES.join(", ")
        );
        return ExitCode::from(2);
    }
    let devices: Vec<String> = flags
        .iter()
        .filter(|(f, _)| f == "--device")
        .map(|(_, v)| v.clone())
        .collect();
    let no_devices = devices.is_empty();
    let opts = scaffold::ScaffoldOptions {
        dir: PathBuf::from(dir),
        name: flag_value(&flags, "--name").map(str::to_string),
        app_type,
        devices,
        min_api: flag_value(&flags, "--min-api")
            .unwrap_or("1.2.0")
            .to_string(),
    };
    match scaffold::scaffold(&opts) {
        Ok(files) => {
            for f in &files {
                println!("created {}", f.display());
            }
            if no_devices {
                println!("note: manifest has no <iq:product> entries — pass --device or edit manifest.xml");
            }
            println!("next: cd {} && monkey doctor . && monkey build .", dir);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("monkey new: {}", e);
            ExitCode::from(1)
        }
    }
}

/// `monkey test [--device <id>|all] [--sdk <path>] [--key <path>]
///              [--run] [--test <name>] [--no-simulator] [path]`
///
/// Default: `monkey check` + `monkey build` verification gate (CI-friendly).
/// With `--run`: also deploys the PRG and executes `Toybox.Test` tests in the
/// simulator via `monkeydo -t` (`--test <name>` selects a single test,
/// `--device all` runs once per manifest product).
fn cmd_test(args: &[String]) -> ExitCode {
    let (flags, rest) = take_flags(args, &["--sdk", "--key", "--device", "--test"]);
    let run_tests = args.iter().any(|a| a == "--run");
    let no_simulator = args.iter().any(|a| a == "--no-simulator");
    let start = positional(&rest)
        .first()
        .cloned()
        .unwrap_or_else(|| ".".to_string());

    // Lint + format-check everything under the project root.
    let check_args: Vec<String> = vec![start.clone()];
    let check_code = cmd_lint(&check_args, true);
    if check_code != ExitCode::SUCCESS {
        return check_code;
    }

    let sdk = flag_value(&flags, "--sdk").map(PathBuf::from);
    let key = flag_value(&flags, "--key").map(PathBuf::from);
    let device = flag_value(&flags, "--device");
    let test_name = flag_value(&flags, "--test");
    // `--device all` runs once per <iq:product> in the manifest — the
    // CI-friendly equivalent of a device matrix (build gate or test run).
    if device == Some("all") {
        let project = monkey_project::discover_from(&start);
        let products: Vec<String> = project.map(|p| p.products().to_vec()).unwrap_or_default();
        if products.is_empty() {
            eprintln!("monkey: --device all needs a manifest with <iq:product> entries");
            return ExitCode::from(2);
        }
        let mut code = 0;
        for d in &products {
            eprintln!(
                "monkey: === {} on {} ===",
                if run_tests { "test" } else { "build" },
                d
            );
            let c = if run_tests {
                build::test_run(
                    &start,
                    sdk.as_deref(),
                    key.as_deref(),
                    Some(d),
                    test_name,
                    no_simulator,
                )
            } else {
                build::build(&start, sdk.as_deref(), key.as_deref(), Some(d))
            };
            if c != 0 {
                code = c;
            }
        }
        return ExitCode::from(code as u8);
    }
    let code = if run_tests {
        build::test_run(
            &start,
            sdk.as_deref(),
            key.as_deref(),
            device,
            test_name,
            no_simulator,
        )
    } else {
        build::build(&start, sdk.as_deref(), key.as_deref(), device)
    };
    ExitCode::from(code as u8)
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

/// `monkey debug [--device <id>] [--sdk <path>] [--key <path>] [path]` —
/// build then launch the SDK's Monkeybrains debugger (`mdd`).
fn cmd_debug(args: &[String]) -> ExitCode {
    let (flags, rest) = take_flags(args, &["--sdk", "--key", "--device"]);
    let start = positional(&rest)
        .first()
        .cloned()
        .unwrap_or_else(|| ".".to_string());
    let sdk = flag_value(&flags, "--sdk").map(PathBuf::from);
    let key = flag_value(&flags, "--key").map(PathBuf::from);
    let device = flag_value(&flags, "--device");
    let code = build::debug(&start, sdk.as_deref(), key.as_deref(), device);
    ExitCode::from(code as u8)
}

/// `monkey mcp` — MCP server over stdio for agent/editor integrations.
fn cmd_mcp() -> ExitCode {
    match monkey_mcp::serve() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("monkey mcp: {}", e);
            ExitCode::from(1)
        }
    }
}

/// `monkey lsp` — minimal language server publishing lint diagnostics.
fn cmd_lsp() -> ExitCode {
    match monkey_lsp::serve() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("monkey lsp: {}", e);
            ExitCode::from(1)
        }
    }
}
