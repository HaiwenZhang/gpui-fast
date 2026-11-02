//! Performance modules assembled from `fast/gpui_windows/src/`.
//! Composition and native GPU feature adapters are owned by `extensions/`.

pub(crate) mod class_names;
pub(crate) mod device_wake;
pub(crate) mod draw_state;
pub(crate) mod frame;
pub(crate) mod frame_schedule;
pub(crate) mod globals;
pub(crate) mod layers;
#[cfg(test)]
pub(crate) mod native_tests;
pub(crate) mod rounded_clip;
