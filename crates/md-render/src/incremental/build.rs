use super::anchor::ScrollAnchor;
use super::engine::{IncrementalEngine, MAX_ITERATIONS, PublishedFrame};
use super::store::{EvictionPolicy, SolveRequest};
use md_core::Px;
use md_core::block::BlockKind;
use md_layout::assembly::Assembly;
use md_layout::box_tree::LayoutBoxId;
use md_layout::compose::compose_into;
use md_layout::flow::HeightState;
use md_layout::island::{IslandSolver, TableColumnConstraintSet};
use md_layout::shaper::TextMeasure;
use md_layout::spine::{COLLAPSE_QUOTA, FlowItemKind};
use std::collections::BTreeMap;
use std::rc::Rc;

impl IncrementalEngine {
    fn ensure_table_cons(&mut self, table: LayoutBoxId) -> Option<TableColumnConstraintSet> {
        if self.tree.get(table).kind() != BlockKind::Table {
            return None;
        }
        if let Some(c) = self.table_cons.get(&table) {
            return Some(c.clone());
        }
        let cons = self.resolve_cons(table);
        Rc::make_mut(&mut self.table_cons).insert(table, cons.clone());
        Some(cons)
    }

    pub fn settle(
        &mut self,
        sa: ScrollAnchor,
        viewport_h: Px,
        measure: &dyn TextMeasure,
        solver: &dyn IslandSolver,
    ) -> PublishedFrame {
        self.settle_inner(None, sa, viewport_h, measure, solver)
    }

    pub fn settle_with_doc(
        &mut self,
        doc: &md_core::document::Document,
        sa: ScrollAnchor,
        viewport_h: Px,
        measure: &dyn TextMeasure,
        solver: &dyn IslandSolver,
    ) -> PublishedFrame {
        self.settle_inner(Some(doc), sa, viewport_h, measure, solver)
    }

    fn settle_inner(
        &mut self,
        doc: Option<&md_core::document::Document>,
        sa: ScrollAnchor,
        viewport_h: Px,
        measure: &dyn TextMeasure,
        solver: &dyn IslandSolver,
    ) -> PublishedFrame {
        let mut iterations = 0u32;

        let mut sa = self.ensure_anchor_exact(sa, measure, solver);

        loop {
            iterations += 1;
            let force_publish = iterations > MAX_ITERATIONS;

            let top = self.spine.resolve(sa.item, sa.within);
            let bottom = top + viewport_h;
            if let Some(doc) = doc {
                self.realize_visible(doc, top, bottom);
            }
            {
                let tree = Rc::clone(&self.tree);
                let estimator = self.estimator;
                let store = &self.store;
                let _ = self.spine.expand_visible(&tree, top, bottom, &|id, avail| {
                    HeightState::Estimated(if tree.nodes().get(&id).is_some() {
                        store.height_px(&tree, estimator, id, avail)
                    } else {
                        tree.deferred_height(id).unwrap_or(0.0)
                    })
                });
            }
            if !force_publish {
                if !sa.item.is_none() && self.spine.get(sa.item).is_none() {
                    sa = match self.spine.y_to_item(top) {
                        Some(item) => {
                            let t = self.spine.item_top(item).unwrap_or(0.0);
                            ScrollAnchor {
                                item,
                                within: (top - t).max(0.0),
                            }
                        }
                        None => ScrollAnchor::top(),
                    };
                }

                let top = self.spine.resolve(sa.item, sa.within);
                let bottom = top + viewport_h;
                let range = self.spine.visible(top, bottom);

                let mut pending: Vec<LayoutBoxId> = Vec::new();
                let mut collapsed_in_window = false;
                for pos in range {
                    let item = self.spine.item_at(pos);
                    match item.kind {
                        FlowItemKind::Content { box_id } => {
                            if !self
                                .store
                                .is_fresh(&self.tree, box_id, self.viewport_width())
                                || !self.spine.effective_height(pos).is_exact()
                            {
                                pending.push(box_id);
                            }
                        }
                        FlowItemKind::Collapsed { box_id } => {
                            if self.tree.deferred_height(box_id).is_some()
                                || self.tree.nodes().contains_key(&box_id)
                            {
                                collapsed_in_window = true;
                            }
                        }
                        FlowItemKind::ContainerOpen { .. }
                        | FlowItemKind::ContainerClose { .. }
                        | FlowItemKind::Gap => {}
                    }
                }

                if !pending.is_empty() {
                    for id in pending {
                        self.materialize_one(id, measure, solver);
                    }
                    continue;
                }
                if collapsed_in_window {
                    continue;
                }
            }

            let top = self.spine.resolve(sa.item, sa.within);
            let bottom = top + viewport_h;
            let range = self.spine.visible(top, bottom);

            let mut visible_content: Vec<LayoutBoxId> = Vec::new();
            for pos in range {
                let item = self.spine.item_at(pos);
                if let FlowItemKind::Content { box_id } = item.kind {
                    visible_content.push(box_id);
                }
            }
            for b in &visible_content {
                self.store.touch(*b);
            }

            let _ = self.run_eviction(top, viewport_h);
            let _ = self.run_collapse(top, viewport_h, sa.item);
            let _ = self.run_release(top, viewport_h, sa.item);
            if let Some(doc) = doc {
                self.realize_warm(doc, top, bottom, viewport_h);
            }

            return PublishedFrame {
                iterations,
                resolved_top: top,
            };
        }
    }

    pub(super) fn materialize_one(
        &mut self,
        id: LayoutBoxId,
        measure: &dyn TextMeasure,
        solver: &dyn IslandSolver,
    ) {
        let avail = self.tree.avail_width(id, self.viewport_width());
        let cons = self
            .tree
            .ancestor_table(id)
            .and_then(|table| self.ensure_table_cons(table));
        let mut stats = std::mem::take(&mut self.island_stats);
        let req = SolveRequest {
            tree: &self.tree,
            id,
            avail,
            measure,
            cons: cons.as_ref(),
            solver,
        };
        self.store.materialize(req, &mut stats);
        self.island_stats = stats;
        if let Some(g) = self.store.get(id) {
            let h = g.border_box_height;
            self.set_content_height(id, HeightState::Exact(h));
        }
    }

    pub fn assemble_incremental(
        &mut self,
        sa: ScrollAnchor,
        viewport_h: Px,
        measure: &dyn TextMeasure,
        solver: &dyn IslandSolver,
    ) -> (Assembly, PublishedFrame) {
        self.assemble_inner(None, sa, viewport_h, measure, solver)
    }

    pub fn assemble_with_doc(
        &mut self,
        doc: &md_core::document::Document,
        sa: ScrollAnchor,
        viewport_h: Px,
        measure: &dyn TextMeasure,
        solver: &dyn IslandSolver,
    ) -> (Assembly, PublishedFrame) {
        self.assemble_inner(Some(doc), sa, viewport_h, measure, solver)
    }

    fn assemble_inner(
        &mut self,
        doc: Option<&md_core::document::Document>,
        sa: ScrollAnchor,
        viewport_h: Px,
        measure: &dyn TextMeasure,
        solver: &dyn IslandSolver,
    ) -> (Assembly, PublishedFrame) {
        let published = self.settle_inner(doc, sa, viewport_h, measure, solver);
        let top = published.resolved_top;
        let pin_boxes: Vec<LayoutBoxId> = self.pins.iter().copied().collect();
        let window = self
            .spine
            .window(&self.tree, top, top + viewport_h, &pin_boxes);
        let mut heights = BTreeMap::new();
        let mut geometries = BTreeMap::new();
        for e in &window.entries {
            if let FlowItemKind::Content { box_id } = e.kind {
                if geometries.contains_key(&box_id) {
                    continue;
                }
                if self
                    .store
                    .is_fresh(&self.tree, box_id, self.viewport_width())
                    && let Some(g) = self.store.get(box_id)
                {
                    heights.insert(box_id, HeightState::Exact(g.border_box_height));
                    geometries.insert(box_id, g.clone());
                } else {
                    heights.insert(box_id, HeightState::Estimated(e.height));
                }
            }
        }
        for id in window.extra_spans.keys() {
            if !geometries.contains_key(id)
                && self.store.is_fresh(&self.tree, *id, self.viewport_width())
                && let Some(g) = self.store.get(*id)
            {
                heights.insert(*id, HeightState::Exact(g.border_box_height));
                geometries.insert(*id, g.clone());
            }
        }

        self.pins.clear();
        self.bump_publish_gen();
        let island_stats = std::mem::take(&mut self.island_stats);
        let assembly = Assembly {
            tree: Rc::clone(&self.tree),
            table_cons: Rc::clone(&self.table_cons),
            heights,
            geometries,
            window: Some(window),
            island_stats,
        };
        (assembly, published)
    }

    pub fn materialize_pin_block(
        &mut self,
        block: md_core::block::BlockId,
        measure: &dyn TextMeasure,
        solver: &dyn IslandSolver,
    ) {
        let Some(island) = self.island_box_of_block(block) else {
            return;
        };
        self.pins.insert(island);
        if !self
            .store
            .is_fresh(&self.tree, island, self.viewport_width())
        {
            self.materialize_one(island, measure, solver);
        }
    }

    #[must_use]
    pub fn ensure_block_on_spine(
        &mut self,
        block: md_core::block::BlockId,
        measure: &dyn TextMeasure,
        solver: &dyn IslandSolver,
    ) -> bool {
        let Some(island) = self.island_box_of_block(block) else {
            return false;
        };
        let on_spine = {
            let tree = Rc::clone(&self.tree);
            let estimator = self.estimator;
            let store = &self.store;
            self.spine.expand_to(&tree, island, &|id, avail| {
                HeightState::Estimated(store.height_px(&tree, estimator, id, avail))
            })
        };
        if !on_spine {
            return false;
        }
        self.materialize_pin_block(block, measure, solver);
        true
    }

    pub fn ensure_composed_block(
        &mut self,
        doc: &md_core::document::Document,
        block: md_core::block::BlockId,
        measure: &dyn TextMeasure,
        solver: &dyn IslandSolver,
    ) -> bool {
        self.compose_block(doc, block);
        self.ensure_block_on_spine(block, measure, solver)
    }

    fn realize_visible(&mut self, doc: &md_core::document::Document, top: Px, bottom: Px) {
        self.realize_deferred_in(doc, top, bottom, None);
    }

    fn realize_warm(
        &mut self,
        doc: &md_core::document::Document,
        top: Px,
        bottom: Px,
        viewport_h: Px,
    ) {
        let EvictionPolicy::Windowed { keep_screens } = self.eviction else {
            return;
        };
        let pad = keep_screens * viewport_h;
        if pad <= 0.0 {
            return;
        }
        let lo = (top - pad).max(0.0);
        let hi = bottom + pad;
        let below = self.realize_deferred_in(doc, bottom, hi, Some(COLLAPSE_QUOTA));
        if let Some(rest) = COLLAPSE_QUOTA.checked_sub(below).filter(|n| *n > 0) {
            self.realize_deferred_in(doc, lo, top, Some(rest));
        }
    }

    fn realize_deferred_in(
        &mut self,
        doc: &md_core::document::Document,
        lo: Px,
        hi: Px,
        limit: Option<u32>,
    ) -> u32 {
        if !self.tree.has_deferred() {
            return 0;
        }
        let mut pending = Vec::new();
        for pos in self.spine.visible(lo, hi) {
            let item = self.spine.item_at(pos);
            if let FlowItemKind::Collapsed { box_id } = item.kind
                && self.tree.deferred_height(box_id).is_some()
            {
                pending.push(box_id);
                if let Some(n) = limit
                    && pending.len() as u32 >= n
                {
                    break;
                }
            }
        }
        let n = pending.len() as u32;
        for id in pending {
            self.compose_box(doc, id);
        }
        n
    }

    fn compose_block(&mut self, doc: &md_core::document::Document, block: md_core::block::BlockId) {
        let Some(node_id) = doc.live_id(block) else {
            return;
        };
        let mut cur = Some(node_id);
        let mut deferred_hit = None;
        while let Some(id) = cur {
            let kind = doc
                .arena
                .get(id)
                .map(|n| n.kind)
                .unwrap_or(md_core::block::BlockKind::Paragraph);
            let box_id = LayoutBoxId::for_kind(kind, id.index);
            if self.tree.deferred_height(box_id).is_some() {
                deferred_hit = Some(box_id);
            }
            if self.tree.nodes().contains_key(&box_id) {
                break;
            }
            cur = doc.arena.get(id).and_then(|n| n.parent);
        }
        if let Some(id) = deferred_hit {
            self.compose_box(doc, id);
        }
    }

    fn compose_box(&mut self, doc: &md_core::document::Document, id: LayoutBoxId) {
        if self.tree.nodes().contains_key(&id) {
            return;
        }
        let tree = Rc::make_mut(&mut self.tree);
        compose_into(tree, doc, &self.layout, id);
    }
}
