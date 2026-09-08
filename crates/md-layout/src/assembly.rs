use crate::box_tree::{BoxTree, LayoutBoxId};
use crate::flow::HeightState;
use crate::island::{
    IslandGeometry, IslandSolver, IslandStats, TableColumnConstraintSet, table_track_overrides,
};
use crate::spine::{FlowSpine, FlowWindow};
use crate::style::BoxLayoutEnvironment;
use md_core::Px;
use md_core::block::{BlockId, BlockKind};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

pub fn first_row_col_count(tree: &BoxTree, table: LayoutBoxId) -> usize {
    use crate::box_tree::BoxChildren;
    if let BoxChildren::Vertical(rows) = &tree.get(table).children {
        for r in rows {
            let Some(node) = tree.nodes().get(r) else {
                continue;
            };
            if let BoxChildren::Island(cells) = &node.children {
                return cells.len();
            }
        }
    }
    1
}

pub struct Assembly {
    pub tree: Rc<BoxTree>,
    pub table_cons: Rc<BTreeMap<LayoutBoxId, TableColumnConstraintSet>>,
    pub heights: BTreeMap<LayoutBoxId, HeightState>,

    pub geometries: BTreeMap<LayoutBoxId, IslandGeometry>,
    pub window: Option<FlowWindow>,
    pub island_stats: IslandStats,
}

pub fn assemble_tree(
    tree: BoxTree,
    env: BoxLayoutEnvironment,
    measure: &dyn crate::shaper::TextMeasure,
    solver: &dyn IslandSolver,
) -> Assembly {
    assemble_shared(Rc::new(tree), env, measure, solver)
}

pub fn assemble_shared(
    tree: Rc<BoxTree>,
    env: BoxLayoutEnvironment,
    measure: &dyn crate::shaper::TextMeasure,
    solver: &dyn IslandSolver,
) -> Assembly {
    assemble_shared_with(tree, env, measure, solver, None)
}

pub fn assemble_shared_with(
    tree: Rc<BoxTree>,
    env: BoxLayoutEnvironment,
    measure: &dyn crate::shaper::TextMeasure,
    solver: &dyn IslandSolver,
    col_tracks: Option<&HashMap<BlockId, Vec<Px>>>,
) -> Assembly {
    let mut table_cons = BTreeMap::new();
    for (id, node) in tree.nodes() {
        if node.kind() == BlockKind::Table {
            let col_count = first_row_col_count(&tree, *id);
            let available = tree.content_width(*id, env.viewport_width);
            table_cons.insert(
                *id,
                TableColumnConstraintSet::resolve_with(
                    *id,
                    col_count,
                    available,
                    table_track_overrides(*id, col_tracks),
                ),
            );
        }
    }

    let mut stats = IslandStats::default();
    let mut heights = BTreeMap::new();
    let mut geometries = BTreeMap::new();
    for id in tree.island_boxes() {
        let avail = tree.avail_width(id, env.viewport_width);
        let cons = tree
            .ancestor_table(id)
            .and_then(|table| table_cons.get(&table));
        let g = solver.solve(&tree, id, avail, measure, cons, &mut stats);
        heights.insert(id, HeightState::Exact(g.border_box_height));
        geometries.insert(id, g);
    }

    let spine = FlowSpine::flatten_complete(&tree, env.viewport_width, &|id, _avail| {
        *heights.get(&id).unwrap_or(&HeightState::Estimated(0.0))
    });
    let total = spine.total_height();
    let window = spine.window(&tree, 0.0, total.max(0.0), &[]);

    Assembly {
        tree,
        table_cons: Rc::new(table_cons),
        heights,
        geometries,
        window: Some(window),
        island_stats: stats,
    }
}
