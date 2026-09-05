use super::support::kind_count;
use crate::block::BlockKind;
use crate::doc::Doc;
use crate::document::DocChange;
use crate::document::PasteIntent;
use crate::document::edit::{Caret, Command, Sel};
use crate::document::{FocusBias, editor_options, load_markdown};

fn doc(md: &str) -> Doc {
    Doc::new(load_markdown(md, editor_options()))
}

fn at(block: u32, offset: usize) -> Sel {
    Sel::collapsed(Caret { block, offset })
}

fn type_chars(doc: &mut Doc, mut sel: Sel, s: &str) -> Sel {
    for ch in s.chars() {
        let c = doc.apply(
            sel,
            Command::Insert {
                text: ch.to_string(),
            },
        );
        sel = Sel::collapsed(c);
    }
    sel
}

fn leaf_text(doc: &Doc) -> String {
    let id = doc.text_leaves()[0];
    doc.text(id).unwrap_or("").to_string()
}

fn content_rev(doc: &Doc, block: u32) -> u64 {
    let id = doc.document.live_id(block).expect("live");
    doc.document.arena.get(id).expect("node").content_revision
}

#[test]
fn consecutive_letters_undo_as_one_word() {
    let mut d = doc("");
    let leaf = d.text_leaves()[0];
    let _ = type_chars(&mut d, at(leaf, 0), "hello");
    assert_eq!(leaf_text(&d), "hello");
    assert!(d.can_undo());
    assert!(!d.can_redo());

    let restored = d.undo().expect("undo");
    assert_eq!(leaf_text(&d), "");
    assert_eq!(restored.head.offset, 0);
    assert!(!d.can_undo());
    assert!(d.can_redo());
}

#[test]
fn space_breaks_the_insert_group() {
    let mut d = doc("");
    let leaf = d.text_leaves()[0];
    let after_ab = type_chars(&mut d, at(leaf, 0), "ab");
    let after_sp = type_chars(&mut d, after_ab, " ");
    let _ = type_chars(&mut d, after_sp, "cd");
    assert_eq!(leaf_text(&d), "ab cd");

    let _ = d.undo().expect("undo word");
    assert_eq!(leaf_text(&d), "ab ");
    let _ = d.undo().expect("undo space");
    assert_eq!(leaf_text(&d), "ab");
    let sel = d.undo().expect("undo first word");
    assert_eq!(leaf_text(&d), "");
    assert_eq!(sel.head.offset, 0);
}

#[test]
fn redo_restores_the_undone_edit() {
    let mut d = doc("");
    let leaf = d.text_leaves()[0];
    let _ = type_chars(&mut d, at(leaf, 0), "hi");
    let _ = d.undo().expect("undo");
    assert_eq!(leaf_text(&d), "");
    let redone = d.redo().expect("redo");
    assert_eq!(leaf_text(&d), "hi");
    assert_eq!(redone.head.offset, 2);
}

#[test]
fn typing_after_undo_drops_redo() {
    let mut d = doc("");
    let leaf = d.text_leaves()[0];
    let _ = type_chars(&mut d, at(leaf, 0), "ab");
    let undone = d.undo().expect("undo");
    assert!(d.can_redo());
    let _ = d.apply(undone, Command::Insert { text: "x".into() });
    assert!(!d.can_redo());
    assert_eq!(leaf_text(&d), "x");
}

#[test]
fn noop_after_undo_keeps_redo() {
    let mut d = doc("");
    let leaf = d.text_leaves()[0];
    let _ = type_chars(&mut d, at(leaf, 0), "hi");
    let undone = d.undo().expect("undo");
    assert!(d.can_redo());
    assert!(!d.can_undo());
    let _ = d.apply(undone, Command::DeleteBackward);
    assert!(d.can_redo());
    assert!(!d.can_undo());
    let _ = d.apply(undone, Command::ToggleTask);
    assert!(d.can_redo());
    let redone = d.redo().expect("redo");
    assert_eq!(leaf_text(&d), "hi");
    assert_eq!(redone.head.offset, 2);
}

#[test]
fn noop_does_not_split_insert_group() {
    let mut d = doc("");
    let leaf = d.text_leaves()[0];
    let after_ab = type_chars(&mut d, at(leaf, 0), "ab");
    let _ = d.apply(after_ab, Command::ToggleTask);
    let _ = type_chars(&mut d, after_ab, "c");
    assert_eq!(leaf_text(&d), "abc");
    let restored = d.undo().expect("undo");
    assert_eq!(leaf_text(&d), "");
    assert_eq!(restored.head.offset, 0);
    assert!(!d.can_undo());
}

#[test]
fn consecutive_commands_accumulate_changes_without_merging_history_entries() {
    let mut d = doc("a");
    let leaf = d.text_leaves()[0];
    let inserted = d.apply(at(leaf, 1), Command::Insert { text: "x".into() });
    let _ = d.apply(Sel::collapsed(inserted), Command::Break);

    let changes = d.take_changes();
    assert!(
        changes
            .changes
            .iter()
            .any(|change| matches!(change, DocChange::TextChanged { .. }))
    );
    assert!(
        changes
            .changes
            .iter()
            .any(|change| matches!(change, DocChange::TreeSpliced { .. }))
    );

    let _ = d.undo().expect("undo split");
    assert_eq!(d.text_leaves().len(), 1);
    assert_eq!(leaf_text(&d), "ax");
    let _ = d.undo().expect("undo insert");
    assert_eq!(leaf_text(&d), "a");
}

#[test]
fn break_is_its_own_undo_step() {
    let mut d = doc("hello");
    let leaf = d.text_leaves()[0];
    let after_insert = type_chars(&mut d, at(leaf, 5), "x");
    let _ = d.apply(Sel::collapsed(after_insert.head), Command::Break);
    assert_eq!(d.text_leaves().len(), 2);

    let _ = d.undo().expect("undo break");
    assert_eq!(d.text_leaves().len(), 1);
    assert_eq!(leaf_text(&d), "hellox");
    let _ = d.undo().expect("undo insert");
    assert_eq!(leaf_text(&d), "hello");
}

#[test]
fn undo_during_marked_insert_keeps_the_previous_word() {
    let mut d = doc("");
    let leaf = d.text_leaves()[0];
    let sel = type_chars(&mut d, at(leaf, 0), "abc");
    let _ = d.apply_marked(sel, Command::Insert { text: "ddd".into() });
    assert_eq!(leaf_text(&d), "abcddd");
    assert!(d.is_composing());

    let restored = d.undo().expect("abort compose");
    assert_eq!(leaf_text(&d), "abc");
    assert_eq!(restored.head.offset, 3);
    assert!(!d.is_composing());

    let mut sel = restored;
    for expect in ["ab", "a", ""] {
        let c = d.apply(sel, Command::DeleteBackward);
        sel = Sel::collapsed(c);
        assert_eq!(leaf_text(&d), expect);
    }
}

#[test]
fn ime_commit_undoes_as_one_step() {
    let mut d = doc("");
    let leaf = d.text_leaves()[0];
    let after_abc = type_chars(&mut d, at(leaf, 0), "abc");
    let marked = d.apply_marked(after_abc, Command::Insert { text: "ddd".into() });
    let sel = Sel {
        anchor: Caret {
            block: marked.block,
            offset: after_abc.head.offset,
        },
        head: marked,
    };
    let _ = d.apply_ime_commit(sel, Command::Insert { text: "ddd".into() });
    assert_eq!(leaf_text(&d), "abcddd");
    assert!(!d.is_composing());

    let restored = d.undo().expect("undo commit");
    assert_eq!(leaf_text(&d), "abc");
    assert_eq!(restored.head.offset, 3);
}

#[test]
fn edits_after_structural_undo_are_text_changed() {
    let mut d = doc("a");
    let leaf = d.text_leaves()[0];
    let _ = d.apply(at(leaf, 1), Command::Break);
    let _ = d.take_changes();
    let undone = d.undo().expect("undo");
    let _ = d.apply(undone, Command::Insert { text: "x".into() });
    let changes = d.take_changes();
    assert!(!changes.is_replace());
    assert_eq!(leaf_text(&d), "ax");
}

#[test]
fn restore_advances_content_revision_past_the_undone_timeline() {
    let mut d = doc("");
    let leaf = d.text_leaves()[0];
    let sel = type_chars(&mut d, at(leaf, 0), "abc");
    let _ = d.apply_marked(sel, Command::Insert { text: "ddd".into() });
    let during = content_rev(&d, leaf);
    let restored = d.undo().expect("abort compose");
    let after_undo = content_rev(&d, restored.head.block);
    assert!(after_undo > during);
    let _ = d.apply(restored, Command::DeleteBackward);
    assert_eq!(leaf_text(&d), "ab");
    assert!(content_rev(&d, restored.head.block) > after_undo);
}

#[test]
fn delete_backward_coalesces_then_undoes_the_run() {
    let mut d = doc("abcd");
    let leaf = d.text_leaves()[0];
    let mut sel = at(leaf, 4);
    for _ in 0..3 {
        let c = d.apply(sel, Command::DeleteBackward);
        sel = Sel::collapsed(c);
    }
    assert_eq!(leaf_text(&d), "a");
    let _ = d.undo().expect("undo deletes");
    assert_eq!(leaf_text(&d), "abcd");
}

#[test]
fn word_undo_emits_text_changed() {
    let mut d = doc("keep\n\nfoo");
    let leaves = d.text_leaves();
    assert_eq!(leaves.len(), 2);
    let second = leaves[1];
    let _ = type_chars(&mut d, at(second, 0), "xyz");
    let _ = d.take_changes();
    let _ = d.undo().expect("undo");
    assert_eq!(d.text(leaves[0]).unwrap_or(""), "keep");
    assert_eq!(d.text(second).unwrap_or(""), "foo");
    let changes = d.take_changes();
    assert!(!changes.is_replace());
    assert!(
        changes
            .changes
            .iter()
            .any(|c| matches!(c, DocChange::TextChanged { .. }))
    );
}

#[test]
fn undo_emits_text_changed_for_an_unrelated_revealed_leaf() {
    let mut d = doc("alpha **bold** beta\n\nfoo");
    let leaves = d.text_leaves();
    let first = leaves[0];
    let second = leaves[1];

    let _ = d.apply(at(second, 3), Command::Insert { text: "x".into() });
    let _ = d.take_changes();

    let _ = d.retarget_focus(Caret {
        block: first,
        offset: 8,
    });
    let reveal = d.take_changes();
    assert!(reveal.changes.iter().any(
        |change| matches!(change, DocChange::TextChanged { node, .. } if node.index == first)
    ));

    let restored = d.undo().expect("undo the second paragraph edit");
    let _ = d.retarget_focus(restored.head);
    let changes = d.take_changes();
    assert!(changes.changes.iter().any(
        |change| matches!(change, DocChange::TextChanged { node, .. } if node.index == first)
    ));
}

#[test]
fn space_undo_emits_text_changed() {
    let mut d = doc("keep\n\nfoo");
    let leaves = d.text_leaves();
    assert_eq!(leaves.len(), 2);
    let second = leaves[1];
    let _ = type_chars(&mut d, at(second, 3), " ");
    let _ = d.take_changes();
    let _ = d.undo().expect("undo");
    assert_eq!(d.text(second).unwrap_or(""), "foo");
    assert!(!d.take_changes().is_replace());
}

#[test]
fn wrap_list_undo_splices() {
    let mut d = doc("");
    let leaf = d.text_leaves()[0];
    let _ = type_chars(&mut d, at(leaf, 0), "- ");
    assert_eq!(kind_count(&d.document, BlockKind::List), 1);
    let _ = d.take_changes();
    let _ = d.undo().expect("undo");
    assert_eq!(leaf_text(&d), "-");
    assert_eq!(kind_count(&d.document, BlockKind::List), 0);
    let changes = d.take_changes();
    assert!(!changes.is_replace());
    assert!(changes.is_structural());
}

#[test]
fn break_undo_emits_tree_spliced() {
    let mut d = doc("a");
    let leaf = d.text_leaves()[0];
    let _ = d.apply(at(leaf, 1), Command::Break);
    let _ = d.take_changes();
    let _ = d.undo().expect("undo");
    let changes = d.take_changes();
    assert!(!changes.is_replace());
    assert!(changes.is_structural());
}

#[test]
fn undo_keeps_structure_commands_intact() {
    let mut d = doc("para");
    let leaf = d.text_leaves()[0];
    let _ = d.apply(at(leaf, 4), Command::Break);
    assert_eq!(kind_count(&d.document, BlockKind::Paragraph), 2);
    let _ = d.undo().expect("undo");
    assert_eq!(kind_count(&d.document, BlockKind::Paragraph), 1);
    assert_eq!(leaf_text(&d), "para");
}

#[test]
fn break_undo_redo_keeps_leaf_identity() {
    let mut d = doc("hello");
    let leaf = d.text_leaves()[0];
    let id = d.document.live_id(leaf).expect("live");
    let after = d.apply(at(leaf, 2), Command::Break);
    let new_leaf = after.block;
    let new_id = d.document.live_id(new_leaf).expect("new");
    let _ = d.undo().expect("undo");
    assert!(d.document.live_id(new_leaf).is_none());
    assert_eq!(
        d.document.live_id(leaf).expect("old").generation,
        id.generation
    );
    let redone = d.redo().expect("redo");
    let again = d.document.live_id(new_leaf).expect("resurrected");
    assert_eq!(again, new_id);
    assert_eq!(redone.head.block, new_leaf);
}

#[test]
fn wrap_list_undo_redo_keeps_paragraph_id() {
    let mut d = doc("para");
    let leaf = d.text_leaves()[0];
    let id = d.document.live_id(leaf).expect("live");
    let _ = d.apply(
        at(leaf, 0),
        Command::WrapList {
            ordered: false,
            task: None,
        },
    );
    assert_eq!(kind_count(&d.document, BlockKind::List), 1);
    let _ = d.undo().expect("undo");
    assert_eq!(kind_count(&d.document, BlockKind::List), 0);
    assert_eq!(d.document.live_id(leaf).expect("moved"), id);
    let _ = d.redo().expect("redo");
    assert_eq!(kind_count(&d.document, BlockKind::List), 1);
    assert_eq!(d.document.live_id(leaf).expect("still"), id);
}

#[test]
fn list_item_break_undo_redo_keeps_ids() {
    let mut d = doc("- abc");
    let leaf = d.text_leaves()[0];
    let old = d.document.live_id(leaf).expect("leaf");
    let after = d.apply(at(leaf, 2), Command::Break);
    let new_leaf = after.block;
    let new_id = d.document.live_id(new_leaf).expect("new");
    assert_eq!(d.text_leaves().len(), 2);
    let _ = d.undo().expect("undo");
    assert_eq!(d.text_leaves().len(), 1);
    assert_eq!(d.text(leaf).unwrap_or(""), "abc");
    assert_eq!(d.document.live_id(leaf).expect("old"), old);
    assert!(d.document.live_id(new_leaf).is_none());
    let _ = d.redo().expect("redo");
    assert_eq!(d.text_leaves().len(), 2);
    assert_eq!(d.document.live_id(new_leaf).expect("resurrected"), new_id);
    assert_eq!(d.text(leaf).unwrap_or(""), "ab");
    assert_eq!(d.text(new_leaf).unwrap_or(""), "c");
}

#[test]
fn join_items_undo_redo_keeps_ids() {
    let mut d = doc("- a\n- b");
    let leaves = d.text_leaves();
    let first = leaves[0];
    let second = leaves[1];
    let first_id = d.document.live_id(first).expect("first");
    let second_id = d.document.live_id(second).expect("second");
    let _ = d.apply(at(second, 0), Command::DeleteBackward);
    assert_eq!(d.text_leaves().len(), 1);
    assert_eq!(leaf_text(&d), "ab");
    let _ = d.undo().expect("undo");
    assert_eq!(d.text_leaves().len(), 2);
    assert_eq!(d.document.live_id(first).expect("first"), first_id);
    assert_eq!(d.document.live_id(second).expect("second"), second_id);
    assert_eq!(d.text(first).unwrap_or(""), "a");
    assert_eq!(d.text(second).unwrap_or(""), "b");
    let _ = d.redo().expect("redo");
    assert_eq!(d.text_leaves().len(), 1);
    assert_eq!(d.document.live_id(first).expect("joined"), first_id);
    assert!(d.document.live_id(second).is_none());
    assert_eq!(leaf_text(&d), "ab");
}

#[test]
fn forward_merge_undo_redo_keeps_block_ids() {
    let mut d = doc("a\n\nb");
    let leaves = d.text_leaves();
    let first = leaves[0];
    let second = leaves[1];
    let first_id = d.document.live_id(first).expect("first");
    let second_id = d.document.live_id(second).expect("second");

    let after = d.apply(at(first, 1), Command::DeleteForward);
    assert_eq!(
        after,
        Caret {
            block: first,
            offset: 1
        }
    );
    assert_eq!(d.text(first), Some("ab"));
    assert!(d.document.live_id(second).is_none());

    let _ = d.undo().expect("undo forward merge");
    assert_eq!(d.text(first), Some("a"));
    assert_eq!(d.text(second), Some("b"));
    assert_eq!(d.document.live_id(first).expect("first"), first_id);
    assert_eq!(d.document.live_id(second).expect("second"), second_id);

    let _ = d.redo().expect("redo forward merge");
    assert_eq!(d.text(first), Some("ab"));
    assert!(d.document.live_id(second).is_none());
}

#[test]
fn literal_merge_undo_redo_replays_display_coordinates() {
    let mut d = doc("```\ncode\n```\n\n**bold**\n");
    let code = d
        .text_leaves()
        .into_iter()
        .find(|&block| d.document.kind(block) == Some(BlockKind::CodeBlock))
        .expect("code block");
    let paragraph = d
        .text_leaves()
        .into_iter()
        .find(|&block| d.document.kind(block) == Some(BlockKind::Paragraph))
        .expect("paragraph");

    let after = d.apply(at(paragraph, 0), Command::DeleteBackward);
    assert_eq!(
        after,
        Caret {
            block: code,
            offset: 4
        }
    );
    assert_eq!(
        d.document.leaf_source(d.document.live_id(code).unwrap()),
        "code**bold**"
    );

    let _ = d.undo().expect("undo literal merge");
    assert_eq!(d.text(code), Some("code"));
    assert_eq!(d.text(paragraph), Some("bold"));
    assert_eq!(d.document.to_markdown(), "```\ncode\n```\n\n**bold**\n");

    let redone = d.redo().expect("redo literal merge");
    assert_eq!(redone, Sel::collapsed(after));
    assert_eq!(d.text(code), Some("code**bold**"));
    assert_eq!(d.document.to_markdown(), "```\ncode**bold**\n```\n");
}

#[test]
fn lifting_only_quote_child_undoes_without_losing_text() {
    let mut d = doc("> hi");
    let leaf = d.text_leaves()[0];
    let _ = d.apply(at(leaf, 0), Command::DeleteBackward);
    assert_eq!(d.text(leaf), Some("hi"));
    assert_eq!(kind_count(&d.document, BlockKind::BlockQuote), 0);

    let _ = d.undo().expect("undo quote lift");
    assert_eq!(d.text(leaf), Some("hi"));
    assert_eq!(kind_count(&d.document, BlockKind::BlockQuote), 1);
    assert!(d.document.preorder().into_iter().any(|id| id.index == leaf));

    let _ = d.redo().expect("redo quote lift");
    assert_eq!(d.text(leaf), Some("hi"));
    assert_eq!(kind_count(&d.document, BlockKind::BlockQuote), 0);
}

#[test]
fn indent_undo_redo_keeps_item_identity() {
    let mut d = doc("- a\n- b");
    let second = d.text_leaves()[1];
    let item = d
        .document
        .live_id(second)
        .and_then(|id| d.document.arena.get(id).and_then(|n| n.parent))
        .expect("item");
    let _ = d.apply(at(second, 0), Command::Indent);
    assert_eq!(kind_count(&d.document, BlockKind::List), 2);
    let nested = d
        .document
        .preorder()
        .into_iter()
        .filter(|&id| d.document.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .nth(1)
        .expect("nested");
    let _ = d.undo().expect("undo");
    assert_eq!(kind_count(&d.document, BlockKind::List), 1);
    assert!(d.document.arena.get(nested).is_none());
    assert_eq!(d.document.live_id(second).expect("leaf").index, second);
    assert_eq!(
        d.document
            .live_id(second)
            .and_then(|id| d.document.arena.get(id).and_then(|n| n.parent)),
        Some(item)
    );
    let _ = d.redo().expect("redo");
    assert_eq!(kind_count(&d.document, BlockKind::List), 2);
    assert!(d.document.arena.get(nested).is_some());
    assert_eq!(
        d.document
            .live_id(second)
            .and_then(|id| d.document.arena.get(id).and_then(|n| n.parent)),
        Some(item)
    );
}

#[test]
fn structural_history_stores_focused_carets_in_collapsed_coordinates() {
    let mut d = doc("- first\n- **bold** tail\n");
    let second = d.text_leaves()[1];
    let focused = d.document.retarget_inline_focus_biased(
        Caret {
            block: second,
            offset: 4,
        },
        FocusBias::Neutral,
    );
    assert!(focused.offset > 4, "focused={focused:?}");

    let _ = d.apply(Sel::collapsed(focused), Command::Indent);
    let undone = d.undo().expect("undo structural edit");

    assert_eq!(undone, at(second, 4));
    let restored = d
        .document
        .retarget_inline_focus_biased(undone.head, FocusBias::Neutral);
    assert!(restored.offset > undone.head.offset);

    let redone = d.redo().expect("redo structural edit");
    assert_eq!(redone, at(second, 4));
}

#[test]
fn independent_paste_undo_redo_keeps_pasted_item() {
    let mut d = doc("- a");
    let leaf = d.text_leaves()[0];
    let off = d.text(leaf).unwrap_or("").len();
    let _ = d.apply(
        at(leaf, off),
        Command::Paste {
            text: "- c\n".into(),
            intent: PasteIntent::IndependentFragment,
        },
    );
    let leaves = d.text_leaves();
    assert_eq!(leaves.len(), 2);
    let pasted = leaves[1];
    let pasted_id = d.document.live_id(pasted).expect("pasted");
    let _ = d.undo().expect("undo");
    assert_eq!(d.text_leaves().len(), 1);
    assert!(d.document.live_id(pasted).is_none());
    let _ = d.redo().expect("redo");
    assert_eq!(d.text_leaves().len(), 2);
    assert_eq!(d.document.live_id(pasted).expect("again"), pasted_id);
    assert_eq!(d.text(pasted).unwrap_or(""), "c");
}

#[test]
fn toggle_task_undo_redo_restores_extra() {
    let mut d = doc("- [ ] a");
    let leaf = d.text_leaves()[0];
    let item = d
        .document
        .live_id(leaf)
        .and_then(|id| d.document.arena.get(id).and_then(|n| n.parent))
        .expect("item");
    assert_eq!(d.document.extra(item).task_checked(), Some(false));
    let _ = d.apply(at(leaf, 1), Command::ToggleTask);
    assert_eq!(d.document.extra(item).task_checked(), Some(true));
    let _ = d.undo().expect("undo");
    assert_eq!(d.document.extra(item).task_checked(), Some(false));
    let _ = d.redo().expect("redo");
    assert_eq!(d.document.extra(item).task_checked(), Some(true));
}

#[test]
fn undo_cannot_reach_past_the_depth_cap() {
    let mut d = doc("hello");
    let leaf = d.text_leaves()[0];
    for _ in 0..200 {
        let _ = d.apply(at(leaf, 0), Command::Insert { text: "x".into() });
    }
    assert_eq!(leaf_text(&d), format!("{}hello", "x".repeat(200)));

    let mut undos = 0;
    while d.undo().is_some() {
        undos += 1;
    }
    assert_eq!(undos, 128);
    assert_eq!(leaf_text(&d), format!("{}hello", "x".repeat(72)));
    assert!(!d.can_undo());

    let mut redos = 0;
    while d.redo().is_some() {
        redos += 1;
    }
    assert_eq!(redos, 128);
    assert_eq!(leaf_text(&d), format!("{}hello", "x".repeat(200)));
    let mut again = 0;
    while d.undo().is_some() {
        again += 1;
    }
    assert_eq!(again, 128);
    assert_eq!(leaf_text(&d), format!("{}hello", "x".repeat(72)));
}
