use super::anchor::ScrollAnchor;
use super::engine::IncrementalEngine;
use super::store::EvictionPolicy;
use md_core::Px;
use md_core::block::{BlockId, BlockKind};
use md_layout::box_tree::{BoxRole, LayoutBoxId};
use md_layout::flow::HeightState;
use md_layout::island::IslandSolver;
use md_layout::shaper::TextMeasure;
use md_layout::spine::{COLLAPSE_QUOTA, FlowItemId, FlowItemKind};
use std::collections::BTreeSet;
use std::rc::Rc;

const MAX_ANCHOR_CARRY: u32 = 8;

const EVICT_COUNT_FLOOR: usize = 512;
const EVICT_COUNT_PER_PROTECTED: usize = 4;

impl IncrementalEngine {
    fn debug_resolve_top(&self, sa: ScrollAnchor) -> Px {
        self.spine.resolve(sa.item, sa.within)
    }

    fn evict_one(&mut self, id: LayoutBoxId) -> bool {
        if !self.store.evict(id) {
            return false;
        }
        let h = self.estimate_island(id);
        self.set_content_height(id, HeightState::Estimated(h));
        true
    }

    pub(super) fn run_eviction(&mut self, top: Px, viewport_h: Px) -> u64 {
        let EvictionPolicy::Windowed { keep_screens } = self.eviction else {
            return 0;
        };

        let pad = keep_screens * viewport_h;
        let warm_lo = (top - pad).max(0.0);
        let warm_hi = top + viewport_h + pad;

        let mut warm: BTreeSet<LayoutBoxId> = BTreeSet::new();
        for pos in self.spine.visible(warm_lo, warm_hi) {
            if let FlowItemKind::Content { box_id } = self.spine.item_at(pos).kind {
                warm.insert(box_id);
            }
        }
        let mut hot: BTreeSet<LayoutBoxId> = BTreeSet::new();
        for pos in self.spine.visible(top, top + viewport_h) {
            if let FlowItemKind::Content { box_id } = self.spine.item_at(pos).kind {
                hot.insert(box_id);
            }
        }
        for p in &self.pins {
            hot.insert(*p);
            warm.insert(*p);
        }

        let mut evicted = 0u64;
        let cold: Vec<LayoutBoxId> = self
            .store
            .last_used
            .keys()
            .copied()
            .filter(|id| !warm.contains(id))
            .collect();
        for id in cold {
            if self.evict_one(id) {
                evicted += 1;
            }
        }

        let valve = EVICT_COUNT_FLOOR.max(warm.len().saturating_mul(EVICT_COUNT_PER_PROTECTED));
        if self.store.materialized_count() <= valve {
            return evicted;
        }

        let mut cands: Vec<(u64, LayoutBoxId)> = self
            .store
            .last_used
            .iter()
            .filter(|(id, _)| !hot.contains(*id))
            .map(|(id, t)| (*t, *id))
            .collect();
        cands.sort();
        let mut cur = self.store.materialized_count();
        for (_, id) in cands {
            if cur <= valve {
                break;
            }
            if self.evict_one(id) {
                evicted += 1;
                cur -= 1;
            }
        }
        evicted
    }

    pub(super) fn run_collapse(&mut self, top: Px, viewport_h: Px, keep_item: FlowItemId) -> u32 {
        let EvictionPolicy::Windowed { keep_screens } = self.eviction else {
            return 0;
        };
        let pad = keep_screens * viewport_h;
        let protect_lo = (top - pad).max(0.0);
        let protect_hi = top + viewport_h + pad;
        let keep = (!keep_item.is_none()).then_some(keep_item);
        let pins: Vec<LayoutBoxId> = self.pins.iter().copied().collect();
        let tree = self.tree.clone();
        self.spine
            .collapse_far(&tree, protect_lo, protect_hi, keep, &pins)
    }

    pub(super) fn run_release(&mut self, top: Px, viewport_h: Px, keep_item: FlowItemId) -> u32 {
        if !self.lazy {
            return 0;
        }
        let EvictionPolicy::Windowed { keep_screens } = self.eviction else {
            return 0;
        };
        let pad = keep_screens * viewport_h;
        let protect_lo = (top - pad).max(0.0);
        let protect_hi = top + viewport_h + pad;
        let keep = (!keep_item.is_none()).then_some(keep_item);
        let pins: Vec<LayoutBoxId> = self.pins.iter().copied().collect();
        let tree = Rc::clone(&self.tree);
        let ids = self.spine.next_release_targets(
            &tree,
            protect_lo,
            protect_hi,
            keep,
            &pins,
            COLLAPSE_QUOTA,
        );
        let mut n = 0u32;
        for id in ids {
            if self.spine.content_id(id).is_some() {
                let _ = self.spine.demote_content_to_collapsed(id);
            }
            if id.role == BoxRole::Frame
                && let Some(block) = id.block()
            {
                let preview = LayoutBoxId::preview(block);
                if self.spine.content_id(preview).is_some() {
                    let _ = self.spine.demote_content_to_collapsed(preview);
                }
            }
            let Some(height) = self.spine.box_item_height(id) else {
                break;
            };
            let mut islands = Vec::new();
            self.collect_islands(id, &mut islands);
            for island in islands {
                let _ = self.store.evict(island);
            }
            let tree = Rc::make_mut(&mut self.tree);
            md_layout::compose::defer_composed(tree, id, height);
            n += 1;
        }
        if n > 0 {
            let live = Rc::clone(&self.tree);
            Rc::make_mut(&mut self.table_cons).retain(|k, _| live.nodes().contains_key(k));
        }
        n
    }

    pub fn warm_media_blocks(&self, top: Px, viewport_h: Px) -> Vec<(BlockId, BlockKind, Px)> {
        let EvictionPolicy::Windowed { keep_screens } = self.eviction else {
            return Vec::new();
        };
        let pad = keep_screens * viewport_h;
        if pad <= 0.0 {
            return Vec::new();
        }
        let lo = (top - pad).max(0.0);
        let hi = top + viewport_h + pad;
        let viewport_width = self.viewport_width();
        let mut out = Vec::new();
        for pos in self.spine.visible(lo, hi) {
            let (FlowItemKind::Content { box_id } | FlowItemKind::Collapsed { box_id }) =
                self.spine.item_at(pos).kind
            else {
                continue;
            };
            let mut leaves = Vec::new();
            self.collect_islands(box_id, &mut leaves);
            for leaf in leaves {
                let Some(block) = leaf.block() else {
                    continue;
                };
                let Some(node) = self.tree.nodes().get(&leaf) else {
                    continue;
                };
                if node.edit_source() {
                    continue;
                }
                let kind = node.kind();
                if !matches!(
                    kind,
                    BlockKind::Mermaid | BlockKind::Math | BlockKind::Image
                ) {
                    continue;
                }
                out.push((block, kind, self.tree.content_width(leaf, viewport_width)));
            }
        }
        out
    }

    pub(super) fn ensure_anchor_exact(
        &mut self,
        sa: ScrollAnchor,
        measure: &dyn TextMeasure,
        solver: &dyn IslandSolver,
    ) -> ScrollAnchor {
        if sa.item.is_none() {
            return sa;
        }
        let mut sa = sa;
        for _ in 0..MAX_ANCHOR_CARRY {
            let kind = self.spine.get(sa.item).map(|it| it.kind);
            let Some(kind) = kind else {
                return sa;
            };
            if let FlowItemKind::Content { box_id } = kind
                && !self
                    .store
                    .is_fresh(&self.tree, box_id, self.viewport_width())
            {
                self.materialize_one(box_id, measure, solver);
            }
            let h = self.spine.get(sa.item).map_or(0.0, |it| it.height.px());
            if h <= 0.0 || sa.within < h {
                return sa;
            }
            let next = self.spine.location(sa.item).and_then(|pos| {
                (pos + 1 < self.spine.len()).then(|| self.spine.item_at(pos + 1).id)
            });
            let Some(next) = next else {
                return ScrollAnchor {
                    item: sa.item,
                    within: strict_predecessor(h),
                };
            };
            sa = ScrollAnchor {
                item: next,
                within: sa.within - h,
            };
        }
        sa
    }

    pub fn anchor_at_y(
        &mut self,
        y: Px,
        measure: &dyn TextMeasure,
        solver: &dyn IslandSolver,
    ) -> ScrollAnchor {
        if self.spine.is_empty() {
            return ScrollAnchor::top();
        }
        let Some(item) = self.spine.y_to_item(y) else {
            return ScrollAnchor::top();
        };
        let t = self.spine.item_top(item).unwrap_or(0.0);
        let raw = ScrollAnchor {
            item,
            within: (y - t).max(0.0),
        };
        self.ensure_anchor_exact(raw, measure, solver)
    }

    pub fn anchor_to_y(&self, sa: ScrollAnchor) -> Px {
        self.debug_resolve_top(sa)
    }
}

fn strict_predecessor(value: Px) -> Px {
    debug_assert!(value.is_finite() && value > 0.0);
    Px::from_bits(value.to_bits() - 1)
}

#[cfg(test)]
mod tests {
    use super::strict_predecessor;

    #[test]
    fn strict_predecessor_stays_below_tiny_positive_heights() {
        for value in [f64::MIN_POSITIVE, 1e-300, 1.0] {
            let predecessor = strict_predecessor(value);
            assert!(predecessor < value, "{predecessor} is not below {value}");
            assert!(predecessor >= 0.0);
        }
    }
}
