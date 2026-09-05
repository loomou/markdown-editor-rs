use super::support::{assert_changeset_parents_live, caret, first_list, items, lead};
use crate::block::BlockKind;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{editor_options, load_markdown};

#[test]
fn insert_dash_space_wraps_empty_paragraph() {
    let mut doc = load_markdown("x\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let host = doc
        .live_id(leaf)
        .and_then(|id| doc.arena.get(id).and_then(|n| n.parent));
    let _ = doc.replace_text(leaf, 0..1, "");
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert { text: "- ".into() },
    );
    let list = first_list(&doc);
    assert_eq!(doc.arena.get(list).and_then(|n| n.parent), host);
    assert!(doc.extra(list).ordered_start().is_none());
    let item = items(&doc, list)[0];
    assert_eq!(lead(&doc, item).index, leaf);
    assert_eq!(doc.display(lead(&doc, item)), "");
    assert_eq!(out, caret(leaf, 0));
    assert!(doc.extra(item).task_checked().is_none());
}

#[test]
fn insert_dash_space_backspace_unwraps_list() {
    let mut doc = load_markdown("x\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let host = doc
        .live_id(leaf)
        .and_then(|id| doc.arena.get(id).and_then(|n| n.parent));
    let _ = doc.replace_text(leaf, 0..1, "");
    let wrapped = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert { text: "- ".into() },
    );
    let _ = doc.take_changes();
    let out = apply(&mut doc, Sel::collapsed(wrapped), Command::DeleteBackward);
    assert_eq!(out, caret(leaf, 0));
    assert_eq!(
        doc.preorder()
            .into_iter()
            .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
            .count(),
        0
    );
    let leaf_id = doc.live_id(out.block).expect("leaf");
    assert_eq!(doc.arena.get(leaf_id).and_then(|n| n.parent), host);
    assert_eq!(doc.display(leaf_id), "");
    let changes = doc.take_changes();
    assert_changeset_parents_live(&doc, &changes);
}

#[test]
fn insert_one_dot_wraps_ordered_list() {
    let mut doc = load_markdown("x\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = doc.replace_text(leaf, 0..1, "");
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert { text: "1. ".into() },
    );
    let list = first_list(&doc);
    assert_eq!(doc.extra(list).ordered_start(), Some(1));
    assert_eq!(doc.display(lead(&doc, items(&doc, list)[0])), "");
}

#[test]
fn insert_task_marker_wraps_unchecked_task() {
    let mut doc = load_markdown("x\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = doc.replace_text(leaf, 0..1, "");
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert {
            text: "- [ ] ".into(),
        },
    );
    let list = first_list(&doc);
    let item = items(&doc, list)[0];
    assert_eq!(doc.extra(item).task_checked(), Some(false));
    assert_eq!(doc.display(lead(&doc, item)), "");
}

#[test]
fn insert_dash_space_inside_item_stays_text() {
    let mut doc = load_markdown("- a\n", editor_options());
    let list = first_list(&doc);
    let item = items(&doc, list)[0];
    let leaf = lead(&doc, item);
    let _ = doc.replace_text(leaf.index, 0..1, "");
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 0)),
        Command::Insert { text: "- ".into() },
    );
    assert_eq!(
        doc.preorder()
            .into_iter()
            .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
            .count(),
        1
    );
    assert_eq!(items(&doc, list)[0], item);
    assert_eq!(doc.display(leaf), "- ");
    assert_eq!(out.offset, 2);
}

#[test]
fn insert_dash_then_space_wraps() {
    let mut doc = load_markdown("x\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = doc.replace_text(leaf, 0..1, "");
    let mid = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert { text: "-".into() },
    );
    assert_eq!(
        doc.preorder()
            .into_iter()
            .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
            .count(),
        0
    );
    assert_eq!(doc.text_of(leaf).unwrap(), "-");
    let out = apply(
        &mut doc,
        Sel::collapsed(mid),
        Command::Insert { text: " ".into() },
    );
    assert_eq!(items(&doc, first_list(&doc)).len(), 1);
    assert_eq!(doc.display(doc.live_id(leaf).expect("leaf")), "");
    assert_eq!(out, caret(leaf, 0));
}

#[test]
fn insert_dash_space_in_quote_stays_in_quote() {
    let mut doc = load_markdown("> x\n", editor_options());
    let quote = doc
        .preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::BlockQuote))
        .expect("quote");
    let leaf = doc.text_leaves()[0];
    let _ = doc.replace_text(leaf, 0..1, "");
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert { text: "- ".into() },
    );
    let list = first_list(&doc);
    assert_eq!(doc.arena.get(list).and_then(|n| n.parent), Some(quote));
}

#[test]
fn insert_dash_space_on_heading_stays_text() {
    let mut doc = load_markdown("# x\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = doc.replace_text(leaf, 0..1, "");
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert { text: "- ".into() },
    );
    assert_eq!(doc.kind(leaf), Some(BlockKind::Heading(1)));
    assert_eq!(doc.text_of(leaf).unwrap(), "- ");
    assert_eq!(
        doc.preorder()
            .into_iter()
            .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
            .count(),
        0
    );
}
