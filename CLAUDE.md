# gpui-fast source ownership

The repository separates upstream source, performance work, feature extensions
and build integration. Read [AGENTS.md](AGENTS.md) and the full procedure in
[docs/upstream-sync.md](docs/upstream-sync.md).

- `crates/` and `tooling/perf/`: pristine upstream source, never local edits.
- `fast/<crate>/src/`: performance implementations and regression tests.
- `extensions/composition/`: native window composition and platform adapters.
- `extensions/native-gpu/`: GPU painter API, scene integration and backend adapters.
- `extensions/cad/`: independent CAD demonstration package and its shaders/tests.
- `apps/gpui_perf/`: performance showcase, benchmarks and non-CAD examples.
- `integration/patches/`: small upstream hooks, applied in declared order.
- `integration/workspace/`: canonical Cargo workspace manifest and lockfile.
- `target/integration/workspace/`: generated build workspace; never hand-edit.

Run `python3 script/cargo test ...`, not bare `cargo test` from the source root.
`python3 script/check-upstream` checks pristine source and assembled hook budgets.
No new GPUI public API; existing exceptions are documented in upstream-sync.md.
All changes reach main through a topic branch and pull request. Prefix PR titles
with the area: `fast:`, `extensions:`, `composition:` or `integration:`.
