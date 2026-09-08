use super::support::{CountingMeasure, dummy_layout, estimator};
use crate::incremental::anchor::ScrollAnchor;
use crate::incremental::engine::IncrementalEngine;
use crate::incremental::store::EvictionPolicy;
use md_core::block::BlockKind;
use md_core::document::{editor_options, load_markdown};
use md_layout::box_tree::LayoutBoxId;
use md_layout::island::FallbackSolver;
use md_layout::spine::FlowItemKind;
use md_layout::style::BoxLayoutEnvironment;
use std::cell::Cell;
use std::fmt::Write;

fn n_paras(n: usize) -> md_core::document::Document {
    let mut md = String::new();
    for i in 0..n {
        let _ = writeln!(md, "paragraph {i} {}\n", "word ".repeat(8));
    }
    load_markdown(&md, editor_options())
}

fn paras_then_quote(n: usize) -> md_core::document::Document {
    let mut md = String::new();
    for i in 0..n {
        let _ = writeln!(md, "paragraph {i} {}\n", "word ".repeat(8));
    }
    let _ = writeln!(md, "> findme quoted");
    load_markdown(&md, editor_options())
}

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
fn open_window_box_count_does_not_track_document_length() {
    let env = BoxLayoutEnvironment::default();
    let a =
        IncrementalEngine::with_window(&n_paras(80), env, estimator(), dummy_layout(), 0.0, 600.0);
    let b =
        IncrementalEngine::with_window(&n_paras(320), env, estimator(), dummy_layout(), 0.0, 600.0);
    let na = a.tree.nodes().len();
    let nb = b.tree.nodes().len();
    assert!(na > 1);
    assert!(
        nb < na * 2 + 16,
        "box_nodes should follow the first screen, not document length: {na} vs {nb}"
    );
}

#[test]
fn open_window_first_screen_is_exact() {
    let doc = n_paras(80);
    let env = BoxLayoutEnvironment::default();
    let mut engine =
        IncrementalEngine::with_window(&doc, env, estimator(), dummy_layout(), 0.0, 1800.0);
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    let vh = 600.0;
    engine.assemble_with_doc(&doc, ScrollAnchor::top(), vh, &measure, &solver);
    let exact = engine.spine.visible(0.0, vh).all(|pos| {
        let item = engine.spine.item_at(pos);
        match item.kind {
            FlowItemKind::Content { .. } => engine.spine.effective_height(pos).is_exact(),
            FlowItemKind::Collapsed { .. } => false,
            _ => true,
        }
    });
    assert!(exact);
}

#[test]
fn ensure_composed_block_builds_an_offscreen_quote() {
    let doc = paras_then_quote(80);
    let env = BoxLayoutEnvironment::default();
    let mut engine =
        IncrementalEngine::with_window(&doc, env, estimator(), dummy_layout(), 0.0, 80.0);
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_with_doc(&doc, ScrollAnchor::top(), 600.0, &measure, &solver);
    let target = leaf_containing(&doc, "findme");
    assert!(!engine.debug_block_on_spine(target));
    assert!(engine.ensure_composed_block(&doc, target, &measure, &solver));
    assert!(engine.debug_block_on_spine(target));
    assert!(engine.debug_block_top(target).is_some());
}

fn nested_quotes(n: usize) -> md_core::document::Document {
    let mut md = String::new();
    for i in 0..n {
        let _ = writeln!(md, "> > quote {i} with some words to measure\n");
    }
    load_markdown(&md, editor_options())
}

fn wrapped_paras(n: usize) -> md_core::document::Document {
    let mut md = String::new();
    for i in 0..n {
        let _ = writeln!(md, "paragraph {i} {}\n", "word ".repeat(120));
    }
    load_markdown(&md, editor_options())
}

#[test]
fn jump_settles_with_no_deferred_collapsed_in_the_published_window() {
    let doc = wrapped_paras(160);
    let env = BoxLayoutEnvironment::default();
    let mut engine =
        IncrementalEngine::with_window(&doc, env, estimator(), dummy_layout(), 0.0, 600.0);
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    let vh = 600.0;
    engine.assemble_with_doc(&doc, ScrollAnchor::top(), vh, &measure, &solver);

    let total = engine.total_height();
    for frac in [0.3, 0.5, 0.7] {
        let sa = engine.anchor_at_y(total * frac, &measure, &solver);
        let (assembly, published) = engine.assemble_with_doc(&doc, sa, vh, &measure, &solver);
        let window = assembly.window.as_ref().expect("published window");
        for e in &window.entries {
            if let FlowItemKind::Collapsed { box_id } = e.kind {
                assert!(
                    engine.tree.nodes().contains_key(&box_id),
                    "frac={frac}: deferred Collapsed leaked into the published window at top={}",
                    e.top
                );
            }
        }
        assert!(
            visible_content_is_exact(&engine, published.resolved_top, vh),
            "frac={frac}: published window must be exact"
        );
    }
}

fn first_quote_box(doc: &md_core::document::Document) -> LayoutBoxId {
    let quote = doc
        .preorder()
        .into_iter()
        .find(|&id| {
            doc.arena
                .get(id)
                .is_some_and(|n| n.kind == BlockKind::BlockQuote)
        })
        .expect("quote");
    LayoutBoxId::for_kind(BlockKind::BlockQuote, quote.index)
}

fn visible_content_is_exact(engine: &IncrementalEngine, top: md_core::Px, vh: md_core::Px) -> bool {
    engine.spine.visible(top, top + vh).all(|pos| {
        let item = engine.spine.item_at(pos);
        match item.kind {
            FlowItemKind::Content { .. } => engine.spine.effective_height(pos).is_exact(),
            FlowItemKind::Collapsed { .. } => false,
            _ => true,
        }
    })
}

#[test]
fn scrolling_away_drops_offscreen_boxes_and_scroll_back_is_exact() {
    let doc = nested_quotes(200);
    let env = BoxLayoutEnvironment::default();
    let mut engine =
        IncrementalEngine::with_window(&doc, env, estimator(), dummy_layout(), 0.0, 1800.0);
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    let vh = 600.0;
    engine.assemble_with_doc(&doc, ScrollAnchor::top(), vh, &measure, &solver);

    let quote = first_quote_box(&doc);
    assert!(engine.tree.nodes().contains_key(&quote));
    let height = engine.total_height();
    let intern_top = engine.tree.intern().len();
    let nodes_top = engine.tree.nodes().len();

    let dest = (engine.total_height() - 1.0).max(0.0);
    let mut y = dest;
    let mut published = {
        let sa = engine.anchor_at_y(y, &measure, &solver);
        engine.assemble_with_doc(&doc, sa, vh, &measure, &solver).1
    };
    y = published.resolved_top;
    for _ in 0..48 {
        let sa = engine.anchor_at_y(y, &measure, &solver);
        published = engine.assemble_with_doc(&doc, sa, vh, &measure, &solver).1;
        y = published.resolved_top;
    }

    assert_eq!(engine.total_height(), height);
    assert!(
        published.resolved_top > height * 0.5,
        "idle frames must stay at the far end, not snap to top: {}",
        published.resolved_top
    );
    assert!(
        engine.tree.nodes().get(&quote).is_none(),
        "scrolled-away quote must drop its boxes"
    );
    assert!(engine.tree.deferred_height(quote).is_some());
    assert!(
        engine.tree.nodes().len() < nodes_top,
        "box_nodes should fall after idle away from the first screen: top={nodes_top} far={}",
        engine.tree.nodes().len()
    );
    assert!(
        engine.tree.intern().len() < intern_top,
        "intern slots must be released, not Vec::remove"
    );
    assert!(visible_content_is_exact(
        &engine,
        published.resolved_top,
        vh
    ));

    let (_, published) = engine.assemble_with_doc(&doc, ScrollAnchor::top(), vh, &measure, &solver);
    assert!(engine.tree.nodes().contains_key(&quote));
    assert!(engine.tree.deferred_height(quote).is_none());
    assert_eq!(engine.total_height(), height);
    assert!(visible_content_is_exact(
        &engine,
        published.resolved_top,
        vh
    ));
}

fn paras_with_mermaid_every(n: usize, every: usize) -> md_core::document::Document {
    let mut md = String::new();
    for i in 0..n {
        if i % every == 0 {
            let _ = writeln!(md, "```mermaid\nflowchart TD\nA{i}-->B{i}\n```\n");
        }
        let _ = writeln!(md, "paragraph {i} {}\n", "word ".repeat(8));
    }
    load_markdown(&md, editor_options())
}

#[test]
fn warm_media_reaches_past_the_visible_window() {
    let doc = paras_with_mermaid_every(120, 6);
    let env = BoxLayoutEnvironment::default();
    let mut engine =
        IncrementalEngine::with_window(&doc, env, estimator(), dummy_layout(), 0.0, 1800.0);
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    let vh = 600.0;
    let sa = engine.anchor_at_y(4_000.0, &measure, &solver);
    let (_, published) = engine.assemble_with_doc(&doc, sa, vh, &measure, &solver);
    let top = published.resolved_top;

    let warm = engine.warm_media_blocks(top, vh);
    assert!(!warm.is_empty(), "the warm window must include a diagram");
    assert!(
        warm.iter().all(|(_, kind, _)| *kind == BlockKind::Mermaid),
        "the fixture holds only mermaid; no other kind may sneak in"
    );
    assert!(
        warm.iter().all(|(_, _, width)| *width > 0.0),
        "the width is part of the cache key: a zero width would raster a bitmap no one can look up"
    );

    let onscreen: Vec<_> = engine
        .spine
        .visible(top, top + vh)
        .filter_map(|pos| match engine.spine.item_at(pos).kind {
            FlowItemKind::Content { box_id } => box_id.block(),
            _ => None,
        })
        .collect();
    let offscreen = warm
        .iter()
        .filter(|(block, _, _)| !onscreen.contains(block))
        .count();
    assert!(
        offscreen > 0,
        "the warm window must reach off-screen, or nothing was prefetched: warm={} onscreen={}",
        warm.len(),
        onscreen.len()
    );
}

#[test]
fn warm_media_is_empty_without_a_window() {
    let doc = paras_with_mermaid_every(40, 6);
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc, env, estimator(), dummy_layout());
    engine.eviction = EvictionPolicy::unbounded();
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_with_doc(&doc, ScrollAnchor::top(), 600.0, &measure, &solver);
    assert!(engine.warm_media_blocks(0.0, 600.0).is_empty());
}

#[test]
fn warm_media_reaches_below_the_viewport() {
    let doc = paras_with_mermaid_every(120, 6);
    let env = BoxLayoutEnvironment::default();
    let mut engine =
        IncrementalEngine::with_window(&doc, env, estimator(), dummy_layout(), 0.0, 1800.0);
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    let vh = 600.0;
    let sa = engine.anchor_at_y(4_000.0, &measure, &solver);
    let (_, published) = engine.assemble_with_doc(&doc, sa, vh, &measure, &solver);
    let top = published.resolved_top;

    let warm = engine.warm_media_blocks(top, vh);
    let mut y_of = std::collections::HashMap::new();
    for pos in engine.spine.visible(0.0, f64::MAX) {
        let item = engine.spine.item_at(pos);
        let (FlowItemKind::Content { box_id } | FlowItemKind::Collapsed { box_id }) = item.kind
        else {
            continue;
        };
        let (Some(block), Some(y)) = (box_id.block(), engine.spine.item_top(item.id)) else {
            continue;
        };
        y_of.entry(block).or_insert(y);
    }
    let mut above = 0usize;
    let mut below = 0usize;
    for (block, _, _) in &warm {
        let y = *y_of
            .get(block)
            .expect("blocks reported by the warm window should be on the spine");
        if y < top {
            above += 1;
        } else if y >= top + vh {
            below += 1;
        }
    }
    assert!(
        below > 0,
        "scrolling down must prefetch the two screens below, yet none were reported: above={above} below={below} warm={}",
        warm.len()
    );
}

#[test]
fn warm_realize_spends_its_quota_ahead_of_the_scroll() {
    let doc = paras_with_mermaid_every(160, 6);
    let env = BoxLayoutEnvironment::default();
    let mut engine =
        IncrementalEngine::with_window(&doc, env, estimator(), dummy_layout(), 0.0, 1800.0);
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    let vh = 600.0;
    let sa = engine.anchor_at_y(6_000.0, &measure, &solver);
    let published = engine.assemble_with_doc(&doc, sa, vh, &measure, &solver).1;
    let top = published.resolved_top;

    let mut boxed = 0usize;
    let mut deferred_only = 0usize;
    for pos in engine.spine.visible(top + vh, top + vh + 2.0 * vh) {
        let (FlowItemKind::Content { box_id } | FlowItemKind::Collapsed { box_id }) =
            engine.spine.item_at(pos).kind
        else {
            continue;
        };
        if engine.tree.nodes().contains_key(&box_id) {
            boxed += 1;
        } else {
            deferred_only += 1;
        }
    }
    assert!(
        boxed > 0,
        "the jump frame built no box below; the quota all went above: boxed={boxed} deferred_only={deferred_only}"
    );
}

#[test]
fn cold_subtree_text_edit_updates_the_spine_estimate() {
    use md_core::document::{Caret, Command, Sel, apply};

    let mut doc = paras_then_quote(80);
    let env = BoxLayoutEnvironment::default();
    let mut engine =
        IncrementalEngine::with_window(&doc, env, estimator(), dummy_layout(), 0.0, 80.0);
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_with_doc(&doc, ScrollAnchor::top(), 600.0, &measure, &solver);
    let target = leaf_containing(&doc, "findme");
    let quote_node = doc
        .preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).unwrap().kind == BlockKind::BlockQuote)
        .expect("the quote container");
    let quote_box = LayoutBoxId::frame(quote_node.index);
    assert!(
        engine.tree.deferred_height(quote_box).is_some(),
        "fixture: the quote must still be cold"
    );
    let item = engine
        .spine
        .collapsed_id(quote_box)
        .expect("a pending collapsed item");
    let before_total = engine.spine.total_height();

    let _ = doc.take_changes();
    apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: target,
            offset: 2,
        }),
        Command::Insert {
            text: "tail ".repeat(400),
        },
    );
    let changes = doc.take_changes();
    engine.apply_changes(&doc, &changes);

    let after_total = engine.spine.total_height();
    assert!(
        after_total > before_total,
        "the collapsed estimate must follow the edit ({after_total} vs {before_total})"
    );
    let spine_h = engine
        .spine
        .get(item)
        .expect("the collapsed item survives")
        .height
        .px();
    let tree_h = engine
        .tree
        .deferred_height(quote_box)
        .expect("re-estimated on the tree side");
    assert_eq!(spine_h, tree_h, "spine and tree must agree on the estimate");
}

#[test]
fn ensure_composed_block_lowers_the_cold_editing_preview() {
    use md_core::document::{Caret, FocusBias};
    use std::fmt::Write as _;

    let mut md = String::new();
    for i in 0..80 {
        let _ = writeln!(md, "paragraph {i} {}\n", "word ".repeat(8));
    }
    md.push_str("$$\nx\n$$\n");
    let mut doc = load_markdown(&md, editor_options());
    let block = doc
        .text_leaves()
        .into_iter()
        .find(|&b| doc.kind(b) == Some(BlockKind::Math))
        .expect("the math block");
    doc.retarget_inline_focus_biased(Caret { block, offset: 0 }, FocusBias::Neutral);
    assert_eq!(doc.block_edit(), Some(block));

    let env = BoxLayoutEnvironment::default();
    let mut engine =
        IncrementalEngine::with_window(&doc, env, estimator(), dummy_layout(), 0.0, 80.0);
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    engine.assemble_with_doc(&doc, ScrollAnchor::top(), 600.0, &measure, &solver);
    let preview = LayoutBoxId::preview(block);
    assert!(
        !engine.tree.nodes().contains_key(&preview),
        "fixture: the editing math block must still be cold"
    );
    assert!(engine.ensure_composed_block(&doc, block, &measure, &solver));
    assert!(
        engine.spine.content_id(preview).is_some(),
        "the preview must ride the spine with its frame"
    );
}
