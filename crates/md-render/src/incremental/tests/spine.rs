use super::support::{CountingMeasure, dummy_layout, estimator};
use crate::incremental::anchor::ScrollAnchor;
use crate::incremental::engine::IncrementalEngine;
use md_core::document::{editor_options, load_markdown};
use md_layout::island::FallbackSolver;
use md_layout::style::BoxLayoutEnvironment;
use std::cell::Cell;
use std::fmt::Write;

fn leaf_containing(doc: &md_core::document::Document, needle: &str) -> md_core::block::BlockId {
    let mut found = None;
    doc.for_each_text_leaf(|id, text| {
        if text.contains(needle) {
            found = Some(id);
            false
        } else {
            true
        }
    });
    found.expect("leaf")
}

#[test]
fn ensure_block_on_spine_expands_collapsed_list() {
    let mut md = String::new();
    for i in 0..200 {
        let _ = writeln!(md, "paragraph {i} {}\n", "word ".repeat(8));
    }
    let _ = writeln!(md, "- findme in list");
    let doc = load_markdown(&md, editor_options());
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let target = leaf_containing(&doc, "findme");
    assert!(!engine.debug_block_on_spine(target));
    assert!(engine.ensure_block_on_spine(target, &measure, &solver));
    assert!(engine.debug_block_on_spine(target));
    assert!(engine.debug_block_top(target).is_some());
}

#[test]
fn ensure_block_on_spine_expands_collapsed_quote() {
    let mut md = String::new();
    for i in 0..200 {
        let _ = writeln!(md, "paragraph {i} {}\n", "word ".repeat(8));
    }
    let _ = writeln!(md, "> findme quoted");
    let doc = load_markdown(&md, editor_options());
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let target = leaf_containing(&doc, "findme");
    assert!(!engine.debug_block_on_spine(target));
    assert!(engine.ensure_block_on_spine(target, &measure, &solver));
    assert!(engine.debug_block_on_spine(target));
    assert!(engine.debug_block_top(target).is_some());
}

#[test]
fn ensure_block_on_spine_pin_lasts_one_publish_then_recollapses() {
    let mut md = String::new();
    for i in 0..200 {
        let _ = writeln!(md, "paragraph {i} {}\n", "word ".repeat(8));
    }
    let _ = writeln!(md, "> findme quoted");
    let doc = load_markdown(&md, editor_options());
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let target = leaf_containing(&doc, "findme");
    assert!(engine.ensure_block_on_spine(target, &measure, &solver));
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert!(
        engine.debug_block_on_spine(target),
        "pin must keep the revealed quote on the spine for the publish frame"
    );
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert!(
        !engine.debug_block_on_spine(target),
        "cleared pin must let the offscreen quote recollapse"
    );
}
