use super::gap::{FlowBoundary, gap_height};
use crate::box_tree::{BoxChildren, BoxTree, LayoutBoxId};
use crate::flow::HeightState;
use md_core::Px;
use std::collections::HashMap;

pub(super) fn box_on_path(tree: &BoxTree, node: LayoutBoxId, target: LayoutBoxId) -> bool {
    let mut cur = Some(target);
    while let Some(id) = cur {
        if id == node {
            return true;
        }
        cur = tree
            .nodes
            .get(&id)
            .and_then(|n| n.parent)
            .or_else(|| tree.deferred(id).and_then(|d| d.parent));
    }
    false
}

pub(super) fn subtree_height(
    tree: &BoxTree,
    id: LayoutBoxId,
    avail: Px,
    heights: &dyn Fn(LayoutBoxId, Px) -> HeightState,
) -> Px {
    let mut cache = HashMap::new();
    subtree_height_cached(tree, id, avail, heights, &mut cache)
}

pub(super) fn subtree_height_cached(
    tree: &BoxTree,
    id: LayoutBoxId,
    avail: Px,
    heights: &dyn Fn(LayoutBoxId, Px) -> HeightState,
    cache: &mut HashMap<LayoutBoxId, Px>,
) -> Px {
    if let Some(height) = cache.get(&id) {
        return *height;
    }
    if let Some(h) = tree.deferred_height(id) {
        cache.insert(id, h);
        return h;
    }
    let node = tree.get(id);
    let height = match &node.children {
        BoxChildren::Vertical(children) => {
            let style = tree.style_of(node);
            let top = style.top_border_padding();
            let bottom = style.bottom_border_padding();
            let child_avail = (avail - style.inline_border_padding()).max(0.0);
            let mut h = top + bottom;
            if children.is_empty() {
                h += gap_height(tree, id, FlowBoundary::Start, FlowBoundary::End);
            } else {
                for (i, child) in children.iter().enumerate() {
                    let before = if i == 0 {
                        FlowBoundary::Start
                    } else {
                        FlowBoundary::Child(children[i - 1])
                    };
                    h += gap_height(tree, id, before, FlowBoundary::Child(*child));
                    h += subtree_height_cached(tree, *child, child_avail, heights, cache);
                }
                let last = *children.last().unwrap();
                h += gap_height(tree, id, FlowBoundary::Child(last), FlowBoundary::End);
            }
            h
        }
        BoxChildren::Island(_) | BoxChildren::None => heights(id, avail).px(),
    };
    cache.insert(id, height);
    height
}
