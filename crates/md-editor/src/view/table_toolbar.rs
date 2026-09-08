use super::table_commands::{TABLE_MENU, TableMenuEntry};
use super::{CursorMotion, EditorView};
use crate::ui::icons;
use crate::ui::theme::{MONO_FONT, RADIUS, ShellTheme};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, BoxShadow, ClickEvent, Context, Div, Entity, FontWeight, InteractiveElement, IntoElement,
    MouseButton, MouseUpEvent, ParentElement, SharedString, Stateful, StatefulInteractiveElement,
    Styled, Window, div, hsla, point, px, rgba, svg,
};
use md_core::Px;
use md_core::block::TableCellAlign;
use md_core::document::{Command, TABLE_PICKER_MAX_COLS, TABLE_PICKER_MAX_ROWS, TableLoc, TableOp};
use md_i18n::{Key, t as t18};
use md_theme::DocumentTheme;

const BAR_H: f32 = 28.0;
const BAR_LIFT: f32 = 38.0;
const DROP_TOP: f32 = BAR_LIFT - 4.0;
const BTN_W: f32 = 24.0;
const BTN_H: f32 = 22.0;
const MORE_W: f32 = 200.0;
const PICK_CELL: f32 = 18.0;
const PICK_GAP: f32 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TableChrome {
    pub x: Px,
    pub y: Px,
    pub w: Px,
    pub loc: TableLoc,
}

pub(crate) fn table_size_label(loc: &TableLoc) -> String {
    md_i18n::fmt::table_size(loc.rows, loc.cols)
}

fn more_menu_height() -> f32 {
    let mut h = 8.0;
    for e in TABLE_MENU {
        h += match e {
            TableMenuEntry::Separator => 9.0,
            TableMenuEntry::Item { .. } => 26.0,
        };
    }
    h + 2.0
}

fn picker_panel_size() -> (f32, f32) {
    let cols = TABLE_PICKER_MAX_COLS as f32;
    let rows = TABLE_PICKER_MAX_ROWS as f32;
    let w = cols * PICK_CELL + (cols - 1.0) * PICK_GAP + 20.0;
    let h = rows * PICK_CELL + (rows - 1.0) * PICK_GAP + 47.0;
    (w, h)
}

pub(crate) fn overlay_contains(
    chrome: TableChrome,
    scroll: Px,
    more_open: bool,
    picker_open: bool,
    pos: (Px, Px),
) -> bool {
    let x0 = chrome.x;
    let x1 = chrome.x + chrome.w;
    let top = chrome.y - scroll - BAR_LIFT as Px;
    if pos.0 >= x0 && pos.0 < x1 && pos.1 >= top && pos.1 < top + BAR_H as Px {
        return true;
    }
    let drop_top = top + DROP_TOP as Px;
    if more_open {
        let mx0 = (x1 - MORE_W as Px).max(x0);
        if pos.0 >= mx0
            && pos.0 < x1
            && pos.1 >= drop_top
            && pos.1 < drop_top + more_menu_height() as Px
        {
            return true;
        }
    }
    if picker_open {
        let (pw, ph) = picker_panel_size();
        if pos.0 >= x0 && pos.0 < x0 + pw as Px && pos.1 >= drop_top && pos.1 < drop_top + ph as Px
        {
            return true;
        }
    }
    false
}

impl EditorView {
    pub(crate) fn apply_table_toolbar(
        &mut self,
        op: TableOp,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.close_table_picker(false, cx);
        self.set_table_more_open(false, cx);
        self.set_table_menu_op_hover(None, cx);
        if let Some(loc) = self.state.doc.table_loc(self.state.cursor.block) {
            self.adjust_col_tracks_for_op(loc, &op);
        }
        self.apply_cmd(Command::Table(op));
        self.note_edit(cx);
        window.focus(&self.focus);
        cx.notify();
    }

    pub(crate) fn set_table_align_hover(
        &mut self,
        align: Option<TableCellAlign>,
        cx: &mut Context<'_, Self>,
    ) {
        if self.table_ui.align_hover != align {
            self.table_ui.align_hover = align;
            cx.notify();
        }
    }

    pub(crate) fn dismiss_table_panels(&mut self, cx: &mut Context<'_, Self>) {
        if self.table_ui.more_open {
            self.set_table_more_open(false, cx);
        }
        if self.table_ui.picker_open {
            self.close_table_picker(true, cx);
        }
    }

    pub(crate) fn close_table_picker(&mut self, restore_caret: bool, cx: &mut Context<'_, Self>) {
        if self.state.doc.is_composing() {
            if restore_caret {
                if let Some(sel) = self.state.doc.abort_compose() {
                    self.restore_sel(sel);
                }
            } else {
                let _ = self.state.doc.abort_compose();
            }
        }
        self.table_ui.picker_open = false;
        self.table_ui.picker_drag = false;
        self.table_ui.picker_hover = None;
        cx.notify();
    }

    pub(crate) fn set_table_more_open(&mut self, open: bool, cx: &mut Context<'_, Self>) {
        if self.table_ui.more_open == open {
            return;
        }
        self.table_ui.more_open = open;
        if open {
            self.close_table_picker(true, cx);
            self.set_table_menu_op_hover(None, cx);
            self.hide_table_grips(cx);
        } else {
            self.set_table_menu_op_hover(None, cx);
            self.apply_menu_grip_hover(cx);
        }
        cx.notify();
    }

    pub(crate) fn toggle_table_picker(&mut self, cx: &mut Context<'_, Self>) {
        if self.table_ui.picker_open {
            self.close_table_picker(true, cx);
            return;
        }
        self.table_ui.more_open = false;
        self.table_ui.picker_open = true;
        self.table_ui.picker_drag = false;
        let loc = self.state.doc.table_loc(self.state.cursor.block);
        self.table_ui.picker_hover = loc.map(|l| {
            (
                l.rows.clamp(1, TABLE_PICKER_MAX_ROWS),
                l.cols.clamp(1, TABLE_PICKER_MAX_COLS),
            )
        });
        self.hide_table_grips(cx);
        cx.notify();
    }

    pub(crate) fn picker_set_hover(
        &mut self,
        rows: usize,
        cols: usize,
        cx: &mut Context<'_, Self>,
    ) {
        let next = Some((rows, cols));
        if self.table_ui.picker_hover != next {
            self.table_ui.picker_hover = next;
            cx.notify();
        }
        if self.table_ui.picker_drag {
            self.apply_table_resize_marked(rows, cols);
        }
    }

    pub(crate) fn picker_press(&mut self, rows: usize, cols: usize, cx: &mut Context<'_, Self>) {
        self.table_ui.picker_drag = true;
        self.table_ui.picker_hover = Some((rows, cols));
        self.apply_table_resize_marked(rows, cols);
        cx.notify();
    }

    pub(crate) fn finish_table_picker(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if !self.table_ui.picker_drag && !self.state.doc.is_composing() {
            return;
        }
        self.state.doc.commit_compose();
        self.table_ui.picker_open = false;
        self.table_ui.picker_drag = false;
        self.table_ui.picker_hover = None;
        self.note_edit(cx);
        window.focus(&self.focus);
        cx.notify();
    }

    fn apply_table_resize_marked(&mut self, rows: usize, cols: usize) {
        let op = TableOp::Resize { rows, cols };
        if let Some(loc) = self.state.doc.table_loc(self.state.cursor.block) {
            self.adjust_col_tracks_for_op(loc, &op);
        }
        let c = self
            .state
            .doc
            .apply_marked(self.editing_sel(), Command::Table(op));
        self.place_cursor(c, CursorMotion::Move);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn overlay(
    chrome: TableChrome,
    scroll: Px,
    more_open: bool,
    picker_open: bool,
    picker_hover: Option<(usize, usize)>,
    theme: DocumentTheme,
    row_below_chord: Option<String>,
    editor: Entity<EditorView>,
) -> Stateful<Div> {
    let t = ShellTheme::from_app(&theme.app);
    let loc = chrome.loc;
    div()
        .id("table-tbar")
        .absolute()
        .left(px(chrome.x as f32))
        .top(px((chrome.y - scroll) as f32 - BAR_LIFT))
        .w(px(chrome.w as f32))
        .on_hover({
            let editor = editor.clone();
            move |hovered, _, cx| {
                editor.update(cx, |v, cx| v.set_table_toolbar_hover(*hovered, cx));
            }
        })
        .on_mouse_down(MouseButton::Right, {
            let editor = editor.clone();
            move |_, _, cx| {
                cx.stop_propagation();
                editor.update(cx, |v, cx| v.dismiss_table_panels(cx));
            }
        })
        .child(
            div()
                .h(px(BAR_H))
                .flex()
                .justify_between()
                .child(left_cluster(t, loc, picker_open, editor.clone()))
                .child(right_cluster(t, more_open, editor.clone())),
        )
        .when(picker_open, |d| {
            d.child(picker_panel(t, loc, picker_hover, editor.clone()))
        })
        .when(more_open, |d| {
            d.child(more_menu(t, row_below_chord, editor))
        })
}

fn bar_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(0x00000066).into(),
        offset: point(px(0.), px(10.)),
        blur_radius: px(28.),
        spread_radius: px(0.),
    }]
}

fn cluster(id: &'static str, t: ShellTheme) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(BAR_H))
        .flex()
        .items_center()
        .gap(px(2.))
        .px(px(5.))
        .bg(t.panel_bg)
        .border_1()
        .border_color(t.border)
        .rounded(px(RADIUS))
        .shadow(bar_shadow())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

fn left_cluster(
    t: ShellTheme,
    loc: TableLoc,
    picker_open: bool,
    editor: Entity<EditorView>,
) -> Stateful<Div> {
    cluster("table-tbar-l", t)
        .child(picker_wrap(t, loc, picker_open, editor.clone()))
        .child(
            div()
                .w(px(1.))
                .h(px(16.))
                .mx(px(3.))
                .flex_none()
                .bg(t.border_variant),
        )
        .child(align_btn(
            "table-align-start",
            icons::ALIGN_START,
            TableCellAlign::Start,
            loc.align == TableCellAlign::Start,
            t,
            editor.clone(),
        ))
        .child(align_btn(
            "table-align-center",
            icons::ALIGN_CENTER,
            TableCellAlign::Center,
            loc.align == TableCellAlign::Center,
            t,
            editor.clone(),
        ))
        .child(align_btn(
            "table-align-end",
            icons::ALIGN_END,
            TableCellAlign::End,
            loc.align == TableCellAlign::End,
            t,
            editor,
        ))
}

fn picker_wrap(
    t: ShellTheme,
    loc: TableLoc,
    picker_open: bool,
    editor: Entity<EditorView>,
) -> Stateful<Div> {
    div()
        .id("table-picker-wrap")
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(grid_button(t, loc, picker_open, editor))
}

fn grid_button(
    t: ShellTheme,
    loc: TableLoc,
    picker_open: bool,
    editor: Entity<EditorView>,
) -> Stateful<Div> {
    let fg = if picker_open { t.text } else { t.text_muted };
    let btn = div()
        .id("table-grid")
        .h(px(BTN_H))
        .px(px(8.))
        .rounded(px(5.))
        .flex()
        .items_center()
        .gap(px(6.))
        .text_color(fg)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
    let btn = if picker_open {
        btn.bg(t.hover)
    } else {
        btn.hover(move |s| s.bg(t.hover).text_color(t.text))
    };
    btn.on_click({
        move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            editor.update(cx, |v, cx| {
                v.toggle_table_picker(cx);
                window.focus(&v.focus);
            });
        }
    })
    .child(
        svg()
            .size(px(13.))
            .path(icons::TABLE_GRID)
            .text_color(if picker_open { t.text } else { t.text_muted }),
    )
    .child(
        div()
            .font_family(MONO_FONT)
            .text_size(px(11.))
            .text_color(t.text)
            .child(table_size_label(&loc)),
    )
    .child(
        svg()
            .size(px(9.))
            .path(icons::CHEV_DOWN)
            .text_color(t.text_disabled),
    )
}

fn picker_panel(
    t: ShellTheme,
    loc: TableLoc,
    hover: Option<(usize, usize)>,
    editor: Entity<EditorView>,
) -> Stateful<Div> {
    let (hr, hc) = hover.unwrap_or((
        loc.rows.clamp(1, TABLE_PICKER_MAX_ROWS),
        loc.cols.clamp(1, TABLE_PICKER_MAX_COLS),
    ));
    div()
        .id("table-gpick")
        .absolute()
        .left(px(0.))
        .top(px(DROP_TOP))
        .flex()
        .flex_col()
        .p(px(10.))
        .bg(t.panel_bg)
        .border_1()
        .border_color(t.border)
        .rounded(px(RADIUS))
        .shadow(vec![BoxShadow {
            color: rgba(0x00000080).into(),
            offset: point(px(0.), px(14.)),
            blur_radius: px(40.),
            spread_radius: px(0.),
        }])
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, {
            let editor = editor.clone();
            move |_, _, cx| {
                cx.stop_propagation();
                editor.update(cx, |v, cx| v.dismiss_table_panels(cx));
            }
        })
        .on_mouse_up(MouseButton::Left, {
            let editor = editor.clone();
            move |_: &MouseUpEvent, window, cx| {
                editor.update(cx, |v, cx| {
                    v.finish_table_picker(window, cx);
                });
            }
        })
        .child(picker_grid(t, hr, hc, editor))
        .child(picker_label(t, hr, hc))
}

fn picker_grid(t: ShellTheme, hover_r: usize, hover_c: usize, editor: Entity<EditorView>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(PICK_GAP))
        .children((0..TABLE_PICKER_MAX_ROWS).map(move |r| {
            let editor = editor.clone();
            div().flex().gap(px(PICK_GAP)).children(
                (0..TABLE_PICKER_MAX_COLS)
                    .map(move |c| picker_cell(t, r, c, hover_r, hover_c, editor.clone())),
            )
        }))
}

fn picker_cell(
    t: ShellTheme,
    r: usize,
    c: usize,
    hover_r: usize,
    hover_c: usize,
    editor: Entity<EditorView>,
) -> Stateful<Div> {
    let rows = r + 1;
    let cols = c + 1;
    let in_range = rows <= hover_r && cols <= hover_c;
    let is_cur = rows == hover_r && cols == hover_c;
    let bg = if is_cur {
        Some(t.accent)
    } else if in_range {
        Some(t.selected_bg)
    } else {
        None
    };
    let cell = div()
        .id(SharedString::from(format!("gcell-{r}-{c}")))
        .w(px(PICK_CELL))
        .h(px(PICK_CELL))
        .rounded(px(2.))
        .border_1()
        .border_color(if in_range {
            hsla(0., 0., 0., 0.)
        } else {
            t.border_variant
        })
        .on_mouse_down(MouseButton::Left, {
            let editor = editor.clone();
            move |_, _, cx| {
                cx.stop_propagation();
                editor.update(cx, |v, cx| v.picker_press(rows, cols, cx));
            }
        })
        .on_hover(move |hovered, _, cx| {
            if *hovered {
                editor.update(cx, |v, cx| v.picker_set_hover(rows, cols, cx));
            }
        });
    if let Some(bg) = bg { cell.bg(bg) } else { cell }
}

fn picker_label(t: ShellTheme, rows: usize, cols: usize) -> Div {
    div()
        .mt(px(9.))
        .flex()
        .justify_between()
        .items_baseline()
        .text_size(px(11.))
        .child(
            div()
                .font_family(MONO_FONT)
                .font_weight(FontWeight(500.0))
                .text_color(t.text)
                .child(md_i18n::fmt::table_size(rows, cols)),
        )
        .child(
            div()
                .text_color(t.text_disabled)
                .child(t18(Key::TablePickerHint)),
        )
}

fn align_btn(
    id: &'static str,
    icon: &'static str,
    align: TableCellAlign,
    on: bool,
    t: ShellTheme,
    editor: Entity<EditorView>,
) -> Stateful<Div> {
    let fg = if on { t.text } else { t.text_muted };
    let btn = div()
        .id(id)
        .w(px(BTN_W))
        .h(px(BTN_H))
        .rounded(px(5.))
        .flex()
        .items_center()
        .justify_center()
        .text_color(fg)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
    let btn = if on {
        btn.bg(t.selected_bg)
    } else {
        btn.hover(move |s| s.bg(t.hover).text_color(t.text))
    };
    btn.on_hover({
        let editor = editor.clone();
        move |hovered, _, cx| {
            editor.update(cx, |v, cx| {
                if *hovered {
                    v.set_table_align_hover(Some(align), cx);
                } else if v.table_ui.align_hover == Some(align) {
                    v.set_table_align_hover(None, cx);
                }
            });
        }
    })
    .on_click({
        move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            editor.update(cx, |v, cx| {
                v.apply_table_toolbar(TableOp::SetColumnAlign(align), window, cx);
            });
        }
    })
    .child(svg().size(px(12.)).path(icon).text_color(fg))
}

fn right_cluster(t: ShellTheme, more_open: bool, editor: Entity<EditorView>) -> Stateful<Div> {
    cluster("table-tbar-r", t)
        .child(more_wrap(t, more_open, editor.clone()))
        .child(icon_action(
            "table-delete",
            icons::TABLE_DELETE,
            TableOp::DeleteTable,
            t,
            editor,
        ))
}

fn more_wrap(t: ShellTheme, more_open: bool, editor: Entity<EditorView>) -> Stateful<Div> {
    div()
        .id("table-more-wrap")
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(more_button(t, more_open, editor))
}

fn more_button(t: ShellTheme, more_open: bool, editor: Entity<EditorView>) -> Stateful<Div> {
    let fg = if more_open { t.text } else { t.text_muted };
    let btn = div()
        .id("table-more")
        .w(px(BTN_W))
        .h(px(BTN_H))
        .rounded(px(5.))
        .flex()
        .items_center()
        .justify_center()
        .text_color(fg)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
    let btn = if more_open {
        btn.bg(t.hover)
    } else {
        btn.hover(move |s| s.bg(t.hover).text_color(t.text))
    };
    btn.on_click({
        move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            editor.update(cx, |v, cx| {
                v.set_table_more_open(!v.table_ui.more_open, cx);
                window.focus(&v.focus);
            });
        }
    })
    .child(svg().size(px(12.)).path(icons::TABLE_MORE).text_color(fg))
}

fn icon_action(
    id: &'static str,
    icon: &'static str,
    op: TableOp,
    t: ShellTheme,
    editor: Entity<EditorView>,
) -> Stateful<Div> {
    div()
        .id(id)
        .w(px(BTN_W))
        .h(px(BTN_H))
        .rounded(px(5.))
        .flex()
        .items_center()
        .justify_center()
        .text_color(t.text_muted)
        .hover(move |s| s.bg(t.hover).text_color(t.text))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click({
            move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                editor.update(cx, |v, cx| {
                    v.apply_table_toolbar(op, window, cx);
                });
            }
        })
        .child(
            svg()
                .size(px(12.))
                .path(icon)
                .text_color(t.text_muted)
                .hover(move |s| s.text_color(t.text)),
        )
}

fn more_menu(t: ShellTheme, chord: Option<String>, editor: Entity<EditorView>) -> Stateful<Div> {
    let rows: Vec<_> = TABLE_MENU
        .iter()
        .map(|entry| match entry {
            TableMenuEntry::Separator => menu_separator(t).into_any_element(),
            TableMenuEntry::Item { label, op, danger } => {
                let kb = crate::keymap::table_op_chord_cmd(*op).and(chord.clone());
                menu_row(t, *label, kb, *op, *danger, editor.clone()).into_any_element()
            }
        })
        .collect();
    div()
        .id("table-more-menu")
        .absolute()
        .right(px(0.))
        .top(px(DROP_TOP))
        .w(px(MORE_W))
        .flex()
        .flex_col()
        .p(px(4.))
        .bg(t.panel_bg)
        .border_1()
        .border_color(t.border)
        .rounded(px(RADIUS))
        .shadow(vec![BoxShadow {
            color: rgba(0x00000080).into(),
            offset: point(px(0.), px(14.)),
            blur_radius: px(40.),
            spread_radius: px(0.),
        }])
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, move |_, _, cx| {
            cx.stop_propagation();
            editor.update(cx, |v, cx| v.dismiss_table_panels(cx));
        })
        .children(rows)
}

fn menu_separator(t: ShellTheme) -> Div {
    div()
        .flex_none()
        .h(px(1.))
        .mx(px(6.))
        .my(px(4.))
        .bg(t.border_variant)
}

fn menu_row(
    t: ShellTheme,
    label: Key,
    kb: Option<String>,
    op: TableOp,
    danger: bool,
    editor: Entity<EditorView>,
) -> Stateful<Div> {
    let color = if danger { t.syn_red } else { t.text_muted };
    let hover_fg = if danger { t.syn_red } else { t.text };
    div()
        .id(label.debug_name())
        .h(px(26.))
        .flex_none()
        .flex()
        .items_center()
        .px(px(10.))
        .rounded(px(5.))
        .text_size(px(12.5))
        .text_color(color)
        .hover(move |s| s.bg(t.hover).text_color(hover_fg))
        .on_mouse_down(MouseButton::Left, {
            move |_, window, cx| {
                cx.stop_propagation();
                editor.update(cx, |v, cx| {
                    v.apply_table_toolbar(op, window, cx);
                });
            }
        })
        .child(t18(label))
        .children(kb.map(|kb| {
            div()
                .debug_selector(move || format!("menukb:{}", label.debug_name()))
                .ml_auto()
                .font_family(MONO_FONT)
                .text_size(px(10.5))
                .text_color(t.text_disabled)
                .child(kb)
        }))
}

#[cfg(test)]
mod overlay_hit_tests {
    use super::{TableChrome, overlay_contains};
    use md_core::block::TableCellAlign;
    use md_core::document::TableLoc;

    fn chrome() -> TableChrome {
        TableChrome {
            x: 100.0,
            y: 200.0,
            w: 400.0,
            loc: TableLoc {
                table: 1,
                rows: 2,
                cols: 2,
                row: 0,
                col: 0,
                align: TableCellAlign::Start,
            },
        }
    }

    #[test]
    fn overlay_contains_the_bar() {
        let c = chrome();
        assert!(overlay_contains(c, 0.0, false, false, (120.0, 170.0)));
        assert!(!overlay_contains(c, 0.0, false, false, (120.0, 250.0)));
    }

    #[test]
    fn overlay_contains_more_menu_below_the_bar() {
        let c = chrome();
        let in_menu = (420.0, 210.0);
        assert!(
            overlay_contains(c, 0.0, true, false, in_menu),
            "menu item sits below the 28px bar hitbox"
        );
        assert!(
            !overlay_contains(c, 0.0, false, false, in_menu),
            "closed menu must not eat table clicks"
        );
        assert!(
            !overlay_contains(c, 0.0, true, false, (150.0, 210.0)),
            "empty space left of the 200px menu is the table"
        );
    }
}
