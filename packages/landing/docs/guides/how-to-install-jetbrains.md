# How to install the monkey-c plugin for JetBrains IDEs

The plugin shells out to the `monkey` CLI, so
[install the CLI](https://hieudoanm.github.io/monkey/docs/getting-started/installation/)
first (or set the binary path in settings).

## Manual download

1. Open the [downloads page](https://hieudoanm.github.io/monkey/downloads/) and
   download **JetBrains** under **Editor Extensions** —
   `extension-jetbrains-monkey-c-0.0.1.zip`.

## Installing the zip

1. Open **Settings/Preferences → Plugins**.
2. Click the **⚙** gear icon → **Install Plugin from Disk…**.
3. Pick the downloaded `extension-jetbrains-monkey-c-0.0.1.zip` and restart the
   IDE if prompted.

## Configuring the CLI path

If `monkey` is not on your `PATH`, set it under **Settings → Tools** (or the
plugin's settings page) to the absolute path of the binary — for example
`~/.local/bin/monkey`.

## Troubleshooting

- To update, repeat the download and **Install Plugin from Disk** again —
  JetBrains replaces the previous version.
