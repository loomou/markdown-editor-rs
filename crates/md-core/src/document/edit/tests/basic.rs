use super::support::caret;
use crate::block::BlockKind;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{PasteIntent, editor_options, load_markdown};

#[test]
fn empty_document_inserts_into_seeded_paragraph() {
    let mut doc = load_markdown("", editor_options());
    let leaves = doc.text_leaves();
    assert_eq!(leaves.len(), 1);
    assert_eq!(doc.kind(leaves[0]), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaves[0]).unwrap(), "");
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(0, 0)),
        Command::Insert { text: "hi".into() },
    );
    assert_eq!(out, caret(leaves[0], 2));
    assert_eq!(doc.text_of(leaves[0]).unwrap(), "hi");
}

#[test]
fn blank_document_inserts_into_seeded_paragraph() {
    let mut doc = load_markdown("\n\n", editor_options());
    let leaves = doc.text_leaves();
    assert_eq!(leaves.len(), 1);
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaves[0], 0)),
        Command::Insert { text: "x".into() },
    );
    assert_eq!(out.offset, 1);
    assert_eq!(doc.text_of(leaves[0]).unwrap(), "x");
}

#[test]
fn apply_insert_matches_paste() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert { text: "X".into() },
    );
    assert_eq!(out, caret(leaf, 1));
    assert!(doc.text_of(leaf).unwrap().starts_with('X'));
}

#[test]
fn apply_break_splits_paragraph() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let out = apply(&mut doc, Sel::collapsed(caret(leaf, 2)), Command::Break);
    assert_eq!(out.offset, 0);
    assert_ne!(out.block, leaf);
    assert_eq!(doc.text_of(leaf).unwrap(), "he");
    assert_eq!(doc.text_of(out.block).unwrap(), "llo");
}

#[test]
fn apply_soft_break_inserts_newline() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let out = apply(&mut doc, Sel::collapsed(caret(leaf, 2)), Command::SoftBreak);
    assert_eq!(out, caret(leaf, 3));
    assert_eq!(doc.text_of(leaf).unwrap(), "he\nllo");
    assert_eq!(doc.text_leaves(), vec![leaf]);
}

#[test]
fn apply_break_is_noop_in_table_cell() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| cd | e |\n", editor_options());
    let cell = doc
        .text_leaves()
        .into_iter()
        .find(|&id| doc.kind(id) == Some(BlockKind::TableCell) && doc.text_of(id) == Some("cd"))
        .expect("cell");
    let at = caret(cell, 1);
    let out = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_eq!(out, at);
    assert_eq!(doc.kind(cell), Some(BlockKind::TableCell));
    assert_eq!(doc.text_of(cell).unwrap(), "cd");
    assert_eq!(doc.to_markdown(), "| a | b |\n| --- | --- |\n| cd | e |\n");
}

#[test]
fn apply_soft_break_inserts_newline_in_table_cell() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| cd | e |\n", editor_options());
    let cell = doc
        .text_leaves()
        .into_iter()
        .find(|&id| doc.kind(id) == Some(BlockKind::TableCell) && doc.text_of(id) == Some("cd"))
        .expect("cell");
    let at = caret(cell, 1);
    let out = apply(&mut doc, Sel::collapsed(at), Command::SoftBreak);
    assert_eq!(out, caret(cell, 2));
    assert_eq!(doc.kind(cell), Some(BlockKind::TableCell));
    assert_eq!(doc.text_of(cell).unwrap(), "c\nd");
}

#[test]
fn line_breaks_are_noop_in_image_blocks() {
    for command in [Command::Break, Command::SoftBreak] {
        let mut doc = load_markdown("![alt](https://example.com/image.png)\n", editor_options());
        let image = doc
            .text_leaves()
            .into_iter()
            .find(|&id| doc.kind(id) == Some(BlockKind::Image))
            .expect("image");
        let at = caret(image, "![alt](https://".len());

        let out = apply(&mut doc, Sel::collapsed(at), command.clone());

        assert_eq!(out, at);
        assert_eq!(doc.kind(image), Some(BlockKind::Image));
        let id = doc.live_id(image).expect("live image");
        assert_eq!(doc.leaf_source(id), "![alt](https://example.com/image.png)");
        assert_eq!(doc.to_markdown(), "![alt](https://example.com/image.png)\n");
    }
}

#[test]
fn line_breaks_on_a_container_are_inert() {
    for command in [Command::Break, Command::SoftBreak] {
        let mut doc = load_markdown("> q\n", editor_options());
        let quote = doc
            .preorder()
            .into_iter()
            .find(|id| doc.kind(id.index) == Some(BlockKind::BlockQuote))
            .expect("quote")
            .index;
        let at = caret(quote, 0);
        let live = doc.arena.live_count();

        let out = apply(&mut doc, Sel::collapsed(at), command.clone());

        assert_eq!(out, at, "{command:?}");
        assert_eq!(doc.arena.live_count(), live, "{command:?}");
        assert_eq!(doc.to_markdown(), "> q\n", "{command:?}");
    }
}

#[test]
fn apply_delete_backward_merges_at_start() {
    let mut doc = load_markdown("ab\n\ncd\n", editor_options());
    let leaves = doc.text_leaves();
    assert_eq!(leaves.len(), 2);
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaves[1], 0)),
        Command::DeleteBackward,
    );
    assert_eq!(out.block, leaves[0]);
    assert_eq!(out.offset, 2);
    assert_eq!(doc.text_of(leaves[0]).unwrap(), "abcd");
}

#[test]
fn apply_delete_forward_merges_at_end() {
    let mut doc = load_markdown("ab\n\ncd\n", editor_options());
    let leaves = doc.text_leaves();
    assert_eq!(leaves.len(), 2);
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaves[0], 2)),
        Command::DeleteForward,
    );
    assert_eq!(out, caret(leaves[0], 2));
    assert_eq!(doc.text_of(leaves[0]), Some("abcd"));
    assert!(doc.live_id(leaves[1]).is_none());
}

#[test]
fn apply_delete_backward_clears_span() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let out = apply(
        &mut doc,
        Sel {
            anchor: caret(leaf, 1),
            head: caret(leaf, 4),
        },
        Command::DeleteBackward,
    );
    assert_eq!(out, caret(leaf, 1));
    assert_eq!(doc.text_of(leaf).unwrap(), "ho");
}

#[test]
fn apply_paste_fragment() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 5)),
        Command::Paste {
            text: "\n\n# title\n".into(),
            intent: PasteIntent::IndependentFragment,
        },
    );
    assert_eq!(doc.kind(out.block), Some(BlockKind::Heading(1)));
    assert_eq!(doc.text_of(out.block).unwrap(), "title");
}
