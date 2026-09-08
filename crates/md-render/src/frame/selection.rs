use super::caret::{caret_logical, resolve_text_frame};
use super::rows::{clamp_text_range, row_bands};
use crate::boxtree::{block_id_of, cmp_box_order, for_each_visible_text_box, text_box_id};
use crate::snapshot::AtomSpan;
use md_content::shaper::GpuiShaper;
use md_core::Px;
use md_core::block::BlockId;
use md_core::doc::{Cursor, Doc};
use md_layout::assembly::Assembly;
use md_layout::box_tree::{BoxTree, LayoutBoxId};
use md_layout::style::BoxLayoutEnvironment;
use std::cmp::Ordering;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub(super) struct TextRect {
    pub(super) x: Px,
    pub(super) origin_y: Px,
    pub(super) row_y: Px,
    pub(super) width: Px,
    pub(super) height: Px,
}

impl TextRect {
    fn logical_y(self) -> Px {
        self.origin_y + self.row_y
    }
}

fn selection_rects_in_leaf(
    assembly: &Assembly,
    spans: &BTreeMap<LayoutBoxId, AtomSpan>,
    shaper: &GpuiShaper,
    env: BoxLayoutEnvironment,
    block: BlockId,
    start: usize,
    end: usize,
) -> Vec<TextRect> {
    let mut out = Vec::new();
    if start >= end {
        return out;
    }
    let Some(tf) = resolve_text_frame(assembly, spans, env, block) else {
        return out;
    };
    let (node, base_x, span_top, inner) = (tf.node, tf.base_x, tf.span_top, tf.inner);
    let art = shaper.artifact(
        assembly.tree.text_of(node),
        assembly.tree.runs_of(node),
        inner,
        node.kind(),
        node.shape_ident(),
    );
    let range = clamp_text_range(&assembly.tree, node, start..end);
    let (a, b) = (range.start, range.end);
    if a >= b {
        return out;
    }
    let top0 = span_top + assembly.tree.style_of(node).top_border_padding();
    for band in row_bands(shaper, &art, a..b, node.inline_align(), inner) {
        out.push(TextRect {
            x: base_x + band.start_x,
            origin_y: top0,
            row_y: art.row_top(band.row),
            width: band.width(),
            height: art.row_height(band.row),
        });
    }
    out
}

pub(super) fn cmp_selection_cursors(doc: &Doc, tree: &BoxTree, a: Cursor, b: Cursor) -> Ordering {
    let a_box = text_box_id(tree, a.block);
    let b_box = text_box_id(tree, b.block);
    match (a_box, b_box) {
        (Some(ab), Some(bb)) => match cmp_box_order(tree, ab, bb) {
            Ordering::Equal => a.offset.cmp(&b.offset),
            order => order,
        },
        _ => match doc.cmp_reading_order(a.block, b.block) {
            Ordering::Equal => a.offset.cmp(&b.offset),
            order => order,
        },
    }
}

pub(super) fn selection_rects_logical(
    doc: &Doc,
    assembly: &Assembly,
    spans: &BTreeMap<LayoutBoxId, AtomSpan>,
    shaper: &GpuiShaper,
    env: BoxLayoutEnvironment,
    from: Cursor,
    to: Cursor,
) -> Vec<TextRect> {
    if from == to {
        return Vec::new();
    }
    let tree = assembly.tree.as_ref();
    let (start, end) = if cmp_selection_cursors(doc, tree, from, to) == Ordering::Greater {
        (to, from)
    } else {
        (from, to)
    };
    if start.block == end.block {
        return selection_rects_in_leaf(
            assembly,
            spans,
            shaper,
            env,
            start.block,
            start.offset,
            end.offset,
        );
    }
    let start_box = text_box_id(tree, start.block);
    let end_box = text_box_id(tree, end.block);
    let mut out = Vec::new();
    for_each_visible_text_box(tree, spans, |id| {
        let Some(block) = block_id_of(id) else {
            return;
        };
        let len = tree.text(id).len();
        let range = if Some(id) == start_box {
            start.offset..len
        } else if Some(id) == end_box {
            0..end.offset
        } else if leaf_between(doc, tree, start, end, id, start_box, end_box) {
            0..len
        } else {
            return;
        };
        out.extend(selection_rects_in_leaf(
            assembly,
            spans,
            shaper,
            env,
            block,
            range.start,
            range.end,
        ));
    });
    out
}

fn leaf_between(
    doc: &Doc,
    tree: &BoxTree,
    start: Cursor,
    end: Cursor,
    leaf: LayoutBoxId,
    start_box: Option<LayoutBoxId>,
    end_box: Option<LayoutBoxId>,
) -> bool {
    if let (Some(start_box), Some(end_box)) = (start_box, end_box) {
        return cmp_box_order(tree, start_box, leaf) == Ordering::Less
            && cmp_box_order(tree, leaf, end_box) == Ordering::Less;
    }
    let Some(leaf_block) = block_id_of(leaf) else {
        return false;
    };
    doc.cmp_reading_order(start.block, leaf_block) == Ordering::Less
        && doc.cmp_reading_order(leaf_block, end.block) == Ordering::Less
}

pub fn caret_logical_y(
    assembly: &Assembly,
    spans: &BTreeMap<LayoutBoxId, AtomSpan>,
    shaper: &GpuiShaper,
    env: BoxLayoutEnvironment,
    cur: Cursor,
) -> Option<Px> {
    caret_logical(assembly, spans, shaper, env, cur).map(|(_, y, _)| y)
}

pub fn selection_vertical_span(
    doc: &Doc,
    assembly: &Assembly,
    spans: &BTreeMap<LayoutBoxId, AtomSpan>,
    shaper: &GpuiShaper,
    env: BoxLayoutEnvironment,
    from: Cursor,
    to: Cursor,
) -> Option<(Px, Px, Px)> {
    let rects = selection_rects_logical(doc, assembly, spans, shaper, env, from, to);
    if !rects.is_empty() {
        let y0 = rects
            .iter()
            .copied()
            .map(TextRect::logical_y)
            .fold(f64::INFINITY, f64::min);
        let y1 = rects
            .iter()
            .copied()
            .map(|r| r.logical_y() + r.height)
            .fold(f64::NEG_INFINITY, f64::max);
        let lh = rects.iter().map(|r| r.height).fold(0.0_f64, f64::max);
        if y0.is_finite() && y1.is_finite() && y1 >= y0 {
            return Some((y0, y1, lh.max(1.0)));
        }
    }
    let a = caret_logical(assembly, spans, shaper, env, from)?;
    let b = caret_logical(assembly, spans, shaper, env, to).unwrap_or(a);
    let y0 = a.1.min(b.1);
    let y1 = (a.1 + a.2).max(b.1 + b.2);
    Some((y0, y1, a.2.max(b.2).max(1.0)))
}
