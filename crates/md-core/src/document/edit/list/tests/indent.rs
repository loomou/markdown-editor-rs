use super::support::{assert_changeset_parents_live, caret, first_list, items, lead};
use crate::block::BlockKind;
use crate::doc::Doc;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{PasteIntent, editor_options, load_markdown};

#[test]
fn outdent_mid_list_item_with_a_pasted_tail_keeps_the_rest() {
    let mut doc = Doc::new(load_markdown(
        "- first item\n- second item\n",
        editor_options(),
    ));
    let leaf = doc.text_leaves()[0];
    let _ = doc.take_changes();
    let pasted_caret = doc.apply(
        Sel::collapsed(caret(leaf, 10)),
        Command::Paste {
            text: "> quoted\n\n```rust\nfn q() {}\n```".into(),
            intent: PasteIntent::PlainText,
        },
    );
    let pasted = doc.document.to_markdown();
    assert_eq!(
        pasted,
        "- first item> quoted\n  \n  \\`\\`\\`rust\n  fn q() {}\n  \\`\\`\\`\n- second item\n"
    );

    let _ = doc.apply(Sel::collapsed(pasted_caret), Command::Outdent);
    let lifted = doc.document.to_markdown();
    assert!(
        lifted.contains("second item"),
        "the rest of the list must stay: {lifted:?}"
    );

    while doc.undo().is_some() {}
    assert_eq!(
        doc.document.to_markdown(),
        "- first item\n- second item\n",
        "undo-to-bottom lost the source: {lifted:?}"
    );
}

#[test]
fn indent_sinks_second_item_keeping_id() {
    let mut doc = load_markdown("- a\n- b\n", editor_options());
    let list = first_list(&doc);
    let first = items(&doc, list)[0];
    let second = items(&doc, list)[1];
    let leaf = lead(&doc, second);
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 0)),
        Command::Indent,
    );
    assert_eq!(items(&doc, list), vec![first]);
    let nested = doc
        .arena
        .children(first)
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .expect("nested");
    assert_eq!(items(&doc, nested), vec![second]);
    assert_eq!(doc.display(lead(&doc, second)), "b");
    assert_eq!(out.block, leaf.index);
    assert_eq!(out.offset, 0);
    assert!(doc.extra(nested).ordered_start().is_none());
    assert!(doc.extra(list).list_loose());
    assert!(!doc.extra(nested).list_loose());
}

#[test]
fn first_item_indent_is_noop() {
    let mut doc = load_markdown("- a\n- b\n", editor_options());
    let list = first_list(&doc);
    let before = items(&doc, list);
    let leaf = lead(&doc, before[0]);
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 1)),
        Command::Indent,
    );
    assert_eq!(items(&doc, list), before);
    assert_eq!(out, caret(leaf.index, 1));
    assert_eq!(
        doc.preorder()
            .into_iter()
            .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
            .count(),
        1
    );
}

#[test]
fn indent_joins_existing_child_list() {
    let mut doc = load_markdown("- a\n  - x\n- b\n", editor_options());
    let lists: Vec<_> = doc
        .preorder()
        .into_iter()
        .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .collect();
    let outer = lists[0];
    let inner = lists[1];
    let b_item = items(&doc, outer)[1];
    let leaf = lead(&doc, b_item);
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 0)),
        Command::Indent,
    );
    assert_eq!(items(&doc, outer).len(), 1);
    let after = items(&doc, inner);
    assert_eq!(after.len(), 2);
    assert_eq!(after[1], b_item);
    assert_eq!(doc.display(lead(&doc, after[0])), "x");
    assert_eq!(doc.display(lead(&doc, after[1])), "b");
}

#[test]
fn indent_ordered_makes_ordered_child() {
    let mut doc = load_markdown("1. a\n2. b\n", editor_options());
    let list = first_list(&doc);
    assert!(doc.extra(list).ordered_start().is_some());
    let second = items(&doc, list)[1];
    let leaf = lead(&doc, second);
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 0)),
        Command::Indent,
    );
    let first = items(&doc, list)[0];
    let nested = doc
        .arena
        .children(first)
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .expect("nested");
    assert_eq!(doc.extra(nested).ordered_start(), Some(1));
    assert_eq!(items(&doc, nested)[0], second);
}

#[test]
fn outdent_nested_item_matches_lift() {
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
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(b_leaf.index, 1)),
        Command::Outdent,
    );
    assert_eq!(items(&doc, outer).len(), 2);
    assert_eq!(items(&doc, outer)[1], b_item);
    assert_eq!(out.block, b_leaf.index);
    assert_eq!(out.offset, 1);
    assert_eq!(
        doc.preorder()
            .into_iter()
            .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
            .count(),
        1
    );
    assert!(!doc.extra(outer).list_loose());
    let changes = doc.take_changes();
    assert_changeset_parents_live(&doc, &changes);
}

#[test]
fn outdent_only_nested_item_stays_reachable_and_undoable() {
    for source in ["- - a\n", "- - a\n- b\n"] {
        let mut doc = Doc::new(load_markdown(source, editor_options()));
        let a = doc
            .text_leaves()
            .into_iter()
            .find(|&block| doc.text(block) == Some("a"))
            .expect("a");
        let _ = doc.apply(Sel::collapsed(caret(a, 1)), Command::Outdent);

        assert_eq!(doc.document.to_markdown(), source.replace("- - a", "- a"));
        assert!(doc.document.live_id(a).is_some());
        assert!(doc.document.preorder().into_iter().any(|id| id.index == a));

        let _ = doc.undo().expect("undo outdent");
        assert_eq!(
            doc.document
                .preorder()
                .into_iter()
                .filter(
                    |&id| doc.document.arena.get(id).map(|node| node.kind) == Some(BlockKind::List)
                )
                .count(),
            2
        );
        assert_eq!(doc.text(a), Some("a"));
        assert!(doc.document.live_id(a).is_some());
        assert!(doc.document.preorder().into_iter().any(|id| id.index == a));

        let _ = doc.redo().expect("redo outdent");
        assert_eq!(doc.document.to_markdown(), source.replace("- - a", "- a"));
        assert!(doc.document.live_id(a).is_some());
        assert!(doc.document.preorder().into_iter().any(|id| id.index == a));
    }
}

#[test]
fn outdent_last_root_item_undo_restores_the_list() {
    let mut doc = Doc::new(load_markdown("- a\n", editor_options()));
    let a = doc.text_leaves()[0];

    let _ = doc.apply(Sel::collapsed(caret(a, 1)), Command::Outdent);
    assert_eq!(doc.document.to_markdown(), "a\n");

    let _ = doc.undo().expect("undo outdent");
    assert_eq!(doc.document.to_markdown(), "- a\n");
    assert_eq!(doc.text(a), Some("a"));

    let _ = doc.redo().expect("redo outdent");
    assert_eq!(doc.document.to_markdown(), "a\n");
}

#[test]
fn indent_then_outdent_restores_tight() {
    let mut doc = load_markdown("- a\n- b\n", editor_options());
    let list = first_list(&doc);
    assert!(!doc.extra(list).list_loose());
    let second = items(&doc, list)[1];
    let leaf = lead(&doc, second);
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 0)),
        Command::Indent,
    );
    assert!(doc.extra(list).list_loose());
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 0)),
        Command::Outdent,
    );
    assert!(!doc.extra(list).list_loose());
    assert_eq!(items(&doc, list).len(), 2);
    assert_eq!(items(&doc, list)[1], second);
}

#[test]
fn indent_preserves_source_loose_spacing_and_undo_restores_it() {
    let source = "- a\n\n- b\n";
    let mut doc = Doc::new(load_markdown(source, editor_options()));
    let list = first_list(&doc.document);
    assert!(doc.document.extra(list).list_loose());
    let second = doc.text_leaves()[1];

    let _ = doc.apply(Sel::collapsed(caret(second, 0)), Command::Indent);

    assert!(doc.document.extra(list).list_loose());
    let indented = "- a\n  \n  - b\n";
    assert_eq!(doc.document.to_markdown(), indented);
    let _ = doc.undo().expect("undo indent");
    assert_eq!(doc.document.to_markdown(), source);
    let _ = doc.redo().expect("redo indent");
    assert_eq!(doc.document.to_markdown(), indented);
    let again = load_markdown(indented, editor_options());
    assert_eq!(again.to_markdown(), indented);
}

#[test]
fn code_indent_inserts_tab() {
    let mut doc = load_markdown("```\nxy\n```\n", editor_options());
    let leaf = doc
        .preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::CodeBlock))
        .expect("code");
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 0)),
        Command::Indent,
    );
    assert_eq!(doc.display(leaf), "\txy");
    assert_eq!(out.offset, 1);
}

#[test]
fn table_indent_is_noop() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| cd | e |\n", editor_options());
    let cell = doc
        .text_leaves()
        .into_iter()
        .find(|&id| doc.kind(id) == Some(BlockKind::TableCell) && doc.text_of(id) == Some("cd"))
        .expect("cell");
    let at = caret(cell, 1);
    let out = apply(&mut doc, Sel::collapsed(at), Command::Indent);
    assert_eq!(out, at);
    assert_eq!(doc.text_of(cell).unwrap(), "cd");
    let out = apply(&mut doc, Sel::collapsed(at), Command::Outdent);
    assert_eq!(out, at);
    assert_eq!(doc.text_of(cell).unwrap(), "cd");
}

#[test]
fn indent_two_items_sinks_into_prev() {
    let mut doc = load_markdown("- a\n- b\n- c\n", editor_options());
    let list = first_list(&doc);
    let host = items(&doc, list);
    let a = host[0];
    let b = host[1];
    let c = host[2];
    let b_leaf = lead(&doc, b);
    let c_leaf = lead(&doc, c);
    let out = apply(
        &mut doc,
        Sel {
            anchor: caret(b_leaf.index, 0),
            head: caret(c_leaf.index, 1),
        },
        Command::Indent,
    );
    assert_eq!(items(&doc, list), vec![a]);
    let nested = doc
        .arena
        .children(a)
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .expect("nested");
    assert_eq!(items(&doc, nested), vec![b, c]);
    assert_eq!(doc.display(lead(&doc, b)), "b");
    assert_eq!(doc.display(lead(&doc, c)), "c");
    assert_eq!(out, caret(c_leaf.index, 1));
}

#[test]
fn indent_range_including_first_is_noop() {
    let mut doc = load_markdown("- a\n- b\n- c\n", editor_options());
    let list = first_list(&doc);
    let before = items(&doc, list);
    let a_leaf = lead(&doc, before[0]);
    let b_leaf = lead(&doc, before[1]);
    let out = apply(
        &mut doc,
        Sel {
            anchor: caret(a_leaf.index, 0),
            head: caret(b_leaf.index, 1),
        },
        Command::Indent,
    );
    assert_eq!(items(&doc, list), before);
    assert_eq!(out, caret(b_leaf.index, 1));
    assert_eq!(
        doc.preorder()
            .into_iter()
            .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
            .count(),
        1
    );
}

#[test]
fn indent_cross_list_is_noop() {
    let mut doc = load_markdown("- a\n  - b\n- c\n", editor_options());
    let lists: Vec<_> = doc
        .preorder()
        .into_iter()
        .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .collect();
    let outer = lists[0];
    let inner = lists[1];
    let b_item = items(&doc, inner)[0];
    let c_item = items(&doc, outer)[1];
    let b_leaf = lead(&doc, b_item);
    let c_leaf = lead(&doc, c_item);
    let outer_before = items(&doc, outer);
    let inner_before = items(&doc, inner);
    let out = apply(
        &mut doc,
        Sel {
            anchor: caret(b_leaf.index, 0),
            head: caret(c_leaf.index, 1),
        },
        Command::Indent,
    );
    assert_eq!(items(&doc, outer), outer_before);
    assert_eq!(items(&doc, inner), inner_before);
    assert_eq!(out, caret(c_leaf.index, 1));
}

#[test]
fn outdent_two_nested_items_together() {
    let mut doc = load_markdown("- a\n  - b\n  - c\n- d\n", editor_options());
    let lists: Vec<_> = doc
        .preorder()
        .into_iter()
        .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
        .collect();
    let outer = lists[0];
    let inner = lists[1];
    let a = items(&doc, outer)[0];
    let d = items(&doc, outer)[1];
    let b = items(&doc, inner)[0];
    let c = items(&doc, inner)[1];
    let b_leaf = lead(&doc, b);
    let c_leaf = lead(&doc, c);
    let out = apply(
        &mut doc,
        Sel {
            anchor: caret(b_leaf.index, 0),
            head: caret(c_leaf.index, 1),
        },
        Command::Outdent,
    );
    assert_eq!(items(&doc, outer), vec![a, b, c, d]);
    assert_eq!(doc.display(lead(&doc, b)), "b");
    assert_eq!(doc.display(lead(&doc, c)), "c");
    assert_eq!(out, caret(c_leaf.index, 1));
    assert_eq!(
        doc.preorder()
            .into_iter()
            .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::List))
            .count(),
        1
    );
}

#[test]
fn paragraph_indent_inserts_tab() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let out = apply(&mut doc, Sel::collapsed(caret(leaf, 0)), Command::Indent);
    assert_eq!(doc.text_of(leaf).unwrap(), "\thello");
    assert_eq!(out.offset, 1);
}

#[test]
fn paragraph_outdent_strips_leading_tab() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = apply(&mut doc, Sel::collapsed(caret(leaf, 0)), Command::Indent);
    let out = apply(&mut doc, Sel::collapsed(caret(leaf, 1)), Command::Outdent);
    assert_eq!(doc.text_of(leaf).unwrap(), "hello");
    assert_eq!(out.offset, 0);
}

#[test]
fn paragraph_outdent_strips_up_to_four_spaces() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert {
            text: "      ".into(),
        },
    );
    let out = apply(&mut doc, Sel::collapsed(caret(leaf, 6)), Command::Outdent);
    assert_eq!(doc.text_of(leaf).unwrap(), "  hello");
    assert_eq!(out.offset, 2);
}

#[test]
fn heading_indent_inserts_tab() {
    let mut doc = load_markdown("# hi\n", editor_options());
    let leaf = doc
        .text_leaves()
        .into_iter()
        .find(|&id| matches!(doc.kind(id), Some(BlockKind::Heading(_))))
        .expect("heading");
    let out = apply(&mut doc, Sel::collapsed(caret(leaf, 0)), Command::Indent);
    assert_eq!(doc.text_of(leaf).unwrap(), "\thi");
    assert_eq!(out.offset, 1);
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(doc.leaf_source(id), "# \thi");
}

#[test]
fn image_selection_indent_uses_source_coordinates() {
    let mut doc = load_markdown("![€€€](u)\n", editor_options());
    let image = doc
        .text_leaves()
        .into_iter()
        .find(|&id| doc.kind(id) == Some(BlockKind::Image))
        .expect("image");
    let source_len = doc
        .live_id(image)
        .map(|id| doc.leaf_source(id).len())
        .expect("live");
    let out = apply(
        &mut doc,
        Sel {
            anchor: caret(image, 0),
            head: caret(image, source_len),
        },
        Command::Indent,
    );
    let id = doc.live_id(image).expect("live");
    assert_eq!(doc.leaf_source(id), "\t![€€€](u)");
    assert_eq!(out.offset, source_len + 1);
}

#[test]
fn image_outdent_at_multibyte_offset_does_not_slice_display() {
    let mut doc = load_markdown("![€€€](u)\n", editor_options());
    let image = doc
        .text_leaves()
        .into_iter()
        .find(|&id| doc.kind(id) == Some(BlockKind::Image))
        .expect("image");
    let out = apply(&mut doc, Sel::collapsed(caret(image, 5)), Command::Outdent);
    let id = doc.live_id(image).expect("live");
    assert_eq!(doc.leaf_source(id), "![€€€](u)");
    assert_eq!(out.offset, 5);
}

#[test]
fn paragraph_selection_indents_each_line() {
    let mut doc = load_markdown("ab\ncd\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let len = doc.text_of(leaf).unwrap().len();
    let out = apply(
        &mut doc,
        Sel {
            anchor: caret(leaf, 0),
            head: caret(leaf, len),
        },
        Command::Indent,
    );
    assert_eq!(doc.text_of(leaf).unwrap(), "\tab\n\tcd");
    assert_eq!(out.offset, len + 2);
}

#[test]
fn code_selection_indents_each_line() {
    let mut doc = load_markdown("```\nxy\nzz\n```\n", editor_options());
    let leaf = doc
        .preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::CodeBlock))
        .expect("code");
    let len = doc.display(leaf).len();
    let out = apply(
        &mut doc,
        Sel {
            anchor: caret(leaf.index, 0),
            head: caret(leaf.index, len),
        },
        Command::Indent,
    );
    assert_eq!(doc.display(leaf), "\txy\n\tzz");
    assert_eq!(out.offset, len + 2);
}

#[test]
fn code_outdent_strips_tab_from_the_line() {
    let mut doc = load_markdown("```\n\txy\n```\n", editor_options());
    let leaf = doc
        .preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::CodeBlock))
        .expect("code");
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 1)),
        Command::Outdent,
    );
    assert_eq!(doc.display(leaf), "xy");
    assert_eq!(out.offset, 0);
}

#[test]
fn mermaid_indent_inserts_tab() {
    let mut doc = load_markdown("```mermaid\nflowchart TD\n```\n", editor_options());
    let leaf = doc
        .preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::Mermaid))
        .expect("mermaid");
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 0)),
        Command::Indent,
    );
    assert_eq!(doc.display(leaf), "\tflowchart TD");
    assert_eq!(out.offset, 1);
}

#[test]
fn math_indent_inserts_tab() {
    let mut doc = load_markdown("$$\nx\n$$\n", editor_options());
    let leaf = doc
        .preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::Math))
        .expect("math");
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 0)),
        Command::Indent,
    );
    assert_eq!(doc.display(leaf), "\tx");
    assert_eq!(out.offset, 1);
}

#[test]
fn top_level_outdent_unwraps_item_to_paragraph() {
    let mut doc = load_markdown("- a\n- b\n", editor_options());
    let list = first_list(&doc);
    let b = items(&doc, list)[1];
    let leaf = lead(&doc, b);
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf.index, 0)),
        Command::Outdent,
    );
    assert_eq!(items(&doc, list).len(), 1);
    assert_eq!(doc.kind(leaf.index), Some(BlockKind::Paragraph));
    assert_eq!(doc.display(leaf), "b");
    assert_eq!(out.offset, 0);
}

#[test]
fn outdent_lifting_the_whole_list_then_undo_restores_the_subtree() {
    use crate::doc::Doc;

    let mut doc = Doc::new(load_markdown("- first\n  - second\n", editor_options()));
    let leaf = doc.text_leaves()[0];
    let _ = doc.take_changes();
    let _ = doc.apply(Sel::collapsed(caret(leaf, 0)), Command::Outdent);
    let lifted = doc.document.to_markdown();
    assert_eq!(lifted, "first\n\n- second\n");

    let restored = doc.undo();
    assert!(restored.is_some(), "the lift must be undoable");
    assert_eq!(
        doc.document.to_markdown(),
        "- first\n  - second\n",
        "undo must return the nested list into the item"
    );
}

#[test]
fn outdent_middle_item_preserves_document_text_order() {
    let mut doc = load_markdown("- a\n- b\n- c\n", editor_options());
    let block = doc.text_leaves()[1];
    let _ = apply(&mut doc, Sel::collapsed(caret(block, 0)), Command::Outdent);
    let texts = doc
        .text_leaves()
        .into_iter()
        .filter_map(|id| doc.text_of(id))
        .collect::<Vec<_>>();
    assert_eq!(texts, vec!["a", "b", "c"], "{:?}", doc.to_markdown());
    let md = doc.to_markdown();
    assert!(md.contains("b"), "{md:?}");
}

#[test]
fn undo_after_middle_outdent_restores_the_list() {
    let mut d = Doc::new(load_markdown("- a\n- b\n- c\n", editor_options()));
    let block = d.text_leaves()[1];
    let before = d.document.to_markdown();
    let _ = d.apply(Sel::collapsed(caret(block, 0)), Command::Outdent);
    assert_ne!(d.document.to_markdown(), before);
    let _ = d.undo().expect("undo");
    assert_eq!(d.document.to_markdown(), before);
    let _ = d.redo().expect("redo");
    assert_eq!(
        d.document
            .text_leaves()
            .into_iter()
            .filter_map(|id| d.text(id))
            .collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
}

#[test]
fn outdent_two_nested_items_keeps_the_tail_after_them() {
    let source = "- a\n  - b\n  - c\n  - d\n- e\n";
    let mut doc = Doc::new(load_markdown(source, editor_options()));
    let leaves = doc.text_leaves();
    let (b, c) = (leaves[1], leaves[2]);
    let _ = doc.apply(
        Sel {
            anchor: caret(b, 0),
            head: caret(c, 1),
        },
        Command::Outdent,
    );
    let texts: Vec<_> = doc
        .text_leaves()
        .into_iter()
        .map(|l| doc.text(l).unwrap().to_string())
        .collect();
    assert_eq!(
        texts,
        ["a", "b", "c", "d", "e"],
        "tree order must not move d"
    );
    assert_eq!(doc.document.to_markdown(), "- a\n- b\n- c\n  - d\n- e\n");
    let changes = doc.take_changes();
    assert_changeset_parents_live(&doc.document, &changes);

    assert!(doc.undo().is_some());
    assert_eq!(doc.document.to_markdown(), source);
    assert!(doc.redo().is_some());
    assert_eq!(doc.document.to_markdown(), "- a\n- b\n- c\n  - d\n- e\n");
}
