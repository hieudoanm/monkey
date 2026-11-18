# CLI reference

The `monkey` binary is a thin wrapper over the `monkey-syntax`,
`monkey-formatter`, and `monkey-linter` crates. It has no runtime dependencies
beyond those sibling crates.

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

## lint

```bash
monkey lint [paths...]
```

Runs text and token lint rules. Diagnostics print as
`path:line:col: severity: message [rule]` and the exit code is 1 on any finding.

Current rules: `no-trailing-whitespace`, `no-multiple-empty-lines`,
`use-space-indent`, `eof-newline`, `no-request-update-in-onupdate`.

## check

```bash
monkey check [paths...]
```

`lint` plus a format-drift check — a superset of `lint`. Use this in CI and
pre-commit hooks.

## tokens

```bash
monkey tokens <file>
```

Dumps the lexer token stream for a file. Useful for debugging syntax or
formatter edge cases.

## Exit codes

- `0` — clean.
- `1` — lint findings or format drift.
- `2` — usage errors.
