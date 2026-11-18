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
monkey sdk    list|detect|info|open <graph|motion|era|doc|samples|manager>
monkey device list|info <id>
monkey project info|check [path]
monkey project products|permissions|languages|barrels <list|set|add|remove> [values...] [--path dir]
monkey project app show|set k=v... [path]
monkey project uuid [path]                     # regenerate application UUID
monkey test   [--device <id>|all] [--run] [--test name] [path]  # check+build; --run executes Toybox.Test (--device all = manifest matrix)
monkey new    <dir> [--name X] [--type watchface|barrel] [--device id] [--min-api x.y.z]
monkey build  [--device <id>] [--sdk <path>] [--key <path>] [path]   # barrel manifests route to barrelbuild
monkey run    [--device <id>] [--sdk <path>] [--key <path>] [--no-simulator] [path]
monkey debug  [--device <id>] [--sdk <path>] [--key <path>] [path]   # build + launch mdd debugger
monkey export [--device <id>] [--sdk <path>] [--key <path>] [path]   # bin/<name>.iq via monkeyc -e
monkey clean  [path]                           # remove bin/ build output
monkey mcp                          # MCP server (stdio JSON-RPC)
monkey lsp                          # language server (diagnostics, symbols, hover, definition, references, completion, formatting, rename)
```

- `format` — default prints formatted source to stdout; `--check` reports
  files that would change (exit 1), `--write` rewrites in place, `--stdin`
  reads stdin → stdout (for editor integrations).
- `lint` — text + token lint rules; diagnostics as
  `path:line:col: severity: message [rule]`; exit 1 on findings. Also covers
  `.mss` (JSON) and `*.xml` resource files with structural well-formedness
  checks.
- `check` — `lint` plus a format-drift check (superset of the old
  `scripts/lint.sh`).
- `tokens` — dumps the lexer token stream for debugging.
- `parse` — lexer/parser diagnostics (`LEX*`/`PARSE*`); `--ast` dumps the AST.
- `doctor` — verifies SDK, developer key, devices, and project setup.
- `sdk` — `list` all SDKs, `detect` the resolved one, `info` on tools/version,
  `open` launches SDK GUI tools (`graph`, `motion`, `era`, `doc`, `samples`,
  `manager`).
- `device` — list Connect IQ device profiles or show one device.
- `project` — `info` shows the discovered project's manifest/jungle details;
  `check` validates the manifest (products, permissions, languages, min API
  level) and API availability per device. The section commands (`products`,
  `permissions`, `languages`, `barrels`) do surgical `manifest.xml` edits
  (`list`/`set`/`add`/`remove`, `--path` selects the project); `app show|set`
  edits application attributes; `uuid` regenerates the application id.
  `<iq:annotations>` is only editable in Monkey Barrel manifests.
- `build` — compiles `bin/<name>.prg` via the SDK's `monkeyc` (Monkey Barrel
  manifests route to `barrelbuild`; barrels need no key/device).
- `run` — build + launch `connectiq` simulator + `monkeydo` deploy
  (`--no-simulator` skips the simulator).
- `debug` — build + launch the SDK's Monkeybrains debugger (`mdd`) with the
  generated `.prg.debug.xml`.
- `export` — package the app as `bin/<name>.iq` via `monkeyc -e` (the
  uploadable bundle).
- `clean` — remove the project's `bin/` output directory.
- `mcp` — MCP server exposing lint/parse/format/project/sdk/device tools.
- `lsp` — language server: lint diagnostics plus document symbols, hover,
  definition, references, completion, signature help, formatting (incl. range),
  rename, document highlight, folding ranges, inlay hints, semantic tokens,
  call/type hierarchy, and code actions. Also publishes `.mss`/resource-XML
  well-formedness diagnostics.

SDK and key resolution mirrors the editor extensions:

1. Explicit flag
2. `MONKEYC_SDK_PATH`/`MONKEY_SDK` and `MONKEYC_DEVELOPER_KEY`/`MONKEY_KEY`
   env vars
3. `current-sdk.cfg` / standard Garmin paths
4. Newest SDK under `Sdks/`

Paths may be files or directories; directories are searched recursively for
`*.mc` and `*.jungle` (skipping `node_modules`, `target`, `.git`).

## Repo usage

```bash
cd crates && cargo build --release
./target/release/monkey check ..    # lint + format-check the whole repo
./target/release/monkey format --write source/MyApp.mc
```

## Known limitations

- **Physical-device deployment** is not supported: `monkeydo` targets
  simulator device IDs; deploying to a physical watch requires Garmin Connect
  Mobile / Express (the SDK exposes no standalone CLI for it).
- **Monkey Barrel builds** delegate to the SDK's `barrelbuild`, which expects
  IDE-provided `devices.json` context and currently fails standalone — the
  command is wired correctly but depends on Garmin's environment.
- **`monkey debug`** launches `mdd` (Monkeybrains) detached; breakpoint-level
  DAP integration is future work.
- **`project annotations`** edits `<iq:annotations>` inside `<iq:barrel>`
  manifests only — app manifests reject the element (SDK schema-validated).
- **`sdk open`** is macOS-oriented (`open`); other platforms fall back to
  executing the tool binary directly.
- **Manifest editing** is text-surgical: it preserves comments/formatting but
  does not reformat the XML.
- **`.mss`/XML checks** are well-formedness only — no schema validation of
  resource contents.
- **Product categories** (`Set Products by Category`) are not implemented —
  use `project products add` with explicit ids, or `monkey device list` to
  browse.
- **`monkey new`** does not scaffold Monkey Barrel projects.
- **Zed** gets diagnostics via `monkey lint --serve`; full LSP features in
  Zed depend on the declarative `[language_servers]` registration being
  picked up by the installed Zed version.

## Submodules

- `manifest_edit.rs` — surgical `manifest.xml` editing (sections, app attrs,
  UUID regeneration) used by `monkey project`.
- `api_check.rs` — per-device API-availability diagnostics for `project check`.
- `manifest_check.rs` — manifest validation rules.
- `scaffold.rs` — `monkey new` project templates.
- `build.rs` — build/run/test/debug/export orchestration over SDK tools.

`data_files` (in `monkey-lsp`) provides the `.mss`/`.xml` validators shared
with the CLI.

## License

MIT — see the workspace [`LICENSE`](../../LICENSE).
