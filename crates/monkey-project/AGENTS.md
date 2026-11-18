# AGENTS.md — monkey-project

Connect IQ project model: `monkey.jungle` parsing, `manifest.xml` parsing
(tolerant mini-XML parser in `xml.rs`), and project discovery. Part of the
`monkey-*` toolchain workspace.

## Conventions

- Zero dependencies.
- `jungle.rs` — `key = value` properties, `#` comments, `project.manifest`
  selects the manifest filename (default `manifest.xml`). `base.sourcePath` /
  `base.resourcePath` may be globs; resolve to their static directory prefix.
- `xml.rs` — deliberately minimal: no validation, namespaces kept in names,
  match children via `local_name()`/`Element::child`/`descendants_named`.
- `project.rs` — `discover()` walks up at most `MAX_DEPTH` (8) parents, same
  convention as the editor extensions. A project root needs `monkey.jungle`
  plus the manifest it names.

## Validation

```bash
cd crates
cargo test -p monkey-project
cargo clippy -p monkey-project
```
