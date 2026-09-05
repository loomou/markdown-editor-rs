use crate::block::{BlockId, BlockKind};
use crate::document::edit::{Caret, Sel};
use crate::document::focus::FocusBias;
use crate::document::{Document, editor_options, load_markdown};

fn caret(block: BlockId, offset: usize) -> Caret {
    Caret { block, offset }
}

fn sel(doc: &Document, a: usize, ao: usize, b: usize, bo: usize) -> Sel {
    let leaves = doc.text_leaves();
    Sel {
        anchor: caret(leaves[a], ao),
        head: caret(leaves[b], bo),
    }
}

fn copy_all(doc: &Document, i: usize) -> String {
    let leaves = doc.text_leaves();
    let len = doc
        .live_id(leaves[i])
        .map(|id| doc.caret_text(id).len())
        .expect("leaf");
    doc.copy_markdown(sel(doc, i, 0, i, len))
}

#[test]
fn collapsed_selection_copies_nothing() {
    let doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    assert_eq!(doc.copy_markdown(Sel::collapsed(caret(leaf, 3))), "");
    assert_eq!(doc.copy_markdown(sel(&doc, 0, 2, 0, 2)), "");
}

#[test]
fn inline_slice_keeps_emphasis_source() {
    let doc = load_markdown("hello **bold** world\n", editor_options());
    let id = doc.live_id(doc.text_leaves()[0]).expect("leaf");
    assert_eq!(doc.display(id), "hello bold world");

    assert_eq!(doc.copy_markdown(sel(&doc, 0, 6, 0, 10)), "**bold**");

    assert_eq!(doc.copy_markdown(sel(&doc, 0, 6, 0, 8)), "bo");

    assert_eq!(doc.copy_markdown(sel(&doc, 0, 4, 0, 12)), "o **bold** w");
}

#[test]
fn inline_slice_keeps_code_span_and_link_source() {
    let doc = load_markdown("hello `d` and *em*\n", editor_options());
    let id = doc.live_id(doc.text_leaves()[0]).expect("leaf");
    assert_eq!(doc.display(id), "hello d and em");
    assert_eq!(doc.copy_markdown(sel(&doc, 0, 6, 0, 7)), "`d`");
    assert_eq!(doc.copy_markdown(sel(&doc, 0, 6, 0, 13)), "`d` and e");
    assert_eq!(doc.copy_markdown(sel(&doc, 0, 12, 0, 14)), "*em*");

    let link = load_markdown("go [here](https://ex.test) now\n", editor_options());
    let lid = link.live_id(link.text_leaves()[0]).expect("leaf");
    assert_eq!(link.display(lid), "go here now");
    assert_eq!(
        link.copy_markdown(sel(&link, 0, 3, 0, 7)),
        "[here](https://ex.test)"
    );
}

#[test]
fn reversed_selection_copies_same_span() {
    let doc = load_markdown("hello **bold** world\n", editor_options());
    assert_eq!(doc.copy_markdown(sel(&doc, 0, 10, 0, 6)), "**bold**");
    let two = load_markdown("alpha\n\nbeta\n", editor_options());
    assert_eq!(two.copy_markdown(sel(&two, 1, 3, 0, 2)), "pha\n\nbet");
}

#[test]
fn whole_leaf_copies_block_syntax() {
    let head = load_markdown("## title\n", editor_options());
    assert_eq!(copy_all(&head, 0), "## title");

    assert_eq!(head.copy_markdown(sel(&head, 0, 0, 0, 3)), "tit");

    let code = load_markdown("```rust\nfn x() {}\n```\n", editor_options());
    assert_eq!(copy_all(&code, 0), "```rust\nfn x() {}\n```");

    assert_eq!(code.copy_markdown(sel(&code, 0, 0, 0, 2)), "fn");

    let math = load_markdown("$$a+b$$\n", editor_options());
    assert_eq!(copy_all(&math, 0), "$$a+b$$");

    let mermaid = load_markdown("```mermaid\ngraph TD\n```\n", editor_options());
    assert_eq!(copy_all(&mermaid, 0), "```mermaid\ngraph TD\n```");

    let img = load_markdown("![alt](u.png)\n", editor_options());
    assert_eq!(img.kind(img.text_leaves()[0]), Some(BlockKind::Image));
    assert_eq!(copy_all(&img, 0), "![alt](u.png)");
}

#[test]
fn whole_leaf_copy_round_trips_through_load() {
    for src in [
        "## title\n",
        "```rust\nfn x() {}\n```\n",
        "$$a+b$$\n",
        "![alt](u.png)\n",
        "hello **bold** `code`\n",
    ] {
        let doc = load_markdown(src, editor_options());
        let md = copy_all(&doc, 0);
        let again = load_markdown(&md, editor_options());
        let a = doc.live_id(doc.text_leaves()[0]).expect("leaf");
        let b = again.live_id(again.text_leaves()[0]).expect("leaf");
        assert_eq!(doc.kind(a.index), again.kind(b.index), "src={src:?}");
        assert_eq!(doc.display(a), again.display(b), "src={src:?}");
    }
}

#[test]
fn list_items_copy_with_markers() {
    let doc = load_markdown("- a\n- b\n", editor_options());
    assert_eq!(doc.copy_markdown(sel(&doc, 0, 0, 1, 1)), "- a\n- b");

    assert_eq!(doc.copy_markdown(sel(&doc, 0, 1, 1, 1)), "- b");

    let long = load_markdown("- a\n- bb\n", editor_options());
    assert_eq!(long.copy_markdown(sel(&long, 0, 0, 1, 1)), "- a\n\nb");
}

#[test]
fn ordered_items_copy_from_the_source_start() {
    let doc = load_markdown("1. x\n2. y\n3. z\n", editor_options());
    assert_eq!(doc.copy_markdown(sel(&doc, 1, 0, 2, 1)), "1. y\n2. z");
    let task = load_markdown("- [ ] a\n- [x] b\n", editor_options());
    assert_eq!(
        task.copy_markdown(sel(&task, 0, 0, 1, 1)),
        "- [ ] a\n- [x] b"
    );
}

#[test]
fn copied_items_keep_marker_style_and_ordered_start() {
    let plus = load_markdown("+ a\n+ b\n", editor_options());
    assert_eq!(plus.copy_markdown(sel(&plus, 0, 0, 1, 1)), "+ a\n+ b");
    let star = load_markdown("* a\n* b\n", editor_options());
    assert_eq!(star.copy_markdown(sel(&star, 0, 0, 1, 1)), "* a\n* b");
    let paren = load_markdown("1) x\n2) y\n", editor_options());
    assert_eq!(paren.copy_markdown(sel(&paren, 0, 0, 1, 1)), "1) x\n2) y");

    let start = load_markdown("3. x\n4. y\n", editor_options());
    assert_eq!(start.copy_markdown(sel(&start, 0, 0, 1, 1)), "3. x\n4. y");

    let mid = load_markdown("3. x\n4. y\n5. z\n", editor_options());
    assert_eq!(mid.copy_markdown(sel(&mid, 1, 0, 2, 1)), "3. y\n4. z");
}

#[test]
fn loose_list_items_keep_blank_line() {
    let doc = load_markdown("- a\n\n- b\n", editor_options());
    assert_eq!(doc.copy_markdown(sel(&doc, 0, 0, 1, 1)), "- a\n\n- b");
}

#[test]
fn nested_list_item_lifts_outermost() {
    let doc = load_markdown("- a\n  - b\n", editor_options());

    assert_eq!(doc.copy_markdown(sel(&doc, 0, 0, 1, 1)), "- a\n  - b");

    assert_eq!(doc.copy_markdown(sel(&doc, 1, 0, 1, 1)), "- b");

    let long = load_markdown("- a\n  - bb\n", editor_options());
    assert_eq!(long.copy_markdown(sel(&long, 1, 0, 1, 1)), "b");
}

#[test]
fn cross_block_copy_joins_with_blank_line() {
    let doc = load_markdown("alpha\n\nbeta\n", editor_options());
    assert_eq!(doc.copy_markdown(sel(&doc, 0, 2, 1, 3)), "pha\n\nbet");
    let mixed = load_markdown("hello\n\n## t\n\n- a\n- b\n", editor_options());
    assert_eq!(
        mixed.copy_markdown(sel(&mixed, 0, 2, 3, 1)),
        "llo\n\n## t\n\n- a\n- b"
    );
}

#[test]
fn select_all_copy_round_trips_shape() {
    let src = "# title\n\nhello **bold**\n\n- a\n- b\n\n> quoted\n";
    let doc = load_markdown(src, editor_options());
    let leaves = doc.text_leaves();
    let last = doc.live_id(*leaves.last().expect("leaf")).expect("live");
    let all = doc.copy_markdown(Sel {
        anchor: caret(leaves[0], 0),
        head: caret(last.index, doc.caret_text(last).len()),
    });
    let again = load_markdown(&all, editor_options());
    let want: Vec<_> = doc
        .text_leaves()
        .into_iter()
        .filter_map(|b| doc.kind(b))
        .collect();
    let got: Vec<_> = again
        .text_leaves()
        .into_iter()
        .filter_map(|b| again.kind(b))
        .collect();
    assert_eq!(want, got, "all={all:?}");
    assert!(all.contains("# title"), "{all:?}");
    assert!(all.contains("**bold**"), "{all:?}");
    assert!(all.contains("- a\n- b"), "{all:?}");
}

#[test]
fn quoted_paragraph_copies_without_marker() {
    let doc = load_markdown("> hi\n", editor_options());
    assert_eq!(copy_all(&doc, 0), "hi");
}

#[test]
fn table_cells_copy_as_plain_text() {
    let doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let leaves = doc.text_leaves();
    assert_eq!(doc.kind(leaves[0]), Some(BlockKind::TableCell));

    assert_eq!(copy_all(&doc, 0), "a");
    assert_eq!(doc.copy_markdown(sel(&doc, 0, 0, 1, 1)), "a\n\nb");
}

#[test]
fn range_retarget_keeps_collapsed_inline_slice() {
    let mut doc = load_markdown("hello **bold** world\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let (a, h) =
        doc.retarget_inline_focus_range(caret(leaf, 6), caret(leaf, 10), FocusBias::Neutral);
    let id = doc.live_id(leaf).expect("leaf");
    assert_eq!(doc.display(id), "hello bold world");
    assert_eq!((a.offset, h.offset), (6, 10));
    assert_eq!(doc.copy_markdown(Sel { anchor: a, head: h }), "**bold**");
}

#[test]
fn revealed_leaf_slices_visible_source() {
    let mut doc = load_markdown("hello **bold** world\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = doc.retarget_inline_focus(caret(leaf, 8));
    let id = doc.live_id(leaf).expect("leaf");
    assert_eq!(doc.display(id), "hello **bold** world");
    assert_eq!(doc.copy_markdown(sel(&doc, 0, 8, 0, 14)), "bold**");
    assert_eq!(doc.copy_markdown(sel(&doc, 0, 6, 0, 14)), "**bold**");
}
