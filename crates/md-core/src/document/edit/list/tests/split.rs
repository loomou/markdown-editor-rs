use super::support::{caret, first_list, items, lead};
use crate::block::BlockKind;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{editor_options, load_markdown};

#[test]
fn break_splits_simple_item_keeping_old_id() {
    let mut doc = load_markdown("- abc\n", editor_options());
    let list = first_list(&doc);
    let item = items(&doc, list)[0];
    let leaf = lead(&doc, item);
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 2)),
        Command::Break,
    );
    let after = items(&doc, list);
    assert_eq!(after.len(), 2);
    assert_eq!(after[0], item);
    assert_eq!(doc.display(leaf), "ab");
    assert_eq!(out.offset, 0);
    assert_eq!(doc.text_of(out.block).unwrap(), "c");
    assert_eq!(lead(&doc, after[1]).index, out.block);
}

#[test]
fn break_at_end_makes_empty_next_item() {
    let mut doc = load_markdown("- ab\n", editor_options());
    let list = first_list(&doc);
    let item = items(&doc, list)[0];
    let leaf = lead(&doc, item);
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 2)),
        Command::Break,
    );
    let after = items(&doc, list);
    assert_eq!(after.len(), 2);
    assert_eq!(after[0], item);
    assert_eq!(doc.display(leaf), "ab");
    assert_eq!(doc.text_of(out.block).unwrap(), "");
}

#[test]
fn empty_item_break_exits_list() {
    let mut doc = load_markdown("- ab\n", editor_options());
    let list = first_list(&doc);
    let item = items(&doc, list)[0];
    let host = doc.arena.get(list).and_then(|n| n.parent);
    let leaf = lead(&doc, item);
    let empty = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 2)),
        Command::Break,
    );
    assert_eq!(items(&doc, list).len(), 2);
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(empty.block, 0)),
        Command::Break,
    );
    assert_eq!(items(&doc, list).len(), 1);
    assert_eq!(items(&doc, list)[0], item);
    assert_eq!(out.block, empty.block);
    assert_eq!(doc.text_of(out.block).unwrap(), "");
    let leaf_id = doc.live_id(out.block).expect("leaf");
    assert_eq!(doc.arena.get(leaf_id).and_then(|n| n.parent), host);
    assert_eq!(
        doc.arena.get(list).and_then(|n| n.next_sibling),
        Some(leaf_id)
    );
}

#[test]
fn sole_empty_item_removes_list() {
    let mut doc = load_markdown("- a\n", editor_options());
    let list = first_list(&doc);
    let host = doc.arena.get(list).and_then(|n| n.parent);
    let leaf = lead(&doc, items(&doc, list)[0]);
    let _ = doc.replace_text(leaf.index, 0..1, "");
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 0)),
        Command::Break,
    );
    assert_eq!(
        doc.preorder()
            .into_iter()
            .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
            .count(),
        0
    );
    let leaf_id = doc.live_id(out.block).expect("leaf");
    assert_eq!(doc.arena.get(leaf_id).and_then(|n| n.parent), host);
    assert_eq!(doc.text_of(out.block).unwrap(), "");
}

#[test]
fn loose_item_break_splits_following_block() {
    let mut doc = load_markdown("- abc\n\n  d\n", editor_options());
    let list = first_list(&doc);
    let item = items(&doc, list)[0];
    let leaf = lead(&doc, item);
    let second = doc.arena.children(item).nth(1).expect("second");
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 1)),
        Command::Break,
    );
    let after = items(&doc, list);
    assert_eq!(after.len(), 2);
    assert_eq!(after[0], item);
    assert_eq!(doc.display(leaf), "a");
    assert_eq!(doc.text_of(out.block).unwrap(), "bc");
    assert_eq!(doc.arena.children(item).count(), 1);
    let kids: Vec<_> = doc.arena.children(after[1]).collect();
    assert_eq!(kids.len(), 2);
    assert_eq!(kids[0].index, out.block);
    assert_eq!(kids[1], second);
    assert_eq!(doc.display(second), "d");
}

#[test]
fn nested_simple_item_splits_inner_list() {
    let mut doc = load_markdown("- a\n  - bc\n", editor_options());
    let lists: Vec<_> = doc
        .preorder()
        .into_iter()
        .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .collect();
    assert_eq!(lists.len(), 2);
    let inner = lists[1];
    let item = items(&doc, inner)[0];
    let leaf = lead(&doc, item);
    assert_eq!(doc.display(leaf), "bc");
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 1)),
        Command::Break,
    );
    assert_eq!(items(&doc, lists[0]).len(), 1);
    let after = items(&doc, inner);
    assert_eq!(after.len(), 2);
    assert_eq!(after[0], item);
    assert_eq!(doc.display(leaf), "b");
    assert_eq!(doc.text_of(out.block).unwrap(), "c");
}

#[test]
fn break_moves_trailing_nested_list() {
    let mut doc = load_markdown("- abc\n  - d\n", editor_options());
    let lists: Vec<_> = doc
        .preorder()
        .into_iter()
        .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .collect();
    assert_eq!(lists.len(), 2);
    let outer = lists[0];
    let inner = lists[1];
    let item = items(&doc, outer)[0];
    let leaf = lead(&doc, item);
    assert_eq!(doc.display(leaf), "abc");
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 1)),
        Command::Break,
    );
    let after = items(&doc, outer);
    assert_eq!(after.len(), 2);
    assert_eq!(after[0], item);
    assert_eq!(doc.display(leaf), "a");
    assert_eq!(doc.text_of(out.block).unwrap(), "bc");
    assert_eq!(doc.arena.children(item).count(), 1);
    let kids: Vec<_> = doc.arena.children(after[1]).collect();
    assert_eq!(kids.len(), 2);
    assert_eq!(kids[0].index, out.block);
    assert_eq!(kids[1], inner);
    assert_eq!(doc.display(lead(&doc, items(&doc, inner)[0])), "d");
}

#[test]
fn task_item_break_copies_unchecked_task() {
    let mut doc = load_markdown("- [x] ab\n", editor_options());
    let list = first_list(&doc);
    let item = items(&doc, list)[0];
    assert_eq!(doc.extra(item).task_checked(), Some(true));
    let leaf = lead(&doc, item);
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 1)),
        Command::Break,
    );
    let after = items(&doc, list);
    assert_eq!(after.len(), 2);
    assert_eq!(after[0], item);
    assert_eq!(doc.extra(item).task_checked(), Some(true));
    assert_eq!(doc.extra(after[1]).task_checked(), Some(false));
}

#[test]
fn empty_nested_item_lifts_one_rank() {
    let mut doc = load_markdown("- a\n  - b\n", editor_options());
    let lists: Vec<_> = doc
        .preorder()
        .into_iter()
        .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .collect();
    let outer = lists[0];
    let inner = lists[1];
    let b_item = items(&doc, inner)[0];
    let b_leaf = lead(&doc, b_item);
    let empty = apply(
        &mut doc,
        Sel::collapsed(caret(b_leaf.index, 1)),
        Command::Break,
    );
    assert_eq!(items(&doc, inner).len(), 2);
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(empty.block, 0)),
        Command::Break,
    );
    assert_eq!(items(&doc, inner).len(), 1);
    assert_eq!(items(&doc, inner)[0], b_item);
    let outer_items = items(&doc, outer);
    assert_eq!(outer_items.len(), 2);
    assert_eq!(lead(&doc, outer_items[1]).index, out.block);
    assert_eq!(doc.text_of(out.block).unwrap(), "");
    assert_eq!(
        doc.preorder()
            .into_iter()
            .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
            .count(),
        2
    );
}

#[test]
fn empty_inner_only_item_prunes_inner_list() {
    let mut doc = load_markdown("- a\n  - b\n", editor_options());
    let lists: Vec<_> = doc
        .preorder()
        .into_iter()
        .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .collect();
    let outer = lists[0];
    let inner = lists[1];
    let b_item = items(&doc, inner)[0];
    let b_leaf = lead(&doc, b_item);
    let _ = doc.replace_text(b_leaf.index, 0..1, "");
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(b_leaf.index, 0)),
        Command::Break,
    );
    assert_eq!(items(&doc, outer).len(), 2);
    assert_eq!(lead(&doc, items(&doc, outer)[1]).index, out.block);
    assert_eq!(
        doc.preorder()
            .into_iter()
            .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
            .count(),
        1
    );
}

#[test]
fn empty_item_in_quote_stays_in_quote() {
    let mut doc = load_markdown("> - ab\n", editor_options());
    let quote = doc
        .preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::BlockQuote))
        .expect("quote");
    let list = first_list(&doc);
    let item = items(&doc, list)[0];
    let leaf = lead(&doc, item);
    let empty = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 2)),
        Command::Break,
    );
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(empty.block, 0)),
        Command::Break,
    );
    let leaf_id = doc.live_id(out.block).expect("leaf");
    assert_eq!(doc.arena.get(leaf_id).and_then(|n| n.parent), Some(quote));
    assert_eq!(doc.arena.get(list).and_then(|n| n.parent), Some(quote));
    assert_eq!(
        doc.arena.get(list).and_then(|n| n.next_sibling),
        Some(leaf_id)
    );
    assert_eq!(items(&doc, list).len(), 1);
    assert_eq!(items(&doc, list)[0], item);
}

#[test]
fn break_with_a_nested_list_then_undo_restores_the_subtree() {
    use crate::doc::Doc;

    let mut doc = Doc::new(load_markdown("- first\n  - nested\n", editor_options()));
    let leaf = doc.text_leaves()[0];
    let _ = doc.take_changes();
    let _ = doc.apply(Sel::collapsed(caret(leaf, 3)), Command::Break);
    let split = doc.document.to_markdown();
    assert!(
        split.contains("- st") && split.contains("- nested"),
        "the nested list must follow the tail item after the split: {split:?}"
    );

    let restored = doc.undo();
    assert!(restored.is_some(), "the split must be undoable");
    assert_eq!(
        doc.document.to_markdown(),
        "- first\n  - nested\n",
        "undo must return the nested list to the original item"
    );
}
