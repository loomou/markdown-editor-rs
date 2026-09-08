use super::engine::{DeferredSettlement, IncrementalEngine, PendingSpineSplice};
use md_core::Px;
use md_core::block::BlockKind;
use md_core::document::{ChangeSet, DocChange, Document, NodeId};
use md_layout::box_tree::{BoxChildren, LayoutBoxId};
use md_layout::compose::{sync_block_edit, sync_layout};
use md_layout::flow::HeightState;
use md_layout::island::IslandSolver;
use md_layout::shaper::TextMeasure;
use std::collections::HashSet;
use std::rc::Rc;

impl IncrementalEngine {
    fn splices_replayable(&self, doc: &Document, changes: &ChangeSet) -> bool {
        let mut transient: HashSet<NodeId> = HashSet::new();
        for c in &changes.changes {
            if let DocChange::TreeSpliced { removed, .. } = c {
                transient.extend(removed.iter().copied());
            }
        }
        for c in &changes.changes {
            let DocChange::TreeSpliced {
                parent,
                before,
                inserted,
                ..
            } = c
            else {
                continue;
            };
            if let Some(b) = before
                && !doc.arena.get(*b).is_some_and(|n| n.parent == Some(*parent))
            {
                return false;
            }
            if inserted
                .iter()
                .any(|n| doc.arena.get(*n).is_none() && !transient.contains(n))
            {
                return false;
            }
        }
        true
    }

    pub fn apply_changes(&mut self, doc: &Document, changes: &ChangeSet) -> DeferredSettlement {
        let mut out = DeferredSettlement::default();
        if changes.is_empty() {
            return out;
        }
        if changes.is_replace() || !self.splices_replayable(doc, changes) {
            let tree = Rc::make_mut(&mut self.tree);
            let _ = sync_layout(tree, doc, changes, &self.layout);
            self.store.clear();
            self.bump_doc_rebuilds();
            self.clear_table_cons();
            self.flatten_spine();
            out.structural_full_clear = true;
            return out;
        }

        let mut drop_ids: Vec<LayoutBoxId> = Vec::new();
        let mut splices: Vec<PendingSpineSplice> = Vec::new();
        let mut cold_parents: Vec<NodeId> = Vec::new();
        if changes.is_structural() {
            for c in &changes.changes {
                let DocChange::TreeSpliced {
                    parent,
                    before,
                    removed,
                    inserted,
                } = c
                else {
                    continue;
                };
                let Some(parent_kind) = doc.arena.get(*parent).map(|n| n.kind) else {
                    for r in removed {
                        self.collect_removed_islands(r.index, &mut drop_ids);
                    }
                    continue;
                };
                let parent_box = LayoutBoxId::for_kind(parent_kind, parent.index);
                if self.box_id_in_tree(parent.index).is_none() {
                    cold_parents.push(*parent);
                }
                let before_box = before.and_then(|n| {
                    doc.arena
                        .get(n)
                        .map(|node| LayoutBoxId::for_kind(node.kind, n.index))
                });
                let mut removed_boxes = Vec::new();
                for r in removed {
                    if let Some(id) = self.collect_removed_islands(r.index, &mut drop_ids) {
                        removed_boxes.push(id);
                    }
                }
                let inserted_boxes: Vec<LayoutBoxId> = inserted
                    .iter()
                    .filter_map(|n| {
                        doc.arena
                            .get(*n)
                            .map(|node| LayoutBoxId::for_kind(node.kind, n.index))
                    })
                    .collect();
                splices.push(PendingSpineSplice {
                    parent: parent_box,
                    before: before_box,
                    removed: removed_boxes,
                    inserted: inserted_boxes,
                });
            }
        }

        let tree = Rc::make_mut(&mut self.tree);
        let _ = sync_layout(tree, doc, changes, &self.layout);

        let tree = Rc::clone(&self.tree);
        let estimator = self.estimator;
        let store = &self.store;
        let mut dirty_row_islands: Vec<LayoutBoxId> = Vec::new();
        for splice in splices {
            let _ = self.spine.splice_children(
                &tree,
                splice.parent,
                splice.before,
                &splice.removed,
                &splice.inserted,
                &|id, avail| HeightState::Estimated(store.height_px(&tree, estimator, id, avail)),
            );
            if tree
                .nodes()
                .get(&splice.parent)
                .is_some_and(|n| n.kind() == BlockKind::TableRow)
                && !dirty_row_islands.contains(&splice.parent)
            {
                dirty_row_islands.push(splice.parent);
            }
        }
        for parent in cold_parents {
            if self.box_id_in_tree(parent.index).is_none() {
                self.refresh_deferred_ancestors(doc, parent);
            }
        }
        for id in drop_ids {
            self.store.drop_box(id);
        }

        for row in dirty_row_islands {
            self.invalidate_one_island(row, &mut out);
        }
        for c in &changes.changes {
            match c {
                DocChange::TextChanged { node, .. } => {
                    let islands = self.islands_of_block(node.index);
                    if islands.is_empty() {
                        self.refresh_deferred_ancestors(doc, *node);
                        continue;
                    }
                    for island in islands {
                        self.invalidate_one_island(island, &mut out);
                    }
                    self.refresh_collapsed_ancestors(node.index);
                }
                DocChange::AttrsChanged { node, .. } => {
                    let root = match doc.arena.get(*node) {
                        Some(n) if n.kind == BlockKind::List => *node,
                        Some(n) if n.kind == BlockKind::ListItem => n
                            .parent
                            .filter(|&p| {
                                doc.arena
                                    .get(p)
                                    .is_some_and(|pn| pn.kind == BlockKind::List)
                            })
                            .unwrap_or(*node),
                        Some(_) => *node,
                        None => {
                            continue;
                        }
                    };
                    let Some(id) = self.box_id_in_tree(root.index) else {
                        continue;
                    };
                    let mut ids = Vec::new();
                    self.collect_islands(id, &mut ids);
                    for island in ids {
                        self.invalidate_one_island(island, &mut out);
                    }
                    let tree = Rc::clone(&self.tree);
                    self.spine.refresh_gaps_of(&tree, id);
                }
                _ => {}
            }
        }
        if !changes.is_text_only() {
            self.clear_table_cons();
        }
        out
    }

    fn invalidate_one_island(&mut self, island: LayoutBoxId, out: &mut DeferredSettlement) {
        self.store.invalidate(island);
        let h = self.estimate_island(island);
        self.set_content_height(island, HeightState::Estimated(h));
        if !out.invalidated.contains(&island) {
            out.invalidated.push(island);
        }
    }

    fn box_id_in_tree(&self, index: u32) -> Option<LayoutBoxId> {
        let frame = LayoutBoxId::frame(index);
        if self.tree.nodes().contains_key(&frame) {
            return Some(frame);
        }
        let cell = LayoutBoxId {
            owner: md_layout::box_tree::BoxOwner::Block(index),
            role: md_layout::box_tree::BoxRole::Cell,
            local_key: 0,
        };
        if self.tree.nodes().contains_key(&cell) {
            Some(cell)
        } else {
            None
        }
    }

    fn collect_removed_islands(
        &self,
        index: u32,
        out: &mut Vec<LayoutBoxId>,
    ) -> Option<LayoutBoxId> {
        let main = self.box_id_in_tree(index).or_else(|| {
            let frame = LayoutBoxId::frame(index);
            (self.spine.content_id(frame).is_some() || self.spine.collapsed_id(frame).is_some())
                .then_some(frame)
        });
        if let Some(id) = main {
            self.collect_islands(id, out);
        }
        let preview = LayoutBoxId::preview(index);
        if self.tree.nodes().contains_key(&preview) {
            out.push(preview);
        }
        main
    }

    pub(super) fn collect_islands(&self, id: LayoutBoxId, out: &mut Vec<LayoutBoxId>) {
        let Some(n) = self.tree.nodes().get(&id) else {
            return;
        };
        match n.children() {
            BoxChildren::Island(_) | BoxChildren::None => {
                if !out.contains(&id) {
                    out.push(id);
                }
            }
            BoxChildren::Vertical(c) => {
                for &child in c {
                    self.collect_islands(child, out);
                }
            }
        }
    }

    pub(super) fn island_box_of_block(
        &self,
        block: md_core::block::BlockId,
    ) -> Option<LayoutBoxId> {
        use md_layout::box_tree::{BoxOwner, BoxRole};
        let frame = LayoutBoxId::frame(block);
        if let Some(n) = self.tree.nodes().get(&frame) {
            return if n.kind().is_vertical_container() {
                None
            } else {
                Some(frame)
            };
        }
        let cell = LayoutBoxId {
            owner: BoxOwner::Block(block),
            role: BoxRole::Cell,
            local_key: 0,
        };
        self.tree.nodes().get(&cell).and_then(|n| n.parent())
    }

    pub fn sync_block_edit(&mut self, doc: &Document) -> bool {
        let next = doc.block_edit();
        let prev = self.block_edit;
        if prev == next {
            return false;
        }
        self.block_edit = next;

        let tree = Rc::make_mut(&mut self.tree);
        let splices = sync_block_edit(tree, doc, &self.layout, prev, next);
        if splices.is_empty() {
            return false;
        }

        let tree = Rc::clone(&self.tree);
        let estimator = self.estimator;
        let store = &self.store;
        for s in &splices {
            let _ = self.spine.splice_children(
                &tree,
                s.parent,
                s.before,
                &s.removed,
                &s.inserted,
                &|id, avail| HeightState::Estimated(store.height_px(&tree, estimator, id, avail)),
            );
        }
        for s in &splices {
            for id in &s.removed {
                self.store.drop_box(*id);
            }
        }

        for block in [prev, next].into_iter().flatten() {
            for island in self.islands_of_block(block) {
                self.reset_island_height(island);
            }
        }
        self.clear_table_cons();
        true
    }

    fn reset_island_height(&mut self, island: LayoutBoxId) {
        self.store.drop_box(island);
        let h = self.estimate_island(island);
        self.set_content_height(island, HeightState::Estimated(h));
    }

    pub fn sync_block_edit_retain_y(
        &mut self,
        doc: &Document,
        block: md_core::block::BlockId,
        scroll: Px,
        measure: &dyn TextMeasure,
        solver: &dyn IslandSolver,
    ) -> Px {
        let prev = self.block_edit;
        let next = doc.block_edit();
        if prev == next {
            return scroll;
        }
        let was_on_spine = self.ensure_composed_block(doc, block, measure, solver);
        let hold = was_on_spine
            .then(|| self.content_top(block))
            .flatten()
            .map(|y| y - scroll);
        if !self.sync_block_edit(doc) {
            return scroll;
        }
        for id in [prev, next].into_iter().flatten() {
            let _ = self.ensure_composed_block(doc, id, measure, solver);
        }
        if !self.ensure_composed_block(doc, block, measure, solver) {
            return scroll;
        }
        match (hold, self.content_top(block)) {
            (Some(hold), Some(y)) => (y - hold).max(0.0),
            _ => scroll,
        }
    }

    pub(super) fn islands_of_block(&self, block: md_core::block::BlockId) -> Vec<LayoutBoxId> {
        let mut out = Vec::new();
        if let Some(main) = self.island_box_of_block(block) {
            out.push(main);
        }
        let preview = LayoutBoxId::preview(block);
        if self.tree.nodes().contains_key(&preview) {
            out.push(preview);
        }
        out
    }

    pub fn invalidate_island_for_block(&mut self, block: md_core::block::BlockId) {
        let islands = self.islands_of_block(block);
        if islands.is_empty() {
            return;
        }
        for island in islands {
            self.store.invalidate(island);
            let h = self.estimate_island(island);
            self.set_content_height(island, HeightState::Estimated(h));
        }
    }

    pub(super) fn refresh_collapsed_ancestors(&mut self, block: md_core::block::BlockId) {
        let Some(mut id) = self.island_box_of_block(block) else {
            return;
        };
        while let Some(parent) = self.tree.get(id).parent() {
            id = parent;
            let tree = Rc::clone(&self.tree);
            let estimator = self.estimator;
            let store = &self.store;
            self.spine
                .refresh_collapsed_height(&tree, id, self.viewport_width(), &|bid, avail| {
                    HeightState::Estimated(store.height_px(&tree, estimator, bid, avail))
                });
        }
    }

    pub(super) fn refresh_deferred_ancestors(&mut self, doc: &Document, node: NodeId) {
        let mut cur = Some(node);
        while let Some(id) = cur {
            let Some((deferred_box, deferred_node)) =
                md_layout::compose::deferred_ancestor_of(&self.tree, doc, id)
            else {
                return;
            };
            let tree = Rc::clone(&self.tree);
            self.spine.refresh_deferred_height(&tree, deferred_box);
            cur = doc
                .arena
                .get(deferred_node)
                .and_then(|n| n.parent)
                .filter(|p| *p != deferred_node);
        }
    }
}
