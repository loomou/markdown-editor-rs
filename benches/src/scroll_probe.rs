use gpui::{AppContext, BenchAppContext, Entity};
use md_core::Px;
use md_editor::Shell;

use crate::gpui_support::VIEWPORT_H;
use crate::mem;

pub const WARMUP_FRAMES: usize = 8;
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
    pub dwell_ms: u64,
    /// Process CPU for the whole pass, warmup excluded. Background media work
    /// runs on the app's executors, so this is the only number that covers what
    /// cooling costs; the main thread alone would show a saving that does not
    /// exist.
    pub cpu_ms: f64,
    /// The part of `cpu_ms` spent inside `dwell`, where frames are painted but
    /// nothing scrolls. A grace that only pays off in a pause has to be charged
    /// here rather than hidden in the total.
    pub cpu_dwell_ms: f64,
    /// Wall time of one scroll step, from setting the offset to the app being
    /// idle again. This is not a frame time: the harness drains every pending
    /// media job before it returns, so the number includes work a real window
    /// would have done after the frame was presented. It is still the quantity
    /// that says whether a grace makes scrolling more expensive.
    pub step_p50: f64,
    pub step_p95: f64,
    pub step_p99: f64,
    pub step_max: f64,
    pub steps: usize,
}

/// Nearest-rank percentile over an already sorted slice.
fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    sorted[index]
}

/// Keep painting without scrolling. A frame that does not scroll still runs the
/// schedule, which is what lets an entry that has been off screen for the grace
/// become a cooling candidate; a pass that only ever scrolls never gives one
/// that long.
fn dwell(shell: &Entity<Shell>, cx: &mut BenchAppContext<'_, '_>, millis: u64) {
    if millis == 0 {
        return;
    }
    let until = std::time::Instant::now() + std::time::Duration::from_millis(millis);
    while std::time::Instant::now() < until {
        cx.update_entity(shell, |_shell, cx| cx.notify());
        cx.run_until_idle();
    }
}

pub fn run_pass(
    shell: &Entity<Shell>,
    cx: &mut BenchAppContext<'_, '_>,
    label: &str,
    steps: usize,
    dwell_ms: u64,
) -> Pass {
    for _ in 0..WARMUP_FRAMES {
        cx.run_until_idle();
    }
    let base = sample(shell, cx, true);
    eprintln!("[media-scroll {label}] base {}", describe(&base));

    // One step is one viewport. A longer step would skip screens, and a cache
    // only ever sees the screens that were built, so a long document would be
    // measured on a fraction of its media. A document that needs more steps to
    // traverse asks for them instead of scrolling faster.
    let step = VIEWPORT_H;
    let mut peak_ws = base.ws;
    let mut peak_private = base.private;
    let mut peak_pagefile = base.pagefile;
    let mut peak_heap = base.heap_peak;
    let mut peak_job = base.job_peak;
    let mut peak_step = 0usize;
    let mut wraps = 0usize;
    let mut samples: Vec<Sample> = Vec::with_capacity(steps);
    let cpu_start = mem::cpu();
    let mut cpu_dwell_ms = 0.0;
    let mut step_ms: Vec<f64> = Vec::with_capacity(steps);

    for index in 0..steps {
        let before = cx.read_entity(shell, |shell, app| shell.bench_frame_metrics(app).0);
        let step_started = std::time::Instant::now();
        cx.update_entity(shell, |shell, cx| shell.bench_scroll_to(before + step, cx));
        cx.run_until_idle();
        step_ms.push(step_started.elapsed().as_secs_f64() * 1000.0);
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
            let dwell_start = mem::cpu();
            dwell(shell, cx, dwell_ms);
            if let (Some(start), Some(end)) = (dwell_start, mem::cpu()) {
                cpu_dwell_ms += start.since(end);
            }
        }
    }

    let cpu_ms = cpu_start
        .zip(mem::cpu())
        .map_or(0.0, |(start, end)| start.since(end));
    step_ms.sort_by(|left, right| left.partial_cmp(right).expect("finite"));

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
        dwell_ms,
        cpu_ms,
        cpu_dwell_ms,
        step_p50: percentile(&step_ms, 0.50),
        step_p95: percentile(&step_ms, 0.95),
        step_p99: percentile(&step_ms, 0.99),
        step_max: step_ms.last().copied().unwrap_or(0.0),
        steps: step_ms.len(),
    }
}

pub fn account_text(a: &md_content::media::Account) -> String {
    format!(
        "display={}/{} source={}/{} mermaid={}/{} math={}/{} zoom={} total={} \
         warm={}/{} warm-skipped={} warm-dropped={} jobs=display{} mermaid{} thaw{} cool{} \
         resident={}/{} refused={} (source{} display{} zoom{} mermaid{} math{} cool{} thaw{}) \
         clamped={} gpu-pending={} gpu-waited={} gpu-direct={}",
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
        mem::format_bytes(a.warm_bytes),
        a.warm_entries,
        a.warm_refused,
        a.warm_discarded,
        a.started[md_content::media::JobKind::Display.index()],
        a.started[md_content::media::JobKind::Mermaid.index()],
        a.started[md_content::media::JobKind::Thaw.index()],
        a.started[md_content::media::JobKind::Cool.index()],
        mem::format_bytes(a.resident_bytes),
        mem::format_bytes(a.resident_limit),
        a.refused(),
        a.refused_source,
        a.refused_display,
        a.refused_zoom,
        a.refused_mermaid,
        a.refused_math,
        a.refused_cool,
        a.refused_thaw,
        a.clamped,
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
        "[media-scroll {label}] summary step_px={:.0} wraps={} dwell_ms={} | peak@step{}: ws={} private-ws={} pagefile={} rust-heap={} job-peak={} | steady: ws={} private-ws={} pagefile={} rust-heap={} shapes={} shape-bytes={} materialized={} job-peak={} | media: {}",
        p.step,
        p.wraps,
        p.dwell_ms,
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
    eprintln!(
        "[media-scroll {label}] cost steps={} wall p50={:.3}ms p95={:.3}ms p99={:.3}ms max={:.3}ms \
         | cpu total={:.0}ms scroll={:.0}ms dwell={:.0}ms",
        p.steps,
        p.step_p50,
        p.step_p95,
        p.step_p99,
        p.step_max,
        p.cpu_ms,
        p.cpu_ms - p.cpu_dwell_ms,
        p.cpu_dwell_ms,
    );
}

/// Two or more windows scrolled in turn, so the shared coordinator is exercised
/// the way an app with two open documents exercises it: the same sources appear
/// twice, and a budget that each window could afford alone has to be shared.
pub struct WindowsPass {
    pub steady: Vec<Sample>,
    pub peak_ws: usize,
    pub peak_heap: usize,
    pub peak_job: usize,
    /// Sum of every window's cache bytes. This is what a per-window view cannot
    /// show: each account can look fine while the app holds several of them.
    pub peak_media: usize,
    pub resident_peak: usize,
    pub resident_limit: usize,
    pub wraps: usize,
    pub steps: usize,
    pub cpu_ms: f64,
    pub step_p50: f64,
    pub step_p95: f64,
    pub step_p99: f64,
}

pub fn run_windows_pass(
    shells: &[Entity<Shell>],
    cx: &mut BenchAppContext<'_, '_>,
    label: &str,
    steps: usize,
) -> WindowsPass {
    for _ in 0..WARMUP_FRAMES {
        cx.run_until_idle();
    }
    let mut peak_ws = 0usize;
    let mut peak_heap = 0usize;
    let mut peak_job = 0usize;
    let mut peak_media = 0usize;
    let mut wraps = 0usize;
    let mut step_ms: Vec<f64> = Vec::with_capacity(steps * shells.len());
    let cpu_start = mem::cpu();

    for index in 0..steps {
        for shell in shells {
            let before = cx.read_entity(shell, |shell, app| shell.bench_frame_metrics(app).0);
            let started = std::time::Instant::now();
            cx.update_entity(shell, |shell, cx| shell.bench_scroll_to(before + VIEWPORT_H, cx));
            cx.run_until_idle();
            step_ms.push(started.elapsed().as_secs_f64() * 1000.0);
            let after = cx.read_entity(shell, |shell, app| shell.bench_frame_metrics(app).0);
            if after <= before + 1.0 {
                wraps += 1;
                cx.update_entity(shell, |shell, cx| shell.bench_scroll_to(0.0, cx));
                cx.run_until_idle();
            }
        }
        if index % SAMPLE_EVERY == 0 || index + TAIL >= steps {
            let mut media = 0usize;
            for shell in shells {
                let s = sample(shell, cx, false);
                media = media.saturating_add(s.media.media_bytes());
                peak_ws = peak_ws.max(s.ws);
                peak_heap = peak_heap.max(s.heap);
                peak_job = peak_job.max(s.job_peak);
            }
            peak_media = peak_media.max(media);
            eprintln!(
                "[media-windows {label}] step={index:>3} windows={} media-sum={} {}",
                shells.len(),
                mem::format_bytes(media),
                shells
                    .first()
                    .map(|shell| describe(&sample(shell, cx, false)))
                    .unwrap_or_default(),
            );
        }
    }

    let cpu_ms = cpu_start
        .zip(mem::cpu())
        .map_or(0.0, |(start, end)| start.since(end));
    step_ms.sort_by(|left, right| left.partial_cmp(right).expect("finite"));
    let steady = shells.iter().map(|shell| sample(shell, cx, true)).collect();
    let (resident_peak, resident_limit) = shells.first().map_or((0, 0), |shell| {
        (
            cx.read_entity(shell, |shell, app| shell.bench_media_resident_peak(app)),
            cx.read_entity(shell, |shell, app| shell.bench_media_account(app).resident_limit),
        )
    });
    WindowsPass {
        steady,
        peak_ws,
        peak_heap,
        peak_job,
        peak_media,
        resident_peak,
        resident_limit,
        wraps,
        steps,
        cpu_ms,
        step_p50: percentile(&step_ms, 0.50),
        step_p95: percentile(&step_ms, 0.95),
        step_p99: percentile(&step_ms, 0.99),
    }
}

pub fn summarize_windows(label: &str, p: &WindowsPass) {
    eprintln!(
        "[media-windows {label}] summary windows={} steps={} wraps={} | peak: ws={} rust-heap={} \
         job-peak={} media-sum={} | resident peak={}/{} | cost wall p50={:.3}ms p95={:.3}ms \
         p99={:.3}ms cpu={:.0}ms",
        p.steady.len(),
        p.steps,
        p.wraps,
        mem::format_bytes(p.peak_ws),
        mem::format_bytes(p.peak_heap),
        mem::format_bytes(p.peak_job),
        mem::format_bytes(p.peak_media),
        mem::format_bytes(p.resident_peak),
        mem::format_bytes(p.resident_limit),
        p.step_p50,
        p.step_p95,
        p.step_p99,
        p.cpu_ms,
    );
    for (index, s) in p.steady.iter().enumerate() {
        eprintln!(
            "[media-windows {label}] window{index} steady: ws={} rust-heap={} media: {}",
            mem::format_bytes(s.ws),
            mem::format_bytes(s.heap),
            account_text(&s.media),
        );
    }
}
