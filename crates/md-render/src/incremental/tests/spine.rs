use super::support::{CountingMeasure, dummy_layout, estimator, loaded};
use crate::incremental::anchor::ScrollAnchor;
use crate::incremental::engine::IncrementalEngine;
use md_core::block::{BlockId, BlockKind};
use md_core::doc::Doc;
use md_core::document::{Caret, Command, Sel, editor_options, load_markdown};
use md_layout::box_tree::LayoutBoxId;
use md_layout::island::FallbackSolver;
use md_layout::spine::FlowItemKind;
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
    assert!(!engine.block_is_on_spine(target));
    assert!(engine.ensure_block_on_spine(target, &measure, &solver));
    assert!(engine.block_is_on_spine(target));
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
    assert!(!engine.block_is_on_spine(target));
    assert!(engine.ensure_block_on_spine(target, &measure, &solver));
    assert!(engine.block_is_on_spine(target));
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
        engine.block_is_on_spine(target),
        "pin must keep the revealed quote on the spine for the publish frame"
    );
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    assert!(
        !engine.block_is_on_spine(target),
        "cleared pin must let the offscreen quote recollapse"
    );
}

fn content_entries(engine: &IncrementalEngine, box_id: LayoutBoxId) -> usize {
    (0..engine.spine.len())
        .filter(|&pos| {
            matches!(
                engine.spine.item_at(pos).kind,
                FlowItemKind::Content { box_id: b } if b == box_id
            )
        })
        .count()
}

fn assert_spine_mirrors_the_tree(engine: &IncrementalEngine) {
    for pos in 0..engine.spine.len() {
        if let FlowItemKind::Content { box_id } = engine.spine.item_at(pos).kind {
            assert!(
                engine.tree.nodes().contains_key(&box_id),
                "spine content {box_id:?} is not in the box tree"
            );
            assert_eq!(
                content_entries(engine, box_id),
                1,
                "spine lists content {box_id:?} more than once"
            );
        }
    }
}

fn step(doc: &mut Doc, engine: &mut IncrementalEngine, block: &mut BlockId, cmd: Command) {
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    let offset = doc.text(*block).map(str::len).unwrap_or(0);
    let caret = doc.apply(
        Sel::collapsed(Caret {
            block: *block,
            offset,
        }),
        cmd,
    );
    *block = caret.block;
    let changes = doc.document.take_changes();
    engine.apply_changes(&doc.document, &changes);
    engine.sync_block_edit_retain_y(&doc.document, *block, 0.0, &measure, &solver);
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
}

fn soft_break_then_marker(
    doc: &mut Doc,
    engine: &mut IncrementalEngine,
    prefix: &str,
    marker: &str,
) -> BlockId {
    let mut block = doc.text_leaves()[0];
    step(
        doc,
        engine,
        &mut block,
        Command::Insert {
            text: prefix.into(),
        },
    );
    step(
        doc,
        engine,
        &mut block,
        Command::Insert { text: "hi".into() },
    );
    step(doc, engine, &mut block, Command::SoftBreak);
    step(
        doc,
        engine,
        &mut block,
        Command::Insert {
            text: marker.into(),
        },
    );
    step(doc, engine, &mut block, Command::Break);
    block
}

const PROMOTIONS: [(&str, BlockKind); 2] = [("```", BlockKind::CodeBlock), ("$$", BlockKind::Math)];

const CONTAINERS: [&str; 3] = ["", "> ", "- "];

fn engine_for(doc: &Doc) -> IncrementalEngine {
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc.document, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &FallbackSolver);
    engine
}

#[test]
fn a_leaf_promoted_after_a_soft_break_reaches_the_spine_once() {
    for (marker, kind) in PROMOTIONS {
        for prefix in CONTAINERS {
            let mut doc = Doc::new(loaded(""));
            let mut engine = engine_for(&doc);
            let block = soft_break_then_marker(&mut doc, &mut engine, prefix, marker);
            assert_eq!(
                doc.document.kind(block),
                Some(kind),
                "prefix {prefix:?} marker {marker:?}"
            );
            assert_eq!(
                content_entries(&engine, LayoutBoxId::frame(block)),
                1,
                "prefix {prefix:?} marker {marker:?}"
            );
            assert_spine_mirrors_the_tree(&engine);
        }
    }
}

#[test]
fn undoing_a_leaf_promoted_after_a_soft_break_leaves_the_spine_consistent() {
    for (marker, _) in PROMOTIONS {
        for prefix in CONTAINERS {
            let mut doc = Doc::new(loaded(""));
            let mut engine = engine_for(&doc);
            let promoted = soft_break_then_marker(&mut doc, &mut engine, prefix, marker);
            assert!(doc.undo().is_some(), "prefix {prefix:?} marker {marker:?}");

            let changes = doc.document.take_changes();
            engine.apply_changes(&doc.document, &changes);
            let caret = doc.text_leaves()[0];
            let measure = CountingMeasure {
                calls: Cell::new(0),
            };
            engine.sync_block_edit_retain_y(&doc.document, caret, 0.0, &measure, &FallbackSolver);
            engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &FallbackSolver);

            assert_eq!(
                content_entries(&engine, LayoutBoxId::frame(promoted)),
                0,
                "prefix {prefix:?} marker {marker:?}"
            );
            assert_spine_mirrors_the_tree(&engine);
        }
    }
}
