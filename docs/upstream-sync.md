# Keeping upstream sources pristine

`crates/` and `tooling/perf/` contain only upstream source. Their paths, bytes,
symlinks and executable bits match the import commit in `UPSTREAM`. Local
implementations and even the hooks connecting them are stored elsewhere.

## Ownership and build layout

| Source | Owner and purpose |
| --- | --- |
| `crates/`, `tooling/perf/` | Unmodified upstream source |
| `fast/<crate>/src/` | Retained layout, text, scrolling and renderer optimizations |
| `extensions/composition/adapters/<crate>/src/` | Native composition |
| `extensions/native-gpu/adapters/<crate>/src/` | GPU painter API and backend integration |
| `extensions/cad/` | Separate `gpui_cad` package and Metal demonstration |
| `apps/gpui_perf/`, `compat/`, `assets/` | Local applications, compatibility crates and fixtures |
| `integration/patches/` | Upstream connection points and layer-to-layer adaptations |
| `integration/workspace/` | Canonical workspace manifest and lockfile |
| `target/integration/workspace/` | Disposable, assembled Cargo workspace |

`integration/overlays.json` declares directory mappings and patch order. The base
layer combines performance code and composition. The native GPU layer is applied
on top, including its adaptations to scene replay and scrolling. Performance
source therefore does not contain CAD code or require native painter modules.

The adapters are source modules compiled inside the original GPUI/backend crates,
not independently linked plugin libraries. This retains private access and the
existing public API without exposing renderer internals or circular Cargo
dependencies. The CAD demonstration is a separate Cargo package. This version
changes source ownership, not the painter protocol or its drawing semantics.

## Building and testing

Python 3.11 or later, Git and the pinned Rust toolchain are required. The checkout
must include the vendor import recorded in `UPSTREAM`; fetch history when using
a shallow clone. Published packages are self-contained and need no Git history.

```sh
python3 script/assemble
python3 script/cargo check -p gpui --lib --locked
python3 script/cargo test -p gpui --features test-support --locked fast::
python3 script/cargo test -p gpui --features test-support --locked extensions::
python3 script/cargo run -p gpui_cad --bin cad_viewport --features gpui_cad/runtime_shaders
python3 script/check-upstream
python3 -m unittest discover -s integration -p 'test_*.py'
```

On macOS without the offline Metal compiler, enable runtime shader compilation
when testing the core/backend together:

```sh
python3 script/cargo test -p gpui -p gpui_apple --lib --features gpui/test-support,gpui_apple/runtime_shaders --locked
```

`script/cargo` assembles first, runs Cargo in the generated workspace, shares the
root `target/` build cache and copies successful Cargo lockfile updates back to
`integration/workspace/Cargo.lock`. The repository root deliberately has no Cargo
manifest: a bare Cargo invocation cannot silently build pristine, unextended GPUI.
For an editor, load `target/integration/workspace/Cargo.toml` as the Rust project;
edit the owned source files, then reassemble to update the editor's build tree.

Assembly uses temporary staging. It checks all patches before synchronizing the
build tree, preserves unchanged file mtimes, removes stale generated files, and
serializes assembly and Cargo commands. A failed patch leaves the previous build
tree intact. Direct edits to generated files stop reassembly rather than being
silently lost. After saving such edits to their canonical owned source or patch,
`python3 script/assemble --discard-generated-changes` explicitly replaces them.
Format owned files with `rustfmt`; `script/cargo fmt` supports `-- --check` only.

For a performance/composition-only workspace:

```sh
python3 script/assemble --without-native-gpu --dest target/integration/performance
CARGO_TARGET_DIR="$PWD/target" cargo check --manifest-path target/integration/performance/Cargo.toml -p gpui --lib
```

This leaves the default workspace unchanged. Disablement is a source assembly
option, not a new GPUI Cargo feature; Cargo may prune native-only entries from
the alternate workspace's generated lockfile.

## Editing hooks

New algorithms, bookkeeping, types, tests and explanations go into owned sources.
The patches contain only fields, forwarding calls, module declarations,
visibility changes and redirects. Outside owned modules, name them explicitly as
`crate::fast::...` or `crate::extensions::...`; do not hide them behind imports.
Inside owned modules, list imports explicitly; no globs.

`script/check-upstream` first requires pristine upstream source, then assembles
and checks the resulting hooks against the import. It retains the original
8-line hunk, 40-added-line and 20-removed-line budgets and the justified exceptions
in `script/upstream-allowlist`. Source modules generated under `src/fast/` and
`src/extensions/` are owned code rather than upstream hooks.

```sh
python3 script/check-upstream --pristine-only
python3 script/check-upstream --all
python3 script/check-upstream --zed ~/github/zed
```

The check does not prove public API compatibility or rendering correctness.
Run relevant tests and Clippy on the assembled workspace too. Source separation
reduces text conflicts; upstream changes to renderer semantics still need review.

### Where our API differs from upstream's

These differences apply to assembled and published crates. The pristine sources
remain unchanged.

The check cannot see a change to what upstream's public types offer, so the
few places where gpui-fast's differs from upstream's are listed here. Each is
forced by what `fast/` keeps; anything not listed here is upstream's API
unchanged, and a new entry needs as good a reason.

- `ViewElement`'s `Element::RequestLayoutState` and `PrepaintState` are
  `fast::retained::ViewLayoutState` and `ViewPrepaintState`, opaque types, in
  place of `Option<AnyElement>`. `ViewElement` is `#[doc(hidden)]`, and the
  states are only ever handed back to it by GPUI.
- `Scene` has a public `layers` field (`fast::layers::scene::SceneLayers`),
  and `gpui` exports `LayerKey`, `TileCoord`, `SceneLayers`, `LayerFrame`,
  `LayerContent` (a layer's content, in parts: one per row of a virtual
  list), `LAYER_TILE_TEXTURE_BASE`, `layer_tile_texture_id`, `layer_tile_id` and
  `decode_layer_tile`, because renderers live in other crates and read scroll
  layers from the scene. Polychrome sprites whose texture index is at or
  above `LAYER_TILE_TEXTURE_BASE` are renderer layer references, not atlas
  textures. Reserved ids from `0xF100_0000` identify rounded overflow groups
  with `tile_size = 0`; their drawable scene carries padding-edge geometry
  in a non-composited frame keyed by `u32::MAX`. The private wire format lives
  in `fast::rounded_clip_scene`; Metal, WGPU and Direct3D evaluate the clip
  stack directly without intermediate render targets.
- `crates/gpui/Cargo.toml` names this repository and sets `publish = false`.
- The workspace pins Taffy 0.14 where upstream pins 0.13. Taffy 0.13 keeps
  one cached size per kind of constraint, so a node measured under two widths
  in one layout evicts one with the other, and the clean subtrees around a
  single dirty node are laid out again on every frame; 0.14 keeps nine sizes
  per node whatever their kind. 0.14 hands a leaf's measure function its whole
  layout, so `taffy.rs` lays the tree out through
  `fast::layout::MeasuredTaffy`, which lays each leaf out around GPUI's
  measurement as 0.13 did. When upstream moves to 0.14 or later, the sync
  takes its version and drops `MeasuredTaffy`.
- Native GPU painters, ported on the `native-gpu` branch at the user's request
  from the four custom renderer commits ending at `9c2861ccaa` in
  `Pomelo-Native/references/zed`: `Window::register_gpu_painter` and
  `Window::paint_gpu`, the `GpuPainter` trait and typed handle, backend contexts,
  draw modes/capabilities/status, and hidden renderer dispatch/registration helpers.
  `PlatformWindow::register_gpu_painter` has a default unsupported implementation.
  `Scene::insert_gpu_paint`, `Scene::render_batches`, its native paint storage and
  `Primitive::FastGpuPaint` preserve drawing order and retained replay across
  renderer crates. Metal and WGPU expose painter registration methods; D3D11
  registers through its window. The backend feature flags enable native context
  types without forcing every graphics API into all consumers. Implementation,
  tests and methods live in `extensions/native-gpu/`; see [Native GPU painters](native-gpu.md).
- Window composition, the API of
  [zed#62379](https://github.com/zed-industries/zed/pull/62379), which
  upstream has not merged yet: native views drawn between a window's GPUI
  content and its overlays. `gpui` exports `WindowComposition`,
  `WindowCompositionSurface`, `CompositionSurfaceId`,
  `CompositionSurfaceKind`, `PlatformSurfaceAttachment`,
  `PlatformCompositionSurface`, `PlatformCompositionSurfaceContent`,
  `ComposedScene` and `ComposedSceneLayer`; `PlatformWindow` gains
  `draw_composed`, `enable_window_composition`, `create_native_surface` and
  `set_composition_order`, with defaults that keep a backend without
  composition working; `Scene` gains `is_empty` and `replay_balanced`, and
  `ComposedScene::layer_scene`, ours, cuts one layer's scene out for a
  renderer; `Window` gains `enable_window_composition` and
  `with_composition_surface`; `gpui_apple` exports `new_overlay_renderer`
for `gpui_macos`, `WgpuRenderer` gains `new_sharing_atlas` and
`replace_surface_sharing_context` for `gpui_linux`, and `Background` gains
`is_opaque` so `gpui_linux` cuts only opaque content out of native surfaces. It all lives in `extensions::composition`, so when
  upstream merges its version, the sync replaces ours with it. Two things
  behave differently from the pull request: a retained view drawn again from
  last frame keeps the surface switches it made, and `with_composition_surface`
  inside a scroll layer paints where the layer does. On X11, when a
  compositing manager blends ARGB windows, the GPUI content above native
  surfaces is drawn into a transparent override-redirect window over the
  window instead of being cut out of them, so shadows and translucent
  backdrops blend over native content; `X11Client::get_window` falls back to
  the window such an overlay window covers. The overlay content is drawn into
  the window too, with its opaque parts cut out, so it still shows when the
  compositor stacks the overlay window below the window (Hyprland renders
  pinned windows above all others). The `native_webview` example in
  `gpui_perf` shows composition on macOS, Windows and Linux, and
  `linux_webview` shows a WebKitGTK page under overlays on X11.

Composition input extends that existing exception: `ComposedScene::layer_hitboxes`
lets the separate platform crates use prepared GPUI hitboxes instead of inferring
input from graphics. `WindowComposition::on_input` registers a cooperating
windowless producer; the defaulted `PlatformSurfaceAttachment::input_forwarding`
reader carries that choice through the existing platform-order update. Existing
attachment implementations and platform surface struct initializers stay valid. OS-managed child views keep
their platform event loop. Capture runs first, press ownership survives through
release, and leave is sent when the pointer changes recipients. Prepaint scopes
and retained context keep input ownership with the same surface as its paint.
No application-specific timeline policy belongs here.

## Syncing with upstream

1. In a Zed checkout, choose the target commit and run
   `python3 script/import-upstream --zed <checkout> <commit>`. The existing tool
   replays upstream commits affecting the tracked directories, preserves vendor
   history and messages, and produces an upstream-only branch.
2. Merge that vendor branch into a topic branch. Our tracked upstream source is
   pristine, so local implementation files do not participate in this merge.
   Preserve a merge commit rather than squashing the vendor history. Use
   `chore: Sync GPUI upstream <zed-commit>` for our sync title.
3. Update `zed_commit` and `import_commit` in `UPSTREAM`, and adjust the tracked
   directory list for crates added or removed upstream.
4. Run `python3 script/check-upstream`. The assembler reapplies integration
   patches against the new source. Fix changed connection points in
   `integration/patches/`, and backend adaptations in their owned directories;
   do not edit `crates/`. Patches are applied without three-way fallback or
   reject files. A failed patch stops assembly.
5. Review upstream changes to replaced algorithms, especially the bounds tree,
   retained view/layout methods and custom frame paths. A patch applying cleanly
   does not prove that an upstream behavior change is incorporated into our copy.
6. Run assembly tests, the relevant performance/composition/native GPU tests,
   Clippy and release preparation. Keep the existing accessibility invariant:
   `a11y_frame_keeps_the_tree_it_sent` must continue to pass.

Published crates are built from the assembled tree and bundle the adapters
inside their packages. Consumers need no assembler. Direct Git dependencies on
this development branch cannot select an assembled workspace automatically; use
published packages, a prepared release workspace, or local paths under
`target/integration/workspace/` after assembly.
