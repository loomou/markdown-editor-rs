use super::support::{caret, fresh, type_chars};
use crate::block::BlockKind;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{FocusBias, editor_options, load_markdown};
use crate::inline::InlineMarks;

fn has_list(doc: &crate::document::Document) -> bool {
    doc.preorder()
        .into_iter()
        .any(|id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
}

#[test]
fn typed_123_enter_splits_paragraph() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "123");
    assert_eq!(at, caret(leaf, 3));
    let out = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_eq!(doc.text_of(leaf).unwrap(), "123");
    assert_ne!(out.block, leaf);
    assert_eq!(out.offset, 0);
    assert_eq!(doc.text_of(out.block).unwrap(), "");
    assert_eq!(doc.kind(out.block), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_leaves().len(), 2);
}

#[test]
fn enter_then_dash_space_wraps_new_paragraph_as_list() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "123");
    let mid = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_eq!(doc.text_of(leaf).unwrap(), "123");
    assert_ne!(mid.block, leaf);
    let out = type_chars(&mut doc, mid, "- ");
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaf).unwrap(), "123");
    assert_ne!(out.block, leaf);
    assert_eq!(out.offset, 0);
    assert_eq!(doc.text_of(out.block).unwrap(), "");
    assert!(has_list(&doc));
    let item_parent = doc
        .live_id(out.block)
        .and_then(|id| doc.arena.get(id).and_then(|n| n.parent));
    assert_eq!(
        item_parent.and_then(|p| doc.arena.get(p).map(|n| n.kind)),
        Some(BlockKind::ListItem)
    );
}

#[test]
fn enter_then_one_dot_space_wraps_ordered_list() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "hello");
    let mid = apply(&mut doc, Sel::collapsed(at), Command::Break);
    let out = apply(
        &mut doc,
        Sel::collapsed(mid),
        Command::Insert { text: "1. ".into() },
    );
    assert_eq!(doc.text_of(leaf).unwrap(), "hello");
    assert_ne!(out.block, leaf);
    assert_eq!(doc.text_of(out.block).unwrap(), "");
    let list = doc
        .preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .expect("list");
    assert!(doc.extra(list).ordered_start().is_some());
}

#[test]
fn typed_123456_enter_after_3_splits_at_caret() {
    let (mut doc, leaf) = fresh();
    let _ = type_chars(&mut doc, caret(leaf, 0), "123456");
    let out = apply(&mut doc, Sel::collapsed(caret(leaf, 3)), Command::Break);
    assert_eq!(doc.text_of(leaf).unwrap(), "123");
    assert_ne!(out.block, leaf);
    assert_eq!(out.offset, 0);
    assert_eq!(doc.text_of(out.block).unwrap(), "456");
}

#[test]
fn mid_enter_then_type_on_first_leaf_keeps_prefix() {
    let (mut doc, leaf) = fresh();
    let _ = type_chars(&mut doc, caret(leaf, 0), "123456");
    let next = apply(&mut doc, Sel::collapsed(caret(leaf, 3)), Command::Break);
    let out = type_chars(&mut doc, caret(leaf, 3), "x");
    assert_eq!(doc.text_of(leaf).unwrap(), "123x");
    assert_eq!(out, caret(leaf, 4));
    assert_eq!(doc.text_of(next.block).unwrap(), "456");
}

#[test]
fn select_soft_broken_123_blank_456_backspace_clears() {
    let (mut doc, leaf) = fresh();
    let _ = type_chars(&mut doc, caret(leaf, 0), "123456");
    let mid = apply(&mut doc, Sel::collapsed(caret(leaf, 3)), Command::SoftBreak);
    assert_eq!(mid, caret(leaf, 4));
    let _ = apply(&mut doc, Sel::collapsed(mid), Command::SoftBreak);
    assert_eq!(doc.text_of(leaf).unwrap(), "123\n\n456");
    let end = doc.text_of(leaf).unwrap().len();
    let out = apply(
        &mut doc,
        Sel {
            anchor: caret(leaf, 0),
            head: caret(leaf, end),
        },
        Command::DeleteBackward,
    );
    assert_eq!(doc.text_of(leaf).unwrap(), "");
    assert_eq!(out, caret(leaf, 0));
}

#[test]
fn enter_at_start_makes_leading_empty_paragraph() {
    let (mut doc, leaf) = fresh();
    let _ = type_chars(&mut doc, caret(leaf, 0), "abc");
    let out = apply(&mut doc, Sel::collapsed(caret(leaf, 0)), Command::Break);
    assert_eq!(doc.text_of(leaf).unwrap(), "");
    assert_ne!(out.block, leaf);
    assert_eq!(out.offset, 0);
    assert_eq!(doc.text_of(out.block).unwrap(), "abc");
}

#[test]
fn backspace_after_soft_break_removes_newline() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "123");
    let mid = apply(&mut doc, Sel::collapsed(at), Command::SoftBreak);
    let out = apply(&mut doc, Sel::collapsed(mid), Command::DeleteBackward);
    assert_eq!(doc.text_of(leaf).unwrap(), "123");
    assert_eq!(out, caret(leaf, 3));
}

#[test]
fn delete_forward_at_line_end_removes_newline() {
    let (mut doc, leaf) = fresh();
    let _ = type_chars(&mut doc, caret(leaf, 0), "abXcd");
    let _ = apply(&mut doc, Sel::collapsed(caret(leaf, 2)), Command::SoftBreak);
    assert_eq!(doc.text_of(leaf).unwrap(), "ab\nXcd");
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 2)),
        Command::DeleteForward,
    );
    assert_eq!(doc.text_of(leaf).unwrap(), "abXcd");
    assert_eq!(out, caret(leaf, 2));
}

#[test]
fn double_enter_at_end_splits_paragraph() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "123");
    let mid = apply(&mut doc, Sel::collapsed(at), Command::Break);
    let out = apply(&mut doc, Sel::collapsed(mid), Command::Break);
    assert_ne!(out.block, leaf);
    assert_eq!(out.offset, 0);
    assert_eq!(doc.text_of(leaf).unwrap(), "123");
    assert_eq!(doc.text_of(out.block).unwrap(), "");
    assert_eq!(doc.kind(out.block), Some(BlockKind::Paragraph));
}

#[test]
fn type_after_soft_break_appends_on_next_line() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "123");
    let mid = apply(&mut doc, Sel::collapsed(at), Command::SoftBreak);
    let out = type_chars(&mut doc, mid, "456");
    assert_eq!(doc.text_of(leaf).unwrap(), "123\n456");
    assert_eq!(out, caret(leaf, 7));
}

#[test]
fn soft_break_then_dash_space_wraps_new_line_as_list() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "123");
    let mid = apply(&mut doc, Sel::collapsed(at), Command::SoftBreak);
    assert_eq!(doc.text_of(leaf).unwrap(), "123\n");
    let out = type_chars(&mut doc, mid, "- ");
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaf).unwrap(), "123");
    assert_ne!(out.block, leaf);
    assert_eq!(doc.text_of(out.block).unwrap(), "");
    assert!(has_list(&doc));
}

#[test]
fn break_replaces_same_block_selection() {
    let (mut doc, leaf) = fresh();
    let _ = type_chars(&mut doc, caret(leaf, 0), "123456");
    let out = apply(
        &mut doc,
        Sel {
            anchor: caret(leaf, 2),
            head: caret(leaf, 4),
        },
        Command::Break,
    );
    assert_eq!(doc.text_of(leaf).unwrap(), "12");
    assert_ne!(out.block, leaf);
    assert_eq!(out.offset, 0);
    assert_eq!(doc.text_of(out.block).unwrap(), "56");
}

#[test]
fn utf8_enter_middle_keeps_caret_on_new_paragraph() {
    let (mut doc, leaf) = fresh();
    let _ = type_chars(&mut doc, caret(leaf, 0), "αβγδ");
    let mid = "αβ".len();
    let out = apply(&mut doc, Sel::collapsed(caret(leaf, mid)), Command::Break);
    assert_eq!(doc.text_of(leaf).unwrap(), "αβ");
    assert_ne!(out.block, leaf);
    assert_eq!(out.offset, 0);
    assert_eq!(doc.text_of(out.block).unwrap(), "γδ");
}

#[test]
fn select_across_newline_deletes_both_sides() {
    let (mut doc, leaf) = fresh();
    let _ = type_chars(&mut doc, caret(leaf, 0), "123456");
    let _ = apply(&mut doc, Sel::collapsed(caret(leaf, 3)), Command::SoftBreak);
    let out = apply(
        &mut doc,
        Sel {
            anchor: caret(leaf, 2),
            head: caret(leaf, 5),
        },
        Command::DeleteBackward,
    );
    assert_eq!(doc.text_of(leaf).unwrap(), "1256");
    assert_eq!(out, caret(leaf, 2));
}

#[test]
fn select_from_middle_to_end_with_newlines() {
    let (mut doc, leaf) = fresh();
    let _ = type_chars(&mut doc, caret(leaf, 0), "123456");
    let _ = apply(&mut doc, Sel::collapsed(caret(leaf, 3)), Command::SoftBreak);
    let _ = apply(&mut doc, Sel::collapsed(caret(leaf, 4)), Command::SoftBreak);
    let end = doc.text_of(leaf).unwrap().len();
    let out = apply(
        &mut doc,
        Sel {
            anchor: caret(leaf, 2),
            head: caret(leaf, end),
        },
        Command::DeleteBackward,
    );
    assert_eq!(doc.text_of(leaf).unwrap(), "12");
    assert_eq!(out, caret(leaf, 2));
}

#[test]
fn list_marker_on_a_soft_line_preserves_inline_source() {
    let mut doc = load_markdown("hello\n**hi**\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = doc.retarget_inline_focus_biased(caret(leaf, "hello\n".len()), FocusBias::Right);
    let out = apply(
        &mut doc,
        Sel::collapsed(at),
        Command::Insert { text: "- ".into() },
    );

    assert_eq!(doc.text_of(leaf), Some("hello"));
    let id = doc.live_id(out.block).expect("list paragraph");
    assert_eq!(doc.leaf_source(id), "**hi**");
    assert!(has_list(&doc));
    assert_eq!(doc.to_markdown(), "hello\n\n- **hi**\n");
}

#[test]
fn insert_over_newline_selection() {
    let (mut doc, leaf) = fresh();
    let _ = type_chars(&mut doc, caret(leaf, 0), "123456");
    let _ = apply(&mut doc, Sel::collapsed(caret(leaf, 3)), Command::SoftBreak);
    let out = apply(
        &mut doc,
        Sel {
            anchor: caret(leaf, 2),
            head: caret(leaf, 5),
        },
        Command::Insert { text: "X".into() },
    );
    assert_eq!(doc.text_of(leaf).unwrap(), "12X56");
    assert_eq!(out, caret(leaf, 3));
}

#[test]
fn trailing_space_then_enter_keeps_space() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "hello ");
    let out = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_eq!(doc.text_of(leaf).unwrap(), "hello ");
    assert_ne!(out.block, leaf);
    assert_eq!(out.offset, 0);
    assert_eq!(doc.text_of(out.block).unwrap(), "");
}

#[test]
fn empty_paragraph_enter_creates_sibling() {
    let (mut doc, leaf) = fresh();
    let out = apply(&mut doc, Sel::collapsed(caret(leaf, 0)), Command::Break);
    assert_ne!(out.block, leaf);
    assert_eq!(out.offset, 0);
    assert_eq!(doc.text_of(leaf).unwrap(), "");
    assert_eq!(doc.kind(out.block), Some(BlockKind::Paragraph));
}

#[test]
fn two_paragraphs_select_across_backspace_clears() {
    let mut doc = load_markdown("123\n\n456\n", editor_options());
    let leaves = doc.text_leaves();
    assert_eq!(leaves.len(), 2);
    let a = leaves[0];
    let b = leaves[1];
    assert_eq!(doc.text_of(a).unwrap(), "123");
    assert_eq!(doc.text_of(b).unwrap(), "456");
    let out = apply(
        &mut doc,
        Sel {
            anchor: caret(a, 0),
            head: caret(b, 3),
        },
        Command::DeleteBackward,
    );
    assert_eq!(doc.text_of(out.block).unwrap(), "");
    assert_eq!(out.offset, 0);
    assert_eq!(doc.text_leaves().len(), 1);
}

#[test]
fn cross_block_delete_preserves_inline_source_in_the_suffix() {
    let mut doc = load_markdown("ab\n\nx**bold**\n", editor_options());
    let leaves = doc.text_leaves();
    let first = leaves[0];
    let last = leaves[1];

    let out = apply(
        &mut doc,
        Sel {
            anchor: caret(first, 1),
            head: caret(last, 1),
        },
        Command::DeleteBackward,
    );

    assert_eq!(out, caret(first, 1));
    let id = doc.live_id(first).expect("surviving paragraph");
    assert_eq!(doc.leaf_source(id), "a**bold**");
    assert!(
        doc.runs(id)
            .iter()
            .any(|run| run.marks.contains(InlineMarks::STRONG))
    );
    assert_eq!(doc.to_markdown(), "a**bold**\n");
}

#[test]
fn merge_then_type_at_end_keeps_joined_text() {
    let mut doc = load_markdown("ab\n\ncd\n", editor_options());
    let leaves = doc.text_leaves();
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaves[1], 0)),
        Command::DeleteBackward,
    );
    assert_eq!(doc.text_of(out.block).unwrap(), "abcd");
    assert_eq!(out.offset, 2);
    let end = doc.text_of(out.block).unwrap().len();
    let typed = type_chars(&mut doc, caret(out.block, end), "x");
    assert_eq!(doc.text_of(out.block).unwrap(), "abcdx");
    assert_eq!(typed.offset, 5);
}

#[test]
fn caret_tracks_each_typed_digit() {
    let (mut doc, leaf) = fresh();
    let mut at = caret(leaf, 0);
    for (i, ch) in ["1", "2", "3", "4", "5", "6"].into_iter().enumerate() {
        at = type_chars(&mut doc, at, ch);
        assert_eq!(at.offset, i + 1, "after {ch}");
        assert_eq!(doc.text_of(leaf).unwrap().len(), i + 1);
    }
}

#[test]
fn backspace_through_newline_then_digits() {
    let (mut doc, leaf) = fresh();
    let _ = type_chars(&mut doc, caret(leaf, 0), "123456");
    let _ = apply(&mut doc, Sel::collapsed(caret(leaf, 3)), Command::SoftBreak);
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 4)),
        Command::DeleteBackward,
    );
    assert_eq!(doc.text_of(leaf).unwrap(), "123456");
    assert_eq!(out, caret(leaf, 3));
}

#[test]
fn heading_enter_puts_caret_on_new_paragraph() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "# title");
    let out = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_eq!(doc.kind(leaf), Some(BlockKind::Heading(1)));
    assert_ne!(out.block, leaf);
    assert_eq!(out.offset, 0);
    assert_eq!(doc.text_of(out.block).unwrap(), "");
}

#[test]
fn select_all_heading_body_deletes_body() {
    let (mut doc, leaf) = fresh();
    let _ = type_chars(&mut doc, caret(leaf, 0), "### title");
    let n = doc.text_of(leaf).unwrap().len();
    let out = apply(
        &mut doc,
        Sel {
            anchor: caret(leaf, 0),
            head: caret(leaf, n),
        },
        Command::DeleteBackward,
    );
    assert_eq!(doc.text_of(leaf).unwrap(), "");
    assert_eq!(out, caret(leaf, 0));
}

#[test]
fn insert_in_empty_second_line_then_type() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "xy");
    let mid = apply(&mut doc, Sel::collapsed(at), Command::SoftBreak);
    let out = type_chars(&mut doc, mid, "z");
    assert_eq!(doc.text_of(leaf).unwrap(), "xy\nz");
    assert_eq!(out, caret(leaf, 4));
}
