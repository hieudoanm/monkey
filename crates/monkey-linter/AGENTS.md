# AGENTS.md — monkey-linter

Diagnostic rules for Monkey C. Part of the `monkey-*` toolchain workspace.

## Conventions

- Depends on `monkey-core` (diagnostics), `monkey-parser` (AST), and
  `monkey-syntax` only.
- All diagnostics are `monkey_core::Diagnostic` with byte `Span`s — never
  line/column pairs.
- `lint_text` must stay language-agnostic (used for `.jungle` too); AST rules
  live in `ast.rs` and only run for Monkey C sources.
- Rule ids are stable public API and are registered in `registry::RULES` with
  description, default severity, and category (`general`/`monkeyc`/`garmin`).
  Do not rename without checking `monkey-cli` output consumers.
- `lint_monkeyc` includes text rules + lexer/parser diagnostics + AST rules;
  `lint_parsed` avoids re-parsing when the caller already has the AST.
- AST rules use `monkey_parser::visit::Visitor`; per-function state (e.g.
  unused-variable) is scoped in `AstLinter::visit_function` — Monkey C has no
  nested functions, but save/restore outer state anyway.

## Validation

```bash
cd crates
cargo test -p monkey-linter
cargo clippy -p monkey-linter
```
