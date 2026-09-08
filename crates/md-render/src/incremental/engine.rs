use super::estimate::Estimator;
use super::store::{EvictionPolicy, MaterializedStore};
use crate::frame::caret::caret_in_box;
use md_content::shaper::GpuiShaper;
use md_core::Px;
use md_core::block::{BlockId, BlockKind};
use md_core::doc::Cursor;
use md_core::document::Document;
use md_layout::assembly::first_row_col_count;
use md_layout::box_tree::{BoxChildren, BoxTree, LayoutBoxId};
use md_layout::compose::{ComposeWindow, LayoutTheme, compose, compose_window};
use md_layout::flow::HeightState;
use md_layout::island::{IslandStats, TableColumnConstraintSet, table_track_overrides};
use md_layout::spine::FlowSpine;
use md_layout::style::BoxLayoutEnvironment;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct PublishedFrame {
    pub iterations: u32,

    pub resolved_top: Px,
}

pub const MAX_ITERATIONS: u32 = 64;

#[derive(Clone, Debug, Default)]
pub struct DeferredSettlement {
    pub invalidated: Vec<LayoutBoxId>,

    pub structural_full_clear: bool,
}

pub struct IncrementalEngine {
    pub(crate) tree: Rc<BoxTree>,
    env: BoxLayoutEnvironment,
    pub(crate) table_cons: Rc<BTreeMap<LayoutBoxId, TableColumnConstraintSet>>,
    pub(crate) store: MaterializedStore,
    pub(crate) estimator: Estimator,
    pub(crate) island_stats: IslandStats,

    viewport_params_gen: u64,

    doc_rebuilds: u64,

    pub(crate) eviction: EvictionPolicy,

    pub(crate) pins: BTreeSet<LayoutBoxId>,

    pub(crate) lazy: bool,

    pub(crate) flatten_gens: u64,

    publish_gen: u64,

    pub(super) layout: LayoutTheme,
    pub(super) spine: FlowSpine,

    pub(super) block_edit: Option<md_core::block::BlockId>,

    table_col_tracks: HashMap<BlockId, Vec<Px>>,
}

pub(super) struct PendingSpineSplice {
    pub(super) parent: LayoutBoxId,
    pub(super) before: Option<LayoutBoxId>,
    pub(super) removed: Vec<LayoutBoxId>,
    pub(super) inserted: Vec<LayoutBoxId>,
}

impl IncrementalEngine {
    pub fn new(
        doc: &Document,
        env: BoxLayoutEnvironment,
        estimator: Estimator,
        layout: LayoutTheme,
    ) -> Self {
        let t = std::time::Instant::now();
        let tree = compose(doc, &layout);
        Self::from_composed(doc, env, estimator, layout, tree, t.elapsed(), false)
    }

    pub fn with_window(
        doc: &Document,
        env: BoxLayoutEnvironment,
        estimator: Estimator,
        layout: LayoutTheme,
        top: Px,
        bottom: Px,
    ) -> Self {
        let t = std::time::Instant::now();
        let tree = compose_window(
            doc,
            &layout,
            ComposeWindow {
                top,
                bottom,
                avail_width: env.viewport_width,
            },
            &estimator.leaf_metrics(),
        );
        Self::from_composed(doc, env, estimator, layout, tree, t.elapsed(), true)
    }

    fn from_composed(
        doc: &Document,
        env: BoxLayoutEnvironment,
        estimator: Estimator,
        layout: LayoutTheme,
        tree: md_layout::box_tree::BoxTree,
        compose_ms: std::time::Duration,
        lazy: bool,
    ) -> Self {
        let tree = Rc::new(tree);

        let t_aux = std::time::Instant::now();
        let table_col_tracks = HashMap::new();
        let mut table_cons = Rc::new(BTreeMap::new());
        for (id, node) in tree.nodes() {
            if node.kind() == BlockKind::Table {
                Rc::make_mut(&mut table_cons).insert(
                    *id,
                    resolve_table_cons(&tree, *id, env.viewport_width, &table_col_tracks),
                );
            }
        }

        let all_islands = tree.island_boxes();
        let islands_for_trace = all_islands
            .iter()
            .filter(|id| tree.get(**id).kind() != BlockKind::TableCell)
            .count();

        let env_w = env.viewport_width;
        let aux_ms = t_aux.elapsed();
        let t_flat = std::time::Instant::now();
        let spine = FlowSpine::flatten(&tree, env_w, &|id, avail| {
            HeightState::Estimated(if tree.nodes().get(&id).is_some() {
                estimator.estimate(&tree, id, avail)
            } else {
                tree.deferred_height(id)
                    .unwrap_or_else(|| panic!("flatten missing box {id:?}"))
            })
        });
        let intern_n = if tree.intern().is_empty() {
            0
        } else {
            tree.intern().len()
        };
        crate::cold_trace::log(&format!(
            "cold engine compose={:.1}ms aux={:.1}ms flatten={:.1}ms nodes={} intern={} spine={} islands={}",
            compose_ms.as_secs_f64() * 1000.0,
            aux_ms.as_secs_f64() * 1000.0,
            t_flat.elapsed().as_secs_f64() * 1000.0,
            tree.nodes().len(),
            intern_n,
            spine.len(),
            islands_for_trace,
        ));

        IncrementalEngine {
            tree,
            env,
            table_cons,
            store: MaterializedStore::new(),
            estimator,
            island_stats: IslandStats::default(),
            viewport_params_gen: 0,
            doc_rebuilds: 0,
            eviction: EvictionPolicy::windowed(2.0),
            pins: BTreeSet::new(),
            lazy,
            flatten_gens: 1,
            publish_gen: 0,
            layout,
            spine,
            block_edit: doc.block_edit(),
            table_col_tracks,
        }
    }

    pub(super) fn clear_table_cons(&mut self) {
        self.table_cons = Rc::new(BTreeMap::new());
    }

    pub(super) fn flatten_spine(&mut self) {
        let env_w = self.env.viewport_width;
        let tree = Rc::clone(&self.tree);
        let estimator = self.estimator;
        let store = &self.store;
        self.spine = FlowSpine::flatten(&tree, env_w, &|id, avail| {
            HeightState::Estimated(if tree.nodes().get(&id).is_some() {
                store.height_px(&tree, estimator, id, avail)
            } else {
                tree.deferred_height(id)
                    .unwrap_or_else(|| panic!("flatten missing box {id:?}"))
            })
        });
        self.flatten_gens += 1;
    }

    pub(super) fn estimate_island(&self, id: LayoutBoxId) -> Px {
        let avail = self.tree.avail_width(id, self.env.viewport_width);
        self.store.height_px(&self.tree, self.estimator, id, avail)
    }

    pub(super) fn set_content_height(&mut self, box_id: LayoutBoxId, height: HeightState) {
        if let Some(id) = self.spine.content_id(box_id) {
            self.spine.set_height(id, height);
        }
    }

    pub fn set_table_col_tracks(&mut self, tracks: &HashMap<BlockId, Vec<Px>>) {
        if &self.table_col_tracks == tracks {
            return;
        }
        let mut dirty: HashSet<BlockId> = self.table_col_tracks.keys().copied().collect();
        for (k, v) in tracks {
            if self.table_col_tracks.get(k) == Some(v) {
                dirty.remove(k);
            } else {
                dirty.insert(*k);
            }
        }
        self.table_col_tracks.clone_from(tracks);
        for table in dirty {
            self.invalidate_table_col_tracks(table);
        }
    }

    pub(crate) fn resolve_cons(&self, table: LayoutBoxId) -> TableColumnConstraintSet {
        resolve_table_cons(
            &self.tree,
            table,
            self.viewport_width(),
            &self.table_col_tracks,
        )
    }

    fn invalidate_table_col_tracks(&mut self, table: BlockId) {
        let tid = LayoutBoxId::frame(table);
        Rc::make_mut(&mut self.table_cons).remove(&tid);
        let Some(node) = self.tree.nodes().get(&tid) else {
            return;
        };
        let BoxChildren::Vertical(rows) = node.children() else {
            return;
        };
        let rows = rows.clone();
        for row in rows {
            if !self.tree.nodes().contains_key(&row) {
                continue;
            }
            self.store.invalidate(row);
            let h = self.estimate_island(row);
            self.set_content_height(row, HeightState::Estimated(h));
        }
    }

    pub fn set_viewport_width(&mut self, w: Px) {
        if (self.env.viewport_width - w).abs() < f64::EPSILON {
            return;
        }
        self.env.viewport_width = w;
        self.viewport_params_gen += 1;
        self.store.set_viewport_epoch(self.viewport_params_gen);
        self.spine.set_viewport_width(w);
        self.clear_table_cons();
    }

    pub fn layout_theme(&self) -> &LayoutTheme {
        &self.layout
    }

    pub fn viewport_width(&self) -> Px {
        self.env.viewport_width
    }

    pub fn total_height(&self) -> Px {
        self.spine.total_height()
    }

    pub fn materialized_count(&self) -> usize {
        self.store.materialized_count()
    }

    pub fn last_exact_count(&self) -> usize {
        self.store.last_exact_count()
    }

    pub fn publish_gen(&self) -> u64 {
        self.publish_gen
    }

    pub fn viewport_params_gen(&self) -> u64 {
        self.viewport_params_gen
    }

    pub fn doc_rebuilds(&self) -> u64 {
        self.doc_rebuilds
    }

    pub(super) fn bump_publish_gen(&mut self) {
        self.publish_gen = self.publish_gen.saturating_add(1);
    }

    pub(super) fn bump_doc_rebuilds(&mut self) {
        self.doc_rebuilds += 1;
    }

    pub fn content_top(&self, block: md_core::block::BlockId) -> Option<Px> {
        let island = self.island_box_of_block(block)?;
        let fid = self.spine.content_id(island)?;
        self.spine.item_top(fid)
    }

    pub fn heading_at_or_above(
        &self,
        doc: &Document,
        heading_blocks: &[BlockId],
        top: Px,
    ) -> Option<BlockId> {
        let at_or_above =
            |block: &BlockId| self.block_flow_top(doc, *block).is_some_and(|y| y <= top);
        let ix = heading_blocks.partition_point(at_or_above);
        (ix > 0).then(|| heading_blocks[ix - 1])
    }

    fn block_flow_top(&self, doc: &Document, block: BlockId) -> Option<Px> {
        if let Some(y) = self.spine_top(LayoutBoxId::frame(block)) {
            return Some(y);
        }
        let mut node = doc.live_id(block)?;
        while let Some(parent) = doc.arena.get(node).and_then(|n| n.parent) {
            if let Some(y) = self.spine_top(LayoutBoxId::frame(parent.index)) {
                return Some(y);
            }
            node = parent;
        }
        None
    }

    fn spine_top(&self, island: LayoutBoxId) -> Option<Px> {
        let item = self
            .spine
            .content_id(island)
            .or_else(|| self.spine.collapsed_id(island))?;
        self.spine.item_top(item)
    }

    pub fn spine_caret_y(
        &self,
        cursor: Cursor,
        shaper: &GpuiShaper,
        env: BoxLayoutEnvironment,
    ) -> Option<Px> {
        let top = self.content_top(cursor.block)?;
        let leaf = LayoutBoxId::frame(cursor.block);
        let node = self.tree.nodes().get(&leaf)?;
        let avail = self.tree.avail_width(leaf, env.viewport_width);
        let inner = (avail - self.tree.style_of(node).inline_border_padding()).max(0.0);
        let (_, y, _) = caret_in_box(&self.tree, node, shaper, top, inner, cursor.offset);
        Some(y)
    }

    #[cfg(test)]
    pub(super) fn debug_block_on_spine(&self, block: md_core::block::BlockId) -> bool {
        self.island_box_of_block(block)
            .is_some_and(|id| self.spine.content_id(id).is_some())
    }

    #[cfg(test)]
    pub(super) fn debug_block_top(&self, block: md_core::block::BlockId) -> Option<Px> {
        self.content_top(block)
    }
}

fn resolve_table_cons(
    tree: &BoxTree,
    table: LayoutBoxId,
    viewport_width: Px,
    tracks: &HashMap<BlockId, Vec<Px>>,
) -> TableColumnConstraintSet {
    let col_count = first_row_col_count(tree, table);
    let available = tree.content_width(table, viewport_width);
    TableColumnConstraintSet::resolve_with(
        table,
        col_count,
        available,
        table_track_overrides(table, Some(tracks)),
    )
}
