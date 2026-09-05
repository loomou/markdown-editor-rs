use super::FlowSpine;
use super::gap::{FlowBoundary, gap_height};
use super::item::{FlowItem, FlowItemId, FlowItemKind};
use super::tree_walk::subtree_height;
use crate::box_tree::{BoxChildren, BoxTree, LayoutBoxId};
use crate::flow::HeightState;
use md_core::Px;
use std::ops::Range;

#[cfg(test)]
use super::fenwick::Fenwick;

impl FlowSpine {
    pub(super) fn alloc_id(&mut self) -> FlowItemId {
        while let Some(old) = self.free_ids.pop() {
            if let Some(generation) = old.generation.checked_add(1) {
                return FlowItemId {
                    index: old.index,
                    generation,
                };
            }
        }
        let index = self.next_index;
        assert!(index > 0, "FlowItemId exhausted");
        self.next_index = index
            .checked_add(1)
            .expect("FlowItemId exhausted (maximum index reached)");
        FlowItemId {
            index,
            generation: 1,
        }
    }

    pub(super) fn fragment_range(&self, box_id: LayoutBoxId) -> Option<Range<usize>> {
        if let (Some(open), Some(close)) = (self.open_of.get(&box_id), self.close_of.get(&box_id)) {
            let a = self.location(*open)?;
            let b = self.location(*close)?;
            return Some(a..(b + 1));
        }
        if let Some(id) = self.collapsed_of.get(&box_id) {
            let p = self.location(*id)?;
            return Some(p..(p + 1));
        }
        let id = self.content_of.get(&box_id)?;
        let p = self.location(*id)?;
        Some(p..(p + 1))
    }

    fn child_splice_span(
        &self,
        parent: LayoutBoxId,
        before: Option<LayoutBoxId>,
        removed: &[LayoutBoxId],
    ) -> Option<(usize, usize)> {
        let open = self.location(*self.open_of.get(&parent)?)?;
        let close = self.location(*self.close_of.get(&parent)?)?;
        let start = match before {
            None => open + 1,
            Some(b) => self.fragment_range(b)?.end,
        };
        if start >= close {
            return None;
        }
        let end = if removed.is_empty() {
            start + 1
        } else {
            let last = *removed.last()?;
            self.fragment_range(last)?.end + 1
        };
        if end > close || start >= end {
            return None;
        }
        Some((start, end))
    }

    fn splice_successor(
        kids: &[LayoutBoxId],
        before: Option<LayoutBoxId>,
        inserted: &[LayoutBoxId],
    ) -> Result<Option<LayoutBoxId>, ()> {
        let pivot = inserted.last().copied().or(before);
        match pivot {
            None => Ok(kids.first().copied()),
            Some(p) => kids
                .iter()
                .position(|x| *x == p)
                .map(|i| kids.get(i + 1).copied())
                .ok_or(()),
        }
    }

    pub(super) fn gap_item(
        &mut self,
        tree: &BoxTree,
        parent: LayoutBoxId,
        before: FlowBoundary,
        after: FlowBoundary,
    ) -> FlowItem {
        let h = gap_height(tree, parent, before, after);
        FlowItem::with_epoch(
            self.alloc_id(),
            FlowItemKind::Gap,
            HeightState::Exact(h),
            self.layout_epoch,
        )
    }

    pub(super) fn push_lowered(
        &mut self,
        tree: &BoxTree,
        id: LayoutBoxId,
        avail: Px,
        heights: &dyn Fn(LayoutBoxId, Px) -> HeightState,
        collapse_nested: bool,
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
            return;
        }
        let node = tree.get(id);
        match &node.children {
            BoxChildren::Vertical(_) if collapse_nested && id != tree.root => {
                let h = subtree_height(tree, id, avail, heights);
                out.push(FlowItem::with_epoch(
                    self.alloc_id(),
                    FlowItemKind::Collapsed { box_id: id },
                    HeightState::Estimated(h),
                    self.layout_epoch,
                ));
            }
            BoxChildren::Vertical(_) => {
                self.emit_vertical(tree, id, avail, heights, collapse_nested, out);
            }
            BoxChildren::Island(_) | BoxChildren::None => {
                out.push(FlowItem::with_epoch(
                    self.alloc_id(),
                    FlowItemKind::Content { box_id: id },
                    heights(id, avail),
                    self.layout_epoch,
                ));
            }
        }
    }

    fn emit_vertical(
        &mut self,
        tree: &BoxTree,
        id: LayoutBoxId,
        avail: Px,
        heights: &dyn Fn(LayoutBoxId, Px) -> HeightState,
        collapse_nested: bool,
        out: &mut Vec<FlowItem>,
    ) {
        let node = tree.get(id);
        let BoxChildren::Vertical(children) = &node.children else {
            return;
        };
        let style = tree.style_of(node);
        let top = style.top_border_padding();
        let bottom = style.bottom_border_padding();
        let child_avail = (avail - style.inline_border_padding()).max(0.0);
        out.push(FlowItem::with_epoch(
            self.alloc_id(),
            FlowItemKind::ContainerOpen { box_id: id },
            HeightState::Exact(top),
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
                self.push_lowered(tree, *child, child_avail, heights, collapse_nested, out);
            }
            let last = *children.last().unwrap();
            out.push(self.gap_item(tree, id, FlowBoundary::Child(last), FlowBoundary::End));
        }
        out.push(FlowItem::with_epoch(
            self.alloc_id(),
            FlowItemKind::ContainerClose { box_id: id },
            HeightState::Exact(bottom),
            self.layout_epoch,
        ));
    }

    #[must_use]
    pub fn splice_children(
        &mut self,
        tree: &BoxTree,
        parent: LayoutBoxId,
        before: Option<LayoutBoxId>,
        removed: &[LayoutBoxId],
        inserted: &[LayoutBoxId],
        heights: &dyn Fn(LayoutBoxId, Px) -> HeightState,
    ) -> bool {
        if tree.nodes.get(&parent).is_none() {
            return false;
        }
        if let Some(collapsed) = self.collapsed_ancestor(tree, parent) {
            let avail = tree.avail_width(collapsed, self.viewport_width);
            let h = subtree_height(tree, collapsed, avail, heights);
            if let Some(fid) = self.collapsed_of.get(&collapsed).copied() {
                self.set_height(fid, HeightState::Estimated(h));
            }
            return true;
        }
        let Some((at, end)) = self.child_splice_span(parent, before, removed) else {
            return false;
        };
        let kids = match &tree.get(parent).children {
            BoxChildren::Vertical(c) => c.as_slice(),

            BoxChildren::Island(_) => unreachable!(
                "row islands carry no open/close span; cell changes invalidate the row instead"
            ),
            BoxChildren::None => &[],
        };
        let Ok(successor) = Self::splice_successor(kids, before, inserted) else {
            return false;
        };
        let left = before
            .map(FlowBoundary::Child)
            .unwrap_or(FlowBoundary::Start);
        let right = successor
            .map(FlowBoundary::Child)
            .unwrap_or(FlowBoundary::End);
        let mut items = Vec::new();
        if inserted.is_empty() {
            items.push(self.gap_item(tree, parent, left, right));
        } else {
            items.push(self.gap_item(tree, parent, left, FlowBoundary::Child(inserted[0])));
            for (i, child) in inserted.iter().enumerate() {
                let avail = tree.avail_width(*child, self.viewport_width);
                self.push_lowered(tree, *child, avail, heights, true, &mut items);
                let after = if i + 1 < inserted.len() {
                    FlowBoundary::Child(inserted[i + 1])
                } else {
                    right
                };
                items.push(self.gap_item(tree, parent, FlowBoundary::Child(*child), after));
            }
        }
        self.splice(at, end - at, items);
        true
    }

    pub(super) fn remap_from(&mut self, from: usize) {
        for i in from..self.items.len() {
            let item = self.items[i];
            Self::grow_id_tables(&mut self.loc, &mut self.gens, item.id.index);
            let idx = item.id.index as usize;
            if self.loc[idx] == 0 {
                self.map_at(i);
            } else {
                self.loc[idx] = (i as u32) + 1;
                self.gens[idx] = item.id.generation;
            }
        }
    }

    pub(super) fn release_id(&mut self, id: FlowItemId) {
        let i = id.index as usize;
        if i < self.loc.len() {
            self.loc[i] = 0;
        }
        self.free_ids.push(id);
    }

    pub(super) fn unmap(&mut self, item: &FlowItem) {
        self.release_id(item.id);
        match item.kind {
            FlowItemKind::Content { box_id } => {
                self.content_of.remove(&box_id);
            }
            FlowItemKind::ContainerOpen { box_id, .. } => {
                self.open_of.remove(&box_id);
            }
            FlowItemKind::ContainerClose { box_id, .. } => {
                self.close_of.remove(&box_id);
            }
            FlowItemKind::Collapsed { box_id } => {
                self.collapsed_of.remove(&box_id);
            }
            FlowItemKind::Gap => {}
        }
    }

    pub(super) fn map_at(&mut self, pos: usize) {
        let item = self.items[pos];
        Self::grow_id_tables(&mut self.loc, &mut self.gens, item.id.index);
        self.loc[item.id.index as usize] = (pos as u32) + 1;
        self.gens[item.id.index as usize] = item.id.generation;
        match item.kind {
            FlowItemKind::Content { box_id } => {
                self.content_of.insert(box_id, item.id);
            }
            FlowItemKind::ContainerOpen { box_id, .. } => {
                self.open_of.insert(box_id, item.id);
            }
            FlowItemKind::ContainerClose { box_id, .. } => {
                self.close_of.insert(box_id, item.id);
            }
            FlowItemKind::Collapsed { box_id } => {
                self.collapsed_of.insert(box_id, item.id);
            }
            FlowItemKind::Gap => {}
        }
    }

    pub(super) fn tally(&mut self, range: Range<usize>, add: bool) {
        let epoch = self.layout_epoch;
        let mut content = 0u32;
        let mut exact = 0u32;
        let mut noncontent = 0u32;
        for item in &self.items[range] {
            if item.is_content() {
                content += 1;
                if item.height.is_exact() && item.height_epoch == epoch {
                    exact += 1;
                }
            } else if !item.height.is_exact() {
                noncontent += 1;
            }
        }
        if add {
            self.content_count += content;
            self.exact_content_in_epoch += exact;
            self.noncontent_estimated += noncontent;
        } else {
            self.content_count = self.content_count.saturating_sub(content);
            self.exact_content_in_epoch = self.exact_content_in_epoch.saturating_sub(exact);
            self.noncontent_estimated = self.noncontent_estimated.saturating_sub(noncontent);
        }
        self.estimated_count = self.noncontent_estimated
            + self
                .content_count
                .saturating_sub(self.exact_content_in_epoch);
    }

    pub(super) fn splice_height_index(&mut self, at: usize, delete: usize, insert: usize) {
        let Self { items, fenwick, .. } = self;
        fenwick.splice(at, delete, insert, &|i| items[i].height.px());
    }

    #[cfg(test)]
    pub(super) fn recount_heights(&mut self) {
        let mut content_count = 0u32;
        let mut exact_content_in_epoch = 0u32;
        let mut noncontent_estimated = 0u32;
        for item in &self.items {
            if item.is_content() {
                content_count += 1;
                if item.height.is_exact() && item.height_epoch == self.layout_epoch {
                    exact_content_in_epoch += 1;
                }
            } else if !item.height.is_exact() {
                noncontent_estimated += 1;
            }
        }
        self.content_count = content_count;
        self.exact_content_in_epoch = exact_content_in_epoch;
        self.noncontent_estimated = noncontent_estimated;
        self.estimated_count =
            noncontent_estimated + content_count.saturating_sub(exact_content_in_epoch);
    }

    #[cfg(test)]
    pub(super) fn rebuild_height_index(&mut self) {
        self.fenwick = Fenwick::from_heights(
            &self
                .items
                .iter()
                .map(|item| item.height.px())
                .collect::<Vec<_>>(),
        );
        self.recount_heights();
    }
}
