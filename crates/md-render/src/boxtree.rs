use crate::snapshot::AtomSpan;
use md_core::Px;
use md_core::block::BlockId;
use md_layout::box_tree::{BoxChildren, BoxOwner, BoxRole, BoxTree, LayoutBoxId};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

pub fn inline_offset(tree: &BoxTree, id: LayoutBoxId) -> Px {
    let mut x = 0.0;
    let chain = tree.ancestor_chain(id);
    for anc in &chain[..chain.len().saturating_sub(1)] {
        let s = tree.style(*anc);
        x += s.border.left + s.padding.left;
    }
    x
}

pub fn block_id_of(id: LayoutBoxId) -> Option<BlockId> {
    match id.owner {
        BoxOwner::Block(b) => Some(b),
        BoxOwner::DocStart => None,
    }
}

pub(crate) fn text_box_id(tree: &BoxTree, block: BlockId) -> Option<LayoutBoxId> {
    let cell = LayoutBoxId {
        owner: BoxOwner::Block(block),
        role: BoxRole::Cell,
        local_key: 0,
    };
    if tree.nodes().contains_key(&cell) {
        return Some(cell);
    }
    let frame = LayoutBoxId::frame(block);
    tree.nodes().contains_key(&frame).then_some(frame)
}

fn child_index(tree: &BoxTree, parent: LayoutBoxId, child: LayoutBoxId) -> Option<usize> {
    match tree.get(parent).children() {
        BoxChildren::Vertical(ids) | BoxChildren::Island(ids) => {
            ids.iter().position(|id| *id == child)
        }
        BoxChildren::None => None,
    }
}

pub(crate) fn cmp_box_order(tree: &BoxTree, a: LayoutBoxId, b: LayoutBoxId) -> Ordering {
    if a == b {
        return Ordering::Equal;
    }
    let ac = tree.ancestor_chain(a);
    let bc = tree.ancestor_chain(b);
    let mut i = 0;
    while i < ac.len() && i < bc.len() && ac[i] == bc[i] {
        i += 1;
    }
    if i == 0 {
        return a.cmp(&b);
    }
    if i == ac.len() {
        return Ordering::Less;
    }
    if i == bc.len() {
        return Ordering::Greater;
    }
    match (
        child_index(tree, ac[i - 1], ac[i]),
        child_index(tree, bc[i - 1], bc[i]),
    ) {
        (Some(ai), Some(bi)) => ai.cmp(&bi).then_with(|| a.cmp(&b)),
        _ => a.cmp(&b),
    }
}

pub(crate) fn for_each_visible_text_box(
    tree: &BoxTree,
    spans: &BTreeMap<LayoutBoxId, AtomSpan>,
    mut visit: impl FnMut(LayoutBoxId),
) {
    let mut seen = BTreeSet::new();
    for id in spans.keys() {
        let Some(node) = tree.nodes().get(id) else {
            continue;
        };
        match node.children() {
            BoxChildren::Island(cells) => {
                for cell in cells {
                    if seen.insert(*cell) {
                        visit(*cell);
                    }
                }
            }
            BoxChildren::None
                if node.kind().is_text_leaf() && id.is_caret_role() && seen.insert(*id) =>
            {
                visit(*id);
            }
            _ => {}
        }
    }
}
