//! Native application encoders share the frame's device and command buffer.
use crate::metal_renderer::{MetalRenderer, new_command_encoder_for_texture};
use anyhow::Result;
use gpui::{
    DevicePixels, GpuBackend, GpuDrawMode, GpuPaintContext, GpuPainterRegistration,
    GpuPainterRegistry, GpuPassContext, GpuPreparationPhase, GpuPrepareContext, GpuResetReason,
    MetalDrawContext, MetalPaintContext, MetalPrepareContext, Scene, Size,
};

pub(crate) struct GpuState {
    registry: GpuPainterRegistry,
    depth_stencil: metal::DepthStencilState,
}
impl GpuState {
    pub(crate) fn new(device: &metal::DeviceRef) -> Self {
        Self {
            registry: GpuPainterRegistry::default(),
            depth_stencil: device.new_depth_stencil_state(&metal::DepthStencilDescriptor::new()),
        }
    }
}
pub(crate) fn destroy(renderer: &MetalRenderer) {
    renderer
        .fast_gpu
        .registry
        .reset(GpuResetReason::WindowDestroyed);
}
impl MetalRenderer {
    /// Registers native application drawing with this renderer's device.
    pub fn register_gpu_painter(&self, registration: GpuPainterRegistration) -> Result<()> {
        registration.validate(GpuBackend::Metal)?;
        self.fast_gpu.registry.register(registration);
        Ok(())
    }
    pub(crate) fn draw_gpu_surface<'frame>(
        &self,
        painter: &mut dyn gpui::ErasedGpuPainter,
        mode: GpuDrawMode,
        context: MetalPaintContext<'frame>,
        data: &(dyn std::any::Any + Send + Sync),
        command_encoder: &mut &'frame metal::RenderCommandEncoderRef,
        viewport_size: Size<DevicePixels>,
    ) -> Result<()> {
        match mode {
            GpuDrawMode::SharedPass => {
                let target = context.target;
                let [left, top, width, height] = target.scissor_rect();
                command_encoder.set_viewport(metal::MTLViewport {
                    originX: target.bounds.origin.x.0 as f64,
                    originY: target.bounds.origin.y.0 as f64,
                    width: target.bounds.size.width.0 as f64,
                    height: target.bounds.size.height.0 as f64,
                    znear: 0.0,
                    zfar: 1.0,
                });
                command_encoder.set_scissor_rect(metal::MTLScissorRect {
                    x: left as u64,
                    y: top as u64,
                    width: width as u64,
                    height: height as u64,
                });
                let result = painter.draw_in_pass_erased(
                    &mut GpuPassContext::Metal(&mut MetalDrawContext {
                        target,
                        device: context.device,
                        encoder: command_encoder,
                        color_format: context.color_format,
                    }),
                    data,
                );
                self.restore_ui_encoder_state(command_encoder, target.size, viewport_size);
                result
            }
            GpuDrawMode::OwnPass => {
                command_encoder.end_encoding();
                let command_buffer = context.command_buffer;
                let color_target = context.color_target;
                let result = painter.paint_erased(&mut GpuPaintContext::Metal(context), data);
                *command_encoder = new_command_encoder_for_texture(
                    command_buffer,
                    color_target,
                    viewport_size,
                    None,
                );
                result
            }
        }
    }

    fn restore_ui_encoder_state(
        &self,
        encoder: &metal::RenderCommandEncoderRef,
        size: [u32; 2],
        viewport_size: Size<DevicePixels>,
    ) {
        encoder.set_viewport(metal::MTLViewport {
            originX: 0.0,
            originY: 0.0,
            width: viewport_size.width.0 as f64,
            height: viewport_size.height.0 as f64,
            znear: 0.0,
            zfar: 1.0,
        });
        encoder.set_scissor_rect(metal::MTLScissorRect {
            x: 0,
            y: 0,
            width: size[0] as u64,
            height: size[1] as u64,
        });
        encoder.set_cull_mode(metal::MTLCullMode::None);
        encoder.set_front_facing_winding(metal::MTLWinding::Clockwise);
        encoder.set_triangle_fill_mode(metal::MTLTriangleFillMode::Fill);
        encoder.set_depth_clip_mode(metal::MTLDepthClipMode::Clip);
        encoder.set_depth_bias(0.0, 0.0, 0.0);
        encoder.set_depth_stencil_state(&self.fast_gpu.depth_stencil);
        encoder.set_stencil_reference_value(0);
        encoder.set_blend_color(0.0, 0.0, 0.0, 0.0);
        encoder.set_visibility_result_mode(metal::MTLVisibilityResultMode::Disabled, 0);
    }
}

pub(crate) fn prepare(
    renderer: &MetalRenderer,
    scene: &Scene,
    texture: &metal::TextureRef,
    commands: &metal::CommandBufferRef,
) -> gpui::GpuScenePreparation {
    gpui::prepare_gpu_scene(
        scene,
        [texture.width() as u32, texture.height() as u32],
        texture.sample_count() as u32,
        GpuBackend::Metal.capabilities(),
        |painter, draw, target, phase| {
            let mut context = GpuPrepareContext::Metal(MetalPrepareContext {
                target,
                device: &renderer.device,
                command_buffer: commands,
                color_format: texture.pixel_format(),
            });
            match phase {
                GpuPreparationPhase::Prepare => {
                    painter.prepare_erased(&mut context, draw.data.as_ref())
                }
                GpuPreparationPhase::FinishPrepare => {
                    painter.finish_prepare_erased(&mut context, draw.data.as_ref())
                }
            }
        },
    )
}

#[cfg(test)]
mod gpu_painter_tests {
    use super::{
        GpuBackend, GpuDrawMode, GpuPaintContext, GpuPassContext, GpuResetReason,
        MetalPaintContext, new_command_encoder_for_texture,
    };
    use crate::metal_renderer::{
        InstanceBufferPool, InstanceBufferWriter, MetalRenderer, read_texture_to_image,
        write_instances,
    };
    use anyhow::{Context as _, Result};
    use foreign_types::ForeignTypeRef;
    use gpui::GpuPainter;
    use gpui::{Bounds, ContentMask, DevicePixels, ScaledPixels, Scene, point, size};
    use gpui::{GpuPaintTarget, Quad};
    use metal::MTLPixelFormat;
    use parking_lot::Mutex;
    use std::{mem, sync::Arc};

    struct Painting {
        mode: GpuDrawMode,
        color: [f32; 4],
        fail: bool,
        skip_drawing: bool,
    }

    struct SolidPainter(metal::RenderPipelineState);

    impl SolidPainter {
        fn new(device: &metal::DeviceRef) -> Result<Self> {
            let library = device
                .new_library_with_source(
                    r#"
                #include <metal_stdlib>
                using namespace metal;
                vertex float4 solid_vertex(uint vertex_id [[vertex_id]]) {
                    const float2 positions[] = {
                        {-1, -1}, {1, -1}, {-1, 1}, {-1, 1}, {1, -1}, {1, 1}
                    };
                    return float4(positions[vertex_id], 0, 1);
                }
                fragment float4 solid_fragment(constant float4 &color [[buffer(0)]]) {
                    return color;
                }
            "#,
                    &metal::CompileOptions::new(),
                )
                .map_err(anyhow::Error::msg)?;
            let descriptor = metal::RenderPipelineDescriptor::new();
            let vertex = library
                .get_function("solid_vertex", None)
                .map_err(anyhow::Error::msg)?;
            let fragment = library
                .get_function("solid_fragment", None)
                .map_err(anyhow::Error::msg)?;
            descriptor.set_vertex_function(Some(&vertex));
            descriptor.set_fragment_function(Some(&fragment));
            descriptor
                .color_attachments()
                .object_at(0)
                .context("color attachment")?
                .set_pixel_format(MTLPixelFormat::BGRA8Unorm);
            Ok(Self(
                device
                    .new_render_pipeline_state(&descriptor)
                    .map_err(anyhow::Error::msg)?,
            ))
        }

        fn draw(
            &self,
            encoder: &metal::RenderCommandEncoderRef,
            painting: &Painting,
        ) -> Result<()> {
            if !painting.skip_drawing {
                encoder.set_render_pipeline_state(&self.0);
                encoder.set_fragment_bytes(
                    0,
                    mem::size_of_val(&painting.color) as u64,
                    painting.color.as_ptr().cast(),
                );
                encoder.draw_primitives(metal::MTLPrimitiveType::Triangle, 0, 6);
            }
            if painting.fail {
                encoder.set_viewport(metal::MTLViewport {
                    originX: 0.0,
                    originY: 0.0,
                    width: 1.0,
                    height: 1.0,
                    znear: 0.0,
                    zfar: 1.0,
                });
                encoder.set_scissor_rect(metal::MTLScissorRect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1,
                });
                encoder.set_cull_mode(metal::MTLCullMode::Front);
                encoder.set_triangle_fill_mode(metal::MTLTriangleFillMode::Lines);
                anyhow::bail!("drawing failed after changing state");
            }
            Ok(())
        }
    }

    impl GpuPainter for SolidPainter {
        type Snapshot = Painting;
        fn supports_backend(&self, backend: GpuBackend) -> bool {
            backend == GpuBackend::Metal
        }
        fn draw_in_pass(
            &mut self,
            context: &mut GpuPassContext<'_, '_>,
            painting: &Painting,
        ) -> Result<()> {
            let context = context.metal()?;
            self.draw(context.encoder, painting)
        }

        fn paint(&mut self, context: &mut GpuPaintContext<'_>, painting: &Painting) -> Result<()> {
            let context = context.metal()?;
            let descriptor = metal::RenderPassDescriptor::new();
            let color = descriptor
                .color_attachments()
                .object_at(0)
                .context("color attachment")?;
            color.set_texture(Some(context.color_target));
            color.set_load_action(metal::MTLLoadAction::Load);
            color.set_store_action(metal::MTLStoreAction::Store);
            let encoder = context
                .command_buffer
                .new_render_command_encoder(&descriptor);
            let target = context.target;
            encoder.set_viewport(metal::MTLViewport {
                originX: target.bounds.origin.x.0 as f64,
                originY: target.bounds.origin.y.0 as f64,
                width: target.bounds.size.width.0 as f64,
                height: target.bounds.size.height.0 as f64,
                znear: 0.0,
                zfar: 1.0,
            });
            let [left, top, width, height] = target.scissor_rect();
            encoder.set_scissor_rect(metal::MTLScissorRect {
                x: left as u64,
                y: top as u64,
                width: width as u64,
                height: height as u64,
            });
            let result = self.draw(encoder, painting);
            encoder.end_encoding();
            result
        }

        fn reset(&mut self, _: GpuResetReason) {}
    }

    #[test]
    fn mixed_passes_preserve_order_and_restore_ui_state_after_errors() -> Result<()> {
        objc2::rc::autoreleasepool(|_| {
            let renderer =
                MetalRenderer::new_headless(Arc::new(Mutex::new(InstanceBufferPool::default())));
            let device = renderer.device.clone();
            let mut painter = SolidPainter::new(&device)?;
            let descriptor = metal::TextureDescriptor::new();
            descriptor.set_width(64);
            descriptor.set_height(64);
            descriptor.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
            descriptor.set_usage(metal::MTLTextureUsage::RenderTarget);
            descriptor.set_storage_mode(if renderer.is_unified_memory {
                metal::MTLStorageMode::Shared
            } else {
                metal::MTLStorageMode::Managed
            });
            let texture = device.new_texture(&descriptor);
            let command_buffer = renderer.command_queue.new_command_buffer();
            let viewport_size = size(DevicePixels(64), DevicePixels(64));
            let mut encoder = new_command_encoder_for_texture(
                command_buffer,
                &texture,
                viewport_size,
                Some(metal::MTLClearColor::new(0.0, 0.0, 0.0, 1.0)),
            );
            let bounds = |left, top, width, height| {
                Bounds::new(
                    point(ScaledPixels(left), ScaledPixels(top)),
                    size(ScaledPixels(width), ScaledPixels(height)),
                )
            };
            let mut scene = Scene::default();
            for (bounds, color) in [
                (bounds(2.0, 2.0, 6.0, 6.0), 0xff00ff),
                (bounds(50.0, 50.0, 8.0, 8.0), 0xffff00),
                (bounds(50.0, 2.0, 8.0, 8.0), 0x00ffff),
            ] {
                scene.insert_primitive(Quad {
                    bounds,
                    content_mask: ContentMask { bounds },
                    background: gpui::rgb(color).into(),
                    ..Default::default()
                });
            }
            scene.finish();
            let mut writer = InstanceBufferWriter::new(
                &device,
                &renderer.instance_buffer_pool,
                renderer.is_unified_memory,
            );
            let bindings = write_instances(&scene, &mut writer)?;
            renderer.draw_quads(0..1, &bindings, viewport_size, encoder);
            let target = GpuPaintTarget {
                size: [64, 64],
                bounds: bounds(16.0, 16.0, 32.0, 32.0),
                clip: bounds(20.0, 20.0, 16.0, 16.0),
                scale_factor: 2.0,
                sample_count: 1,
                opacity: 1.0,
            };
            for (index, painting) in [
                Painting {
                    mode: GpuDrawMode::SharedPass,
                    color: [1.0, 0.0, 0.0, 1.0],
                    fail: false,
                    skip_drawing: false,
                },
                Painting {
                    mode: GpuDrawMode::OwnPass,
                    color: [0.0, 1.0, 0.0, 1.0],
                    fail: false,
                    skip_drawing: false,
                },
                Painting {
                    mode: GpuDrawMode::SharedPass,
                    color: [0.0, 0.0, 1.0, 1.0],
                    fail: true,
                    skip_drawing: false,
                },
                Painting {
                    mode: GpuDrawMode::OwnPass,
                    color: [0.0; 4],
                    fail: true,
                    skip_drawing: true,
                },
            ]
            .into_iter()
            .enumerate()
            {
                let previous_encoder = encoder.as_ptr();
                let result = renderer.draw_gpu_surface(
                    &mut painter,
                    painting.mode,
                    MetalPaintContext {
                        target,
                        device: &device,
                        command_buffer,
                        color_target: &texture,
                        color_format: texture.pixel_format(),
                    },
                    &painting,
                    &mut encoder,
                    viewport_size,
                );
                assert_eq!(result.is_err(), painting.fail);
                if painting.mode == GpuDrawMode::SharedPass {
                    assert_eq!(encoder.as_ptr(), previous_encoder);
                }
                if index == 2 {
                    renderer.draw_quads(1..2, &bindings, viewport_size, encoder);
                }
            }
            renderer.draw_quads(2..3, &bindings, viewport_size, encoder);
            encoder.end_encoding();
            if !renderer.is_unified_memory {
                let blit = command_buffer.new_blit_command_encoder();
                blit.synchronize_resource(&texture);
                blit.end_encoding();
            }
            command_buffer.commit();
            command_buffer.wait_until_completed();
            anyhow::ensure!(
                command_buffer.status() == metal::MTLCommandBufferStatus::Completed,
                "GPU command buffer failed"
            );
            let image = read_texture_to_image(&texture)?;
            for (position, expected) in [
                ((4, 4), [255, 0, 255, 255]),
                ((28, 28), [0, 0, 255, 255]),
                ((18, 28), [0, 0, 0, 255]),
                ((54, 54), [255, 255, 0, 255]),
                ((54, 4), [0, 255, 255, 255]),
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
        })
    }
}
