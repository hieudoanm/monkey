## Monkey C — latest build

Language support and tooling for Garmin Connect IQ Monkey C and Jungle:

- `monkey` — standalone CLI for `*.mc`/`*.jungle` files: `format`, `lint`,
  `check`, `parse`, `tokens`, `doctor`, `sdk`, `device`, `project`, `new`,
  `build`, and `run`, plus `mcp` and `lsp` stdio servers for agent/editor
  integrations (MCP tools for lint/parse/format/project/SDK/device; LSP
  diagnostics, document symbols, hover, definition, references,
  completion, formatting, and rename; MCP also exposes build/run tools). JSON/SARIF output for CI, `--stdin` modes, `monkey-ignore`
  suppression comments, Connect IQ SDK / developer-key / device discovery,
  and PRG builds + simulator launches via the official SDK. Prebuilt binaries for
  Linux (x86_64), macOS (Apple Silicon), and Windows (x86_64) are attached
  below.
- `extension-monkey-c.vsix` — VS Code extension with syntax highlighting,
  document formatting, inline lint diagnostics (via `monkey lint --stdin`),
  a dark theme, file icons, and a runtime-preview command.
- `extension-monkey-c-zed.zip` — Zed extension source (Tree-sitter languages,
  formatter, lint/check tasks, theme, icon theme). Install via `zed: install`
  from a local checkout or the extension registry.
- Companion Zed extension lives in the same repo.

### Install

**CLI:** download the `cli-monkey-*` binary for your platform and put it on
your `PATH` (e.g. `~/.local/bin`). `monkey doctor` verifies your SDK, developer
key, and project setup.

**VS Code:** download `extension-monkey-c.vsix` from the assets below and install via
**Extensions → Install from VSIX**. Lint diagnostics require the `monkey` CLI
on `PATH` (configurable via `monkeyC.cliPath`).

**Zed:** the extension is declarative — clone the repo and run
`./scripts/install.sh` inside `packages/extensions/zed`, or use the
`extension-monkey-c-zed.zip` asset as a source snapshot. The lint/check tasks
also need the `monkey` CLI on `PATH`.

### Build from source

```bash
cd crates && cargo build --release   # binary: crates/target/release/monkey
```

### Documentation

- [Repo README](https://github.com/hieudoanm/monkey#readme)
- [CLI reference](https://hieudoan.github.io/monkey/docs/guides/cli-reference)
- [Extension README](https://github.com/hieudoanm/monkey/tree/master/packages/extensions/vscode)
