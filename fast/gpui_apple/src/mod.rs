//! Performance modules assembled from `fast/gpui_apple/src/`.
//! Composition and native GPU feature adapters are owned by `extensions/`.

pub(crate) mod layers;
pub(crate) mod paths;
pub(crate) mod rounded_clip;
#[cfg(feature = "font-kit")]
pub(crate) mod text_system;
