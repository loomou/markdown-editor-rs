use super::chrome::collect_chrome;
use super::request::Pass;
use crate::blocks::{DecorationScope, for_kind};
use crate::boxtree::{block_id_of, inline_offset};
use crate::snapshot::{DecorationPiece, DeviceRect};
use md_core::Px;
use md_core::block::BlockKind;
use md_layout::spine::{FlowItemKind, FlowWindow};

pub(super) fn collect_decorations_window(
    pass: &Pass<'_>,
    window: &FlowWindow,
) -> Vec<DecorationPiece> {
    let mut out = Vec::new();
    for span in &window.container_spans {
        collect_chrome(pass, *span, &mut out);
    }
    for e in &window.entries {
        let FlowItemKind::Content { box_id } = e.kind else {
            continue;
        };
        let tree = &pass.assembly.tree;
        let kind = tree.get(box_id).shape_kind();
        if !wants_leaf_decoration(kind) {
            continue;
        }
        let x = inline_offset(tree, box_id);
        let w = tree.avail_width(box_id, pass.env.viewport_width);
        let Some(hit_block) = block_id_of(box_id) else {
            continue;
        };
        if let Some((rect_device, clip_device)) =
            leaf_decoration_geometry(pass, (x, e.top, w, e.height))
        {
            out.push(DecorationPiece {
                rect_device,
                clip_device: Some(clip_device),
                kind,
                role: tree.get(box_id).id().role,
                hit_block,
                gutter_dot: None,
                gutter_label: None,
                gutter_label_at: None,
                gutter_label_size: 0.0,
                list_nest: 0,
                task: None,
                alert: None,
            });
        }
    }
    out
}

fn leaf_decoration_geometry(
    pass: &Pass<'_>,
    rect: (Px, Px, Px, Px),
) -> Option<(DeviceRect, DeviceRect)> {
    let (x, y, w, h) = rect;
    let (top, bottom) = pass.view();
    let y2 = y + h;
    if y2 <= top || y >= bottom {
        return None;
    }
    let (full_t, full_b) = pass.snap.snap_range(y - pass.scroll, y2 - pass.scroll);
    let vis_t = y.max(top);
    let vis_b = y2.min(bottom);
    let (clip_t, clip_b) = pass
        .snap
        .snap_range(vis_t - pass.scroll, vis_b - pass.scroll);
    Some((
        (x, full_t, w, (full_b - full_t).max(0.0)),
        (x, clip_t, w, (clip_b - clip_t).max(0.0)),
    ))
}

pub(super) fn clip_decoration(pass: &Pass<'_>, rect: (Px, Px, Px, Px)) -> Option<(Px, Px, Px, Px)> {
    let (x, y, w, h) = rect;
    let (top, bottom) = pass.view();
    let y2 = y + h;
    if y2 <= top || y >= bottom {
        return None;
    }
    let vis_t = y.max(top);
    let vis_b = y2.min(bottom);
    let (t, b) = pass
        .snap
        .snap_range(vis_t - pass.scroll, vis_b - pass.scroll);
    Some((x, t, w, (b - t).max(0.0)))
}

fn wants_leaf_decoration(kind: BlockKind) -> bool {
    for_kind(kind).decoration_scope() == DecorationScope::Leaf
}
