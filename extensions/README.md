# Feature extensions

Feature implementations live outside the pristine upstream crates and the
performance sources. `composition/adapters/` contains native window composition;
`native-gpu/adapters/` contains the GPU painter API, scene integration and Metal,
WGPU, D3D11 and platform adapters. Each directory is organized by its host crate.

These adapters are source extensions, assembled as modules inside their host
crates so private renderer access and existing public type identities survive.
They are not separate dynamic plugins or independently usable Cargo packages.
The directory mappings and integration patches are declared in
[`integration/overlays.json`](../integration/overlays.json).

[`cad/`](cad/) is the separate `gpui_cad` demonstration package. It owns its
viewport renderer, shaders and regression tests, and does not depend on
`gpui_perf`. New shader/SVG feature packages can be added here when implemented.

Build through `python3 script/cargo`; see the
[source ownership and sync guide](../docs/upstream-sync.md).
