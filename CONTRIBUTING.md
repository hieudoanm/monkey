# Contributing

Thanks for contributing to the Monkey toolchain repo.

## Scope

This repo contains:

- `crates/*` — the `monkey-*` Rust toolchain and the `monkey` CLI.
- `packages/extensions/vscode` — VS Code extension for Monkey C/Jungle (TypeScript).
- `packages/extensions/zed` — Zed extension for Monkey C/Jungle (Tree-sitter).
- `packages/landing` — GitHub Pages landing page (Next.js static export).
- `scripts/*` — repo-level install helpers.

## Prerequisites

- [Rust](https://rustup.rs/) (stable) for `crates/`.
- [Node.js](https://nodejs.org/) and [pnpm](https://pnpm.io/) for the extensions and landing page.
- [Garmin Connect IQ SDK](https://developer.garmin.com/connect-iq/sdk/) and a developer key only if you need runtime preview or PRG builds.

## Setup

```bash
git clone https://github.com/hieudoanm/monkey.git
cd monkey
pnpm install        # workspace deps for extensions + landing
```

## Toolchain (`crates/`)

```bash
cd crates
cargo build --workspace
cargo test --workspace
./target/release/monkey check ..   # lint + format-check the repo
```

Keep `monkey check ..` clean before opening a PR. The formatter is
conservative; review diffs after `monkey format --write`.

## VS Code extension (`packages/extensions/vscode`)

```bash
cd packages/extensions/vscode
pnpm install
pnpm run format:check
pnpm run lint
pnpm run typecheck
pnpm run test
pnpm run build
```

When changing `src/format-monkeyc.ts`, keep behavior aligned with the Rust
formatter and the Zed formatter, and update the Vitest tests.

## Zed extension (`packages/extensions/zed`)

```bash
cd packages/extensions/zed
make test
```

Commit regenerated Tree-sitter parser files after editing
`grammars/tree-sitter-jungle/grammar.js`. See `packages/extensions/zed/AGENTS.md`.

## Landing page (`packages/landing`)

```bash
cd packages/landing
pnpm install
pnpm run lint
pnpm run typecheck
pnpm run build    # static export → out/
```

## Releases

CI publishes a rolling `latest` GitHub release on pushes to `master` with the
CLI binaries, VSIX, and Zed zip. Release notes come from `RELEASE.md` — update
it when you change what ships.

## License

By contributing, you agree that your contributions are licensed under the
[MIT License](LICENSE).
