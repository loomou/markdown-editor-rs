use md_core::block::BlockKind;
use md_core::document::{Caret, Command, Sel, apply};
use md_layout::box_tree::LayoutBoxId;
use md_layout::island::{FallbackSolver, TableColumnConstraintSet};
use md_layout::style::BoxLayoutEnvironment;
use std::cell::Cell;
use std::rc::Rc;

use super::support::{CountingMeasure, dummy_layout, estimator, loaded};
use crate::incremental::anchor::ScrollAnchor;
use crate::incremental::engine::IncrementalEngine;

fn mixed_doc() -> md_core::document::Document {
    let mut md = String::new();
    for i in 0..12 {
        md.push_str(&format!("paragraph {i} with some words\n\n"));
        md.push_str(&format!("## heading {i}\n\n"));
        md.push_str(&format!("- item {i}a\n- item {i}b\n\n"));
        md.push_str(&format!("> quoted {i}\n\n"));
        md.push_str("| a | b |\n| --- | --- |\n| c | d |\n\n");
        md.push_str(&format!("```\ncode {i}\n```\n\n"));
    }
    loaded(&md)
}

fn tables_of(engine: &IncrementalEngine) -> Vec<LayoutBoxId> {
    engine
        .tree
        .nodes()
        .into_iter()
        .filter(|(_, n)| n.kind() == BlockKind::Table)
        .map(|(id, _)| *id)
        .collect()
}

fn cons_by_value(engine: &IncrementalEngine) -> Vec<(LayoutBoxId, TableColumnConstraintSet)> {
    tables_of(engine)
        .into_iter()
        .map(|id| (id, engine.resolve_cons(id)))
        .collect()
}

#[test]
fn text_only_edits_leave_islands_and_tables_identical() {
    let mut doc = mixed_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);

    let islands_before = engine.tree.island_boxes();
    let tables_before = tables_of(&engine);
    assert!(
        !tables_before.is_empty(),
        "the fixture must contain a table"
    );
    let cons_before = Rc::clone(&engine.table_cons);

    let leaf = doc.text_leaves()[0];
    let mut at = Caret {
        block: leaf,
        offset: 0,
    };
    for i in 0..20 {
        at = apply(
            &mut doc,
            Sel::collapsed(at),
            Command::Insert {
                text: if i % 2 == 0 { "w" } else { "q" }.to_string(),
            },
        );
        let changes = doc.take_changes();
        assert!(
            changes.is_text_only(),
            "edit {i} must be text-only, got {:?}",
            changes.changes
        );
        engine.apply_changes(&doc, &changes);
    }

    assert_eq!(
        islands_before,
        engine.tree.island_boxes(),
        "the island set changed on a text-only edit"
    );
    assert_eq!(
        tables_before,
        tables_of(&engine),
        "the table set changed on a text-only edit"
    );
    assert!(
        Rc::ptr_eq(&cons_before, &engine.table_cons),
        "a text-only edit refilled table_cons: the clear was not skipped"
    );
}

#[test]
fn structural_edits_still_refresh_aux() {
    let mut doc = mixed_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);

    let leaf = doc.text_leaves()[0];
    let mut at = Caret {
        block: leaf,
        offset: 0,
    };
    for _ in 0..3 {
        at = apply(
            &mut doc,
            Sel::collapsed(at),
            Command::Insert {
                text: "z".to_string(),
            },
        );
        let changes = doc.take_changes();
        engine.apply_changes(&doc, &changes);
    }
    apply(&mut doc, Sel::collapsed(at), Command::Break);
    let changes = doc.take_changes();
    assert!(
        changes.is_structural(),
        "a break must be a structural change"
    );
    engine.apply_changes(&doc, &changes);

    let fresh = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    assert_eq!(
        engine.tree.island_boxes(),
        fresh.tree.island_boxes(),
        "after a structural edit the island set disagrees with a fresh build"
    );
    assert_eq!(
        cons_by_value(&engine),
        cons_by_value(&fresh),
        "after a structural edit the table constraints disagree with a fresh build"
    );
}

#[test]
fn a_structural_edit_discards_the_solved_constraints() {
    let mut doc = mixed_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert!(
        !engine.table_cons.is_empty(),
        "the first frame must have solved at least one table, or this case asserts nothing"
    );

    let leaf = doc.text_leaves()[0];
    let at = Caret {
        block: leaf,
        offset: 1,
    };
    apply(&mut doc, Sel::collapsed(at), Command::Break);
    let changes = doc.take_changes();
    assert!(
        changes.is_structural(),
        "a break must be a structural change"
    );
    engine.apply_changes(&doc, &changes);

    assert!(
        engine.table_cons.is_empty(),
        "stale column constraints survive the structural edit: {:?}",
        engine.table_cons.keys().collect::<Vec<_>>()
    );
}

#[test]
fn the_next_frame_resolves_the_cleared_constraints_again() {
    let mut doc = mixed_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);

    let leaf = doc.text_leaves()[0];
    let at = Caret {
        block: leaf,
        offset: 1,
    };
    apply(&mut doc, Sel::collapsed(at), Command::Break);
    let changes = doc.take_changes();
    engine.apply_changes(&doc, &changes);
    assert!(
        engine.table_cons.is_empty(),
        "premise: this step must clear the constraints"
    );

    engine.assemble_incremental(ScrollAnchor::top(), 2000.0, &measure, &solver);

    assert!(
        !engine.table_cons.is_empty(),
        "the next frame re-solved no table — if nothing refills after the clear, clearing is wrong"
    );
    let want: std::collections::BTreeMap<_, _> = cons_by_value(&engine).into_iter().collect();
    for (id, got) in engine.table_cons.iter() {
        assert_eq!(
            Some(got),
            want.get(id),
            "the re-solved constraints disagree with the expected values table={id:?}"
        );
    }
}

#[test]
fn an_attrs_change_also_discards_the_constraints() {
    let md = "- [ ] task\n\n  | a | b |\n  | --- | --- |\n  | c | d |\n";
    let mut doc = loaded(md);
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 2000.0, &measure, &solver);
    assert!(
        !engine.table_cons.is_empty(),
        "premise: the first frame must solve that nested table"
    );

    let leaf = doc.text_leaves()[0];
    apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: leaf,
            offset: 0,
        }),
        Command::ToggleTask,
    );
    let changes = doc.take_changes();
    assert!(
        !changes.is_text_only(),
        "toggling a task item must not count as text-only, got {:?}",
        changes.changes
    );
    engine.apply_changes(&doc, &changes);

    assert!(
        engine.table_cons.is_empty(),
        "stale column constraints survive an attribute change"
    );
}
