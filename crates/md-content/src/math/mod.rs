use md_theme::{DocumentTheme, ThemeColor};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

pub(crate) const MATH_PAD: f32 = 1.0;
const MAX_IN_FLIGHT: usize = 4;
const MAX_ENTRIES: usize = 256;
const MAX_BYTES: usize = 32 * 1024 * 1024;

const WARM_EXTRA_ENTRIES: usize = 192;

const MAX_METRIC_ENTRIES: usize = 4096;

mod cache;
mod metrics;
mod raster;

#[cfg(test)]
mod tests;

pub use crate::pixels::ReadyImage;
pub use metrics::MathEm;
pub use raster::{paint_ready, raster};

#[derive(Clone, Copy, Debug, Default)]
pub struct MathMetricSlots {
    inline: Option<Option<MathEm>>,
    display: Option<Option<MathEm>>,
}

impl MathMetricSlots {
    fn get(self, display: bool) -> Option<Option<MathEm>> {
        if display { self.display } else { self.inline }
    }

    fn set(&mut self, display: bool, value: Option<MathEm>) {
        if display {
            self.display = Some(value);
        } else {
            self.inline = Some(value);
        }
    }

    fn remove(&mut self, display: bool) -> bool {
        if display {
            self.display.take().is_some()
        } else {
            self.inline.take().is_some()
        }
    }

    fn is_empty(self) -> bool {
        self.inline.is_none() && self.display.is_none()
    }
}

pub type MathMetrics = HashMap<String, MathMetricSlots>;

type MetricId = (String, bool);

pub fn metric(metrics: &MathMetrics, latex: &str, display: bool) -> Option<Option<MathEm>> {
    metrics.get(latex).and_then(|slots| slots.get(display))
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct MathKey {
    pub latex: String,
    pub display: bool,
    pub em_bits: u32,
    pub color_bits: u32,
    pub dpr_q: u16,
}

impl MathKey {
    pub fn font_size(&self) -> f32 {
        f32::from_bits(self.em_bits)
    }

    pub fn dpr(&self) -> f32 {
        crate::pixels::dpr_from_q(self.dpr_q)
    }
}

pub struct RasterSpec {
    pub display: bool,
    pub font_size: f32,
    pub dpr: f32,
    pub color: ThemeColor,
}

pub struct RasterOut {
    pub em: MathEm,
    pub image: Option<ReadyImage>,
}

enum Slot {
    InFlight,
    Ready(ReadyImage),
    Failed(crate::Error),
}

pub struct MathCache {
    map: HashMap<MathKey, Slot>,
    lru: VecDeque<MathKey>,

    hot: HashSet<MathKey>,

    warm: HashSet<MathKey>,
    bytes: usize,
    inflight: usize,

    metrics: Rc<MathMetrics>,

    metrics_order: VecDeque<MetricId>,
    metrics_gen: u64,
    ready: Rc<HashMap<MathKey, ReadyImage>>,
    failed: Rc<HashSet<MathKey>>,
}

pub fn key_for(latex: &str, display: bool, font_size: f32, color: ThemeColor, dpr: f64) -> MathKey {
    MathKey {
        latex: latex.to_string(),
        display,
        em_bits: font_size.to_bits(),
        color_bits: color.to_rgba_u32(),
        dpr_q: crate::pixels::dpr_q(dpr),
    }
}

pub fn spec_from_key(key: &MathKey, color: ThemeColor) -> RasterSpec {
    RasterSpec {
        display: key.display,
        font_size: key.font_size(),
        dpr: key.dpr(),
        color,
    }
}

pub fn color_for(theme: &DocumentTheme, kind: md_core::block::BlockKind) -> ThemeColor {
    theme.type_role(kind).color
}
