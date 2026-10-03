use super::support::{caret, type_chars};
use crate::block::BlockKind;
use crate::document::edit::{Caret, Command, Sel, apply};
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

fn prose_rows(doc: &Document) -> usize {
    doc.preorder()
        .into_iter()
        .filter(|id| doc.kind(id.index) == Some(BlockKind::Paragraph))
        .map(|id| doc.display(id).matches('\n').count() + 1)
        .sum()
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
fn a_soft_break_beside_a_break_adds_a_line() {
    let mut doc = load_markdown("hi\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = apply(&mut doc, Sel::collapsed(caret(leaf, 2)), Command::SoftBreak);
    let out = apply(&mut doc, Sel::collapsed(at), Command::SoftBreak);
    assert_ne!(out.block, leaf);
    assert_eq!(out.offset, 0);
    assert_eq!(doc.text_of(leaf), Some("hi"));
    assert_eq!(doc.text_of(out.block), Some(""));
    assert_eq!(doc.to_markdown(), "hi\n\n\n");
    assert_eq!(shape(&reload(&doc.to_markdown())), shape(&doc));
}

#[test]
fn a_paragraph_started_by_a_soft_break_takes_the_next_character() {
    let mut doc = load_markdown("hi\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = apply(&mut doc, Sel::collapsed(caret(leaf, 2)), Command::SoftBreak);
    let out = apply(&mut doc, Sel::collapsed(at), Command::SoftBreak);
    assert_ne!(out.block, leaf);
    assert_eq!(out.offset, 0);
    let typed = type_chars(&mut doc, out, "x");
    assert_eq!(typed.block, out.block);
    assert_eq!(typed.offset, 1);
    assert_eq!(doc.text_of(leaf), Some("hi"));
    assert_eq!(doc.text_of(out.block), Some("x"));
    assert_eq!(doc.to_markdown(), "hi\n\n\nx\n");
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
    for (offset, landed_on) in [(2, ""), (3, "x")] {
        let mut doc = load_markdown("hi\nx\n", editor_options());
        let leaf = doc.text_leaves()[0];
        assert_eq!(doc.text_of(leaf), Some("hi\nx"));
        let out = apply(
            &mut doc,
            Sel::collapsed(caret(leaf, offset)),
            Command::SoftBreak,
        );
        assert_eq!(doc.text_of(leaf), Some("hi"), "offset={offset}");
        assert_eq!(doc.text_of(out.block), Some(landed_on), "offset={offset}");
        assert_eq!(out.offset, 0, "offset={offset}");
        assert_eq!(doc.to_markdown(), "hi\n\n\nx\n", "offset={offset}");
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
    assert_eq!(doc.to_markdown(), "> hi\n> \n> \n");
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
    assert_eq!(doc.to_markdown(), "- hi\n  \n  \n");
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

fn assert_the_editor_settles_after_one_save(doc: &Document, step: usize) {
    let first = doc.to_markdown();
    let second = reload(&first).to_markdown();
    assert_eq!(
        reload(&second).to_markdown(),
        second,
        "step {step}: {first:?} then {second:?}"
    );
}

fn step_the_caret_back(doc: &Document, at: Caret) -> Caret {
    let text = doc.text_of(at.block).unwrap_or_default().to_string();
    let left = crate::document::prev_grapheme_boundary(&text, at.offset.min(text.len()));
    if left != at.offset {
        return caret(at.block, left);
    }
    doc.nth_text_leaf_from(at.block, -1)
        .map(|block| caret(block, 0))
        .unwrap_or(at)
}

#[test]
fn the_prose_invariant_holds_through_a_scripted_session() {
    let mut doc = load_markdown("one two\nthree four\n", editor_options());
    let first = doc.text_leaves()[0];
    let mut at = caret(first, 3);
    let mut step = 0usize;
    let script = [
        Command::SoftBreak,
        Command::Insert { text: "x".into() },
        Command::Break,
        Command::SoftBreak,
        Command::Insert { text: "y".into() },
        Command::DeleteBackward,
        Command::SoftBreak,
        Command::Insert { text: "z".into() },
        Command::DeleteBackward,
        Command::DeleteBackward,
        Command::Break,
        Command::SoftBreak,
        Command::Insert { text: "w".into() },
    ];
    for cmd in script {
        at = apply(&mut doc, Sel::collapsed(at), cmd);
        step += 1;
        assert_no_blank_line_in_prose(&doc, step);
        assert_the_editor_settles_after_one_save(&doc, step);
        at = step_the_caret_back(&doc, at);
        assert_no_blank_line_in_prose(&doc, step);
        assert_the_editor_settles_after_one_save(&doc, step);
    }
}

#[test]
fn each_soft_break_at_the_end_of_a_line_adds_one_line() {
    let mut doc = load_markdown("132\n", editor_options());
    let mut at = caret(doc.text_leaves()[0], 3);
    at = apply(&mut doc, Sel::collapsed(at), Command::SoftBreak);
    assert_eq!(prose_rows(&doc), 2);
    assert_eq!(doc.to_markdown(), "132\n");
    for (pressed, saved) in [(2, "132\n\n\n"), (3, "132\n\n\n\n"), (4, "132\n\n\n\n\n")] {
        at = apply(&mut doc, Sel::collapsed(at), Command::SoftBreak);
        assert_eq!(prose_rows(&doc), pressed + 1, "after {pressed} presses");
        assert_eq!(doc.to_markdown(), saved, "after {pressed} presses");
        assert_eq!(
            doc.text_leaves().last().copied(),
            Some(at.block),
            "after {pressed} presses the caret must sit on the last line"
        );
        assert_eq!(
            shape(&reload(saved)),
            shape(&doc),
            "after {pressed} presses"
        );
    }
}
