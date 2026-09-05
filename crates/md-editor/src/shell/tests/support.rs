use crate::keymap::Cmd;
use crate::shell::Shell;
use crate::shell::menu::MenuEntry::{Item, Separator, Submenu};
use crate::shell::menu::{MenuId, entries_for};
use crate::ui::theme::MONO_FONT;
use crate::ui::theme::UI_FONT;
use gpui::VisualTestContext;
use gpui::{Entity, px};
use gpui::{Font, FontStyle, FontWeight, TextRun};
use md_core::doc::Doc;
use md_core::document::{editor_options, load_markdown};

pub(super) fn test_doc() -> Doc {
    Doc::new(load_markdown(
        "# Heading\n\nneedle appears twice: needle\n",
        editor_options(),
    ))
}

pub(super) fn stop_blink(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    cx.update(|_, app| {
        let editor = shell.read(app).editor.clone();
        editor.update(app, |editor, _| editor.stop_blink());
    });
}

pub(super) fn temp_settings_path(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let mut p = std::env::temp_dir();
    p.push(format!(
        "md-test-shell-set-{tag}-{}-{n}",
        std::process::id()
    ));
    p.push("settings.json");
    p
}

pub(super) fn key_for(cmd: Cmd) -> String {
    cmd.default_chord()
        .unwrap_or_else(|| panic!("{} should come with a default chord", cmd.key()))
        .unparse()
}

pub(super) fn context_labels(in_table: bool) -> Vec<&'static str> {
    entries_for(MenuId::Context, in_table)
        .iter()
        .map(|e| match e {
            Item { label, .. } | Submenu { label, .. } => md_i18n::t_in(md_i18n::Lang::En, *label),
            Separator => "---",
        })
        .collect()
}

pub(super) fn mono_px(cx: &mut VisualTestContext, text: &str, size: f32) -> f32 {
    let run = TextRun {
        len: text.len(),
        font: Font {
            family: MONO_FONT.into(),
            features: Default::default(),
            fallbacks: None,
            weight: FontWeight(400.0),
            style: FontStyle::Normal,
        },
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    cx.update(|window, _| {
        f32::from(
            window
                .text_system()
                .shape_line(text.to_string().into(), px(size), &[run], None)
                .width,
        )
    })
}

pub(super) fn text_px(cx: &mut VisualTestContext, text: &str, size: f32) -> f32 {
    let run = TextRun {
        len: text.len(),
        font: Font {
            family: UI_FONT.into(),
            features: Default::default(),
            fallbacks: None,
            weight: FontWeight(400.0),
            style: FontStyle::Normal,
        },
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    cx.update(|window, _| {
        f32::from(
            window
                .text_system()
                .shape_line(text.to_string().into(), px(size), &[run], None)
                .width,
        )
    })
}
