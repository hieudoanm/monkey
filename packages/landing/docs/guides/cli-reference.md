# CLI reference

The `monkey` binary is a thin wrapper over the `monkey-*` crates: `monkey-core`
(spans/diagnostics), `monkey-syntax` (lexer), `monkey-parser` (AST),
`monkey-formatter`, `monkey-linter`, `monkey-project` (jungle/manifest), and
`monkey-sdk` (SDK/key/device discovery). It has no runtime dependencies beyond
those crates.

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
```

`--stdin` reads source from stdin and always emits a JSON diagnostics array
(SARIF with `--sarif`) — used by editor integrations to lint unsaved buffers.
The optional `label` (e.g. `App.mc`, `file.jungle`) selects the language;
anything not ending in `.jungle` is treated as Monkey C.

Runs text rules, lexical/syntax diagnostics, and AST-backed lint rules.
Diagnostics print as `path:line:col: severity: message [code]` and the exit
code is 1 on any finding.

```bash
monkey lint --json [paths...]    # JSON array of diagnostics
monkey lint --sarif [paths...]   # SARIF 2.1.0 log
```

Diagnostics can be suppressed inline with `monkey-ignore` comments —
`// monkey-ignore` (or `# monkey-ignore` in Jungle) on the same line, or alone
on the line above to cover the next line. Append rule codes to suppress only
those: `// monkey-ignore unused-variable,PARSE001`.

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

`lint` plus a format-drift check — a superset of `lint`. Accepts `--json` and
`--sarif` as well (format drift is reported as a `format-drift` diagnostic).
Use this in CI and pre-commit hooks.

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

Runs the Monkey C parser and reports lexical and syntax diagnostics
(`LEX*`/`PARSE*` codes) with source spans. `--ast` dumps the parsed abstract
syntax tree; `--stdin` reads source from stdin. The parser is permissive —
it recovers from errors so it works on incomplete code.

## doctor

```bash
monkey doctor [--sdk <path>] [--key <path>] [path]
```

Checks the local toolchain setup: Connect IQ SDK discovery, developer key,
installed device profiles, and project discovery (`monkey.jungle` +
`manifest.xml`) from `path` (default `.`). Prints `ok`/`warn`/`FAIL` per
check; exits 1 when the SDK or key is missing.

## sdk

```bash
monkey sdk list     # all discovered SDKs, newest first
monkey sdk detect   # resolved SDK path + version
monkey sdk info     # tools (monkeyc/monkeydo/connectiq/monkeydoc) + version
```

SDK lookup mirrors the editor extensions: `--sdk` flag →
`MONKEYC_SDK_PATH`/`MONKEY_SDK` env → `current-sdk.cfg` → newest SDK under
`Sdks/`. The same env vars apply to `doctor` and `sdk info`/`detect`.

Developer-key lookup (`doctor --key`, and later build commands):
`--key` flag → `MONKEYC_DEVELOPER_KEY`/`MONKEY_KEY` env →
`~/.config/garmin/developer_key[.der]`,
`~/Library/Application Support/Garmin/ConnectIQ/developer_key.der`, and
editor global-storage locations.

## device

```bash
monkey device list        # id, display name, resolution
monkey device info <id>   # details for one device
```

Reads device profiles from the Connect IQ `Devices/` directory.

## project

```bash
monkey project info [path]
```

Shows the discovered project's root, jungle/manifest paths, app type,
`minApiLevel`, entry point, and jungle properties.

## new

```bash
monkey new <dir> [--name Name] [--type watchface|datafield|watchapp|widget] [--device id]... [--min-api x.y.z]
```

Scaffolds a minimal Connect IQ project: `monkey.jungle`, `manifest.xml`,
`resources/strings/strings.xml`, an SVG launcher icon, and a two-file app
(`source/<Name>App.mc` + `source/<Name>View.mc`) using the
`Application.AppBase` + `WatchUi.View` structure. Generated `.mc`/`.jungle`
files are formatter-clean, so `monkey check` passes immediately and
`monkey build` works once a device is added. Fails if the target already
contains `monkey.jungle` or `manifest.xml`.

## test

```bash
monkey test [--device <id>] [--sdk <path>] [--key <path>] [path]
```

Project verification gate: runs `monkey check` (lint + format drift) over
the project's files, then a full `monkey build`. Exits non-zero on any
finding or build failure — intended for CI and pre-commit. Runtime
`Toybox.Test` execution inside the simulator is not yet supported.

## build

```bash
monkey build [--device <id>] [--sdk <path>] [--key <path>] [path]
```

Compiles the discovered project with the official SDK:
`monkeyc -f <monkey.jungle> -d <device> -o bin/<name>.prg -y <key> -w`.
Prints the `.prg` path on success; streams `monkeyc` output.

Device selection: `--device` → `MONKEYC_DEVICE` env → first `<iq:product>`
in the manifest.

## run

```bash
monkey run [--device <id>] [--sdk <path>] [--key <path>] [--no-simulator] [path]
```

Builds, launches the Connect IQ simulator (`connectiq`), waits for it to boot,
then deploys the PRG with `monkeydo <prg> <device>` — the same flow as the
editor runtime preview. `--no-simulator` skips the simulator launch (useful
when it's already running or deploying to a physical device).

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

Runs a language server over stdio (LSP, full document sync). Capabilities:

- **Diagnostics** — `monkey-linter` rules, lex/parse errors, and
  `monkey-ignore` suppressions published as
  `textDocument/publishDiagnostics` on open/change/save/close.
- **`textDocument/documentSymbol`** — hierarchical outline of classes,
  methods, fields, enums, and functions.
- **`textDocument/hover`** — markdown signature/kind for the declaration
  under the cursor (params and enum members included).
- **`textDocument/definition`** — same-file jump to a declaration.
- **`textDocument/completion`** — Monkey C keywords plus all declared
  names in the document.
- **`textDocument/references`** — same-file find-references for the symbol
  under the cursor.
- **`textDocument/formatting`** — full-document formatting via
  `monkey-formatter` (returns no edit when already formatted).
- **`textDocument/rename`** — rename returning a `WorkspaceEdit` covering
  the declaration and all uses **across the project**; rejects invalid
  identifiers.
- **`textDocument/signatureHelp`** — parameter info inside call
  parentheses.
- **`textDocument/prepareRename`** — validates rename targets up front.
- **Context-aware completion** — `X.` completes members of `X`'s declared or inferred
  (`new X()`) type (or the enclosing class for `this.`); plain positions offer keywords
  plus declarations from the whole project.

Definition, references, and rename resolve across all `*.mc` files in the
workspace (indexed from `workspaceFolders`/`rootUri`, or the opened file's
directory). All positions are UTF-16-correct. Jungle documents get
diagnostics, formatting, and empty results for the Monkey C features.

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
