use super::support::{CountingMeasure, dummy_layout, estimator};
use crate::incremental::anchor::ScrollAnchor;
use crate::incremental::engine::{IncrementalEngine, MAX_ITERATIONS};
use md_core::Px;
use md_core::block::{BlockId, BlockKind};
use md_core::document::{Caret, Document, FocusBias, editor_options, load_markdown};
use md_layout::box_tree::LayoutBoxId;
use md_layout::compose::compose;
use md_layout::island::FallbackSolver;
use md_layout::shaper::{MeasureKind, MeasureResult, ShapeIdentity, TextMeasure};
use md_layout::spine::FlowItemKind;
use md_layout::style::BoxLayoutEnvironment;
use std::cell::Cell;

const MERMAID_MD: &str = "before\n\n```mermaid\nflowchart TD\nA-->B\nB-->C\n```\n\nafter\n";
const MATH_MD: &str = "before\n\n$$\n\\frac{a}{b}\n$$\n\nafter\n";
const IMAGE_MD: &str = "before\n\n![a](u)\n\nafter\n";

fn cases() -> [(&'static str, BlockKind, Px); 3] {
    [
        (MERMAID_MD, BlockKind::Mermaid, 3.0),
        (MATH_MD, BlockKind::Math, 1.0),
        (IMAGE_MD, BlockKind::Image, 1.0),
    ]
}

fn block_of(doc: &Document, kind: BlockKind) -> BlockId {
    let mut found = None;
    doc.for_each_text_leaf(|id, _| {
        if doc.kind(id) == Some(kind) {
            found = Some(id);
            false
        } else {
            true
        }
    });
    found.unwrap_or_else(|| panic!("no {kind:?} block"))
}

fn enter(doc: &mut Document, block: BlockId) {
    doc.retarget_inline_focus_biased(Caret { block, offset: 0 }, FocusBias::Neutral);
}

fn engine_for(doc: &Document) -> IncrementalEngine {
    IncrementalEngine::new(
        doc,
        BoxLayoutEnvironment::default(),
        estimator(),
        dummy_layout(),
    )
}

#[test]
fn parse_fixtures_have_the_expected_kinds() {
    for (md, kind, _) in cases() {
        let doc = load_markdown(md, editor_options());
        let block = block_of(&doc, kind);
        assert_eq!(doc.kind(block), Some(kind), "md={md:?}");
    }
}

#[test]
fn caret_in_block_attaches_preview_box() {
    for (md, kind, _) in cases() {
        let mut doc = load_markdown(md, editor_options());
        let block = block_of(&doc, kind);
        assert_eq!(doc.block_edit(), None);

        let mut engine = engine_for(&doc);
        let preview = LayoutBoxId::preview(block);
        assert!(!engine.tree.nodes().contains_key(&preview), "{kind:?}");

        enter(&mut doc, block);
        assert_eq!(doc.block_edit(), Some(block), "{kind:?}");
        assert!(engine.sync_block_edit(&doc), "{kind:?}");

        assert!(engine.tree.nodes().contains_key(&preview), "{kind:?}");

        assert!(engine.tree.get(LayoutBoxId::frame(block)).edit_source());
        assert!(!engine.tree.get(preview).edit_source());

        assert!(engine.spine.content_id(preview).is_some(), "{kind:?}");

        assert!(!engine.sync_block_edit(&doc), "{kind:?}");
    }
}

#[test]
fn media_completion_invalidates_frame_and_preview() {
    for (md, kind, _) in cases() {
        let mut doc = load_markdown(md, editor_options());
        let block = block_of(&doc, kind);
        let mut engine = engine_for(&doc);
        let measure = CountingMeasure {
            calls: Cell::new(0),
        };

        enter(&mut doc, block);
        engine.sync_block_edit(&doc);
        engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &FallbackSolver);

        let frame = LayoutBoxId::frame(block);
        let preview = LayoutBoxId::preview(block);
        for island in [frame, preview] {
            assert!(engine.store.is_materialized(island), "{kind:?} {island:?}");
        }

        engine.invalidate_island_for_block(block);

        for island in [frame, preview] {
            assert!(!engine.store.is_materialized(island), "{kind:?} {island:?}");
            let item = engine.spine.content_id(island).expect("content item");
            let pos = engine.spine.location(item).expect("content position");
            assert!(
                !engine.spine.effective_height(pos).is_exact(),
                "{kind:?} {island:?}"
            );
        }
    }
}

#[test]
fn leaving_block_detaches_preview_box() {
    for (md, kind, _) in cases() {
        let mut doc = load_markdown(md, editor_options());
        let block = block_of(&doc, kind);
        let mut engine = engine_for(&doc);

        enter(&mut doc, block);
        engine.sync_block_edit(&doc);

        let para = block_of(&doc, BlockKind::Paragraph);
        enter(&mut doc, para);
        assert_eq!(doc.block_edit(), None, "{kind:?}");
        assert!(engine.sync_block_edit(&doc), "{kind:?}");

        let preview = LayoutBoxId::preview(block);
        assert!(!engine.tree.nodes().contains_key(&preview), "{kind:?}");
        assert!(engine.spine.content_id(preview).is_none(), "{kind:?}");
        assert!(!engine.tree.get(LayoutBoxId::frame(block)).edit_source());
    }
}

#[test]
fn image_frame_shapes_source_while_preview_shapes_alt() {
    let mut doc = load_markdown(IMAGE_MD, editor_options());
    let block = block_of(&doc, BlockKind::Image);
    let mut engine = engine_for(&doc);
    let _ = doc.take_changes();

    enter(&mut doc, block);
    engine.sync_block_edit(&doc);
    assert_eq!(engine.tree.text(LayoutBoxId::frame(block)), "![a](u)");
    assert_eq!(engine.tree.text(LayoutBoxId::preview(block)), "a");

    doc.replace_text(block, 3..3, "b");
    let changes = doc.take_changes();
    let settle = engine.apply_changes(&doc, &changes);
    assert_eq!(
        settle.invalidated,
        vec![LayoutBoxId::frame(block), LayoutBoxId::preview(block)]
    );
    assert_eq!(engine.tree.text(LayoutBoxId::frame(block)), "![ab](u)");
    assert_eq!(engine.tree.text(LayoutBoxId::preview(block)), "ab");
}

fn shape(t: &md_layout::box_tree::BoxTree) -> Vec<(LayoutBoxId, String)> {
    let mut v = Vec::new();
    for (id, n) in t.nodes() {
        v.push((
            *id,
            format!(
                "{:?}|{:?}|{}|{:?}",
                n.parent(),
                n.kind(),
                n.edit_source(),
                n.children()
            ),
        ));
    }
    v
}

#[test]
fn hot_tree_matches_cold_compose_in_edit_state() {
    for (md, kind, _) in cases() {
        let mut doc = load_markdown(md, editor_options());
        let block = block_of(&doc, kind);
        let mut engine = engine_for(&doc);

        enter(&mut doc, block);
        engine.sync_block_edit(&doc);
        assert_eq!(
            shape(&engine.tree),
            shape(&compose(&doc, &dummy_layout())),
            "{kind:?}"
        );

        let para = block_of(&doc, BlockKind::Paragraph);
        enter(&mut doc, para);
        engine.sync_block_edit(&doc);
        assert_eq!(
            shape(&engine.tree),
            shape(&compose(&doc, &dummy_layout())),
            "{kind:?} after leave"
        );
    }
}

#[test]
fn settling_a_broken_image_drops_the_preview_box() {
    let mut doc = load_markdown(IMAGE_MD, editor_options());
    let block = block_of(&doc, BlockKind::Image);
    let para = block_of(&doc, BlockKind::Paragraph);
    let mut engine = engine_for(&doc);

    enter(&mut doc, block);
    engine.sync_block_edit(&doc);
    doc.replace_text(block, 0..7, "hello");
    let changes = doc.take_changes();
    engine.apply_changes(&doc, &changes);

    enter(&mut doc, para);
    let changes = doc.take_changes();
    engine.apply_changes(&doc, &changes);
    engine.sync_block_edit(&doc);

    assert_eq!(doc.kind(block), Some(BlockKind::Paragraph));
    let frame = LayoutBoxId::frame(block);
    assert_eq!(engine.tree.get(frame).kind(), BlockKind::Paragraph);
    assert!(!engine.tree.get(frame).edit_source());
    assert_eq!(engine.tree.text(frame), "hello");
    let preview = LayoutBoxId::preview(block);
    assert!(!engine.tree.nodes().contains_key(&preview));
    assert!(engine.spine.content_id(preview).is_none());
    assert_eq!(shape(&engine.tree), shape(&compose(&doc, &dummy_layout())));
}

#[test]
fn toggling_edit_state_settles_without_full_clear() {
    for (md, kind, _) in cases() {
        let mut doc = load_markdown(md, editor_options());
        let block = block_of(&doc, kind);
        let para = block_of(&doc, BlockKind::Paragraph);
        let mut engine = engine_for(&doc);
        let measure = CountingMeasure {
            calls: Cell::new(0),
        };
        let solver = FallbackSolver;
        engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
        let rebuilds = engine.doc_rebuilds();

        for _ in 0..3 {
            for target in [block, para] {
                enter(&mut doc, target);
                engine.sync_block_edit(&doc);
                let (_, p) =
                    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
                assert!(
                    p.iterations < MAX_ITERATIONS,
                    "{kind:?} iterations={}",
                    p.iterations
                );
            }
        }
        assert_eq!(engine.doc_rebuilds(), rebuilds, "{kind:?}");
    }
}

#[test]
fn edit_state_frame_estimates_by_line_count() {
    let layout = dummy_layout();
    let e = estimator();
    for (md, kind, rows) in cases() {
        let mut doc = load_markdown(md, editor_options());
        let block = block_of(&doc, kind);
        let frame = LayoutBoxId::frame(block);

        let cold = compose(&doc, &layout);
        let rendered = e.estimate(&cold, frame, 800.0);

        enter(&mut doc, block);
        let hot = compose(&doc, &layout);
        assert_eq!(
            e.estimate(&hot, frame, 800.0),
            rows * 20.0,
            "{kind:?} frame"
        );
        assert_eq!(
            e.estimate(&hot, LayoutBoxId::preview(block), 800.0),
            rendered,
            "{kind:?} preview"
        );
    }
}

struct RenderedMeasure;

const RENDERED_H: Px = 80.0;
const LINE_H: Px = 20.0;

impl TextMeasure for RenderedMeasure {
    fn begin_island(&self) {}

    fn measure(
        &self,
        text: &str,
        _runs: &[md_core::inline::InlineRun],
        avail_width: Px,
        _kind: MeasureKind,
        block_kind: BlockKind,
        ident: ShapeIdentity,
    ) -> MeasureResult {
        let height = if block_kind.supports_block_edit() && !ident.edit_source {
            RENDERED_H
        } else {
            (text.lines().count() as Px).max(1.0) * LINE_H
        };
        MeasureResult {
            width: avail_width,
            height,
            rows: 1,
            first_baseline: 16.0,
        }
    }
}

fn last_block_of(doc: &Document, kind: BlockKind) -> BlockId {
    let mut found = None;
    doc.for_each_text_leaf(|id, _| {
        if doc.kind(id) == Some(kind) {
            found = Some(id);
        }
        true
    });
    found.unwrap_or_else(|| panic!("no {kind:?} block"))
}

fn tall_mermaid_md() -> String {
    let mut md = String::new();
    for i in 0..30 {
        md.push_str(&format!("para {i}\n\n"));
    }
    md.push_str("```mermaid\nflowchart TD\n");
    for i in 0..40 {
        md.push_str(&format!("N{i}-->N{}\n", i + 1));
    }
    md.push_str("```\n\nafter\n");
    md
}

#[test]
fn leaving_edit_forgets_source_height() {
    let mut doc = load_markdown(&tall_mermaid_md(), editor_options());
    let mermaid = block_of(&doc, BlockKind::Mermaid);
    let after = last_block_of(&doc, BlockKind::Paragraph);
    let mut engine = engine_for(&doc);
    let measure = RenderedMeasure;
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);

    enter(&mut doc, mermaid);
    engine.sync_block_edit(&doc);
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);

    let mermaid_top = engine.content_top(mermaid).expect("mermaid");
    let after_top = engine.content_top(after).expect("after");
    let gap = after_top - mermaid_top;
    assert!(
        gap > 600.0,
        "edit-state mermaid should be tall source+preview, gap={gap}"
    );

    enter(&mut doc, after);
    engine.sync_block_edit(&doc);

    let after_top = engine.content_top(after).expect("after");
    let mermaid_top = engine.content_top(mermaid).expect("mermaid");
    let collapsed = after_top - mermaid_top;
    assert!(
        collapsed < gap * 0.6,
        "source height must not linger as estimate: before={gap} after={collapsed}"
    );
}

#[test]
fn leaving_tall_mermaid_keeps_following_paragraph_in_place() {
    let mut doc = load_markdown(&tall_mermaid_md(), editor_options());
    let mermaid = block_of(&doc, BlockKind::Mermaid);
    let after = last_block_of(&doc, BlockKind::Paragraph);
    let mut engine = engine_for(&doc);
    let measure = RenderedMeasure;
    let solver = FallbackSolver;
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);

    enter(&mut doc, mermaid);
    engine.sync_block_edit(&doc);
    engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);

    let after_top = engine.content_top(after).expect("after");
    let hold = 150.0;
    let scroll = after_top - hold;

    enter(&mut doc, after);
    let new_scroll = engine.sync_block_edit_retain_y(&doc, after, scroll, &measure, &solver);
    let new_top = engine.content_top(after).expect("after");

    assert!(
        new_top < after_top - 400.0,
        "paragraph should move up with the source collapse: {after_top} -> {new_top}"
    );
    assert!(
        (new_top - new_scroll - hold).abs() < 1.0,
        "viewport offset of the paragraph must be kept: top={new_top} scroll={new_scroll}"
    );
}

#[test]
fn releasing_an_edited_blocks_frame_keeps_the_preview_item_consistent() {
    let mut doc = {
        let mut md = String::new();
        for i in 0..160 {
            md.push_str(&format!("para {i}\n\n"));
        }
        md.push_str("```mermaid\nflowchart TD\n");
        for i in 0..40 {
            md.push_str(&format!("N{i}-->N{}\n", i + 1));
        }
        md.push_str("```\n\nafter\n");
        load_markdown(&md, editor_options())
    };
    let mermaid = block_of(&doc, BlockKind::Mermaid);
    let mut engine = IncrementalEngine::with_window(
        &doc,
        BoxLayoutEnvironment::default(),
        estimator(),
        dummy_layout(),
        0.0,
        600.0,
    );
    let measure = RenderedMeasure;
    let solver = FallbackSolver;
    let vh = 600.0;
    let preview = LayoutBoxId::preview(mermaid);

    engine.assemble_with_doc(&doc, ScrollAnchor::top(), vh, &measure, &solver);
    let frame_top = spine_box_top(&engine, LayoutBoxId::frame(mermaid));
    let sa = engine.anchor_at_y(frame_top + 400.0, &measure, &solver);
    engine.assemble_with_doc(&doc, sa, vh, &measure, &solver);
    enter(&mut doc, mermaid);
    assert!(
        engine
            .tree
            .nodes()
            .contains_key(&LayoutBoxId::frame(mermaid)),
        "after scrolling there, the frame must be on the tree"
    );
    assert!(
        engine.sync_block_edit(&doc),
        "entering edit mode with the frame on the tree must attach the preview"
    );
    engine.assemble_with_doc(&doc, sa, vh, &measure, &solver);
    assert!(
        engine.tree.nodes().contains_key(&preview),
        "in edit mode the preview box must be on the tree"
    );
    let preview_top = spine_box_top(&engine, preview);

    for _ in 0..8 {
        engine.assemble_with_doc(&doc, ScrollAnchor::top(), vh, &measure, &solver);
    }
    for pos in 0..engine.spine.len() {
        let item = engine.spine.item_at(pos);
        let hit = match item.kind {
            FlowItemKind::Content { box_id } | FlowItemKind::Collapsed { box_id } => {
                box_id.owner == md_layout::box_tree::BoxOwner::Block(mermaid)
            }
            _ => false,
        };
        if hit {
            eprintln!(
                "after release pos {pos} {:?} top {:?} deferred {:?} on_tree {}",
                item.kind,
                engine.spine.item_top(item.id),
                engine.tree.deferred_height(match item.kind {
                    FlowItemKind::Content { box_id } | FlowItemKind::Collapsed { box_id } => box_id,
                    _ => unreachable!(),
                }),
                engine.tree.nodes().contains_key(&match item.kind {
                    FlowItemKind::Content { box_id } | FlowItemKind::Collapsed { box_id } => box_id,
                    _ => unreachable!(),
                }),
            );
        }
    }
    assert!(
        !engine.tree.nodes().contains_key(&preview),
        "once out of the warm window, the preview box must be removed together with its host"
    );

    let sa = engine.anchor_at_y(preview_top, &measure, &solver);
    let _ = engine.assemble_with_doc(&doc, sa, vh, &measure, &solver);

    let sa = engine.anchor_at_y(frame_top + 400.0, &measure, &solver);
    engine.assemble_with_doc(&doc, sa, vh, &measure, &solver);
    assert!(
        engine.tree.nodes().contains_key(&preview),
        "when the host re-enters the window, the preview box must be rebuilt"
    );
    assert!(
        engine.spine.content_id(preview).is_some(),
        "the preview item must rise back to Content"
    );
}

fn spine_box_top(engine: &IncrementalEngine, id: LayoutBoxId) -> f64 {
    for pos in 0..engine.spine.len() {
        let item = engine.spine.item_at(pos);
        let hit = match item.kind {
            FlowItemKind::Content { box_id } | FlowItemKind::Collapsed { box_id } => box_id == id,
            _ => false,
        };
        if hit && let Some(top) = engine.spine.item_top(item.id) {
            return top;
        }
    }
    panic!("box {id:?} has no spine item");
}
