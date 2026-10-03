use super::support::{caret, type_chars};
use crate::block::BlockKind;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{Document, editor_options, load_markdown};

fn shape(doc: &Document) -> Vec<String> {
    doc.preorder()
        .into_iter()
        .map(|id| format!("{:?}|{:?}", doc.kind(id.index), doc.display(id)))
        .collect()
}

fn reload(saved: &str) -> Document {
    load_markdown(saved, editor_options())
}

fn assert_no_blank_line_in_prose(doc: &Document, step: usize) {
    for id in doc.preorder() {
        if !matches!(
            doc.kind(id.index),
            Some(BlockKind::Paragraph | BlockKind::Heading(_))
        ) {
            continue;
        }
        let display = doc.display(id);
        assert!(!display.starts_with('\n'), "step {step}: {display:?}");
        assert!(!display.contains("\n\n"), "step {step}: {display:?}");
    }
}

#[test]
fn a_soft_break_inside_a_line_stays_in_the_paragraph() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let out = apply(&mut doc, Sel::collapsed(caret(leaf, 2)), Command::SoftBreak);
    assert_eq!(out, caret(leaf, 3));
    assert_eq!(doc.text_leaves(), vec![leaf]);
    assert_eq!(doc.to_markdown(), "he\nllo\n");
    assert_eq!(shape(&reload(&doc.to_markdown())), shape(&doc));
}

#[test]
fn a_soft_break_at_the_end_keeps_the_line_until_the_next_character() {
    let mut doc = load_markdown("hi\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = apply(&mut doc, Sel::collapsed(caret(leaf, 2)), Command::SoftBreak);
    assert_eq!(at, caret(leaf, 3));
    assert_eq!(doc.text_leaves(), vec![leaf]);
    let typed = type_chars(&mut doc, at, "x");
    assert_eq!(typed, caret(leaf, 4));
    assert_eq!(doc.text_leaves(), vec![leaf]);
    assert_eq!(doc.to_markdown(), "hi\nx\n");
    assert_eq!(shape(&reload(&doc.to_markdown())), shape(&doc));
}

#[test]
fn a_soft_break_beside_a_break_starts_a_paragraph() {
    let mut doc = load_markdown("hi\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = apply(&mut doc, Sel::collapsed(caret(leaf, 2)), Command::SoftBreak);
    let out = apply(&mut doc, Sel::collapsed(at), Command::SoftBreak);
    assert_ne!(out.block, leaf);
    assert_eq!(out.offset, 0);
    assert_eq!(doc.text_of(leaf), Some("hi"));
    assert_eq!(doc.text_of(out.block), Some(""));
    assert_eq!(doc.to_markdown(), "hi\n\n");
    assert_eq!(shape(&reload(&doc.to_markdown())), shape(&doc));
}

#[test]
fn a_soft_break_at_the_start_starts_a_paragraph() {
    let mut doc = load_markdown("hi\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let out = apply(&mut doc, Sel::collapsed(caret(leaf, 0)), Command::SoftBreak);
    assert_ne!(out.block, leaf);
    assert_eq!(out.offset, 0);
    assert_eq!(doc.text_of(out.block), Some("hi"));
    assert_eq!(doc.to_markdown(), "\nhi\n");
    assert_eq!(shape(&reload(&doc.to_markdown())), shape(&doc));
}

#[test]
fn a_soft_break_on_either_side_of_a_break_gives_the_same_paragraphs() {
    for offset in [2, 3] {
        let mut doc = load_markdown("hi\nx\n", editor_options());
        let leaf = doc.text_leaves()[0];
        assert_eq!(doc.text_of(leaf), Some("hi\nx"));
        let out = apply(
            &mut doc,
            Sel::collapsed(caret(leaf, offset)),
            Command::SoftBreak,
        );
        assert_eq!(doc.text_of(leaf), Some("hi"), "offset={offset}");
        assert_eq!(doc.text_of(out.block), Some("x"), "offset={offset}");
        assert_eq!(doc.to_markdown(), "hi\n\nx\n", "offset={offset}");
        assert_eq!(shape(&reload(&doc.to_markdown())), shape(&doc));
    }
}

#[test]
fn a_soft_break_at_the_end_of_a_quoted_paragraph_starts_a_paragraph() {
    let mut doc = load_markdown("> hi\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = apply(&mut doc, Sel::collapsed(caret(leaf, 2)), Command::SoftBreak);
    let out = apply(&mut doc, Sel::collapsed(at), Command::SoftBreak);
    assert_eq!(doc.text_of(leaf), Some("hi"));
    assert_eq!(doc.text_of(out.block), Some(""));
    assert_eq!(doc.to_markdown(), "> hi\n> \n");
    assert_eq!(shape(&reload(&doc.to_markdown())), shape(&doc));
}

#[test]
fn a_soft_break_at_the_end_of_a_list_item_starts_a_paragraph() {
    let mut doc = load_markdown("- hi\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = apply(&mut doc, Sel::collapsed(caret(leaf, 2)), Command::SoftBreak);
    let out = apply(&mut doc, Sel::collapsed(at), Command::SoftBreak);
    assert_eq!(doc.text_of(leaf), Some("hi"));
    assert_eq!(doc.text_of(out.block), Some(""));
    assert_eq!(doc.to_markdown(), "- hi\n  \n");
    assert_eq!(shape(&reload(&doc.to_markdown())), shape(&doc));
}

#[test]
fn a_lone_trailing_soft_break_is_lost_on_save() {
    let mut doc = load_markdown("hi\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = apply(&mut doc, Sel::collapsed(caret(leaf, 2)), Command::SoftBreak);
    assert_eq!(at, caret(leaf, 3));
    assert_eq!(doc.text_of(leaf), Some("hi\n"));
    assert_eq!(doc.to_markdown(), "hi\n");
    let reloaded = reload(&doc.to_markdown());
    assert_eq!(reloaded.text_leaves().len(), 1);
    assert_eq!(reloaded.text_of(reloaded.text_leaves()[0]), Some("hi"));
}

#[test]
fn a_soft_break_never_leaves_a_blank_line_in_a_paragraph() {
    for start in [0, 3, 7] {
        let mut doc = load_markdown("one two\n", editor_options());
        let leaf = doc.text_leaves()[0];
        let mut at = caret(leaf, start);
        for step in 0..10 {
            at = apply(&mut doc, Sel::collapsed(at), Command::SoftBreak);
            assert_no_blank_line_in_prose(&doc, step);
        }
        at = type_chars(&mut doc, at, "z");
        assert_no_blank_line_in_prose(&doc, 10);
    }
}
