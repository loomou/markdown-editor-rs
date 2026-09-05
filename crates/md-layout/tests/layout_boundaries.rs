use md_core::Px;
use md_core::block::BlockKind;
use md_core::document::{LeafSnapshot, editor_options, load_markdown};
use md_core::inline::InlineRun;
use md_layout::assembly::assemble_tree;
use md_layout::box_tree::{BoxIntern, BoxOwner, BoxRole, LayoutBoxId};
use md_layout::compose::{LayoutTheme, compose};
use md_layout::island::{FallbackSolver, IslandStats, TableColumnConstraintSet};
use md_layout::shaper::{MeasureKind, MeasureResult, ShapeIdentity, TextMeasure};
use md_layout::style::{BoxDisplay, BoxLayoutStyle, Edges};

fn theme() -> LayoutTheme {
    LayoutTheme::from_resolver(|_| BoxLayoutStyle {
        display: BoxDisplay::FlowStack,
        margin: Edges::all(2.0),
        padding: Edges::vh(3.0, 4.0),
        border: Edges::all(1.0),
        gap: 5.0,
    })
}

#[test]
fn layout_box_ids_assign_roles_and_chrome() {
    assert_eq!(
        LayoutBoxId::for_kind(BlockKind::DocStart, 9),
        LayoutBoxId::doc_start()
    );
    assert_eq!(
        LayoutBoxId::for_kind(BlockKind::TableCell, 9),
        LayoutBoxId {
            owner: BoxOwner::Block(9),
            role: BoxRole::Cell,
            local_key: 0,
        }
    );
    assert_eq!(
        LayoutBoxId::chrome(BlockKind::BlockQuote, 4),
        Some(LayoutBoxId::bar(4))
    );
    assert_eq!(
        LayoutBoxId::chrome(BlockKind::ListItem, 4),
        Some(LayoutBoxId::slot(4))
    );
    assert!(LayoutBoxId::frame(4).is_materializable_role());
    assert!(!LayoutBoxId::bar(4).is_materializable_role());
}

#[test]
fn box_intern_handles_empty_entries_and_replacement() {
    use std::sync::Arc;
    let mut intern = BoxIntern::default();
    assert_eq!(intern.push(None), None);
    assert_eq!(
        intern.push(Some(Arc::new(LeafSnapshot {
            display: String::new(),
            runs: Vec::new(),
        }))),
        None
    );
    let id = intern
        .push(Some(Arc::new(LeafSnapshot {
            display: "old".into(),
            runs: Vec::new(),
        })))
        .expect("non-empty text gets an id");
    assert_eq!(intern.text(Some(id)), "old");
    intern.replace(
        id,
        Some(Arc::new(LeafSnapshot {
            display: "new".into(),
            runs: Vec::new(),
        })),
    );
    assert_eq!(intern.text(Some(id)), "new");
    intern.replace(id, None);
    assert_eq!(intern.text(Some(id)), "");
    assert_eq!(intern.text(Some(99)), "");
    assert!(intern.runs(None).is_empty());
}

#[test]
fn compose_exposes_parent_chain_and_clamped_widths() {
    let doc = load_markdown("> quoted\n\nbody\n", editor_options());
    let tree = compose(&doc, &theme());
    let leaf = tree
        .island_boxes()
        .into_iter()
        .find(|id| tree.get(*id).kind() == BlockKind::Paragraph)
        .expect("paragraph island");
    let chain = tree.ancestor_chain(leaf);
    assert_eq!(chain.first().copied(), Some(tree.root()));
    assert_eq!(chain.last().copied(), Some(leaf));
    assert!(tree.avail_width(leaf, 0.0) >= 0.0);
    assert!(tree.content_width(leaf, 1.0) >= 0.0);
    assert!(!tree.nodes().is_empty());
}

#[test]
fn table_constraints_use_at_least_one_track() {
    let table = LayoutBoxId::frame(7);
    let zero = TableColumnConstraintSet::resolve(table, 0, 120.0);
    assert_eq!(zero.tracks, vec![120.0]);
    let three = TableColumnConstraintSet::resolve(table, 3, 120.0);
    assert_eq!(three.tracks, vec![40.0; 3]);
    assert_eq!(three.available_inline_size, 120.0);
}

#[test]
fn table_constraints_reject_negative_tracks() {
    let table = LayoutBoxId::frame(7);
    let negative = TableColumnConstraintSet::resolve(table, 2, -10.0);
    assert!(negative.tracks.iter().all(|track| *track >= 0.0));
}

#[test]
fn table_constraints_reject_non_finite_tracks() {
    let table = LayoutBoxId::frame(7);
    let nan = TableColumnConstraintSet::resolve(table, 2, Px::NAN);
    assert!(nan.tracks.iter().all(|track| track.is_finite()));
}

#[test]
fn table_constraints_resolve_with_falls_back_to_equal() {
    let table = LayoutBoxId::frame(7);
    let equal = TableColumnConstraintSet::resolve(table, 3, 120.0);
    let none = TableColumnConstraintSet::resolve_with(table, 3, 120.0, None);
    let empty = TableColumnConstraintSet::resolve_with(table, 3, 120.0, Some(&[]));
    assert_eq!(equal.tracks, none.tracks);
    assert_eq!(equal.tracks, empty.tracks);
    assert_eq!(equal.tracks, vec![40.0; 3]);
}

#[test]
fn table_constraints_resolve_with_overrides() {
    let table = LayoutBoxId::frame(7);
    let set = TableColumnConstraintSet::resolve_with(table, 2, 200.0, Some(&[80.0, 120.0]));
    assert_eq!(set.tracks.len(), 2);
    assert!((set.tracks[0] - 80.0).abs() < 1e-6);
    assert!((set.tracks[1] - 120.0).abs() < 1e-6);
    let padded = TableColumnConstraintSet::resolve_with(table, 2, 200.0, Some(&[80.0]));
    assert_eq!(padded.tracks.len(), 2);
    assert!((padded.tracks[0] + padded.tracks[1] - 200.0).abs() < 0.5);
    let truncated =
        TableColumnConstraintSet::resolve_with(table, 2, 200.0, Some(&[90.0, 110.0, 999.0]));
    assert_eq!(truncated.tracks.len(), 2);
    assert!((truncated.tracks[0] - 90.0).abs() < 1e-6);
    assert!((truncated.tracks[1] - 110.0).abs() < 1e-6);
}

#[test]
fn table_constraints_resolve_with_min_width() {
    let table = LayoutBoxId::frame(7);
    let floor = TableColumnConstraintSet::resolve_with(table, 2, 80.0, Some(&[10.0, 10.0]));
    assert_eq!(floor.tracks, vec![48.0, 48.0]);
    assert!(floor.tracks.iter().sum::<Px>() > floor.available_inline_size);
    let scaled = TableColumnConstraintSet::resolve_with(table, 2, 200.0, Some(&[10.0, 100.0]));
    assert!(scaled.tracks.iter().all(|t| *t >= 48.0 - 1e-6));
    assert!((scaled.tracks.iter().sum::<Px>() - 200.0).abs() < 1.0);
}

struct StubMeasure;

impl TextMeasure for StubMeasure {
    fn begin_island(&self) {}

    fn measure(
        &self,
        text: &str,
        _runs: &[InlineRun],
        avail_width: Px,
        _kind: MeasureKind,
        _block_kind: BlockKind,
        _ident: ShapeIdentity,
    ) -> MeasureResult {
        let width = (text.chars().count() as Px * 8.0).min(avail_width.max(0.0));
        MeasureResult {
            width,
            height: 20.0,
            rows: 1,
            first_baseline: 15.0,
        }
    }
}

#[test]
fn assembly_measures_islands_and_builds_window() {
    let doc = load_markdown("# title\n\nbody\n", editor_options());
    let tree = compose(&doc, &theme());
    let assembly = assemble_tree(tree, Default::default(), &StubMeasure, &FallbackSolver);
    assert!(!assembly.geometries.is_empty());
    assert_eq!(
        assembly.island_stats.islands_built as usize,
        assembly.geometries.len()
    );
    assert!(
        assembly
            .window
            .as_ref()
            .is_some_and(|window| { window.total_height > 0.0 && !window.entries.is_empty() })
    );
}

#[test]
fn non_incremental_assembly_solves_rows_not_cells() {
    let doc = load_markdown(TABLE_2X2_MD, editor_options());
    let tree = compose(&doc, &theme());
    let assembly = assemble_tree(tree, Default::default(), &StubMeasure, &FallbackSolver);

    assert_eq!(assembly.island_stats.islands_built, 2);
    assert_eq!(assembly.geometries.len(), 2);
    let cells: usize = assembly.geometries.values().map(|g| g.cells.len()).sum();
    assert_eq!(
        cells, 4,
        "cell geometry is carried along by the row island solve"
    );
}

const TABLE_2X2_MD: &str = "| a | b |\n| --- | --- |\n| c | d |\n";

#[test]
fn island_stats_starts_empty() {
    let stats = IslandStats::default();
    assert_eq!(stats.islands_built, 0);
}
