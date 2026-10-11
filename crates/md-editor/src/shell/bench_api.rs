use super::Shell;
use gpui::{App, Context};
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
}
