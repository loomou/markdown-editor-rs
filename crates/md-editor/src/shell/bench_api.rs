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

    pub fn bench_media_account(&self, cx: &App) -> md_content::media::Account {
        self.editor.read(cx).media_ledger(cx)
    }

    pub fn bench_media_job_peak(&self, cx: &App) -> usize {
        md_content::media::MediaCoordinator::app_peak(cx)
    }

    pub fn bench_media_resident_peak(&self, cx: &App) -> usize {
        cx.try_global::<md_content::media::MediaCoordinator>()
            .map_or(0, md_content::media::MediaCoordinator::resident_peak)
    }
}
