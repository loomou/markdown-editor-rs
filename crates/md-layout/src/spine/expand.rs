use super::FlowSpine;
use super::gap::FlowBoundary;
use super::item::{FlowItem, FlowItemKind};
use super::tree_walk::box_on_path;
use crate::box_tree::{BoxChildren, BoxTree, LayoutBoxId};
use crate::flow::HeightState;
use md_core::Px;
use std::collections::HashMap;

impl FlowSpine {
    #[must_use]
    pub fn expand_visible(
        &mut self,
        tree: &BoxTree,
        top: Px,
        bottom: Px,
        heights: &dyn Fn(LayoutBoxId, Px) -> HeightState,
    ) -> u32 {
        let mut rounds = 0u32;
        loop {
            let mut range = self.visible(top, bottom);
            let end = range.end;

            let first = range.find(|&pos| {
                matches!(self.items[pos].kind, FlowItemKind::Collapsed { box_id }
                    if tree.nodes.get(&box_id).is_some())
            });
            let Some(first) = first else {
                return rounds;
            };
            rounds += 1;
            if rounds > 64 {
                return rounds;
            }
            let y0 = self.fenwick.prefix(first);
            self.tally(first..end, false);
            let collapsed: Vec<LayoutBoxId> = self.items[first..end]
                .iter()
                .filter_map(|item| match item.kind {
                    FlowItemKind::Collapsed { box_id } if tree.nodes.get(&box_id).is_some() => {
                        Some(box_id)
                    }
                    _ => None,
                })
                .collect();
            for box_id in &collapsed {
                if let Some(fid) = self.collapsed_of.remove(box_id) {
                    self.release_id(fid);
                }
            }

            let mut items = std::mem::take(&mut self.items);
            let mut out: Vec<FlowItem> = Vec::with_capacity(end - first + 32);
            let mut y = y0;
            let mut subtree_heights = HashMap::new();
            for item in items.drain(first..end) {
                if let FlowItemKind::Collapsed { box_id } = item.kind {
                    if tree.nodes.get(&box_id).is_none() {
                        y += item.height.px();
                        out.push(item);
                        continue;
                    }
                    let avail = tree.avail_width(box_id, self.viewport_width);
                    self.emit_intersecting(
                        tree,
                        box_id,
                        avail,
                        &mut y,
                        top,
                        bottom,
                        heights,
                        &mut subtree_heights,
                        &mut out,
                    );
                } else {
                    y += item.height.px();
                    out.push(item);
                }
            }
            let inserted = out.len();
            items.splice(first..first, out);
            self.items = items;
            self.remap_from(first);
            self.tally(first..first + inserted, true);
            self.splice_height_index(first, end - first, inserted);
        }
    }

    #[must_use]
    pub fn expand_to(
        &mut self,
        tree: &BoxTree,
        target: LayoutBoxId,
        heights: &dyn Fn(LayoutBoxId, Px) -> HeightState,
    ) -> bool {
        let mut rounds = 0u32;
        loop {
            if self.content_of.contains_key(&target) {
                return true;
            }
            let Some(collapsed_id) = self.collapsed_ancestor(tree, target) else {
                return false;
            };
            let Some(fid) = self.collapsed_of.get(&collapsed_id).copied() else {
                return false;
            };
            let Some(pos) = self.location(fid) else {
                return false;
            };
            rounds += 1;
            if rounds > 64 {
                return self.content_of.contains_key(&target);
            }
            if let Some(id) = self.collapsed_of.remove(&collapsed_id) {
                self.release_id(id);
            }
            if tree.nodes.get(&collapsed_id).is_none() {
                return false;
            }
            let avail = tree.avail_width(collapsed_id, self.viewport_width);
            let mut out = Vec::new();
            let mut subtree_heights = HashMap::new();
            self.emit_path_to(
                tree,
                collapsed_id,
                avail,
                heights,
                target,
                &mut subtree_heights,
                &mut out,
            );
            self.splice_shift(pos, 1, out);
        }
    }

    pub(super) fn collapsed_ancestor(
        &self,
        tree: &BoxTree,
        target: LayoutBoxId,
    ) -> Option<LayoutBoxId> {
        let mut cur = Some(target);
        while let Some(id) = cur {
            if self.collapsed_of.contains_key(&id) {
                return Some(id);
            }
            cur = tree
                .nodes
                .get(&id)
                .and_then(|n| n.parent)
                .or_else(|| tree.deferred(id).and_then(|d| d.parent));
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_path_to(
        &mut self,
        tree: &BoxTree,
        id: LayoutBoxId,
        avail: Px,
        heights: &dyn Fn(LayoutBoxId, Px) -> HeightState,
        target: LayoutBoxId,
        subtree_heights: &mut HashMap<LayoutBoxId, Px>,
        out: &mut Vec<FlowItem>,
    ) {
        let node = tree.get(id);
        match &node.children {
            BoxChildren::Vertical(children) => {
                if !box_on_path(tree, id, target) {
                    let h = super::tree_walk::subtree_height_cached(
                        tree,
                        id,
                        avail,
                        heights,
                        subtree_heights,
                    );
                    out.push(FlowItem::with_epoch(
                        self.alloc_id(),
                        FlowItemKind::Collapsed { box_id: id },
                        HeightState::Estimated(h),
                        self.layout_epoch,
                    ));
                    return;
                }
                let style = tree.style_of(node);
                let pad_top = style.top_border_padding();
                let pad_bottom = style.bottom_border_padding();
                let child_avail = (avail - style.inline_border_padding()).max(0.0);
                out.push(FlowItem::with_epoch(
                    self.alloc_id(),
                    FlowItemKind::ContainerOpen { box_id: id },
                    HeightState::Exact(pad_top),
                    self.layout_epoch,
                ));
                if children.is_empty() {
                    out.push(self.gap_item(tree, id, FlowBoundary::Start, FlowBoundary::End));
                } else {
                    for (i, child) in children.iter().enumerate() {
                        let before = if i == 0 {
                            FlowBoundary::Start
                        } else {
                            FlowBoundary::Child(children[i - 1])
                        };
                        out.push(self.gap_item(tree, id, before, FlowBoundary::Child(*child)));
                        self.emit_path_to(
                            tree,
                            *child,
                            child_avail,
                            heights,
                            target,
                            subtree_heights,
                            out,
                        );
                    }
                    let last = *children
                        .last()
                        .expect("vertical container has at least one child");
                    out.push(self.gap_item(tree, id, FlowBoundary::Child(last), FlowBoundary::End));
                }
                out.push(FlowItem::with_epoch(
                    self.alloc_id(),
                    FlowItemKind::ContainerClose { box_id: id },
                    HeightState::Exact(pad_bottom),
                    self.layout_epoch,
                ));
            }
            BoxChildren::Island(_) | BoxChildren::None => {
                let hs = heights(id, avail);
                out.push(FlowItem::with_epoch(
                    self.alloc_id(),
                    FlowItemKind::Content { box_id: id },
                    hs,
                    self.layout_epoch,
                ));
            }
        }
    }

    fn splice_shift(&mut self, at: usize, delete: usize, insert: Vec<FlowItem>) {
        assert!(at + delete <= self.items.len());
        let inserted = insert.len();
        self.tally(at..at + delete, false);
        let removed: Vec<FlowItem> = self.items[at..at + delete].to_vec();
        for item in &removed {
            if !matches!(item.kind, FlowItemKind::Collapsed { .. }) {
                self.unmap(item);
            }
        }
        self.items.splice(at..at + delete, insert);
        self.remap_from(at);
        self.tally(at..at + inserted, true);
        self.splice_height_index(at, delete, inserted);
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_intersecting(
        &mut self,
        tree: &BoxTree,
        id: LayoutBoxId,
        avail: Px,
        y: &mut Px,
        top: Px,
        bottom: Px,
        heights: &dyn Fn(LayoutBoxId, Px) -> HeightState,
        subtree_heights: &mut HashMap<LayoutBoxId, Px>,
        out: &mut Vec<FlowItem>,
    ) {
        if tree.nodes.get(&id).is_none() {
            let h = tree
                .deferred_height(id)
                .unwrap_or_else(|| heights(id, avail).px());
            out.push(FlowItem::with_epoch(
                self.alloc_id(),
                FlowItemKind::Collapsed { box_id: id },
                HeightState::Estimated(h),
                self.layout_epoch,
            ));
            *y += h;
            return;
        }
        let node = tree.get(id);
        match &node.children {
            BoxChildren::Vertical(children) => {
                let h = super::tree_walk::subtree_height_cached(
                    tree,
                    id,
                    avail,
                    heights,
                    subtree_heights,
                );
                if *y >= bottom || *y + h <= top {
                    out.push(FlowItem::with_epoch(
                        self.alloc_id(),
                        FlowItemKind::Collapsed { box_id: id },
                        HeightState::Estimated(h),
                        self.layout_epoch,
                    ));
                    *y += h;
                    return;
                }
                let style = tree.style_of(node);
                let pad_top = style.top_border_padding();
                let pad_bottom = style.bottom_border_padding();
                let child_avail = (avail - style.inline_border_padding()).max(0.0);
                out.push(FlowItem::with_epoch(
                    self.alloc_id(),
                    FlowItemKind::ContainerOpen { box_id: id },
                    HeightState::Exact(pad_top),
                    self.layout_epoch,
                ));
                *y += pad_top;
                if children.is_empty() {
                    let g = self.gap_item(tree, id, FlowBoundary::Start, FlowBoundary::End);
                    *y += g.height.px();
                    out.push(g);
                } else {
                    for (i, child) in children.iter().enumerate() {
                        let before = if i == 0 {
                            FlowBoundary::Start
                        } else {
                            FlowBoundary::Child(children[i - 1])
                        };
                        let g = self.gap_item(tree, id, before, FlowBoundary::Child(*child));
                        *y += g.height.px();
                        out.push(g);
                        self.emit_intersecting(
                            tree,
                            *child,
                            child_avail,
                            y,
                            top,
                            bottom,
                            heights,
                            subtree_heights,
                            out,
                        );
                    }
                    let last = *children
                        .last()
                        .expect("vertical container has at least one child");
                    let g = self.gap_item(tree, id, FlowBoundary::Child(last), FlowBoundary::End);
                    *y += g.height.px();
                    out.push(g);
                }
                out.push(FlowItem::with_epoch(
                    self.alloc_id(),
                    FlowItemKind::ContainerClose { box_id: id },
                    HeightState::Exact(pad_bottom),
                    self.layout_epoch,
                ));
                *y += pad_bottom;
            }
            BoxChildren::Island(_) | BoxChildren::None => {
                let hs = heights(id, avail);
                out.push(FlowItem::with_epoch(
                    self.alloc_id(),
                    FlowItemKind::Content { box_id: id },
                    hs,
                    self.layout_epoch,
                ));
                *y += hs.px();
            }
        }
    }
}
