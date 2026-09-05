use super::EditorView;
use gpui::Context;
use md_core::Px;
use md_core::block::{BlockId, BlockKind};
use md_core::doc::Doc;
use md_core::document::{
    DocChange, DocumentArena, NodeId, TABLE_PICKER_MAX_COLS, TableLoc, TableOp,
};
use md_layout::island::TABLE_MIN_COL_WIDTH;
use std::collections::HashMap;

const SEAM_HIT_PX: Px = 4.0;

#[derive(Clone, Copy, Debug)]
pub(super) struct ColResizeDrag {
    pub table: BlockId,
    pub col: usize,
    pub start_x: Px,
    pub start_left: Px,
    pub start_right: Px,
}

pub(super) struct ColSeamHit {
    pub table: BlockId,
    pub col: usize,
    pub start_x: Px,
    pub tracks: Vec<Px>,
}

struct TableGeom {
    cols: usize,
    widths: Vec<Option<Px>>,
    rights: Vec<Option<Px>>,
    y0: Px,
    y1: Px,
}

pub(super) type CellHit = (BlockId, (Px, Px, Px, Px));

pub(super) fn hit_col_seam(doc: &Doc, cells: &[CellHit], pos: (Px, Px)) -> Option<ColSeamHit> {
    let mut tables: HashMap<BlockId, TableGeom> = HashMap::new();
    for &(block, (x, y, w, h)) in cells {
        let Some(loc) = doc.table_loc(block) else {
            continue;
        };
        let e = tables.entry(loc.table).or_insert_with(|| TableGeom {
            cols: loc.cols,
            widths: vec![None; loc.cols],
            rights: vec![None; loc.cols],
            y0: y,
            y1: y + h,
        });
        e.y0 = e.y0.min(y);
        e.y1 = e.y1.max(y + h);
        if loc.col < e.cols {
            e.widths[loc.col] = Some(w);
            e.rights[loc.col] = Some(x + w);
        }
    }
    let mut best: Option<(Px, ColSeamHit)> = None;
    for (table, e) in tables {
        if pos.1 < e.y0 || pos.1 > e.y1 {
            continue;
        }
        if e.cols < 2 {
            continue;
        }
        let Some(tracks) = e.widths.iter().copied().collect::<Option<Vec<_>>>() else {
            continue;
        };
        for col in 0..e.cols.saturating_sub(1) {
            let Some(seam_x) = e.rights[col] else {
                continue;
            };
            let dist = (pos.0 - seam_x).abs();
            if dist <= SEAM_HIT_PX {
                let better = best.as_ref().is_none_or(|(d, _)| dist < *d);
                if better {
                    best = Some((
                        dist,
                        ColSeamHit {
                            table,
                            col,
                            start_x: seam_x,
                            tracks: tracks.clone(),
                        },
                    ));
                }
            }
        }
    }
    best.map(|(_, hit)| hit)
}

impl EditorView {
    pub(super) fn prune_table_col_widths(&mut self) {
        let mut drop = Vec::new();
        let mut align = Vec::new();
        for (&id, tracks) in &self.table_ui.col_widths {
            if self.state.doc.kind(id) != Some(BlockKind::Table) {
                drop.push(id);
                continue;
            }
            if let Some(n) = table_first_row_cols(&self.state.doc, id)
                && n != tracks.len()
            {
                align.push((id, n));
            }
        }
        for id in drop {
            self.table_ui.col_widths.remove(&id);
        }
        for (id, n) in align {
            self.align_col_tracks(id, n);
        }
    }

    pub(crate) fn adjust_col_tracks_for_op(&mut self, loc: TableLoc, op: &TableOp) {
        match op {
            TableOp::DeleteTable => {
                self.table_ui.col_widths.remove(&loc.table);
            }
            TableOp::InsertColumnLeft => {
                if let Some(t) = self.table_ui.col_widths.get_mut(&loc.table) {
                    let w = insert_track_width(t);
                    let i = loc.col.min(t.len());
                    t.insert(i, w);
                }
            }
            TableOp::InsertColumnRight => {
                if let Some(t) = self.table_ui.col_widths.get_mut(&loc.table) {
                    let w = insert_track_width(t);
                    let i = (loc.col + 1).min(t.len());
                    t.insert(i, w);
                }
            }
            TableOp::DeleteColumn => {
                if let Some(t) = self.table_ui.col_widths.get_mut(&loc.table)
                    && loc.col < t.len()
                {
                    t.remove(loc.col);
                }
            }
            TableOp::MoveColumnLeft if loc.col > 0 => {
                if let Some(t) = self.table_ui.col_widths.get_mut(&loc.table)
                    && loc.col < t.len()
                {
                    t.swap(loc.col, loc.col - 1);
                }
            }
            TableOp::MoveColumnRight => {
                if let Some(t) = self.table_ui.col_widths.get_mut(&loc.table)
                    && loc.col + 1 < t.len()
                {
                    t.swap(loc.col, loc.col + 1);
                }
            }
            TableOp::MoveColumnTo { index } => {
                if let Some(t) = self.table_ui.col_widths.get_mut(&loc.table)
                    && loc.col < t.len()
                {
                    let to = (*index).min(t.len() - 1);
                    if loc.col != to {
                        let w = t.remove(loc.col);
                        t.insert(to, w);
                    }
                }
            }
            TableOp::Resize { cols, .. } => {
                let n = (*cols).clamp(1, TABLE_PICKER_MAX_COLS);
                self.align_col_tracks(loc.table, n);
            }
            _ => {}
        }
    }

    pub(super) fn drop_col_widths_touched_since(&mut self, start: usize) {
        let doc = &self.state.doc;
        let mut touched = Vec::new();
        if let Some(changes) = doc.document.pending_changes().changes.get(start..) {
            for change in changes {
                let DocChange::TreeSpliced {
                    parent, inserted, ..
                } = change
                else {
                    continue;
                };

                for id in std::iter::once(*parent).chain(inserted.iter().copied()) {
                    if let Some(table) = enclosing_table(&doc.document.arena, id) {
                        touched.push(table);
                    }
                }
            }
        }
        for table in touched {
            self.table_ui.col_widths.remove(&table);
        }
    }

    fn align_col_tracks(&mut self, table: BlockId, cols: usize) {
        let Some(t) = self.table_ui.col_widths.get_mut(&table) else {
            return;
        };
        if t.len() == cols {
            return;
        }
        if t.len() > cols {
            t.truncate(cols);
        } else {
            let pad = if t.is_empty() {
                TABLE_MIN_COL_WIDTH
            } else {
                t.iter().copied().sum::<Px>() / t.len() as Px
            };
            t.resize(cols, pad.max(TABLE_MIN_COL_WIDTH));
        }
    }

    pub(super) fn begin_col_resize(&mut self, hit: ColSeamHit, cx: &mut Context<'_, Self>) {
        if hit.col + 1 >= hit.tracks.len() {
            return;
        }
        let start_left = hit.tracks[hit.col];
        let start_right = hit.tracks[hit.col + 1];
        self.table_ui.col_widths.insert(hit.table, hit.tracks);
        self.table_ui.col_resize = Some(ColResizeDrag {
            table: hit.table,
            col: hit.col,
            start_x: hit.start_x,
            start_left,
            start_right,
        });
        self.dragging = false;
        self.pending_click = None;
        self.drag_pointer = None;
        cx.notify();
    }

    pub(super) fn col_resize_to(&mut self, x: Px, cx: &mut Context<'_, Self>) {
        let Some(drag) = self.table_ui.col_resize else {
            return;
        };
        let Some(tracks) = self.table_ui.col_widths.get_mut(&drag.table) else {
            return;
        };
        if drag.col + 1 >= tracks.len() {
            return;
        }
        let pair = drag.start_left + drag.start_right;
        let max_left = (pair - TABLE_MIN_COL_WIDTH).max(TABLE_MIN_COL_WIDTH);
        let left = (drag.start_left + (x - drag.start_x)).clamp(TABLE_MIN_COL_WIDTH, max_left);
        tracks[drag.col] = left;
        tracks[drag.col + 1] = pair - left;
        cx.notify();
    }

    pub(super) fn end_col_resize(&mut self, cx: &mut Context<'_, Self>) {
        if self.table_ui.col_resize.take().is_some() {
            cx.notify();
        }
    }
}

fn insert_track_width(tracks: &[Px]) -> Px {
    if tracks.is_empty() {
        TABLE_MIN_COL_WIDTH
    } else {
        (tracks.iter().copied().sum::<Px>() / tracks.len() as Px).max(TABLE_MIN_COL_WIDTH)
    }
}

fn table_first_row_cols(doc: &Doc, table: BlockId) -> Option<usize> {
    let nid = doc.document.live_id(table)?;
    for child in doc.document.arena.children(nid) {
        if doc.document.arena.get(child).map(|n| n.kind) == Some(BlockKind::TableRow) {
            return Some(doc.document.arena.children(child).count());
        }
    }
    None
}

fn enclosing_table(arena: &DocumentArena, id: NodeId) -> Option<BlockId> {
    let mut cur = id;
    loop {
        let node = arena.get(cur)?;
        if node.kind == BlockKind::Table {
            return Some(cur.index);
        }
        cur = node.parent?;
    }
}
