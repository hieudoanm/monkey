# AGENTS.md

Guidance for coding agents working in this repository.

## Repository purpose

Toolchain and editor extensions for **Monkey C / Jungle**, the languages of
Garmin Connect IQ.

## Layout

- `crates/` — Rust workspace with the `monkey-*` toolchain (zero dependencies):
  - `monkey-syntax` — Monkey C lexer/token model.
  - `monkey-formatter` — Monkey C + Jungle formatter (parity with the extension formatter).
  - `monkey-linter` — lint rules (`no-trailing-whitespace`, `no-multiple-empty-lines`, `use-space-indent`, `eof-newline`, `no-request-update-in-onupdate`).
  - `monkey-cli` — `monkey` binary: `format`, `lint`, `check`, `tokens` subcommands.
- `packages/extensions/vscode/` — VS Code extension for Monkey C/Jungle (TypeScript). Has its own `AGENTS.md` and README.
- `packages/extensions/zed/` — Zed extension for Monkey C/Jungle (Tree-sitter, theme, icons, formatter CLI, runtime-preview task). Has its own `AGENTS.md` and README.
- `packages/landing/` — Next.js static-export landing page deployed to GitHub Pages. Has its own `AGENTS.md`.
- `scripts/` — repo-level helper scripts (`install.sh`, `install-local.sh`).
- `.github/workflows/ci.yaml` — tests the toolchain and extensions; on `master`/`main` publishes the rolling `latest` release with `monkey-*` CLI binaries, `monkey-c.vsix`, and `monkey-c-zed.zip`.

## Working conventions

- Keep changes focused and match existing project structure.
- The `monkey` CLI, the VS Code formatter (`src/format-monkeyc.ts`), and the Zed formatter (`formatter/format-monkeyc.mjs`) must stay behaviorally aligned — the Rust formatter in `crates/` is canonical.
- Do not commit secrets, developer keys, SDK credentials, or local machine paths as required defaults. Runtime preview must discover the SDK/key via settings, env vars, flags, and standard Garmin paths.
- `crates/target/`, `packages/extensions/vscode/out/`, `packages/extensions/vscode/dist/`, `node_modules/`, `packages/landing/out/`, and generated artifacts should not be committed.

## Monkey C toolchain (`crates/`)

```bash
cd crates && cargo build --release
./target/release/monkey check ..   # lint + format-check *.mc/*.jungle
./target/release/monkey format --write <paths>
```

- `monkey check` must pass on the repo before considering edits complete.
- Build: `cd crates && cargo build --workspace`; test: `cargo test --workspace`; binary at `crates/target/{debug,release}/monkey`.
- The formatter is conservative (indentation/brace/common spacing). Review diffs after formatting.
- The CLI has zero dependencies beyond the sibling crates; keep it that way.

## VS Code extension (`packages/extensions/vscode`)

Use pnpm in `packages/extensions/vscode`:

```bash
cd packages/extensions/vscode
pnpm install
pnpm run format:check
pnpm run lint
pnpm run typecheck
pnpm run test
pnpm run build
```

CI runs these checks for extension changes. See `packages/extensions/vscode/AGENTS.md` for TypeScript/tooling details.

## Zed extension (`packages/extensions/zed`)

No package install is required; the formatter uses Node built-ins and the grammar uses the Tree-sitter CLI via `npx`:

```bash
cd packages/extensions/zed
make test                 # node --test formatter/ + Jungle grammar corpus
./scripts/install.sh      # symlink monkey-c-format / monkey-c-preview into ~/.local/bin
```

- Zed uses Tree-sitter queries under `languages/*/` and a vendored Jungle grammar under `grammars/tree-sitter-jungle/`. Commit generated parser files after editing `grammar.js`.
- The Jungle grammar is fetched from this repository at `rev = "master"`. It must be committed and pushed before a Zed dev install or release can resolve it; pin `rev` to a commit for releases.
- `scripts/runtime-preview.mjs` mirrors `packages/extensions/vscode/src/runtime-preview.ts` and discovers the SDK/key via flags, env vars, and standard Garmin paths.
- See `packages/extensions/zed/AGENTS.md` for details.

## Landing page (`packages/landing`)

Next.js 16 static export (`output: "export"` → `out/`), Tailwind v4 + DaisyUI, deployed to GitHub Pages at `https://hieudoanm.github.io/monkey/` (`basePath`/`assetPrefix` are `/monkey` in production builds). Shared components under `src/components/molecules/` are byte-identical across `hieudoanm/*/packages/landing` repos — pass content via props, do not fork them. See `packages/landing/AGENTS.md`.

```bash
cd packages/landing
pnpm install
pnpm run dev
pnpm run build   # static export → out/
```

## CI/release

- CI triggers on changes under `crates/**`, `packages/extensions/vscode/**`, `packages/extensions/zed/**`, `packages/landing/**`, `scripts/**`, and workflow dispatch.
- On pushes to `master`/`main`, CI updates GitHub release tag `latest` with `monkey-c.vsix`, `monkey-c-zed.zip`, and `monkey-*` CLI binaries; release notes come from `RELEASE.md`.
- Be careful changing `.github/workflows/ci.yaml`: pnpm must be available before `actions/setup-node` when using `cache: pnpm`.

## Docs

- Update `README.md`, extension READMEs, or `packages/landing/docs/` when behavior/commands/structure change.
- Keep `RELEASE.md` in sync with what the `latest` release actually ships.

## Connect IQ SDK workflows

- The Connect IQ SDK (`monkeyc`, `connectiq`, `monkeydo`) and a developer key are required to build/run PRGs or use editor runtime preview. Do not claim an app build/run succeeded unless you actually ran it in an environment with the required SDK/key.
- Typical build: `monkeyc -f monkey.jungle -d <device> -o bin/<pkg>.prg -y <developer_key> -w` then `monkeydo bin/<pkg>.prg <device>`.
- The default preview device is `fr55`; users override via `monkeyC.previewDevice`, `MONKEYC_DEVICE`, or the `--device` flag.
