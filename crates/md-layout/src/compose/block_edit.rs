use super::flow_policy::type_slot_for;
use super::restyle;
use super::theme::LayoutTheme;
use crate::box_tree::{
    BoxChildren, BoxIntern, BoxNode, BoxStore, BoxStyleStore, BoxTree, LayoutBoxId, owned_snapshot,
};
use md_core::block::BlockKind;
use md_core::document::NodeId;
use md_core::document::{Document, LeafSnapshot};
use std::sync::Arc;

pub(super) fn box_snapshot(
    doc: &Document,
    id: NodeId,
    edit_source: bool,
) -> Option<Arc<LeafSnapshot>> {
    if edit_source {
        if doc
            .arena
            .get(id)
            .is_some_and(|n| n.kind.supports_block_edit())
        {
            owned_snapshot(doc.block_source(id).to_string(), Vec::new())
        } else {
            owned_snapshot(doc.display(id).to_string(), Vec::new())
        }
    } else {
        doc.layout_leaf_snapshot(id)
    }
}

pub(super) fn wants_preview(doc: &Document, id: NodeId) -> bool {
    if doc.block_edit() != Some(id.index) {
        return false;
    }
    doc.arena
        .get(id)
        .is_some_and(|n| n.kind.supports_block_edit())
        || display_math_preview(doc, id)
}

fn display_math_preview(doc: &Document, id: NodeId) -> bool {
    doc.revealed_math()
        .is_some_and(|m| m.display_math && m.block == id.index)
}

pub(super) fn preview_payload(
    doc: &Document,
    id: NodeId,
    frame_kind: BlockKind,
) -> (BlockKind, Option<Arc<LeafSnapshot>>) {
    if let Some(m) = doc
        .revealed_math()
        .filter(|m| m.display_math && m.block == id.index)
    {
        return (
            BlockKind::Math,
            owned_snapshot(m.latex.to_string(), Vec::new()),
        );
    }
    (frame_kind, box_snapshot(doc, id, false))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_preview(
    doc: &Document,
    id: NodeId,
    frame: LayoutBoxId,
    parent: Option<LayoutBoxId>,
    theme: &LayoutTheme,
    nodes: &mut BoxStore,
    styles: &mut BoxStyleStore,
    intern: &mut BoxIntern,
) -> Option<LayoutBoxId> {
    if !wants_preview(doc, id) {
        return None;
    }
    let (kind, extra, content_revision, content_generation) = {
        let f = nodes.get(&frame)?;
        (f.kind, f.extra, f.content_revision, f.content_generation)
    };
    let (preview_kind, snapshot) = preview_payload(doc, id, kind);
    let code = theme.style_for(BlockKind::CodeBlock);
    if nodes.contains_key(&frame) {
        restyle(nodes, styles, frame, |style| {
            style.margin.bottom = 0.0;

            style.padding = code.padding;
        });
        if let Some(f) = nodes.get_mut(&frame) {
            f.edit_source = true;
        }
    }
    let preview_id = LayoutBoxId::preview(id.index);
    let mut style = theme.style_for(preview_kind);
    style.margin.top = code.margin.top;
    nodes.insert(
        preview_id,
        BoxNode {
            id: preview_id,
            kind: preview_kind,
            style_id: styles.intern(style),
            parent,
            children: BoxChildren::None,
            text_id: intern.push(snapshot),
            extra,
            content_revision,
            content_generation,

            edit_source: false,
            type_slot: type_slot_for(theme, doc, id, preview_kind, extra),
        },
    );
    Some(preview_id)
}

#[derive(Clone, Debug)]
pub struct PreviewSplice {
    pub parent: LayoutBoxId,
    pub before: Option<LayoutBoxId>,
    pub removed: Vec<LayoutBoxId>,
    pub inserted: Vec<LayoutBoxId>,
}

pub fn sync_block_edit(
    tree: &mut BoxTree,
    doc: &Document,
    theme: &LayoutTheme,
    prev: Option<md_core::block::BlockId>,
    next: Option<md_core::block::BlockId>,
) -> Vec<PreviewSplice> {
    if prev == next {
        return Vec::new();
    }
    let mut out = Vec::new();
    if let Some(b) = prev
        && let Some(s) = detach_preview(tree, doc, theme, b)
    {
        out.push(s);
    }
    if let Some(b) = next
        && let Some(s) = attach_preview(tree, doc, theme, b)
    {
        out.push(s);
    }
    out
}

fn detach_preview(
    tree: &mut BoxTree,
    doc: &Document,
    theme: &LayoutTheme,
    block: md_core::block::BlockId,
) -> Option<PreviewSplice> {
    let preview = LayoutBoxId::preview(block);
    let parent = tree.nodes.get(&preview)?.parent?;
    let preview_text = tree.nodes.get(&preview).and_then(|n| n.text_id);
    tree.nodes.remove(&preview);
    tree.intern.release(preview_text);
    let frame = LayoutBoxId::frame(block);
    if tree.nodes.contains_key(&frame) {
        let kind = tree.nodes.get(&frame).expect("live frame").kind;

        let own = theme.style_for(kind);
        restyle(&mut tree.nodes, &mut tree.styles, frame, |style| {
            style.margin.bottom = own.margin.bottom;
            style.padding = own.padding;
        });
        if let Some(f) = tree.nodes.get_mut(&frame) {
            f.edit_source = false;
        }
    }

    if let Some(id) = doc.live_id(block) {
        tree.set_leaf_text(frame, box_snapshot(doc, id, false));
    }
    remove_child(tree, parent, preview);
    Some(PreviewSplice {
        parent,
        before: Some(frame),
        removed: vec![preview],
        inserted: Vec::new(),
    })
}

fn attach_preview(
    tree: &mut BoxTree,
    doc: &Document,
    theme: &LayoutTheme,
    block: md_core::block::BlockId,
) -> Option<PreviewSplice> {
    let id = doc.live_id(block)?;
    let frame = LayoutBoxId::frame(block);
    let parent = tree.nodes.get(&frame)?.parent?;

    if !child_list_contains(tree, parent, frame) {
        return None;
    }
    let preview = emit_preview(
        doc,
        id,
        frame,
        Some(parent),
        theme,
        &mut tree.nodes,
        &mut tree.styles,
        &mut tree.intern,
    )?;

    tree.set_leaf_text(frame, box_snapshot(doc, id, true));
    debug_assert!(
        insert_child_after(tree, parent, frame, preview),
        "frame was in the child list a moment ago"
    );
    Some(PreviewSplice {
        parent,
        before: Some(frame),
        removed: Vec::new(),
        inserted: vec![preview],
    })
}

fn child_list_contains(tree: &BoxTree, parent: LayoutBoxId, child: LayoutBoxId) -> bool {
    match tree.nodes.get(&parent).map(|n| &n.children) {
        Some(BoxChildren::Vertical(c) | BoxChildren::Island(c)) => c.contains(&child),
        _ => false,
    }
}

fn child_list_mut(tree: &mut BoxTree, parent: LayoutBoxId) -> Option<&mut Vec<LayoutBoxId>> {
    match &mut tree.nodes.get_mut(&parent)?.children {
        BoxChildren::Vertical(c) | BoxChildren::Island(c) => Some(c),
        BoxChildren::None => None,
    }
}

fn remove_child(tree: &mut BoxTree, parent: LayoutBoxId, child: LayoutBoxId) {
    if let Some(kids) = child_list_mut(tree, parent) {
        kids.retain(|c| *c != child);
    }
}

fn insert_child_after(
    tree: &mut BoxTree,
    parent: LayoutBoxId,
    after: LayoutBoxId,
    child: LayoutBoxId,
) -> bool {
    if let Some(kids) = child_list_mut(tree, parent)
        && let Some(i) = kids.iter().position(|c| *c == after)
    {
        kids.insert(i + 1, child);
        true
    } else {
        false
    }
}
