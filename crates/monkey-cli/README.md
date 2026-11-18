# monkey-cli

`monkey` — command-line interface for the Monkey C toolchain. Thin wrapper
over the `monkey-*` crates (`syntax`, `formatter`, `linter`, `parser`,
`project`, `sdk`, `core`).

## Build

```bash
cd crates
cargo build --release        # binary: crates/target/release/monkey
```

## Commands

```bash
monkey format [--check|--write] [--stdin] [--indent-spaces N|--indent-tabs] [paths...]
monkey lint   [paths...]
monkey check  [paths...]
monkey tokens <file>
monkey parse  [--ast] [--stdin] <file>
monkey doctor [--sdk <path>] [--key <path>] [path]
monkey sdk    list|detect|info
monkey device list|info <id>
monkey project info [path]
monkey build  [--device <id>] [--sdk <path>] [--key <path>] [path]
monkey run    [--device <id>] [--sdk <path>] [--key <path>] [--no-simulator] [path]
```

- `format` — default prints formatted source to stdout; `--check` reports
  files that would change (exit 1), `--write` rewrites in place, `--stdin`
  reads stdin → stdout (for editor integrations).
- `lint` — text + token lint rules; diagnostics as
  `path:line:col: severity: message [rule]`; exit 1 on findings.
- `check` — `lint` plus a format-drift check (superset of the old
  `scripts/lint.sh`).
- `tokens` — dumps the lexer token stream for debugging.
- `parse` — lexer/parser diagnostics (`LEX*`/`PARSE*`); `--ast` dumps the AST.
- `doctor` — verifies SDK, developer key, devices, and project setup.
- `sdk` — `list` all SDKs, `detect` the resolved one, `info` on tools/version.
- `device` — list Connect IQ device profiles or show one device.
- `project` — `info` shows the discovered project's manifest/jungle details.
- `build` — compiles `bin/<name>.prg` via the SDK's `monkeyc`.
- `run` — build + launch `connectiq` simulator + `monkeydo` deploy
  (`--no-simulator` skips the simulator).

SDK and key resolution mirrors the editor extensions: explicit flag →
`MONKEYC_SDK_PATH`/`MONKEY_SDK` and `MONKEYC_DEVELOPER_KEY`/`MONKEY_KEY` env
vars → `current-sdk.cfg` / standard Garmin paths → newest SDK under `Sdks/`.

Paths may be files or directories; directories are searched recursively for
`*.mc` and `*.jungle` (skipping `node_modules`, `target`, `.git`).

## Repo usage

```bash
cd crates && cargo build --release
./target/release/monkey check ..    # lint + format-check the whole repo
./target/release/monkey format --write source/MyApp.mc
```
