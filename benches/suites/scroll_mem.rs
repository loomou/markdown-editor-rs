use gpui::{AppContext, BenchAppContext};
use md_bench::gpui_support::{FrameCase, large_cases, mount_shell};
use md_bench::mem;
use md_bench::scroll_probe::{Sample, median, sample};

const WARMUP_FRAMES: usize = 8;
const STEPS: usize = 300;
const STEPS_PER_PASS: f64 = 200.0;
const SAMPLE_EVERY: usize = 20;
const TAIL: usize = 5;

fn scroll_mem_cases() -> Vec<FrameCase> {
    trace_floor("before-corpora");
    large_cases()
}

fn trace_floor(stage: &str) {
    let counters = mem::sample();
    eprintln!(
        "[scroll-mem floor {stage}] ws={} pagefile={} private-ws={}",
        counters.map_or_else(|| "n/a".to_owned(), |m| mem::format_bytes(m.working_set)),
        counters.map_or_else(|| "n/a".to_owned(), |m| mem::format_bytes(m.pagefile)),
        mem::private_working_set().map_or_else(|| "n/a".to_owned(), mem::format_bytes),
    );
}

fn bytes(value: usize, known: bool) -> String {
    if known {
        mem::format_bytes(value)
    } else {
        "n/a".to_owned()
    }
}

fn mem_text(s: &Sample) -> String {
    if s.mem {
        format!(
            "ws={} private-ws={} pagefile={} os-peak-ws={} os-peak-pagefile={}",
            mem::format_bytes(s.ws),
            bytes(s.private, s.has_private),
            mem::format_bytes(s.pagefile),
            mem::format_bytes(s.os_peak_ws),
            mem::format_bytes(s.os_peak_pagefile)
        )
    } else {
        "ws=n/a pagefile=n/a".to_owned()
    }
}

fn trace(label: &str, step: usize, s: &Sample) {
    eprintln!(
        "[scroll-mem {label}] step={step:>3} scroll={:.0}/{:.0} materialized={} shapes={} shape-bytes={} {}",
        s.scroll,
        s.total,
        s.materialized,
        s.shapes,
        s.shape_bytes,
        mem_text(s)
    );
}

#[gpui::bench(inputs = scroll_mem_cases(), input_name = "case", group = "large/scroll-mem", sample_size = 10)]
fn scroll_mem(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let shell = mount_shell(case, cx);
    for _ in 0..WARMUP_FRAMES {
        cx.run_until_idle();
    }
    let base = sample(&shell, cx, true);
    eprintln!("[scroll-mem {}] base", case.label);
    trace(&case.label, 0, &base);

    let step = (base.total / STEPS_PER_PASS).max(600.0);
    let mut peak_ws = base.ws;
    let mut peak_private = base.private;
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
        let deep = index % SAMPLE_EVERY == 0 || index + TAIL >= STEPS;
        let s = sample(&shell, cx, deep);
        if s.ws > peak_ws {
            peak_ws = s.ws;
            peak_step = index;
        }
        if s.has_private {
            peak_private = peak_private.max(s.private);
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
    let steady_private = median(&tail.iter().map(|s| s.private).collect::<Vec<_>>());
    let steady_pagefile = median(&tail.iter().map(|s| s.pagefile).collect::<Vec<_>>());
    let steady_materialized = median(&tail.iter().map(|s| s.materialized).collect::<Vec<_>>());
    let steady_shapes = median(&tail.iter().map(|s| s.shapes).collect::<Vec<_>>());

    let peak_mem = if base.mem {
        format!(
            "ws={} private-ws={} pagefile={}",
            mem::format_bytes(peak_ws),
            bytes(peak_private, base.has_private),
            mem::format_bytes(peak_pagefile)
        )
    } else {
        "ws=n/a".to_owned()
    };
    let steady_mem = if base.mem {
        format!(
            "ws={} private-ws={} pagefile={}",
            mem::format_bytes(steady_ws),
            bytes(steady_private, base.has_private),
            mem::format_bytes(steady_pagefile)
        )
    } else {
        "ws=n/a".to_owned()
    };

    eprintln!(
        "[scroll-mem {}] summary step_px={step:.0} wraps={wraps} base={} | peak@step{peak_step}: {peak_mem} materialized={peak_materialized} shapes={peak_shapes} shape-bytes={peak_shape_bytes} | steady(tail{TAIL}): {steady_mem} materialized={steady_materialized} shapes={steady_shapes}",
        case.label,
        mem_text(&base),
    );

    cx.bench_renderer(shell, move |shell, _window, cx| {
        shell.bench_scroll_by(step, cx);
        cx.notify();
    });
}

gpui::bench_group!(benches, scroll_mem);
gpui::bench_main!(benches);
