# AGENTS.md — monkey-lsp

Monkey C language server: LSP over stdio. Publishes lint diagnostics and
serves document/workspace symbols, hover (incl. Toybox API docs),
definition, references, completion, formatting, rename, signature help,
code actions, and type hierarchy.
Part of the `monkey-*` toolchain workspace.

## Conventions

- Content-Length framed JSON-RPC (standard LSP). `serve()` is the framed
  stdio loop; `Server::handle` is pure and unit-tested.
- Incremental document sync (`textDocumentSync: 2`); diagnostics computed
  by `monkey-linter` on didOpen/didChange/didSave; didClose clears.
- `ProjectIndex` (`index.rs`) scans workspace `*.mc` files for
  project-wide definition/references/rename/workspace-symbol;
  `toybox.rs` loads the SDK's `api.debug.xml` for `Toybox.*` member
  completion, return-type inference, and hover documentation.
- Positions must be LSP-correct: 0-based line, UTF-16 `character` — see
  `position_at_byte` (byte→LSP) and `intel::offset_at` (LSP→byte).
- Diagnostic severity mapping: error=1, warning=2, information=3, hint=4;
  `source: "monkey"`, `code` = rule id.
- `intel.rs` collects declarations via `monkey-parser`'s `Visitor` —
  classes, methods, fields, enums, functions, vars, params. Jungle
  documents get diagnostics only (language features return empty/null).
- The index rebuilds on any document change — fine for watchface-sized
  projects; very large trees may want incremental indexing later.
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
