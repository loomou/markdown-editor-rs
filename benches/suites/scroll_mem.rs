use gpui::{AppContext, BenchAppContext, Entity};
use md_bench::gpui_support::{FrameCase, large_cases, mount_shell};
use md_bench::mem;
use md_core::Px;
use md_editor::Shell;

const WARMUP_FRAMES: usize = 8;
const STEPS: usize = 300;
const STEPS_PER_PASS: f64 = 200.0;
const SAMPLE_EVERY: usize = 20;
const TAIL: usize = 5;

#[derive(Clone, Copy)]
struct Sample {
    mem: bool,
    ws: usize,
    pagefile: usize,
    os_peak_ws: usize,
    os_peak_pagefile: usize,
    scroll: Px,
    total: Px,
    materialized: usize,
    shapes: usize,
    shape_bytes: usize,
}

impl Sample {
    fn mem_text(self) -> String {
        if self.mem {
            format!(
                "ws={} pagefile={} os-peak-ws={} os-peak-pagefile={}",
                mem::human_bytes(self.ws),
                mem::human_bytes(self.pagefile),
                mem::human_bytes(self.os_peak_ws),
                mem::human_bytes(self.os_peak_pagefile)
            )
        } else {
            "ws=n/a pagefile=n/a".to_owned()
        }
    }
}

fn sample(shell: &Entity<Shell>, cx: &BenchAppContext<'_, '_>) -> Sample {
    let (scroll, total, materialized, shapes, shape_bytes) =
        cx.read_entity(shell, |shell, app| shell.bench_frame_metrics(app));
    let counters = mem::sample();
    Sample {
        mem: counters.is_some(),
        ws: counters.map_or(0, |m| m.working_set),
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

fn median(values: &[usize]) -> usize {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn trace(label: &str, step: usize, s: &Sample) {
    eprintln!(
        "[scroll-mem {label}] step={step:>3} scroll={:.0}/{:.0} materialized={} shapes={} shape-bytes={} {}",
        s.scroll,
        s.total,
        s.materialized,
        s.shapes,
        s.shape_bytes,
        s.mem_text()
    );
}

#[gpui::bench(inputs = large_cases(), input_name = "case", group = "large/scroll-mem", sample_size = 10)]
fn scroll_mem(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let shell = mount_shell(case, cx);
    for _ in 0..WARMUP_FRAMES {
        cx.run_until_idle();
    }
    let base = sample(&shell, cx);
    eprintln!("[scroll-mem {}] base", case.label);
    trace(&case.label, 0, &base);

    let step = (base.total / STEPS_PER_PASS).max(600.0);
    let mut peak_ws = base.ws;
    let mut peak_pagefile = base.pagefile;
    let mut peak_materialized = base.materialized;
    let mut peak_shapes = base.shapes;
    let mut peak_shape_bytes = base.shape_bytes;
    let mut peak_step = 0usize;
    let mut samples: Vec<Sample> = Vec::with_capacity(STEPS);
    let mut wraps = 0usize;

    for index in 0..STEPS {
        let before = cx.read_entity(&shell, |shell, app| shell.bench_frame_metrics(app).0);
        cx.update_entity(&shell, |shell, cx| shell.bench_scroll_to(before + step, cx));
        cx.run_until_idle();
        let s = sample(&shell, cx);
        if s.ws > peak_ws {
            peak_ws = s.ws;
            peak_step = index;
        }
        peak_pagefile = peak_pagefile.max(s.pagefile);
        peak_materialized = peak_materialized.max(s.materialized);
        peak_shapes = peak_shapes.max(s.shapes);
        peak_shape_bytes = peak_shape_bytes.max(s.shape_bytes);
        if index % SAMPLE_EVERY == 0 {
            trace(&case.label, index, &s);
        }
        samples.push(s);
        if s.scroll <= before + 1.0 {
            wraps += 1;
            cx.update_entity(&shell, |shell, cx| shell.bench_scroll_to(0.0, cx));
            cx.run_until_idle();
        }
    }

    let tail = &samples[samples.len().saturating_sub(TAIL)..];
    let steady_ws = median(&tail.iter().map(|s| s.ws).collect::<Vec<_>>());
    let steady_pagefile = median(&tail.iter().map(|s| s.pagefile).collect::<Vec<_>>());
    let steady_materialized = median(&tail.iter().map(|s| s.materialized).collect::<Vec<_>>());
    let steady_shapes = median(&tail.iter().map(|s| s.shapes).collect::<Vec<_>>());

    eprintln!(
        "[scroll-mem {}] summary step_px={step:.0} wraps={wraps} base={} | peak@step{peak_step}: {} materialized={} shapes={} shape-bytes={} | steady(tail{TAIL}): ws={} pagefile={} materialized={steady_materialized} shapes={steady_shapes}",
        case.label,
        base.mem_text(),
        if base.mem {
            format!(
                "ws={} pagefile={}",
                mem::human_bytes(peak_ws),
                mem::human_bytes(peak_pagefile)
            )
        } else {
            "ws=n/a".to_owned()
        },
        peak_materialized,
        peak_shapes,
        peak_shape_bytes,
        if base.mem {
            mem::human_bytes(steady_ws)
        } else {
            "n/a".to_owned()
        },
        if base.mem {
            mem::human_bytes(steady_pagefile)
        } else {
            "n/a".to_owned()
        },
    );

    cx.bench_renderer(shell, move |shell, _window, cx| {
        shell.bench_scroll_by(step, cx);
        cx.notify();
    });
}

gpui::bench_group!(benches, scroll_mem);
gpui::bench_main!(benches);
