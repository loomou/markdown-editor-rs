use super::{CursorMotion, EditorView, PendingClick};
use gpui::{Context, Window};
use md_content::shaper::{GpuiShaper, ShapeMedia};
use md_core::Px;
use md_core::block::BlockKind;
use md_core::doc::Cursor;
use md_core::document::{Command, Sel};
use md_render::query::hit_test;
use md_render::snapshot::LayoutSnapshot;
use std::ops::Range;
use std::rc::Rc;

impl EditorView {
    pub(crate) fn editing_sel(&self) -> Sel {
        match self.state.selection {
            Some((a, b)) => Sel { anchor: a, head: b },
            None => Sel::collapsed(self.state.cursor),
        }
    }

    pub(super) fn live_ime_range(&self) -> Option<Range<usize>> {
        if self.ime_stale {
            return None;
        }
        match &self.state.marked {
            Some((_, r)) if r.start < r.end => Some(r.clone()),
            _ => None,
        }
    }

    pub(super) fn insert_sel(&self, range: Option<Range<usize>>) -> Sel {
        if let Some(r) = range {
            let blk = self.state.cursor.block;
            return Sel {
                anchor: Cursor {
                    block: blk,
                    offset: r.start,
                },
                head: Cursor {
                    block: blk,
                    offset: r.end,
                },
            };
        }
        if let Some((blk, r)) = &self.state.marked
            && r.start < r.end
            && !self.ime_stale
        {
            return Sel {
                anchor: Cursor {
                    block: *blk,
                    offset: r.start,
                },
                head: Cursor {
                    block: *blk,
                    offset: r.end,
                },
            };
        }
        self.editing_sel()
    }

    pub(crate) fn apply_cmd(&mut self, cmd: Command) {
        let c = self.state.doc.apply(self.editing_sel(), cmd);
        self.place_cursor(c, CursorMotion::Move);
    }

    pub(crate) fn undo(&mut self) {
        let composing = self.state.marked.is_some() || self.state.doc.is_composing();
        let start = self.state.doc.document.pending_changes().changes.len();
        if let Some(sel) = self.state.doc.undo() {
            self.restore_sel(sel);
        }
        self.drop_col_widths_touched_since(start);
        if composing {
            self.ime_stale = true;
        }
    }

    pub(crate) fn redo(&mut self) {
        let composing = self.state.marked.is_some() || self.state.doc.is_composing();
        let start = self.state.doc.document.pending_changes().changes.len();
        if let Some(sel) = self.state.doc.redo() {
            self.restore_sel(sel);
        }
        self.drop_col_widths_touched_since(start);
        if composing {
            self.ime_stale = true;
        }
    }

    pub(super) fn abort_composing(&mut self) {
        if self.state.marked.is_some() || self.state.doc.is_composing() {
            let _ = self.state.doc.abort_compose();
            self.state.marked = None;
            self.ime_stale = true;
        }
    }

    pub(crate) fn restore_sel(&mut self, sel: Sel) {
        self.state.marked = None;
        if sel.anchor == sel.head {
            self.place_cursor(sel.head, CursorMotion::Move);
        } else {
            self.select_anchor = Some(sel.anchor);
            self.place_cursor(sel.head, CursorMotion::Extend);
        }
        if self.search_open {
            let q = self.search.query.clone();
            self.refresh_search(&q);
        }
    }

    pub(crate) fn place_cursor(&mut self, c: Cursor, motion: CursorMotion) {
        self.place_cursor_biased(c, motion, md_core::doc::FocusBias::Neutral);
    }

    pub(super) fn place_cursor_biased(
        &mut self,
        c: Cursor,
        motion: CursorMotion,
        bias: md_core::doc::FocusBias,
    ) {
        match motion {
            CursorMotion::Extend => {
                let anchor = *self.select_anchor.get_or_insert(self.state.cursor);
                let (anchor, head) = self.state.doc.retarget_focus_range(anchor, c, bias);
                self.select_anchor = Some(anchor);
                self.state.selection = if anchor == head {
                    None
                } else {
                    Some((anchor, head))
                };
                self.state.cursor = head;
            }
            CursorMotion::Move => {
                let c = self.state.doc.retarget_focus_biased(c, bias);
                self.select_anchor = None;
                self.state.selection = None;
                self.state.cursor = c;
            }
        }

        self.wake_caret();
        self.follow_caret = true;
    }

    pub(super) fn place_pointer_cursor(&mut self, c: Cursor, motion: CursorMotion) {
        match motion {
            CursorMotion::Extend => self.place_cursor(c, CursorMotion::Extend),
            CursorMotion::Move => {
                let already = self.state.doc.block_edit() == Some(c.block);
                let well = self
                    .state
                    .doc
                    .kind(c.block)
                    .is_some_and(BlockKind::supports_block_edit);
                if well && !already {
                    let c = self
                        .state
                        .doc
                        .retarget_focus_without_block_edit(c, md_core::doc::FocusBias::Neutral);
                    self.select_anchor = None;
                    self.state.selection = None;
                    self.state.cursor = c;
                    self.wake_caret();
                    self.follow_caret = true;
                } else {
                    self.place_cursor(c, CursorMotion::Move);
                }
            }
        }
    }

    pub(super) fn apply_click_hit(&mut self, c: Cursor, click: PendingClick) {
        self.abort_composing();
        self.place_pointer_cursor(c, click.motion);
        if click.motion == CursorMotion::Move {
            self.select_anchor = Some(self.state.cursor);
            self.pending_open = if click.follow_link {
                self.state.doc.link_at(c).map(str::to_string)
            } else {
                None
            };
        } else {
            self.pending_open = None;
        }
        if click.select_word && click.motion == CursorMotion::Move {
            self.select_word_at(self.state.cursor);
            self.pending_open = None;
        }
    }

    pub(super) fn selection_covers(&self, c: Cursor) -> bool {
        let Some((a, h)) = self.state.selection else {
            return false;
        };
        if a.block == h.block {
            if c.block != a.block {
                return false;
            }
            let lo = a.offset.min(h.offset);
            let hi = a.offset.max(h.offset);
            return c.offset >= lo && c.offset <= hi;
        }
        let leaves = self.state.doc.text_leaves();
        let pos = |block| leaves.iter().position(|&b| b == block);
        let (Some(ai), Some(h_i), Some(ci)) = (pos(a.block), pos(h.block), pos(c.block)) else {
            return false;
        };
        let (lo, hi) = if (ai, a.offset) <= (h_i, h.offset) {
            ((ai, a.offset), (h_i, h.offset))
        } else {
            ((h_i, h.offset), (ai, a.offset))
        };
        let at = (ci, c.offset);
        at >= lo && at <= hi
    }

    pub(super) fn hit_cursor_at(
        &mut self,
        snapshot: &LayoutSnapshot,
        paint_rev: u64,
        pos: (Px, Px),
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Option<Cursor> {
        let scale = f64::from(window.scale_factor());
        self.doc_maps.sync(&self.state.doc);
        let media = ShapeMedia {
            mermaid_fitted: self.mermaid.fitted_snapshot(),
            math_metrics: self.math.metrics_snapshot(),
            math_gen: self.math.metrics_gen(),
            image_sizes: self.images.sizes_snapshot(),
            image_failed: self.images.failed_sources(),
            image_gen: self.images.sizes_gen(),
            link_dests: Rc::clone(&self.doc_maps.link_dests),
            link_raw: Rc::clone(&self.doc_maps.link_raw),
            block_image_dest: Rc::clone(&self.doc_maps.block_image_dest),
            block_code_lang: Rc::clone(&self.doc_maps.block_code_lang),
        };
        let shaper = GpuiShaper::new(
            &*window,
            cx,
            &self.state.theme,
            scale,
            self.state.shape_cache.clone(),
            media,
        );
        self.state.shape_cache.begin_frame(shaper.env_fingerprint());
        hit_test(snapshot, paint_rev, pos, &shaper, |id| {
            super::draw::well_scroll_xy(&self.well_scroll, id)
        })
    }

    pub(super) fn place_caret_at_hit(
        &mut self,
        snapshot: &LayoutSnapshot,
        paint_rev: u64,
        click: PendingClick,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let Some(c) = self.hit_cursor_at(snapshot, paint_rev, (click.x, click.y), window, cx)
        else {
            return false;
        };
        self.apply_click_hit(c, click);
        self.pending_click = None;
        cx.notify();
        true
    }

    pub(super) fn commit_block_edit_on_click(&mut self) {
        if self.state.selection.is_some() {
            return;
        }
        let c = self.state.cursor;
        if self.state.doc.block_edit() == Some(c.block) {
            return;
        }
        if !self
            .state
            .doc
            .kind(c.block)
            .is_some_and(BlockKind::supports_block_edit)
        {
            return;
        }
        self.place_cursor(c, CursorMotion::Move);
    }

    pub(crate) fn select_word_at(&mut self, at: Cursor) {
        let Some(text) = self.state.doc.caret_text(at.block) else {
            return;
        };
        let Some((lo, hi)) = md_core::document::word_span(text, at.offset) else {
            return;
        };
        if lo >= hi {
            return;
        }
        self.select_anchor = Some(Cursor {
            block: at.block,
            offset: lo,
        });
        self.place_cursor(
            Cursor {
                block: at.block,
                offset: hi,
            },
            CursorMotion::Extend,
        );
    }

    pub(crate) fn selection_seed(&self) -> String {
        let Some((a, b)) = self.state.selection else {
            return String::new();
        };
        if a.block != b.block {
            return String::new();
        }
        let Some(t) = self.state.doc.text(a.block) else {
            return String::new();
        };
        let (s, e) = if a.offset <= b.offset {
            (a.offset, b.offset)
        } else {
            (b.offset, a.offset)
        };
        let s = md_core::doc::floor_char_boundary(t, s.min(t.len()));
        let e = md_core::doc::floor_char_boundary(t, e.min(t.len())).max(s);
        t.get(s..e).unwrap_or("").to_string()
    }
}
