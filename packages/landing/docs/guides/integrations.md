# MCP and LSP integrations

The `monkey` binary can run as a **Model Context Protocol (MCP) server** or a
minimal **language server (LSP)** — both over stdio, both dependency-free
wrappers around the same `monkey-*` crates the CLI uses.

## MCP server

```bash
monkey mcp
```

Speaks newline-delimited JSON-RPC (`2024-11-05`): `initialize`,
`notifications/initialized`, `tools/list`, `tools/call`. Point any MCP client
at it:

```json
{
  "mcpServers": {
    "monkey": { "command": "monkey", "args": ["mcp"] }
  }
}
```

Available tools:

| Tool             | What it does                                              |
| ---------------- | --------------------------------------------------------- |
| `lint_source`    | Lint source text; `path` picks `.mc` vs `.jungle` rules   |
| `lint_file`      | Lint a file on disk                                       |
| `parse_source`   | Monkey C lexical/syntax diagnostics (`LEX*`/`PARSE*`)     |
| `format_source`  | Format source text (same rules as `monkey format`)        |
| `project_info`   | Discover `monkey.jungle` + `manifest.xml` and describe it |
| `sdk_info`       | SDK, developer key, and device discovery summary          |
| `device_list`    | Connect IQ device profiles (id, name, resolution)         |
| `build_project`  | Compile a PRG via `monkey build` (SDK + key auto-detected) |
| `run_project`    | Build + deploy to the simulator via `monkey run`          |

Diagnostics come back as the same JSON array `monkey lint --json` emits.

## Language server

```bash
monkey lsp
```

Speaks LSP over stdio with full document sync. Beyond publishing lint
diagnostics (`textDocument/publishDiagnostics` — parse errors plus every
`monkey-linter` rule, honoring `monkey-ignore`), it serves:

- `textDocument/documentSymbol` — class/method/field/enum outline
- `textDocument/hover` — declaration signatures in markdown
- `textDocument/definition` — same-file go-to-definition
- `textDocument/references` — same-file find-references
- `textDocument/completion` — keywords + declared names
- `textDocument/formatting` — full-document format via `monkey-formatter`
- `textDocument/rename` — project-wide rename (declaration + all uses)
- `textDocument/signatureHelp` — call parameter info
- `textDocument/prepareRename` — rename validation
- Member completion after `X.` via declared types; project-wide symbols

All ranges use UTF-16 positions per the LSP spec.

Configure your editor to launch `monkey lsp` as the Monkey C language
server. The bundled VS Code extension does this automatically via
`vscode-languageclient` (`monkeyC.cliPath`, `monkeyC.lspEnabled`) and falls
back to a persistent `monkey lint --serve` process when the LSP can't
start. Zed: the bundled extension registers `monkey-lsp` — set
`lsp.monkey.binary` in settings.json, see the Zed extension README.

## Suppressing diagnostics

Both paths honor `monkey-ignore` comments and `lint.*` keys in
`monkey.jungle` (`lint.disable`, `lint.severity.<rule>`):

```monkeyc
var radius = 5; // monkey-ignore unused-variable
// monkey-ignore
var draft = compute(); // whole-line comment covers the next line
```

Use `monkey-ignore` alone to suppress everything on the line (or next line),
or list codes: `// monkey-ignore unused-variable,PARSE001`.
