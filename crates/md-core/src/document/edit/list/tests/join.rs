use super::support::{assert_changeset_parents_live, caret, first_list, items, lead};
use crate::block::BlockKind;
use crate::doc::Doc;
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
        doc.arena.get(list).and_then(|n| n.prev_sibling),
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
    let outer_items = items(&doc, outer);
    assert_eq!(outer_items.len(), 2);
    assert_eq!(outer_items[1], b_item);
    assert_eq!(out.block, b_leaf.index);
    assert_eq!(doc.display(b_leaf), "b");
    let b_kids: Vec<_> = doc.arena.children(b_item).collect();
    let b_sublist = b_kids
        .into_iter()
        .find(|&id| doc.arena.get(id).is_some_and(|n| n.kind == BlockKind::List))
        .expect("b carries its tail as a sublist");
    assert_eq!(
        doc.display(lead(&doc, items(&doc, b_sublist)[0])),
        "c",
        "reading order must stay a, b, c: {:?}",
        doc.to_markdown()
    );
    assert_eq!(doc.to_markdown(), "- a\n- b\n  - c\n");
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

#[test]
fn outdenting_a_middle_nested_item_keeps_tail_order() {
    use crate::doc::Doc;
    let mut d = Doc::new(load_markdown(
        "- parent\n  - a\n  - b\n  - c\n- tail\n",
        editor_options(),
    ));
    let before = d.document.to_markdown();
    let block = d.text_leaves()[2];
    let _ = d.apply(Sel::collapsed(caret(block, 0)), Command::Outdent);
    let after = d.document.to_markdown();
    assert_eq!(after, "- parent\n  - a\n- b\n  - c\n- tail\n", "{after:?}");
    let order: Vec<&str> = d
        .document
        .text_leaves()
        .iter()
        .map(|&id| d.collapsed_text(id).unwrap_or(""))
        .collect();
    assert_eq!(order, ["parent", "a", "b", "c", "tail"], "{after:?}");
    assert!(d.undo().is_some());
    assert_eq!(d.document.to_markdown(), before, "undo must restore");
    assert!(d.redo().is_some());
    assert_eq!(d.document.to_markdown(), after, "redo must replay");
}

#[test]
fn joining_items_redo_keeps_the_migrated_children() {
    for (source, at_leaf, joined) in [
        ("- a\n- b\n  - c\n- d\n", 1usize, "- ab\n  \n  - c\n\n- d\n"),
        (
            "- parent\n  - a\n  - b\n  - c\n- tail\n",
            4,
            "- parent\n  \n  - a\n  - b\n  - c\n  \n  tail\n",
        ),
        ("- a\n- b\n  - c\n  - d\n", 1, "- ab\n  \n  - c\n  - d\n"),
    ] {
        let mut d = Doc::new(load_markdown(source, editor_options()));
        let before = d.document.to_markdown();
        let block = d.text_leaves()[at_leaf];
        let _ = d.apply(Sel::collapsed(caret(block, 0)), Command::DeleteBackward);
        let after = d.document.to_markdown();
        assert_eq!(after, joined, "source={source:?}");
        assert!(d.undo().is_some(), "source={source:?}");
        assert_eq!(
            d.document.to_markdown(),
            before,
            "undo must restore: {source:?}"
        );
        assert!(d.redo().is_some(), "source={source:?}");
        assert_eq!(
            d.document.to_markdown(),
            after,
            "redo must replay the migrated children: {source:?}"
        );
        assert!(d.undo().is_some(), "source={source:?}");
        assert_eq!(d.document.to_markdown(), before, "second undo: {source:?}");
    }
}
