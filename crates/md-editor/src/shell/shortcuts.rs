use super::Shell;
use crate::keymap::{Chord, Cmd, Mods, Refusal};
use crate::ui::theme::{MONO_FONT, ShellTheme};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, Entity, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, Stateful, StatefulInteractiveElement, Styled, Window, div, px,
};
use md_i18n::{Key, t as t18};

impl Shell {
    #[cfg(test)]
    pub(crate) fn rebind(&mut self, cmd: Cmd, text: &str, cx: &mut Context<'_, Self>) {
        let chord = Chord::parse(text).unwrap_or_else(|| panic!("`{text}` does not parse"));
        self.settings
            .keymap
            .set(cmd, chord)
            .unwrap_or_else(|e| panic!("`{text}` will not bind: {e:?}"));
        self.sync_editor_keymap(cx);
    }

    fn start_recording(&mut self, cmd: Cmd, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.recording = Some(cmd);
        self.record_note = None;
        window.focus(&self.focus);
        cx.notify();
    }

    pub(super) fn cancel_recording(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        if self.recording.is_none() {
            return false;
        }
        self.recording = None;
        self.record_note = None;
        window.focus(&self.editor_focus);
        cx.notify();
        true
    }

    pub(super) fn record_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let Some(cmd) = self.recording else {
            return false;
        };
        if ev.is_held {
            return true;
        }
        let key = ev.keystroke.key.as_str();
        if matches!(
            key,
            "ctrl" | "alt" | "shift" | "cmd" | "super" | "win" | "platform"
        ) {
            return true;
        }

        if key == "backspace" && Mods::from(ev.keystroke.modifiers) == Mods::none() {
            self.settings.keymap.clear(cmd);
            self.commit_recording(None, window, cx);
            return true;
        }

        let Some(chord) = Chord::from_keystroke(&ev.keystroke) else {
            return true;
        };
        match self.settings.keymap.set(cmd, chord) {
            Ok(taken) => {
                let note = taken.map(|other| md_i18n::fmt::shortcut_taken_from(other.label()));
                self.commit_recording(note.map(|n| (cmd, n)), window, cx);
            }
            Err(Refusal::NeedsModifier) => {
                self.record_note = Some((cmd, md_i18n::t(Key::ShortcutNeedsModifier).to_owned()));
                cx.notify();
            }
            Err(Refusal::Reserved(chord)) => {
                self.record_note = Some((cmd, md_i18n::fmt::shortcut_reserved(&chord)));
                cx.notify();
            }
        }
        true
    }

    fn commit_recording(
        &mut self,
        note: Option<(Cmd, String)>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.recording = None;
        self.record_note = note;
        self.sync_editor_keymap(cx);
        self.save_settings();
        window.focus(&self.editor_focus);
        cx.notify();
    }

    fn reset_all_shortcuts(&mut self, cx: &mut Context<'_, Self>) {
        self.settings.keymap.reset_all();
        self.recording = None;
        self.record_note = None;
        self.sync_editor_keymap(cx);
        self.save_settings();
        cx.notify();
    }

    pub(super) fn shortcut_sections(&self, t: ShellTheme, this: &Entity<Self>) -> Vec<AnyElement> {
        let mut out: Vec<AnyElement> = vec![
            self.set_note(t, t18(Key::SetShortcutsHint))
                .into_any_element(),
        ];
        out.extend(
            Cmd::ALL
                .iter()
                .copied()
                .map(|cmd| self.shortcut_row(t, this, cmd).into_any_element()),
        );
        if !self.settings.keymap.is_default() {
            out.push(self.shortcut_reset_row(t, this).into_any_element());
        }
        out
    }

    fn shortcut_row(&self, t: ShellTheme, this: &Entity<Self>, cmd: Cmd) -> Div {
        let label = cmd.label();
        let note = self
            .record_note
            .as_ref()
            .filter(|(who, _)| *who == cmd)
            .map(|(_, text)| text.clone());
        div()
            .debug_selector(move || format!("row:{}", label.debug_name()))
            .w_full()
            .flex()
            .flex_col()
            .py(px(9.))
            .border_b_1()
            .border_color(t.border_variant)
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(t.text)
                            .child(t18(label)),
                    )
                    .child(self.shortcut_button(t, this, cmd)),
            )
            .children(note.map(|text| {
                div()
                    .debug_selector(move || format!("note:{}", label.debug_name()))
                    .mt(px(3.))
                    .text_size(px(11.5))
                    .text_color(t.syn_red)
                    .child(text)
            }))
    }

    fn shortcut_button(&self, t: ShellTheme, this: &Entity<Self>, cmd: Cmd) -> Stateful<Div> {
        let recording = self.recording == Some(cmd);
        let chord = self.settings.keymap.chord_for(cmd).map(Chord::display);

        let (text, mono) = match (recording, chord) {
            (true, _) => (t18(Key::ShortcutRecording).to_owned(), false),
            (false, Some(c)) => (c, true),
            (false, None) => (t18(Key::ShortcutUnset).to_owned(), false),
        };
        let this = this.clone();
        div()
            .id(cmd.debug_name())
            .debug_selector(move || format!("kb:{}", cmd.debug_name()))
            .flex_none()
            .px(px(10.))
            .py(px(4.))
            .rounded(px(5.))
            .border_1()
            .border_color(if recording {
                t.accent
            } else {
                t.border_variant
            })
            .when(mono, |d| d.font_family(MONO_FONT))
            .text_size(px(11.5))
            .text_color(if recording { t.text } else { t.text_muted })
            .when(!recording, |d| d.hover(move |s| s.bg(t.hover)))
            .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                this.update(cx, |shell, cx| shell.start_recording(cmd, window, cx));
            })
            .child(text)
    }

    fn shortcut_reset_row(&self, t: ShellTheme, this: &Entity<Self>) -> Div {
        let n = self.settings.keymap.overrides().count();
        let this = this.clone();
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(px(11.))
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(t.text_disabled)
                    .child(md_i18n::fmt::shortcut_overrides_count(n)),
            )
            .child(
                div()
                    .id("shortcuts-reset-all")
                    .debug_selector(|| "btn:shortcuts-reset".into())
                    .px(px(10.))
                    .py(px(4.))
                    .rounded(px(5.))
                    .border_1()
                    .border_color(t.border_variant)
                    .text_size(px(11.5))
                    .text_color(t.text_muted)
                    .hover(move |s| s.bg(t.hover))
                    .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                        this.update(cx, |shell, cx| shell.reset_all_shortcuts(cx));
                    })
                    .child(t18(Key::ShortcutsResetAll)),
            )
    }
}
