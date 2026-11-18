# First steps

With the CLI on your `PATH`, the fastest way to see it work is `monkey check` on
a Monkey C project:

```bash
monkey check .
```

Directories are searched recursively for `*.mc` and `*.jungle` files (skipping
`node_modules`, `target`, and `.git`). `check` runs the linter plus a
format-drift check and exits non-zero on any finding — ideal for pre-commit
hooks and CI.

To apply formatting in place:

```bash
monkey format --write .
```

To verify your Connect IQ setup (SDK, developer key, device profiles, and
project discovery):

```bash
monkey doctor .
```

See the [CLI reference](../guides/cli-reference.md) for every subcommand and
flag, and [editor setup](../guides/editor-extensions.md) to wire formatting into
VS Code or Zed.
