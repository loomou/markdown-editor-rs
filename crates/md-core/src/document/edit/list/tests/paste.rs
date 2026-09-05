use super::support::{caret, first_list, items, lead};
use crate::block::BlockKind;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{PasteIntent, editor_options, load_markdown};

#[test]
fn independent_list_joins_host_list() {
    let mut doc = load_markdown("- a\n- b\n", editor_options());
    let list = first_list(&doc);
    let host = items(&doc, list);
    let a = host[0];
    let b = host[1];
    let lists = doc
        .preorder()
        .into_iter()
        .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .count();
    let leaf = lead(&doc, a);
    let off = doc.display(leaf).len();
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, off)),
        Command::Paste {
            text: "- c\n".into(),
            intent: PasteIntent::IndependentFragment,
        },
    );
    let after = items(&doc, list);
    assert_eq!(after.len(), 3);
    assert_eq!(after[0], a);
    assert_eq!(after[2], b);
    assert_eq!(doc.display(lead(&doc, after[1])), "c");
    assert_eq!(doc.text_of(out.block).unwrap(), "c");
    assert_eq!(
        doc.preorder()
            .into_iter()
            .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
            .count(),
        lists
    );
    assert_eq!(doc.arena.children(a).count(), 1);
}

#[test]
fn independent_list_paste_in_paragraph_stays_new_list() {
    let mut doc = load_markdown("- a\n- b\n\nhello\n", editor_options());
    let host = first_list(&doc);
    let host_items = items(&doc, host).len();
    let lists = doc
        .preorder()
        .into_iter()
        .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .count();
    let para = doc
        .text_leaves()
        .into_iter()
        .find(|&id| doc.text_of(id) == Some("hello"))
        .expect("para");
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(para, 0)),
        Command::Paste {
            text: "- c\n".into(),
            intent: PasteIntent::IndependentFragment,
        },
    );
    assert_eq!(items(&doc, host).len(), host_items);
    assert_eq!(
        doc.preorder()
            .into_iter()
            .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
            .count(),
        lists + 1
    );
    assert_eq!(doc.text_of(para).unwrap(), "hello");
}

#[test]
fn ordered_fragment_does_not_join_unordered_host() {
    let mut doc = load_markdown("- a\n- b\n", editor_options());
    let list = first_list(&doc);
    let host_n = items(&doc, list).len();
    let lists = doc
        .preorder()
        .into_iter()
        .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .count();
    let leaf = lead(&doc, items(&doc, list)[0]);
    let off = doc.display(leaf).len();
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, off)),
        Command::Paste {
            text: "1. c\n".into(),
            intent: PasteIntent::IndependentFragment,
        },
    );
    assert_eq!(items(&doc, list).len(), host_n);
    assert_eq!(
        doc.preorder()
            .into_iter()
            .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
            .count(),
        lists + 1
    );
    let nested = doc
        .arena
        .children(items(&doc, list)[0])
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .expect("nested");
    assert!(doc.extra(nested).ordered_start().is_some());
}

#[test]
fn pasted_task_item_keeps_extra() {
    let mut doc = load_markdown("- a\n- b\n", editor_options());
    let list = first_list(&doc);
    let a = items(&doc, list)[0];
    let leaf = lead(&doc, a);
    let off = doc.display(leaf).len();
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, off)),
        Command::Paste {
            text: "- [ ] c\n".into(),
            intent: PasteIntent::IndependentFragment,
        },
    );
    let after = items(&doc, list);
    assert_eq!(after.len(), 3);
    assert_eq!(after[0], a);
    assert_eq!(doc.extra(after[1]).task_checked(), Some(false));
    assert_eq!(doc.display(lead(&doc, after[1])), "c");
}

#[test]
fn pasted_list_items_keep_identity() {
    let mut doc = load_markdown("- a\n- b\n", editor_options());
    let list = first_list(&doc);
    let a = items(&doc, list)[0];
    let leaf = lead(&doc, a);
    let off = doc.display(leaf).len();
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, off)),
        Command::Paste {
            text: "- c\n- d\n".into(),
            intent: PasteIntent::IndependentFragment,
        },
    );
    let after = items(&doc, list);
    assert_eq!(after.len(), 4);
    assert_eq!(after[0], a);
    assert_eq!(doc.display(lead(&doc, after[1])), "c");
    assert_eq!(doc.display(lead(&doc, after[2])), "d");
    assert_eq!(
        doc.preorder()
            .into_iter()
            .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
            .count(),
        1
    );
}
