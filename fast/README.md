# Performance implementation sources

Each `<crate>/src/` directory is assembled into `crates/<crate>/src/fast/` in the
generated build workspace. Edit these canonical files, not generated copies.
Tests live alongside their implementation or under the core's `tests/` directory.

Native GPU drawing and composition are owned by `extensions/`. Their adaptations
to performance source are integration patches, so this base source can also be
assembled without native GPU support. See
[`docs/upstream-sync.md`](../docs/upstream-sync.md).
