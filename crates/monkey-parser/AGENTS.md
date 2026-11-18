# AGENTS.md — monkey-parser

Recursive-descent parser and typed AST for Monkey C. Depends on
`monkey-syntax` (tokens) and `monkey-core` (spans/diagnostics) only.

## Conventions

- `parse(src) -> ParseResult` never fails: it returns a `SourceFile` plus
  `Diagnostic`s. Unrecoverable fragments become `Item::Error`,
  `ClassMember::Error`, `Stmt::Error`, or `Expr::Missing` nodes — keep the
  parser permissive; editor integrations rely on partial trees.
- Every AST node carries a `monkey_core::Span` of byte offsets.
- Error recovery must guarantee progress: every parse loop has a
  `pos == before` fallback `bump()`, and `parse_prefix` must not consume
  closing delimiters (`)`, `]`, `}`, `,`, `;`).
- Comments are filtered out before parsing; trivia lives in the lexer.
- `Expr::Path` is produced by collapsing `a.b.c` member chains rooted at an
  identifier — keep `collapse_path` semantics stable for consumers.
- The visitor (`visit::Visitor`) uses default-method recursion; override
  only the hooks you need and replicate child visits when overriding
  container nodes.

## Validation

```bash
cd crates
cargo test -p monkey-parser
cargo clippy -p monkey-parser --all-targets -- -D warnings
../crates/target/release/monkey parse --ast <file.mc>   # debug
```
