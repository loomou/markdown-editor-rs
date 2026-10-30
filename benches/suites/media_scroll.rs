use gpui::BenchAppContext;
use md_bench::gpui_support::{FrameCase, media_cases, mount_shell};
use md_bench::mem;
use md_bench::scroll_probe::{run_pass, summarize};

const STEPS: usize = 300;

fn media_scroll_cases() -> Vec<FrameCase> {
    media_cases()
}

#[gpui::bench(inputs = media_scroll_cases(), input_name = "case", group = "media/scroll-mem", sample_size = 10)]
fn media_scroll(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    if std::env::var("MD_BENCH_COOLING").as_deref() == Ok("0") {
        md_content::media::set_cooling(false);
    }
    if let Some(millis) = std::env::var("MD_BENCH_COOL_GRACE")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
    {
        md_content::media::set_cool_grace(std::time::Duration::from_millis(millis));
    }
    mem::reset_peak();
    cx.update(md_content::media::MediaCoordinator::reset_peak);
    let shell = mount_shell(case, cx);
    let steps = case.steps.unwrap_or(STEPS);
    let pass = run_pass(&shell, cx, &case.label, steps, case.dwell_ms);
    summarize(&case.label, &pass);

    cx.bench_renderer(shell, move |shell, _window, cx| {
        shell.bench_scroll_by(pass.step, cx);
        cx.notify();
    });
}

gpui::bench_group!(benches, media_scroll);
gpui::bench_main!(benches);
