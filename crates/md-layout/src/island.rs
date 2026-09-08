use crate::box_tree::{BoxChildren, BoxOwner, BoxTree, LayoutBoxId};
use crate::shaper::{MeasureKind, TextMeasure};
use md_core::Px;
use md_core::block::BlockId;
use std::collections::HashMap;

pub const TABLE_MIN_COL_WIDTH: Px = 48.0;

#[derive(Clone, Debug, PartialEq)]
pub struct TableColumnConstraintSet {
    pub table: LayoutBoxId,
    pub tracks: Vec<Px>,
    pub available_inline_size: Px,
}

impl TableColumnConstraintSet {
    pub fn resolve(table: LayoutBoxId, col_count: usize, available: Px) -> Self {
        Self::resolve_with(table, col_count, available, None)
    }

    pub fn resolve_with(
        table: LayoutBoxId,
        col_count: usize,
        available: Px,
        overrides: Option<&[Px]>,
    ) -> Self {
        let n = col_count.max(1);
        let available = if available.is_finite() && available >= 0.0 {
            available
        } else {
            0.0
        };
        let equal = available / n as Px;
        let tracks = match overrides {
            Some(over) if !over.is_empty() => mix_override_tracks(n, available, equal, over),
            _ => vec![equal; n],
        };
        TableColumnConstraintSet {
            table,
            tracks,
            available_inline_size: available,
        }
    }
}

pub fn table_track_overrides(
    table: LayoutBoxId,
    map: Option<&HashMap<BlockId, Vec<Px>>>,
) -> Option<&[Px]> {
    match table.owner {
        BoxOwner::Block(b) => map.and_then(|m| m.get(&b)).map(Vec::as_slice),
        BoxOwner::DocStart => None,
    }
}

fn mix_override_tracks(n: usize, available: Px, equal: Px, over: &[Px]) -> Vec<Px> {
    let mut tracks: Vec<Px> = (0..n)
        .map(|i| {
            let w = over.get(i).copied().unwrap_or(equal);
            let w = if w.is_finite() && w > 0.0 { w } else { equal };
            w.max(TABLE_MIN_COL_WIDTH)
        })
        .collect();
    let floor = n as Px * TABLE_MIN_COL_WIDTH;
    if available <= 0.0 || floor > available {
        return tracks;
    }
    let sum: Px = tracks.iter().sum();
    if sum <= 0.0 {
        return vec![equal.max(TABLE_MIN_COL_WIDTH); n];
    }
    if (sum - available).abs() < 0.01 {
        return tracks;
    }
    let scale = available / sum;
    for t in &mut tracks {
        *t *= scale;
    }
    let mut deficit = 0.0;
    for t in &mut tracks {
        if *t < TABLE_MIN_COL_WIDTH {
            deficit += TABLE_MIN_COL_WIDTH - *t;
            *t = TABLE_MIN_COL_WIDTH;
        }
    }
    if deficit > 0.0 {
        let extra: Px = tracks
            .iter()
            .map(|t| (*t - TABLE_MIN_COL_WIDTH).max(0.0))
            .sum();
        if extra > 0.0 {
            for t in &mut tracks {
                let slack = (*t - TABLE_MIN_COL_WIDTH).max(0.0);
                if slack > 0.0 {
                    *t -= deficit * (slack / extra);
                }
            }
        }
    }
    tracks
}

#[derive(Clone, Debug, PartialEq)]

pub struct CellGeometry {
    pub cell_box: LayoutBoxId,
    pub x: Px,
    pub width: Px,

    pub border_box_height: Px,

    pub row_height_vote: Px,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IslandGeometry {
    pub box_id: LayoutBoxId,

    pub border_box_height: Px,
    pub content_height: Px,
    pub first_baseline: Option<Px>,

    pub cells: Vec<CellGeometry>,
    pub content_generation: u32,
    pub content_revision: u64,
    pub avail_width: Px,
}

#[derive(Debug, Default)]
pub struct IslandStats {
    pub islands_built: u64,
}

fn solve_island(
    tree: &BoxTree,
    id: LayoutBoxId,
    avail_width: Px,
    shaper: &dyn TextMeasure,
    table_constraints: Option<&TableColumnConstraintSet>,
    stats: &mut IslandStats,
) -> IslandGeometry {
    stats.islands_built += 1;
    shaper.begin_island();

    let node = tree.get(id);
    let style = tree.style_of(node);
    let own_bp_block = style.top_border_padding() + style.bottom_border_padding();
    let own_bp_inline = style.inline_border_padding();

    match &node.children {
        BoxChildren::Island(cell_ids) => {
            let fallback;
            let cons = match table_constraints {
                Some(cons) => cons,
                None => {
                    fallback = TableColumnConstraintSet::resolve(id, cell_ids.len(), avail_width);
                    &fallback
                }
            };
            let fallback;
            let cons = if cons.tracks.len() == cell_ids.len() {
                cons
            } else {
                fallback = TableColumnConstraintSet::resolve(
                    cons.table,
                    cell_ids.len(),
                    cons.available_inline_size,
                );
                &fallback
            };

            let mut votes = Vec::new();
            let mut x = 0.0;
            let mut max_h: Px = 0.0;
            for (i, cid) in cell_ids.iter().enumerate() {
                let cell = tree.get(*cid);
                let track_w = cons.tracks[i];
                let cell_style = tree.style_of(cell);
                let inner_w = (track_w - cell_style.inline_border_padding()).max(0.0);

                let m = shaper.measure(
                    tree.text_of(cell),
                    tree.runs_of(cell),
                    inner_w,
                    MeasureKind::Final,
                    cell.kind,
                    cell.shape_ident(),
                );

                let vote =
                    m.height + cell_style.top_border_padding() + cell_style.bottom_border_padding();
                votes.push((*cid, x, track_w, vote));
                if vote > max_h {
                    max_h = vote;
                }
                x += track_w;
            }

            let cells = votes
                .into_iter()
                .map(|(cell_box, x, width, vote)| CellGeometry {
                    cell_box,
                    x,
                    width,
                    border_box_height: max_h,
                    row_height_vote: vote,
                })
                .collect();
            IslandGeometry {
                box_id: id,
                border_box_height: max_h + own_bp_block,
                content_height: max_h,
                first_baseline: None,
                cells,
                content_generation: node.content_generation,
                content_revision: node.content_revision,
                avail_width,
            }
        }
        BoxChildren::None => {
            debug_assert!(
                table_constraints.is_none(),
                "table constraints are for row islands; a flow leaf has none"
            );
            let inner_w = (avail_width - own_bp_inline).max(0.0);
            let m = shaper.measure(
                tree.text_of(node),
                tree.runs_of(node),
                inner_w,
                MeasureKind::Final,
                node.kind,
                node.shape_ident(),
            );
            IslandGeometry {
                box_id: id,
                border_box_height: m.height + own_bp_block,
                content_height: m.height,
                first_baseline: Some(m.first_baseline + style.top_border_padding()),
                cells: Vec::new(),
                content_generation: node.content_generation,
                content_revision: node.content_revision,
                avail_width,
            }
        }
        BoxChildren::Vertical(_) => {
            panic!(
                "vertical FlowStack {id:?} must be lowered onto the flow spine, never submitted to an island \
                 (design §3.3)"
            );
        }
    }
}

pub trait IslandSolver {
    fn solve(
        &self,
        tree: &BoxTree,
        id: LayoutBoxId,
        avail_width: Px,
        shaper: &dyn TextMeasure,
        table_constraints: Option<&TableColumnConstraintSet>,
        stats: &mut IslandStats,
    ) -> IslandGeometry;
}

pub struct FallbackSolver;

impl IslandSolver for FallbackSolver {
    fn solve(
        &self,
        tree: &BoxTree,
        id: LayoutBoxId,
        avail_width: Px,
        shaper: &dyn TextMeasure,
        table_constraints: Option<&TableColumnConstraintSet>,
        stats: &mut IslandStats,
    ) -> IslandGeometry {
        solve_island(tree, id, avail_width, shaper, table_constraints, stats)
    }
}
