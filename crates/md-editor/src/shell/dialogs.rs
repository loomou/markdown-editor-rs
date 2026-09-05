use super::Shell;
use crate::ui::theme::{DLG_MIN_H, DLG_W, MONO_FONT, RADIUS, ShellTheme};
use crate::view::{
    EditorView, InsertTableField, SaveConflictChoice, UnsavedChoice, unsaved_file_name,
};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, BoxShadow, ClickEvent, Context, Div, Entity, FontWeight, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, ParentElement, Stateful, StatefulInteractiveElement, Styled, Window,
    div, point, px, rgba,
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

struct InsertTableDialogSpec {
    rows: String,
    cols: String,
    field: InsertTableField,
    selected: bool,
    can_create: bool,
}

struct InsertTableFieldSpec {
    id: &'static str,
    label: Key,
    value: String,
    field: InsertTableField,
    focused: bool,
    selected: bool,
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
        let name = unsaved_file_name(&editor.state.doc).to_string();
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
                .child(self.unsaved_dialog(t, this, name)),
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
            .shadow(vec![BoxShadow {
                color: rgba(0x0000008c).into(),
                offset: point(px(0.), px(18.)),
                blur_radius: px(50.),
                spread_radius: px(0.),
            }])
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

    fn unsaved_dialog(&self, t: ShellTheme, this: Entity<Self>, name: String) -> impl IntoElement {
        let (question_head, question_tail) = md_i18n::fmt::save_changes_question();
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
            .shadow(vec![BoxShadow {
                color: rgba(0x0000008c).into(),
                offset: point(px(0.), px(18.)),
                blur_radius: px(50.),
                spread_radius: px(0.),
            }])
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .text_size(px(15.))
                    .font_weight(FontWeight(600.0))
                    .text_color(t.text)
                    .child(question_head)
                    .child(
                        div()
                            .font_family(MONO_FONT)
                            .text_size(px(12.5))
                            .font_weight(FontWeight(500.0))
                            .child(name),
                    )
                    .child(question_tail),
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

    pub(super) fn insert_table_overlay(
        &self,
        t: ShellTheme,
        this: Entity<Self>,
        editor: &EditorView,
    ) -> Option<impl IntoElement> {
        let state = editor.insert_table.as_ref()?;
        let rows = state.rows.clone();
        let cols = state.cols.clone();
        let field = state.field;
        let selected = state.selected;
        let can_create = state.can_create();
        let focus = editor.insert_table_focus.clone();
        Some(
            div()
                .id("insert-table-mask")
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
                .on_mouse_down(MouseButton::Left, {
                    let this = this.clone();
                    move |_, window, cx| {
                        this.update(cx, |shell, cx| {
                            shell.editor.update(cx, |editor, cx| {
                                editor.close_insert_table(window, cx);
                            });
                        });
                        cx.stop_propagation();
                    }
                })
                .on_key_down({
                    let this = this.clone();
                    move |ev: &KeyDownEvent, window, cx| {
                        this.update(cx, |shell, cx| {
                            shell.editor.update(cx, |editor, cx| {
                                editor.on_insert_table_key(ev, window, cx);
                            });
                        });
                        cx.stop_propagation();
                    }
                })
                .child(self.insert_table_dialog(
                    t,
                    this,
                    InsertTableDialogSpec {
                        rows,
                        cols,
                        field,
                        selected,
                        can_create,
                    },
                )),
        )
    }

    fn insert_table_dialog(
        &self,
        t: ShellTheme,
        this: Entity<Self>,
        spec: InsertTableDialogSpec,
    ) -> impl IntoElement {
        let InsertTableDialogSpec {
            rows,
            cols,
            field,
            selected,
            can_create,
        } = spec;
        div()
            .id("insert-table-dialog")
            .w(px(DLG_W))
            .flex()
            .flex_col()
            .px(px(16.))
            .pt(px(22.))
            .pb(px(16.))
            .bg(t.editor_bg)
            .border_1()
            .border_color(t.border)
            .rounded(px(RADIUS))
            .shadow(vec![BoxShadow {
                color: rgba(0x0000008c).into(),
                offset: point(px(0.), px(18.)),
                blur_radius: px(50.),
                spread_radius: px(0.),
            }])
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight(600.0))
                    .text_color(t.text)
                    .child(t18(Key::DlgInsertTable)),
            )
            .child(self.insert_table_field_row(
                t,
                this.clone(),
                InsertTableFieldSpec {
                    label: Key::DlgTableRows,
                    id: "insert-table-rows",
                    value: rows,
                    field: InsertTableField::Rows,
                    focused: field == InsertTableField::Rows,
                    selected,
                },
            ))
            .child(self.insert_table_field_row(
                t,
                this.clone(),
                InsertTableFieldSpec {
                    label: Key::DlgTableCols,
                    id: "insert-table-cols",
                    value: cols,
                    field: InsertTableField::Cols,
                    focused: field == InsertTableField::Cols,
                    selected,
                },
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .pt(px(22.))
                    .child(div().flex_1())
                    .child(self.insert_table_btn(
                        t,
                        "insert-table-cancel",
                        Key::DlgCancel,
                        false,
                        true,
                        this.clone(),
                    ))
                    .child(self.insert_table_btn(
                        t,
                        "insert-table-create",
                        Key::DlgCreate,
                        true,
                        can_create,
                        this,
                    )),
            )
    }

    fn insert_table_field_row(
        &self,
        t: ShellTheme,
        this: Entity<Self>,
        spec: InsertTableFieldSpec,
    ) -> impl IntoElement {
        let InsertTableFieldSpec {
            id,
            label,
            value,
            field,
            focused,
            selected,
        } = spec;
        let highlight = focused && selected;
        div()
            .flex()
            .items_center()
            .gap(px(12.))
            .pt(px(14.))
            .child(
                div()
                    .w(px(72.))
                    .text_size(px(12.5))
                    .text_color(t.text_muted)
                    .child(t18(label)),
            )
            .child(
                div()
                    .id(id)
                    .h(px(28.))
                    .w(px(88.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .rounded(px(RADIUS))
                    .border_1()
                    .border_color(if focused { t.accent } else { t.border })
                    .bg(t.panel_bg)
                    .text_size(px(13.))
                    .font_family(MONO_FONT)
                    .text_color(t.text)
                    .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                        this.update(cx, |shell, cx| {
                            shell.editor.update(cx, |editor, cx| {
                                editor.set_insert_table_field(field, cx);
                            });
                        });
                    })
                    .child(
                        div()
                            .when(highlight, |d| d.bg(t.accent).text_color(t.on_accent))
                            .child(value),
                    ),
            )
    }

    fn insert_table_btn(
        &self,
        t: ShellTheme,
        id: &'static str,
        label: Key,
        primary: bool,
        enabled: bool,
        this: Entity<Self>,
    ) -> Stateful<Div> {
        let fg = if !enabled {
            t.text_disabled
        } else if primary {
            t.on_accent
        } else {
            t.text_muted
        };
        let border = if primary {
            rgba(0x00000000).into()
        } else {
            t.border_variant
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
            .when(primary, |d| d.font_weight(FontWeight(500.0)))
            .when(primary && enabled, |d| d.bg(t.accent))
            .when(!primary && enabled, |d| {
                d.hover(move |s| s.bg(t.hover).text_color(t.text))
            })
            .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                if !enabled {
                    return;
                }
                this.update(cx, |shell, cx| {
                    shell.editor.update(cx, |editor, cx| {
                        if primary {
                            editor.confirm_insert_table(window, cx);
                        } else {
                            editor.close_insert_table(window, cx);
                        }
                    });
                    cx.notify();
                });
            })
            .child(t18(label))
    }
}
