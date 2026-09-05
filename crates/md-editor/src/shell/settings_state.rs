use super::Shell;
use crate::store::settings::SettingsStore;
use gpui::{App, Context, Pixels};
use md_theme::{Appearance, ThemeVariant};
use std::path::PathBuf;

pub(super) fn font_size_fraction(size_px: f32) -> f32 {
    let span = Appearance::MAX_BODY_SIZE_PX - Appearance::MIN_BODY_SIZE_PX;
    ((size_px - Appearance::MIN_BODY_SIZE_PX) / span).clamp(0.0, 1.0)
}

fn mtime_of(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

pub(super) fn font_size_at(x: Pixels, track: gpui::Bounds<Pixels>) -> f32 {
    let width = f32::from(track.size.width);
    if width <= 0.0 {
        return Appearance::MIN_BODY_SIZE_PX;
    }
    let frac = ((f32::from(x) - f32::from(track.origin.x)) / width).clamp(0.0, 1.0);
    let span = Appearance::MAX_BODY_SIZE_PX - Appearance::MIN_BODY_SIZE_PX;
    (Appearance::MIN_BODY_SIZE_PX + frac * span).round()
}

impl Shell {
    pub(super) fn load_settings(&mut self, cx: &mut Context<'_, Self>) {
        self.adopt_settings_store(SettingsStore::discover(), cx);
    }

    pub(super) fn adopt_settings_store(
        &mut self,
        store: Option<SettingsStore>,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(store) = store else {
            return;
        };
        if let Some(settings) = store.load() {
            let autosave = settings.autosave;
            let remote_images = settings.remote_images;
            self.settings = settings;
            self.sync_editor_theme(cx);
            self.sync_editor_keymap(cx);
            self.editor.update(cx, |editor, cx| {
                editor.set_autosave(autosave);
                editor.set_remote_images(remote_images, cx);
            });
        }
        self.settings_mtime = mtime_of(store.path());
        self.settings_store = Some(store);
    }

    pub(super) fn reload_settings_if_changed(&mut self, cx: &mut Context<'_, Self>) {
        let Some(store) = &self.settings_store else {
            return;
        };
        let now = mtime_of(store.path());
        if now.is_none() || now == self.settings_mtime {
            return;
        }
        let Some(settings) = store.load() else {
            self.settings_mtime = now;
            return;
        };
        self.settings_mtime = now;
        if settings == self.settings {
            return;
        }
        let autosave = settings.autosave;
        let remote_images = settings.remote_images;
        self.settings = settings;

        self.color_picker = None;
        self.sync_editor_theme(cx);
        self.sync_editor_keymap(cx);
        self.editor.update(cx, |editor, cx| {
            editor.set_autosave(autosave);
            editor.set_remote_images(remote_images, cx);
        });
        cx.notify();
    }

    pub(super) fn appearance_changed(&mut self, cx: &mut App) {
        self.sync_editor_theme(cx);
        self.save_settings();
    }

    pub(super) fn settings_file_for_open(&mut self) -> Option<PathBuf> {
        let store = self.settings_store.as_ref()?;
        if store.path().exists() {
            return Some(store.path().to_path_buf());
        }
        if let Err(e) = store.save(&self.settings) {
            tracing::warn!(path = %store.path().display(), error = %e, "settings seed failed");
            return None;
        }
        let path = store.path().to_path_buf();

        self.settings_mtime = mtime_of(&path);
        Some(path)
    }

    pub(super) fn save_settings(&mut self) {
        let Some(store) = &self.settings_store else {
            return;
        };
        if let Err(e) = store.save(&self.settings) {
            tracing::warn!(path = %store.path().display(), error = %e, "settings save failed");
        }
        self.settings_mtime = mtime_of(store.path());
    }

    pub(super) fn is_dark(&self) -> bool {
        self.settings.appearance.variant == ThemeVariant::OneDark
    }

    pub(super) fn toggle_variant(&mut self, cx: &mut App) {
        self.settings.appearance.variant = if self.is_dark() {
            ThemeVariant::OneLight
        } else {
            ThemeVariant::OneDark
        };
        self.appearance_changed(cx);
    }

    pub(super) fn sync_editor_theme(&self, cx: &mut App) {
        let theme = self.settings.appearance.document_theme();
        self.editor
            .update(cx, |editor, cx| editor.set_theme(theme, cx));
    }

    pub(super) fn sync_editor_keymap(&self, cx: &mut App) {
        let keymap = self.settings.keymap.clone();
        self.editor
            .update(cx, |editor, cx| editor.set_keymap(keymap, cx));
    }
}
