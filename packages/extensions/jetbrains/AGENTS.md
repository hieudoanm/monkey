# AGENTS.md — packages/extensions/jetbrains

JetBrains IDE plugin (IntelliJ Platform, Kotlin) for Monkey C/Jungle support.
Part of the `monkey-*` toolchain workspace.

## Purpose

- File types + syntax highlighting for Monkey C (`.mc`, `.mb`, `.mcgen`) and
  Jungle (`.jungle`).
- Lint diagnostics via `monkey lint --stdin` (`MonkeyLintInspection`) —
  inspects unsaved buffers, no LSP.
- Format Document via `monkey format --stdin <name>`.
- CLI actions mirroring the VS Code extension: new project, build, run,
  test, export, debug, clean, project info, doctor.

## Conventions

- Kotlin + `org.jetbrains.intellij.platform` Gradle plugin; platform is
  IntelliJ IDEA Community **2024.3** (same baseline as the mars repo's
  `extensions/jetbrains`). Java toolchain 21.
- Keep lexers in sync with `crates/monkey-syntax/src/token.rs` (keyword list,
  multi/single punctuation) — highlighting only, permissive by design.
- `MonkeyLint.parse` consumes the `diagnostics_json_all` shape from
  `crates/monkey-core/src/render.rs`; line/column are 1-based, char columns.
- CLI invocations must go through `MonkeySettings.cliPath` and never
  hardcode machine-specific SDK/key paths — the `monkey` CLI does its own
  discovery.
- Do not commit `.gradle/`, `build/`, `.intellijPlatform/`, `.idea/`.

## Commands

```bash
cd packages/extensions/jetbrains
./gradlew test          # JUnit tests (lint JSON, lexers)
./gradlew buildPlugin   # plugin zip in build/distributions/
./gradlew runIde        # sandbox IDE (downloads IC on first run)
```

## CI

`.github/workflows/ci.yaml` has an `extension-jetbrains` job
(`./gradlew buildPlugin test`) and publishes
`extension-jetbrains-monkey-c-<version>.zip` on the `latest` release.
