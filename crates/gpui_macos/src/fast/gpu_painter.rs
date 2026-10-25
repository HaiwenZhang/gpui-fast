use crate::window::MacWindow;
pub(crate) fn register(
    window: &MacWindow,
    registration: gpui::GpuPainterRegistration,
) -> anyhow::Result<()> {
    window.0.lock().renderer.register_gpu_painter(registration)
}
