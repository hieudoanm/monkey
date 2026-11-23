# Install

There are three pieces to install: the `monkey` CLI, the VS Code extension, and
the Zed extension. You only need the ones you use.

## CLI

Prebuilt binaries for Linux (x86_64), macOS (Apple Silicon), and Windows
(x86_64) are attached to the rolling
[`latest` GitHub release](https://github.com/hieudoanm/monkey/releases/tag/latest).

```bash
curl -fsSLO https://github.com/hieudoanm/monkey/releases/latest/download/cli-monkey-x86_64-unknown-linux-gnu
chmod +x cli-monkey-x86_64-unknown-linux-gnu
mv cli-monkey-x86_64-unknown-linux-gnu ~/.local/bin/monkey
```

Or build from source:

```bash
git clone https://github.com/hieudoanm/monkey.git
cd monkey/crates
cargo build --release -p monkey-cli   # binary: target/release/monkey
```

Verify:

```bash
monkey --help
```

## VS Code

Download `extension-vscode-monkey-c.vsix` from the
[latest release](https://github.com/hieudoanm/monkey/releases/tag/latest) and
install it:

```bash
curl -fsSLO https://github.com/hieudoanm/monkey/releases/latest/download/extension-vscode-monkey-c.vsix
code --install-extension extension-vscode-monkey-c.vsix
```

Or build from source:

```bash
cd packages/extensions/vscode
pnpm install --frozen-lockfile
pnpm exec vsce package --no-dependencies --out monkey-c.vsix
```

## Zed

The Zed extension is declarative — clone the repo and install it as a dev
extension:

```bash
git clone https://github.com/hieudoanm/monkey.git
cd monkey/packages/extensions/zed
./scripts/install.sh   # symlink monkey-c-format / monkey-c-preview into ~/.local/bin
```

Then in Zed: **Extensions → Install Dev Extension** and select
`packages/extensions/zed`.

The `extension-zed-monkey-c.zip` release asset is a source snapshot of the same
directory if you prefer not to clone.
