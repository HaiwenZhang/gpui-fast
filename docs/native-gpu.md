# Native GPU painters

Implement `gpui::GpuPainter`, register it with `Window::register_gpu_painter`,
then call
`Window::paint_gpu(bounds, &handle, snapshot)` during an element's paint phase.
The handle belongs to one window. The scene retains the immutable snapshot and
draws the surface in order with ordinary UI primitives, including cached scene
replays.

`GpuPainter` is the common interface for Metal, wgpu and D3D11. Set its
`Snapshot` associated type to your immutable application data and implement
`supports_backend` for the native backends your painter supports. Registration
returns `GpuPainterHandle<P::Snapshot>`; `paint_gpu` accepts an `Arc` of that same
type, so mismatched snapshots are rejected at compile time. No backend adapter
function is needed:

```rust
let painter = window.register_gpu_painter(CadPainter::default())?;
window.paint_gpu(bounds, &painter, Arc::new(snapshot))?;
```

All callbacks use contexts from `gpui`: `GpuPrepareContext`, `GpuPassContext`
and `GpuPaintContext`. Their `target()` and `capabilities()` methods expose common
geometry and access information. Match their backend variants, or use the fallible
`metal()`, `wgpu()` and `d3d11()` accessors to borrow native resources. Shared-pass
contexts provide Metal and wgpu access only. Backend crates enable the corresponding
`gpui/gpu-metal`, `gpui/gpu-wgpu` or `gpui/gpu-d3d11` feature. A backend is a graphics
API, not an OS: wgpu remains `GpuBackend::Wgpu` when it uses Metal internally.
Applications can keep native drawing code and shaders in platform modules while
implementing one public painter trait.

The common lifecycle has these stages:

| Stage | Execution | Typical work |
| --- | --- | --- |
| `prepare` | Before ordinary scene drawing, once per visible surface | Upload geometry, allocate targets, run compute or offscreen passes |
| `finish_prepare` | After all surfaces have prepared, once per successfully prepared surface | Work that depends on preparation of other surfaces |
| `draw_mode` | After the surface finishes preparation | Select shared or independent drawing access |
| `draw_in_pass` / `paint` | At the surface's position in the scene | Draw or composite with the selected access |

Preparation methods default to success without doing any work. A preparation
error skips the surface's remaining stages; errors are logged and available through
`GpuPainterHandle::status()`. Its `encoded` counter counts successful drawing
callbacks, not preparation, mode selection or GPU completion.

`draw_mode` receives the current `GpuCapabilities`. Check
`capabilities.supports(GpuDrawMode::SharedPass)` before choosing shared access.
Metal and wgpu support this mode; D3D11 supports `OwnPass` only. An unsupported
mode skips the surface and records an error instead of silently falling back.
Painters can select `GpuDrawMode::SharedPass` from `draw_mode`
and implement `draw_in_pass`. The context borrows the current Metal render encoder
or wgpu render pass, together with the device, target geometry and color format.
GPUI sets the viewport to the complete surface bounds and an integer scissor
inside the inherited clip; `GpuPaintTarget::scissor_rect()` exposes that conversion.
Apply `target.opacity` when writing color. Shared drawing must preserve existing
color, match the current target format and sample count, and leave the pass open.
It cannot replace attachments or encode other passes. GPUI restores the state
needed by ordinary UI drawing after success or error; an error does not fall back
to independent drawing or undo encoded commands.

The default `GpuDrawMode::OwnPass` invokes `paint`. Metal and wgpu end
the UI pass before giving the painter independent encoding access. Shared-only
painters can omit `paint`; choosing a mode without implementing its callback
returns an error. Surfaces of both modes execute strictly in scene order rather
than being regrouped by mode. Mode selection runs again for every encoding
attempt, including scene replays. D3D11 continues using its state-isolated native
context through `paint`.

For direct rendering, use the provided command encoder/buffer or D3D11 context
to create your own pass targeting the window's color attachment. Preserve its
existing contents, obey `target.clip`, and apply `target.opacity`. An
application-owned depth attachment must match the color target's full dimensions
and `target.sample_count`. The pipeline must also match the target format and
sample count. GPUI resumes ordinary UI rendering after your callback.

For an offscreen CAD viewport, own the color, depth, multisample and any picking
attachments in your painter. Render them during preparation, resolve multisampled
color if needed, and composite the result with `draw_in_pass` in the existing UI
pass, or with `paint` when independent passes are needed. Offscreen attachments
can use a different resolution and sample count from the window. Apply clipping
and inherited opacity during the final composition. Additional passes can also
be encoded during `paint` when they must execute at that position in the scene.
GPUI does not supply a CAD renderer or manage these application-owned attachments.
Direct and offscreen surfaces can coexist in the same window. Both paths can
update every frame; offscreen rendering additionally allows reuse of a completed
texture when viewport content has not changed.

Callbacks execute serially. Never submit or present the frame, reenter GPUI,
retain borrowed frame targets, or leave an application encoder active on return,
including error returns. Encoding is not GPU completion: caches and asynchronous
readbacks must account for frames that fail or are abandoned before submission.
Keep separate resources or buffer ranges for different snapshots when multiple
surfaces share a painter. Preparation runs again on scene replays and encoding
retries. On device replacement, `reset` releases resources and subsequent callbacks
recreate them on the supplied device; resize alone does not invoke `reset`.

The macOS Metal example demonstrates two independent, animated, depth-tested
viewports with different camera angles. Each can switch between direct rendering
with its own pass and offscreen rendering with shared-pass composition. It also
demonstrates MSAA resolve, clipping, opacity and offscreen caching. Run it from
the repository root:

```sh
cargo run -p gpui_perf --example cad_viewport --features gpui_platform_fast/runtime_shaders
```

Click each viewport's rendering mode to switch paths and pause animation to reuse
offscreen results. Each viewport registers a separate painter so camera data,
depth attachments and offscreen caches remain independent. Shader, pipeline and
immutable model resources can be shared by application-owned caches when the
device and pipeline configuration match.

## Source and GPUI-fast integration

Ported from `Pomelo-Native/references/zed` at `9c2861ccaa`, branch
`gpui-pre-native-gpu`, including these four commits:

- `0463c82ffb` — Implement GPU painter support across native backends
- `760e9f567b` — Add GPU painter preparation for CAD viewports
- `d7549ea249` — Add shared-pass GPU drawing for CAD viewports
- `9c2861ccaa` — Unify GPU painter API across native backends

The implementation and regression tests live under each crate's `src/fast/`.
Native commands are retained as scene primitives so balanced replay, composition
splitting, content-mask intersection and scrolling translations keep their snapshots.
Scrolling content containing native paints is replayed into the current frame;
its callbacks are never replaced by a cached scroll tile. Rounded scene groups
are visited during preparation before UI drawing starts. Native painters receive
rectangular clipping; their own shaders/passes must implement any additional
shape clipping they require, as they must apply the supplied inherited opacity.

The source workspace uses the package name `gpui` (version `0.2.2`); release
packaging produces the `gpui-fast` crates. For GPUI Kit 0.8.0, use the
[compatibility patches](../compat/README.md) against this branch so all crates
share the same native types and backend device.

## Validation

On macOS, the `fast::` library regressions pass: 351 GPUI tests, 33 Metal
renderer tests, and 42 WGPU tests (three manual performance tests stay ignored).
The CAD example's three GPU tests and both typed-handle doctests pass as well.
`script/check-upstream`, `git diff --check` and scoped Clippy complete; Clippy
reports existing profiler visibility and duplicate fixture-module warnings.
Windows D3D11 cross-checks with `--target x86_64-pc-windows-gnu
--no-default-features`; this host does not have Windows resource compiler tools.
Linux cross-checking is blocked by the missing `x86_64-linux-musl-g++` toolchain;
X11/Wayland need a native Linux build and runtime check.
