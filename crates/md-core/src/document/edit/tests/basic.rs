use super::support::caret;
use crate::block::BlockKind;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{PasteIntent, editor_options, load_markdown};
use crate::inline::InlineMarks;

#[test]
fn empty_document_inserts_into_seeded_paragraph() {
    let mut doc = load_markdown("", editor_options());
    let leaves = doc.text_leaves();
    assert_eq!(leaves.len(), 1);
    assert_eq!(doc.kind(leaves[0]), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaves[0]).unwrap(), "");
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(0, 0)),
        Command::Insert { text: "hi".into() },
    );
    assert_eq!(out, caret(leaves[0], 2));
    assert_eq!(doc.text_of(leaves[0]).unwrap(), "hi");
}

#[test]
fn blank_document_inserts_into_seeded_paragraph() {
    let mut doc = load_markdown("\n\n", editor_options());
    let leaves = doc.text_leaves();
    assert_eq!(leaves.len(), 1);
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaves[0], 0)),
        Command::Insert { text: "x".into() },
    );
    assert_eq!(out.offset, 1);
    assert_eq!(doc.text_of(leaves[0]).unwrap(), "x");
}

#[test]
fn apply_insert_matches_paste() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert { text: "X".into() },
    );
    assert_eq!(out, caret(leaf, 1));
    assert!(doc.text_of(leaf).unwrap().starts_with('X'));
}

#[test]
fn apply_break_splits_paragraph() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let out = apply(&mut doc, Sel::collapsed(caret(leaf, 2)), Command::Break);
    assert_eq!(out.offset, 0);
    assert_ne!(out.block, leaf);
    assert_eq!(doc.text_of(leaf).unwrap(), "he");
    assert_eq!(doc.text_of(out.block).unwrap(), "llo");
}

#[test]
fn apply_soft_break_inserts_newline() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let out = apply(&mut doc, Sel::collapsed(caret(leaf, 2)), Command::SoftBreak);
    assert_eq!(out, caret(leaf, 3));
    assert_eq!(doc.text_of(leaf).unwrap(), "he\nllo");
    assert_eq!(doc.text_leaves(), vec![leaf]);
}

#[test]
fn apply_break_is_noop_in_table_cell() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| cd | e |\n", editor_options());
    let cell = doc
        .text_leaves()
        .into_iter()
        .find(|&id| doc.kind(id) == Some(BlockKind::TableCell) && doc.text_of(id) == Some("cd"))
        .expect("cell");
    let at = caret(cell, 1);
    let out = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_eq!(out, at);
    assert_eq!(doc.kind(cell), Some(BlockKind::TableCell));
    assert_eq!(doc.text_of(cell).unwrap(), "cd");
    assert_eq!(doc.to_markdown(), "| a | b |\n| --- | --- |\n| cd | e |\n");
}

#[test]
fn apply_soft_break_inserts_newline_in_table_cell() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| cd | e |\n", editor_options());
    let cell = doc
        .text_leaves()
        .into_iter()
        .find(|&id| doc.kind(id) == Some(BlockKind::TableCell) && doc.text_of(id) == Some("cd"))
        .expect("cell");
    let at = caret(cell, 1);
    let out = apply(&mut doc, Sel::collapsed(at), Command::SoftBreak);
    assert_eq!(out, caret(cell, 2));
    assert_eq!(doc.kind(cell), Some(BlockKind::TableCell));
    assert_eq!(doc.text_of(cell).unwrap(), "c\nd");
}

#[test]
fn line_breaks_are_noop_in_image_blocks() {
    for command in [Command::Break, Command::SoftBreak] {
        let mut doc = load_markdown("![alt](https://example.com/image.png)\n", editor_options());
        let image = doc
            .text_leaves()
            .into_iter()
            .find(|&id| doc.kind(id) == Some(BlockKind::Image))
            .expect("image");
        let at = caret(image, "![alt](https://".len());

        let out = apply(&mut doc, Sel::collapsed(at), command.clone());

        assert_eq!(out, at);
        assert_eq!(doc.kind(image), Some(BlockKind::Image));
        let id = doc.live_id(image).expect("live image");
        assert_eq!(doc.leaf_source(id), "![alt](https://example.com/image.png)");
        assert_eq!(doc.to_markdown(), "![alt](https://example.com/image.png)\n");
    }
}

#[test]
fn line_breaks_on_a_container_are_inert() {
    for command in [Command::Break, Command::SoftBreak] {
        let mut doc = load_markdown("> q\n", editor_options());
        let quote = doc
            .preorder()
            .into_iter()
            .find(|id| doc.kind(id.index) == Some(BlockKind::BlockQuote))
            .expect("quote")
            .index;
        let at = caret(quote, 0);
        let live = doc.arena.live_count();

        let out = apply(&mut doc, Sel::collapsed(at), command.clone());

        assert_eq!(out, at, "{command:?}");
        assert_eq!(doc.arena.live_count(), live, "{command:?}");
        assert_eq!(doc.to_markdown(), "> q\n", "{command:?}");
    }
}

#[test]
fn apply_delete_backward_merges_at_start() {
    let mut doc = load_markdown("ab\n\ncd\n", editor_options());
    let leaves = doc.text_leaves();
    assert_eq!(leaves.len(), 2);
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaves[1], 0)),
        Command::DeleteBackward,
    );
    assert_eq!(out.block, leaves[0]);
    assert_eq!(out.offset, 2);
    assert_eq!(doc.text_of(leaves[0]).unwrap(), "abcd");
}

#[test]
fn apply_delete_forward_merges_at_end() {
    let mut doc = load_markdown("ab\n\ncd\n", editor_options());
    let leaves = doc.text_leaves();
    assert_eq!(leaves.len(), 2);
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaves[0], 2)),
        Command::DeleteForward,
    );
    assert_eq!(out, caret(leaves[0], 2));
    assert_eq!(doc.text_of(leaves[0]), Some("abcd"));
    assert!(doc.live_id(leaves[1]).is_none());
}

#[test]
fn apply_delete_backward_clears_span() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let out = apply(
        &mut doc,
        Sel {
            anchor: caret(leaf, 1),
            head: caret(leaf, 4),
        },
        Command::DeleteBackward,
    );
    assert_eq!(out, caret(leaf, 1));
    assert_eq!(doc.text_of(leaf).unwrap(), "ho");
}

#[test]
fn apply_paste_fragment() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 5)),
        Command::Paste {
            text: "\n\n# title\n".into(),
            intent: PasteIntent::IndependentFragment,
        },
    );
    assert_eq!(doc.kind(out.block), Some(BlockKind::Heading(1)));
    assert_eq!(doc.text_of(out.block).unwrap(), "title");
}

#[test]
fn cross_block_delete_maps_revealed_anchor_before_collapsing() {
    let mut doc = load_markdown("**ab**\n\ncd\n", editor_options());
    let leaves = doc.text_leaves();
    let anchor = doc.retarget_inline_focus(caret(leaves[0], 1));
    assert_eq!(anchor.offset, 3, "fixture must be revealed");
    let _ = apply(
        &mut doc,
        Sel {
            anchor,
            head: caret(leaves[1], 1),
        },
        Command::DeleteBackward,
    );
    let visible = doc.text_of(leaves[0]).unwrap();
    assert!(
        !visible.contains('b') && visible.contains('a') && visible.ends_with('d'),
        "b and c must be gone: {visible:?}; markdown={:?}",
        doc.to_markdown()
    );
}

#[test]
fn partial_selection_across_two_tables_preserves_unselected_cells() {
    let source =
        "| a | b |\n| --- | --- |\n| c | d |\n\nbetween\n\n| e | f |\n| --- | --- |\n| g | h |\n";
    let mut doc = load_markdown(source, editor_options());
    let leaves = doc.text_leaves();
    let _ = apply(
        &mut doc,
        Sel {
            anchor: caret(leaves[2], 1),
            head: caret(leaves[6], 0),
        },
        Command::DeleteBackward,
    );
    let text: Vec<_> = doc
        .text_leaves()
        .into_iter()
        .filter_map(|id| doc.text_of(id))
        .collect();
    assert!(
        text.contains(&"a") && text.contains(&"c") && text.contains(&"f") && text.contains(&"h"),
        "unselected cells lost: {text:?}; markdown={:?}",
        doc.to_markdown()
    );
}

#[test]
fn reversed_partial_selection_across_two_tables_preserves_unselected_cells() {
    let source =
        "| a | b |\n| --- | --- |\n| c | d |\n\nbetween\n\n| e | f |\n| --- | --- |\n| g | h |\n";
    let mut doc = load_markdown(source, editor_options());
    let leaves = doc.text_leaves();
    let _ = apply(
        &mut doc,
        Sel {
            anchor: caret(leaves[6], 0),
            head: caret(leaves[2], 1),
        },
        Command::DeleteForward,
    );
    let text: Vec<_> = doc
        .text_leaves()
        .into_iter()
        .filter_map(|id| doc.text_of(id))
        .collect();
    assert!(
        text.contains(&"a") && text.contains(&"c") && text.contains(&"f") && text.contains(&"h"),
        "unselected cells lost: {text:?}; markdown={:?}",
        doc.to_markdown()
    );
}

#[test]
fn indent_cross_block_selection_does_not_replace_selected_content() {
    let mut doc = load_markdown("alpha\n\n```rust\nbeta\n```\n", editor_options());
    let leaves = doc.text_leaves();
    let _ = apply(
        &mut doc,
        Sel {
            anchor: caret(leaves[0], 2),
            head: caret(leaves[1], 2),
        },
        Command::Indent,
    );
    let text: String = doc
        .text_leaves()
        .into_iter()
        .filter_map(|id| doc.collapsed_text_of(id))
        .flat_map(str::chars)
        .filter(|ch| !ch.is_whitespace())
        .collect();
    assert_eq!(
        text,
        "alphabeta",
        "selected text lost: {:?}",
        doc.to_markdown()
    );
    assert_eq!(doc.kind(leaves[1]), Some(BlockKind::CodeBlock));
}

#[test]
fn indenting_a_selection_keeps_existing_inline_marks() {
    let mut doc = load_markdown("hello **world**\n", editor_options());
    let block = doc.text_leaves()[0];
    let _ = apply(
        &mut doc,
        Sel {
            anchor: caret(block, 0),
            head: caret(block, 11),
        },
        Command::Indent,
    );
    let id = doc.live_id(block).expect("live");
    let snapshot = doc.leaf_snapshot(id).expect("snapshot");
    assert!(
        snapshot
            .runs
            .iter()
            .any(|run| run.marks.contains(InlineMarks::STRONG)),
        "inline marks lost: source={:?}, display={:?}",
        doc.to_markdown(),
        snapshot.display
    );
    assert!(
        doc.to_markdown().contains("**world**"),
        "{:?}",
        doc.to_markdown()
    );
}

#[test]
fn deleting_list_text_preserves_its_unselected_rule() {
    let mut doc = load_markdown("- abc\n\n  ---\n", editor_options());
    assert!(
        doc.preorder()
            .into_iter()
            .any(|id| doc.kind(id.index) == Some(BlockKind::ThematicBreak)),
        "fixture must contain a rule"
    );
    let block = doc.text_leaves()[0];
    let _ = apply(
        &mut doc,
        Sel {
            anchor: caret(block, 0),
            head: caret(block, 3),
        },
        Command::DeleteBackward,
    );
    assert!(
        doc.preorder()
            .into_iter()
            .any(|id| doc.kind(id.index) == Some(BlockKind::ThematicBreak)),
        "unselected rule lost: {:?}",
        doc.to_markdown()
    );
    let markdown = doc.to_markdown();
    assert!(markdown.contains("---"), "{markdown:?}");
}

#[test]
fn deleting_across_a_rule_removes_the_selected_rule() {
    let mut doc = load_markdown("alpha\n\n---\n\nomega\n", editor_options());
    let leaves = doc.text_leaves();
    let last_len = doc
        .live_id(leaves[1])
        .map(|id| doc.caret_text(id).len())
        .expect("leaf");
    let _ = apply(
        &mut doc,
        Sel {
            anchor: caret(leaves[0], 0),
            head: caret(leaves[1], last_len),
        },
        Command::DeleteBackward,
    );
    assert!(
        !doc.preorder()
            .into_iter()
            .any(|id| doc.kind(id.index) == Some(BlockKind::ThematicBreak)),
        "selected rule survived: {:?}",
        doc.to_markdown()
    );
    let markdown = doc.to_markdown();
    let reloaded = load_markdown(&markdown, editor_options());
    assert!(
        !reloaded
            .preorder()
            .into_iter()
            .any(|id| reloaded.kind(id.index) == Some(BlockKind::ThematicBreak)),
        "rule survives a save round-trip: {markdown:?}"
    );
}

#[test]
fn forward_merge_with_a_heading_keeps_saved_text_equal_to_display() {
    let mut doc = load_markdown("a\n\n# b\n", editor_options());
    let block = doc.text_leaves()[0];
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(block, 1)),
        Command::DeleteForward,
    );
    assert_eq!(doc.collapsed_text_of(block), Some("ab"));
    let markdown = doc.to_markdown();
    let reloaded = load_markdown(&markdown, editor_options());
    assert_eq!(
        reloaded.text_of(reloaded.first_text_leaf().unwrap()),
        Some("ab"),
        "markdown={markdown:?}"
    );
    assert_eq!(out.block, block);
}

#[test]
fn forward_merge_with_a_setext_heading_drops_the_underline() {
    let mut doc = load_markdown("a\n\nb\n===\n", editor_options());
    let block = doc.text_leaves()[0];
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(block, 1)),
        Command::DeleteForward,
    );
    assert_eq!(doc.kind(block), Some(BlockKind::Paragraph));
    assert_eq!(doc.collapsed_text_of(block), Some("ab"));
    let markdown = doc.to_markdown();
    let reloaded = load_markdown(&markdown, editor_options());
    assert_eq!(
        reloaded.text_of(reloaded.first_text_leaf().unwrap()),
        Some("ab"),
        "markdown={markdown:?}"
    );
    let headings = reloaded
        .preorder()
        .into_iter()
        .filter(|id| reloaded.kind(id.index) == Some(BlockKind::Heading(1)))
        .count();
    assert_eq!(headings, 0, "saved markdown reloads as a heading");
    assert_eq!(out.block, block);
}

#[test]
fn merging_paragraphs_preserves_the_tail_inline_style() {
    let mut doc = load_markdown("a\n\n**b**\n\nafter\n", editor_options());
    let block = doc.text_leaves()[0];
    let next = doc.text_leaves()[2];
    let after_merge = apply(
        &mut doc,
        Sel::collapsed(caret(block, 1)),
        Command::DeleteForward,
    );
    doc.retarget_inline_focus(after_merge);
    doc.retarget_inline_focus(caret(next, 0));
    let id = doc.live_id(block).expect("live");
    let snapshot = doc.leaf_snapshot(id).expect("snapshot");
    assert_eq!(snapshot.display, "ab");
    assert!(
        snapshot
            .runs
            .iter()
            .any(|run| run.marks.contains(InlineMarks::STRONG)),
        "tail style lost: {:?}; source={:?}",
        snapshot.runs,
        doc.to_markdown()
    );
    assert!(
        doc.to_markdown().contains("**b**"),
        "{:?}",
        doc.to_markdown()
    );
}

#[test]
fn deleting_list_text_preserves_an_unselected_empty_alt_image() {
    let mut doc = load_markdown("- remove\n\n- ![](image.png)\n", editor_options());
    let block = doc.text_leaves()[0];
    let _ = apply(
        &mut doc,
        Sel {
            anchor: caret(block, 0),
            head: caret(block, 6),
        },
        Command::DeleteBackward,
    );
    let markdown = doc.to_markdown();
    assert!(
        markdown.contains("![](image.png)"),
        "unselected image lost: {markdown:?}"
    );
    let reloaded = load_markdown(&markdown, editor_options());
    assert!(
        reloaded
            .preorder()
            .into_iter()
            .any(|id| reloaded.kind(id.index) == Some(BlockKind::Image)),
        "image lost on reload: {markdown:?}"
    );
}

#[test]
fn delete_spanning_a_table_endpoint_removes_the_rules_between() {
    let mut doc = load_markdown("| a |\n| --- |\n\n---\n\nz\n", editor_options());
    let leaves = doc.text_leaves();
    let _ = apply(
        &mut doc,
        Sel {
            anchor: caret(leaves[0], 0),
            head: caret(*leaves.last().unwrap(), 1),
        },
        Command::DeleteForward,
    );
    assert_eq!(doc.to_markdown(), "", "rule must go with the selection");

    let mut doc = load_markdown("| a | b |\n| --- | --- |\n\n---\n\nz\n", editor_options());
    let leaves = doc.text_leaves();
    let _ = apply(
        &mut doc,
        Sel {
            anchor: caret(leaves[1], 0),
            head: caret(*leaves.last().unwrap(), 1),
        },
        Command::DeleteForward,
    );
    assert_eq!(
        doc.to_markdown(),
        "| a |  |\n| --- | --- |\n",
        "partial table keeps the first cell, drops the rule"
    );

    let mut doc = load_markdown("a\n\n---\n\n| x | y |\n| --- | --- |\n", editor_options());
    let leaves = doc.text_leaves();
    let _ = apply(
        &mut doc,
        Sel {
            anchor: caret(leaves[0], 0),
            head: caret(leaves[2], 0),
        },
        Command::DeleteForward,
    );
    assert_eq!(
        doc.to_markdown(),
        "|  | y |\n| --- | --- |\n",
        "partial tail table keeps the last cell, drops the rule"
    );

    let mut doc = load_markdown(
        "| a | b |\n| --- | --- |\n\n---\n\n| e | f |\n| --- | --- |\n",
        editor_options(),
    );
    let leaves = doc.text_leaves();
    let _ = apply(
        &mut doc,
        Sel {
            anchor: caret(leaves[1], 0),
            head: caret(leaves[3], 0),
        },
        Command::DeleteForward,
    );
    assert_eq!(
        doc.to_markdown(),
        "| a |  |\n| --- | --- |\n\n|  | f |\n| --- | --- |\n",
        "both tables keep their unselected cells, the rule between goes"
    );
}

#[test]
fn indenting_multiline_phrasing_lines_targets_source_line_starts() {
    let mut doc = load_markdown("**bold** and [link](u)\nplain\n", editor_options());
    let block = doc.text_leaves()[0];
    let end = doc.collapsed_text_of(block).unwrap().len();
    assert_eq!(end, 19, "fixture display: `bold and link\\nplain`");
    let out = apply(
        &mut doc,
        Sel {
            anchor: caret(block, 0),
            head: caret(block, end),
        },
        Command::Indent,
    );
    let id = doc.live_id(block).expect("live");
    assert_eq!(doc.leaf_source(id), "\t**bold** and [link](u)\n\tplain");
    assert_eq!(doc.collapsed_display(id), "\tbold and link\n\tplain");
    assert_eq!(doc.to_markdown(), "\t**bold** and [link](u)\n\tplain\n");
    let snapshot = doc.leaf_snapshot(id).expect("snapshot");
    assert!(
        snapshot
            .runs
            .iter()
            .any(|run| run.marks.contains(InlineMarks::STRONG)),
        "strong lost: {:?}",
        doc.to_markdown()
    );
    let tabbed = doc.collapsed_display(id);
    let link = tabbed.find("link").expect("link text");
    assert_eq!(doc.link_at(id, link), Some("u"));
    assert_eq!(out, caret(block, tabbed.len()));
}

#[test]
fn outdenting_multiline_phrasing_lines_restores_the_source() {
    let mut doc = load_markdown("**bold** and [link](u)\nplain\n", editor_options());
    let block = doc.text_leaves()[0];
    let end = doc.collapsed_text_of(block).unwrap().len();
    let _ = apply(
        &mut doc,
        Sel {
            anchor: caret(block, 0),
            head: caret(block, end),
        },
        Command::Indent,
    );
    let id = doc.live_id(block).expect("live");
    let len = doc.collapsed_display(id).len();
    let out = apply(
        &mut doc,
        Sel {
            anchor: caret(block, 0),
            head: caret(block, len),
        },
        Command::Outdent,
    );
    assert_eq!(doc.leaf_source(id), "**bold** and [link](u)\nplain");
    assert_eq!(doc.collapsed_display(id), "bold and link\nplain");
    assert_eq!(doc.to_markdown(), "**bold** and [link](u)\nplain\n");
    assert_eq!(out, caret(block, 19));
}

#[test]
fn backspace_deletes_a_displayed_entity_as_one_character() {
    let mut doc = load_markdown("A &amp; B\n", editor_options());
    let block = doc.text_leaves()[0];
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(block, 3)),
        Command::DeleteBackward,
    );
    assert_eq!(doc.text_of(block), Some("A  B"));
    assert_eq!(doc.to_markdown(), "A  B\n");
}
