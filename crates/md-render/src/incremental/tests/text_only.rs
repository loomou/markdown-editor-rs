use md_core::block::BlockKind;
use md_core::document::{Caret, Command, Sel, TableOp, apply};
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
    assert!(!tables_before.is_empty(), "the fixture must contain tables");
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
        "Enter should be a structural change"
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
fn an_unrelated_structural_edit_keeps_the_solved_constraints() {
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
    let cons_before = Rc::clone(&engine.table_cons);

    let leaf = doc.text_leaves()[0];
    let at = Caret {
        block: leaf,
        offset: 1,
    };
    apply(&mut doc, Sel::collapsed(at), Command::Break);
    let changes = doc.take_changes();
    assert!(
        changes.is_structural(),
        "Enter should be a structural change"
    );
    engine.apply_changes(&doc, &changes);

    assert!(
        !engine.table_cons.is_empty(),
        "an unrelated structural edit dropped the untouched table constraints (whole-map clear again?)"
    );
    assert!(
        Rc::ptr_eq(&cons_before, &engine.table_cons),
        "the paragraph splice touches no table, yet table_cons was swapped"
    );
}

#[test]
fn a_spliced_table_re_resolves_after_the_edit() {
    let md = "lead\n\n| a | b |\n| --- | --- |\n| c | d |\n\ntail\n";
    let mut doc = loaded(md);
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 2000.0, &measure, &solver);
    let tables = tables_of(&engine);
    assert_eq!(tables.len(), 1, "the fixture needs exactly one table");
    let table = tables[0];
    assert!(
        engine.table_cons.contains_key(&table),
        "premise: the first frame solved the table"
    );

    let cell = doc
        .text_leaves()
        .into_iter()
        .find(|&b| doc.text_of(b) == Some("a"))
        .expect("cell a");
    apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: cell,
            offset: 1,
        }),
        Command::Table(TableOp::InsertRowBelow),
    );
    let changes = doc.take_changes();
    assert!(
        changes.is_structural(),
        "inserting a row must be a structural change"
    );
    engine.apply_changes(&doc, &changes);
    assert!(
        !engine.table_cons.contains_key(&table),
        "the host table's stale constraints survived a row-set change"
    );

    engine.assemble_incremental(ScrollAnchor::top(), 2000.0, &measure, &solver);
    assert!(
        engine.table_cons.contains_key(&table),
        "the next frame did not re-solve the affected table"
    );
    let fresh = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    assert_eq!(
        cons_by_value(&engine),
        cons_by_value(&fresh),
        "after the precise invalidation the table constraints disagree with a fresh build"
    );
}

#[test]
fn a_marker_width_change_invalidates_the_list_nested_table() {
    let md = concat!(
        "1. one\n2. two\n3. three\n4. four\n5. five\n",
        "6. six\n7. seven\n8. eight\n9. nine\n",
        "   | a | b |\n   | --- | --- |\n   | c | d |\n",
    );
    let mut doc = loaded(md);
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 2000.0, &measure, &solver);
    let tables = tables_of(&engine);
    assert_eq!(
        tables.len(),
        1,
        "the fixture nests one table inside item nine"
    );
    let table = tables[0];
    assert!(
        engine.table_cons.contains_key(&table),
        "premise: the first frame solved the table"
    );

    let nine = doc
        .text_leaves()
        .into_iter()
        .find(|&b| doc.text_of(b) == Some("nine"))
        .expect("item nine");
    apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: nine,
            offset: 4,
        }),
        Command::Break,
    );
    let changes = doc.take_changes();
    assert!(
        changes.is_structural(),
        "splitting out item ten must be a structural change"
    );
    engine.apply_changes(&doc, &changes);

    assert!(
        !engine.table_cons.contains_key(&table),
        "the list-nested table kept its stale constraints after the marker width changed"
    );
    engine.assemble_incremental(ScrollAnchor::top(), 2000.0, &measure, &solver);
    let fresh = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    assert_eq!(
        cons_by_value(&engine),
        cons_by_value(&fresh),
        "after the re-solve the table constraints disagree with a fresh build"
    );
}

#[test]
fn an_attrs_change_invalidates_only_the_tables_in_the_subtree() {
    let md = concat!(
        "- [ ] task\n\n",
        "  | a | b |\n  | --- | --- |\n  | c | d |\n\n",
        "outside\n\n",
        "| e | f |\n| --- | --- |\n| g | h |\n",
    );
    let mut doc = loaded(md);
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 2000.0, &measure, &solver);
    let tables = tables_of(&engine);
    assert_eq!(
        tables.len(),
        2,
        "the fixture needs one nested and one outside table"
    );
    let nested = tables
        .iter()
        .copied()
        .find(|id| {
            let mut cur = *id;
            while let Some(p) = engine.tree.get(cur).parent() {
                if engine.tree.get(p).kind() == BlockKind::ListItem {
                    return true;
                }
                cur = p;
            }
            false
        })
        .expect("the fixture must contain a table nested in a list item");
    let outside = tables.iter().copied().find(|id| id != &nested).unwrap();
    assert!(
        engine.table_cons.contains_key(&nested) && engine.table_cons.contains_key(&outside),
        "premise: the first frame must have solved both tables"
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
        "checking a task item must not be treated as a text-only change, got {:?}",
        changes.changes
    );
    engine.apply_changes(&doc, &changes);

    assert!(
        !engine.table_cons.contains_key(&nested),
        "the subtree's table kept its stale constraints after the attribute change"
    );
    assert!(
        engine.table_cons.contains_key(&outside),
        "the attribute change dropped a table outside the subtree (whole-map clear again?)"
    );
}
