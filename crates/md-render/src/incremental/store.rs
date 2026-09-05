use super::estimate::Estimator;
use md_core::Px;
use md_layout::box_tree::{BoxChildren, BoxNode, BoxTree, LayoutBoxId};
use md_layout::island::{IslandGeometry, IslandSolver, IslandStats, TableColumnConstraintSet};
use md_layout::shaper::TextMeasure;
use std::collections::BTreeMap;

fn geometry_fresh(g: &IslandGeometry, n: &BoxNode) -> bool {
    if g.content_generation != n.content_generation() || g.content_revision != n.content_revision()
    {
        return false;
    }
    match n.children() {
        BoxChildren::Island(ids) => {
            g.cells.len() == ids.len()
                && g.cells
                    .iter()
                    .zip(ids.iter())
                    .all(|(c, id)| c.cell_box == *id)
        }
        _ => true,
    }
}

#[derive(Clone, Copy)]
pub(super) struct SolveRequest<'a> {
    pub(super) tree: &'a BoxTree,
    pub(super) id: LayoutBoxId,
    pub(super) avail: Px,
    pub(super) measure: &'a dyn TextMeasure,
    pub(super) cons: Option<&'a TableColumnConstraintSet>,
    pub(super) solver: &'a dyn IslandSolver,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EvictionPolicy {
    Unbounded,

    Windowed { keep_screens: f64 },
}

impl Default for EvictionPolicy {
    fn default() -> Self {
        EvictionPolicy::windowed(2.0)
    }
}

impl EvictionPolicy {
    pub fn unbounded() -> Self {
        EvictionPolicy::Unbounded
    }

    pub fn windowed(keep_screens: f64) -> Self {
        EvictionPolicy::Windowed {
            keep_screens: keep_screens.max(0.0),
        }
    }
}

#[derive(Default)]
pub struct MaterializedStore {
    exact: BTreeMap<LayoutBoxId, IslandGeometry>,
    exact_epochs: BTreeMap<LayoutBoxId, u64>,

    last_exact: BTreeMap<LayoutBoxId, Px>,

    pub(super) last_used: BTreeMap<LayoutBoxId, u64>,
    clock: u64,

    pub solve_calls: u64,

    viewport_epoch: u64,
}

impl MaterializedStore {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub(crate) fn is_materialized(&self, id: LayoutBoxId) -> bool {
        self.exact.contains_key(&id)
    }

    pub(crate) fn is_fresh(&self, tree: &BoxTree, id: LayoutBoxId) -> bool {
        let Some(g) = self.exact.get(&id) else {
            return false;
        };
        let Some(n) = tree.nodes().get(&id) else {
            return false;
        };
        self.exact_epochs.get(&id) == Some(&self.viewport_epoch) && geometry_fresh(g, n)
    }

    pub fn get(&self, id: LayoutBoxId) -> Option<&IslandGeometry> {
        self.exact.get(&id)
    }

    pub(crate) fn materialized_count(&self) -> usize {
        self.exact.len()
    }

    pub(crate) fn last_exact_count(&self) -> usize {
        self.last_exact.len()
    }

    #[cfg(test)]
    pub(crate) fn last_exact_height(&self, id: LayoutBoxId) -> Option<Px> {
        self.last_exact.get(&id).copied()
    }

    pub(super) fn height_px(
        &self,
        tree: &BoxTree,
        estimator: Estimator,
        id: LayoutBoxId,
        avail: Px,
    ) -> Px {
        if let Some(g) = self.exact.get(&id) {
            return g.border_box_height;
        }
        if let Some(h) = self.last_exact.get(&id) {
            return *h;
        }
        estimator.estimate(tree, id, avail)
    }

    pub(super) fn touch(&mut self, id: LayoutBoxId) {
        if self.exact.contains_key(&id) {
            self.clock += 1;
            let c = self.clock;
            self.last_used.insert(id, c);
        }
    }

    pub(super) fn materialize(&mut self, req: SolveRequest<'_>, stats: &mut IslandStats) {
        let SolveRequest {
            tree,
            id,
            avail,
            measure,
            cons,
            solver,
        } = req;
        if let Some(g) = self.exact.get(&id) {
            let n = tree.get(id);
            if self.exact_epochs.get(&id) == Some(&self.viewport_epoch) && geometry_fresh(g, n) {
                return;
            }
            let h = g.border_box_height;
            self.exact.remove(&id);
            self.exact_epochs.remove(&id);
            self.last_exact.insert(id, h);
        }
        let g = solver.solve(tree, id, avail, measure, cons, stats);
        self.solve_calls += 1;

        self.last_exact.remove(&id);
        self.clock += 1;
        self.last_used.insert(id, self.clock);
        self.exact.insert(id, g);
        self.exact_epochs.insert(id, self.viewport_epoch);
    }

    pub(super) fn evict(&mut self, id: LayoutBoxId) -> bool {
        match self.exact.remove(&id) {
            Some(g) => {
                self.exact_epochs.remove(&id);
                self.last_exact.insert(id, g.border_box_height);
                self.last_used.remove(&id);
                true
            }
            None => false,
        }
    }

    pub(crate) fn set_viewport_epoch(&mut self, epoch: u64) {
        self.viewport_epoch = epoch;
    }

    pub fn clear(&mut self) {
        md_layout::hot_path::add_store_clear();
        self.exact.clear();
        self.exact_epochs.clear();
        self.last_exact.clear();
        self.last_used.clear();
    }

    pub(crate) fn drop_box(&mut self, id: LayoutBoxId) -> bool {
        let had = self.exact.remove(&id).is_some();
        self.exact_epochs.remove(&id);
        self.last_exact.remove(&id);
        self.last_used.remove(&id);
        had
    }

    pub(crate) fn invalidate(&mut self, id: LayoutBoxId) -> bool {
        self.evict(id)
    }
}
