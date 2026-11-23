# Extensions

## VS Code

`packages/extensions/vscode` provides editor support for Connect IQ files:

- Monkey C (`.mc`, `.mb`, `.mcgen`) syntax highlighting, language config, icons, and breakpoints.
- Jungle (`.jungle`) syntax highlighting and language config.
- Language server client (`monkey lsp` via `vscode-languageclient`): diagnostics,
  hover, definition, references, rename, completion, signature help — with a
  lint-only fallback when the CLI is unavailable.
- Document and range formatting for Monkey C and Jungle.
- Command-palette actions: build, run, test, export `.iq`, debug, clean,
  project info, doctor.
- Monkey C dark theme and file icon theme.

It adapts editor-facing Jungle grammar/language patterns from the official
`garmin.monkey-c` extension.

See [`extensions/vscode/README.md`](vscode/README.md) for details and packaging instructions.

## Zed

`packages/extensions/zed` provides the same editor-facing surface as the VS Code extension using Zed's Tree-sitter and task systems:

- Monkey C (`.mc`, `.mb`, `.mcgen`) and Jungle (`.jungle`) tree-sitter languages with highlighting, brackets, indentation, folds, outline, and text objects.
- External formatter (`monkey-c-format`) for Monkey C and Jungle.
- `monkey-lsp` language-server registration for diagnostics and language features.
- Monkey C dark theme and file icon theme.
- `Monkey C: Runtime Preview` / `Monkey C: Build PRG` tasks backed by `monkey-c-preview`.

Zed constraints and equivalents:

- No TextMate grammar support — Tree-sitter languages instead.
- No extension command API — formatting is an external formatter; workflows are tasks.
- The proprietary Garmin language server/debugger is not available; the
  editor-independent `monkey lsp` is used instead.

See [`extensions/zed/README.md`](zed/README.md) for installation, formatter settings, task usage, and the vendored Jungle grammar requirement.

## JetBrains

`packages/extensions/jetbrains` is an IntelliJ Platform plugin (Kotlin) for
JetBrains IDEs 2024.3+:

- Monkey C (`.mc`, `.mb`, `.mcgen`) and Jungle (`.jungle`) file types with
  hand-written lexer-based syntax highlighting.
- Lint inspections powered by `monkey lint --stdin` (works on unsaved buffers).
- Format Document via `monkey format --stdin`.
- Tools → Monkey C actions: new project, build, run, test, export `.iq`,
  debug, clean, project info, doctor.

The platform LSP client API is not available in IntelliJ IDEA Community, so
hover/completion/rename are not provided yet — diagnostics come from the CLI.

See [`extensions/jetbrains/README.md`](jetbrains/README.md) for build/run
instructions and known limitations.
