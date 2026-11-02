//! Performance modules assembled from `fast/gpui/src/`.
//! Composition and native GPU feature adapters are owned by `extensions/`.

pub(crate) mod a11y;
pub(crate) mod dependencies;
pub(crate) mod dispatch;
pub(crate) mod global_id;
pub(crate) mod glyphs;
pub(crate) mod interactivity;
pub(crate) mod layers;
pub(crate) mod layout;
pub(crate) mod layout_bounds;
pub(crate) mod layout_key;
pub(crate) mod line_break;
pub(crate) mod misc;
pub(crate) mod number_shaping;
pub(crate) mod path_cache;
pub(crate) mod retained;
pub(crate) mod rounded_clip;
pub(crate) mod rounded_clip_scene;
pub(crate) mod scene;
pub(crate) mod splice;
pub(crate) mod stats;
pub(crate) mod text;
pub(crate) mod text_style;

#[cfg(test)]
mod tests;
