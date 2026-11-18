# Editor extensions

Both extensions share the same formatter rules as the `monkey` CLI, so what you
see in the editor matches `monkey format`.

## VS Code

[`packages/extensions/vscode`](https://github.com/hieudoanm/monkey/tree/master/packages/extensions/vscode)
provides:

- Syntax highlighting and language config for `.mc`, `.mb`, `.mcgen`, and
  `.jungle`.
- Document formatting via **Format Document** / **Monkey C: Format Document**.
- Lint diagnostics — the extension runs `monkey lint --stdin` on
  `.mc`/`.mb`/`.mcgen`/`.jungle` buffers (including unsaved changes) and shows
  parse errors and lint findings inline. Controlled by `monkeyC.cliPath` and
  `monkeyC.lintEnabled`; requires the `monkey` CLI on `PATH`.
- A dark color theme and file icon theme.
- Breakpoints contribution for Monkey C.
- **Monkey C: Runtime Preview (Simulator)** — builds the nearest Connect IQ
  package and launches it in the simulator.

Install the `.vsix` from the
[latest release](https://github.com/hieudoanm/monkey/releases/tag/latest), or
see [Install](../getting-started/install.md#vscode).

### Settings

| Setting                    | Purpose                                                   |
| -------------------------- | --------------------------------------------------------- |
| `monkeyC.sdkPath`          | Path to the Connect IQ SDK                                |
| `monkeyC.developerKeyPath` | Path to the developer key                                 |
| `monkeyC.previewDevice`    | Device id for build/run (default `fr55`)                  |
| `monkeyC.cliPath`          | Path to the `monkey` CLI used for lint (default `monkey`) |
| `monkeyC.lintEnabled`      | Enable/disable inline lint diagnostics (default `true`)   |

## Zed

[`packages/extensions/zed`](https://github.com/hieudoanm/monkey/tree/master/packages/extensions/zed)
provides the same surface via Zed's Tree-sitter and task systems:

- Monkey C/Jungle highlighting, brackets, indentation, folds, outline, and text
  objects.
- External formatter (`monkey-c-format`) for both languages — enable it under
  **Settings → Languages → Monkey C/Jungle → Formatter**.
- Dark theme and file icon theme.
- **Monkey C: Runtime Preview** and **Monkey C: Build PRG** tasks.
- **Monkey C: Lint File** and **Monkey C: Check Project** tasks — these call
  the `monkey` CLI, which must be on `PATH`.

Install as a dev extension — see [Install](../getting-started/install.md#zed).
`./scripts/install.sh` symlinks `monkey-c-format` and `monkey-c-preview` into
`~/.local/bin` so the tasks can find them on `PATH`.

### Formatter settings

| Setting                     | Purpose                         |
| --------------------------- | ------------------------------- |
| `monkey-c-format` on `PATH` | Formatter binary used by Zed    |
| `MONKEYC_DEVICE`            | Device id (default `fr55`)      |
| `MONKEYC_SDK_PATH` / flags  | SDK and developer-key discovery |

See the
[Zed extension README](https://github.com/hieudoanm/monkey/tree/master/packages/extensions/zed)
for the full list.
