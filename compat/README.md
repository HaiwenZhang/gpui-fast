# gpui-pre compatibility crates

[GPUI Kit](https://github.com/longbridge/gpui-kit) depends on GPUI through the
`gpui-pre-*` snapshots on crates.io, and Cargo's `[patch]` only replaces a
package with one of the same name and a version that meets the requirement.
The crates here carry exactly those names and the version GPUI Kit pins, and
each re-exports this repository's crate of the same library name
(`pub use gpui_fast::*`). On this source-overlay branch, assemble the checkout
before using local compatibility patches:

```sh
python3 script/assemble
```

Point the application at the **generated** compatibility crates (replace the
checkout path below):

```toml
[patch.crates-io]
gpui-pre = { path = "/path/to/gpui-fast/target/integration/workspace/compat/gpui-pre" }
gpui-pre-platform = { path = "/path/to/gpui-fast/target/integration/workspace/compat/gpui-pre-platform" }
gpui-pre-macros = { path = "/path/to/gpui-fast/target/integration/workspace/compat/gpui-pre-macros" }
gpui-pre-sum-tree = { path = "/path/to/gpui-fast/target/integration/workspace/compat/gpui-pre-sum-tree" }
# and gpui-pre-web / gpui-pre-reqwest-client when the graph has them
```

Direct Git dependencies on this development branch cannot run the assembler.
Published releases remain self-contained and do not require Python or assembly.

Each crate exposes the same features as the gpui-pre crate it stands in for,
forwarded to ours. Nothing else lives here: gpui-fast's own crates keep
upstream's names and versions.

Two things to keep in mind:

- The version is the snapshot GPUI Kit pins, `=0.3.9` today. When GPUI Kit
  moves to a newer `gpui-pre`, move every crate here with it; until then Cargo
  warns that the patch was not used and keeps the snapshot.
- gpui-pre rewrites the `gpui::` paths its macros emit to `::gpui_kit::`, and
  gpui-fast's macros do not. A crate that depends on GPUI Kit alone aliases it
  at its root: `extern crate gpui_kit as gpui;`.
