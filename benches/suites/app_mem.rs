use gpui::{AppContext, BenchAppContext};
use md_bench::corpus::{BLOCK_SEED, generate_blocks, human_blocks};
use md_bench::gpui_support::mount_shell_text;
use md_bench::mem;
use md_bench::scroll_probe::{Sample, median, sample};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

const WARMUP_FRAMES: usize = 8;
const STEPS: usize = 300;
const STEPS_PER_PASS: f64 = 200.0;
const SAMPLE_EVERY: usize = 20;
const TAIL: usize = 5;
const DEFAULT_TIER: usize = 100_000;

static MEASURED: AtomicBool = AtomicBool::new(false);

struct AppCase {
    label: String,
    text: String,
}

impl fmt::Display for AppCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

fn parse_tier(raw: &str) -> Option<usize> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if let Some(head) = trimmed.strip_suffix('k') {
        head.trim()
            .parse::<usize>()
            .ok()
            .map(|n| n.saturating_mul(1_000))
    } else {
        trimmed.parse::<usize>().ok()
    }
}

fn selected_tier() -> usize {
    std::env::var("MD_BENCH_APP_TIER")
        .ok()
        .and_then(|raw| parse_tier(&raw))
        .filter(|blocks| *blocks > 0)
        .unwrap_or(DEFAULT_TIER)
}

fn mib(value: usize, known: bool) -> String {
    if known {
        mem::format_mib(value)
    } else {
        "n/a".to_owned()
    }
}

fn emit(
    label: &str,
    stage: &str,
    private: Option<usize>,
    ws: Option<usize>,
    pagefile: Option<usize>,
) {
    eprintln!(
        "[app-mem {label}] {stage:<14} private={} ws={} pagefile={}",
        private.map_or_else(|| "n/a".to_owned(), mem::format_mib),
        ws.map_or_else(|| "n/a".to_owned(), mem::format_mib),
        pagefile.map_or_else(|| "n/a".to_owned(), mem::format_mib),
    );
}

fn emit_now(label: &str, stage: &str) {
    let counters = mem::sample();
    emit(
        label,
        stage,
        mem::private_working_set(),
        counters.map(|m| m.working_set),
        counters.map(|m| m.pagefile),
    );
}

fn emit_sample(label: &str, stage: &str, s: &Sample) {
    let private = if s.has_private { Some(s.private) } else { None };
    emit(label, stage, private, Some(s.ws), Some(s.pagefile));
}

fn app_mem_cases() -> Vec<AppCase> {
    let tier = selected_tier();
    let label = human_blocks(tier);
    emit_now(&label, "floor");
    let text = generate_blocks(tier, BLOCK_SEED);
    emit_now(&label, "text");
    vec![AppCase {
        label: format!("blocks-{label}"),
        text,
    }]
}

#[gpui::bench(inputs = app_mem_cases(), input_name = "case", group = "app/scroll-mem", sample_size = 10)]
fn app_mem(case: &AppCase, cx: &mut BenchAppContext<'_, '_>) {
    let label = human_blocks(selected_tier());
    let first = !MEASURED.swap(true, Ordering::Relaxed);

    if first {
        emit_now(&label, "app");
    }

    let shell = mount_shell_text(&case.text, cx);
    for _ in 0..WARMUP_FRAMES {
        cx.run_until_idle();
    }
    let base = sample(&shell, cx, true);
    let step = (base.total / STEPS_PER_PASS).max(600.0);

    if first {
        emit_sample(&label, "base", &base);

        let mut peak_private = base.private;
        let mut peak_ws = base.ws;
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
            samples.push(s);
            if s.scroll <= before + 1.0 {
                wraps += 1;
                cx.update_entity(&shell, |shell, cx| shell.bench_scroll_to(0.0, cx));
                cx.run_until_idle();
            }
        }

        let tail = &samples[samples.len().saturating_sub(TAIL)..];
        let steady_private = median(&tail.iter().map(|s| s.private).collect::<Vec<_>>());
        let steady_ws = median(&tail.iter().map(|s| s.ws).collect::<Vec<_>>());

        eprintln!(
            "[app-mem {label}] summary step_px={step:.0} wraps={wraps} cold-open private={} ws={} | scroll-peak@step{peak_step} private={} ws={} (delta-private={}) | steady private={} ws={}",
            mib(base.private, base.has_private),
            mem::format_mib(base.ws),
            mib(peak_private, base.has_private),
            mem::format_mib(peak_ws),
            mib(peak_private.saturating_sub(base.private), base.has_private),
            mib(steady_private, base.has_private),
            mem::format_mib(steady_ws),
        );
    }

    cx.bench_renderer(shell, move |shell, _window, cx| {
        shell.bench_scroll_by(step, cx);
        cx.notify();
    });
}

gpui::bench_group!(benches, app_mem);
gpui::bench_main!(benches);
