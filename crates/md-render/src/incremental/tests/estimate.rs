use md_core::block::BlockKind;
use md_layout::box_tree::LayoutBoxId;
use md_layout::compose::compose;

use super::support::{dummy_layout, estimator};
use md_core::document::{editor_options, load_markdown};

#[test]
fn mermaid_estimate_uses_max_height_before_raster() {
    let short = load_markdown("```mermaid\nflowchart TD\nA-->B\n```\n", editor_options());
    let long = load_markdown(
        &format!("```mermaid\nflowchart TD\n{}\n```\n", "A-->B\n".repeat(80)),
        editor_options(),
    );
    let layout = dummy_layout();
    let tree_s = compose(&short, &layout);
    let tree_l = compose(&long, &layout);
    let id_s = mermaid_leaf(&short);
    let id_l = mermaid_leaf(&long);
    let e = estimator();
    let hs = e.estimate(&tree_s, id_s, 800.0);
    let hl = e.estimate(&tree_l, id_l, 400.0);
    assert_eq!(hs, 420.0);
    assert_eq!(hl, 420.0);
}

#[test]
fn text_estimate_counts_display_width_instead_of_utf8_bytes() {
    let ascii = load_markdown(&format!("{}\n", "a".repeat(40)), editor_options());
    let cjk = load_markdown(&format!("{}\n", "Ａ".repeat(20)), editor_options());
    let layout = dummy_layout();
    let ascii_tree = compose(&ascii, &layout);
    let cjk_tree = compose(&cjk, &layout);
    let estimate = estimator();

    let ascii_height = estimate.estimate(&ascii_tree, paragraph_leaf(&ascii), 160.0);
    let cjk_height = estimate.estimate(&cjk_tree, paragraph_leaf(&cjk), 160.0);

    assert_eq!(ascii_height, 40.0);
    assert_eq!(cjk_height, ascii_height);
}

fn mermaid_leaf(doc: &md_core::document::Document) -> LayoutBoxId {
    let mut found = None;
    doc.for_each_text_leaf(|id, _| {
        if doc.kind(id) == Some(BlockKind::Mermaid) {
            found = Some(LayoutBoxId::for_kind(BlockKind::Mermaid, id));
            false
        } else {
            true
        }
    });
    found.expect("mermaid leaf")
}

fn paragraph_leaf(doc: &md_core::document::Document) -> LayoutBoxId {
    let mut found = None;
    doc.for_each_text_leaf(|id, _| {
        if doc.kind(id) == Some(BlockKind::Paragraph) {
            found = Some(LayoutBoxId::frame(id));
            false
        } else {
            true
        }
    });
    found.expect("paragraph leaf")
}
