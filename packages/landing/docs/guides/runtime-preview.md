# Runtime preview

Both editor extensions can build a PRG with the Connect IQ SDK and launch it in
the simulator — no terminal required.

## Requirements

- The **Garmin Connect IQ SDK** (`monkeyc`, `connectiq`, `monkeydo`) — install
  from the
  [Garmin developer portal](https://developer.garmin.com/connect-iq/sdk/).
- A **developer key** for signing the PRG.
- A Connect IQ package in the workspace: `manifest.xml` + `monkey.jungle` +
  `source/`.

## VS Code

Run **Monkey C: Runtime Preview (Simulator)** from the command palette. The
extension finds the nearest package containing the active file, builds it,
starts the simulator, and launches the app with `monkeydo`.

Configure via settings or let it discover the SDK automatically:

- `monkeyC.sdkPath`
- `monkeyC.developerKeyPath`
- `monkeyC.previewDevice` (default `fr55`)

## Zed

Run **task: spawn** and choose **Monkey C: Runtime Preview** or **Monkey C:
Build PRG**. The tasks shell out to `monkey-c-preview`, which uses the same
discovery order: CLI flags → env vars → settings → standard Garmin SDK paths.

Environment variables:

| Variable                | Purpose                    |
| ----------------------- | -------------------------- |
| `MONKEYC_SDK`           | Path to the Connect IQ SDK |
| `MONKEYC_DEVELOPER_KEY` | Path to the developer key  |
| `MONKEYC_DEVICE`        | Device id (default `fr55`) |

The CLI equivalent:

```bash
monkey-c-preview --file source/MyApp.mc --device fr55
monkey-c-preview --file source/MyApp.mc --build-only
```

> Runtime preview depends on your local SDK and key; nothing is hardcoded and no
> credentials are committed.
