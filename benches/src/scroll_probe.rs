use gpui::{AppContext, BenchAppContext, Entity};
use md_core::Px;
use md_editor::Shell;

use crate::mem;

#[derive(Clone, Copy)]
pub struct Sample {
    pub mem: bool,
    pub ws: usize,
    pub private: usize,
    pub has_private: bool,
    pub pagefile: usize,
    pub os_peak_ws: usize,
    pub os_peak_pagefile: usize,
    pub scroll: Px,
    pub total: Px,
    pub materialized: usize,
    pub shapes: usize,
    pub shape_bytes: usize,
}

pub fn sample(shell: &Entity<Shell>, cx: &BenchAppContext<'_, '_>, deep: bool) -> Sample {
    let (scroll, total, materialized, shapes, shape_bytes) =
        cx.read_entity(shell, |shell, app| shell.bench_frame_metrics(app));
    let counters = mem::sample();
    let private = if deep {
        mem::private_working_set()
    } else {
        None
    };
    Sample {
        mem: counters.is_some(),
        ws: counters.map_or(0, |m| m.working_set),
        private: private.unwrap_or(0),
        has_private: private.is_some(),
        pagefile: counters.map_or(0, |m| m.pagefile),
        os_peak_ws: counters.map_or(0, |m| m.peak_working_set),
        os_peak_pagefile: counters.map_or(0, |m| m.peak_pagefile),
        scroll,
        total,
        materialized,
        shapes,
        shape_bytes,
    }
}

pub fn median(values: &[usize]) -> usize {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}
