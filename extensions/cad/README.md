# CAD viewport demonstration

This package owns the Metal viewport demo previously hosted by `gpui_perf`.
It is a demonstration of depth testing, MSAA, independent/shared passes, immutable
snapshots and offscreen cache reuse, not a general CAD engine. GPU painter
infrastructure lives separately in `extensions/native-gpu/`.

From the checkout root on macOS:

```sh
python3 script/cargo run -p gpui_cad --bin cad_viewport --features gpui_cad/runtime_shaders
python3 script/cargo test -p gpui_cad --bin cad_viewport --features gpui_cad/runtime_shaders --locked
```

The three real-GPU tests retain the original depth/clipping/opacity, independent
viewport and abandoned-frame/cache/reset checks. Other platforms compile a
message explaining that this demonstration requires Metal.
