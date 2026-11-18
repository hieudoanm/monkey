# AGENTS.md — monkey-sdk

Garmin Connect IQ SDK, developer-key, and device discovery. Part of the
`monkey-*` toolchain workspace.

## Conventions

- Only external dependency is `serde_json` (device `compiler.json` parsing).
- Discovery order must stay aligned with `packages/extensions/*/runtime-preview`:
  explicit path → `MONKEYC_SDK_PATH`/`MONKEY_SDK` → `current-sdk.cfg` → newest
  valid SDK under `Sdks/` (`bin/monkeyc` required). Same for keys:
  `MONKEYC_DEVELOPER_KEY`/`MONKEY_KEY` → standard Garmin locations.
- Never hardcode machine-specific SDK/key paths as defaults; discovery is
  based on `HOME`/platform env vars only.
- `device.rs` reads `<ConnectIQ>/Devices/<id>/compiler.json` — `deviceId`,
  `displayName`, `deviceFamily`, `resolution {width,height}`.

## Validation

```bash
cd crates
cargo test -p monkey-sdk
cargo clippy -p monkey-sdk
./target/debug/monkey doctor   # smoke test against the local SDK
```
