use super::Shell;
use super::color_picker::ColorPicker;
use crate::ui::text_input::{InputMark, KeyOutcome};
use crate::ui::theme::{MONO_FONT, ShellTheme};
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, Entity, FontWeight, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, ParentElement, Pixels, Point, Size, Stateful,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use md_content::gpui_theme::ThemeColorExt;
use md_i18n::{Key, t as t18};
use md_theme::{ColorGroup, ColorSlot, DocumentTheme, ThemeColor};
use std::ops::Range;

impl Shell {
    pub(super) fn recolor(
        &mut self,
        slot: ColorSlot,
        f: impl FnOnce(ThemeColor) -> ThemeColor,
        cx: &mut App,
    ) {
        let over = self.settings.appearance.colors();
        let current = over
            .get(slot)
            .unwrap_or_else(|| slot.read(&self.settings.appearance.document_theme()));
        self.settings.appearance.colors_mut().set(slot, f(current));
        self.sync_editor_theme(cx);
    }

    pub(super) fn reset_color(&mut self, slot: ColorSlot, cx: &mut App) {
        self.settings.appearance.colors_mut().clear(slot);
        self.appearance_changed(cx);
    }

    pub(super) fn reset_all_colors(&mut self, cx: &mut App) {
        self.settings.appearance.colors_mut().clear_all();
        self.color_picker = None;
        self.appearance_changed(cx);
    }

    pub(super) fn slot_color(&self, slot: ColorSlot, theme: &DocumentTheme) -> ThemeColor {
        ThemeColor {
            a: 1.0,
            ..slot.read(theme)
        }
    }

    pub(super) fn open_color_picker(
        &mut self,
        slot: ColorSlot,
        at: Point<Pixels>,
        viewport: Size<Pixels>,
    ) {
        self.open_menu = None;
        if self.color_picker.as_ref().is_some_and(|p| p.slot() == slot) {
            self.color_picker = None;
            return;
        }
        let color = self.slot_color(slot, &self.settings.appearance.document_theme());
        self.color_picker = Some(ColorPicker::open(slot, at, viewport, color));
    }

    pub(super) fn close_color_picker(&mut self) {
        self.color_picker = None;
    }

    pub(super) fn hex_focused(&self, window: &Window) -> bool {
        self.color_picker.is_some() && self.hex_focus.is_focused(window)
    }

    pub(super) fn hex_replace(&mut self, range: Range<usize>, text: &str, cx: &mut App) {
        let Some(picker) = self.color_picker.as_mut() else {
            return;
        };
        picker.hex.replace(range, text, InputMark::Plain);
        self.apply_hex(cx);
    }

    fn apply_hex(&mut self, cx: &mut App) {
        let Some(picker) = self.color_picker.as_ref() else {
            return;
        };
        let Some(parsed) = picker.parsed_hex() else {
            return;
        };
        let slot = picker.slot();
        self.recolor(slot, |c| ThemeColor { a: c.a, ..parsed }, cx);
        self.save_settings();
    }

    pub(super) fn hex_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.color_picker.is_none() {
            return;
        }
        let m = &ev.keystroke.modifiers;
        if !crate::ui::chord::has_chord(m) && ev.keystroke.key == "enter" {
            window.focus(&self.focus);
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let key = ev.keystroke.key.clone();
        let m = *m;
        let Some(picker) = self.color_picker.as_mut() else {
            return;
        };
        match picker.hex.nav_key(&key, &m, cx) {
            KeyOutcome::Ignored => {}
            KeyOutcome::Moved => {
                cx.stop_propagation();
                cx.notify();
            }
            KeyOutcome::Edited => {
                self.apply_hex(cx);
                cx.stop_propagation();
                cx.notify();
            }
        }
    }

    pub(super) fn hex_escape(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> bool {
        if !self.hex_focused(window) {
            return false;
        }
        let Some(slot) = self.color_picker.as_ref().map(ColorPicker::slot) else {
            return false;
        };
        let color = self.slot_color(slot, &self.settings.appearance.document_theme());
        if let Some(picker) = self.color_picker.as_mut() {
            picker.sync_hex(color, false);
        }
        window.focus(&self.focus);
        cx.notify();
        true
    }

    pub(super) fn hex_mouse_down(
        &mut self,
        at: Point<Pixels>,
        click_count: usize,
        shift: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.focus(&self.hex_focus);
        if let Some(picker) = self.color_picker.as_mut() {
            picker.hex.mouse_down(at, click_count, shift);
        }
        cx.notify();
    }

    pub(super) fn toggle_color_group(&mut self, group: ColorGroup) {
        let open = &mut self.color_groups_open[group.index()];
        *open = !*open;
        if !*open
            && self
                .color_picker
                .as_ref()
                .is_some_and(|p| p.slot().group() == group)
        {
            self.color_picker = None;
        }
    }

    pub(super) fn color_sections(
        &self,
        t: ShellTheme,
        this: &Entity<Self>,
        theme: &DocumentTheme,
    ) -> Vec<AnyElement> {
        let _ = theme;
        let changed = self.settings.appearance.colors().len();
        let mut out: Vec<AnyElement> = vec![
            self.set_row(
                t,
                Key::SetColors,
                &md_i18n::fmt::color_count_hint(ColorSlot::COUNT),
                self.settings_file_button(t, this).into_any_element(),
            )
            .into_any_element(),
        ];
        if changed > 0 {
            out.push(self.color_reset_row(t, this).into_any_element());
        }
        out
    }

    #[expect(dead_code, reason = "see the \"kept\" section in color_row")]
    fn color_group_header(
        &self,
        t: ShellTheme,
        this: &Entity<Self>,
        group: ColorGroup,
    ) -> Stateful<Div> {
        let open = self.color_groups_open[group.index()];
        let total = group.slots().count();
        let changed = group
            .slots()
            .filter(|s| self.settings.appearance.colors().get(*s).is_some())
            .count();
        let this = this.clone();
        div()
            .id(("color-group", group.index()))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(px(11.))
            .border_b_1()
            .border_color(t.border_variant)
            .hover(move |s| s.bg(t.hover))
            .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                this.update(cx, |shell, cx| {
                    shell.toggle_color_group(group);
                    cx.notify();
                });
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .w(px(10.))
                            .text_size(px(9.))
                            .text_color(t.text_muted)
                            .child(if open { "▾" } else { "▸" }),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight(500.0))
                            .text_color(t.text)
                            .child(t18(group.label())),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(if changed > 0 {
                        t.accent
                    } else {
                        t.text_disabled
                    })
                    .child(md_i18n::fmt::group_count(total, changed)),
            )
    }

    #[expect(dead_code, reason = "see the \"kept\" section above")]
    fn color_row(
        &self,
        t: ShellTheme,
        this: &Entity<Self>,
        slot: ColorSlot,
        theme: &DocumentTheme,
    ) -> Div {
        let color = self.slot_color(slot, theme);
        let changed = self.settings.appearance.colors().get(slot).is_some();
        let picker = self.color_picker.as_ref().filter(|p| p.slot() == slot);
        let open = picker.is_some();
        let this = this.clone();
        div()
            .relative()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(px(7.))
            .border_b_1()
            .border_color(t.border_variant)
            .children(picker.map(ColorPicker::anchor_probe))
            .child(
                div()
                    .pl(px(16.))
                    .text_size(px(12.5))
                    .text_color(if changed { t.text } else { t.text_muted })
                    .child(t18(slot.label())),
            )
            .child(
                div()
                    .id(("color-swatch", slot.index()))
                    .debug_selector(|| format!("swatch:{}", slot.key()))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .py(px(3.))
                    .pl(px(8.))
                    .rounded(px(5.))
                    .hover(move |s| s.bg(t.hover))
                    .capture_any_mouse_down({
                        move |ev: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                            if ev.button != MouseButton::Left {
                                return;
                            }
                            cx.stop_propagation();
                            let at = ev.position;
                            let viewport = window.viewport_size();
                            this.update(cx, |shell, cx| {
                                shell.open_color_picker(slot, at, viewport);
                                cx.notify();
                            });
                        }
                    })
                    .child(
                        div()
                            .font_family(MONO_FONT)
                            .text_size(px(11.))
                            .text_color(if changed { t.accent } else { t.text_disabled })
                            .child(color.to_css_hex()),
                    )
                    .child(
                        div()
                            .size(px(20.))
                            .flex_none()
                            .rounded(px(4.))
                            .border_1()
                            .border_color(if open { t.accent } else { t.border })
                            .bg(color.hsla()),
                    ),
            )
    }

    pub(super) fn color_picker_overlay(
        &self,
        t: ShellTheme,
        this: &Entity<Self>,
    ) -> Option<impl IntoElement> {
        let picker = self.color_picker.as_ref()?;
        let theme = self.settings.appearance.document_theme();
        let color = self.slot_color(picker.slot(), &theme);
        Some(picker.overlay(t, color, &self.hex_focus, this.clone()))
    }

    fn color_reset_row(&self, t: ShellTheme, this: &Entity<Self>) -> Div {
        let n = self.settings.appearance.colors().len();
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
                    .child(md_i18n::fmt::overrides_count(n)),
            )
            .child(
                div()
                    .id("color-reset-all")
                    .px(px(10.))
                    .py(px(4.))
                    .rounded(px(5.))
                    .border_1()
                    .border_color(t.border_variant)
                    .text_size(px(11.5))
                    .text_color(t.text_muted)
                    .hover(move |s| s.bg(t.hover))
                    .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                        this.update(cx, |shell, cx| {
                            shell.reset_all_colors(cx);
                            cx.notify();
                        });
                    })
                    .child(t18(Key::SetColorsResetAll)),
            )
    }
}
