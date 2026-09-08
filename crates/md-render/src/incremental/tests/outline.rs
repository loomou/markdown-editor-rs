use super::support::{CountingMeasure, dummy_layout, estimator, loaded};
use crate::incremental::anchor::ScrollAnchor;
use crate::incremental::engine::IncrementalEngine;
use md_core::Px;
use md_core::block::{BlockId, BlockKind};
use md_layout::island::FallbackSolver;
use md_layout::style::BoxLayoutEnvironment;
use std::cell::Cell;
use std::fmt::Write;

fn headings(doc: &md_core::document::Document) -> Vec<BlockId> {
    doc.preorder()
        .iter()
        .filter(|id| {
            doc.arena
                .get(**id)
                .is_some_and(|n| matches!(n.kind, BlockKind::Heading(_)))
        })
        .map(|id| id.index)
        .collect()
}

#[test]
fn heading_at_or_above_picks_the_last_heading_past_the_line() {
    let doc = loaded("# one\n\npara one\n\n## two\n\npara two\n\n# three\n\npara three\n");
    let mut engine = IncrementalEngine::new(
        &doc,
        BoxLayoutEnvironment::default(),
        estimator(),
        dummy_layout(),
    );
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &FallbackSolver);

    let hs = headings(&doc);
    assert_eq!(hs.len(), 3, "all three headings must be in the list");

    for (i, &h) in hs.iter().enumerate() {
        let y = engine
            .content_top(h)
            .expect("a flat heading should have an exact item");
        assert_eq!(
            engine.heading_at_or_above(&doc, &hs, y),
            Some(h),
            "heading {i} must be picked at its own top edge"
        );
        let prev = (i > 0).then(|| hs[i - 1]);
        assert_eq!(
            engine.heading_at_or_above(&doc, &hs, y - 0.5),
            prev,
            "half a pixel below the top edge must fall back to the previous heading"
        );
    }

    assert_eq!(engine.heading_at_or_above(&doc, &hs, -1.0), None);
    assert_eq!(
        engine.heading_at_or_above(&doc, &hs, engine.total_height() + 10.0),
        Some(hs[2]),
        "a reference line above the whole document must stop at the last heading"
    );
    assert_eq!(engine.heading_at_or_above(&doc, &[], 100.0), None);
}

#[test]
fn heading_at_or_above_falls_back_to_a_collapsed_ancestor() {
    let mut md = String::from("# a\n\npara a\n\n");
    for i in 0..110 {
        let _ = writeln!(md, "filler {i} {}\n", "word ".repeat(8));
    }
    md.push_str("> ## h1\n>\n> ## h2\n\n# b\n\npara b\n");
    let doc = loaded(&md);
    let mut engine = IncrementalEngine::new(
        &doc,
        BoxLayoutEnvironment::default(),
        estimator(),
        dummy_layout(),
    );
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &FallbackSolver);

    let hs = headings(&doc);
    assert_eq!(hs.len(), 4, "# a, two inside the quote, and # b");
    let [a, h1, h2, b] = hs[..] else {
        panic!("heading order is wrong: {hs:?}");
    };
    let quote = doc
        .preorder()
        .iter()
        .find(|id| {
            doc.arena
                .get(**id)
                .is_some_and(|n| n.kind == BlockKind::BlockQuote)
        })
        .expect("the quote block")
        .index;

    let ay = engine
        .content_top(a)
        .expect("# a should be on the first screen");
    assert_eq!(
        engine.content_top(h1),
        None,
        "headings inside the collapsed quote have no exact item"
    );
    assert_eq!(engine.content_top(h2), None);
    let by = engine
        .content_top(b)
        .expect("# b is a top-level leaf, its item is always there");

    let qy = engine
        .spine
        .collapsed_id(md_layout::box_tree::LayoutBoxId::frame(quote))
        .and_then(|item| engine.spine.item_top(item))
        .expect("the quote must have one collapsed item");

    assert_eq!(engine.heading_at_or_above(&doc, &hs, ay + 0.5), Some(a));
    assert_eq!(engine.heading_at_or_above(&doc, &hs, qy - 0.5), Some(a));
    assert_eq!(
        engine.heading_at_or_above(&doc, &hs, qy),
        Some(h2),
        "tied coarse values must pick the last heading in the container"
    );
    assert_eq!(
        engine.heading_at_or_above(&doc, &hs, by - 0.5),
        Some(h2),
        "a reference line past the quote but short of # b must still stop at the quote's last heading"
    );
    assert_eq!(engine.heading_at_or_above(&doc, &hs, by), Some(b));

    let anchor = engine.anchor_at_y(qy, &measure, &FallbackSolver);
    engine.assemble_incremental(anchor, 600.0, &measure, &FallbackSolver);
    let hy = engine
        .content_top(h1)
        .expect("after expansion, headings inside the quote must have exact items");
    assert_eq!(engine.heading_at_or_above(&doc, &hs, hy), Some(h1));
    assert_eq!(engine.heading_at_or_above(&doc, &hs, hy + 0.5), Some(h1));
    let hy2 = engine
        .content_top(h2)
        .expect("h2 must also have an exact item");
    assert_eq!(engine.heading_at_or_above(&doc, &hs, hy2), Some(h2));
}

#[test]
fn demoted_offscreen_heading_keeps_its_pickable_top() {
    let mut md = String::new();
    let _ = writeln!(md, "# first heading");
    for i in 0..120 {
        let _ = writeln!(md, "paragraph {i} {}\n", "word ".repeat(8));
    }
    let doc = loaded(&md);
    let vh: Px = 600.0;
    let mut engine = IncrementalEngine::with_window(
        &doc,
        BoxLayoutEnvironment::default(),
        estimator(),
        dummy_layout(),
        0.0,
        vh,
    );
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };

    let hs = headings(&doc);
    assert_eq!(hs.len(), 1);
    let y0 = engine
        .content_top(hs[0])
        .expect("the first-screen heading should have an exact item");

    for _ in 0..8 {
        let anchor = engine.anchor_at_y(
            (engine.total_height() - vh).max(0.0),
            &measure,
            &FallbackSolver,
        );
        engine.assemble_incremental(anchor, vh, &measure, &FallbackSolver);
    }
    assert_eq!(
        engine.content_top(hs[0]),
        None,
        "after scrolling away, the top heading must be demoted and have no Content item"
    );
    assert_eq!(
        engine.heading_at_or_above(&doc, &hs, y0),
        Some(hs[0]),
        "the demoted item stays in place; its old top edge must still pick it"
    );
    assert_eq!(
        engine.heading_at_or_above(&doc, &hs, y0 - 0.5),
        None,
        "there must be no heading before it"
    );
}
