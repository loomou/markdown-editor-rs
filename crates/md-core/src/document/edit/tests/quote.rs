use super::support::{caret, has_mark, type_chars};
use crate::block::BlockKind;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{editor_options, load_markdown};

fn quote_of(doc: &crate::document::Document) -> crate::document::NodeId {
    doc.preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::BlockQuote))
        .expect("quote")
}

#[test]
fn gt_stays_paragraph_until_quote_commit() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = type_chars(&mut doc, caret(leaf, 0), ">");
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaf).unwrap(), ">");
    assert!(
        doc.preorder()
            .into_iter()
            .all(|n| doc.arena.get(n).map(|n| n.kind) != Some(BlockKind::BlockQuote))
    );
}

#[test]
fn quote_marker_commits_in_place() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let at = type_chars(&mut doc, caret(leaf, 0), "> hi");
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaf).unwrap(), "hi");
    assert_eq!(doc.leaf_source(id), "hi");
    assert_eq!(
        doc.arena.get(id).and_then(|n| n.parent),
        Some(quote_of(&doc))
    );
    assert_eq!(at.block, leaf);
    assert_eq!(at.offset, 2);
}

#[test]
fn quote_commit_keeps_the_collapsed_map_fresh() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert {
            text: "> *a*".into(),
        },
    );
    assert_eq!(doc.text_of(leaf).unwrap(), "a");
    let id = doc.live_id(leaf).expect("live");
    let text = doc
        .arena
        .get(id)
        .and_then(|n| n.text)
        .and_then(|t| doc.texts.get(t))
        .expect("text");
    assert_eq!(
        text.s2d.len(),
        doc.leaf_source(id).len() + 1,
        "the s2d index domain should align with the leaf source"
    );
    assert_eq!(text.s2d.last().copied(), Some(doc.display(id).len()));
    assert_eq!(doc.leaf_source(id), "*a*");
    assert_eq!(doc.display(id), "a");
}

#[test]
fn quote_line_insert_commits_in_place() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let at = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert {
            text: "> hi".into(),
        },
    );
    assert_eq!(doc.text_of(leaf).unwrap(), "hi");
    assert_eq!(doc.leaf_source(id), "hi");
    assert_eq!(
        doc.arena.get(id).and_then(|n| n.parent),
        Some(quote_of(&doc))
    );
    assert_eq!(at.block, leaf);
    assert_eq!(at.offset, 2);
}

#[test]
fn nested_quote_marker_stays_paragraph() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = type_chars(&mut doc, caret(leaf, 0), ">> hi");
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaf).unwrap(), ">> hi");
    assert!(
        doc.preorder()
            .into_iter()
            .all(|n| doc.arena.get(n).map(|n| n.kind) != Some(BlockKind::BlockQuote))
    );
}

#[test]
fn quote_inline_code_still_projects() {
    use crate::inline::InlineMarks;
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert {
            text: "> hi".into(),
        },
    );
    let _ = type_chars(&mut doc, at, "`a`");
    assert_eq!(doc.text_of(leaf).unwrap(), "hi`a`");
    assert!(has_mark(&doc, leaf, InlineMarks::CODE));
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(
        doc.arena.get(id).and_then(|n| n.parent),
        Some(quote_of(&doc))
    );
}

#[test]
fn quote_in_list_item_wraps() {
    let mut doc = load_markdown("- x", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let item = doc.arena.get(id).and_then(|n| n.parent);
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert { text: "> ".into() },
    );
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaf).unwrap(), "x");
    assert_eq!(doc.leaf_source(id), "x");
    let quote = quote_of(&doc);
    assert_eq!(doc.arena.get(id).and_then(|n| n.parent), Some(quote));
    assert_eq!(doc.arena.get(quote).and_then(|n| n.parent), item);
}

#[test]
fn quote_inside_quote_nests() {
    let mut doc = load_markdown("> x", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let outer = quote_of(&doc);
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert { text: "> ".into() },
    );
    assert_eq!(doc.text_of(leaf).unwrap(), "x");
    let inner = doc.arena.get(id).and_then(|n| n.parent).expect("inner");
    assert_eq!(
        doc.arena.get(inner).map(|n| n.kind),
        Some(BlockKind::BlockQuote)
    );
    assert_eq!(doc.arena.get(inner).and_then(|n| n.parent), Some(outer));
    assert_eq!(
        doc.preorder()
            .into_iter()
            .filter(|&n| doc.arena.get(n).map(|n| n.kind) == Some(BlockKind::BlockQuote))
            .count(),
        2
    );
}

fn no_kind(doc: &crate::document::Document, kind: BlockKind) -> bool {
    doc.preorder()
        .into_iter()
        .all(|n| doc.arena.get(n).map(|n| n.kind) != Some(kind))
}

#[test]
fn quote_space_backspace_unwraps() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let host = doc
        .live_id(leaf)
        .and_then(|id| doc.arena.get(id).and_then(|n| n.parent));
    let at = type_chars(&mut doc, caret(leaf, 0), "> ");
    assert_eq!(doc.text_of(leaf).unwrap(), "");
    assert!(!no_kind(&doc, BlockKind::BlockQuote));
    let out = apply(&mut doc, Sel::collapsed(at), Command::DeleteBackward);
    assert_eq!(out, caret(leaf, 0));
    assert_eq!(doc.text_of(leaf).unwrap(), "");
    assert!(no_kind(&doc, BlockKind::BlockQuote));
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(doc.arena.get(id).and_then(|n| n.parent), host);
}

#[test]
fn quote_space_enter_unwraps() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let host = doc
        .live_id(leaf)
        .and_then(|id| doc.arena.get(id).and_then(|n| n.parent));
    let at = type_chars(&mut doc, caret(leaf, 0), "> ");
    let out = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_eq!(out, caret(leaf, 0));
    assert_eq!(doc.text_of(leaf).unwrap(), "");
    assert!(no_kind(&doc, BlockKind::BlockQuote));
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(doc.arena.get(id).and_then(|n| n.parent), host);
}

#[test]
fn quote_content_enter_then_empty_enter_exits() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = type_chars(&mut doc, caret(leaf, 0), "> hi");
    let empty = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_ne!(empty.block, leaf);
    let qid = doc.live_id(leaf).expect("live");
    assert_eq!(
        doc.arena
            .get(qid)
            .and_then(|n| n.parent)
            .and_then(|p| doc.arena.get(p).map(|n| n.kind)),
        Some(BlockKind::BlockQuote)
    );
    let out = apply(&mut doc, Sel::collapsed(empty), Command::Break);
    assert_eq!(out.offset, 0);
    assert_eq!(doc.text_of(out.block).unwrap(), "");
    let out_id = doc.live_id(out.block).expect("live");
    assert_ne!(
        doc.arena
            .get(out_id)
            .and_then(|n| n.parent)
            .and_then(|p| doc.arena.get(p).map(|n| n.kind)),
        Some(BlockKind::BlockQuote)
    );
    assert_eq!(
        doc.arena
            .get(qid)
            .and_then(|n| n.parent)
            .and_then(|p| doc.arena.get(p).map(|n| n.kind)),
        Some(BlockKind::BlockQuote)
    );
    assert_eq!(doc.text_of(leaf).unwrap(), "hi");
}

#[test]
fn footnote_last_empty_enter_exits() {
    let mut doc = load_markdown("hi[^1]\n\n[^1]: note\n", editor_options());
    let note = doc
        .preorder()
        .into_iter()
        .rev()
        .find(|&id| {
            doc.arena.get(id).is_some_and(|n| {
                n.kind.is_text_leaf()
                    && n.parent.is_some_and(|p| {
                        doc.arena.get(p).map(|pn| pn.kind) == Some(BlockKind::FootnoteDefinition)
                    })
            })
        })
        .expect("fn leaf");
    let n = doc.display(note).len();
    let _ = doc.replace_text(note.index, 0..n, "");
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(note.index, 0)),
        Command::Break,
    );
    let id = doc.live_id(out.block).expect("live");
    assert_ne!(
        doc.arena
            .get(id)
            .and_then(|n| n.parent)
            .and_then(|p| doc.arena.get(p).map(|n| n.kind)),
        Some(BlockKind::FootnoteDefinition)
    );
}

#[test]
fn quote_content_backspace_at_start_unwraps() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let host = doc
        .live_id(leaf)
        .and_then(|id| doc.arena.get(id).and_then(|n| n.parent));
    let _ = type_chars(&mut doc, caret(leaf, 0), "> hi");
    assert_eq!(doc.text_of(leaf).unwrap(), "hi");
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::DeleteBackward,
    );
    assert_eq!(out, caret(leaf, 0));
    assert_eq!(doc.text_of(leaf).unwrap(), "hi");
    assert!(no_kind(&doc, BlockKind::BlockQuote));
    let id = doc.live_id(leaf).expect("live");
    assert_eq!(doc.arena.get(id).and_then(|n| n.parent), host);
}

#[test]
fn quote_first_of_two_paragraphs_lifts_only_lead() {
    let mut doc = load_markdown("> a\n>\n> b\n", editor_options());
    let leaves = doc.text_leaves();
    assert!(leaves.len() >= 2);
    let a = leaves[0];
    let b = leaves[1];
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(a, 0)),
        Command::DeleteBackward,
    );
    assert_eq!(out, caret(a, 0));
    assert_eq!(doc.text_of(a).unwrap(), "a");
    let aid = doc.live_id(a).expect("a");
    let bid = doc.live_id(b).expect("b");
    assert_ne!(
        doc.arena
            .get(aid)
            .and_then(|n| n.parent)
            .and_then(|p| doc.arena.get(p).map(|n| n.kind)),
        Some(BlockKind::BlockQuote)
    );
    assert_eq!(
        doc.arena
            .get(bid)
            .and_then(|n| n.parent)
            .and_then(|p| doc.arena.get(p).map(|n| n.kind)),
        Some(BlockKind::BlockQuote)
    );
}

#[test]
fn select_all_delete_drops_fully_covered_quotes() {
    use crate::doc::Doc;
    use crate::document::edit::{Caret, Sel};
    for (label, src) in [
        ("alert with body", "> [!NOTE]\n> body\n"),
        ("plain quote", "> body\n"),
        ("nested quote", "> > deep\n"),
        ("quote after para", "para\n\n> quoted\n"),
        ("para after quote", "> quoted\n\npara\n"),
        ("two quotes", "> one\n\n> two\n"),
        ("alert with two paras", "> [!NOTE]\n> a\n>\n> b\n"),
        ("quote in list, all selected", "- > [!TIP]\n  > hi\n"),
        ("mixed list and quote", "- a\n\n> q\n\n- b\n"),
    ] {
        let mut doc = Doc::new(load_markdown(src, editor_options()));
        doc.enable_trailing_blank();
        let leaves = doc.text_leaves();
        let first = leaves[0];
        let last = *leaves.last().expect("last leaf");
        let end = doc.caret_text(last).unwrap_or("").len();
        let caret = doc.apply(
            Sel {
                anchor: Caret {
                    block: first,
                    offset: 0,
                },
                head: Caret {
                    block: last,
                    offset: end,
                },
            },
            Command::DeleteBackward,
        );
        assert_eq!(
            doc.document.to_markdown(),
            "",
            "{label}: markdown={:?}",
            doc.document.to_markdown()
        );
        assert!(
            doc.document.live_id(caret.block).is_some(),
            "{label}: caret on a dead leaf"
        );
    }
}

#[test]
fn partial_delete_keeps_the_unselected_alert() {
    let mut doc = load_markdown("- a\n\n> [!WARNING]\n> w\n", editor_options());
    let leaves = doc.text_leaves();
    let a = leaves[0];
    let _ = apply(
        &mut doc,
        Sel {
            anchor: caret(a, 0),
            head: caret(a, 1),
        },
        Command::DeleteBackward,
    );
    let md = doc.to_markdown();
    assert!(md.contains("[!WARNING]"), "unselected alert lost: {md:?}");
    assert!(md.contains("> w"), "unselected body lost: {md:?}");
}

#[test]
fn select_all_delete_undo_restores_the_alert_quote() {
    use crate::doc::Doc;
    use crate::document::edit::{Caret, Sel};
    let mut doc = Doc::new(load_markdown(
        "intro\n\n> [!NOTE]\n> body\n",
        editor_options(),
    ));
    doc.enable_trailing_blank();
    let leaves = doc.text_leaves();
    let first = leaves[0];
    let last = *leaves.last().expect("last");
    let end = doc.caret_text(last).unwrap_or("").len();
    let _ = doc.apply(
        Sel {
            anchor: Caret {
                block: first,
                offset: 0,
            },
            head: Caret {
                block: last,
                offset: end,
            },
        },
        Command::DeleteBackward,
    );
    assert_eq!(doc.document.to_markdown(), "");
    doc.undo();
    let md = doc.document.to_markdown();
    assert!(
        md.contains("[!NOTE]") && md.contains("body"),
        "undo did not restore the alert quote: {md:?}"
    );
}

#[test]
fn select_all_delete_undo_redo_round_trips_a_list_inside_a_quote() {
    use crate::doc::Doc;
    use crate::document::edit::{Caret, Sel};
    let mut doc = Doc::new(load_markdown("> 1. x\n", editor_options()));
    doc.enable_trailing_blank();
    let leaves = doc.text_leaves();
    let first = leaves[0];
    let last = *leaves.last().expect("last");
    let end = doc.caret_text(last).unwrap_or("").len();
    let _ = doc.apply(
        Sel {
            anchor: Caret {
                block: first,
                offset: 0,
            },
            head: Caret {
                block: last,
                offset: end,
            },
        },
        Command::DeleteBackward,
    );
    assert_eq!(doc.document.to_markdown(), "");
    assert!(doc.undo().is_some());
    assert_eq!(
        doc.document.to_markdown(),
        "> 1. x\n",
        "undo must put the leaf back inside the list item, not beside it"
    );
    assert!(doc.redo().is_some());
    assert_eq!(doc.document.to_markdown(), "");
}
