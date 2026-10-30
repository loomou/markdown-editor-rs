use gpui::BenchAppContext;
use md_bench::gpui_support::{media_cases, mount_shell, FrameCase};
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
    cx.update(|app| md_content::media::MediaCoordinator::reset_peak(app));
    let shell = mount_shell(case, cx);
    let pass = run_pass(&shell, cx, &case.label, STEPS, case.dwell_ms);
    summarize(&case.label, &pass);

    cx.bench_renderer(shell, move |shell, _window, cx| {
        shell.bench_scroll_by(pass.step, cx);
        cx.notify();
    });
}

gpui::bench_group!(benches, media_scroll);
gpui::bench_main!(benches);
