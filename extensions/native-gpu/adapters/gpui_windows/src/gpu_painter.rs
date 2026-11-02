//! D3D11 painters use isolated immediate-context state, restored even on errors.
use crate::directx_renderer::RENDER_TARGET_FORMAT;
use crate::fast::frame::Target;
use crate::{DirectXRenderer, window::WindowsWindow};
use anyhow::{Context as _, Result};
use gpui::{GpuBackend, GpuPainterRegistration, GpuResetReason};
use gpui_util::ResultExt;
use windows::{
    Win32::Graphics::Direct3D11::{
        D3D11_1_CREATE_DEVICE_CONTEXT_STATE_SINGLETHREADED, D3D11_CREATE_DEVICE_SINGLETHREADED,
        D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, ID3D11Device, ID3D11Device1, ID3D11DeviceContext,
        ID3D11DeviceContext1, ID3D11Texture2D, ID3DDeviceContextState,
    },
    core::Interface,
};

#[derive(Default)]
pub(crate) struct GpuState(gpui::GpuPainterRegistry);
pub(crate) fn destroy(renderer: &mut DirectXRenderer) {
    renderer.fast_gpu.0.reset(GpuResetReason::WindowDestroyed);
    renderer.fast_frame.native = NativeState::default();
}
pub(crate) fn device_replaced(renderer: &mut DirectXRenderer) {
    renderer.fast_gpu.0.reset(GpuResetReason::DeviceReplaced);
    renderer.fast_frame.native = NativeState::default();
}
pub(crate) fn register(window: &WindowsWindow, registration: GpuPainterRegistration) -> Result<()> {
    let mut renderer = window.0.state.renderer.borrow_mut();
    registration.validate(GpuBackend::D3D11)?;
    let devices = renderer.devices.as_ref().context("devices missing")?;
    let device = devices.device.clone();
    renderer.fast_frame.native.ensure(&device)?;
    renderer.fast_gpu.0.register(registration);
    Ok(())
}
#[derive(Default)]
pub(crate) struct NativeState {
    state: Option<ID3DDeviceContextState>,
    preparation: Option<gpui::GpuScenePreparation>,
}
impl NativeState {
    fn ensure(&mut self, device: &ID3D11Device) -> Result<()> {
        if self.state.is_none() {
            let device1: ID3D11Device1 = device.cast()?;
            unsafe {
                device1.CreateDeviceContextState(
                    if device.GetCreationFlags() & D3D11_CREATE_DEVICE_SINGLETHREADED.0 as u32 != 0
                    {
                        D3D11_1_CREATE_DEVICE_CONTEXT_STATE_SINGLETHREADED.0 as u32
                    } else {
                        0
                    },
                    &[device.GetFeatureLevel()],
                    D3D11_SDK_VERSION,
                    &ID3D11Device::IID,
                    None,
                    Some(&mut self.state),
                )?;
            }
        }
        Ok(())
    }
    fn enter(
        &mut self,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
    ) -> Result<StateRestore> {
        self.ensure(device)?;
        let context: ID3D11DeviceContext1 = context.cast()?;
        let mut previous = None;
        unsafe {
            context.SwapDeviceContextState(
                self.state.as_ref().context("painter state missing")?,
                Some(&mut previous),
            );
        }
        Ok(StateRestore { context, previous })
    }
}
struct StateRestore {
    context: ID3D11DeviceContext1,
    previous: Option<ID3DDeviceContextState>,
}
impl Drop for StateRestore {
    fn drop(&mut self) {
        if let Some(previous) = &self.previous {
            unsafe {
                self.context.ClearState();
                self.context.SwapDeviceContextState(previous, None);
            }
        }
    }
}
fn color_target(
    target: &Target<'_>,
) -> Result<(
    windows::Win32::Graphics::Direct3D11::ID3D11RenderTargetView,
    [u32; 2],
)> {
    let mut views = [None];
    unsafe {
        target
            .device_context
            .OMGetRenderTargets(Some(&mut views), None);
    }
    let view = views[0]
        .take()
        .context("native painter color target missing")?;
    let texture: ID3D11Texture2D = unsafe { view.GetResource()? }.cast()?;
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe {
        texture.GetDesc(&mut desc);
    }
    Ok((view, [desc.Width, desc.Height]))
}
pub(crate) fn prepare(
    target: &Target<'_>,
    state: &mut NativeState,
    scene: &gpui::Scene,
) -> Result<()> {
    if !contains_paints(scene) {
        state.preparation = None;
        return Ok(());
    }
    let (_, size) = color_target(target)?;
    let preparation = gpui::prepare_gpu_scene(
        scene,
        size,
        1,
        GpuBackend::D3D11.capabilities(),
        |painter, primitive, info, phase| {
            let _restore = state.enter(target.device, target.device_context)?;
            unsafe {
                target.device_context.OMSetRenderTargets(None, None);
            }
            let mut context = gpui::GpuPrepareContext::D3D11(gpui::D3D11PrepareContext {
                target: info,
                device: target.device,
                context: target.device_context,
                color_format: RENDER_TARGET_FORMAT,
            });
            match phase {
                gpui::GpuPreparationPhase::Prepare => {
                    painter.prepare_erased(&mut context, primitive.data.as_ref())
                }
                gpui::GpuPreparationPhase::FinishPrepare => {
                    painter.finish_prepare_erased(&mut context, primitive.data.as_ref())
                }
            }
        },
    );
    state.preparation = Some(preparation);
    Ok(())
}
fn contains_paints(scene: &gpui::Scene) -> bool {
    !scene.gpu_paints.is_empty()
        || scene
            .layers
            .frames
            .iter()
            .any(|frame| frame.content.scene().is_some_and(contains_paints))
}
pub(crate) fn draw(
    target: &Target<'_>,
    state: &mut NativeState,
    scene: &gpui::Scene,
    range: std::ops::Range<usize>,
) -> Result<()> {
    let (view, size) = color_target(target)?;
    for surface in &scene.gpu_paints[range] {
        if state
            .preparation
            .as_ref()
            .and_then(|preparation| preparation.draw_mode(surface))
            .is_none()
        {
            continue;
        }
        let info = surface
            .draw
            .target(size, surface.bounds, surface.content_mask, 1);
        let [left, top, width, height] = info.scissor_rect();
        if width == 0 || height == 0 {
            continue;
        }
        let _restore = state.enter(target.device, target.device_context)?;
        unsafe {
            target
                .device_context
                .OMSetRenderTargets(Some(&[Some(view.clone())]), None);
            target.device_context.RSSetViewports(Some(&[
                windows::Win32::Graphics::Direct3D11::D3D11_VIEWPORT {
                    TopLeftX: info.bounds.origin.x.0,
                    TopLeftY: info.bounds.origin.y.0,
                    Width: info.bounds.size.width.0,
                    Height: info.bounds.size.height.0,
                    MinDepth: 0.0,
                    MaxDepth: 1.0,
                },
            ]));
            target
                .device_context
                .RSSetScissorRects(Some(&[windows::Win32::Foundation::RECT {
                    left: left as i32,
                    top: top as i32,
                    right: (left + width) as i32,
                    bottom: (top + height) as i32,
                }]));
        }
        surface
            .draw
            .handle
            .invoke(|painter| {
                painter.paint_erased(
                    &mut gpui::GpuPaintContext::D3D11(gpui::D3D11PaintContext {
                        target: info,
                        device: target.device,
                        context: target.device_context,
                        color_target: &view,
                        color_format: RENDER_TARGET_FORMAT,
                    }),
                    surface.draw.data.as_ref(),
                )
            })
            .log_err();
    }
    Ok(())
}
