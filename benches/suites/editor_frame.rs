use gpui::BenchAppContext;
use md_bench::gpui_support::{FrameCase, cases, mount_shell};

#[gpui::bench(inputs = cases(), input_name = "case", group = "editor/frame", sample_size = 10)]
fn frame(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let shell = mount_shell(case, cx);
    cx.bench_renderer(shell, |_, _, cx| cx.notify());
}

gpui::bench_group!(benches, frame);
gpui::bench_main!(benches);
