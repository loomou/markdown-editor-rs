use gpui::BenchAppContext;
use md_bench::gpui_support::{FrameCase, cases, mount_shell};
use md_theme::{Density, LineBreakMode};
use std::cell::Cell;

#[gpui::bench(inputs = cases(), input_name = "case", group = "toggles/reading", sample_size = 10)]
fn reading(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let shell = mount_shell(case, cx);
    let on = Cell::new(false);
    cx.bench_renderer(shell, |shell, _window, cx| {
        let next = !on.get();
        on.set(next);
        shell.bench_set_reading(next, cx);
        cx.notify();
    });
}

#[gpui::bench(inputs = cases(), input_name = "case", group = "toggles/variant", sample_size = 10)]
fn variant(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let shell = mount_shell(case, cx);
    cx.bench_renderer(shell, |shell, _window, cx| {
        shell.bench_toggle_variant(cx);
        cx.notify();
    });
}

#[gpui::bench(inputs = cases(), input_name = "case", group = "toggles/density", sample_size = 10)]
fn density(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let shell = mount_shell(case, cx);
    let compact = Cell::new(false);
    cx.bench_renderer(shell, |shell, _window, cx| {
        let next = !compact.get();
        compact.set(next);
        let density = if next {
            Density::Compact
        } else {
            Density::Normal
        };
        shell.bench_set_density(density, cx);
        cx.notify();
    });
}

#[gpui::bench(inputs = cases(), input_name = "case", group = "toggles/line-break", sample_size = 10)]
fn line_break(case: &FrameCase, cx: &mut BenchAppContext<'_, '_>) {
    let shell = mount_shell(case, cx);
    let optimal = Cell::new(false);
    cx.bench_renderer(shell, |shell, _window, cx| {
        let next = !optimal.get();
        optimal.set(next);
        let mode = if next {
            LineBreakMode::Optimal
        } else {
            LineBreakMode::Greedy
        };
        shell.bench_set_line_break(mode, cx);
        cx.notify();
    });
}

gpui::bench_group!(benches, reading, variant, density, line_break);
gpui::bench_main!(benches);
