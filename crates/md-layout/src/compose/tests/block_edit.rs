use crate::box_tree::{BoxTree, LayoutBoxId};
use crate::compose::LayoutTheme;
use crate::compose::{compose, sync_block_edit};
use crate::style::{BoxDisplay, BoxLayoutStyle, Edges};
use md_core::block::{BlockId, BlockKind};
use md_core::document::{Caret, Document, FocusBias, editor_options, load_markdown};

const MERMAID_MD: &str = "before\n\n```mermaid\nflowchart TD\nA-->B\n```\n\nafter\n";
const MATH_MD: &str = "before\n\n$$\n\\frac{a}{b}\n$$\n\nafter\n";
const IMAGE_MD: &str = "before\n\n![a](u)\n\nafter\n";

const CODE_PAD: Edges = Edges {
    top: 24.0,
    right: 16.0,
    bottom: 12.0,
    left: 16.0,
};

fn margin_bottom(kind: BlockKind) -> md_core::Px {
    match kind {
        BlockKind::CodeBlock => 32.0,
        BlockKind::Mermaid => 30.0,
        BlockKind::Math => 28.0,
        BlockKind::Image => 26.0,
        _ => 0.0,
    }
}

fn margin_top(kind: BlockKind) -> md_core::Px {
    match kind {
        BlockKind::CodeBlock => 18.0,
        BlockKind::Mermaid => 17.0,
        BlockKind::Math => 16.0,
        BlockKind::Image => 15.0,
        _ => 0.0,
    }
}

fn distinct_layout() -> LayoutTheme {
    LayoutTheme::from_resolver(|kind| BoxLayoutStyle {
        display: BoxDisplay::FlowStack,
        margin: Edges {
            top: margin_top(kind),
            right: 0.0,
            bottom: margin_bottom(kind),
            left: 0.0,
        },
        padding: match kind {
            BlockKind::CodeBlock => CODE_PAD,
            _ => Edges::ZERO,
        },
        border: Edges::ZERO,
        gap: 0.0,
    })
}

fn cases() -> [(&'static str, BlockKind); 3] {
    [
        (MERMAID_MD, BlockKind::Mermaid),
        (MATH_MD, BlockKind::Math),
        (IMAGE_MD, BlockKind::Image),
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

fn frame(tree: &BoxTree, block: BlockId) -> &crate::box_tree::BoxNode {
    tree.get(LayoutBoxId::frame(block))
}

#[test]
fn edit_state_frame_takes_code_block_padding_and_shape_kind() {
    let theme = distinct_layout();
    for (md, kind) in cases() {
        let mut doc = load_markdown(md, editor_options());
        let block = block_of(&doc, kind);

        let cold = compose(&doc, &theme);
        assert_eq!(
            cold.style(LayoutBoxId::frame(block)).padding,
            Edges::ZERO,
            "{kind:?}"
        );
        assert_eq!(frame(&cold, block).shape_kind(), kind, "{kind:?}");

        enter(&mut doc, block);
        let hot = compose(&doc, &theme);
        let f = frame(&hot, block);
        assert_eq!(hot.style_of(f).padding, CODE_PAD, "{kind:?} padding");
        assert_eq!(f.shape_kind(), BlockKind::CodeBlock, "{kind:?} shape kind");
        assert_eq!(f.kind(), kind, "{kind:?} kind");
        let p = hot.get(LayoutBoxId::preview(block));
        assert_eq!(p.shape_kind(), kind, "{kind:?} preview shape kind");
        assert_eq!(
            hot.style_of(p).padding,
            Edges::ZERO,
            "{kind:?} preview padding"
        );
    }
}

#[test]
fn edit_state_frame_keeps_its_own_margins() {
    let theme = distinct_layout();
    for (md, kind) in cases() {
        let mut doc = load_markdown(md, editor_options());
        let block = block_of(&doc, kind);
        let own = margin_bottom(kind);

        enter(&mut doc, block);
        let hot = compose(&doc, &theme);
        assert_eq!(
            hot.style(LayoutBoxId::frame(block)).margin.bottom,
            0.0,
            "{kind:?}"
        );
        assert_eq!(
            hot.style(LayoutBoxId::frame(block)).margin.top,
            margin_top(kind),
            "{kind:?} frame top"
        );
        let p = hot.get(LayoutBoxId::preview(block));
        assert_eq!(
            hot.style_of(p).margin.bottom,
            own,
            "{kind:?} preview bottom"
        );
        assert_eq!(
            hot.style_of(p).margin.top,
            margin_top(BlockKind::CodeBlock),
            "{kind:?} preview gap"
        );
    }
}

#[test]
fn leaving_edit_state_restores_padding() {
    let theme = distinct_layout();
    for (md, kind) in cases() {
        let mut doc = load_markdown(md, editor_options());
        let block = block_of(&doc, kind);
        let mut tree = compose(&doc, &theme);

        enter(&mut doc, block);
        sync_block_edit(&mut tree, &doc, &theme, None, Some(block));
        assert_eq!(
            tree.style(LayoutBoxId::frame(block)).padding,
            CODE_PAD,
            "{kind:?}"
        );

        let para = block_of(&doc, BlockKind::Paragraph);
        enter(&mut doc, para);
        sync_block_edit(&mut tree, &doc, &theme, Some(block), None);

        let f = frame(&tree, block);
        assert_eq!(tree.style_of(f).padding, Edges::ZERO, "{kind:?} padding");
        assert_eq!(f.shape_kind(), kind, "{kind:?} shape kind");
        let cold = compose(&doc, &theme);
        assert_eq!(
            tree.style_of(f),
            cold.style(LayoutBoxId::frame(block)),
            "{kind:?} vs cold"
        );
    }
}

#[test]
fn other_kinds_are_untouched_by_the_rewrite() {
    let theme = distinct_layout();
    let doc = load_markdown("para\n\n```rust\nfn a() {}\n```\n", editor_options());
    let tree = compose(&doc, &theme);
    for kind in [BlockKind::Paragraph, BlockKind::CodeBlock] {
        let block = block_of(&doc, kind);
        assert_eq!(frame(&tree, block).shape_kind(), kind, "{kind:?}");
    }
}

const PARA_MATH_MD: &str = "line1\nline2\n$$\n\\frac{a}{b}\n$$\n";

fn enter_glued_math(doc: &mut Document) -> BlockId {
    let math = block_of(doc, BlockKind::Math);
    enter(doc, math);
    math
}

#[test]
fn revealed_display_math_uses_same_edit_chrome_as_mermaid() {
    let theme = distinct_layout();
    let mut doc = load_markdown(PARA_MATH_MD, editor_options());
    let math = enter_glued_math(&mut doc);
    assert_eq!(doc.block_edit(), Some(math));

    let hot = compose(&doc, &theme);
    let f = frame(&hot, math);
    assert_eq!(hot.style_of(f).padding, CODE_PAD, "padding");
    assert_eq!(f.shape_kind(), BlockKind::CodeBlock, "shape kind");
    assert_eq!(f.kind(), BlockKind::Math, "kind");
    assert!(f.edit_source());
    let source = hot.text(LayoutBoxId::frame(math));
    assert_eq!(
        source, "\\frac{a}{b}",
        "the source well should not carry the blank line after the opening $$"
    );
    assert!(
        !source.contains("line1") && !source.contains("line2"),
        "those two lines above must not enter the source well: {source:?}"
    );

    let p = hot.get(LayoutBoxId::preview(math));
    assert_eq!(p.shape_kind(), BlockKind::Math, "preview shape kind");
    assert_eq!(p.kind(), BlockKind::Math, "preview kind");
    assert!(!p.edit_source());
    assert_eq!(
        hot.style_of(p).margin.top,
        margin_top(BlockKind::CodeBlock),
        "preview gap"
    );
    let preview_text = hot.text(LayoutBoxId::preview(math));
    assert_eq!(
        preview_text, "\\frac{a}{b}",
        "preview latex={preview_text:?}"
    );
}

#[test]
fn leaving_revealed_display_math_restores_padding() {
    let theme = distinct_layout();
    let mut doc = load_markdown(PARA_MATH_MD, editor_options());
    let math = enter_glued_math(&mut doc);
    let mut tree = compose(&doc, &theme);
    assert_eq!(tree.style(LayoutBoxId::frame(math)).padding, CODE_PAD);

    let para = block_of(&doc, BlockKind::Paragraph);
    enter(&mut doc, para);
    assert_eq!(doc.block_edit(), None);
    sync_block_edit(&mut tree, &doc, &theme, Some(math), None);

    let f = frame(&tree, math);
    assert_eq!(tree.style_of(f).padding, Edges::ZERO);
    assert_eq!(f.shape_kind(), BlockKind::Math);
    assert!(!tree.nodes().contains_key(&LayoutBoxId::preview(math)));
}

#[test]
fn attaching_without_a_child_slot_reports_nothing_and_emits_nothing() {
    use crate::box_tree::BoxChildren;

    let theme = distinct_layout();
    let mut doc = load_markdown(MERMAID_MD, editor_options());
    let block = block_of(&doc, BlockKind::Mermaid);
    let mut tree = compose(&doc, &theme);

    let frame = LayoutBoxId::frame(block);
    let parent = tree.get(frame).parent.expect("frame parent");
    if let Some(node) = tree.nodes.get_mut(&parent)
        && let BoxChildren::Vertical(kids) = &mut node.children
    {
        kids.retain(|c| *c != frame);
    }

    enter(&mut doc, block);
    let splices = sync_block_edit(&mut tree, &doc, &theme, None, Some(block));
    assert!(
        splices.is_empty(),
        "main box is not in the children list, so no splice should be reported:{splices:?}"
    );
    assert!(
        !tree.nodes().contains_key(&LayoutBoxId::preview(block)),
        "preview box was emitted yet could not be inserted into children, leaving an orphan"
    );
    assert!(
        !tree.get(frame).edit_source,
        "the main box did not enter edit mode"
    );
}

#[test]
fn entering_block_edit_attaches_the_preview_to_its_parent() {
    use crate::box_tree::BoxChildren;

    let theme = distinct_layout();
    let mut doc = load_markdown(MERMAID_MD, editor_options());
    let block = block_of(&doc, BlockKind::Mermaid);
    let mut tree = compose(&doc, &theme);

    enter(&mut doc, block);
    let splices = sync_block_edit(&mut tree, &doc, &theme, None, Some(block));
    assert_eq!(splices.len(), 1);
    let preview = LayoutBoxId::preview(block);
    let parent = tree.get(preview).parent.expect("preview parent");
    let host = tree.get(parent);
    let BoxChildren::Vertical(children) = &host.children else {
        panic!("expected vertical parent");
    };
    assert!(
        children.contains(&preview),
        "a reported inserted preview must be reachable from its parent"
    );
    assert!(
        tree.island_boxes().contains(&preview),
        "the tree walk must see the preview through the child list"
    );
}
