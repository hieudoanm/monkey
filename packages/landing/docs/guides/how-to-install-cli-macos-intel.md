# How to install the monkey CLI on macOS (Intel)

For Intel-based Macs. The `monkey` CLI formats, lints and checks Monkey C /
Jungle code.

## Option 1: install script (recommended)

```sh
curl -fsSL https://raw.githubusercontent.com/hieudoanm/monkey/master/scripts/install.sh | bash -- --cli-only
```

The script downloads `cli-monkey-x86_64-apple-darwin` from the
[latest release](https://github.com/hieudoanm/monkey/releases/latest) and
installs `monkey` to `~/.local/bin` (add it to your `PATH` if needed).

## Option 2: manual download

1. Open the [downloads page](https://hieudoanm.github.io/monkey/downloads/) and
   download **macOS (Intel)** under **CLI** — `cli-monkey-x86_64-apple-darwin`.
2. Make it executable and move it onto your `PATH`:

```sh
chmod +x cli-monkey-x86_64-apple-darwin
mv cli-monkey-x86_64-apple-darwin ~/.local/bin/monkey
```

3. Verify:

```sh
monkey --version
```

## From source

```sh
git clone https://github.com/hieudoanm/monkey
cd monkey/crates && cargo build --release
```
