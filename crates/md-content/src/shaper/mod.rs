use gpui::WindowTextSystem;
use md_theme::{DecorationTokens, InlineTokens, SyntaxTokens};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use crate::math::MathMetrics;

mod artifact;
mod atoms;
mod bands;
mod build;
mod cache;
mod color;
mod flow;
mod flow_text;
mod measure;
mod position;
mod resolved;
mod shape;
mod tabs;

#[cfg(test)]
mod tests;

pub use artifact::{ShapeArtifact, ShapeBand, ShapeMedia, ShapePart};
pub use cache::{CacheStats, ShapeCache, Stats};
pub use position::band_align_shift;
use resolved::ResolvedTypes;
pub use resolved::snapped_row_advance;

const SCRIPT_SCALE: f32 = 0.75;
const SUPER_RISE: f32 = 0.4;
const SUB_DROP: f32 = 0.2;

pub struct GpuiShaper {
    text_system: Arc<WindowTextSystem>,
    roles: ResolvedTypes,
    cache: Rc<ShapeCache>,
    stats: RefCell<Stats>,
    scale: f64,
    inline: InlineTokens,
    decoration: DecorationTokens,
    mermaid_fitted: Rc<HashMap<(u32, u32), (f32, f32)>>,
    math_metrics: Rc<MathMetrics>,
    image_sizes: Rc<HashMap<crate::images::SourceKey, (u32, u32)>>,
    image_failed: Rc<HashSet<crate::images::SourceKey>>,
    link_dests: Rc<HashMap<u32, String>>,
    link_raw: Rc<HashMap<u32, (String, String)>>,
    block_image_dest: Rc<HashMap<u32, String>>,
    block_code_lang: Rc<HashMap<u32, String>>,
    syntax: SyntaxTokens,
    math_gen: u64,
    image_gen: u64,
}
