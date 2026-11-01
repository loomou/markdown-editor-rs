use gpui::{AppContext, BenchAppContext};
use md_bench::gpui_support::{FrameCase, media_cases, mount_shell};
use md_bench::mem;
use md_bench::scroll_probe::percentile;
use md_content::media::MediaReport;

const WARMUP_FRAMES: usize = 8;
const STEPS: usize = 200;
const STEP_PX: f64 = 600.0;

fn trace(label: &str, stage: &str, report: MediaReport) {
    eprintln!(
        "[media-scroll {label}] {stage} cache={} resident={} untracked={} over-budget={} budget={}",
        report.cache_bytes,
        report.resident_bytes,
        report.untracked_bytes(),
        report.over_budget,
        report.budget
    );
}

#[gpui::bench(inputs = media_cases(), input_name = "case", group = "media/scroll", sample_size = 10)]
fn media_scroll(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let shell = mount_shell(case, cx);
    for _ in 0..WARMUP_FRAMES {
        cx.run_until_idle();
    }
    mem::reset_peak();
    let base = cx.update_entity(&shell, |shell, cx| shell.bench_media_report(cx));
    let base_heap = mem::heap();
    let base_cpu = mem::cpu();
    trace(&case.label, "base", base);

    let (_, total, ..) = cx.read_entity(&shell, |shell, app| shell.bench_frame_metrics(app));
    let mut step_ms: Vec<f64> = Vec::with_capacity(STEPS);
    let mut peak_resident = base.resident_bytes;
    let mut peak_cache = base.cache_bytes;
    let mut peak_over = base.over_budget;
    let mut wraps = 0usize;
    let mut first_media: Option<(usize, f64)> = None;

    for step in 0..STEPS {
        let before = cx.read_entity(&shell, |shell, app| shell.bench_frame_metrics(app).0);
        let started = std::time::Instant::now();
        cx.update_entity(&shell, |shell, cx| {
            shell.bench_scroll_to(before + STEP_PX, cx)
        });
        cx.run_until_idle();
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        step_ms.push(elapsed);

        let report = cx.update_entity(&shell, |shell, cx| shell.bench_media_report(cx));
        peak_cache = peak_cache.max(report.cache_bytes);
        peak_resident = peak_resident.max(report.resident_bytes);
        peak_over = peak_over.max(report.over_budget);
        if first_media.is_none() && report.resident_bytes > base.resident_bytes {
            first_media = Some((step, elapsed));
        }

        let after = cx.read_entity(&shell, |shell, app| shell.bench_frame_metrics(app).0);
        if after <= before + 1.0 {
            wraps += 1;
            cx.update_entity(&shell, |shell, cx| shell.bench_scroll_to(0.0, cx));
            cx.run_until_idle();
        }
    }

    let end = cx.update_entity(&shell, |shell, cx| shell.bench_media_report(cx));
    let heap = mem::heap();
    let cpu = match (base_cpu, mem::cpu()) {
        (Some(before), Some(after)) => before.since(after),
        _ => f64::NAN,
    };
    let first = match first_media {
        Some((step, ms)) => format!("step{step}={ms:.2}ms"),
        None => "none".to_owned(),
    };
    eprintln!(
        "[media-scroll {}] summary steps={STEPS} step_px={STEP_PX:.0} wraps={wraps} total={total:.0} step-ms p50={:.2} p95={:.2} p99={:.2} max={:.2} | first-media {first} | heap base={} peak={} | cpu={:.1}ms | media peak cache={peak_cache} resident={peak_resident} over-budget={peak_over} | end cache={} resident={}",
        case.label,
        percentile(&step_ms, 0.5),
        percentile(&step_ms, 0.95),
        percentile(&step_ms, 0.99),
        percentile(&step_ms, 1.0),
        mem::format_bytes(base_heap.live),
        mem::format_bytes(heap.peak),
        cpu,
        end.cache_bytes,
        end.resident_bytes,
    );

    cx.bench_renderer(shell, |_, _, cx| cx.notify());
}

gpui::bench_group!(benches, media_scroll);
gpui::bench_main!(benches);
