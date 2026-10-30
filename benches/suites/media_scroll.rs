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
    mem::reset_peak();
    let shell = mount_shell(case, cx);
    let pass = run_pass(&shell, cx, &case.label, STEPS);
    summarize(&case.label, &pass);

    cx.bench_renderer(shell, move |shell, _window, cx| {
        shell.bench_scroll_by(pass.step, cx);
        cx.notify();
    });
}

gpui::bench_group!(benches, media_scroll);
gpui::bench_main!(benches);
