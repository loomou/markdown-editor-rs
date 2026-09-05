use super::decoration::clip_decoration;
use super::probe::{
    first_line_height, first_line_top, first_text_box, list_nest_depth, ordered_label,
};
use super::request::Pass;
use crate::boxtree::{block_id_of, inline_offset};
use crate::snapshot::DecorationPiece;
use md_core::Px;
use md_core::block::BlockKind;
use md_layout::box_tree::{BoxRole, LayoutBoxId};
use md_layout::spine::ContainerSpan;

fn pin_mark(mark_top: Px, top: Px, bottom: Px, scroll: Px, marker_h: Px) -> Px {
    let hi = (bottom - marker_h).max(top);
    mark_top.clamp(scroll.min(hi), hi)
}

pub(super) fn collect_chrome(pass: &Pass<'_>, span: ContainerSpan, out: &mut Vec<DecorationPiece>) {
    let ContainerSpan {
        box_id,
        top,
        bottom,
    } = span;
    let (assembly, snap, scroll, theme) = (pass.assembly, pass.snap, pass.scroll, pass.theme);
    let node = assembly.tree.get(box_id);
    if node.id().role != BoxRole::Frame {
        return;
    }
    let Some(owner) = block_id_of(box_id) else {
        return;
    };
    let Some(chrome_id) = LayoutBoxId::chrome(node.kind(), owner) else {
        return;
    };
    let Some(chrome) = assembly.tree.nodes().get(&chrome_id) else {
        return;
    };
    let x = inline_offset(&assembly.tree, box_id);
    let hit_block = first_text_box(&assembly.tree, box_id)
        .and_then(block_id_of)
        .unwrap_or(owner);
    match chrome.id().role {
        BoxRole::Bar => {
            let bar_w = assembly.tree.style_of(chrome).border.left.max(1.0);
            if let Some(rect_device) =
                clip_decoration(pass, (x, top, bar_w, (bottom - top).max(0.0)))
            {
                out.push(DecorationPiece {
                    rect_device,
                    clip_device: None,
                    kind: node.kind(),
                    role: BoxRole::Bar,
                    hit_block,
                    gutter_dot: node.extra().quote_alert().map(|_| {
                        (
                            x + theme.decoration.quote_alert_label_dx,
                            snap.snap(top - scroll),
                        )
                    }),
                    gutter_label: node.extra().quote_alert().map(|a| a.label().to_string()),
                    gutter_label_at: None,
                    gutter_label_size: theme.type_scale.body.size_px,
                    list_nest: 0,
                    task: None,
                    alert: node.extra().quote_alert(),
                });
            }
        }
        BoxRole::Slot => {
            if node.kind() != BlockKind::ListItem {
                return;
            }
            let task = node.extra().task_checked();
            let ordered = ordered_label(assembly, box_id);
            let line_top = first_line_top(assembly, box_id, top);
            let line_h = first_line_height(assembly, box_id, theme);
            let d = &theme.decoration;
            let gap = d.list_marker_gap;
            let (dot_x, size, mark_top, label_at) = if task.is_some() {
                let size = d.task_size;
                let gap = d.task_gap;
                let box_x = x - size - gap;
                let mt = line_top + ((line_h - size) * 0.5).max(0.0);
                let label_at = ordered.as_ref().map(|_| (box_x - gap, line_top));
                (box_x, size, mt, label_at)
            } else if ordered.is_some() {
                (x - gap, line_h, line_top, Some((x - gap, line_top)))
            } else {
                let size = d.list_marker_size;
                (
                    x - gap - size,
                    size,
                    line_top + ((line_h - size) * 0.5).max(0.0),
                    None,
                )
            };
            let marker_h = if ordered.is_some() && task.is_none() {
                line_h
            } else {
                size
            };
            let doc_y = pin_mark(mark_top, top, bottom, scroll, marker_h);
            let gy = snap.snap(doc_y - scroll);
            let gutter_label_at = label_at.map(|(lx, ly)| {
                let pinned = pin_mark(ly, top, bottom, scroll, line_h);
                (lx, snap.snap(pinned - scroll))
            });
            let gutter = node
                .parent()
                .map(|p| assembly.tree.style(p).padding.left)
                .unwrap_or(d.list_gutter_min);
            let hit_x = x - gutter;
            let label_size = first_text_box(&assembly.tree, box_id)
                .map(|leaf| {
                    let node = assembly.tree.get(leaf);
                    theme
                        .type_role_for(node.shape_kind(), node.type_slot())
                        .size_px
                })
                .unwrap_or(theme.type_scale.body.size_px);
            if let Some(rect_device) =
                clip_decoration(pass, (hit_x, top, gutter.max(1.0), (bottom - top).max(0.0)))
            {
                out.push(DecorationPiece {
                    rect_device,
                    clip_device: None,
                    kind: node.kind(),
                    role: BoxRole::Slot,
                    hit_block,
                    gutter_dot: Some((dot_x, gy)),
                    gutter_label: ordered,
                    gutter_label_at,
                    gutter_label_size: label_size,
                    list_nest: list_nest_depth(&assembly.tree, box_id),
                    task,
                    alert: None,
                });
            }
        }
        _ => {}
    }
}
