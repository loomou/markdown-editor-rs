use super::{CursorMotion, EditorView};
use gpui::{
    Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window, point, px, size,
};
use md_core::document::Command;
use std::ops::Range;

use crate::ui::text_input::{offset_to_utf16, range_from_utf16, range_to_utf16};

impl EditorView {
    fn input_range_from_utf16(&self, range: Option<Range<usize>>) -> Option<Range<usize>> {
        let range = range?;
        let text = self.state.doc.text(self.state.cursor.block)?;
        Some(range_from_utf16(text, &range))
    }
}

impl EntityInputHandler for EditorView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<String> {
        let s = self.state.doc.text(self.state.cursor.block)?;
        let range = range_from_utf16(s, &range_utf16);
        let back = range_to_utf16(s, &range);
        if back != range_utf16 {
            *adjusted = Some(back);
        }
        Some(s.get(range).unwrap_or("").to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore: bool,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<UTF16Selection> {
        let text = self.state.doc.text(self.state.cursor.block)?;
        let off = offset_to_utf16(text, self.state.cursor.offset);
        Some(UTF16Selection {
            range: off..off,
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<Range<usize>> {
        let range = self.live_ime_range()?;
        let text = self.state.doc.text(self.state.cursor.block)?;
        Some(range_to_utf16(text, &range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<'_, Self>) {
        self.state.marked = None;
        if self.ime_stale {
            self.ime_stale = false;
            return;
        }
        self.state.doc.commit_compose();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.bind_window(window);
        if self.unsaved_nav.is_some() || self.save_conflict.is_some() || self.insert_table.is_some()
        {
            return;
        }
        if self.ime_stale {
            self.state.marked = None;
            cx.notify();
            return;
        }
        if text.is_empty()
            && self.state.doc.is_composing()
            && let Some(sel) = self.state.doc.abort_compose()
        {
            self.restore_sel(sel);
            cx.notify();
            return;
        }
        let cmd = Command::Insert {
            text: text.to_string(),
        };
        let range = self.input_range_from_utf16(range);
        let sel = self.insert_sel(range);
        let c = if self.state.doc.is_composing() {
            self.state.doc.apply_ime_commit(sel, cmd)
        } else {
            self.state.doc.apply(sel, cmd)
        };
        self.place_cursor(c, CursorMotion::Move);
        self.state.marked = None;
        self.note_edit(cx);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        _new_selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.bind_window(window);
        if self.unsaved_nav.is_some() || self.save_conflict.is_some() || self.insert_table.is_some()
        {
            return;
        }
        if self.ime_stale {
            self.state.marked = None;
            cx.notify();
            return;
        }
        if new_text.is_empty() {
            self.state.marked = None;
            if let Some(sel) = self.state.doc.abort_compose() {
                self.restore_sel(sel);
            }
            cx.notify();
            return;
        }
        let range = self.input_range_from_utf16(range);
        let sel = self.insert_sel(range);
        let start = if sel.anchor.block == sel.head.block {
            sel.anchor.offset.min(sel.head.offset)
        } else {
            sel.head.offset
        };
        let c = self.state.doc.apply_marked(
            sel,
            Command::Insert {
                text: new_text.to_string(),
            },
        );
        self.state.marked = if start < c.offset {
            Some((c.block, start..c.offset))
        } else {
            None
        };
        self.state.cursor = c;
        self.state.selection = None;
        self.select_anchor = None;
        self.wake_caret();
        self.follow_caret = true;
        self.note_edit(cx);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<Bounds<Pixels>> {
        let d = self.state.diag.borrow();
        let (x, y, w, h) = d.caret?;
        Some(Bounds {
            origin: point(
                element_bounds.origin.x + px(x as f32),
                element_bounds.origin.y + px(y as f32),
            ),
            size: size(px(w.max(1.0) as f32), px(h as f32)),
        })
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<usize> {
        let text = self.state.doc.text(self.state.cursor.block)?;
        Some(offset_to_utf16(text, self.state.cursor.offset))
    }
}
