use super::support::{assert_changeset_parents_live, caret, first_list, items, lead};
use crate::block::BlockKind;
use crate::document::change::DocChange;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{editor_options, load_markdown};
use crate::inline::InlineMarks;

#[test]
fn backspace_joins_simple_items() {
    let mut doc = load_markdown("- a\n- b\n", editor_options());
    let list = first_list(&doc);
    let first = items(&doc, list)[0];
    let second = items(&doc, list)[1];
    let leaf = lead(&doc, second);
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 0)),
        Command::DeleteBackward,
    );
    assert_eq!(items(&doc, list).len(), 1);
    assert_eq!(items(&doc, list)[0], first);
    assert_eq!(doc.display(lead(&doc, first)), "ab");
    assert_eq!(out.offset, 1);
    assert_eq!(out.block, lead(&doc, first).index);
    assert_eq!(doc.arena.children(first).count(), 1);
}

#[test]
fn joining_items_preserves_inline_source() {
    let mut doc = load_markdown("- a\n- **b**\n", editor_options());
    let list = first_list(&doc);
    let second = items(&doc, list)[1];
    let leaf = lead(&doc, second);
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 0)),
        Command::DeleteBackward,
    );

    let id = doc.live_id(out.block).expect("joined leaf");
    assert_eq!(doc.display(id), "ab");
    assert_eq!(doc.leaf_source(id), "a**b**");
    assert!(
        doc.runs(id)
            .iter()
            .any(|run| run.marks.contains(InlineMarks::STRONG))
    );
    let markdown = doc.to_markdown();
    assert!(markdown.contains("a**b**"), "{markdown:?}");
}

#[test]
fn first_item_backspace_does_not_join_preceding_para() {
    let mut doc = load_markdown("hello\n\n- abc\n- d\n", editor_options());
    let list = first_list(&doc);
    let first = items(&doc, list)[0];
    let leaf = lead(&doc, first);
    let host = doc.arena.get(list).and_then(|n| n.parent);
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 0)),
        Command::DeleteBackward,
    );
    assert_eq!(doc.text_leaves().len(), 3);
    assert_eq!(doc.text_of(doc.text_leaves()[0]).unwrap(), "hello");
    assert_eq!(items(&doc, list).len(), 1);
    let leaf_id = doc.live_id(out.block).expect("leaf");
    assert_eq!(doc.display(leaf_id), "abc");
    assert_eq!(doc.arena.get(leaf_id).and_then(|n| n.parent), host);
    assert_eq!(
        doc.arena.get(list).and_then(|n| n.next_sibling),
        Some(leaf_id)
    );
}

#[test]
fn sole_item_backspace_removes_list() {
    let mut doc = load_markdown("- abc\n", editor_options());
    let list = first_list(&doc);
    let host = doc.arena.get(list).and_then(|n| n.parent);
    let leaf = lead(&doc, items(&doc, list)[0]);
    let _ = doc.take_changes();
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 0)),
        Command::DeleteBackward,
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
    assert_eq!(doc.display(leaf_id), "abc");
    let changes = doc.take_changes();
    assert_changeset_parents_live(&doc, &changes);
    assert!(changes.changes.iter().any(|c| matches!(
        c,
        DocChange::TreeSpliced {
            parent,
            removed,
            inserted,
            ..
        } if *parent == host.unwrap()
            && removed.as_slice() == [list]
            && inserted.as_slice() == [leaf_id]
    )));
}

#[test]
fn nested_first_item_backspace_lifts_one_rank() {
    let mut doc = load_markdown("- a\n  - b\n  - c\n", editor_options());
    let lists: Vec<_> = doc
        .preorder()
        .into_iter()
        .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .collect();
    let outer = lists[0];
    let inner = lists[1];
    let b_item = items(&doc, inner)[0];
    let b_leaf = lead(&doc, b_item);
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(b_leaf.index, 0)),
        Command::DeleteBackward,
    );
    assert_eq!(items(&doc, inner).len(), 1);
    assert_eq!(doc.display(lead(&doc, items(&doc, inner)[0])), "c");
    let outer_items = items(&doc, outer);
    assert_eq!(outer_items.len(), 2);
    assert_eq!(outer_items[1], b_item);
    assert_eq!(out.block, b_leaf.index);
    assert_eq!(doc.display(b_leaf), "b");
}

#[test]
fn backspace_joins_loose_item_paragraphs() {
    let mut doc = load_markdown("- a\n\n  b\n", editor_options());
    let list = first_list(&doc);
    let item = items(&doc, list)[0];
    assert_eq!(doc.arena.children(item).count(), 2);
    assert!(doc.extra(list).list_loose());
    let second = doc.arena.children(item).nth(1).expect("second");
    let _ = doc.take_changes();
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(second.index, 0)),
        Command::DeleteBackward,
    );
    assert_eq!(items(&doc, list).len(), 1);
    assert_eq!(doc.arena.children(item).count(), 1);
    assert_eq!(doc.display(lead(&doc, item)), "ab");
    assert_eq!(out.offset, 1);
    assert!(!doc.extra(list).list_loose());
    let changes = doc.take_changes();
    assert!(
        changes
            .changes
            .iter()
            .any(|c| matches!(c, DocChange::AttrsChanged { node, .. } if *node == list))
    );
}

#[test]
fn joining_sibling_image_preserves_its_url() {
    let mut doc = load_markdown("- a\n\n  ![alt](image.png)\n", editor_options());
    let list = first_list(&doc);
    let item = items(&doc, list)[0];
    let image = doc.arena.children(item).nth(1).expect("image");
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(image.index, 0)),
        Command::DeleteBackward,
    );

    let id = doc.live_id(out.block).expect("joined leaf");
    assert_eq!(doc.leaf_source(id), "a![alt](image.png)");
    let link = doc
        .runs(id)
        .iter()
        .find(|run| run.marks.contains(InlineMarks::IMAGE))
        .and_then(|run| run.link)
        .expect("image link");
    assert_eq!(doc.link_dest(link), Some("image.png"));
    let markdown = doc.to_markdown();
    assert!(markdown.contains("a![alt](image.png)"), "{markdown:?}");
}
