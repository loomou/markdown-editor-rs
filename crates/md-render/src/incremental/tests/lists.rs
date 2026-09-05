use md_layout::compose::compose;

use super::support::{
    CountingMeasure, assert_tree_matches_cold, dummy_layout, estimator, loaded, long_doc, long_list,
};
use crate::incremental::anchor::ScrollAnchor;
use crate::incremental::engine::IncrementalEngine;
use md_core::document::{Caret, Command, PasteIntent, Sel, apply};
use md_layout::island::FallbackSolver;
use md_layout::style::BoxLayoutEnvironment;
use std::cell::Cell;
use std::fmt::Write;

#[test]
fn list_break_splices_without_clear_or_flatten() {
    let mut doc = long_list();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let first = doc.text_leaves()[0];
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: first,
            offset: 1,
        }),
        Command::Break,
    );
    let changes = doc.take_changes();
    assert!(changes.is_structural());
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
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
fn list_empty_break_splices_without_clear_or_flatten() {
    let mut doc = long_list();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let first = doc.text_leaves()[0];
    let end = doc.text_of(first).unwrap().len();
    let empty = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: first,
            offset: end,
        }),
        Command::Break,
    );
    let mid = doc.take_changes();
    let settle = engine.apply_changes(&doc, &mid);
    assert!(!settle.structural_full_clear);
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: empty.block,
            offset: 0,
        }),
        Command::Break,
    );
    let changes = doc.take_changes();
    assert!(changes.is_structural());
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    for (id, node) in cold.nodes() {
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
    }
}

#[test]
fn list_join_prev_splices_without_clear_or_flatten() {
    let mut doc = long_list();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let second = doc.text_leaves()[1];
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: second,
            offset: 0,
        }),
        Command::DeleteBackward,
    );
    let changes = doc.take_changes();
    assert!(changes.is_structural());
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    for (id, node) in cold.nodes() {
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
    }
}

#[test]
fn list_indent_splices_without_clear_or_flatten() {
    let mut doc = long_list();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let second = doc.text_leaves()[1];
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: second,
            offset: 0,
        }),
        Command::Indent,
    );
    let changes = doc.take_changes();
    assert!(changes.is_structural());
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    for (id, node) in cold.nodes() {
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
    }
}

#[test]
fn list_range_indent_splices_without_clear_or_flatten() {
    let mut doc = long_list();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let second = doc.text_leaves()[1];
    let third = doc.text_leaves()[2];
    let _ = apply(
        &mut doc,
        Sel {
            anchor: Caret {
                block: second,
                offset: 0,
            },
            head: Caret {
                block: third,
                offset: 0,
            },
        },
        Command::Indent,
    );
    let changes = doc.take_changes();
    assert!(changes.is_structural());
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    for (id, node) in cold.nodes() {
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
    }
}

fn nested_head_list() -> md_core::document::Document {
    let mut md = String::new();
    let _ = writeln!(md, "- item 0 {}", "word ".repeat(8));
    let _ = writeln!(md, "  - nested");
    for i in 1..80 {
        let _ = writeln!(md, "- item {i} {}", "word ".repeat(8));
    }
    loaded(&md)
}

#[test]
fn list_rich_break_splices_without_clear_or_flatten() {
    let mut doc = nested_head_list();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let first = doc.text_leaves()[0];
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: first,
            offset: 1,
        }),
        Command::Break,
    );
    let changes = doc.take_changes();
    assert!(changes.is_structural());
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    for (id, node) in cold.nodes() {
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
    }
}

#[test]
fn list_toggle_task_does_not_flatten() {
    let mut doc = long_list();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let first = doc.text_leaves()[0];
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: first,
            offset: 0,
        }),
        Command::ToggleTask,
    );
    let changes = doc.take_changes();
    assert!(!changes.is_structural());
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    for (id, node) in cold.nodes() {
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
        assert_eq!(engine.tree.get(*id).extra(), node.extra());
    }
}

#[test]
fn wrap_list_splices_without_clear_or_flatten() {
    let mut doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let first = doc.text_leaves()[0];
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: first,
            offset: 0,
        }),
        Command::WrapList {
            ordered: false,
            task: None,
        },
    );
    let changes = doc.take_changes();
    assert!(changes.is_structural());
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    for (id, node) in cold.nodes() {
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
    }
}

#[test]
fn wrap_list_undo_splices_without_clear_or_flatten() {
    let mut d = md_core::doc::Doc::new(long_doc());
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&d.document, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let first = d.text_leaves()[0];
    let _ = d.apply(
        Sel::collapsed(Caret {
            block: first,
            offset: 0,
        }),
        Command::WrapList {
            ordered: false,
            task: None,
        },
    );
    let wrap_cs = d.take_changes();
    let settle = engine.apply_changes(&d.document, &wrap_cs);
    assert!(!settle.structural_full_clear);
    let _ = d.undo().expect("undo");
    let changes = d.take_changes();
    assert!(changes.is_structural());
    assert!(!changes.is_replace());
    let settle = engine.apply_changes(&d.document, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
    assert_tree_matches_cold(&engine, &d.document);
    let cold = compose(&d.document, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    for (id, node) in cold.nodes() {
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
    }
}

#[test]
fn wrap_then_sole_item_backspace_splices_without_clear_or_flatten() {
    let mut doc = long_doc();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let first = doc.text_leaves()[0];
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: first,
            offset: 0,
        }),
        Command::WrapList {
            ordered: false,
            task: None,
        },
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
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    for (id, node) in cold.nodes() {
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
    }
}

#[test]
fn sole_item_backspace_splices_without_clear_or_flatten() {
    let mut md = String::from("- abc\n\n");
    for i in 0..80 {
        let _ = writeln!(md, "paragraph {i} {}\n", "word ".repeat(8));
    }
    let mut doc = loaded(&md);
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let first = doc.text_leaves()[0];
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
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    for (id, node) in cold.nodes() {
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
    }
}

#[test]
fn nested_sole_item_lift_splices_without_clear_or_flatten() {
    let mut doc = nested_head_list();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let nested = doc.text_leaves()[1];
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: nested,
            offset: 0,
        }),
        Command::DeleteBackward,
    );
    let changes = doc.take_changes();
    assert!(changes.is_structural());
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    for (id, node) in cold.nodes() {
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
    }
}

#[test]
fn list_paste_join_splices_without_clear_or_flatten() {
    let mut doc = long_list();
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let first = doc.text_leaves()[0];
    let off = doc.text_of(first).unwrap().len();
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: first,
            offset: off,
        }),
        Command::Paste {
            text: "- extra\n".into(),
            intent: PasteIntent::IndependentFragment,
        },
    );
    let changes = doc.take_changes();
    assert!(changes.is_structural());
    let settle = engine.apply_changes(&doc, &changes);
    assert!(!settle.structural_full_clear);
    assert_eq!(engine.flatten_gens, 1);
    assert_tree_matches_cold(&engine, &doc);
    let cold = compose(&doc, &dummy_layout());
    assert_eq!(engine.tree.nodes().len(), cold.nodes().len());
    for (id, node) in cold.nodes() {
        assert_eq!(engine.tree.get(*id).kind(), node.kind());
    }
}
