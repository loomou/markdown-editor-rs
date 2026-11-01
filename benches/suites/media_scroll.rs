use gpui::{AppContext, BenchAppContext};
use md_bench::gpui_support::{FrameCase, media_cases, mount_shell};
use md_bench::mem;
use md_bench::scroll_probe::percentile;
use md_content::media::Account;

const WARMUP_FRAMES: usize = 8;
const STEPS: usize = 200;
const STEP_PX: f64 = 600.0;

fn account_text(a: Account) -> String {
    format!(
        "display={}/{} source={}/{} mermaid={}/{} sealed-svg={} math={}/{} zoom={} cache={} resident={} untracked={} over-budget={}",
        mem::format_bytes(a.display_bytes),
        a.display_entries,
        mem::format_bytes(a.source_bytes),
        a.source_entries,
        mem::format_bytes(a.mermaid_bytes),
        a.mermaid_entries,
        mem::format_bytes(a.mermaid_svg_bytes),
        mem::format_bytes(a.math_bytes),
        a.math_entries,
        mem::format_bytes(a.zoom_bytes),
        mem::format_bytes(a.cache_bytes),
        mem::format_bytes(a.resident_bytes),
        mem::format_bytes(a.untracked_bytes()),
        mem::format_bytes(a.over_budget),
    )
}

fn trace(label: &str, stage: &str, account: Account) {
    eprintln!("[media-scroll {label}] {stage} {}", account_text(account));
}

#[gpui::bench(inputs = media_cases(), input_name = "case", group = "media/scroll", sample_size = 10)]
fn media_scroll(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let shell = mount_shell(case, cx);
    for _ in 0..WARMUP_FRAMES {
        cx.run_until_idle();
    }
    mem::reset_peak();
    let base = cx.update_entity(&shell, |shell, cx| shell.bench_media_account(cx));
    let base_heap = mem::heap();
    let base_mem = mem::sample();
    let base_cpu = mem::cpu();
    trace(&case.label, "base", base);

    let (_, total, materialized, ..) =
        cx.read_entity(&shell, |shell, app| shell.bench_frame_metrics(app));
    let mut step_ms: Vec<f64> = Vec::with_capacity(STEPS);
    let mut peak = base;
    let mut peak_step = 0usize;
    let mut peak_ws = base_mem.map_or(0, |m| m.working_set);
    let mut peak_pagefile = base_mem.map_or(0, |m| m.pagefile);
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

        let account = cx.update_entity(&shell, |shell, cx| shell.bench_media_account(cx));
        if account.resident_bytes > peak.resident_bytes {
            peak = account;
            peak_step = step;
        }
        if first_media.is_none() && account.resident_bytes > base.resident_bytes {
            first_media = Some((step, elapsed));
        }
        if let Some(memory) = mem::sample() {
            peak_ws = peak_ws.max(memory.working_set);
            peak_pagefile = peak_pagefile.max(memory.pagefile);
        }

        let after = cx.read_entity(&shell, |shell, app| shell.bench_frame_metrics(app).0);
        if after <= before + 1.0 {
            wraps += 1;
            cx.update_entity(&shell, |shell, cx| shell.bench_scroll_to(0.0, cx));
            cx.run_until_idle();
        }
    }

    let end = cx.update_entity(&shell, |shell, cx| shell.bench_media_account(cx));
    let heap = mem::heap();
    let private = mem::private_working_set().unwrap_or(0);
    let cpu = match (base_cpu, mem::cpu()) {
        (Some(before), Some(after)) => before.since(after),
        _ => f64::NAN,
    };
    let first = match first_media {
        Some((step, ms)) => format!("step{step}={ms:.2}ms"),
        None => "none".to_owned(),
    };
    eprintln!(
        "[media-scroll {}] summary steps={STEPS} step_px={STEP_PX:.0} wraps={wraps} total={total:.0} materialized={materialized} step-ms p50={:.2} p95={:.2} p99={:.2} max={:.2} | first-media {first} | heap base={} peak={} | cpu={:.1}ms | ws base={} peak={} private-ws={} pagefile base={} peak={} | media peak@step{peak_step} [{}]",
        case.label,
        percentile(&step_ms, 0.5),
        percentile(&step_ms, 0.95),
        percentile(&step_ms, 0.99),
        percentile(&step_ms, 1.0),
        mem::format_bytes(base_heap.live),
        mem::format_bytes(heap.peak),
        cpu,
        mem::format_bytes(base_mem.map_or(0, |m| m.working_set)),
        mem::format_bytes(peak_ws),
        mem::format_bytes(private),
        mem::format_bytes(base_mem.map_or(0, |m| m.pagefile)),
        mem::format_bytes(peak_pagefile),
        account_text(peak),
    );
    eprintln!("[media-scroll {}] end [{}]", case.label, account_text(end));

    cx.bench_renderer(shell, |_, _, cx| cx.notify());
}

gpui::bench_group!(benches, media_scroll);
gpui::bench_main!(benches);
