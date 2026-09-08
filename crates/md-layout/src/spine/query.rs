use super::FlowSpine;
use super::item::{FlowItem, FlowItemId, FlowItemKind};
use crate::box_tree::{BoxTree, LayoutBoxId};
use crate::flow::HeightState;
use md_core::Px;
use std::ops::Range;

impl FlowSpine {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn total_height(&self) -> Px {
        self.fenwick.total()
    }

    pub fn estimated_count(&self) -> u32 {
        self.estimated_count
    }

    pub fn set_viewport_width(&mut self, viewport_width: Px) {
        self.viewport_width = viewport_width;
        self.invalidate_layout_epoch();
    }

    pub fn invalidate_layout_epoch(&mut self) {
        self.layout_epoch = self.layout_epoch.saturating_add(1);
        self.exact_content_in_epoch = 0;
        self.estimated_count = self.noncontent_estimated + self.content_count;
    }

    pub fn effective_height(&self, pos: usize) -> HeightState {
        let item = &self.items[pos];
        if !item.is_content() {
            return item.height;
        }
        if item.height.is_exact() && item.height_epoch == self.layout_epoch {
            item.height
        } else {
            HeightState::Estimated(item.height.px())
        }
    }

    pub fn location(&self, id: FlowItemId) -> Option<usize> {
        if id.is_none() {
            return None;
        }
        let i = id.index as usize;
        let g = *self.gens.get(i)?;
        if g != id.generation {
            return None;
        }
        let p = *self.loc.get(i)?;
        if p == 0 {
            return None;
        }
        Some((p - 1) as usize)
    }

    pub fn get(&self, id: FlowItemId) -> Option<&FlowItem> {
        let pos = self.location(id)?;
        self.items.get(pos)
    }

    pub fn content_id(&self, box_id: LayoutBoxId) -> Option<FlowItemId> {
        self.content_of.get(&box_id).copied()
    }

    pub fn collapsed_id(&self, box_id: LayoutBoxId) -> Option<FlowItemId> {
        self.collapsed_of.get(&box_id).copied()
    }

    pub fn refresh_collapsed_height(
        &mut self,
        tree: &crate::box_tree::BoxTree,
        box_id: LayoutBoxId,
        viewport_width: Px,
        heights: &dyn Fn(LayoutBoxId, Px) -> HeightState,
    ) -> bool {
        let Some(item) = self.collapsed_of.get(&box_id).copied() else {
            return false;
        };
        let avail = tree.avail_width(box_id, viewport_width);
        let h = super::tree_walk::subtree_height(tree, box_id, avail, heights);
        self.set_height(item, HeightState::Estimated(h));
        true
    }

    pub fn refresh_deferred_height(
        &mut self,
        tree: &crate::box_tree::BoxTree,
        box_id: LayoutBoxId,
    ) -> bool {
        let Some(item) = self.collapsed_of.get(&box_id).copied() else {
            return false;
        };
        let Some(h) = tree.deferred_height(box_id) else {
            return false;
        };
        self.set_height(item, HeightState::Estimated(h));
        true
    }

    pub fn refresh_gaps_of(&mut self, tree: &BoxTree, container: LayoutBoxId) -> bool {
        use super::gap::FlowBoundary;
        let (Some(open), Some(close)) = (
            self.open_of.get(&container).copied(),
            self.close_of.get(&container).copied(),
        ) else {
            return false;
        };
        let (Some(lo), Some(hi)) = (self.location(open), self.location(close)) else {
            return false;
        };
        if lo >= hi || hi >= self.items.len() {
            return false;
        }
        let kids: Vec<usize> = (lo + 1..hi)
            .filter(|&pos| match self.items[pos].kind {
                FlowItemKind::Gap => false,
                FlowItemKind::ContainerClose { .. } => false,
                _ => {
                    let owner = self.child_box_at(pos);
                    let parent = tree
                        .nodes
                        .get(&owner)
                        .and_then(|n| n.parent)
                        .or_else(|| tree.deferred(owner).and_then(|d| d.parent));
                    parent == Some(container)
                }
            })
            .collect();
        let mut touched = false;
        for (i, &child) in kids.iter().enumerate() {
            let Some(gap_pos) = self.gap_before_child(child) else {
                continue;
            };
            let before = if i == 0 {
                FlowBoundary::Start
            } else {
                FlowBoundary::Child(self.child_box_at(kids[i - 1]))
            };
            let after = FlowBoundary::Child(self.child_box_at(child));
            let h = super::gap::gap_height(tree, container, before, after);
            let id = self.items[gap_pos].id;
            self.set_height(id, HeightState::Exact(h));
            touched = true;
        }
        if let Some(&last) = kids.last()
            && matches!(self.items[hi - 1].kind, FlowItemKind::Gap)
        {
            let before = FlowBoundary::Child(self.child_box_at(last));
            let h = super::gap::gap_height(tree, container, before, FlowBoundary::End);
            let id = self.items[hi - 1].id;
            self.set_height(id, HeightState::Exact(h));
            touched = true;
        }
        touched
    }

    fn child_box_at(&self, pos: usize) -> LayoutBoxId {
        match self.items[pos].kind {
            FlowItemKind::Content { box_id }
            | FlowItemKind::Collapsed { box_id }
            | FlowItemKind::ContainerOpen { box_id }
            | FlowItemKind::ContainerClose { box_id } => box_id,
            FlowItemKind::Gap => unreachable!("caller skips gaps"),
        }
    }

    fn gap_before_child(&self, child_pos: usize) -> Option<usize> {
        let pos = child_pos.checked_sub(1)?;
        matches!(self.items[pos].kind, FlowItemKind::Gap).then_some(pos)
    }

    pub fn item_top(&self, id: FlowItemId) -> Option<Px> {
        let pos = self.location(id)?;
        Some(self.fenwick.prefix(pos))
    }

    pub fn item_at(&self, pos: usize) -> &FlowItem {
        &self.items[pos]
    }

    pub fn y_to_item(&self, y: Px) -> Option<FlowItemId> {
        let n = self.items.len();
        if n == 0 {
            return None;
        }
        let y = y.max(0.0);
        let total = self.total_height();
        if total <= 0.0 {
            return Some(self.items[0].id);
        }
        let y = if y >= total {
            (total * (1.0 - f64::EPSILON)).max(0.0)
        } else {
            y
        };
        let mut lo = 0usize;
        let mut hi = n;
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.fenwick.prefix(mid + 1) <= y {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let i = lo.min(n - 1);
        Some(self.items[i].id)
    }

    pub fn visible(&self, top: Px, bottom: Px) -> Range<usize> {
        if self.items.is_empty() {
            return 0..0;
        }
        let Some(first_id) = self.y_to_item(top) else {
            return 0..0;
        };
        let first = self.location(first_id).unwrap_or(0);
        let mut last = first;
        let mut cursor = self.fenwick.prefix(first + 1);
        while last + 1 < self.items.len() && cursor < bottom {
            last += 1;
            cursor += self.items[last].height.px();
        }
        first..(last + 1)
    }
}
