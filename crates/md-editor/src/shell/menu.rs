use super::{Shell, VIEW_MARGIN};
use crate::keymap::{Chord, Cmd, table_op_chord_cmd};
use crate::ui::theme::{MONO_FONT, RADIUS, ShellTheme, TITLE_BAR_H};
use crate::view::table_commands::{TABLE_MENU, TableMenuEntry};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, Bounds, BoxShadow, ClickEvent, Context, Div, Entity, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement, Pixels, Point, Size, Stateful,
    StatefulInteractiveElement, Styled, Window, canvas, div, point, px, rgba,
};
use md_core::document::TableOp;
use md_i18n::{Key, t as t18};
use std::cell::Cell;
use std::rc::Rc;

pub(super) const MENU_W: f32 = 210.0;

const TABLE_FLYOUT_W: f32 = 186.0;

pub(super) const MENU_PAD: f32 = 4.0;

pub(super) const FLYOUT_GAP: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MenuId {
    File,
    Edit,
    View,
    Go,
    Help,
    Context,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum MenuAction {
    Nothing,
    Cmd(Cmd),
    Table(TableOp),
}

#[derive(Clone, Copy)]
pub(super) enum MenuEntry {
    Separator,
    Item {
        label: Key,
        action: MenuAction,
    },
    Submenu {
        label: Key,
        items: &'static [TableMenuEntry],
    },
}

use MenuEntry::{Item, Separator, Submenu};

impl MenuAction {
    fn chord_cmd(self) -> Option<Cmd> {
        match self {
            MenuAction::Cmd(cmd) => Some(cmd),
            MenuAction::Table(op) => table_op_chord_cmd(op),
            MenuAction::Nothing => None,
        }
    }
}

pub(super) fn menu_entries(id: MenuId) -> &'static [MenuEntry] {
    match id {
        MenuId::File => &[
            Item {
                label: Key::MenuNew,
                action: MenuAction::Cmd(Cmd::New),
            },
            Item {
                label: Key::MenuOpen,
                action: MenuAction::Cmd(Cmd::Open),
            },
            Item {
                label: Key::Save,
                action: MenuAction::Cmd(Cmd::Save),
            },
            Item {
                label: Key::MenuSaveAs,
                action: MenuAction::Cmd(Cmd::SaveAs),
            },
            Separator,
            Item {
                label: Key::MenuExit,
                action: MenuAction::Cmd(Cmd::Quit),
            },
        ],
        MenuId::Edit => &[
            Item {
                label: Key::MenuUndo,
                action: MenuAction::Cmd(Cmd::Undo),
            },
            Item {
                label: Key::MenuRedo,
                action: MenuAction::Cmd(Cmd::Redo),
            },
            Separator,
            Item {
                label: Key::MenuCut,
                action: MenuAction::Cmd(Cmd::Cut),
            },
            Item {
                label: Key::MenuCopy,
                action: MenuAction::Cmd(Cmd::Copy),
            },
            Item {
                label: Key::MenuPaste,
                action: MenuAction::Cmd(Cmd::Paste),
            },
            Separator,
            Item {
                label: Key::MenuSelectAll,
                action: MenuAction::Cmd(Cmd::SelectAll),
            },
            Item {
                label: Key::MenuFind,
                action: MenuAction::Cmd(Cmd::Find),
            },
            Item {
                label: Key::MenuInsertTable,
                action: MenuAction::Cmd(Cmd::InsertTable),
            },
        ],
        MenuId::View => &[
            Item {
                label: Key::MenuOutline,
                action: MenuAction::Cmd(Cmd::ToggleOutline),
            },
            Item {
                label: Key::Theme,
                action: MenuAction::Cmd(Cmd::ToggleTheme),
            },
        ],
        MenuId::Go => &[Item {
            label: Key::MenuGoToLine,
            action: MenuAction::Nothing,
        }],
        MenuId::Help => &[
            Item {
                label: Key::MenuShortcuts,
                action: MenuAction::Nothing,
            },
            Item {
                label: Key::MenuAbout,
                action: MenuAction::Nothing,
            },
        ],
        MenuId::Context => &[
            Item {
                label: Key::MenuUndo,
                action: MenuAction::Cmd(Cmd::Undo),
            },
            Item {
                label: Key::MenuRedo,
                action: MenuAction::Cmd(Cmd::Redo),
            },
            Separator,
            Item {
                label: Key::MenuCopy,
                action: MenuAction::Cmd(Cmd::Copy),
            },
            Item {
                label: Key::MenuPaste,
                action: MenuAction::Cmd(Cmd::Paste),
            },
            Separator,
            Item {
                label: Key::MenuSelectAll,
                action: MenuAction::Cmd(Cmd::SelectAll),
            },
            Item {
                label: Key::MenuFind,
                action: MenuAction::Cmd(Cmd::Find),
            },
        ],
    }
}

fn context_entries(in_table: bool) -> Vec<MenuEntry> {
    let mut out = menu_entries(MenuId::Context).to_vec();
    if in_table {
        out.insert(
            6,
            Submenu {
                label: Key::MenuTable,
                items: TABLE_MENU,
            },
        );
        out.insert(7, Separator);
    } else {
        out.push(Separator);
        out.push(Item {
            label: Key::MenuInsertTable,
            action: MenuAction::Cmd(Cmd::InsertTable),
        });
    }
    out
}

pub(super) fn entries_for(id: MenuId, in_table: bool) -> Vec<MenuEntry> {
    if id == MenuId::Context {
        context_entries(in_table)
    } else {
        menu_entries(id).to_vec()
    }
}

fn menu_entry_kind_height(entry: &MenuEntry) -> f32 {
    match entry {
        Separator => 9.0,
        Item { .. } | Submenu { .. } => 26.0,
    }
}

fn menu_height(id: MenuId, in_table: bool) -> f32 {
    entries_for(id, in_table)
        .iter()
        .map(menu_entry_kind_height)
        .sum::<f32>()
        + 8.0
}

fn bar_menu_anchor(left: Pixels, id: MenuId, viewport: Size<Pixels>) -> Point<Pixels> {
    clamp_menu_pos(point(left, px(TITLE_BAR_H + 2.)), id, viewport, false)
}

pub(super) fn clamp_menu_pos(
    pos: Point<Pixels>,
    id: MenuId,
    viewport: Size<Pixels>,
    in_table: bool,
) -> Point<Pixels> {
    let max_x = (viewport.width - px(MENU_W + VIEW_MARGIN)).max(px(0.));
    let max_y = (viewport.height - px(menu_height(id, in_table) + VIEW_MARGIN)).max(px(0.));
    point(pos.x.max(px(0.)).min(max_x), pos.y.max(px(0.)).min(max_y))
}

fn table_flyout_height() -> f32 {
    let mut h = MENU_PAD * 2.0;
    for e in TABLE_MENU {
        h += match e {
            TableMenuEntry::Separator => 9.0,
            TableMenuEntry::Item { .. } => 26.0,
        };
    }
    h + 2.0
}

pub(super) fn submenu_row_top(id: MenuId, in_table: bool) -> f32 {
    let mut y = MENU_PAD;
    for e in entries_for(id, in_table) {
        if matches!(e, Submenu { .. }) {
            return y;
        }
        y += menu_entry_kind_height(&e);
    }
    y
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct FlyoutPlace {
    pub(super) x: f32,
    pub(super) y: f32,
}

pub(super) fn place_table_flyout(
    popup: Point<Pixels>,
    viewport: Size<Pixels>,
    id: MenuId,
    in_table: bool,
) -> FlyoutPlace {
    let origin_x = f32::from(popup.x) + MENU_PAD;
    let origin_y = f32::from(popup.y) + submenu_row_top(id, in_table);
    let row_w = MENU_W - MENU_PAD * 2.0;
    let fw = TABLE_FLYOUT_W;
    let fh = table_flyout_height();
    let vw = f32::from(viewport.width);
    let vh = f32::from(viewport.height);
    let right_x = origin_x + row_w + FLYOUT_GAP;
    let left_x = origin_x - FLYOUT_GAP - fw;
    let mut screen_x = right_x;
    if right_x + fw > vw - VIEW_MARGIN && left_x >= VIEW_MARGIN {
        screen_x = left_x;
    }
    let max_x = (vw - VIEW_MARGIN - fw).max(VIEW_MARGIN);
    screen_x = screen_x.clamp(VIEW_MARGIN, max_x);
    let mut screen_y = origin_y - 5.0;
    let max_y = (vh - VIEW_MARGIN - fh).max(VIEW_MARGIN);
    screen_y = screen_y.clamp(VIEW_MARGIN, max_y);
    FlyoutPlace {
        x: screen_x - origin_x,
        y: screen_y - origin_y,
    }
}

pub(super) fn flyout_screen_rect(
    popup: Point<Pixels>,
    viewport: Size<Pixels>,
    id: MenuId,
    in_table: bool,
) -> (f32, f32, f32, f32) {
    let origin_x = f32::from(popup.x) + MENU_PAD;
    let origin_y = f32::from(popup.y) + submenu_row_top(id, in_table);
    let place = place_table_flyout(popup, viewport, id, in_table);
    (
        origin_x + place.x,
        origin_y + place.y,
        TABLE_FLYOUT_W,
        table_flyout_height(),
    )
}

const MENU_REOPEN_COOLDOWN: std::time::Duration = std::time::Duration::from_millis(150);

impl Shell {
    fn close_menu(&mut self, cx: &mut Context<'_, Self>) {
        self.open_menu = None;
        self.clear_table_submenu_state(cx);
        self.menu_closed_at = Some(std::time::Instant::now());
    }

    pub(super) fn clear_table_submenu_state(&mut self, cx: &mut Context<'_, Self>) {
        self.table_submenu_open = false;
        self.editor.update(cx, |editor, cx| {
            editor.clear_table_menu_hover(cx);
        });
    }

    fn set_table_submenu_open(&mut self, open: bool, cx: &mut Context<'_, Self>) {
        if self.table_submenu_open == open {
            return;
        }
        self.table_submenu_open = open;
        cx.notify();
    }

    fn hit_table_flyout(&self, pos: Point<Pixels>, viewport: Size<Pixels>) -> bool {
        if !self.table_submenu_open {
            return false;
        }
        let Some((id, popup)) = self.open_menu else {
            return false;
        };
        if id != MenuId::Context {
            return false;
        }
        let (x, y, w, h) = flyout_screen_rect(popup, viewport, id, true);
        let px = f32::from(pos.x);
        let py = f32::from(pos.y);
        px >= x && px < x + w && py >= y && py < y + h
    }

    fn menu_available(&self) -> bool {
        self.menu_closed_at
            .is_none_or(|t| t.elapsed() > MENU_REOPEN_COOLDOWN)
    }

    pub(super) fn menu_label(
        &self,
        t: ShellTheme,
        this: Entity<Self>,
        id: MenuId,
        label: Key,
    ) -> Stateful<Div> {
        let open = self.open_menu.is_some_and(|(m, _)| m == id);
        let key = match id {
            MenuId::File => "menu-file",
            MenuId::Edit => "menu-edit",
            MenuId::View => "menu-view",
            MenuId::Go => "menu-go",
            MenuId::Help => "menu-help",
            MenuId::Context => "menu-context",
        };
        let left = Rc::new(Cell::new(px(0.)));
        div()
            .id(key)
            .debug_selector(move || format!("menubar:{}", label.debug_name()))
            .relative()
            .h(px(24.))
            .flex()
            .items_center()
            .rounded(px(5.))
            .text_color(if open { t.text } else { t.text_muted })
            .when(open, |d| d.bg(t.hover))
            .hover(move |s| s.bg(t.hover).text_color(t.text))
            .on_mouse_move({
                let this = this.clone();
                let left = Rc::clone(&left);
                move |_: &MouseMoveEvent, window: &mut Window, cx: &mut App| {
                    this.update(cx, |shell, cx| {
                        if let Some((open_id, _)) = shell.open_menu
                            && open_id != MenuId::Context
                            && open_id != id
                        {
                            let anchor = bar_menu_anchor(left.get(), id, window.viewport_size());
                            shell.clear_table_submenu_state(cx);
                            shell.open_menu = Some((id, anchor));
                            cx.notify();
                        }
                    });
                }
            })
            .on_click({
                let left = Rc::clone(&left);
                move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                    this.update(cx, |shell, cx| {
                        if shell.open_menu.is_some_and(|(m, _)| m == id) {
                            shell.close_menu(cx);
                            cx.notify();
                        } else if shell.menu_available() {
                            let anchor = bar_menu_anchor(left.get(), id, window.viewport_size());
                            shell.clear_table_submenu_state(cx);
                            shell.open_menu = Some((id, anchor));
                            cx.notify();
                        }
                    });
                }
            })
            .child(
                canvas(
                    {
                        let left = Rc::clone(&left);
                        move |bounds: Bounds<Pixels>, _, _| {
                            left.set(bounds.origin.x);
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(div().px(px(9.)).child(t18(label)))
    }

    pub(super) fn menu_popup(
        &self,
        t: ShellTheme,
        id: MenuId,
        pos: Point<Pixels>,
        in_table: bool,
        viewport: Size<Pixels>,
        this: Entity<Self>,
    ) -> Stateful<Div> {
        div()
            .id("menu-popup")
            .absolute()
            .left(pos.x)
            .top(pos.y)
            .w(px(MENU_W))
            .flex()
            .flex_col()
            .p(px(4.))
            .bg(t.panel_bg)
            .border_1()
            .border_color(t.border)
            .rounded(px(RADIUS))
            .when(id != MenuId::Context, |d| {
                d.shadow(vec![BoxShadow {
                    color: rgba(0x00000080).into(),
                    offset: point(px(0.), px(14.)),
                    blur_radius: px(40.),
                    spread_radius: px(0.),
                }])
            })
            .on_mouse_down_out({
                let this = this.clone();
                move |ev: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                    this.update(cx, |shell, cx| {
                        if shell.hit_table_flyout(ev.position, window.viewport_size()) {
                            return;
                        }
                        shell.close_menu(cx);
                        cx.notify();
                    });
                }
            })
            .children(entries_for(id, in_table).into_iter().map(|entry| {
                match entry {
                    Separator => self.menu_separator(t).into_any_element(),
                    Item { label, action } => self
                        .menu_row(t, label, action, false, this.clone())
                        .into_any_element(),
                    Submenu { label, items } => self
                        .table_submenu(t, label, items, pos, viewport, this.clone())
                        .into_any_element(),
                }
            }))
    }

    fn menu_separator(&self, t: ShellTheme) -> Div {
        div()
            .flex_none()
            .h(px(1.))
            .mx(px(6.))
            .my(px(4.))
            .bg(t.border_variant)
    }

    fn menu_row(
        &self,
        t: ShellTheme,
        label: Key,
        action: MenuAction,
        danger: bool,
        this: Entity<Self>,
    ) -> Stateful<Div> {
        let chord = action
            .chord_cmd()
            .and_then(|cmd| self.settings.keymap.chord_for(cmd))
            .map(Chord::display);
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
            .on_hover({
                let this = this.clone();
                move |hovered, _, cx| {
                    if !*hovered || matches!(action, MenuAction::Table(_)) {
                        return;
                    }
                    this.update(cx, |shell, cx| {
                        shell.set_table_submenu_open(false, cx);
                    });
                }
            })
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                this.update(cx, |shell, cx| {
                    shell.close_menu(cx);
                    shell.run_menu_action(action, window, cx);
                    cx.notify();
                });
            })
            .child(t18(label))
            .children(chord.map(|chord| {
                div()
                    .debug_selector(move || format!("menukb:{}", label.debug_name()))
                    .ml_auto()
                    .font_family(MONO_FONT)
                    .text_size(px(10.5))
                    .text_color(t.text_disabled)
                    .child(chord)
            }))
    }

    fn table_submenu(
        &self,
        t: ShellTheme,
        label: Key,
        items: &'static [TableMenuEntry],
        popup: Point<Pixels>,
        viewport: Size<Pixels>,
        this: Entity<Self>,
    ) -> Stateful<Div> {
        let open = self.table_submenu_open;
        div()
            .id("menu-table")
            .relative()
            .h(px(26.))
            .flex_none()
            .flex()
            .items_center()
            .px(px(10.))
            .rounded(px(5.))
            .text_size(px(12.5))
            .text_color(if open { t.text } else { t.text_muted })
            .when(open, |d| d.bg(t.hover))
            .hover(move |s| s.bg(t.hover).text_color(t.text))
            .on_hover({
                let this = this.clone();
                move |hovered, _, cx| {
                    if *hovered {
                        this.update(cx, |shell, cx| {
                            shell.set_table_submenu_open(true, cx);
                        });
                    }
                }
            })
            .child(t18(label))
            .child(
                div()
                    .ml_auto()
                    .text_size(px(10.5))
                    .text_color(if open { t.text } else { t.text_disabled })
                    .child("▸"),
            )
            .when(open, |d| {
                d.child(self.table_flyout(t, items, popup, viewport, this))
            })
    }

    fn table_flyout(
        &self,
        t: ShellTheme,
        items: &'static [TableMenuEntry],
        popup: Point<Pixels>,
        viewport: Size<Pixels>,
        this: Entity<Self>,
    ) -> Stateful<Div> {
        let place = place_table_flyout(popup, viewport, MenuId::Context, true);
        div()
            .id("menu-table-flyout")
            .absolute()
            .left(px(place.x))
            .top(px(place.y))
            .w(px(TABLE_FLYOUT_W))
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
            .children(items.iter().map(|entry| {
                match entry {
                    TableMenuEntry::Separator => self.menu_separator(t).into_any_element(),
                    TableMenuEntry::Item { label, op, danger } => self
                        .menu_row(t, *label, MenuAction::Table(*op), *danger, this.clone())
                        .into_any_element(),
                }
            }))
    }

    pub(super) fn run_menu_action(
        &mut self,
        action: MenuAction,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match action {
            MenuAction::Cmd(cmd) => {
                let _ = self.run_command(cmd, window, cx);
            }
            MenuAction::Table(op) => {
                self.editor.update(cx, |editor, cx| {
                    editor.apply_table_toolbar(op, window, cx);
                });
            }
            MenuAction::Nothing => {}
        }
    }

    fn open_find(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let seed = self.editor.read(cx).selection_seed();
        let query = self.find.update(cx, |find, cx| {
            find.open(window, seed, cx);
            find.query().to_string()
        });
        self.editor.update(cx, |editor, cx| {
            editor.search_open = true;
            editor.set_search_query(&query);
            cx.notify();
        });
        cx.notify();
    }

    pub(super) fn run_command(
        &mut self,
        cmd: Cmd,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        match cmd {
            Cmd::ToggleOutline => {
                self.outline_open = !self.outline_open;
                cx.notify();
            }
            Cmd::ToggleTheme => self.toggle_variant(cx),
            Cmd::Find => self.open_find(window, cx),
            Cmd::FindNext | Cmd::FindPrev => {
                if self.find.read(cx).open {
                    let dir = if cmd == Cmd::FindPrev { -1 } else { 1 };
                    self.editor.update(cx, |editor, cx| {
                        editor.search_step(dir);
                        cx.notify();
                    });
                } else {
                    self.open_find(window, cx);
                }
            }
            _ => {
                return self
                    .editor
                    .update(cx, |editor, cx| editor.run_command(cmd, window, cx));
            }
        }
        true
    }
}
