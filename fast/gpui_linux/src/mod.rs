//! Performance modules assembled from `fast/gpui_linux/src/`.
//! Composition and native GPU feature adapters are owned by `extensions/`.

#[cfg(feature = "x11")]
pub(crate) mod x11_scale;
#[cfg(feature = "x11")]
pub(crate) mod xim;
