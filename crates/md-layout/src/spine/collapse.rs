use super::FlowSpine;
use super::item::{FlowItem, FlowItemId, FlowItemKind};
use crate::box_tree::{BoxRole, BoxTree, LayoutBoxId};
use crate::flow::HeightState;
use md_core::Px;
use std::ops::Range;

pub const COLLAPSE_QUOTA: u32 = 8;

struct CollapseFar<'a> {
    tree: &'a BoxTree,
    protect_lo: Px,
    protect_hi: Px,
    keep_item: Option<FlowItemId>,
    keep_boxes: &'a [LayoutBoxId],
}

impl FlowSpine {
    #[must_use]
    pub fn collapse_far(
        &mut self,
        tree: &BoxTree,
        protect_lo: Px,
        protect_hi: Px,
        keep_item: Option<FlowItemId>,
        keep_boxes: &[LayoutBoxId],
    ) -> u32 {
        if self.open_of.len() <= 1 {
            return 0;
        }
        let ctx = CollapseFar {
            tree,
            protect_lo,
            protect_hi,
            keep_item,
            keep_boxes,
        };
        let mut n = 0u32;
        while n < COLLAPSE_QUOTA {
            let Some(id) = self.next_far_open(&ctx) else {
                break;
            };
            if !self.try_collapse(id, &ctx) {
                break;
            }
            n += 1;
        }
        n
    }

    pub fn next_release_targets(
        &self,
        tree: &BoxTree,
        protect_lo: Px,
        protect_hi: Px,
        keep_item: Option<FlowItemId>,
        keep_boxes: &[LayoutBoxId],
        limit: u32,
    ) -> Vec<LayoutBoxId> {
        if limit == 0 {
            return Vec::new();
        }
        let ctx = CollapseFar {
            tree,
            protect_lo,
            protect_hi,
            keep_item,
            keep_boxes,
        };
        let root = tree.root();
        let mut collapsed: Vec<(usize, LayoutBoxId)> = Vec::new();
        let mut content: Vec<(usize, LayoutBoxId)> = Vec::new();
        for node in tree.nodes().live_nodes() {
            let box_id = node.id();
            if box_id == root
                || ctx.keep_boxes.contains(&box_id)
                || self.open_of.contains_key(&box_id)
            {
                continue;
            }
            if !matches!(box_id.role, BoxRole::Frame | BoxRole::Cell) {
                continue;
            }
            if let Some(&fid) = self.collapsed_of.get(&box_id) {
                let Some(pos) = self.location(fid) else {
                    continue;
                };
                if self.release_unit_is_unprotected(box_id, pos, &ctx) {
                    collapsed.push((pos, box_id));
                }
            } else if let Some(&fid) = self.content_of.get(&box_id) {
                let Some(pos) = self.location(fid) else {
                    continue;
                };
                if self.release_unit_is_unprotected(box_id, pos, &ctx) {
                    content.push((pos, box_id));
                }
            }
        }
        collapsed.sort_unstable_by_key(|k| k.0);
        content.sort_unstable_by_key(|k| k.0);
        collapsed
            .into_iter()
            .chain(content)
            .map(|(_, id)| id)
            .take(limit as usize)
            .collect()
    }

    pub fn demote_content_to_collapsed(&mut self, box_id: LayoutBoxId) -> Option<Px> {
        let fid = self.content_of.get(&box_id).copied()?;
        let pos = self.location(fid)?;
        self.tally(pos..pos + 1, false);
        self.content_of.remove(&box_id);
        let px = self.items[pos].height.px();
        self.items[pos].kind = FlowItemKind::Collapsed { box_id };
        self.items[pos].height = HeightState::Estimated(px);
        self.collapsed_of.insert(box_id, fid);
        self.tally(pos..pos + 1, true);
        Some(px)
    }

    pub fn box_item_height(&self, box_id: LayoutBoxId) -> Option<Px> {
        let fid = self
            .collapsed_of
            .get(&box_id)
            .or_else(|| self.content_of.get(&box_id))
            .copied()?;
        let pos = self.location(fid)?;
        Some(self.items[pos].height.px())
    }

    fn next_far_open(&self, ctx: &CollapseFar<'_>) -> Option<LayoutBoxId> {
        let root = ctx.tree.root();
        let mut best: Option<(usize, usize, LayoutBoxId)> = None;
        for &box_id in self.open_of.keys() {
            if box_id == root || ctx.keep_boxes.contains(&box_id) {
                continue;
            }
            let Some(range) = self.fragment_range(box_id) else {
                continue;
            };
            if range.end <= range.start + 1 {
                continue;
            }
            if !self.span_is_unprotected(range.start..range.end, ctx) {
                continue;
            }
            match best {
                None => best = Some((range.start, range.end, box_id)),
                Some((start, end, _))
                    if range.start < start || (range.start == start && range.end > end) =>
                {
                    best = Some((range.start, range.end, box_id));
                }
                _ => {}
            }
        }
        best.map(|(_, _, id)| id)
    }

    fn release_unit_is_unprotected(
        &self,
        host: LayoutBoxId,
        pos: usize,
        ctx: &CollapseFar<'_>,
    ) -> bool {
        if host.role != BoxRole::Frame {
            return self.span_is_unprotected(pos..pos + 1, ctx);
        }
        let Some(block) = host.block() else {
            return self.span_is_unprotected(pos..pos + 1, ctx);
        };
        let preview = LayoutBoxId::preview(block);
        if ctx.keep_boxes.contains(&preview) {
            return false;
        }
        let Some(ppos) = self.spine_pos(preview) else {
            return self.span_is_unprotected(pos..pos + 1, ctx);
        };
        self.span_is_unprotected(pos..pos + 1, ctx) && self.span_is_unprotected(ppos..ppos + 1, ctx)
    }

    fn span_is_unprotected(&self, range: Range<usize>, ctx: &CollapseFar<'_>) -> bool {
        let top = self.fenwick.prefix(range.start);
        let bot = self.fenwick.prefix(range.end);
        if bot > ctx.protect_lo && top <= ctx.protect_hi {
            return false;
        }
        if let Some(pos) = ctx.keep_item.and_then(|id| self.location(id))
            && pos >= range.start
            && pos < range.end
        {
            return false;
        }
        for keep in ctx.keep_boxes {
            if let Some(pos) = self.spine_pos(*keep)
                && pos >= range.start
                && pos < range.end
            {
                return false;
            }
        }
        true
    }

    fn try_collapse(&mut self, box_id: LayoutBoxId, ctx: &CollapseFar<'_>) -> bool {
        if ctx.keep_boxes.contains(&box_id) {
            return false;
        }
        let Some(range) = self.fragment_range(box_id) else {
            return false;
        };
        if range.end <= range.start + 1 {
            return false;
        }
        if !self.span_is_unprotected(range.start..range.end, ctx) {
            return false;
        }
        let height = HeightState::Estimated(
            self.fenwick.prefix(range.end) - self.fenwick.prefix(range.start),
        );
        self.collapse_span(range.start, range.end, box_id, height);
        true
    }

    fn spine_pos(&self, box_id: LayoutBoxId) -> Option<usize> {
        self.content_of
            .get(&box_id)
            .or_else(|| self.collapsed_of.get(&box_id))
            .or_else(|| self.open_of.get(&box_id))
            .copied()
            .and_then(|id| self.location(id))
    }

    fn collapse_span(
        &mut self,
        start: usize,
        end: usize,
        box_id: LayoutBoxId,
        height: HeightState,
    ) {
        debug_assert!(end > start);
        self.tally(start..end, false);
        for pos in start..end {
            let item = self.items[pos];
            self.unmap(&item);
        }
        let item = FlowItem::with_epoch(
            self.alloc_id(),
            FlowItemKind::Collapsed { box_id },
            height,
            self.layout_epoch,
        );
        self.items.splice(start..end, std::iter::once(item));
        self.remap_from(start);
        self.tally(start..start + 1, true);
        self.splice_height_index(start, end - start, 1);
    }
}
