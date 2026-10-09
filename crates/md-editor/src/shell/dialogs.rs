use super::Shell;
use crate::ui::theme::{DLG_MIN_H, DLG_W, MONO_FONT, RADIUS, ShellTheme};
use crate::view::{EditorView, SaveConflictChoice, UnsavedChoice};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, ClickEvent, Context, Div, Entity, FontWeight, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ParentElement, Stateful, StatefulInteractiveElement, Styled, Window,
    div, px, rgba,
};
use md_i18n::{Key, t as t18};

#[derive(Clone, Copy, PartialEq)]
enum UnsavedBtnKind {
    Danger,
    Ghost,
    Primary,
}

#[derive(Clone, Copy)]
struct UnsavedBtnSpec {
    id: &'static str,
    label: Key,
    kind: UnsavedBtnKind,
    choice: UnsavedChoice,
}

impl Shell {
    pub fn set_startup_notice(&mut self, err: crate::Error, cx: &mut Context<'_, Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.notice = Some(err);
            cx.notify();
        });
    }

    pub fn on_window_should_close(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        self.editor
            .update(cx, |editor, cx| editor.on_window_should_close(window, cx))
    }

    pub(super) fn notice_bar(
        &self,
        t: ShellTheme,
        this: Entity<Self>,
        text: Option<String>,
    ) -> Option<impl IntoElement> {
        let text = text?;
        Some(
            div()
                .id("error-notice")
                .flex()
                .flex_none()
                .items_center()
                .px(px(10.))
                .h(px(28.))
                .gap(px(8.))
                .bg(t.panel_bg)
                .border_b_1()
                .border_color(t.border)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(px(12.))
                        .text_color(t.syn_red)
                        .child(text),
                )
                .child(
                    div()
                        .id("notice-dismiss")
                        .px(px(6.))
                        .py(px(2.))
                        .rounded(px(4.))
                        .text_size(px(12.))
                        .text_color(t.text_muted)
                        .hover(move |s| s.bg(t.hover))
                        .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                            this.update(cx, |shell, cx| {
                                shell.editor.update(cx, |editor, cx| {
                                    editor.dismiss_notice(cx);
                                });
                            });
                        })
                        .child(t18(Key::DlgClose)),
                ),
        )
    }

    pub(super) fn unsaved_overlay(
        &self,
        t: ShellTheme,
        this: Entity<Self>,
        editor: &EditorView,
    ) -> Option<impl IntoElement> {
        editor.unsaved_nav?;
        let focus = editor.unsaved_focus.clone();
        Some(
            div()
                .id("unsaved-mask")
                .absolute()
                .top(px(0.))
                .left(px(0.))
                .right(px(0.))
                .bottom(px(0.))
                .flex()
                .items_center()
                .justify_center()
                .bg(t.overlay)
                .occlude()
                .track_focus(&focus)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_key_down({
                    let this = this.clone();
                    move |ev: &KeyDownEvent, window, cx| {
                        this.update(cx, |shell, cx| {
                            shell.editor.update(cx, |editor, cx| {
                                editor.on_unsaved_key(ev, window, cx);
                            });
                        });
                        cx.stop_propagation();
                    }
                })
                .child(self.unsaved_dialog(t, this)),
        )
    }

    pub(super) fn save_conflict_overlay(
        &self,
        t: ShellTheme,
        this: Entity<Self>,
        editor: &EditorView,
    ) -> Option<impl IntoElement> {
        let path = editor.save_conflict.as_ref()?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("markdown")
            .to_string();
        let focus = editor.save_conflict_focus.clone();
        Some(
            div()
                .id("save-conflict-mask")
                .absolute()
                .top(px(0.))
                .left(px(0.))
                .right(px(0.))
                .bottom(px(0.))
                .flex()
                .items_center()
                .justify_center()
                .bg(t.overlay)
                .occlude()
                .track_focus(&focus)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_key_down({
                    let this = this.clone();
                    move |ev: &KeyDownEvent, window, cx| {
                        this.update(cx, |shell, cx| {
                            shell.editor.update(cx, |editor, cx| {
                                editor.on_save_conflict_key(ev, window, cx);
                            });
                        });
                        cx.stop_propagation();
                    }
                })
                .child(self.save_conflict_dialog(t, this, name)),
        )
    }

    fn save_conflict_dialog(
        &self,
        t: ShellTheme,
        this: Entity<Self>,
        name: String,
    ) -> impl IntoElement {
        div()
            .id("save-conflict-dialog")
            .w(px(DLG_W))
            .min_h(px(DLG_MIN_H))
            .flex()
            .flex_col()
            .px(px(16.))
            .pt(px(22.))
            .pb(px(16.))
            .bg(t.editor_bg)
            .border_1()
            .border_color(t.border)
            .rounded(px(RADIUS))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight(600.0))
                    .text_color(t.text)
                    .child(t18(Key::DlgFileChanged)),
            )
            .child(
                div()
                    .mt(px(10.))
                    .font_family(MONO_FONT)
                    .text_size(px(12.5))
                    .font_weight(FontWeight(500.0))
                    .text_color(t.text)
                    .child(name),
            )
            .child(
                div()
                    .mt(px(8.))
                    .text_size(px(11.5))
                    .text_color(t.text_disabled)
                    .child(t18(Key::DlgFileChangedDetail)),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(6.))
                    .pt(px(22.))
                    .child(self.save_conflict_btn(
                        t,
                        "save-conflict-cancel",
                        Key::DlgCancel,
                        UnsavedBtnKind::Ghost,
                        SaveConflictChoice::Cancel,
                        this.clone(),
                    ))
                    .child(self.save_conflict_btn(
                        t,
                        "save-conflict-overwrite",
                        Key::DlgOverwriteAnyway,
                        UnsavedBtnKind::Danger,
                        SaveConflictChoice::Overwrite,
                        this,
                    )),
            )
    }

    fn save_conflict_btn(
        &self,
        t: ShellTheme,
        id: &'static str,
        label: Key,
        kind: UnsavedBtnKind,
        choice: SaveConflictChoice,
        this: Entity<Self>,
    ) -> Stateful<Div> {
        let (fg, border, fill) = match kind {
            UnsavedBtnKind::Danger => (t.syn_red, t.border_variant, None),
            UnsavedBtnKind::Ghost => (t.text_muted, t.border_variant, None),
            UnsavedBtnKind::Primary => (t.on_accent, rgba(0x00000000).into(), Some(t.accent)),
        };
        let hover_fg = if kind == UnsavedBtnKind::Danger {
            t.syn_red
        } else {
            t.text
        };
        div()
            .id(id)
            .h(px(26.))
            .px(px(12.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(RADIUS))
            .text_size(px(12.5))
            .text_color(fg)
            .border_1()
            .border_color(border)
            .when_some(fill, |d, bg| d.bg(bg))
            .hover(move |s| s.bg(t.hover).text_color(hover_fg))
            .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                this.update(cx, |shell, cx| {
                    shell.editor.update(cx, |editor, cx| {
                        editor.apply_save_conflict_choice(choice, window, cx);
                    });
                    cx.notify();
                });
            })
            .child(t18(label))
    }

    fn unsaved_dialog(&self, t: ShellTheme, this: Entity<Self>) -> impl IntoElement {
        let question = md_i18n::fmt::save_changes_question();
        div()
            .id("unsaved-dialog")
            .w(px(DLG_W))
            .min_h(px(DLG_MIN_H))
            .flex()
            .flex_col()
            .px(px(16.))
            .pt(px(22.))
            .pb(px(16.))
            .bg(t.editor_bg)
            .border_1()
            .border_color(t.border)
            .rounded(px(RADIUS))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight(600.0))
                    .text_color(t.text)
                    .child(question),
            )
            .child(
                div()
                    .mt(px(10.))
                    .text_size(px(11.5))
                    .text_color(t.text_disabled)
                    .child(t18(Key::DlgUnsavedDetail)),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .pt(px(22.))
                    .child(self.unsaved_btn(
                        t,
                        UnsavedBtnSpec {
                            id: "unsaved-discard",
                            label: Key::DlgDontSave,
                            kind: UnsavedBtnKind::Danger,
                            choice: UnsavedChoice::Discard,
                        },
                        this.clone(),
                    ))
                    .child(div().flex_1())
                    .child(self.unsaved_btn(
                        t,
                        UnsavedBtnSpec {
                            id: "unsaved-cancel",
                            label: Key::DlgCancel,
                            kind: UnsavedBtnKind::Ghost,
                            choice: UnsavedChoice::Cancel,
                        },
                        this.clone(),
                    ))
                    .child(self.unsaved_btn(
                        t,
                        UnsavedBtnSpec {
                            id: "unsaved-save",
                            label: Key::Save,
                            kind: UnsavedBtnKind::Primary,
                            choice: UnsavedChoice::Save,
                        },
                        this,
                    )),
            )
    }

    fn unsaved_btn(
        &self,
        t: ShellTheme,
        spec: UnsavedBtnSpec,
        this: Entity<Self>,
    ) -> Stateful<Div> {
        let UnsavedBtnSpec {
            id,
            label,
            kind,
            choice,
        } = spec;
        let (fg, border, fill) = match kind {
            UnsavedBtnKind::Danger => (t.syn_red, rgba(0x00000000).into(), None),
            UnsavedBtnKind::Ghost => (t.text_muted, t.border_variant, None),
            UnsavedBtnKind::Primary => (t.on_accent, rgba(0x00000000).into(), Some(t.accent)),
        };
        let hover_fg = match kind {
            UnsavedBtnKind::Danger => t.syn_red,
            UnsavedBtnKind::Ghost => t.text,
            UnsavedBtnKind::Primary => t.on_accent,
        };
        div()
            .id(id)
            .h(px(26.))
            .px(px(12.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(RADIUS))
            .text_size(px(12.5))
            .text_color(fg)
            .border_1()
            .border_color(border)
            .when(kind == UnsavedBtnKind::Primary, |d| {
                d.font_weight(FontWeight(500.0))
            })
            .when_some(fill, |d, bg| d.bg(bg))
            .when(kind != UnsavedBtnKind::Primary, |d| {
                d.hover(move |s| s.bg(t.hover).text_color(hover_fg))
            })
            .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                this.update(cx, |shell, cx| {
                    shell.editor.update(cx, |editor, cx| {
                        editor.apply_unsaved_choice(choice, window, cx);
                    });
                    cx.notify();
                });
            })
            .child(t18(label))
    }
}
