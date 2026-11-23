# Monkey C for JetBrains IDEs

IntelliJ Platform plugin for [Monkey](https://github.com/hieudoanm/monkey) —
Monkey C/Jungle editor support for Garmin Connect IQ development. Kotlin,
built with the `org.jetbrains.intellij.platform` Gradle plugin.

## Features

- **File types** — `.mc`, `.mb`, `.mcgen` (Monkey C) and `.jungle` (Jungle),
  each with its own language and file icon.
- **Syntax highlighting** — hand-written lexers mirroring
  `crates/monkey-syntax`: keywords, numbers, strings/chars, line and block
  comments, longest-match operators, and `key = value` highlighting for
  Jungle files.
- **Lint inspections** — `monkey lint --stdin` runs on the current buffer
  (including unsaved changes) and reports diagnostics in the editor. Toggle
  under Settings → Tools → Monkey C.
- **Format Document** — `monkey format --stdin <name>`; works for both
  Monkey C and Jungle buffers.
- **Project actions** (Tools → Monkey C):
  - New Monkey C Project (`monkey new`, with type picker)
  - Build PRG, Build and Run, Test, Export .iq, Debug, Clean
  - Project Info, Doctor

## Requirements

- A JetBrains IDE 2024.3+ (IntelliJ IDEA Community or Ultimate, etc.).
- The `monkey` CLI on `PATH`, or set the path in Settings → Tools → Monkey C.
- For build/run/test/export: the official Connect IQ SDK and a developer key
  (same requirements as the `monkey` CLI itself).

## Known limitations

- **No language-server features yet.** The platform LSP client API is not
  available in IntelliJ IDEA Community, so hover, completion, go-to-definition,
  and rename are not provided. Diagnostics come from `monkey lint`; semantic
  features are planned once an LSP path is viable.
- Lint inspections run the CLI per file on the fly; very large files may see
  a short delay.
- Runtime preview (simulator launch) is not yet ported; use **Build and Run**
  or the `monkey run` CLI.

## Build & run

```bash
cd packages/extensions/jetbrains
./gradlew runIde    # launches a sandbox IDE with the plugin installed
./gradlew buildPlugin  # produces a distributable plugin zip
./gradlew test         # unit tests (lint JSON parsing, lexers)
```

The plugin targets IntelliJ IDEA Community 2024.3 (`IC-2024.3`); adjust the
platform version in `build.gradle.kts` to match your IDE.

## Layout

```text
src/main/
├── resources/META-INF/plugin.xml   # file types, highlighters, inspections, actions
└── kotlin/com/monkey/idea/
    ├── MonkeyLanguage.kt           # MonkeyC / Jungle Language singletons
    ├── MonkeyTokens.kt             # IElementType sets per language
    ├── AbstractMonkeyLexer.kt      # shared LexerBase implementation
    ├── MonkeyCLexer.kt             # Monkey C + Jungle lexers
    ├── MonkeyFileTypes.kt          # .mc/.mb/.mcgen + .jungle file types
    ├── MonkeySyntaxHighlighter.kt  # token → highlight attribute mapping
    ├── MonkeySettings*.kt          # CLI path + lint toggle
    ├── MonkeyLint.kt               # `monkey lint --stdin` inspection
    └── actions/MonkeyActions.kt    # CLI actions + format/new-project
```
