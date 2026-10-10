use super::support::{CountingMeasure, dummy_layout, estimator, loaded};
use crate::incremental::anchor::ScrollAnchor;
use crate::incremental::engine::IncrementalEngine;
use md_core::doc::Doc;
use md_core::document::{Caret, Command, FocusBias, PasteIntent, Sel};
use md_layout::box_tree::LayoutBoxId;
use md_layout::island::FallbackSolver;
use md_layout::style::BoxLayoutEnvironment;
use std::cell::Cell;

const LIST_MD: &str = "before\n\n- item\n\nafter\n";

#[test]
fn undo_after_pasting_an_image_inside_a_list_item_drops_the_preview_item() {
    let mut doc = Doc::new(loaded(LIST_MD));
    let env = BoxLayoutEnvironment::default();
    let mut engine = IncrementalEngine::new(&doc.document, env, estimator(), dummy_layout());
    let measure = CountingMeasure {
        calls: Cell::new(0),
    };
    let solver = FallbackSolver;
    let _ = engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);

    let item = doc
        .document
        .text_leaves()
        .into_iter()
        .find(|&b| doc.document.text_of(b).is_some_and(|t| t.contains("item")))
        .expect("list item leaf");
    let off = doc.document.text_of(item).unwrap().len();
    let _ = doc.apply(
        Sel::collapsed(Caret {
            block: item,
            offset: off,
        }),
        Command::Paste {
            text: "![a](u)\n".into(),
            intent: PasteIntent::IndependentFragment,
        },
    );
    let changes = doc.take_changes();
    let _ = engine.apply_changes(&doc.document, &changes);
    let _ = engine.sync_block_edit(&doc.document);
    let _ = engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);

    let image = doc
        .document
        .text_leaves()
        .into_iter()
        .find(|&b| doc.document.kind(b) == Some(md_core::block::BlockKind::Image))
        .expect("image block");
    doc.document.retarget_inline_focus_biased(
        Caret {
            block: image,
            offset: 0,
        },
        FocusBias::Neutral,
    );
    let _ = engine.sync_block_edit(&doc.document);
    let _ = engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
    let preview = LayoutBoxId::preview(image);
    assert!(engine.tree.nodes().contains_key(&preview));
    assert!(engine.spine.content_id(preview).is_some());

    let _ = doc.undo();
    let changes = doc.take_changes();
    let _ = engine.apply_changes(&doc.document, &changes);
    let _ = engine.sync_block_edit(&doc.document);

    assert!(!engine.tree.nodes().contains_key(&preview));
    assert!(
        engine.spine.content_id(preview).is_none(),
        "the preview flow item must not outlive its box"
    );

    let _ = engine.assemble_incremental(ScrollAnchor::top(), 600.0, &measure, &solver);
}
