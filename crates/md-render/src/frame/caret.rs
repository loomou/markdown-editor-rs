use crate::boxtree::inline_offset;
use crate::snapshot::AtomSpan;
use md_content::shaper::GpuiShaper;
use md_core::Px;
use md_core::block::BlockId;
use md_core::doc::Cursor;
use md_layout::assembly::Assembly;
use md_layout::box_tree::{BoxNode, BoxOwner, BoxTree, LayoutBoxId};
use md_layout::style::BoxLayoutEnvironment;
use std::collections::BTreeMap;

pub(super) struct TextFrame<'a> {
    pub(super) node: &'a BoxNode,

    pub(super) base_x: Px,

    pub(super) span_top: Px,

    pub(super) inner: Px,
}

pub(super) fn resolve_text_frame<'a>(
    assembly: &'a Assembly,
    spans: &BTreeMap<LayoutBoxId, AtomSpan>,
    env: BoxLayoutEnvironment,
    block: BlockId,
) -> Option<TextFrame<'a>> {
    let leaf = LayoutBoxId::frame(block);
    if let Some(sp) = spans.get(&leaf) {
        let node = assembly.tree.get(leaf);
        let style = assembly.tree.style_of(node);
        let avail = assembly.tree.avail_width(leaf, env.viewport_width);
        let inner = (avail - style.inline_border_padding()).max(0.0);
        let base_x = inline_offset(&assembly.tree, leaf) + style.border.left + style.padding.left;
        return Some(TextFrame {
            node,
            base_x,
            span_top: sp.top,
            inner,
        });
    }

    let cell = LayoutBoxId {
        owner: BoxOwner::Block(block),
        role: md_layout::box_tree::BoxRole::Cell,
        local_key: 0,
    };
    let node = assembly.tree.nodes().get(&cell)?;
    let row_box = node.parent()?;
    let sp = spans.get(&row_box)?;

    let g = assembly.geometries.get(&row_box)?;
    let cg = g.cells.iter().find(|c| c.cell_box == cell)?;
    let x = inline_offset(&assembly.tree, row_box) + cg.x;
    let style = assembly.tree.style_of(node);
    let inner = (cg.width - style.inline_border_padding()).max(0.0);
    let base_x = x + style.border.left + style.padding.left;
    Some(TextFrame {
        node,
        base_x,
        span_top: sp.top,
        inner,
    })
}

pub fn caret_logical(
    assembly: &Assembly,
    spans: &BTreeMap<LayoutBoxId, AtomSpan>,
    shaper: &GpuiShaper,
    env: BoxLayoutEnvironment,
    cur: Cursor,
) -> Option<(Px, Px, Px)> {
    let tf = resolve_text_frame(assembly, spans, env, cur.block)?;
    let (x, y, h) = caret_in_box(
        &assembly.tree,
        tf.node,
        shaper,
        tf.span_top,
        tf.inner,
        cur.offset,
    );
    Some((tf.base_x + x, y, h))
}

pub(crate) fn caret_in_box(
    tree: &BoxTree,
    node: &BoxNode,
    shaper: &GpuiShaper,
    top: Px,
    inner: Px,
    offset: usize,
) -> (Px, Px, Px) {
    let art = shaper.artifact(
        tree.text_of(node),
        tree.runs_of(node),
        inner,
        node.kind(),
        node.shape_ident(),
    );
    let (x, row) = shaper.position_for_offset(&art, offset, node.inline_align(), inner);
    let (dy, h) = shaper.caret_ink(node.shape_kind(), node.type_slot(), &art, row);
    let y = top + tree.style_of(node).top_border_padding() + art.row_top(row) + dy;
    (x, y, h)
}
