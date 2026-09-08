use super::caret::{caret_logical, resolve_text_frame};
use super::geometry::visible_geometry;
use super::inline_code::inline_code_plates_device;
use super::request::{FrameContext, FrameRequest, Pass};
use super::search_rects::search_highlights_device;
use super::selection::{caret_logical_y, selection_rects_logical};
use crate::snapshot::{Frame, LayoutSnapshot, SnapshotRevs, next_geometry_revision};
use md_core::Px;
use md_core::block::BlockId;
use md_core::doc::Cursor;
use md_layout::assembly::{Assembly, assemble_shared_with};
use md_layout::island::IslandSolver;
use std::collections::HashMap;
use std::rc::Rc;

pub fn compose(
    cx: FrameContext<'_>,
    req: &FrameRequest<'_>,
    solver: &dyn IslandSolver,
    col_tracks: Option<&HashMap<BlockId, Vec<Px>>>,
) -> Frame {
    let layout = cx.theme.layout_theme();
    let tree = md_layout::compose::compose(&cx.doc.document, &layout);
    let assembly = assemble_shared_with(Rc::new(tree), cx.env, cx.shaper, solver, col_tracks);
    from_assembly(
        cx,
        req,
        assembly,
        SnapshotRevs {
            document: cx.doc.document.revision(),
            layout: cx.env.viewport_width.to_bits(),
            viewport: req.viewport.0.to_bits().rotate_left(17) ^ req.viewport.1.to_bits(),
        },
    )
}

pub fn from_assembly(
    cx: FrameContext<'_>,
    req: &FrameRequest<'_>,
    assembly: Assembly,
    revs: SnapshotRevs,
) -> Frame {
    let pass = Pass::new(cx, &assembly, req);
    let geom = visible_geometry(&pass);
    let (scroll, snap) = (pass.scroll, pass.snap);

    let mut caret_device = None;
    if let Some((caret_x, cy, ch)) =
        caret_logical(&assembly, &geom.spans, cx.shaper, cx.env, req.cursor)
        && let Some(tf) = resolve_text_frame(&assembly, &geom.spans, cx.env, req.cursor.block)
    {
        let origin_y = tf.span_top + assembly.tree.style_of(tf.node).top_border_padding();
        let y = snap.snap(origin_y - scroll) + (cy - origin_y);
        caret_device = Some((caret_x, y, cx.theme.paint.caret_width, ch.max(1.0)));
    }

    let mut selection_device = Vec::new();
    if let Some((from, to)) = req.selection {
        for rect in
            selection_rects_logical(cx.doc, &assembly, &geom.spans, cx.shaper, cx.env, from, to)
        {
            let y = snap.snap(rect.origin_y - scroll) + rect.row_y;
            selection_device.push((rect.x, y, rect.width, rect.height.max(0.0)));
        }
    }

    let search = search_highlights_device(cx.doc, &pass, &geom, req.search_query, req.search_skip);

    let inline_code_device = inline_code_plates_device(&pass, &geom);

    let mut ime_device = Vec::new();
    if let Some((blk, r)) = req.marked.clone() {
        let c0 = Cursor {
            block: blk,
            offset: r.start,
        };
        let c1 = Cursor {
            block: blk,
            offset: r.end,
        };
        for rect in
            selection_rects_logical(cx.doc, &assembly, &geom.spans, cx.shaper, cx.env, c0, c1)
        {
            let y = snap.snap(rect.origin_y - scroll) + rect.row_y;
            ime_device.push((rect.x, y, rect.width, rect.height.max(1.0)));
        }
    }

    let caret_logical_y = caret_logical_y(&assembly, &geom.spans, cx.shaper, cx.env, req.cursor);
    Frame {
        snapshot: Rc::new(LayoutSnapshot {
            document_revision: revs.document,
            layout_revision: revs.layout,
            viewport_revision: revs.viewport,
            geometry_revision: next_geometry_revision(),
            total_height: geom.total_height,
            scroll,
            viewport: req.viewport,
            texts: geom.texts,
            decorations: geom.decorations,
            cells: geom.cells,
            caret_device,
            caret_logical_y,
            selection_device,
            inline_code_device,
            search_device: search.rest,
            search_active_device: search.active,
            ime_device,
            spans: geom.spans,
            content_atoms_painted: geom.content_atoms_painted,
            absent_visible: geom.absent_visible,
        }),
        assembly,
    }
}
