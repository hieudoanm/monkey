# CLI reference

The `monkey` binary is a thin wrapper over the `monkey-*` crates:

- `monkey-core` — spans/diagnostics
- `monkey-syntax` — lexer
- `monkey-parser` — AST
- `monkey-formatter`
- `monkey-linter`
- `monkey-project` — jungle/manifest
- `monkey-sdk` — SDK/key/device discovery

It has no runtime dependencies beyond those crates.

```text
monkey <command> [options] [paths...]
```

Paths may be files or directories. Directories are searched recursively for
`*.mc` and `*.jungle`, skipping `node_modules`, `target`, and `.git`.

## format

```bash
monkey format [--check|--write] [--stdin] [--indent-spaces N|--indent-tabs] [paths...]
```

- Default: prints the formatted source to stdout (rustfmt-style).
- `--check` — report which files would change; exit 1 on drift.
- `--write` — rewrite files in place.
- `--stdin` — read source from stdin, write result to stdout (for editor
  integrations).
- `--indent-spaces N` / `--indent-tabs` — indentation style (default: 2 spaces).
- `--crlf` — emit CRLF line endings.

## lint

```bash
monkey lint [paths...]
monkey lint --stdin [label]   # source from stdin → JSON on stdout
monkey lint --serve           # persistent lint server (editor integrations)
```

`--stdin` behavior:

- Reads source from stdin.
- Always emits a JSON diagnostics array (SARIF with `--sarif`).
- Used by editor integrations to lint unsaved buffers.

The optional `label` (e.g. `App.mc`, `file.jungle`) selects the language;
anything not ending in `.jungle` is treated as Monkey C.

`--serve` keeps the process alive and answers repeated requests so editors
lint without spawning per keystroke.

- Frame protocol: send `<label>\n<byte-length>\n<source bytes>`.
- Receive one line containing the same JSON diagnostics array.
- EOF on stdin exits cleanly.

`lint` runs:

- Text rules
- Lexical/syntax diagnostics
- AST-backed lint rules
- `.mss` string tables and `*.xml` resource files get structural
  well-formedness checks (JSON/XML) — no format drift, the formatter is
  Monkey-C-only.

Diagnostics print as `path:line:col: severity: message [code]`; exit code is 1
on any finding.

```bash
monkey lint --json [paths...]    # JSON array of diagnostics
monkey lint --sarif [paths...]   # SARIF 2.1.0 log
```

Diagnostics can be suppressed inline with `monkey-ignore` comments:

- `// monkey-ignore` (or `# monkey-ignore` in Jungle) on the same line
- Alone on the line above — covers the next line
- Append rule codes to suppress only those: `// monkey-ignore unused-variable,PARSE001`

Rules (`monkey-linter` registry):

| Code | Severity | Category | Description |
|---|---|---|---|
| `no-trailing-whitespace` | warning | general | Trailing whitespace at end of line |
| `no-multiple-empty-lines` | warning | general | More than one consecutive blank line |
| `use-space-indent` | warning | general | Tab indentation; spaces are used instead |
| `eof-newline` | warning | general | File does not end with a newline |
| `unreachable-code` | warning | general | Statement after return/break/continue/throw in the same block |
| `unused-variable` | warning | general | Local variable declared but never referenced |
| `identical-operands` | warning | general | Both sides of a comparison are identical |
| `empty-block` | hint | general | Empty `{}` block |
| `no-request-update-in-onupdate` | error | garmin | `requestUpdate()` inside `onUpdate` causes a continuous redraw loop |

Lexical (`LEX001`–`LEX004`) and syntax (`PARSE001`) diagnostics from the
lexer/parser are also reported.

## check

```bash
monkey check [paths...]
```

`check` is `lint` plus a format-drift check — a superset of `lint`.

- Accepts `--json` and `--sarif` (format drift is reported as a
  `format-drift` diagnostic).
- Use this in CI and pre-commit hooks.

## tokens

```bash
monkey tokens <file>
```

Dumps the lexer token stream for a file. Useful for debugging syntax or
formatter edge cases.

## parse

```bash
monkey parse [--ast] [--stdin] <file>
```

`parse` behavior:

- Runs the Monkey C parser and reports lexical and syntax diagnostics
  (`LEX*`/`PARSE*` codes) with source spans.
- `--ast` dumps the parsed abstract syntax tree.
- `--stdin` reads source from stdin.
- The parser is permissive — it recovers from errors so it works on
  incomplete code.

## doctor

```bash
monkey doctor [--sdk <path>] [--key <path>] [path]
```

`doctor` checks the local toolchain setup:

- Connect IQ SDK discovery
- Developer key
- Installed device profiles
- Project discovery (`monkey.jungle` + `manifest.xml`) from `path`
  (default `.`)

Prints `ok`/`warn`/`FAIL` per check; exits 1 when the SDK or key is missing.

## sdk

```bash
monkey sdk list     # all discovered SDKs, newest first
monkey sdk detect   # resolved SDK path + version
monkey sdk info     # tools (monkeyc/monkeydo/connectiq/monkeydoc) + version
monkey sdk open <tool>   # launch an SDK tool or folder
```

`open` targets: `graph` (MonkeyGraph), `motion` (MonkeyMotion), `era`
(ERA viewer), `doc` (bundled documentation), `samples` (SDK samples folder),
`manager` (Connect IQ SDK Manager app, when installed). On macOS it uses
`open`; elsewhere it executes the tool binary directly.

SDK lookup mirrors the editor extensions — in order:

1. `--sdk` flag
2. `MONKEYC_SDK_PATH`/`MONKEY_SDK` env
3. `current-sdk.cfg`
4. Newest SDK under `Sdks/`

The same env vars apply to `doctor` and `sdk info`/`detect`.

Developer-key lookup (`doctor --key`, and later build commands), in order:

1. `--key` flag
2. `MONKEYC_DEVELOPER_KEY`/`MONKEY_KEY` env
3. `~/.config/garmin/developer_key[.der]`
4. `~/Library/Application Support/Garmin/ConnectIQ/developer_key.der`
5. Editor global-storage locations

## device

```bash
monkey device list        # id, display name, resolution
monkey device info <id>   # details for one device
```

Reads device profiles from the Connect IQ `Devices/` directory.

## project

```bash
monkey project info [path]
monkey project check [--sdk <path>] [path]
```

`info` shows the discovered project's root, jungle/manifest paths, app
type, `minApiLevel`, entry point, and jungle properties.

`check` validates `manifest.xml` against the discovered Connect IQ SDK:

- Product IDs against `Devices/` profiles
- `minApiLevel` against each device's API level
- App `type` against supported app types
- API-availability pass: resolves `Toybox.*` references in every `.mc` file
  (via `using` bindings) against each product's
  `<device>.api.debug.xml` surface and reports `[api-unavailable]` errors
  for symbols missing on a target device

Exits 1 on findings.

### Editing the manifest

```bash
monkey project products|permissions|languages|barrels <list|set|add|remove> [values...] [--path dir]
monkey project app show [path]
monkey project app set key=value... [path]
monkey project uuid [path]
```

The section commands perform surgical `manifest.xml` edits — they preserve
comments and formatting. `list` prints current values; `set` replaces;
`add`/`remove` modify. `--path` selects the project (default: discover from
the current directory).

- `products` — `<iq:product id="..."/>` entries
- `permissions` — `<iq:uses-permission id="..."/>` entries
- `languages` — `<iq:language>` codes (`eng`, `fra`, …)
- `barrels` — `<iq:depends name="..."/>` Monkey Barrel dependencies
- `app set` — application attributes: `id`, `type`, `name`, `entry`,
  `launcherIcon`, `minApiLevel`
- `uuid` — regenerate the application UUID

Sections resolve to `<iq:application>` for app manifests and `<iq:barrel>`
for Monkey Barrel manifests automatically. `<iq:annotations>` is only valid
inside `<iq:barrel>` — app manifests reject it (SDK schema-validated), so
there is no `project annotations` command for apps; edit barrel manifests'
`<iq:annotations>` section manually.

## new

```bash
monkey new <dir> [--name Name] [--type watchface|datafield|watchapp|widget|barrel] [--device id]... [--min-api x.y.z]
```

Scaffolds a minimal Connect IQ project:

- `monkey.jungle`
- `manifest.xml`
- `resources/strings/strings.xml`
- An SVG launcher icon
- A two-file app (`source/<Name>App.mc` + `source/<Name>View.mc`) using the
  `Application.AppBase` + `WatchUi.View` structure

With `--type barrel`, scaffolds a Monkey Barrel library instead:

- `monkey.jungle` + `manifest.xml` with an `<iq:barrel>` element
  (`id`, `module`, `version`, `minApiLevel`) and an empty
  `<iq:products>`/permissions section
- `source/<Name>.mc` containing an empty `module <Name> { }`

Generated `.mc`/`.jungle` files are formatter-clean, so `monkey check` passes
immediately and `monkey build` works once a device is added. Fails if the
target already contains `monkey.jungle` or `manifest.xml`.

## test

```bash
monkey test [--device <id>|all] [--sdk <path>] [--key <path>]
            [--run] [--test <name>] [--no-simulator] [path]
```

Default mode — project verification gate:

- Runs `monkey check` (lint + format drift) then `monkey build`.
- Exits non-zero on any finding or build failure.
- Intended for CI and pre-commit.

With `--run`:

- Compiles with `monkeyc -t` (unit-test mode).
- Launches the simulator.
- Executes `Toybox.Test` tests via `monkeydo -t`.
- `--test <name>` runs a single `(:test)`-annotated method.
- `--no-simulator` assumes a simulator is already running (physical devices).
- Per-test results are parsed from the RESULTS block — failures emit
  `::error::` GitHub Actions annotations and a markdown table is appended to
  `$GITHUB_STEP_SUMMARY`.
- Note: `monkeydo -t` exits non-zero even when tests pass — Monkey parses the
  summary instead.

`--device all` runs the suite once per `<iq:product>` in `manifest.xml` —
the in-process equivalent of a CI device matrix. For a real matrix (one
runner per device), pass the device via a workflow matrix instead:

```yaml
# .github/workflows/test.yml — run Toybox.Test on every manifest product
name: test
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    strategy:
      fail-fast: false
      matrix:
        device: [fr55, venu3, fenix7]
    steps:
      - uses: actions/checkout@v4
      - name: Install Monkey
        run: bash <(curl -fsSL https://raw.githubusercontent.com/hieudoanm/monkey/master/scripts/install.sh) --cli-only
      - name: SDK + key
        run: |
          # Install the Connect IQ SDK and developer key per Garmin's docs,
          # then export MONKEYC_SDK_PATH / MONKEYC_DEVELOPER_KEY.
          :
      - name: monkey test
        run: monkey test --run --device ${{ matrix.device }}
        env:
          MONKEYC_SDK_PATH: ${{ secrets.MONKEYC_SDK_PATH }}
          MONKEYC_DEVELOPER_KEY: ${{ secrets.MONKEYC_DEVELOPER_KEY }}
```

`--device all` vs a workflow matrix:

- `--device all` is simpler for local runs and small manifests.
- The matrix above parallelizes and isolates per-device failures in CI.

## build

```bash
monkey build [--device <id>] [--sdk <path>] [--key <path>] [path]
```

Compiles the discovered project with the official SDK:
`monkeyc -f <monkey.jungle> -d <device> -o bin/<name>.prg -y <key> -w`.

- Prints the `.prg` path on success.
- Streams `monkeyc` output.
- Monkey Barrel manifests (`<iq:barrel>`) route to `barrelbuild` instead —
  barrels are unsigned and device-agnostic, so no key/device is required.
  Note: `barrelbuild` expects IDE-provided `devices.json` context and may
  fail standalone; this is an SDK-side limitation.

Device selection, in order:

1. `--device` flag
2. `MONKEYC_DEVICE` env
3. First `<iq:product>` in the manifest

## run

```bash
monkey run [--device <id>] [--sdk <path>] [--key <path>] [--no-simulator] [path]
```

`run` builds, launches the Connect IQ simulator (`connectiq`), waits for it
to boot, then deploys the PRG with `monkeydo <prg> <device>` — the same flow
as the editor runtime preview.

- `--no-simulator` skips the simulator launch (useful when it's already
  running).
- `monkeydo` targets simulator device IDs; deploying to a **physical watch**
  is not supported — use Garmin Connect Mobile / Express.

## debug

```bash
monkey debug [--device <id>] [--sdk <path>] [--key <path>] [path]
```

Builds the project, then launches the SDK's Monkeybrains debugger (`mdd`)
detached with `-d <device> -e <prg> -x <prg>.debug.xml` (the debug XML is
generated by `monkeyc` during the build).

## export

```bash
monkey export [--device <id>] [--sdk <path>] [--key <path>] [path]
```

Packages the app as `bin/<name>.iq` via `monkeyc -e` — the uploadable bundle
accepted by the Garmin Connect IQ store / Express.

## clean

```bash
monkey clean [path]
```

Removes the discovered project's `bin/` build-output directory.

## mcp

```bash
monkey mcp
```

Runs an MCP (Model Context Protocol) server over stdio — newline-delimited
JSON-RPC, protocol `2024-11-05`. Agents and editors can call Monkey
toolchain tools without shelling out:

| Tool | Description |
|---|---|
| `lint_source` | Lint source text (`path` selects `.mc` vs `.jungle`) |
| `lint_file` | Lint a file on disk |
| `parse_source` | Monkey C parse/lex diagnostics |
| `format_source` | Format source text |
| `project_info` | Discover `monkey.jungle` + `manifest.xml` project |
| `sdk_info` | SDK/key/device discovery summary |
| `device_list` | Connect IQ device profiles |

Example MCP client config:

```json
{
  "mcpServers": {
    "monkey": { "command": "monkey", "args": ["mcp"] }
  }
}
```

## lsp

```bash
monkey lsp
```

Runs a language server over stdio (LSP, full document sync).

Capabilities:

- **Diagnostics** — `monkey-linter` rules, lex/parse errors, and
  `monkey-ignore` suppressions published as
  `textDocument/publishDiagnostics` on open/change/save/close.
- **`textDocument/documentSymbol`** — hierarchical outline of classes,
  methods, fields, enums, and functions.
- **`workspace/symbol`** — project-wide symbol search (Ctrl+T) over all
  indexed `*.mc` declarations.
- **`textDocument/hover`** — markdown signature/kind for the declaration
  under the cursor (params and enum members included); `Toybox.*` members
  show the SDK's `api.debug.xml` documentation (description, params,
  returns).
- **`textDocument/definition`** — jump to a declaration, across files.
- **`textDocument/completion`** — Monkey C keywords, statement snippets
  (`fn`, `class`, `if`, `for`, `while`, `try`, `var`, `method` — LSP
  snippet syntax with tab stops), and all declared names in the document
  and project.
- **`textDocument/references`** — find-references across the project,
  member-aware for `X.name` uses.
- **`textDocument/formatting`** and **`textDocument/rangeFormatting`** —
  formatting via `monkey-formatter` (returns no edit when already
  formatted; range requests get the whole-document edit).
- **`textDocument/rename`** — rename returning a `WorkspaceEdit` covering
  the declaration and all uses **across the project**; rejects invalid
  identifiers.
- **`textDocument/signatureHelp`** — parameter info inside call
  parentheses.
- **`textDocument/prepareRename`** — validates rename targets up front.
- **Context-aware completion** — after a dot:
  - `X.` completes members of `X`'s declared or inferred (`new X()`) type
    (or the enclosing class for `this.`).
  - `Toybox.`/`Toybox.Module.`/bare module paths complete from the SDK's
    `api.debug.xml` member database.
  - Plain positions offer keywords plus declarations from the whole project.

Definition, references, and rename resolve across all `*.mc` files in the
workspace (indexed from `workspaceFolders`/`rootUri`, or the opened file's
directory).

- All positions are UTF-16-correct.
- Jungle documents get diagnostics, formatting, and empty results for the
  Monkey C features.

## Lint configuration

`monkey.jungle` accepts `lint.*` keys, applied by `lint`/`check`/`test` (and
`--stdin` modes) for files inside that project:

```text
project.manifest = manifest.xml
lint.disable = unused-variable,empty-block
lint.severity.no-trailing-whitespace = error
```

- `lint.disable` — comma-separated rule ids to suppress entirely.
- `lint.severity.<rule-id>` — `error`, `warning`, `information`/`info`, or
  `hint`.

Inline `monkey-ignore` comments still apply on top of this.

## Exit codes

- `0` — clean / found.
- `1` — lint findings or format drift; also `doctor`/`sdk`/`device`/`project`
  when the SDK, key, or requested item is not found.
- `2` — usage errors.
