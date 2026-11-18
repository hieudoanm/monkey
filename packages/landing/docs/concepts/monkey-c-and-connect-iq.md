# Monkey C and Connect IQ

Monkey C is the programming language for **Garmin Connect IQ**, the platform for
Garmin's wearable devices — watches, cycling computers, and more.

## The languages

- **Monkey C** (`.mc`, `.mb`, `.mcgen`) — the application language. A watched
  Monkey C app is typically an `Application` subclass plus `*View.mc` pages that
  draw to a `Graphics.Dc`.
- **Jungle** (`.jungle`) — a simple `key = value` manifest format that declares
  the app: `project_type`, `monkeyc_version`, device `permissions`, `resources`,
  `source_files`, and SDK `$(...)` lookups.

A minimal package looks like:

```text
my-app/
├── manifest.xml
├── monkey.jungle
├── resources/
└── source/
    ├── MyAppApp.mc
    └── MyAppView.mc
```

## Building and running

PRGs are built with the official Connect IQ SDK toolchain:

```bash
monkeyc -f monkey.jungle -d <device> -o bin/<package>.prg -y <developer_key> -w
monkeydo bin/<package>.prg <device>
```

The `monkey` tool in this repo wraps `monkeyc`/`monkeydo`/`connectiq` for
builds and simulator runs (`monkey build`, `monkey run`) and adds formatting,
linting, parsing, and project/SDK/device discovery on top. It orchestrates the
official SDK; it does not replace it.

## What this repo provides

- `crates/` — the `monkey` CLI for formatting, linting, parsing, and checking
  `*.mc`/`*.jungle` files, plus project/SDK/device discovery (`doctor`,
  `sdk`, `device`, `project`) and editor-friendly `--stdin`/`--check` modes.
- `packages/extensions/vscode/` and `packages/extensions/zed/` — editor support
  (highlighting, formatting, themes, icons) and optional simulator preview that
  wraps the SDK commands above.
