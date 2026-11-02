use super::Shell;
use gpui::{App, Context};
use md_core::Px;
use md_theme::{Density, LineBreakMode};

impl Shell {
    pub fn bench_set_reading(&mut self, on: bool, cx: &mut Context<'_, Self>) {
        self.set_reading(on, cx);
    }

    pub fn bench_toggle_variant(&mut self, cx: &mut App) {
        self.toggle_variant(cx);
    }

    pub fn bench_set_density(&mut self, density: Density, cx: &mut App) {
        self.settings.appearance.density = density;
        self.appearance_changed(cx);
    }

    pub fn bench_set_line_break(&mut self, mode: LineBreakMode, cx: &mut App) {
        self.settings.appearance.line_break = mode;
        self.appearance_changed(cx);
    }

    pub fn bench_scroll_to(&mut self, scroll: Px, cx: &mut Context<'_, Self>) {
        self.editor.update(cx, |view, cx| {
            view.state.scroll = scroll;
            cx.notify();
        });
    }

    pub fn bench_scroll_by(&mut self, dy: Px, cx: &mut Context<'_, Self>) {
        self.editor.update(cx, |view, cx| {
            view.state.scroll += dy;
            cx.notify();
        });
    }

    pub fn bench_frame_metrics(&self, cx: &App) -> (Px, Px, usize, usize, usize) {
        let view = self.editor.read(cx);
        let engine = view.state.incremental.as_ref();
        (
            view.state.scroll,
            engine.map_or(0.0, |engine| engine.total_height()),
            engine.map_or(0, |engine| engine.materialized_count()),
            view.state.shape_cache.entry_count(),
            view.state.shape_cache.byte_count(),
        )
    }

    pub fn bench_media_report(
        &mut self,
        cx: &mut Context<'_, Self>,
    ) -> md_content::media::MediaReport {
        self.editor.update(cx, |view, _| view.media_report())
    }

    pub fn bench_media_account(
        &mut self,
        cx: &mut Context<'_, Self>,
    ) -> md_content::media::Account {
        self.editor.update(cx, |view, _| view.media_account())
    }

    pub fn bench_open_first_mermaid(&mut self, cx: &mut Context<'_, Self>) {
        self.editor
            .update(cx, |view, cx| view.bench_open_first_mermaid(cx));
    }

    pub fn bench_report_media(&mut self, stage: &str, cx: &mut Context<'_, Self>) {
        let account = self.bench_media_account(cx);
        let mib = |bytes: usize| bytes as f64 / (1024.0 * 1024.0);
        crate::measure!(
            "media {stage} display={:.2} source={:.2} mermaid={:.2} math={:.2} zoom={:.2} cache={:.2} resident={:.2} untracked={:.2}",
            mib(account.display_bytes),
            mib(account.source_bytes),
            mib(account.mermaid_bytes),
            mib(account.math_bytes),
            mib(account.zoom_bytes),
            mib(account.cache_bytes),
            mib(account.resident_bytes),
            mib(account.untracked_bytes()),
        );
    }
}
