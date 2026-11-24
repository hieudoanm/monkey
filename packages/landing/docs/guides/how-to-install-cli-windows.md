# How to install the monkey CLI on Windows

For 64-bit Windows — the script cannot fetch the Windows binary, so download it
manually (Option 2).

## Option 1: install script

The install script does not cover Windows — download the binary manually:

## Option 2: manual download

1. Open the [downloads page](https://hieudoanm.github.io/monkey/downloads/) and
   download **Windows** under **CLI** — `cli-monkey-x86_64-pc-windows-msvc.exe`.
2. Rename it to `monkey.exe` and place it in a directory on your `PATH`.
3. Verify in a new terminal:

```powershell
monkey --version
```

## From source (all platforms)

```sh
git clone https://github.com/hieudoanm/monkey
cd monkey/crates && cargo build --release
# binary at target/release/monkey
```
