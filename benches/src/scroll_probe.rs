use gpui::{AppContext, BenchAppContext, Entity};
use md_core::Px;
use md_editor::Shell;

use crate::mem;

pub const WARMUP_FRAMES: usize = 8;
pub const STEPS_PER_PASS: f64 = 200.0;
pub const SAMPLE_EVERY: usize = 20;
pub const TAIL: usize = 5;

#[derive(Clone, Copy)]
pub struct Sample {
    pub mem: bool,
    pub ws: usize,
    pub private: usize,
    pub has_private: bool,
    pub pagefile: usize,
    pub os_peak_ws: usize,
    pub os_peak_pagefile: usize,
    pub heap: usize,
    pub heap_peak: usize,
    pub media: md_content::media::Account,
    pub job_peak: usize,
    pub scroll: Px,
    pub total: Px,
    pub materialized: usize,
    pub shapes: usize,
    pub shape_bytes: usize,
}

pub fn sample(shell: &Entity<Shell>, cx: &BenchAppContext<'_, '_>, deep: bool) -> Sample {
    let (scroll, total, materialized, shapes, shape_bytes) =
        cx.read_entity(shell, |shell, app| shell.bench_frame_metrics(app));
    let media = cx.read_entity(shell, |shell, app| shell.bench_media_account(app));
    let job_peak = cx.read_entity(shell, |shell, app| shell.bench_media_job_peak(app));
    let counters = mem::sample();
    let private = if deep {
        mem::private_working_set()
    } else {
        None
    };
    let heap = mem::heap();
    Sample {
        mem: counters.is_some(),
        ws: counters.map_or(0, |m| m.working_set),
        private: private.unwrap_or(0),
        has_private: private.is_some(),
        pagefile: counters.map_or(0, |m| m.pagefile),
        os_peak_ws: counters.map_or(0, |m| m.peak_working_set),
        os_peak_pagefile: counters.map_or(0, |m| m.peak_pagefile),
        heap: heap.live,
        heap_peak: heap.peak,
        media,
        job_peak,
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

pub struct Pass {
    pub base: Sample,
    pub peak_ws: usize,
    pub peak_private: usize,
    pub peak_pagefile: usize,
    pub peak_heap: usize,
    pub peak_job: usize,
    pub peak_step: usize,
    pub steady: Sample,
    pub wraps: usize,
    pub step: f64,
}

pub fn run_pass(
    shell: &Entity<Shell>,
    cx: &mut BenchAppContext<'_, '_>,
    label: &str,
    steps: usize,
) -> Pass {
    for _ in 0..WARMUP_FRAMES {
        cx.run_until_idle();
    }
    let base = sample(shell, cx, true);
    eprintln!("[media-scroll {label}] base {}", describe(&base));

    let step = (base.total / STEPS_PER_PASS).max(600.0);
    let mut peak_ws = base.ws;
    let mut peak_private = base.private;
    let mut peak_pagefile = base.pagefile;
    let mut peak_heap = base.heap_peak;
    let mut peak_job = base.job_peak;
    let mut peak_step = 0usize;
    let mut wraps = 0usize;
    let mut samples: Vec<Sample> = Vec::with_capacity(steps);

    for index in 0..steps {
        let before = cx.read_entity(shell, |shell, app| shell.bench_frame_metrics(app).0);
        cx.update_entity(shell, |shell, cx| shell.bench_scroll_to(before + step, cx));
        cx.run_until_idle();
        let deep = index % SAMPLE_EVERY == 0 || index + TAIL >= steps;
        let s = sample(shell, cx, deep);
        if s.ws > peak_ws {
            peak_ws = s.ws;
            peak_step = index;
        }
        if s.has_private {
            peak_private = peak_private.max(s.private);
        }
        peak_pagefile = peak_pagefile.max(s.pagefile);
        peak_heap = peak_heap.max(s.heap_peak);
        peak_job = peak_job.max(s.job_peak);
        if index % SAMPLE_EVERY == 0 {
            eprintln!("[media-scroll {label}] step={index:>3} {}", describe(&s));
        }
        samples.push(s);
        if s.scroll <= before + 1.0 {
            wraps += 1;
            cx.update_entity(shell, |shell, cx| shell.bench_scroll_to(0.0, cx));
            cx.run_until_idle();
        }
    }

    let tail = &samples[samples.len().saturating_sub(TAIL)..];
    let steady = Sample {
        mem: base.mem,
        ws: median(&tail.iter().map(|s| s.ws).collect::<Vec<_>>()),
        private: median(&tail.iter().map(|s| s.private).collect::<Vec<_>>()),
        has_private: base.has_private,
        pagefile: median(&tail.iter().map(|s| s.pagefile).collect::<Vec<_>>()),
        os_peak_ws: base.os_peak_ws,
        os_peak_pagefile: base.os_peak_pagefile,
        heap: median(&tail.iter().map(|s| s.heap).collect::<Vec<_>>()),
        heap_peak: peak_heap,
        media: tail.last().map_or(Default::default(), |s| s.media),
        job_peak: peak_job,
        scroll: tail.last().map_or(0.0, |s| s.scroll),
        total: base.total,
        materialized: median(&tail.iter().map(|s| s.materialized).collect::<Vec<_>>()),
        shapes: median(&tail.iter().map(|s| s.shapes).collect::<Vec<_>>()),
        shape_bytes: median(&tail.iter().map(|s| s.shape_bytes).collect::<Vec<_>>()),
    };

    Pass {
        base,
        peak_ws,
        peak_private,
        peak_pagefile,
        peak_heap,
        peak_job,
        peak_step,
        steady,
        wraps,
        step,
    }
}

pub fn account_text(a: &md_content::media::Account) -> String {
    format!(
        "display={}/{} source={}/{} mermaid={}/{} math={}/{} zoom={} total={} \
         resident={}/{} refused={} gpu-pending={} gpu-waited={} gpu-direct={}",
        mem::format_bytes(a.display_bytes),
        a.display_entries,
        mem::format_bytes(a.source_bytes),
        a.source_entries,
        mem::format_bytes(a.mermaid_bytes),
        a.mermaid_entries,
        mem::format_bytes(a.math_bytes),
        a.math_entries,
        mem::format_bytes(a.zoom_bytes),
        mem::format_bytes(a.media_bytes()),
        mem::format_bytes(a.resident_bytes),
        mem::format_bytes(a.resident_limit),
        a.refused(),
        a.gpu_pending,
        a.gpu_released_after_wait,
        a.gpu_released_immediately,
    )
}

pub fn describe(s: &Sample) -> String {
    if !s.mem {
        return "ws=n/a pagefile=n/a".to_owned();
    }
    let private = if s.has_private {
        mem::format_bytes(s.private)
    } else {
        "n/a".to_owned()
    };
    format!(
        "ws={} private-ws={} pagefile={} rust-heap={} shapes={} shape-bytes={} materialized={} job-peak={} media[{}] scroll={:.0}/{:.0}",
        mem::format_bytes(s.ws),
        private,
        mem::format_bytes(s.pagefile),
        mem::format_bytes(s.heap),
        s.shapes,
        mem::format_bytes(s.shape_bytes),
        s.materialized,
        mem::format_bytes(s.job_peak),
        account_text(&s.media),
        s.scroll,
        s.total,
    )
}

pub fn summarize(label: &str, p: &Pass) {
    let private = if p.base.has_private {
        mem::format_bytes(p.peak_private)
    } else {
        "n/a".to_owned()
    };
    let steady_private = if p.steady.has_private {
        mem::format_bytes(p.steady.private)
    } else {
        "n/a".to_owned()
    };
    eprintln!(
        "[media-scroll {label}] summary step_px={:.0} wraps={} | peak@step{}: ws={} private-ws={} pagefile={} rust-heap={} job-peak={} | steady: ws={} private-ws={} pagefile={} rust-heap={} shapes={} shape-bytes={} materialized={} job-peak={} | media: {}",
        p.step,
        p.wraps,
        p.peak_step,
        mem::format_bytes(p.peak_ws),
        private,
        mem::format_bytes(p.peak_pagefile),
        mem::format_bytes(p.peak_heap),
        mem::format_bytes(p.peak_job),
        mem::format_bytes(p.steady.ws),
        steady_private,
        mem::format_bytes(p.steady.pagefile),
        mem::format_bytes(p.steady.heap),
        p.steady.shapes,
        mem::format_bytes(p.steady.shape_bytes),
        p.steady.materialized,
        mem::format_bytes(p.steady.job_peak),
        account_text(&p.steady.media),
    );
}
