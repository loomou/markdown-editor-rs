use super::source::{data_cache_key, resolve};
use super::{DisplayKey, Resolved};
use crate::pixels::{WIDTH_QUANT, dpr_from_q, dpr_q as quantise_dpr};
use std::path::Path;

pub fn display_key(dest: &str, slot_w: f32, slot_h: f32, dpr: f64) -> DisplayKey {
    let width_q = quantized_axis(slot_w);
    let height_q = quantized_axis(slot_h);
    let dpr_q = quantise_dpr(dpr);
    DisplayKey {
        dest: dest.to_string(),
        width_q,
        height_q,
        dpr_q,
    }
}

fn quantized_axis(slot: f32) -> u32 {
    (f64::from(slot).max(0.0) / WIDTH_QUANT).floor().max(1.0) as u32
}

pub(crate) fn quantized_slot(slot_w: f32, slot_h: f32) -> (f32, f32) {
    (
        quantized_axis(slot_w) as f32 * WIDTH_QUANT as f32,
        quantized_axis(slot_h) as f32 * WIDTH_QUANT as f32,
    )
}

impl DisplayKey {
    pub fn raster_slot(&self) -> (f32, f32) {
        (
            f64::from(self.width_q.max(1)) as f32 * WIDTH_QUANT as f32,
            f64::from(self.height_q.max(1)) as f32 * WIDTH_QUANT as f32,
        )
    }

    pub(super) fn raster_dpr(&self) -> f32 {
        dpr_from_q(self.dpr_q)
    }
}

pub fn cache_key(dest: &str, source_path: Option<&Path>) -> String {
    if let Some(key) = data_cache_key(dest) {
        return key;
    }
    match resolve(dest, source_path) {
        Some(Resolved::Remote(u)) => u,
        Some(Resolved::Network(u)) => u,
        Some(Resolved::Local(p)) => p.to_string_lossy().into_owned(),
        Some(Resolved::Data { .. }) => dest.to_string(),
        None => dest.to_string(),
    }
}
