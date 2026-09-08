use super::unsaved::PendingNav;
use super::{CursorMotion, Direction, EditorView, LineEdge, PendingVertical};
use crate::keymap::{Cmd, Keymap};
use gpui::{Context, KeyDownEvent, Window};
use md_core::doc::{Cursor, next_char_boundary, prev_char_boundary};
use md_core::document::{Command, TableOp, TableStep};

fn word_bound(text: &str, offset: usize, dir: Direction) -> usize {
    match dir {
        Direction::Next => md_core::document::next_word_boundary(text, offset),
        Direction::Prev => md_core::document::prev_word_boundary(text, offset),
    }
}

impl EditorView {
    pub(super) fn horizontal(&mut self, dir: Direction, motion: CursorMotion) {
        let cur = self.state.cursor;
        let Some(text) = self.state.doc.text(cur.block) else {
            return;
        };
        let at_edge = match dir {
            Direction::Prev => cur.offset == 0,
            Direction::Next => cur.offset >= text.len(),
        };
        let bias = match dir {
            Direction::Prev => md_core::doc::FocusBias::Left,
            Direction::Next => md_core::doc::FocusBias::Right,
        };
        if at_edge {
            if self.state.doc.in_table(cur.block) {
                let step = match dir {
                    Direction::Prev => TableStep::PrevCell,
                    Direction::Next => TableStep::NextCell,
                };
                if let Some(next) = self.state.doc.table_step(cur, step) {
                    self.place_cursor_biased(next, motion, bias);
                    return;
                }
            }
            if let Some(next) = self.state.doc.sibling_leaf(cur.block, dir.sign()) {
                let off = match dir {
                    Direction::Prev => self.state.doc.text(next).map_or(0, |t| t.len()),
                    Direction::Next => 0,
                };
                self.place_cursor_biased(
                    Cursor {
                        block: next,
                        offset: off,
                    },
                    motion,
                    bias,
                );
            }
            return;
        }
        let off = match dir {
            Direction::Prev => prev_char_boundary(text, cur.offset),
            Direction::Next => next_char_boundary(text, cur.offset),
        };
        self.place_cursor_biased(
            Cursor {
                block: cur.block,
                offset: off,
            },
            motion,
            bias,
        );
    }

    pub(super) fn word(&mut self, dir: Direction, motion: CursorMotion) {
        let cur = self.state.cursor;
        let Some(text) = self.state.doc.text(cur.block) else {
            return;
        };
        let off = word_bound(text, cur.offset, dir);
        if off != cur.offset {
            let bias = match dir {
                Direction::Prev => md_core::doc::FocusBias::Left,
                Direction::Next => md_core::doc::FocusBias::Right,
            };
            self.place_cursor_biased(
                Cursor {
                    block: cur.block,
                    offset: off,
                },
                motion,
                bias,
            );
            return;
        }
        self.horizontal(dir, motion);
    }

    pub(super) fn line_edge(&mut self, edge: LineEdge, motion: CursorMotion) {
        let cur = self.state.cursor;
        let Some(text) = self.state.doc.text(cur.block) else {
            return;
        };
        let off = match edge {
            LineEdge::Start => 0,
            LineEdge::End => text.len(),
        };
        self.place_cursor(
            Cursor {
                block: cur.block,
                offset: off,
            },
            motion,
        );
    }

    #[cfg(target_os = "macos")]
    fn document_edge(&mut self, dir: Direction, motion: CursorMotion) {
        let leaves = self.state.doc.text_leaves();
        let Some(&block) = (match dir {
            Direction::Prev => leaves.first(),
            Direction::Next => leaves.last(),
        }) else {
            return;
        };
        let offset = match dir {
            Direction::Prev => 0,
            Direction::Next => self.state.doc.text(block).map_or(0, |t| t.len()),
        };
        self.place_cursor(Cursor { block, offset }, motion);
    }

    fn key_backspace(&mut self, cx: &mut Context<'_, Self>) {
        self.apply_cmd(Command::DeleteBackward);
        self.note_edit(cx);
    }

    #[cfg(target_os = "macos")]
    fn key_backspace_to_line_start(&mut self, cx: &mut Context<'_, Self>) {
        if self.state.selection.is_some() || self.state.marked.is_some() {
            self.key_backspace(cx);
            return;
        }
        let cur = self.state.cursor;
        if cur.offset == 0 {
            return;
        }
        self.state.selection = Some((
            Cursor {
                block: cur.block,
                offset: 0,
            },
            cur,
        ));
        self.key_backspace(cx);
    }

    fn key_delete(&mut self, cx: &mut Context<'_, Self>) {
        self.apply_cmd(Command::DeleteForward);
        self.note_edit(cx);
    }

    fn key_enter(&mut self, shift: bool, cx: &mut Context<'_, Self>) {
        if shift {
            self.apply_cmd(Command::SoftBreak);
        } else {
            self.apply_cmd(Command::Break);
        }
        self.note_edit(cx);
    }

    fn key_table_tab(&mut self, shift: bool, cx: &mut Context<'_, Self>) {
        let cur = self.state.cursor;
        if shift {
            if let Some(c) = self.state.doc.table_step(cur, TableStep::PrevCell) {
                self.place_cursor(c, CursorMotion::Move);
            }
            return;
        }
        if let Some(c) = self.state.doc.table_step(cur, TableStep::NextCell) {
            self.place_cursor(c, CursorMotion::Move);
            return;
        }
        self.apply_cmd(Command::Table(TableOp::InsertRowBelow));
        if let Some(c) = self
            .state
            .doc
            .table_step(self.state.cursor, TableStep::RowHome)
        {
            self.place_cursor(c, CursorMotion::Move);
        }
        self.note_edit(cx);
    }

    fn key_indent(&mut self, outdent: bool, cx: &mut Context<'_, Self>) {
        if outdent {
            self.apply_cmd(Command::Outdent);
        } else {
            self.apply_cmd(Command::Indent);
        }
        self.note_edit(cx);
    }

    pub(crate) fn caret_in_table(&self) -> bool {
        self.state.doc.in_table(self.state.cursor.block)
    }

    pub(crate) fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    pub(crate) fn set_keymap(&mut self, keymap: Keymap, cx: &mut Context<'_, Self>) {
        if self.keymap == keymap {
            return;
        }
        self.keymap = keymap;
        cx.notify();
    }

    pub(crate) fn run_command(
        &mut self,
        cmd: Cmd,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        match cmd {
            Cmd::New => self.request_nav(PendingNav::New, window, cx),
            Cmd::Open => self.request_nav(PendingNav::Open, window, cx),
            Cmd::Save => self.save(window, cx),
            Cmd::SaveAs => self.save_as(window, cx),
            Cmd::Quit => self.request_nav(PendingNav::Close, window, cx),
            Cmd::Undo => {
                self.undo();
                self.note_edit(cx);
                cx.notify();
            }
            Cmd::Redo => {
                self.redo();
                self.note_edit(cx);
                cx.notify();
            }
            Cmd::Cut => {
                self.cut(cx);
                cx.notify();
            }
            Cmd::Copy => self.copy(cx),
            Cmd::Paste => {
                self.paste(cx);
                cx.notify();
            }
            Cmd::SelectAll => {
                self.select_all();
                cx.notify();
            }
            Cmd::Find => return false,
            Cmd::FindNext | Cmd::FindPrev => {
                if !self.search_open {
                    return false;
                }
                self.search_step(if cmd == Cmd::FindPrev { -1 } else { 1 });
                cx.notify();
            }
            Cmd::InsertTable => self.open_insert_table(window, cx),
            Cmd::TableRowBelow => {
                if !self.caret_in_table() {
                    return false;
                }
                self.apply_cmd(Command::Table(TableOp::InsertRowBelow));
                self.note_edit(cx);
                cx.notify();
            }
            Cmd::ToggleOutline | Cmd::ToggleTheme => return false,
        }
        true
    }

    fn chord_arrow(&mut self, key: &str, m: &gpui::Modifiers, shift: bool) -> bool {
        let motion = CursorMotion::from_shift(shift);
        if crate::ui::chord::primary_down(m) {
            #[cfg(target_os = "macos")]
            {
                match key {
                    "left" => self.line_edge(LineEdge::Start, motion),
                    "right" => self.line_edge(LineEdge::End, motion),
                    "up" => self.document_edge(Direction::Prev, motion),
                    "down" => self.document_edge(Direction::Next, motion),
                    _ => return false,
                }
                return true;
            }
            #[cfg(not(target_os = "macos"))]
            {
                let dir = match key {
                    "left" => Direction::Prev,
                    "right" => Direction::Next,
                    _ => return false,
                };
                self.word(dir, motion);
                return true;
            }
        }
        let dir = match key {
            "left" => Direction::Prev,
            "right" => Direction::Next,
            _ => return false,
        };
        if crate::ui::chord::word_mod_down(m) {
            self.word(dir, motion);
            return true;
        }
        false
    }

    fn on_media_zoom_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let m = &ev.keystroke.modifiers;
        let key = ev.keystroke.key.as_str();
        if key == "escape" && !crate::ui::chord::has_chord(m) {
            self.close_media_zoom(cx);
            window.focus(&self.focus);
            return true;
        }
        if !ev.is_held
            && let Some(cmd) = self.keymap.lookup(&ev.keystroke)
        {
            return match cmd {
                Cmd::New | Cmd::Open | Cmd::Save | Cmd::SaveAs | Cmd::Quit => {
                    self.run_command(cmd, window, cx)
                }
                Cmd::Find
                | Cmd::FindNext
                | Cmd::FindPrev
                | Cmd::ToggleOutline
                | Cmd::ToggleTheme => false,
                _ => true,
            };
        }
        true
    }

    pub(crate) fn on_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        self.bind_window(window);
        if self.on_unsaved_key(ev, window, cx) {
            cx.stop_propagation();
            return true;
        }
        if self.on_insert_table_key(ev, window, cx) {
            cx.stop_propagation();
            return true;
        }
        if self.media_zoom.is_some() {
            let handled = self.on_media_zoom_key(ev, window, cx);
            if handled {
                cx.stop_propagation();
            }
            return handled;
        }
        let m = &ev.keystroke.modifiers;
        let shift = m.shift;
        let key = ev.keystroke.key.as_str();
        if !ev.is_held
            && let Some(cmd) = self.keymap.lookup(&ev.keystroke)
            && self.run_command(cmd, window, cx)
        {
            cx.stop_propagation();
            return true;
        }
        if self.chord_arrow(key, m, shift) {
            self.wake_caret();
            cx.notify();
            cx.stop_propagation();
            return true;
        }
        #[cfg(target_os = "macos")]
        if crate::ui::chord::primary_down(m) && key == "backspace" {
            self.key_backspace_to_line_start(cx);
            self.wake_caret();
            cx.notify();
            cx.stop_propagation();
            return true;
        }
        if crate::ui::chord::primary_down(m) && !m.shift && matches!(key, "[" | "]") {
            self.key_indent(key == "]", cx);
            self.wake_caret();
            cx.notify();
            cx.stop_propagation();
            return true;
        }
        if crate::ui::chord::has_chord(m) {
            return false;
        }
        match key {
            "left" => self.horizontal(Direction::Prev, CursorMotion::from_shift(shift)),
            "right" => self.horizontal(Direction::Next, CursorMotion::from_shift(shift)),

            "up" => {
                self.pending_vertical = Some(PendingVertical {
                    dir: Direction::Prev,
                    motion: CursorMotion::from_shift(shift),
                })
            }
            "down" => {
                self.pending_vertical = Some(PendingVertical {
                    dir: Direction::Next,
                    motion: CursorMotion::from_shift(shift),
                })
            }
            "home" => self.line_edge(LineEdge::Start, CursorMotion::from_shift(shift)),
            "end" => self.line_edge(LineEdge::End, CursorMotion::from_shift(shift)),
            "backspace" => self.key_backspace(cx),
            "delete" => self.key_delete(cx),
            "enter" => self.key_enter(shift, cx),
            "tab" => {
                if self.caret_in_table() {
                    self.key_table_tab(shift, cx);
                } else {
                    self.key_indent(shift, cx);
                }
            }
            "escape" => {
                if self.search_open {
                    if let Some(find) = self.find_bar.as_ref().and_then(|w| w.upgrade()) {
                        find.update(cx, |f, cx| {
                            f.dismiss(cx);
                        });
                    }
                    self.clear_search();
                    window.focus(&self.focus);
                } else if self.table_ui.reorder.is_some() {
                    self.cancel_table_reorder(cx);
                    window.focus(&self.focus);
                } else if self.table_ui.picker_open {
                    self.close_table_picker(true, cx);
                    window.focus(&self.focus);
                } else {
                    self.state.selection = None;
                    self.select_anchor = None;
                    if let Some(c) = self
                        .state
                        .doc
                        .table_step(self.state.cursor, TableStep::ExitAfter)
                    {
                        self.place_cursor(c, CursorMotion::Move);
                    }
                }
            }

            "f5" => {
                if !ev.is_held {
                    self.state.show_fps = !self.state.show_fps;
                }
            }
            "f6" => {
                if !ev.is_held {
                    self.state.stress_redraw = !self.state.stress_redraw;
                }
            }
            _ => return false,
        }
        self.wake_caret();
        cx.notify();
        true
    }
}

#[cfg(test)]
mod word_bound_tests {
    use super::word_bound;
    use crate::view::Direction;

    #[test]
    fn skips_ascii_words_and_spaces() {
        let t = "hello world";
        assert_eq!(word_bound(t, 0, Direction::Next), 6);
        assert_eq!(word_bound(t, 3, Direction::Next), 6);
        assert_eq!(word_bound(t, 5, Direction::Next), 6);
        assert_eq!(word_bound(t, 6, Direction::Next), 11);
        assert_eq!(word_bound(t, 11, Direction::Next), 11);
        assert_eq!(word_bound(t, 11, Direction::Prev), 6);
        assert_eq!(word_bound(t, 6, Direction::Prev), 0);
        assert_eq!(word_bound(t, 3, Direction::Prev), 0);
    }

    #[test]
    fn skips_cjk_runs() {
        let t = "你好 世界";
        let second = "你好 ".len();
        assert_eq!(word_bound(t, 0, Direction::Next), second);
        assert_eq!(word_bound(t, t.len(), Direction::Prev), second);
    }
}
