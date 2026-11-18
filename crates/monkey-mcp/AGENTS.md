# AGENTS.md — monkey-mcp

MCP (Model Context Protocol) server exposing Monkey toolchain tools over
stdio. Part of the `monkey-*` toolchain workspace.

## Conventions

- Newline-delimited JSON-RPC 2.0, protocol `2024-11-05` — matches what MCP
  clients (e.g. `hieudoanm`'s `harness`) expect: `initialize` →
  `notifications/initialized` → `tools/list` → `tools/call`.
- Core logic lives in `handle_message`/`call_tool` (pure, testable); `serve()`
  is only the stdio loop.
- Tool results: diagnostics tools return the raw `monkey lint --json` array
  as text content; structured results return JSON text. `isError` marks
  failures — keep messages human-readable.
- Only external dep is `serde_json` (protocol). Reuse `monkey-*` crates for
  all real work — no duplicated logic.
- `build_project`/`run_project` re-invoke this same binary via
  `std::env::current_exe()` (`monkey build`/`monkey run`) rather than
  duplicating SDK orchestration — `run` has a 120s timeout, `build` 300s.

## Validation

```bash
cd crates
cargo test -p monkey-mcp
cargo clippy -p monkey-mcp
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' | ./target/debug/monkey mcp
```
