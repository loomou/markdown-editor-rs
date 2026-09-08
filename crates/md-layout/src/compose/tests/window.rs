use super::support::layout;
use crate::box_tree::LayoutBoxId;
use crate::compose::{ComposeWindow, LeafMetrics, compose, compose_into, compose_window};
use crate::flow::HeightState;
use crate::spine::FlowSpine;
use md_core::document::{editor_options, load_markdown};
use std::fmt::Write;

fn metrics() -> LeafMetrics {
    LeafMetrics {
        line_height: 20.0,
        em_width: 16.0,
        heading1_mult: 1.0,
        heading_mult: 1.0,
        table_row_mult: 1.2,
        mermaid_max_height: 420.0,
        image_placeholder_height: 80.0,
        code_max_height: 420.0,
        math_max_height: 420.0,
        image_max_height: 720.0,
    }
}

fn many_paras_and_a_quote(n: usize) -> md_core::document::Document {
    let mut md = String::new();
    for i in 0..n {
        let _ = writeln!(md, "paragraph {i} {}\n", "word ".repeat(8));
    }
    let _ = writeln!(md, "> deep quoted findme");
    load_markdown(&md, editor_options())
}

fn window(bottom: md_core::Px) -> ComposeWindow {
    ComposeWindow {
        top: 0.0,
        bottom,
        avail_width: 800.0,
    }
}

#[test]
fn compose_window_skips_offscreen_top_level_boxes() {
    let doc = many_paras_and_a_quote(80);
    let theme = layout();
    let full = compose(&doc, &theme);
    let lazy = compose_window(&doc, &theme, window(80.0), &metrics());
    assert!(
        lazy.nodes().len() < full.nodes().len(),
        "lazy {} vs full {}",
        lazy.nodes().len(),
        full.nodes().len()
    );
    assert!(lazy.deferred_len() > 0);
    let root = lazy.get(lazy.root());
    let crate::box_tree::BoxChildren::Vertical(kids) = root.children() else {
        panic!("root stacks children");
    };
    assert_eq!(
        kids.len(),
        full.get(full.root()).children().as_vertical_len()
    );
}

trait VerticalLen {
    fn as_vertical_len(&self) -> usize;
}

impl VerticalLen for crate::box_tree::BoxChildren {
    fn as_vertical_len(&self) -> usize {
        match self {
            crate::box_tree::BoxChildren::Vertical(c) => c.len(),
            _ => panic!("expected vertical"),
        }
    }
}

#[test]
fn compose_window_box_count_does_not_track_document_length() {
    let theme = layout();
    let small = compose_window(
        &many_paras_and_a_quote(40),
        &theme,
        window(80.0),
        &metrics(),
    );
    let large = compose_window(
        &many_paras_and_a_quote(200),
        &theme,
        window(80.0),
        &metrics(),
    );
    assert!(
        large.nodes().len() < small.nodes().len() * 2 + 8,
        "small={} large={}",
        small.nodes().len(),
        large.nodes().len()
    );
}

#[test]
fn compose_into_builds_a_deferred_child() {
    let doc = many_paras_and_a_quote(80);
    let theme = layout();
    let mut tree = compose_window(&doc, &theme, window(80.0), &metrics());
    let quote = doc
        .preorder()
        .into_iter()
        .find(|&id| {
            doc.arena
                .get(id)
                .is_some_and(|n| n.kind == md_core::block::BlockKind::BlockQuote)
        })
        .expect("quote");
    let box_id = LayoutBoxId::for_kind(md_core::block::BlockKind::BlockQuote, quote.index);
    assert!(tree.nodes().get(&box_id).is_none());
    assert!(tree.deferred_height(box_id).is_some());
    compose_into(&mut tree, &doc, &theme, box_id);
    assert!(tree.nodes().get(&box_id).is_some());
    assert!(tree.deferred_height(box_id).is_none());
    let node = tree.get(box_id);
    match node.children() {
        crate::box_tree::BoxChildren::Vertical(c) => assert!(!c.is_empty()),
        other => panic!("quote should stack, got {other:?}"),
    }
}

#[test]
fn defer_composed_keeps_parent_child_slot_and_restores() {
    let doc = many_paras_and_a_quote(8);
    let theme = layout();
    let mut tree = compose(&doc, &theme);
    let root = tree.root();
    let kids = match tree.get(root).children() {
        crate::box_tree::BoxChildren::Vertical(c) => c.clone(),
        other => panic!("root stacks children, got {other:?}"),
    };
    let id = kids
        .iter()
        .copied()
        .find(|k| tree.nodes().get(k).is_some_and(|n| n.text_id.is_some()))
        .expect("interned child");
    let intern_before = tree.intern().len();
    let nodes_before = tree.nodes().len();
    let height = 24.0;
    crate::compose::defer_composed(&mut tree, id, height);
    assert_eq!(
        match tree.get(root).children() {
            crate::box_tree::BoxChildren::Vertical(c) => c.len(),
            _ => panic!("root stacks children"),
        },
        kids.len()
    );
    assert!(tree.nodes().get(&id).is_none());
    assert_eq!(tree.deferred_height(id), Some(height));
    assert!(tree.intern().len() < intern_before);
    assert!(tree.nodes().len() < nodes_before);
    compose_into(&mut tree, &doc, &theme, id);
    assert!(tree.nodes().contains_key(&id));
    assert!(tree.deferred_height(id).is_none());
}

#[test]
fn release_targets_never_pick_a_preview_box() {
    use crate::box_tree::BoxRole;
    use crate::flow::HeightState;
    use crate::spine::FlowSpine;
    use md_core::block::BlockKind;
    use md_core::document::{Caret, FocusBias};

    let mut doc = load_markdown(
        "> quoted intro\n\n```mermaid\nflowchart TD\n  A0-->B0\n```\n",
        editor_options(),
    );
    let block = doc
        .preorder()
        .into_iter()
        .find(|&id| {
            doc.arena
                .get(id)
                .is_some_and(|n| n.kind == BlockKind::Mermaid)
        })
        .expect("mermaid block")
        .index;
    doc.retarget_inline_focus_biased(Caret { block, offset: 0 }, FocusBias::Neutral);
    let theme = layout();
    let tree = compose(&doc, &theme);
    let preview = LayoutBoxId::preview(block);
    assert!(
        tree.nodes().contains_key(&preview),
        "edit mode should have a Preview box"
    );

    let spine = FlowSpine::flatten(&tree, 800.0, &|_, h| HeightState::Exact(h));
    let targets = spine.next_release_targets(&tree, 10_000.0, 10_100.0, None, &[], 64);
    assert!(
        !targets.is_empty(),
        "with nothing protected there should always be release targets"
    );
    for id in &targets {
        assert!(
            matches!(id.role, BoxRole::Frame | BoxRole::Cell),
            "the release targets must not contain {id:?}: preview/chrome ride with the host Frame's lifecycle"
        );
    }
    assert!(
        !targets.contains(&preview),
        "a Preview box made it into the release targets on its own"
    );
}

#[test]
fn flatten_keeps_deferred_collapsed_off_the_store() {
    let doc = many_paras_and_a_quote(80);
    let theme = layout();
    let tree = compose_window(&doc, &theme, window(80.0), &metrics());
    let spine = FlowSpine::flatten(&tree, 800.0, &|id, _avail| {
        HeightState::Estimated(tree.deferred_height(id).unwrap_or(20.0))
    });
    assert!(spine.len() > 1);
    let mut collapsed_without_box = 0usize;
    for pos in 0..spine.len() {
        let item = spine.item_at(pos);
        if let crate::spine::FlowItemKind::Collapsed { box_id } = item.kind
            && tree.nodes().get(&box_id).is_none()
        {
            collapsed_without_box += 1;
            assert!(tree.deferred_height(box_id).is_some());
        }
        if let crate::spine::FlowItemKind::Content { box_id } = item.kind {
            assert!(tree.nodes().contains_key(&box_id));
        }
    }
    assert!(collapsed_without_box > 0);
}

#[test]
fn window_tolerates_a_deferred_collapsed_in_view() {
    let doc = many_paras_and_a_quote(80);
    let theme = layout();
    let tree = compose_window(&doc, &theme, window(80.0), &metrics());
    let spine = FlowSpine::flatten(&tree, 800.0, &|id, _avail| {
        HeightState::Estimated(tree.deferred_height(id).unwrap_or(20.0))
    });
    let total = spine.total_height();
    let w = spine.window(&tree, 0.0, total, &[]);
    assert!(
        w.entries.iter().any(|e| {
            matches!(
                e.kind,
                crate::spine::FlowItemKind::Collapsed { box_id }
                    if tree.nodes().get(&box_id).is_none()
            )
        }),
        "fixture must put a deferred Collapsed in view"
    );
    assert!(
        w.container_spans.iter().any(|s| s.box_id == tree.root()),
        "root span must survive the deferred skip"
    );
}

#[test]
fn release_targets_scan_live_boxes_not_every_collapsed() {
    let doc = many_paras_and_a_quote(200);
    let theme = layout();
    let tree = compose_window(&doc, &theme, window(80.0), &metrics());
    let spine = FlowSpine::flatten(&tree, 800.0, &|id, _avail| {
        HeightState::Estimated(tree.deferred_height(id).unwrap_or(20.0))
    });
    let live = tree.nodes().live_nodes().count();
    assert!(live > 1);
    assert!(
        live < spine.len(),
        "live boxes must be the first screen, not the whole spine"
    );
    let total = spine.total_height();
    let ids = spine.next_release_targets(&tree, total - 40.0, total, None, &[], 8);
    assert!(!ids.is_empty());
    for id in &ids {
        assert!(tree.nodes().contains_key(id));
        assert!(tree.deferred_height(*id).is_none());
    }
}

#[test]
fn compose_still_builds_the_full_tree() {
    let doc = many_paras_and_a_quote(40);
    let theme = layout();
    let tree = compose(&doc, &theme);
    assert!(tree.deferred_len() == 0);
    assert!(tree.nodes().len() > 40);
}

fn constant_height(_id: LayoutBoxId, _avail: md_core::Px) -> HeightState {
    HeightState::Exact(10.0)
}

#[test]
fn editing_a_deferred_quote_refreshes_its_estimate() {
    use md_core::document::{Caret, Command, Sel, apply};

    let mut doc = load_markdown("head\n\n> a\n", editor_options());
    let theme = layout();
    let mut tree = compose_window(&doc, &theme, window(10.0), &metrics());
    let quote = doc
        .preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).unwrap().kind == md_core::block::BlockKind::BlockQuote)
        .unwrap();
    let quote_box = LayoutBoxId::frame(quote.index);
    let before = tree.deferred_height(quote_box).expect("deferred quote");
    let block = doc.text_leaves()[1];
    let _ = doc.take_changes();
    apply(
        &mut doc,
        Sel::collapsed(Caret { block, offset: 1 }),
        Command::Insert {
            text: "a".repeat(1_000),
        },
    );
    let changes = doc.take_changes();
    crate::compose::sync_layout(&mut tree, &doc, &changes, &theme);
    let fresh = compose_window(&doc, &theme, window(10.0), &metrics());
    assert_eq!(
        tree.deferred_height(quote_box),
        fresh.deferred_height(quote_box),
        "a cold subtree edit must invalidate or refresh its old estimate"
    );
    assert!(
        tree.deferred_height(quote_box).unwrap_or(0.0) > before,
        "the re-estimate must react to the new document"
    );
}

#[test]
fn windowed_height_of_a_block_editing_leaf_includes_its_preview() {
    use md_core::document::{Caret, FocusBias};

    let plain = load_markdown("head\n\n$$\nx\n$$\n", editor_options());
    let block = plain
        .text_leaves()
        .into_iter()
        .find(|&b| plain.kind(b) == Some(md_core::block::BlockKind::Math))
        .unwrap();
    let frame = LayoutBoxId::frame(block);
    let theme = layout();
    let cold = compose_window(&plain, &theme, window(10.0), &metrics());
    let plain_height = cold.deferred_height(frame).expect("deferred math");

    let mut doc = load_markdown("head\n\n$$\nx\n$$\n", editor_options());
    doc.retarget_inline_focus_biased(Caret { block, offset: 0 }, FocusBias::Neutral);
    assert_eq!(doc.block_edit(), Some(block));
    let hot = compose_window(&doc, &theme, window(10.0), &metrics());
    let edit_height = hot.deferred_height(frame).expect("deferred editing math");
    assert!(
        edit_height > plain_height,
        "the estimate must include the source frame plus the preview ({edit_height} vs {plain_height})"
    );
    let again = compose_window(&doc, &theme, window(10.0), &metrics());
    assert_eq!(again.deferred_height(frame), Some(edit_height));
}

#[test]
fn realizing_a_cold_editing_block_includes_its_preview() {
    use md_core::document::{Caret, FocusBias};

    let mut doc = load_markdown("head\n\n$$\nx\n$$\n", editor_options());
    let block = doc
        .text_leaves()
        .into_iter()
        .find(|&b| doc.kind(b) == Some(md_core::block::BlockKind::Math))
        .unwrap();
    doc.retarget_inline_focus_biased(Caret { block, offset: 0 }, FocusBias::Neutral);
    assert_eq!(doc.block_edit(), Some(block));
    let frame = LayoutBoxId::frame(block);
    let preview = LayoutBoxId::preview(block);
    let theme = layout();
    let mut tree = compose_window(&doc, &theme, window(10.0), &metrics());
    assert!(tree.deferred_height(frame).is_some());
    let mut spine = FlowSpine::flatten(&tree, 800.0, &constant_height);
    compose_into(&mut tree, &doc, &theme, frame);
    assert!(
        tree.nodes().contains_key(&preview),
        "fixture: realized preview"
    );
    let _ = spine.expand_visible(&tree, 0.0, 1_000.0, &constant_height);
    assert!(
        spine.content_id(preview).is_some(),
        "realizing a cold editing block must lower every emitted materializable box"
    );
    let cold_tree = compose(&doc, &theme);
    let cold = FlowSpine::flatten(&cold_tree, 800.0, &constant_height);
    assert_eq!(spine.total_height(), cold.total_height());
}

#[test]
fn deep_nested_quotes_compose_without_recursion() {
    let depth = 1024;
    let mut md = "> ".repeat(depth);
    md.push_str("x\n");
    let doc = load_markdown(&md, editor_options());
    let theme = layout();
    let tree = compose(&doc, &theme);
    assert!(tree.nodes().len() >= depth, "nodes={}", tree.nodes().len());
    let _ = compose_window(&doc, &theme, window(10.0), &metrics());
}
