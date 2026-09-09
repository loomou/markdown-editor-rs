use super::block_edit::{box_snapshot, emit_preview, preview_payload};
use super::flow_policy::{apply_list_metrics, flow_top_margin, type_slot_for};
use super::restyle;
use super::theme::LayoutTheme;
use super::window::subtree_height;
use super::{compose, emit};
use crate::box_tree::{BoxChildren, BoxOwner, BoxRole, BoxTree, DeferredBox, LayoutBoxId};
use md_core::Px;
use md_core::block::BlockKind;
use md_core::document::Document;
use md_core::document::NodeId;
use md_core::document::{ChangeSet, DocChange};
use std::collections::HashMap;

pub fn patch_text(tree: &mut BoxTree, doc: &Document, theme: &LayoutTheme, id: NodeId) {
    let kind = doc
        .arena
        .get(id)
        .map(|n| n.kind)
        .unwrap_or(BlockKind::Paragraph);
    let box_id = LayoutBoxId::for_kind(kind, id.index);
    if tree.nodes.contains_key(&box_id) {
        let edit_source = tree.nodes.get(&box_id).is_some_and(|n| n.edit_source);
        tree.set_leaf_text(box_id, box_snapshot(doc, id, edit_source));
        let preview = LayoutBoxId::preview(id.index);
        if tree.nodes.contains_key(&preview) {
            let (preview_kind, snapshot) = preview_payload(doc, id, kind);
            tree.set_leaf_text(preview, snapshot);
            if let Some(n) = tree.nodes.get_mut(&preview) {
                n.kind = preview_kind;
            }
        }
        let extra = doc.extra(id);
        let content_revision = doc.arena.get(id).map(|node| node.content_revision);
        for target in [box_id, preview] {
            if let Some(n) = tree.nodes.get_mut(&target) {
                n.extra = extra;
                n.content_revision = content_revision.unwrap_or(n.content_revision);
                n.content_generation = id.generation.get();
            }
        }
    } else {
        refresh_deferred_estimate(tree, doc, theme, id);
    }
}

pub fn deferred_ancestor_of(
    tree: &BoxTree,
    doc: &Document,
    id: NodeId,
) -> Option<(LayoutBoxId, NodeId)> {
    let mut cur = id;
    loop {
        let node = doc.arena.get(cur)?;
        let bid = LayoutBoxId::for_kind(node.kind, cur.index);
        if tree.deferred.contains_key(&bid) {
            return Some((bid, cur));
        }
        cur = node.parent?;
    }
}

fn refresh_deferred_estimate(
    tree: &mut BoxTree,
    doc: &Document,
    theme: &LayoutTheme,
    id: NodeId,
) -> bool {
    let Some((deferred_box, root)) = deferred_ancestor_of(tree, doc, id) else {
        return false;
    };
    let Some(ref lazy) = tree.lazy else {
        return false;
    };
    let Some(parent) = tree.deferred(deferred_box).and_then(|d| d.parent) else {
        return false;
    };
    let avail = tree.content_width(parent, lazy.viewport.get());
    let h = subtree_height(doc, theme, &lazy.metrics, root, avail, &mut HashMap::new());
    tree.deferred
        .get_mut(&deferred_box)
        .expect("deferred_ancestor_of confirmed the entry")
        .height = h;
    true
}

pub fn sync_layout(
    tree: &mut BoxTree,
    doc: &Document,
    changes: &ChangeSet,
    theme: &LayoutTheme,
) -> bool {
    if changes.is_empty() {
        return false;
    }
    if changes.is_replace() {
        *tree = compose(doc, theme);
        return true;
    }
    if !changes.is_structural() {
        for c in &changes.changes {
            match c {
                DocChange::TextChanged { node, .. } => patch_text(tree, doc, theme, *node),
                DocChange::AttrsChanged { node, .. } => patch_attrs(tree, doc, *node, theme),
                _ => {}
            }
        }
        return false;
    }
    for c in &changes.changes {
        match c {
            DocChange::TextChanged { node, .. } => patch_text(tree, doc, theme, *node),
            DocChange::AttrsChanged { node, .. } => patch_attrs(tree, doc, *node, theme),
            DocChange::TreeSpliced {
                parent,
                removed,
                inserted,
                ..
            } => splice_nodes(tree, doc, *parent, removed, inserted, theme),
            DocChange::DocumentReplaced => {}
            DocChange::ReferenceDefsChanged { .. } | DocChange::TableAlignOverflow { .. } => {}
        }
    }
    false
}

fn patch_attrs(tree: &mut BoxTree, doc: &Document, id: NodeId, theme: &LayoutTheme) {
    let Some(node) = doc.arena.get(id) else {
        return;
    };
    let extra = node.extra;
    let kind = node.kind;
    let box_id = LayoutBoxId::for_kind(kind, id.index);
    if !tree.nodes.contains_key(&box_id) {
        refresh_deferred_estimate(tree, doc, theme, id);
        return;
    }
    let kind_changed = tree.nodes.get(&box_id).is_some_and(|n| n.kind != kind);
    if let Some(n) = tree.nodes.get_mut(&box_id) {
        n.extra = extra;
        n.content_revision = node.content_revision;
        n.content_generation = id.generation.get();
        if n.kind != kind {
            n.kind = kind;
        }
        n.type_slot = type_slot_for(theme, doc, id, kind, extra);
    }
    if tree.nodes.contains_key(&box_id) {
        restyle(&mut tree.nodes, &mut tree.styles, box_id, |style| {
            if kind_changed {
                *style = theme.style_for(kind);
            }
            style.margin.top = flow_top_margin(theme, doc, id, kind);
        });
    }
    if let Some(chrome_id) = LayoutBoxId::chrome(kind, id.index)
        && let Some(c) = tree.nodes.get_mut(&chrome_id)
    {
        c.extra = extra;
        c.content_revision = node.content_revision;
        c.content_generation = id.generation.get();
    }
    let list = if kind == BlockKind::List {
        Some(id)
    } else if kind == BlockKind::ListItem {
        node.parent
            .filter(|&p| doc.arena.get(p).is_some_and(|n| n.kind == BlockKind::List))
    } else {
        None
    };
    if let Some(list) = list {
        let list_box = LayoutBoxId::for_kind(BlockKind::List, list.index);
        if tree.nodes.contains_key(&list_box) {
            apply_list_metrics(
                doc,
                list,
                list_box,
                theme,
                &mut tree.nodes,
                &mut tree.styles,
            );
        }
    }
    if kind == BlockKind::ListItem {
        retarget_descendant_slots(tree, doc, id, theme);
    }
}

fn retarget_descendant_slots(tree: &mut BoxTree, doc: &Document, id: NodeId, theme: &LayoutTheme) {
    let mut stack = vec![id];
    while let Some(c) = stack.pop() {
        let kind = doc
            .arena
            .get(c)
            .map(|n| n.kind)
            .unwrap_or(BlockKind::Paragraph);
        let extra = doc.extra(c);
        let bid = LayoutBoxId::for_kind(kind, c.index);
        if let Some(n) = tree.nodes.get_mut(&bid) {
            n.type_slot = type_slot_for(theme, doc, c, kind, extra);
        }
        let kids: Vec<NodeId> = doc.arena.children(c).collect();
        for k in kids.into_iter().rev() {
            stack.push(k);
        }
    }
}

fn retarget_lead_top_margins(
    tree: &mut BoxTree,
    doc: &Document,
    parent: NodeId,
    parent_kind: BlockKind,
    theme: &LayoutTheme,
) {
    if !matches!(
        parent_kind,
        BlockKind::ListItem
            | BlockKind::BlockQuote
            | BlockKind::DocRoot
            | BlockKind::FootnoteDefinition
    ) {
        return;
    }
    let kids: Vec<NodeId> = doc.arena.children(parent).collect();
    for c in kids {
        let kind = doc
            .arena
            .get(c)
            .map(|n| n.kind)
            .unwrap_or(BlockKind::Paragraph);
        let extra = doc.extra(c);
        let bid = LayoutBoxId::for_kind(kind, c.index);
        if kind == BlockKind::List && parent_kind == BlockKind::ListItem {
            apply_list_metrics(doc, c, bid, theme, &mut tree.nodes, &mut tree.styles);
            continue;
        }
        if tree.nodes.contains_key(&bid) {
            restyle(&mut tree.nodes, &mut tree.styles, bid, |style| {
                style.margin.top = flow_top_margin(theme, doc, c, kind);
            });
            if let Some(n) = tree.nodes.get_mut(&bid) {
                n.type_slot = type_slot_for(theme, doc, c, kind, extra);
            }
        } else if let Some(d) = tree.deferred.get_mut(&bid) {
            d.margin_top = flow_top_margin(theme, doc, c, kind);
        }
    }
}

fn existing_box(tree: &BoxTree, index: u32) -> Option<LayoutBoxId> {
    let frame = LayoutBoxId::frame(index);
    if tree.nodes.contains_key(&frame) {
        return Some(frame);
    }
    let cell = LayoutBoxId {
        owner: BoxOwner::Block(index),
        role: BoxRole::Cell,
        local_key: 0,
    };
    if tree.nodes.contains_key(&cell) {
        Some(cell)
    } else {
        None
    }
}

pub fn defer_composed(tree: &mut BoxTree, id: LayoutBoxId, height: Px) {
    if !tree.nodes.contains_key(&id) {
        return;
    }
    let parent = tree.nodes.get(&id).and_then(|n| n.parent);
    let (margin_top, margin_bottom) = tree.flow_margins(id);
    drop_subtree(tree, id);
    tree.deferred.insert(
        id,
        DeferredBox {
            parent,
            height,
            margin_top,
            margin_bottom,
        },
    );
}

pub(super) fn drop_subtree(tree: &mut BoxTree, id: LayoutBoxId) {
    let mut stack = vec![id];
    while let Some(id) = stack.pop() {
        tree.deferred.remove(&id);
        let (kind, owner) = match tree.nodes.get(&id) {
            Some(n) => (n.kind, n.id.owner),
            None => continue,
        };
        let children = match tree.nodes.get(&id).map(|n| &n.children) {
            Some(BoxChildren::Vertical(c) | BoxChildren::Island(c)) => c.clone(),
            _ => Vec::new(),
        };
        for c in children.into_iter().rev() {
            stack.push(c);
        }
        let text_id = tree.nodes.get(&id).and_then(|node| node.text_id);
        tree.nodes.remove(&id);
        tree.intern.release(text_id);
        if id.role == BoxRole::Frame
            && let BoxOwner::Block(index) = owner
        {
            let preview = LayoutBoxId::preview(index);
            let preview_text = tree.nodes.get(&preview).and_then(|n| n.text_id);
            tree.nodes.remove(&preview);
            tree.intern.release(preview_text);
            if let Some(chrome) = LayoutBoxId::chrome(kind, index) {
                tree.nodes.remove(&chrome);
            }
        }
    }
}

fn arena_reachable(doc: &Document, id: NodeId) -> bool {
    let mut cur = id;
    loop {
        let Some(node) = doc.arena.get(cur) else {
            return false;
        };
        if cur == doc.root {
            return true;
        }
        match node.parent {
            Some(p) => cur = p,
            None => return false,
        }
    }
}

fn drop_subtree_doc(tree: &mut BoxTree, doc: &Document, id: LayoutBoxId) {
    let children = match tree.nodes.get(&id).map(|n| &n.children) {
        Some(BoxChildren::Vertical(c) | BoxChildren::Island(c)) => c.clone(),
        _ => Vec::new(),
    };
    for c in children {
        let reachable = match c.owner {
            BoxOwner::Block(index) => doc
                .live_id(index)
                .is_some_and(|live| arena_reachable(doc, live)),
            BoxOwner::DocStart => false,
        };
        if !reachable {
            drop_subtree_doc(tree, doc, c);
        }
    }
    if let Some(n) = tree.nodes.get_mut(&id) {
        n.children = BoxChildren::None;
    }
    drop_subtree(tree, id);
}

fn splice_nodes(
    tree: &mut BoxTree,
    doc: &Document,
    parent: NodeId,
    removed: &[NodeId],
    inserted: &[NodeId],
    theme: &LayoutTheme,
) {
    for r in removed {
        let rid = LayoutBoxId::frame(r.index);
        tree.deferred.remove(&rid);
        let Some(id) = existing_box(tree, r.index) else {
            continue;
        };
        let reattached = doc.arena.get(*r).is_some_and(|_| arena_reachable(doc, *r));
        if reattached {
            continue;
        }
        drop_subtree_doc(tree, doc, id);
    }
    let Some(parent_kind) = doc.arena.get(parent).map(|n| n.kind) else {
        return;
    };
    let Some(parent_box) = existing_box(tree, parent.index) else {
        refresh_deferred_estimate(tree, doc, theme, parent);
        return;
    };
    for n in inserted {
        if doc.arena.get(*n).is_none() {
            continue;
        }
        let cid = emit(
            doc,
            *n,
            Some(parent_box),
            theme,
            &mut tree.nodes,
            &mut tree.styles,
            &mut tree.intern,
        );
        emit_preview(
            doc,
            *n,
            cid,
            Some(parent_box),
            theme,
            &mut tree.nodes,
            &mut tree.styles,
            &mut tree.intern,
        );
    }
    let mut child_ids: Vec<LayoutBoxId> = Vec::new();
    for c in doc.arena.children(parent) {
        let kind = doc
            .arena
            .get(c)
            .map(|n| n.kind)
            .unwrap_or(BlockKind::Paragraph);
        let bid = LayoutBoxId::for_kind(kind, c.index);
        if let Some(node) = tree.nodes.get_mut(&bid) {
            node.parent = Some(parent_box);
            child_ids.push(bid);
        } else if tree.deferred.contains_key(&bid) {
            child_ids.push(bid);
        }
        let preview = LayoutBoxId::preview(c.index);
        if let Some(node) = tree.nodes.get_mut(&preview) {
            node.parent = Some(parent_box);
            child_ids.push(preview);
        }
    }
    if let Some(p) = tree.nodes.get_mut(&parent_box) {
        p.children = if p.kind == BlockKind::TableRow {
            BoxChildren::Island(child_ids)
        } else if p.kind.is_vertical_container() {
            BoxChildren::Vertical(child_ids)
        } else if child_ids.is_empty() {
            BoxChildren::None
        } else {
            panic!(
                "parent kind {:?} cannot take {} spliced children",
                p.kind,
                child_ids.len()
            );
        };
    }
    retarget_lead_top_margins(tree, doc, parent, parent_kind, theme);
    if parent_kind == BlockKind::List {
        apply_list_metrics(
            doc,
            parent,
            parent_box,
            theme,
            &mut tree.nodes,
            &mut tree.styles,
        );
    }
}
