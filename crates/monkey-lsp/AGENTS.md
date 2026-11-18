# AGENTS.md — monkey-lsp

Monkey C language server: LSP over stdio. Publishes lint diagnostics and
serves document symbols, hover, definition, references, completion,
and formatting.
Part of the `monkey-*` toolchain workspace.

## Conventions

- Content-Length framed JSON-RPC (standard LSP). `serve()` is the framed
  stdio loop; `Server::handle` is pure and unit-tested.
- Full document sync only (`textDocumentSync: 1`); diagnostics computed by
  `monkey-linter` on didOpen/didChange/didSave; didClose clears.
- Positions must be LSP-correct: 0-based line, UTF-16 `character` — see
  `position_at_byte` (byte→LSP) and `intel::offset_at` (LSP→byte).
- Diagnostic severity mapping: error=1, warning=2, information=3, hint=4;
  `source: "monkey"`, `code` = rule id.
- `intel.rs` collects declarations via `monkey-parser`'s `Visitor` —
  classes, methods, fields, enums, functions, vars, params. Jungle
  documents get diagnostics only (language features return empty/null).
- Definition/references are same-file only; no cross-file index yet.
- Formatting delegates to `monkey-formatter` and returns a single
  full-document TextEdit (null when already formatted).
- Deps: `monkey-core`, `monkey-formatter`, `monkey-linter`,
  `monkey-parser`, `monkey-syntax`, `serde_json`. Keep it that way.

## Validation

```bash
cd crates
cargo test -p monkey-lsp
cargo clippy -p monkey-lsp
```
