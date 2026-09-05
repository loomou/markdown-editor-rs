use super::support::kind_count;
use crate::block::{BlockId, BlockKind};
use crate::doc::Doc;
use crate::document::edit::{Caret, Command, Sel, apply};
use crate::document::{PasteIntent, editor_options, load_markdown};

fn fragment(text: &str) -> Command {
    Command::Paste {
        text: text.into(),
        intent: PasteIntent::IndependentFragment,
    }
}

fn at(block: BlockId, offset: usize) -> Sel {
    Sel::collapsed(Caret { block, offset })
}

#[test]
fn plain_paste_list_markers_does_not_splice() {
    let mut doc = load_markdown("hello\n", editor_options());
    let _ = doc.take_changes();
    let leaf = doc.text_leaves()[0];
    let lists = kind_count(&doc, crate::block::BlockKind::List);
    let (changes, _, _) = doc.paste(leaf, 5..5, "- item\n", PasteIntent::PlainText);
    assert!(!changes.is_structural());
    assert_eq!(kind_count(&doc, crate::block::BlockKind::List), lists);
    assert!(doc.text_of(leaf).unwrap().contains("- item"));
    assert_eq!(doc.text_leaves().len(), 1);
}

#[test]
fn plain_paste_footnote_does_not_splice() {
    let mut doc = load_markdown("hello\n", editor_options());
    let _ = doc.take_changes();
    let leaf = doc.text_leaves()[0];
    let quotes = kind_count(&doc, crate::block::BlockKind::BlockQuote);
    let (changes, _, _) = doc.paste(leaf, 5..5, "[^1]: note\n", PasteIntent::PlainText);
    assert!(!changes.is_structural());
    assert_eq!(
        kind_count(&doc, crate::block::BlockKind::BlockQuote),
        quotes
    );
    assert!(doc.text_of(leaf).unwrap().contains("[^1]: note"));
}

#[test]
fn independent_list_joins_host_list() {
    let mut doc = load_markdown("- a\n- b\n", editor_options());
    let host_list = doc
        .preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(crate::block::BlockKind::List))
        .expect("list");
    let host_items = doc.arena.children(host_list).count();
    let lists = kind_count(&doc, crate::block::BlockKind::List);
    let leaf = doc.text_leaves()[0];
    let text = doc.text_of(leaf).unwrap();
    let off = text.len();
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: leaf,
            offset: off,
        }),
        Command::Paste {
            text: "- c\n".into(),
            intent: PasteIntent::IndependentFragment,
        },
    );
    let changes = doc.take_changes();
    assert!(changes.is_structural());
    assert_eq!(doc.arena.children(host_list).count(), host_items + 1);
    assert_eq!(kind_count(&doc, crate::block::BlockKind::List), lists);
}

#[test]
fn plain_paste_in_a_cell_never_splices_blocks_into_the_row() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let _ = doc.take_changes();
    let cell = doc
        .text_leaves()
        .into_iter()
        .find(|id| doc.kind(*id) == Some(BlockKind::TableCell))
        .expect("cell");
    let leaves = doc.text_leaves().len();
    let (changes, block, _) = doc.paste(cell, 1..1, "> quoted\n\npara", PasteIntent::PlainText);
    assert!(!changes.is_structural());
    assert_eq!(block, cell);
    assert_eq!(doc.text_leaves().len(), leaves);
    assert_eq!(doc.text_of(cell).unwrap(), "a> quoted\n\npara");
    for row in doc.preorder() {
        if doc.arena.get(row).map(|n| n.kind) == Some(BlockKind::TableRow) {
            let kids: Vec<_> = doc.arena.children(row).collect();
            assert_eq!(kids.len(), 2);
            assert!(
                kids.iter()
                    .all(|k| { doc.arena.get(*k).map(|n| n.kind) == Some(BlockKind::TableCell) })
            );
        }
    }
}

#[test]
fn independent_two_items_join_host_list() {
    let mut doc = load_markdown("- a\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let off = doc.text_of(leaf).unwrap().len();
    let _ = apply(&mut doc, at(leaf, off), fragment("- x\n- y\n"));
    assert_eq!(kind_count(&doc, BlockKind::List), 1);
    assert_eq!(kind_count(&doc, BlockKind::ListItem), 3);
    assert_eq!(doc.to_markdown(), "- a\n- x\n- y\n");
}

#[test]
fn independent_paragraph_merges_into_host_leaf() {
    let mut doc = load_markdown("hello\n", editor_options());
    let _ = doc.take_changes();
    let leaf = doc.text_leaves()[0];
    let c = apply(&mut doc, at(leaf, 2), fragment("xx"));
    assert_eq!(doc.text_leaves().len(), 1);
    assert_eq!(doc.text_of(leaf).unwrap(), "hexxllo");
    assert_eq!(c.block, leaf);
    assert_eq!(c.offset, 4);
    assert_eq!(kind_count(&doc, BlockKind::Paragraph), 1);
    let changes = doc.take_changes();
    assert!(!changes.is_structural());
}

#[test]
fn merged_paragraph_keeps_inline_source() {
    let mut doc = load_markdown("a b\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = apply(&mut doc, at(leaf, 2), fragment("**bold**"));

    assert_eq!(doc.text_of(leaf).unwrap(), "a **bold**b");
    assert_eq!(doc.collapsed_text_of(leaf).unwrap(), "a boldb");
    assert_eq!(doc.to_markdown(), "a **bold**b\n");
    let id = doc.live_id(leaf).expect("leaf");
    assert!(
        doc.collapsed_runs(id)
            .iter()
            .any(|r| r.marks.contains(crate::inline::InlineMarks::STRONG))
    );
}

#[test]
fn merged_paragraph_drops_trailing_newline() {
    let mut doc = load_markdown("ab\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let c = apply(&mut doc, at(leaf, 2), fragment("x\n"));
    assert_eq!(doc.text_of(leaf).unwrap(), "abx");
    assert_eq!(c.offset, 3);
}

#[test]
fn independent_block_fragments_still_graft() {
    let mut head = load_markdown("hello\n", editor_options());
    let leaf = head.text_leaves()[0];
    let c = apply(&mut head, at(leaf, 5), fragment("# title\n"));
    assert_eq!(head.kind(c.block), Some(BlockKind::Heading(1)));
    assert_eq!(kind_count(&head, BlockKind::Heading(1)), 1);

    let mut fence = load_markdown("hello\n", editor_options());
    let fleaf = fence.text_leaves()[0];
    let fc = apply(
        &mut fence,
        at(fleaf, 5),
        fragment("```rust\nfn x() {}\n```\n"),
    );
    assert_eq!(fence.kind(fc.block), Some(BlockKind::CodeBlock));

    let mut two = load_markdown("hello\n", editor_options());
    let tleaf = two.text_leaves()[0];
    let _ = apply(&mut two, at(tleaf, 5), fragment("one\n\ntwo\n"));
    assert_eq!(kind_count(&two, BlockKind::Paragraph), 3);
}

#[test]
fn merging_into_a_nested_item_keeps_the_nesting() {
    let mut doc = load_markdown("- a\n  - b\n", editor_options());
    let inner = doc.text_leaves()[1];
    let _ = apply(&mut doc, at(inner, 1), fragment("x"));
    assert_eq!(doc.text_of(inner).unwrap(), "bx");
    assert_eq!(kind_count(&doc, BlockKind::List), 2);
    assert_eq!(doc.to_markdown(), "- a\n  - bx\n");
}

#[test]
fn plain_paste_into_code_block_stays_literal() {
    let mut doc = load_markdown("```rust\nfn x() {}\n```\n", editor_options());
    let _ = doc.take_changes();
    let leaf = doc.text_leaves()[0];
    let blocks = kind_count(&doc, BlockKind::CodeBlock);
    let (changes, _, off) = doc.paste(leaf, 0..0, "# t\n- i\n", PasteIntent::PlainText);
    assert!(!changes.is_structural());
    assert_eq!(kind_count(&doc, BlockKind::CodeBlock), blocks);
    assert_eq!(kind_count(&doc, BlockKind::Heading(1)), 0);
    assert_eq!(doc.text_leaves().len(), 1);
    assert_eq!(doc.text_of(leaf).unwrap(), "# t\n- i\nfn x() {}");
    assert_eq!(off, 8);
}

#[test]
fn plain_paste_with_blank_lines_stays_inside_fenced_blocks() {
    for (source, at, text, joined) in [
        ("```\ncode\n```\n", 4, "\nx\n\ny", "code\nx\n\ny"),
        (
            "```mermaid\ngraph TD\n```\n",
            8,
            "\nA-->B\n\nC-->D",
            "graph TD\nA-->B\n\nC-->D",
        ),
    ] {
        let mut doc = load_markdown(source, editor_options());
        let _ = doc.take_changes();
        let leaf = doc.text_leaves()[0];
        let kind = doc.kind(leaf).unwrap();
        let (changes, block, _) = doc.paste(leaf, at..at, text, PasteIntent::PlainText);
        assert!(!changes.is_structural(), "{source:?}");
        assert_eq!(block, leaf, "{source:?}");
        assert_eq!(doc.text_leaves().len(), 1, "{source:?}");
        assert_eq!(doc.text_of(leaf).unwrap(), joined, "{source:?}");

        let markdown = doc.to_markdown();
        let again = load_markdown(&markdown, editor_options());
        assert_eq!(again.text_leaves().len(), 1, "{markdown:?}");
        assert_eq!(
            again.kind(again.text_leaves()[0]),
            Some(kind),
            "{markdown:?}"
        );
        assert_eq!(
            again.text_of(again.text_leaves()[0]),
            Some(joined),
            "{markdown:?}"
        );
        assert_eq!(again.to_markdown(), markdown, "{source:?}");
    }
}

#[test]
fn plain_paste_with_blank_lines_splits_after_the_math_block() {
    let mut doc = load_markdown("$$\na\n$$\n", editor_options());
    let _ = doc.take_changes();
    let math = doc
        .text_leaves()
        .into_iter()
        .find(|&b| doc.kind(b) == Some(BlockKind::Math))
        .expect("math");
    let (changes, block, _) = doc.paste(math, 1..1, "x\n\ny", PasteIntent::PlainText);
    assert!(changes.is_structural());
    assert_eq!(doc.kind(math), Some(BlockKind::Math));
    assert_eq!(doc.text_of(math).unwrap(), "ax");
    assert_eq!(doc.kind(block), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(block).unwrap(), "y");

    let markdown = doc.to_markdown();
    let again = load_markdown(&markdown, editor_options());
    assert_eq!(
        again
            .preorder()
            .into_iter()
            .filter(|&id| again.arena.get(id).map(|n| n.kind) == Some(BlockKind::Math))
            .count(),
        1,
        "{markdown:?}"
    );
    assert_eq!(again.text_leaves().len(), 2, "{markdown:?}");
    assert_eq!(again.text_of(again.text_leaves()[0]).unwrap(), "ax");
    assert_eq!(again.text_of(again.text_leaves()[1]).unwrap(), "y");
    assert_eq!(again.to_markdown(), markdown);
}

#[test]
fn plain_paste_with_blank_lines_still_splits_phrasing_hosts() {
    let mut doc = load_markdown("# h\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = doc.text_of(leaf).unwrap().len();
    let (changes, block, _) = doc.paste(leaf, at..at, "\n\ntail", PasteIntent::PlainText);
    assert!(changes.is_structural());
    assert_eq!(doc.kind(leaf), Some(BlockKind::Heading(1)));
    assert_eq!(doc.text_of(leaf).unwrap(), "h");
    assert_eq!(doc.kind(block), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(block).unwrap(), "tail");
    let markdown = doc.to_markdown();
    let again = load_markdown(&markdown, editor_options());
    assert_eq!(again.text_leaves().len(), 2, "{markdown:?}");
    assert_eq!(
        again.kind(again.text_leaves()[0]),
        Some(BlockKind::Heading(1))
    );
    assert_eq!(
        again.kind(again.text_leaves()[1]),
        Some(BlockKind::Paragraph)
    );
    assert_eq!(again.text_of(again.text_leaves()[1]).unwrap(), "tail");
}

#[test]
fn pasted_paragraphs_inside_a_tight_item_keep_their_boundary() {
    for (text, intent, saved, kids) in [
        (
            "a\n\nb",
            PasteIntent::PlainText,
            "- itema\n  \n  b\n",
            vec![BlockKind::Paragraph, BlockKind::Paragraph],
        ),
        (
            "a\n\n---",
            PasteIntent::PlainText,
            "- itema\n  \n  ---\n",
            vec![BlockKind::Paragraph, BlockKind::ThematicBreak],
        ),
        (
            "a\n\nb",
            PasteIntent::IndependentFragment,
            "- item\n  \n  a\n  \n  b\n",
            vec![
                BlockKind::Paragraph,
                BlockKind::Paragraph,
                BlockKind::Paragraph,
            ],
        ),
    ] {
        let mut doc = load_markdown("- item\n", editor_options());
        let _ = doc.take_changes();
        let leaf = doc.text_leaves()[0];
        let at = doc.text_of(leaf).unwrap().len();
        let _ = doc.paste(leaf, at..at, text, intent);
        let markdown = doc.to_markdown();
        assert_eq!(markdown, saved, "paste={text:?}");

        let again = load_markdown(&markdown, editor_options());
        assert_eq!(again.to_markdown(), markdown, "paste={text:?}");
        let item = again
            .preorder()
            .into_iter()
            .find(|&id| again.arena.get(id).map(|n| n.kind) == Some(BlockKind::ListItem))
            .expect("item");
        let reloaded: Vec<BlockKind> = again
            .arena
            .children(item)
            .filter_map(|k| again.arena.get(k).map(|n| n.kind))
            .collect();
        assert_eq!(reloaded, kids, "paste={text:?} {markdown:?}");
        if text == "a\n\nb" {
            let texts: Vec<&str> = again
                .arena
                .children(item)
                .filter(|&k| again.arena.get(k).map(|n| n.kind) == Some(BlockKind::Paragraph))
                .filter_map(|k| again.text_of(k.index))
                .collect();
            let expected: Vec<&str> = if intent == PasteIntent::PlainText {
                vec!["itema", "b"]
            } else {
                vec!["item", "a", "b"]
            };
            assert_eq!(texts, expected, "paste={text:?}");
        }
    }
}

#[test]
fn trailing_blank_line_in_a_paste_follows_the_host() {
    let mut doc = load_markdown("$$\na\n$$\n", editor_options());
    let math = doc
        .text_leaves()
        .into_iter()
        .find(|&b| doc.kind(b) == Some(BlockKind::Math))
        .expect("math");
    let _ = doc.paste(math, 1..1, "x\n\n", PasteIntent::PlainText);
    assert_eq!(doc.text_of(math).unwrap(), "ax");
    let markdown = doc.to_markdown();
    let again = load_markdown(&markdown, editor_options());
    assert_eq!(
        again
            .preorder()
            .into_iter()
            .filter(|&id| again.arena.get(id).map(|n| n.kind) == Some(BlockKind::Math))
            .count(),
        1,
        "{markdown:?}"
    );
    assert_eq!(again.text_of(again.text_leaves()[0]).unwrap(), "ax");

    let mut doc = load_markdown("para\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = doc.text_of(leaf).unwrap().len();
    let _ = doc.paste(leaf, at..at, "a\n\n", PasteIntent::PlainText);
    assert_eq!(doc.text_of(leaf).unwrap(), "paraa");
    assert_eq!(doc.to_markdown(), "paraa\n");

    let mut doc = load_markdown("```\nc\n```\n", editor_options());
    let code = doc.text_leaves()[0];
    let _ = doc.paste(code, 1..1, "x\n\n", PasteIntent::PlainText);
    assert_eq!(doc.text_of(code).unwrap(), "cx\n\n");
    let markdown = doc.to_markdown();
    let again = load_markdown(&markdown, editor_options());
    assert_eq!(again.text_of(again.text_leaves()[0]).unwrap(), "cx\n\n");

    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let cell = doc
        .text_leaves()
        .into_iter()
        .find(|&b| doc.kind(b) == Some(BlockKind::TableCell))
        .expect("cell");
    let _ = doc.paste(cell, 1..1, "x\n\n", PasteIntent::PlainText);
    assert_eq!(doc.text_of(cell).unwrap(), "ax\n\n");
}

#[test]
fn crlf_paste_is_folded_before_it_reaches_the_leaf() {
    for intent in [PasteIntent::PlainText, PasteIntent::IndependentFragment] {
        let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
        let _ = doc.take_changes();
        let cell = doc
            .text_leaves()
            .into_iter()
            .find(|&b| doc.kind(b) == Some(BlockKind::TableCell))
            .expect("cell");
        let _ = doc.paste(cell, 1..1, "x\r\ny", intent);
        assert_eq!(doc.text_of(cell).unwrap(), "ax\ny", "{intent:?}");
        let markdown = doc.to_markdown();
        assert!(!markdown.contains('\r'), "{intent:?} {markdown:?}");
        let again = load_markdown(&markdown, editor_options());
        assert_eq!(
            again
                .preorder()
                .into_iter()
                .filter(|&id| again.arena.get(id).map(|n| n.kind) == Some(BlockKind::Table))
                .count(),
            1,
            "{intent:?} {markdown:?}"
        );
    }

    let mut doc = load_markdown("para\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = doc.text_of(leaf).unwrap().len();
    let _ = doc.paste(leaf, at..at, "a\rb", PasteIntent::PlainText);
    assert_eq!(doc.text_of(leaf).unwrap(), "paraa\nb");
    let markdown = doc.to_markdown();
    assert!(!markdown.contains('\r'), "{markdown:?}");
    let again = load_markdown(&markdown, editor_options());
    assert_eq!(again.text_of(again.text_leaves()[0]).unwrap(), "paraa\nb");
}

#[test]
fn cross_paragraph_cut_joins_and_undoes() {
    let mut d = Doc::new(load_markdown("alpha\n\nbeta\n", editor_options()));
    let leaves = d.text_leaves();
    let sel = Sel {
        anchor: Caret {
            block: leaves[0],
            offset: 2,
        },
        head: Caret {
            block: leaves[1],
            offset: 3,
        },
    };
    assert_eq!(d.copy_markdown(sel), "pha\n\nbet");
    let after = d.apply(sel, Command::DeleteBackward);
    assert_eq!(d.text_leaves().len(), 1);
    assert_eq!(d.text(after.block).unwrap_or(""), "ala");
    let _ = d.undo().expect("undo");
    assert_eq!(d.text_leaves().len(), 2);
    assert_eq!(d.text(d.text_leaves()[0]).unwrap_or(""), "alpha");
    assert_eq!(d.text(d.text_leaves()[1]).unwrap_or(""), "beta");
    let _ = d.redo().expect("redo");
    assert_eq!(d.text_leaves().len(), 1);
}

#[test]
fn paste_over_a_selection_replaces_it() {
    let mut d = Doc::new(load_markdown("hello\n", editor_options()));
    let leaf = d.text_leaves()[0];
    let sel = Sel {
        anchor: Caret {
            block: leaf,
            offset: 1,
        },
        head: Caret {
            block: leaf,
            offset: 4,
        },
    };
    let after = d.apply(sel, fragment("XY"));
    assert_eq!(d.text(leaf).unwrap_or(""), "hXYo");
    assert_eq!(after.offset, 3);
    let _ = d.undo().expect("undo");
    assert_eq!(d.text(leaf).unwrap_or(""), "hello");
    assert!(!d.can_undo());
    let _ = d.redo().expect("redo");
    assert_eq!(d.text(leaf).unwrap_or(""), "hXYo");
}

#[test]
fn paste_onto_root_or_container_is_a_graceful_noop() {
    let mut doc = load_markdown("> quote\n", editor_options());
    let _ = doc.take_changes();
    let quote = doc
        .preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::BlockQuote))
        .expect("quote container");
    let root = doc.root;
    let rev = doc.revision();
    let quote_kids = doc.arena.children(quote).count();

    let (changes, block, offset) =
        doc.paste(root.index, 0..0, "# hi\n", PasteIntent::IndependentFragment);
    assert!(changes.is_empty());
    assert_eq!((block, offset), (root.index, 0));
    let (changes, _, _) = doc.paste(root.index, 0..0, "text", PasteIntent::PlainText);
    assert!(changes.is_empty());

    let (changes, block, offset) = doc.paste(
        quote.index,
        0..0,
        "- item\n",
        PasteIntent::IndependentFragment,
    );
    assert!(changes.is_empty());
    assert_eq!((block, offset), (quote.index, 0));
    assert_eq!(doc.revision(), rev, "no revision may be issued");
    assert_eq!(
        doc.arena.children(quote).count(),
        quote_kids,
        "nothing grafted"
    );
    assert_eq!(kind_count(&doc, BlockKind::List), 0, "no list appears");
    assert_eq!(doc.text_leaves().len(), 1, "no phantom leaves");
}
