# Monkey

[![CI](https://github.com/hieudoanm/monkey/actions/workflows/ci.yaml/badge.svg?branch=master)](https://github.com/hieudoanm/monkey/actions/workflows/ci.yaml?branch=master) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**The modern, Rust-powered developer toolchain for Garmin Connect IQ and Monkey C.**

**Live demo:** <https://hieudoanm.github.io/monkey/>

Monkey provides a fast, zero-dependency CLI plus editor extensions for Monkey C
development:

- Formats, lints, parses, and checks Connect IQ projects.
- Delegates building and simulation to the official Garmin SDK.

## Install

One command installs the CLI, the VS Code extension, and the Zed extension
from the [latest release](https://github.com/hieudoanm/monkey/releases/tag/latest):

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/hieudoanm/monkey/master/scripts/install.sh)          # everything
bash <(curl -fsSL https://raw.githubusercontent.com/hieudoanm/monkey/master/scripts/install.sh) --cli-only
bash <(curl -fsSL https://raw.githubusercontent.com/hieudoanm/monkey/master/scripts/install.sh) --editor-only
bash <(curl -fsSL https://raw.githubusercontent.com/hieudoanm/monkey/master/scripts/install.sh) --zed-only
```

Or install each piece manually:

- **CLI**: download the `cli-monkey-*` binary for your platform from the
  [latest release](https://github.com/hieudoanm/monkey/releases/tag/latest) and
  put it on your `PATH` (e.g. `~/.local/bin`), or build from source:
  `cargo install --path crates/monkey-cli`.
- **VS Code**: install `extension-vscode-monkey-c.vsix` from the same release, or
  build with `pnpm run package` inside `packages/extensions/vscode`.
- **Zed**: install `extension-zed-monkey-c.tar.gz` from the same release, or add
  the extension as a dev extension (see `packages/extensions/zed/README.md`).

Verify your setup with `monkey doctor` — it checks the SDK, developer key,
device profiles, and project discovery.

## What's in this repo

- **[`crates/`](crates/)** — the Rust toolchain workspace:
  - `monkey-syntax` — UTF-8-safe lexer with tracked positions and lexical diagnostics.
  - `monkey-parser` — recursive-descent parser, typed AST, error recovery, visitor.
  - `monkey-core` — shared spans, source maps, and diagnostics.
  - `monkey-formatter` — Monkey C + Jungle formatter (parity with the VS Code extension formatter).
  - `monkey-linter` — lint rules (`no-trailing-whitespace`, `no-multiple-empty-lines`, `use-space-indent`, `eof-newline`, `no-request-update-in-onupdate`).
  - `monkey-cli` — the `monkey` binary: `format`, `lint`, `check`, `tokens`, `parse`.
- **[`packages/extensions/vscode/`](packages/extensions/vscode/)** — Monkey C/Jungle VS Code extension: syntax highlighting, document formatting, theme, icons, and a Connect IQ SDK/simulator runtime preview.
- **[`packages/extensions/zed/`](packages/extensions/zed/)** — Monkey C/Jungle Zed extension: Tree-sitter highlighting, external formatter, theme, icons, runtime-preview tasks.
- **[`packages/landing/`](packages/landing/)** — landing/docs site deployed to GitHub Pages.
- **[`scripts/`](scripts/)** — install helpers (`install.sh`, `install-local.sh`).
- **[`.github/workflows/ci.yaml`](.github/workflows/ci.yaml)** — Rust checks, extension packaging, and the rolling `latest` GitHub release.

## CLI quick start

```bash
monkey format --check .     # report files that would be reformatted
monkey format --write .     # format in place
monkey lint .               # lint diagnostics
monkey check .              # lint + format drift (CI-friendly)
monkey tokens file.mc       # dump the token stream (debug)
monkey parse file.mc        # parse and report syntax diagnostics
monkey parse --ast file.mc  # dump the AST (debug)
monkey format --stdin       # editor/pipe mode: stdin → stdout
```

See the [CLI reference](packages/landing/docs/guides/cli-reference.md) for details.

## Editor extensions

Both extensions provide:

- Syntax highlighting
- Formatting
- A runtime preview that builds and launches apps with the official Connect IQ
  SDK (`monkeyc`, `connectiq`, `monkeydo`)

Building requires a Garmin developer key and an installed SDK — Monkey does
not ship its own compiler or simulator.

## Repository layout

```text
.
├── .github/workflows/ci.yaml   # checks + rolling latest release
├── crates/                     # monkey-* Rust toolchain
│   ├── monkey-cli/             # `monkey` binary
│   ├── monkey-core/            # spans, source maps, diagnostics
│   ├── monkey-syntax/          # lexer + tokens
│   ├── monkey-parser/          # parser + AST + visitor
│   ├── monkey-formatter/       # Monkey C / Jungle formatter
│   └── monkey-linter/          # lint rules
├── packages/
│   ├── README.md               # editor extensions overview
│   ├── extensions/
│   │   ├── vscode/             # VS Code extension
│   │   └── zed/                # Zed extension
│   └── landing/                # GitHub Pages site + docs
└── scripts/                    # install helpers
```

## Development

```bash
cd crates
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p monkey-cli
./target/release/monkey check ..   # repo root: must be clean
```

See [AGENTS.md](AGENTS.md) and [CONTRIBUTING.md](CONTRIBUTING.md) for conventions.
