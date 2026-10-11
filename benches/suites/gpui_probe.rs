use gpui::{BenchAppContext, Context, IntoElement, Render, Styled as _, Window, div, px};

struct ProbeView;

impl Render for ProbeView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        div().w(px(64.)).h(px(64.))
    }
}

#[gpui::bench]
fn empty_window(cx: &mut BenchAppContext<'_, '_>) {
    let mut window = cx.add_empty_window();
    let view = window.update(|window, cx| window.replace_root(cx, |_window, _cx| ProbeView));
    cx.bench_renderer(view, |_view, _window, cx| cx.notify());
}

gpui::bench_group!(benches, empty_window);
gpui::bench_main!(benches);
