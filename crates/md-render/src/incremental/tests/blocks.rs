use md_core::block::BlockKind;
use md_core::document::DocChange;
use md_layout::compose::compose;
use md_layout::spine::FlowItemKind;

use super::support::{
    CountingMeasure, assert_tree_matches_cold, dummy_layout, estimator, loaded, long_doc,
};
use crate::incremental::anchor::ScrollAnchor;
use crate::incremental::engine::IncrementalEngine;
use md_core::document::{Caret, Command, Sel, apply};
use md_layout::island::FallbackSolver;
use md_layout::style::BoxLayoutEnvironment;
use std::cell::Cell;

#[test]
fn island_inline_edit_is_text_changed_without_flatten() {
    let mut doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert_eq!(engine.flatten_gens, 1);
    let first = doc.text_leaves()[0];
    let mut at = Caret {
        block: first,
        offset: doc.text_of(first).unwrap().len(),
    };
    for ch in ['`', 'a', '`'] {
        at = apply(
            &mut doc,
            Sel::collapsed(at),
            Command::Insert {
                text: ch.to_string(),
            },
        );
        let changes = doc.take_changes();
        assert!(!changes.is_structural());
        assert!(!changes.is_replace());
        assert!(
            changes
                .changes
                .iter()
                .all(|c| matches!(c, DocChange::TextChanged { .. }))
        );
        let settle = engine.apply_changes(&doc, &changes);
        assert!(!settle.structural_full_clear);
    }
    assert_eq!(engine.flatten_gens, 1);
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    for (id, node) in cold.nodes() {
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
    }
    let id = doc.live_id(first).expect("live");
    assert!(
        doc.runs(id)
            .iter()
            .any(|r| { r.marks.contains(md_core::inline::InlineMarks::CODE) })
    );
}

#[test]
fn typing_undo_is_text_changed_without_flatten() {
    let mut d = md_core::doc::Doc::new(long_doc());
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&d.document, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert_eq!(engine.flatten_gens, 1);
    let first = d.text_leaves()[0];
    let at = d.text(first).unwrap_or("").len();
    let _ = d.apply(
        Sel::collapsed(Caret {
            block: first,
            offset: at,
        }),
        Command::Insert { text: "x".into() },
    );
    let typed = d.take_changes();
    assert!(!typed.is_replace());
    let settle = engine.apply_changes(&d.document, &typed);
    assert!(!settle.structural_full_clear);
    let _ = d.undo().expect("undo");
    let changes = d.take_changes();
    assert!(!changes.is_replace());
    assert!(!changes.is_structural());
    assert!(
        changes
            .changes
            .iter()
            .all(|c| matches!(c, DocChange::TextChanged { .. }))
    );
    let settle = engine.apply_changes(&d.document, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
}

#[test]
fn atx_space_splices_heading_without_clear_or_flatten() {
    let mut doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert_eq!(engine.flatten_gens, 1);
    let first = doc.text_leaves()[0];
    let id = doc.live_id(first).expect("live");
    let parent = doc.arena.get(id).and_then(|n| n.parent).expect("parent");
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: first,
            offset: 0,
        }),
        Command::Insert {
            text: "### ".into(),
        },
    );
    let changes = doc.take_changes();
    assert!(changes.is_structural());
    assert!(changes.changes.iter().all(|c| match c {
        DocChange::TreeSpliced { parent: p, .. } => doc.arena.get(*p).is_some(),
        _ => true,
    }));
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
    assert!(doc.arena.get(parent).is_some());
    assert_eq!(doc.kind(first), Some(BlockKind::Heading(3)));
    assert!(!doc.text_of(first).unwrap().starts_with('#'));
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    for (id, node) in cold.nodes() {
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
    }
}

#[test]
fn quote_marker_splices_without_clear_or_flatten() {
    let mut doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert_eq!(engine.flatten_gens, 1);
    let first = doc.text_leaves()[0];
    let id = doc.live_id(first).expect("live");
    let parent = doc.arena.get(id).and_then(|n| n.parent).expect("parent");
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: first,
            offset: 0,
        }),
        Command::Insert { text: "> ".into() },
    );
    let changes = doc.take_changes();
    assert!(changes.is_structural());
    assert!(changes.changes.iter().all(|c| match c {
        DocChange::TreeSpliced { parent: p, .. } => doc.arena.get(*p).is_some(),
        _ => true,
    }));
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
    assert!(doc.arena.get(parent).is_some());
    let quote = doc.arena.get(id).and_then(|n| n.parent).expect("quote");
    assert_eq!(
        doc.arena.get(quote).map(|n| n.kind),
        Some(BlockKind::BlockQuote)
    );
    assert!(!doc.text_of(first).unwrap().starts_with('>'));
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    for (id, node) in cold.nodes() {
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
    }
}

#[test]
fn quote_backspace_unwraps_without_clear_or_flatten() {
    let mut doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let first = doc.text_leaves()[0];
    let host = doc
        .live_id(first)
        .and_then(|id| doc.arena.get(id).and_then(|n| n.parent))
        .expect("host");
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: first,
            offset: 0,
        }),
        Command::Insert { text: "> ".into() },
    );
    let mid = doc.take_changes();
    let settle = engine.apply_changes(&doc, &mid);
    assert!(!settle.structural_full_clear);
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: first,
            offset: 0,
        }),
        Command::DeleteBackward,
    );
    let changes = doc.take_changes();
    assert!(changes.is_structural());
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
    assert!(doc.arena.get(host).is_some());
    let id = doc.live_id(first).expect("live");
    assert_ne!(
        doc.arena.get(id).and_then(|n| n.parent).map(|p| doc
            .arena
            .get(p)
            .map(|n| n.kind)
            .expect("kind")),
        Some(BlockKind::BlockQuote)
    );
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    for (id, node) in cold.nodes() {
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
    }
}

#[test]
fn rule_enter_splices_without_clear_or_flatten() {
    let mut doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert_eq!(engine.flatten_gens, 1);
    let first = doc.text_leaves()[0];
    let n = doc.text_of(first).unwrap().len();
    let _ = doc.replace_text(first, 0..n, "---");
    let changes = doc.take_changes();
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    let id = doc.live_id(first).expect("live");
    let parent = doc.arena.get(id).and_then(|n| n.parent).expect("parent");
    let off = doc.text_of(first).unwrap().len();
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: first,
            offset: off,
        }),
        Command::Break,
    );
    let changes = doc.take_changes();
    assert!(changes.is_structural());
    assert!(changes.changes.iter().all(|c| match c {
        DocChange::TreeSpliced { parent: p, .. } => doc.arena.get(*p).is_some(),
        _ => true,
    }));
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
    assert!(doc.arena.get(parent).is_some());
    assert_eq!(doc.kind(first), Some(BlockKind::ThematicBreak));
    assert_eq!(doc.text_of(first).unwrap(), "");
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    for (id, node) in cold.nodes() {
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
    }
}

#[test]
fn fence_enter_splices_without_clear_or_flatten() {
    let mut doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert_eq!(engine.flatten_gens, 1);
    let first = doc.text_leaves()[0];
    let n = doc.text_of(first).unwrap().len();
    let _ = doc.replace_text(first, 0..n, "```rust");
    let changes = doc.take_changes();
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    let id = doc.live_id(first).expect("live");
    let parent = doc.arena.get(id).and_then(|n| n.parent).expect("parent");
    let off = doc.text_of(first).unwrap().len();
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: first,
            offset: off,
        }),
        Command::Break,
    );
    let changes = doc.take_changes();
    assert!(changes.is_structural());
    assert!(changes.changes.iter().all(|c| match c {
        DocChange::TreeSpliced { parent: p, .. } => doc.arena.get(*p).is_some(),
        _ => true,
    }));
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
    assert!(doc.arena.get(parent).is_some());
    assert_eq!(doc.kind(first), Some(BlockKind::CodeBlock));
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    for (id, node) in cold.nodes() {
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
    }
}

#[test]
fn enter_splices_without_clear_or_flatten() {
    let mut doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let sa = engine.anchor_at_y(2_000.0, &measure, &solver);
    engine.assemble_incremental(sa, 600.0, &measure, &solver);
    let item = sa.item;
    let Some(FlowItemKind::Content { box_id }) = engine.spine.get(item).map(|i| i.kind) else {
        panic!("anchor content");
    };
    assert!(
        engine.store.is_materialized(box_id),
        "precondition: the anchored content must be materialized"
    );
    let first = doc.text_leaves()[0];
    let changes = doc.split_leaf(first, 1).0;
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
    assert!(engine.spine.location(item).is_some());
    assert_eq!(engine.spine.content_id(box_id).expect("kept id"), item);
    assert!(engine.store.is_materialized(box_id));
    engine.assemble_incremental(sa, 600.0, &measure, &solver);
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    for (id, node) in cold.nodes() {
        assert_eq!(
            engine.tree.intern().text(engine.tree.get(*id).text_id()),
            cold.intern().text(node.text_id())
        );
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
    }
}

#[test]
fn merge_splices_without_clear_or_flatten() {
    let mut doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    let sa = engine.anchor_at_y(2_000.0, &measure, &solver);
    engine.assemble_incremental(sa, 600.0, &measure, &solver);
    let item = sa.item;
    let leaves = doc.text_leaves();
    let changes = doc.merge_into_prev(leaves[1]).expect("merge").0;
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
    assert!(engine.spine.location(item).is_some());
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
}

#[test]
fn independent_fragment_paste_splices_without_flatten() {
    let mut doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let first = doc.text_leaves()[0];
    let off = doc.text_of(first).unwrap().len();
    let (changes, _, _) = doc.paste(
        first,
        off..off,
        "> quoted\n",
        md_core::document::PasteIntent::IndependentFragment,
    );
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    let bars = cold
        .nodes()
        .values()
        .filter(|n| n.id().role == md_layout::box_tree::BoxRole::Bar)
        .count();
    assert!(bars >= 1);
    for (id, node) in cold.nodes() {
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
    }
}

#[test]
fn deleting_every_block_keeps_the_survivor_renderable() {
    let mut doc = loaded("alpha\n\nbeta\n\ngamma\n");
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    let (before, _) = engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert_eq!(before.geometries.len(), 3);
    let leaves = doc.text_leaves();
    let last = *leaves.last().expect("leaf");
    let end = doc.text_of(last).map_or(0, |t| t.len());
    let cur = apply(
        &mut doc,
        Sel {
            anchor: Caret {
                block: leaves[0],
                offset: 0,
            },
            head: Caret {
                block: last,
                offset: end,
            },
        },
        Command::DeleteBackward,
    );
    let changes = doc.take_changes();
    assert!(
        changes.changes.iter().any(|c| matches!(
            c,
            DocChange::TreeSpliced { before: Some(b), .. } if doc.arena.get(*b).is_none()
        )),
        "no dangling `before` in the change set, the fixture has gone stale: {:?}",
        changes.changes
    );
    engine.apply_changes(&doc, &changes);
    assert_eq!(doc.text_leaves(), vec![cur.block]);
    assert!(engine.debug_block_on_spine(cur.block));
    assert!(engine.debug_block_top(cur.block).is_some());
    let (after, published) =
        engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert_eq!(after.geometries.len(), 1);
    assert_eq!(published.resolved_top, 0.0);
}

#[test]
fn move_column_rematerializes_row_cell_x() {
    use md_core::document::{TableOp, table_loc};
    use md_layout::box_tree::{BoxChildren, BoxOwner};
    use std::collections::BTreeMap;

    let mut doc = loaded("| a | b | c |\n| --- | --- | --- |\n| d | e | f |\n");
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    let (before, _) = engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert!(before.geometries.values().any(|g| g.cells.len() == 3));

    let c = doc
        .text_leaves()
        .into_iter()
        .find(|&id| doc.text_of(id) == Some("c"))
        .expect("c");
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: c,
            offset: 0,
        }),
        Command::Table(TableOp::MoveColumnTo { index: 0 }),
    );
    let changes = doc.take_changes();
    assert!(changes.is_structural());
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    assert!(
        !settle.invalidated.is_empty(),
        "row islands must drop after a column splice"
    );

    let (after, _) = engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    for (id, node) in after.tree.nodes() {
        let BoxChildren::Island(kids) = node.children() else {
            continue;
        };
        let Some(g) = after.geometries.get(id) else {
            continue;
        };
        let got: Vec<_> = g.cells.iter().map(|c| c.cell_box).collect();
        assert_eq!(got, *kids, "row island geometry must follow child order");
    }

    let mut by_row: BTreeMap<usize, Vec<(usize, md_core::Px)>> = BTreeMap::new();
    for g in after.geometries.values() {
        for cell in &g.cells {
            let BoxOwner::Block(block) = cell.cell_box.owner else {
                continue;
            };
            let Some(loc) = table_loc(&doc, block) else {
                continue;
            };
            by_row.entry(loc.row).or_default().push((loc.col, cell.x));
        }
    }
    assert_eq!(by_row.len(), 2);
    for (row, cols) in &mut by_row {
        cols.sort_by_key(|(col, _)| *col);
        assert_eq!(cols.len(), 3, "row {row}");
        for pair in cols.windows(2) {
            assert!(
                pair[0].1 < pair[1].1,
                "row {row}: col {} x {} should sit left of col {} x {}",
                pair[0].0,
                pair[0].1,
                pair[1].0,
                pair[1].1
            );
        }
    }
}
