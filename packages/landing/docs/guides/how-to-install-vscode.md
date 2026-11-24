# How to install the monkey-c extension for VS Code

The extension shells out to the `monkey` CLI for formatting and linting —
install the CLI first (see the CLI install guides).

## Option 1: install script (recommended)

```sh
curl -fsSL https://github.com/hieudoanm/monkey/master/scripts/install.sh | bash -- --editor-only
```

The script downloads `extension-vscode-monkey-c.vsix` from the
[latest release](https://github.com/hieudoanm/monkey/releases/latest) and runs
`code --install-extension` when the `code` CLI is available.

## Option 2: manual install from the downloads page

1. Open the [downloads page](https://hieudoanm.github.io/monkey/downloads/) and
   download **VS Code** under **Editor Extensions** —
   `extension-vscode-monkey-c.vsix`.
2. In VS Code, open the Extensions view (`Cmd+Shift+X` / `Ctrl+Shift+X`).
3. Click **… → Install from VSIX…** and pick the downloaded file.

## Verifying

Open the Command Palette (`Cmd+Shift+P`) and run an `monkey-c:` command, or look
for the extension in the Extensions view.

## Troubleshooting

- To update, re-run the install script or install the newer `.vsix` the same
  way; VS Code replaces the existing version.
