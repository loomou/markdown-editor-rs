use super::support::{caret, fresh, type_chars};
use crate::block::BlockKind;
use crate::document::edit::{Caret, Command, Sel, apply};
use crate::document::{FocusBias, editor_options, load_markdown};

fn kind_of(doc: &crate::document::Document, id: crate::document::NodeId) -> Option<BlockKind> {
    doc.arena.get(id).map(|n| n.kind)
}

fn parent_kind(doc: &crate::document::Document, id: crate::document::NodeId) -> Option<BlockKind> {
    doc.arena
        .get(id)
        .and_then(|n| n.parent)
        .and_then(|p| kind_of(doc, p))
}

fn ancestors(doc: &crate::document::Document, id: crate::document::NodeId) -> Vec<BlockKind> {
    let mut out = Vec::new();
    let mut cur = doc.arena.get(id).and_then(|n| n.parent);
    while let Some(p) = cur {
        if let Some(k) = kind_of(doc, p) {
            out.push(k);
        }
        cur = doc.arena.get(p).and_then(|n| n.parent);
    }
    out
}

fn count_kind(doc: &crate::document::Document, kind: BlockKind) -> usize {
    doc.preorder()
        .into_iter()
        .filter(|&id| kind_of(doc, id) == Some(kind))
        .count()
}

#[test]
fn typed_quote_then_gt_space_nests() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "> ");
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(parent_kind(&doc, id), Some(BlockKind::BlockQuote));
    let _ = type_chars(&mut doc, at, "> ");
    let id = doc.live_id(leaf).expect("live");
    let up = ancestors(&doc, id);
    assert_eq!(up[0], BlockKind::BlockQuote);
    assert_eq!(up[1], BlockKind::BlockQuote);
    assert_eq!(count_kind(&doc, BlockKind::BlockQuote), 2);
    assert_eq!(doc.text_of(leaf).unwrap(), "");
}

#[test]
fn typed_list_then_gt_space_wraps_quote_in_item() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "- ");
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(parent_kind(&doc, id), Some(BlockKind::ListItem));
    let _ = type_chars(&mut doc, at, "> ");
    let id = doc.live_id(leaf).expect("live");
    let up = ancestors(&doc, id);
    assert_eq!(up[0], BlockKind::BlockQuote);
    assert_eq!(up[1], BlockKind::ListItem);
    assert_eq!(up[2], BlockKind::List);
    assert_eq!(doc.text_of(leaf).unwrap(), "");
}

#[test]
fn typed_quote_then_list_then_gt_space_nests_quote_in_item() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "> ");
    let at = type_chars(&mut doc, at, "- ");
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(parent_kind(&doc, id), Some(BlockKind::ListItem));
    assert!(ancestors(&doc, id).contains(&BlockKind::BlockQuote));
    let _ = type_chars(&mut doc, at, "> ");
    let id = doc.live_id(leaf).expect("live");
    let up = ancestors(&doc, id);
    assert_eq!(up[0], BlockKind::BlockQuote);
    assert_eq!(up[1], BlockKind::ListItem);
    assert_eq!(up[2], BlockKind::List);
    assert_eq!(up[3], BlockKind::BlockQuote);
    assert_eq!(count_kind(&doc, BlockKind::BlockQuote), 2);
    assert_eq!(doc.text_of(leaf).unwrap(), "");
}

#[test]
fn nested_quote_backspace_lifts_one_level() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "> ");
    let at = type_chars(&mut doc, at, "> ");
    let _ = type_chars(&mut doc, at, "hi");
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(ancestors(&doc, id)[0], BlockKind::BlockQuote);
    assert_eq!(ancestors(&doc, id)[1], BlockKind::BlockQuote);
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::DeleteBackward,
    );
    assert_eq!(out, caret(leaf, 0));
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(parent_kind(&doc, id), Some(BlockKind::BlockQuote));
    assert_eq!(count_kind(&doc, BlockKind::BlockQuote), 1);
    assert_eq!(doc.text_of(leaf).unwrap(), "hi");
}

#[test]
fn nested_quote_empty_enter_lifts_one_level() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "> ");
    let at = type_chars(&mut doc, at, "> ");
    let out = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_eq!(out.block, leaf);
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(parent_kind(&doc, id), Some(BlockKind::BlockQuote));
    assert_eq!(count_kind(&doc, BlockKind::BlockQuote), 1);
}

#[test]
fn quote_in_list_empty_enter_stays_in_item() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "- ");
    let at = type_chars(&mut doc, at, "> ");
    let out = apply(&mut doc, Sel::collapsed(at), Command::Break);
    let id = doc.live_id(out.block).expect("live");
    assert_eq!(parent_kind(&doc, id), Some(BlockKind::ListItem));
    assert_eq!(count_kind(&doc, BlockKind::BlockQuote), 0);
}

#[test]
fn quote_in_list_backspace_unwraps_to_item() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "- ");
    let _ = type_chars(&mut doc, at, "> ");
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::DeleteBackward,
    );
    let id = doc.live_id(out.block).expect("live");
    assert_eq!(parent_kind(&doc, id), Some(BlockKind::ListItem));
    assert_eq!(count_kind(&doc, BlockKind::BlockQuote), 0);
}

#[test]
fn plus_and_star_markers_wrap_lists() {
    for marker in ["+ ", "* "] {
        let (mut doc, leaf) = fresh();
        let _ = type_chars(&mut doc, caret(leaf, 0), marker);
        let id = doc.live_id(leaf).expect("live");
        assert_eq!(parent_kind(&doc, id), Some(BlockKind::ListItem), "{marker}");
        assert_eq!(doc.text_of(leaf).unwrap(), "", "{marker}");
    }
}

#[test]
fn ordered_marker_inside_quote_wraps() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "> ");
    let _ = type_chars(&mut doc, at, "1. ");
    let id = doc.live_id(leaf).expect("live");
    let up = ancestors(&doc, id);
    assert_eq!(up[0], BlockKind::ListItem);
    assert_eq!(up[1], BlockKind::List);
    assert_eq!(up[2], BlockKind::BlockQuote);
}

#[test]
fn task_marker_inside_quote_wraps() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "> ");
    let _ = type_chars(&mut doc, at, "- [ ] ");
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(parent_kind(&doc, id), Some(BlockKind::ListItem));
    let item = doc.arena.get(id).and_then(|n| n.parent).expect("item");
    assert_eq!(doc.extra(item).task_checked(), Some(false));
    assert_eq!(doc.text_of(leaf).unwrap(), "");
}

#[test]
fn typed_task_checkbox_after_dash_space() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "- ");
    let _ = type_chars(&mut doc, at, "[ ] ");
    let id = doc.live_id(leaf).expect("live");
    let item = doc.arena.get(id).and_then(|n| n.parent).expect("item");
    assert_eq!(doc.extra(item).task_checked(), Some(false));
    assert_eq!(doc.text_of(leaf).unwrap(), "");
}

#[test]
fn list_enter_then_gt_space_quotes_new_item() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "- ab");
    let empty = apply(&mut doc, Sel::collapsed(at), Command::Break);
    let _ = type_chars(&mut doc, empty, "> ");
    let id = doc.live_id(empty.block).expect("live");
    let up = ancestors(&doc, id);
    assert_eq!(up[0], BlockKind::BlockQuote);
    assert_eq!(up[1], BlockKind::ListItem);
}

#[test]
fn quote_enter_then_gt_space_nests() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "> hi");
    let empty = apply(&mut doc, Sel::collapsed(at), Command::Break);
    let _ = type_chars(&mut doc, empty, "> ");
    let id = doc.live_id(empty.block).expect("live");
    assert_eq!(parent_kind(&doc, id), Some(BlockKind::BlockQuote));
    let outer = doc
        .arena
        .get(id)
        .and_then(|n| n.parent)
        .and_then(|p| doc.arena.get(p).and_then(|n| n.parent));
    assert_eq!(
        outer.and_then(|p| kind_of(&doc, p)),
        Some(BlockKind::BlockQuote)
    );
    assert_eq!(count_kind(&doc, BlockKind::BlockQuote), 2);
}

#[test]
fn soft_break_then_gt_space_wraps_quote() {
    let (mut doc, leaf) = fresh();
    let at = type_chars(&mut doc, caret(leaf, 0), "hello");
    let mid = apply(&mut doc, Sel::collapsed(at), Command::SoftBreak);
    let out = type_chars(&mut doc, mid, "> ");
    assert_eq!(doc.text_of(leaf).unwrap(), "hello");
    assert_ne!(out.block, leaf);
    let id = doc.live_id(out.block).expect("live");
    assert_eq!(parent_kind(&doc, id), Some(BlockKind::BlockQuote));
    assert_eq!(doc.text_of(out.block).unwrap(), "");
}

#[test]
fn quote_marker_in_middle_preserves_following_soft_line() {
    let mut doc = load_markdown("one\ntwo\nthree", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = doc.retarget_inline_focus_biased(caret(leaf, "one\n".len()), FocusBias::Right);
    let out = type_chars(&mut doc, at, "> ");

    assert_eq!(doc.text_of(leaf), Some("one"));
    assert_eq!(doc.text_leaves().len(), 3);
    assert_eq!(doc.text_of(out.block), Some("two "));
    let tail = doc
        .text_leaves()
        .into_iter()
        .find(|&block| block != leaf && block != out.block)
        .expect("preserved trailing line");
    assert_eq!(doc.text_of(tail), Some("three"));
}

#[test]
fn quote_marker_on_a_soft_line_preserves_inline_source() {
    let mut doc = load_markdown("hello\n**hi**\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = doc.retarget_inline_focus_biased(caret(leaf, "hello\n".len()), FocusBias::Right);
    let out = apply(
        &mut doc,
        Sel::collapsed(at),
        Command::Insert { text: "> ".into() },
    );

    assert_eq!(doc.text_of(leaf), Some("hello"));
    let id = doc.live_id(out.block).expect("quoted paragraph");
    assert_eq!(doc.leaf_source(id), "**hi**");
    assert_eq!(parent_kind(&doc, id), Some(BlockKind::BlockQuote));
    assert_eq!(doc.to_markdown(), "hello\n\n> **hi**\n");
}

#[test]
fn tab_still_nests_list_not_quote() {
    let mut doc = load_markdown("- a\n- b\n", editor_options());
    let leaves = doc.text_leaves();
    let b = leaves[1];
    let _ = apply(&mut doc, Sel::collapsed(caret(b, 0)), Command::Indent);
    let id = doc.live_id(b).expect("live");
    let up = ancestors(&doc, id);
    assert_eq!(up[0], BlockKind::ListItem);
    assert_eq!(up[1], BlockKind::List);
    assert_eq!(up[2], BlockKind::ListItem);
    assert_eq!(count_kind(&doc, BlockKind::BlockQuote), 0);
}

#[test]
fn join_sibling_without_list_ancestor_is_a_noop() {
    let mut doc = load_markdown("- a\n\n  b\n", editor_options());
    let leaves = doc.text_leaves();
    assert_eq!(leaves.len(), 2, "one loose item, two paragraphs");
    let (a, b) = (leaves[0], leaves[1]);
    let rev = doc.revision();

    let item = doc
        .arena
        .get(doc.live_id(b).expect("live"))
        .and_then(|n| n.parent)
        .expect("item parent");
    assert_eq!(
        doc.arena.get(item).map(|n| n.kind),
        Some(BlockKind::ListItem)
    );
    doc.arena.detach(item);
    doc.arena.append_child(doc.root, item);

    let out = apply(
        &mut doc,
        Sel::collapsed(caret(b, 0)),
        Command::DeleteBackward,
    );
    assert_eq!(out, caret(b, 0), "caret stays where it was");
    assert_eq!(doc.revision(), rev, "nothing may be committed");
    assert_eq!(doc.text_of(a).unwrap(), "a", "texts must not merge");
    assert_eq!(doc.text_of(b).unwrap(), "b");
    assert!(doc.live_id(a).is_some(), "no tombstone");
    assert!(doc.live_id(b).is_some(), "no tombstone");
}

#[test]
fn indented_paragraph_reveals_the_complete_inline_construct() {
    use crate::doc::Doc;
    let mut doc = Doc::new(load_markdown("**bold** tail\n", editor_options()));
    let leaf = doc.text_leaves()[0];
    let _ = doc.apply(
        Sel {
            anchor: caret(leaf, 0),
            head: caret(leaf, 4),
        },
        Command::Indent,
    );
    doc.retarget_focus(Caret {
        block: leaf,
        offset: 3,
    });
    assert_eq!(
        doc.text(leaf),
        Some("\t**bold** tail"),
        "revealed text must carry the full delimiters"
    );
}
