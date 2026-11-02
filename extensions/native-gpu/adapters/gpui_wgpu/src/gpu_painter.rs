//! Application drawing uses the optimized frame path's encoder and pass.
use crate::WgpuRenderer;
use anyhow::Result;
use gpui::{GpuBackend, GpuPaintTarget, GpuPassContext, GpuResetReason, WgpuDrawContext};

#[derive(Default)]
pub(crate) struct GpuState(gpui::GpuPainterRegistry);
pub(crate) fn destroy(renderer: &WgpuRenderer) {
    renderer.fast_gpu.0.reset(GpuResetReason::WindowDestroyed);
}
pub(crate) fn device_replaced(renderer: &WgpuRenderer) {
    renderer.fast_gpu.0.reset(GpuResetReason::DeviceReplaced);
}
#[cfg(not(target_family = "wasm"))]
pub(crate) fn recover_renderer(
    renderer: &mut WgpuRenderer,
    gpu_context: crate::GpuContext,
    context: &crate::WgpuContext,
    surface: wgpu::Surface<'static>,
    config: crate::WgpuSurfaceConfig,
) -> Result<()> {
    let mut replacement = WgpuRenderer::new_internal(
        Some(gpu_context),
        context,
        surface,
        config,
        renderer.compositor_gpu,
        renderer.atlas.clone(),
    )?;
    replacement.fast_gpu = std::mem::take(&mut renderer.fast_gpu);
    *renderer = replacement;
    Ok(())
}
impl WgpuRenderer {
    /// Registers application drawing on this window's native device.
    pub fn register_gpu_painter(&self, registration: gpui::GpuPainterRegistration) -> Result<()> {
        registration.validate(GpuBackend::Wgpu)?;
        self.fast_gpu.0.register(registration);
        Ok(())
    }
}
pub(crate) fn draw_in_pass(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    painter: &mut dyn gpui::ErasedGpuPainter,
    target: GpuPaintTarget,
    data: &(dyn std::any::Any + Send + Sync),
    pass: &mut wgpu::RenderPass<'_>,
) -> Result<()> {
    let bounds = target.bounds;
    let maximum_size = device.limits().max_texture_dimension_2d as f32;
    let maximum_position = maximum_size * 2.0;
    anyhow::ensure!(
        bounds.origin.x.0.is_finite()
            && bounds.origin.y.0.is_finite()
            && bounds.size.width.0.is_finite()
            && bounds.size.height.0.is_finite()
            && bounds.size.width.0 > 0.0
            && bounds.size.height.0 > 0.0
            && bounds.size.width.0 <= maximum_size
            && bounds.size.height.0 <= maximum_size
            && bounds.origin.x.0 >= -maximum_position
            && bounds.origin.y.0 >= -maximum_position
            && bounds.right().0 <= maximum_position - 1.0
            && bounds.bottom().0 <= maximum_position - 1.0,
        "GPU painter viewport exceeds the device's supported range: {bounds:?}"
    );
    let [left, top, width, height] = target.scissor_rect();
    pass.set_viewport(
        target.bounds.origin.x.0,
        target.bounds.origin.y.0,
        target.bounds.size.width.0,
        target.bounds.size.height.0,
        0.0,
        1.0,
    );
    pass.set_scissor_rect(left, top, width, height);
    let result = painter.draw_in_pass_erased(
        &mut GpuPassContext::Wgpu(WgpuDrawContext {
            target,
            device: &device,
            pass,
            color_format: format,
        }),
        data,
    );
    pass.set_viewport(
        0.0,
        0.0,
        target.size[0] as f32,
        target.size[1] as f32,
        0.0,
        1.0,
    );
    pass.set_scissor_rect(0, 0, target.size[0], target.size[1]);
    pass.set_blend_constant(wgpu::Color::TRANSPARENT);
    pass.set_stencil_reference(0);
    result
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::{GpuBackend, GpuPaintTarget, GpuPassContext, GpuResetReason, draw_in_pass};
    use anyhow::{Context as _, Result};
    use gpui::GpuDrawMode;
    use gpui::{Bounds, ContentMask, DevicePixels, Quad, ScaledPixels, Scene};
    #[test]
    fn shared_pass_restores_ui_state_after_an_encoding_error() -> Result<()> {
        struct SharedPainter(wgpu::RenderPipeline);
        impl gpui::GpuPainter for SharedPainter {
            type Snapshot = ();
            fn supports_backend(&self, backend: GpuBackend) -> bool {
                backend == GpuBackend::Wgpu
            }
            fn draw_mode(&self, _: gpui::GpuCapabilities, _: &()) -> GpuDrawMode {
                GpuDrawMode::SharedPass
            }
            fn draw_in_pass(&mut self, context: &mut GpuPassContext<'_, '_>, _: &()) -> Result<()> {
                let context = context.wgpu()?;
                context.pass.set_pipeline(&self.0);
                context.pass.draw(0..6, 0..1);
                context.pass.set_viewport(0.0, 0.0, 1.0, 1.0, 0.0, 1.0);
                context.pass.set_scissor_rect(0, 0, 1, 1);
                context.pass.set_blend_constant(wgpu::Color::WHITE);
                context.pass.set_stencil_reference(123);
                anyhow::bail!("shared drawing failed after encoding");
            }
            fn reset(&mut self, _: GpuResetReason) {}
        }

        let mut renderer = crate::wgpu_renderer::WgpuHeadlessRenderer::new()?;
        let viewport_size = gpui::size(DevicePixels(64), DevicePixels(64));
        renderer.render(&Scene::default(), viewport_size)?;
        let device = renderer.core.resources().device.clone();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shared_pass_test"),
            source: wgpu::ShaderSource::Wgsl(r#"
                @vertex fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
                    let positions = array<vec2<f32>, 6>(
                        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0),
                        vec2(-1.0, 1.0), vec2(1.0, -1.0), vec2(1.0, 1.0)
                    );
                    return vec4(positions[index], 0.0, 1.0);
                }
                @fragment fn fragment() -> @location(0) vec4<f32> {
                    return vec4(0.0, 0.0, 1.0, 1.0);
                }
            "#.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shared_pass_test"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                targets: &[Some(renderer.core.target_format.into())],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let mut painter = SharedPainter(pipeline);
        let bounds = |left, top, width, height| {
            Bounds::new(
                gpui::point(ScaledPixels(left), ScaledPixels(top)),
                gpui::size(ScaledPixels(width), ScaledPixels(height)),
            )
        };
        let mut scene = Scene::default();
        for (bounds, color) in [
            (bounds(2.0, 2.0, 6.0, 6.0), 0xff00ff),
            (bounds(50.0, 50.0, 8.0, 8.0), 0xffff00),
        ] {
            scene.insert_primitive(Quad {
                bounds,
                content_mask: ContentMask { bounds },
                background: gpui::rgb(color).into(),
                ..Default::default()
            });
        }
        scene.finish();
        let mut offset = 0;
        let bindings = renderer.core.write_instances(&scene, &mut offset)?;
        let view = renderer
            .render_target
            .as_ref()
            .context("target")?
            .view
            .clone();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shared_pass_test"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            let resources = renderer.core.resources();
            let invalid = draw_in_pass(
                &renderer.core.resources().device,
                renderer.core.target_format,
                &mut painter,
                GpuPaintTarget {
                    size: [64, 64],
                    bounds: bounds(
                        0.0,
                        0.0,
                        device.limits().max_texture_dimension_2d as f32 + 1.0,
                        16.0,
                    ),
                    clip: bounds(0.0, 0.0, 16.0, 16.0),
                    scale_factor: 1.0,
                    sample_count: 1,
                    opacity: 1.0,
                },
                &(),
                &mut pass,
            );
            assert!(invalid.is_err_and(|error| error.to_string().contains("supported range")));
            renderer.core.draw_instances(
                &bindings.quads,
                &resources.pipelines.quads,
                0..1,
                &mut pass,
            );
            let result = draw_in_pass(
                &renderer.core.resources().device,
                renderer.core.target_format,
                &mut painter,
                GpuPaintTarget {
                    size: [64, 64],
                    bounds: bounds(16.0, 16.0, 32.0, 32.0),
                    clip: bounds(20.0, 20.0, 16.0, 16.0),
                    scale_factor: 2.0,
                    sample_count: 1,
                    opacity: 1.0,
                },
                &(),
                &mut pass,
            );
            assert!(result.is_err());
            renderer.core.draw_instances(
                &bindings.quads,
                &resources.pipelines.quads,
                1..2,
                &mut pass,
            );
        }
        renderer.core.resources().queue.submit([encoder.finish()]);
        let image = renderer.read_image()?;
        for (position, expected) in [
            ((4, 4), [255, 0, 255, 255]),
            ((28, 28), [0, 0, 255, 255]),
            ((18, 28), [0, 0, 0, 255]),
            ((54, 54), [255, 255, 0, 255]),
        ] {
            assert_eq!(
                image
                    .get_pixel_checked(position.0, position.1)
                    .context("pixel")?
                    .0,
                expected
            );
        }
        Ok(())
    }
}
