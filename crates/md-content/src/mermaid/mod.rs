use crate::pixels::{WIDTH_QUANT, snap_px};
use gpui::{Bounds, Corners, Window, point, px, size};
use md_core::Px;
use md_core::block::BlockId;
use md_core::document::Document;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::time::Instant;

const MAX_IN_FLIGHT: usize = 4;
const MAX_ENTRIES: usize = 48;
const MAX_BYTES: usize = 96 * 1024 * 1024;

const RASTER_SCREENS: usize = 8;
const MIN_BYTES: usize = 48 * 1024 * 1024;
const VIEWPORT_ENTRIES: usize = 48;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub bytes: usize,
    pub entries: usize,
}

impl Budget {
    pub fn for_viewport(width: f64, height: f64, dpr: f64) -> Self {
        let dpr = dpr.max(0.25);
        let screen = (width.max(1.0) * height.max(1.0) * dpr * dpr * 4.0) as usize;
        Budget {
            bytes: screen.saturating_mul(RASTER_SCREENS).max(MIN_BYTES),
            entries: VIEWPORT_ENTRIES,
        }
    }
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            bytes: MAX_BYTES,
            entries: MAX_ENTRIES,
        }
    }
}

const WARM_IN_FLIGHT: usize = 1;

const WARM_EXTRA_ENTRIES: usize = 8;
const VIEWBOX_PAD: f32 = 2.0;

mod cache;
mod raster;
mod svg;
mod theme;
mod warm;

#[cfg(test)]
mod tests;

pub use crate::pixels::ReadyImage;
#[cfg(feature = "bench-support")]
pub use raster::prepare_sealed;
pub use raster::{
    estimate_job, estimate_zoom_job, raster, raster_svg, raster_with_svg, render_sealed, render_svg,
};
pub use svg::MAX_RASTER_PIXELS;
pub use theme::{spec_from_theme, theme_fingerprint};

const FAIL_LABEL_CAP: usize = 512;

pub fn fail_label(err: &crate::Error) -> String {
    struct Capped {
        buf: String,
    }
    impl std::fmt::Write for Capped {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            let room = FAIL_LABEL_CAP.saturating_sub(self.buf.len());
            if room == 0 {
                return Ok(());
            }
            let mut take = s.len().min(room);
            while !s.is_char_boundary(take) {
                take -= 1;
            }
            self.buf.push_str(&s[..take]);
            Ok(())
        }
    }
    let mut out = Capped { buf: String::new() };
    use std::fmt::Write as _;
    let _ = std::write!(out, "{err}");
    out.buf
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MermaidRasterKey {
    pub source: u64,
    pub theme_fp: u64,
    pub width_q: u32,
    pub max_h: u32,
    pub dpr_q: u16,
}

impl MermaidRasterKey {
    pub fn for_source(source: &str, theme_fp: u64, width_q: u32, max_h: u32, dpr_q: u16) -> Self {
        Self {
            source: hash_source(source),
            theme_fp,
            width_q,
            max_h,
            dpr_q,
        }
    }

    pub fn dpr(self) -> f32 {
        crate::pixels::dpr_from_q(self.dpr_q)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MermaidKey {
    pub block: BlockId,
    pub generation: u32,
    pub raster: MermaidRasterKey,
}

impl MermaidKey {
    pub fn dpr(self) -> f32 {
        self.raster.dpr()
    }

    pub fn svg_key(self) -> MermaidSvgKey {
        MermaidSvgKey {
            source: self.raster.source,
            theme_fp: self.raster.theme_fp,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MermaidSvgKey {
    pub source: u64,
    pub theme_fp: u64,
}

#[derive(Clone)]
pub struct RasterSpec {
    pub theme_fp: u64,
    pub dark: bool,
    pub canvas: String,
    pub surface: String,
    pub cluster: String,
    pub text: String,
    pub subtle: String,
    pub border: String,
    pub line: String,
    pub ok: String,
    pub warn: String,
    pub error: String,
    pub series: [String; 8],
    pub font_size: String,
    pub font_family: String,
    pub fit_w: u32,
    pub fit_h: u32,
    pub dpr: f32,
}

enum Slot {
    InFlight,
    Ready(ReadyImage),
    /// Pixels replaced by their compressed form while the diagram is off
    /// screen. The entry still answers "do I have this diagram", it just has
    /// no pixels to paint until it is inflated again.
    Warm(crate::media::WarmImage),
    /// An inflation job is running, so nothing else may start for this key.
    Thawing(crate::media::WarmImage),
    Failed(crate::Error),
}

type CurrentMermaid = (MermaidRasterKey, (f32, f32));
type MermaidIdentity = (BlockId, u32);

const MAX_SVG_ENTRIES: usize = 16;

const MAX_FITTED_ENTRIES: usize = 1024;

pub struct MermaidCache {
    budget: Budget,
    map: HashMap<MermaidRasterKey, Slot>,
    lru: VecDeque<MermaidRasterKey>,
    latest: HashMap<MermaidIdentity, MermaidRasterKey>,
    hot: HashSet<MermaidRasterKey>,
    warm: HashSet<MermaidRasterKey>,
    bytes: usize,
    warm_bytes: usize,
    inflight: usize,
    cooling: HashSet<MermaidRasterKey>,
    left_screen: HashMap<MermaidRasterKey, Instant>,
    cool_refused: HashSet<MermaidRasterKey>,
    cool_discarded: usize,
    /// Set the first time a diagram is dropped for lack of room. A cache that
    /// has never had to evict is holding every diagram the document asked for,
    /// and compressing one there only costs an encode and a decode.
    spilled: bool,
    theme_fp: u64,
    svgs: HashMap<MermaidSvgKey, std::sync::Arc<merman::svg::ResvgCompatibleSvg>>,
    svg_order: VecDeque<MermaidSvgKey>,
    current: Rc<HashMap<MermaidIdentity, CurrentMermaid>>,
    fitted: Rc<HashMap<MermaidIdentity, (f32, f32)>>,
    fitted_order: VecDeque<MermaidIdentity>,
}

pub fn key_for(
    doc: &Document,
    block: BlockId,
    content_width: Px,
    max_w: f64,
    max_h: f64,
    dpr: f64,
    theme_fp: u64,
) -> Option<MermaidKey> {
    let nid = doc.live_id(block)?;
    doc.arena.get(nid)?;
    let source = doc.text_of(block).map(hash_source).unwrap_or_default();
    let width = content_width.min(max_w).max(1.0);
    let width_q = ((width / WIDTH_QUANT).round() * WIDTH_QUANT).max(WIDTH_QUANT) as u32;
    let max_h = max_h.round().max(1.0) as u32;
    let dpr_q = crate::pixels::dpr_q(dpr);
    Some(MermaidKey {
        block,
        generation: nid.generation.get(),
        raster: MermaidRasterKey {
            source,
            theme_fp,
            width_q,
            max_h,
            dpr_q,
        },
    })
}

fn hash_source(source: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

fn fit_rect(slot_w: f32, slot_h: f32, image_w: f32, image_h: f32) -> Option<(f32, f32, f32, f32)> {
    if slot_w <= 0.0 || slot_h <= 0.0 || image_w <= 0.0 || image_h <= 0.0 {
        return None;
    }
    let scale = (slot_w / image_w).min(slot_h / image_h).min(1.0);
    let width = image_w * scale;
    let height = image_h * scale;
    Some((
        (slot_w - width) * 0.5,
        (slot_h - height) * 0.5,
        width,
        height,
    ))
}

pub fn paint_ready(
    window: &mut Window,
    origin_x: f32,
    origin_y: f32,
    slot_w: Px,
    slot_h: Px,
    ready: &ReadyImage,
) {
    let Some((x, y, w, h)) = paint_rect(
        origin_x,
        origin_y,
        slot_w,
        slot_h,
        ready,
        window.scale_factor(),
    ) else {
        return;
    };
    let bounds = Bounds {
        origin: point(px(x), px(y)),
        size: size(px(w), px(h)),
    };
    let _ = window.paint_image(
        bounds,
        bounds,
        Corners::all(px(0.0)),
        ready.image.clone(),
        0,
        false,
    );
}

fn paint_rect(
    origin_x: f32,
    origin_y: f32,
    slot_w: Px,
    slot_h: Px,
    ready: &ReadyImage,
    window_dpr: f32,
) -> Option<(f32, f32, f32, f32)> {
    let (iw, ih) = ready.css_size();
    let (dx, dy, w, h) = fit_rect(slot_w as f32, slot_h as f32, iw, ih)?;
    let dpr = window_dpr.max(0.25);
    Some((
        snap_px(origin_x + dx, dpr),
        snap_px(origin_y + dy, dpr),
        w,
        h,
    ))
}
