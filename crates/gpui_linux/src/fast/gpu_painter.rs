#[cfg(feature = "x11")]
use crate::linux::X11Window;
#[cfg(feature = "wayland")]
use crate::linux::wayland::window::WaylandWindow;
#[cfg(any(feature = "x11", feature = "wayland"))]
use anyhow::Context as _;
#[cfg(feature = "x11")]
pub(crate) fn x11(
    window: &X11Window,
    registration: gpui::GpuPainterRegistration,
) -> anyhow::Result<()> {
    window
        .0
        .state
        .borrow()
        .renderer
        .as_ref()
        .context("cannot register GPU painter after window renderer is dropped")?
        .register_gpu_painter(registration)
}
#[cfg(feature = "wayland")]
pub(crate) fn wayland(
    window: &WaylandWindow,
    registration: gpui::GpuPainterRegistration,
) -> anyhow::Result<()> {
    window
        .borrow()
        .renderer
        .as_ref()
        .context("cannot register GPU painter after window renderer is dropped")?
        .register_gpu_painter(registration)
}
