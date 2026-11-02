# gpui-fast for agents

Read `CLAUDE.md` and `docs/upstream-sync.md` before changing anything.

- `crates/` and `tooling/perf/` are pristine upstream source, byte-for-byte
  identical to `UPSTREAM.import_commit`. Do not edit them for local features.
- Performance implementations and tests live in `fast/<crate>/src/`.
  Native GPU drawing and composition live in `extensions/`; CAD has its own
  workspace crate at `extensions/cad/`. Local applications live in `apps/`.
- Upstream connection points belong in `integration/patches/`. Keep them small:
  fields, forwarding calls, module declarations, visibility changes and redirects.
  Name owned code explicitly by `crate::fast::...` or `crate::extensions::...`.
  No glob imports in owned adapters; no imports hiding hooks in upstream files.
- Sources needing GPUI private access are assembled as modules of the original
  crate. Do not expose renderer internals or add public API just to move files.
  Existing API exceptions remain recorded in `docs/upstream-sync.md`.
- `integration/overlays.json` maps owned directories into the generated build
  workspace. Never edit `target/integration/workspace/` as source.
- Use `python3 script/cargo ...` to build/test/check. Bare Cargo commands at the
  repository root are not the build entry point. The workspace manifest and
  lockfile live in `integration/workspace/`.
- Run `python3 script/check-upstream` and the appropriate assembled tests before
  committing. The check verifies pristine source AND the generated hooks.
- Changes reach `main` through pull requests from topic branches. Never commit
  directly to `main`.
- Upstream sync titles use `chore: Sync GPUI upstream <zed-commit>`; preserve
  replayed upstream messages and vendor history. Do not use `Merge` titles.
