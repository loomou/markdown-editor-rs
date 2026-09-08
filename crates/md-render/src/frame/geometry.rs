use super::decoration::collect_decorations_window;
use super::request::Pass;
use crate::boxtree::{block_id_of, inline_offset};
use crate::snapshot::{AtomSpan, CellPiece, DecorationPiece, TextPiece};
use md_core::Px;
use md_layout::box_tree::{BoxChildren, LayoutBoxId};
use md_layout::spine::{FlowItemKind, FlowWindow};
use std::collections::BTreeMap;

pub(super) struct VisibleGeom {
    pub(super) total_height: Px,
    pub(super) spans: BTreeMap<LayoutBoxId, AtomSpan>,
    pub(super) texts: Vec<TextPiece>,
    pub(super) decorations: Vec<DecorationPiece>,
    pub(super) cells: Vec<CellPiece>,
    pub(super) absent_visible: Vec<LayoutBoxId>,
    pub(super) content_atoms_painted: u64,
}

pub(super) fn visible_geometry(pass: &Pass<'_>) -> VisibleGeom {
    if let Some(window) = &pass.assembly.window {
        geometry_from_window(pass, window)
    } else {
        VisibleGeom {
            total_height: 0.0,
            spans: BTreeMap::new(),
            texts: Vec::new(),
            decorations: Vec::new(),
            cells: Vec::new(),
            absent_visible: Vec::new(),
            content_atoms_painted: 0,
        }
    }
}

fn paint_content(pass: &Pass<'_>, box_id: LayoutBoxId, a_top: Px, out: &mut VisibleGeom) {
    let (assembly, env, shaper, snap, scroll) =
        (pass.assembly, pass.env, pass.shaper, pass.snap, pass.scroll);
    let node = assembly.tree.get(box_id);
    match node.children() {
        BoxChildren::Island(_) => {
            let row_x = inline_offset(&assembly.tree, box_id);
            if let Some(g) = assembly.geometries.get(&box_id) {
                for cg in &g.cells {
                    let Some(block) = block_id_of(cg.cell_box) else {
                        out.absent_visible.push(cg.cell_box);
                        continue;
                    };
                    let Some(table) = assembly
                        .tree
                        .ancestor_table(cg.cell_box)
                        .and_then(block_id_of)
                    else {
                        out.absent_visible.push(cg.cell_box);
                        continue;
                    };
                    let cell = assembly.tree.get(cg.cell_box);
                    let cell_style = assembly.tree.style_of(cell);
                    let inner_w = (cg.width - cell_style.inline_border_padding()).max(0.0);
                    let art = shaper.artifact(
                        assembly.tree.text_of(cell),
                        assembly.tree.runs_of(cell),
                        inner_w,
                        cell.kind(),
                        cell.shape_ident(),
                    );
                    let (t, b) =
                        snap.snap_range(a_top - scroll, a_top + cg.border_box_height - scroll);
                    let cx = row_x + cg.x;
                    let co_x = cx + cell_style.border.left + cell_style.padding.left;
                    let co_y = snap.snap(a_top + cell_style.top_border_padding() - scroll);
                    out.cells.push(CellPiece {
                        cell_box: cg.cell_box,
                        block,
                        table,
                        rect_device: (cx, t, cg.width, (b - t).max(0.0)),
                        content_origin_device: (co_x, co_y),
                        art,
                        content_width: inner_w,
                        align: cell.inline_align(),
                        header: cell.extra().table_header()
                            || cell.type_slot() == md_layout::box_tree::TypeSlot::TableHeader,
                    });
                }
            } else {
                out.absent_visible.push(box_id);
            }
            out.content_atoms_painted += 1;
        }
        BoxChildren::None => {
            let Some(block) = block_id_of(box_id) else {
                return;
            };
            let avail = assembly.tree.avail_width(box_id, env.viewport_width);
            let style = assembly.tree.style_of(node);
            let inner_w = (avail - style.inline_border_padding()).max(0.0);
            let art = shaper.artifact(
                assembly.tree.text_of(node),
                assembly.tree.runs_of(node),
                inner_w,
                node.shape_kind(),
                node.shape_ident(),
            );
            let x = inline_offset(&assembly.tree, box_id);
            let co_x_logical = x + style.border.left + style.padding.left;
            let co_y_logical = a_top + style.top_border_padding();
            let co_y_device = snap.snap(co_y_logical - scroll);
            let view_height =
                shaper.well_view_height(node.shape_kind(), node.edit_source(), art.height);
            out.texts.push(TextPiece {
                box_id,
                block,
                kind: node.shape_kind(),
                edit_source: node.edit_source(),
                content_origin_device: (co_x_logical, co_y_device),
                content_width: inner_w,
                view_height,
                art,
                align: node.inline_align(),
            });
            out.content_atoms_painted += 1;
        }
        BoxChildren::Vertical(_) => {
            out.absent_visible.push(box_id);
        }
    }
}

fn geometry_from_window(pass: &Pass<'_>, window: &FlowWindow) -> VisibleGeom {
    let mut out = VisibleGeom {
        total_height: window.total_height,
        spans: spans_from_window(window),
        texts: Vec::new(),
        decorations: collect_decorations_window(pass, window),
        cells: Vec::new(),
        absent_visible: Vec::new(),
        content_atoms_painted: 0,
    };
    for e in &window.entries {
        let FlowItemKind::Content { box_id } = e.kind else {
            continue;
        };
        paint_content(pass, box_id, e.top, &mut out);
    }
    out
}

fn spans_from_window(window: &FlowWindow) -> BTreeMap<LayoutBoxId, AtomSpan> {
    let mut out = BTreeMap::new();
    for e in &window.entries {
        if let FlowItemKind::Content { box_id } = e.kind {
            out.insert(box_id, AtomSpan { top: e.top });
        }
    }
    for (id, top) in &window.extra_spans {
        out.insert(*id, AtomSpan { top: *top });
    }
    out
}
