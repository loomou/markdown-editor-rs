use super::support::{caret, first_list, items};
use crate::block::BlockKind;
use crate::doc::Doc;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{editor_options, load_markdown};

fn span(doc: &crate::document::Document, a: usize, ao: usize, b: usize, bo: usize) -> Sel {
    let leaves = doc.text_leaves();
    Sel {
        anchor: caret(leaves[a], ao),
        head: caret(leaves[b], bo),
    }
}

fn list_count(doc: &crate::document::Document) -> usize {
    doc.preorder()
        .into_iter()
        .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .count()
}

fn item_count(doc: &crate::document::Document) -> usize {
    doc.preorder()
        .into_iter()
        .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::ListItem))
        .count()
}

#[test]
fn select_all_list_items_unwraps_list() {
    let mut doc = load_markdown("- a\n- b\n- c\n", editor_options());
    let last = *doc.text_leaves().last().expect("last");
    let end = doc.text_of(last).unwrap().len();
    let sel = span(&doc, 0, 0, 2, end);
    let out = apply(&mut doc, sel, Command::DeleteBackward);
    assert_eq!(list_count(&doc), 0);
    assert_eq!(item_count(&doc), 0);
    assert_eq!(doc.kind(out.block), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(out.block).unwrap(), "");
    assert_eq!(out.offset, 0);
}

#[test]
fn select_all_nested_list_unwraps_list() {
    let mut doc = load_markdown("- a\n  - b\n  - c\n- d\n", editor_options());
    assert!(list_count(&doc) >= 2);
    let last = *doc.text_leaves().last().expect("last");
    let end = doc.text_of(last).unwrap().len();
    let n = doc.text_leaves().len();
    let sel = span(&doc, 0, 0, n - 1, end);
    let out = apply(&mut doc, sel, Command::DeleteBackward);
    assert_eq!(list_count(&doc), 0);
    assert_eq!(item_count(&doc), 0);
    assert_eq!(doc.kind(out.block), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(out.block).unwrap(), "");
}

#[test]
fn select_all_single_item_unwraps_list() {
    let mut doc = load_markdown("- hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let end = doc.text_of(leaf).unwrap().len();
    let sel = span(&doc, 0, 0, 0, end);
    let out = apply(&mut doc, sel, Command::DeleteBackward);
    assert_eq!(list_count(&doc), 0);
    assert_eq!(item_count(&doc), 0);
    assert_eq!(doc.kind(out.block), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(out.block).unwrap(), "");
}

#[test]
fn clearing_one_item_among_siblings_keeps_the_list() {
    let mut doc = load_markdown("- a\n- hello\n- c\n", editor_options());
    let mid = doc.text_leaves()[1];
    let end = doc.text_of(mid).unwrap().len();
    let sel = span(&doc, 1, 0, 1, end);
    let _ = apply(&mut doc, sel, Command::DeleteBackward);
    assert_eq!(list_count(&doc), 1);
    assert_eq!(items(&doc, first_list(&doc)).len(), 3);
    assert_eq!(doc.text_of(mid).unwrap(), "");
}

#[test]
fn select_all_list_delete_undo_restores_items() {
    let mut d = Doc::new(load_markdown("- a\n- b\n", editor_options()));
    let last = *d.text_leaves().last().expect("last");
    let end = d.document.text_of(last).unwrap().len();
    let sel = span(&d.document, 0, 0, 1, end);
    let _ = d.apply(sel, Command::DeleteBackward);
    assert_eq!(list_count(&d.document), 0);
    let _ = d.undo().expect("undo");
    assert_eq!(list_count(&d.document), 1);
    assert_eq!(d.document.text_of(d.text_leaves()[0]).unwrap(), "a");
    assert_eq!(d.document.text_of(d.text_leaves()[1]).unwrap(), "b");
}

#[test]
fn select_across_list_and_paragraph_drops_the_list() {
    let mut doc = load_markdown("- a\n- b\n\nhello\n", editor_options());
    let last = *doc.text_leaves().last().expect("last");
    let end = doc.text_of(last).unwrap().len();
    let n = doc.text_leaves().len();
    let sel = span(&doc, 0, 0, n - 1, end);
    let out = apply(&mut doc, sel, Command::DeleteBackward);
    assert_eq!(list_count(&doc), 0);
    assert_eq!(item_count(&doc), 0);
    assert_eq!(doc.kind(out.block), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(out.block).unwrap(), "");
}

fn editor_select_content(d: &Doc) -> Sel {
    let mut leaves = d.text_leaves();
    if leaves.len() > 1 {
        let last = *leaves.last().expect("last");
        if d.kind(last) == Some(BlockKind::Paragraph)
            && d.caret_text(last).is_some_and(|t| t.is_empty())
        {
            leaves.pop();
        }
    }
    let first = leaves[0];
    let last = *leaves.last().expect("last");
    let end = d.caret_text(last).map_or(0, |t| t.len());
    Sel {
        anchor: caret(first, 0),
        head: caret(last, end),
    }
}

#[test]
fn select_all_list_delete_then_break_with_trailing_blank() {
    let mut d = Doc::new(load_markdown("- a\n- b\n- c\n", editor_options()));
    d.enable_trailing_blank();
    let after = d.apply(editor_select_content(&d), Command::DeleteBackward);
    assert!(
        d.document.live_id(after.block).is_some(),
        "caret {after:?} is tombstoned after list select-all delete"
    );
    let out = d.apply(Sel::collapsed(after), Command::Break);
    assert!(d.document.live_id(out.block).is_some());
    assert_eq!(list_count(&d.document), 0);
}

#[test]
fn select_all_nested_list_delete_then_break_with_trailing_blank() {
    let mut d = Doc::new(load_markdown("- a\n  - b\n  - c\n- d\n", editor_options()));
    d.enable_trailing_blank();
    let after = d.apply(editor_select_content(&d), Command::DeleteBackward);
    assert!(
        d.document.live_id(after.block).is_some(),
        "caret {after:?} is tombstoned after nested list select-all delete"
    );
    let out = d.apply(Sel::collapsed(after), Command::Break);
    assert!(d.document.live_id(out.block).is_some());
    assert_eq!(list_count(&d.document), 0);
}

#[test]
fn typed_list_select_all_delete_then_break() {
    let mut d = Doc::new(load_markdown("", editor_options()));
    d.enable_trailing_blank();
    let leaf = d.text_leaves()[0];
    let mut c = d.apply(
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert { text: "- ".into() },
    );
    c = d.apply(Sel::collapsed(c), Command::Insert { text: "a".into() });
    c = d.apply(Sel::collapsed(c), Command::Break);
    c = d.apply(Sel::collapsed(c), Command::Insert { text: "b".into() });
    c = d.apply(Sel::collapsed(c), Command::Break);
    let _ = d.apply(Sel::collapsed(c), Command::Insert { text: "c".into() });
    assert!(list_count(&d.document) >= 1);
    let after = d.apply(editor_select_content(&d), Command::DeleteBackward);
    assert!(
        d.document.live_id(after.block).is_some(),
        "caret {after:?} is tombstoned after typed-list select-all delete; leaves={:?}",
        d.text_leaves()
    );
    let out = d.apply(Sel::collapsed(after), Command::Break);
    assert!(d.document.live_id(out.block).is_some());
}

#[test]
fn delete_prefix_items_then_break_keeps_live_caret() {
    let mut d = Doc::new(load_markdown("- a\n- b\n- c\n", editor_options()));
    d.enable_trailing_blank();
    let leaves = d.text_leaves();
    let end = d.document.text_of(leaves[1]).unwrap().len();
    let sel = Sel {
        anchor: caret(leaves[0], 0),
        head: caret(leaves[1], end),
    };
    let after = d.apply(sel, Command::DeleteBackward);
    assert!(
        d.document.live_id(after.block).is_some(),
        "caret {after:?} is tombstoned after deleting the first two items"
    );
    let out = d.apply(Sel::collapsed(after), Command::Break);
    assert!(d.document.live_id(out.block).is_some());
    assert_eq!(list_count(&d.document), 1);
}

#[test]
fn deleting_list_text_preserves_an_unselected_empty_table() {
    let mut doc = Doc::new(load_markdown(
        "- abc\n\n  |   |\n  | --- |\n  |   |\n",
        editor_options(),
    ));
    let leaf = doc.text_leaves()[0];
    let count_cells = |doc: &Doc| {
        doc.text_leaves()
            .into_iter()
            .filter(|&id| doc.kind(id) == Some(BlockKind::TableCell))
            .count()
    };
    assert_eq!(count_cells(&doc), 2);
    let _ = doc.apply(
        Sel {
            anchor: caret(leaf, 0),
            head: caret(leaf, 3),
        },
        Command::DeleteBackward,
    );
    assert_eq!(count_cells(&doc), 2, "the table was outside the selection");
}
