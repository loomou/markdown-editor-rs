use super::support::{caret, has_mark, type_chars};
use crate::block::BlockKind;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{editor_options, load_markdown};
use crate::inline::InlineMarks;

#[test]
fn hash_without_space_stays_paragraph() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let _ = type_chars(&mut doc, caret(leaf, 0), "#title");
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaf).unwrap(), "#title");
    assert_eq!(doc.leaf_source(id), "#title");
}

#[test]
fn hashes_stay_visible_until_atx_space() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = type_chars(&mut doc, caret(leaf, 0), "###");
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaf).unwrap(), "###");
}

#[test]
fn atx_space_commits_heading_in_place() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let at = type_chars(&mut doc, caret(leaf, 0), "### title");
    assert_eq!(doc.kind(leaf), Some(BlockKind::Heading(3)));
    assert_eq!(doc.text_of(leaf).unwrap(), "title");
    assert_eq!(doc.leaf_source(id), "### title");
    assert_eq!(at.block, leaf);
    assert_eq!(at.offset, 5);
}

#[test]
fn atx_line_insert_commits_heading_in_place() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let at = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert {
            text: "### title".into(),
        },
    );
    assert_eq!(doc.kind(leaf), Some(BlockKind::Heading(3)));
    assert_eq!(doc.text_of(leaf).unwrap(), "title");
    assert_eq!(doc.leaf_source(id), "### title");
    assert_eq!(at.block, leaf);
    assert_eq!(at.offset, 5);
}

#[test]
fn heading_keeps_typed_trailing_space() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let at = type_chars(&mut doc, caret(leaf, 0), "### title");
    let out = type_chars(&mut doc, at, " ");
    assert_eq!(doc.kind(leaf), Some(BlockKind::Heading(3)));
    assert_eq!(doc.text_of(leaf).unwrap(), "title ");
    assert_eq!(doc.leaf_source(id), "### title ");
    assert_eq!(out, caret(leaf, 6));
}

#[test]
fn paragraph_keeps_typed_trailing_space() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let at = type_chars(&mut doc, caret(leaf, 0), "hello");
    let out = type_chars(&mut doc, at, " ");
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaf).unwrap(), "hello ");
    assert_eq!(doc.leaf_source(id), "hello ");
    assert_eq!(out, caret(leaf, 6));
}

#[test]
fn paragraph_enter_splits() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let end = doc.text_of(leaf).unwrap().len();
    let out = apply(&mut doc, Sel::collapsed(caret(leaf, end)), Command::Break);
    assert_ne!(out.block, leaf);
    assert_eq!(out.offset, 0);
    assert_eq!(doc.text_of(leaf).unwrap(), "hello");
    assert_eq!(doc.text_of(out.block).unwrap(), "");
    assert_eq!(doc.kind(out.block), Some(BlockKind::Paragraph));
}

#[test]
fn heading_enter_still_splits() {
    let mut doc = load_markdown("# title\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let end = doc.text_of(leaf).unwrap().len();
    let out = apply(&mut doc, Sel::collapsed(caret(leaf, end)), Command::Break);
    assert_ne!(out.block, leaf);
    assert_eq!(out.offset, 0);
    assert_eq!(doc.kind(leaf), Some(BlockKind::Heading(1)));
    assert_eq!(doc.text_of(leaf).unwrap(), "title");
    assert_eq!(doc.kind(out.block), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(out.block).unwrap(), "");
}

#[test]
fn heading_backspace_at_start_demotes() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let _ = type_chars(&mut doc, caret(leaf, 0), "### title");
    assert_eq!(doc.kind(leaf), Some(BlockKind::Heading(3)));
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::DeleteBackward,
    );
    assert_eq!(out, caret(leaf, 0));
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaf).unwrap(), "title");
    assert_eq!(doc.leaf_source(id), "title");
}

#[test]
fn empty_heading_backspace_demotes() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let at = type_chars(&mut doc, caret(leaf, 0), "# ");
    assert_eq!(doc.kind(leaf), Some(BlockKind::Heading(1)));
    assert_eq!(doc.text_of(leaf).unwrap(), "");
    assert_eq!(at.offset, 0);
    let out = apply(&mut doc, Sel::collapsed(at), Command::DeleteBackward);
    assert_eq!(out, caret(leaf, 0));
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaf).unwrap(), "");
    assert_eq!(doc.leaf_source(id), "");
}

#[test]
fn typing_hash_after_heading_space_is_visible() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let at = type_chars(&mut doc, caret(leaf, 0), "# hello ");
    assert_eq!(doc.text_of(leaf).unwrap(), "hello ");
    let at = type_chars(&mut doc, at, "#");
    assert_eq!(doc.kind(leaf), Some(BlockKind::Heading(1)));
    assert_eq!(doc.text_of(leaf).unwrap(), "hello #");
    assert_eq!(doc.leaf_source(id), "# hello #");
    assert_eq!(at, caret(leaf, 7));
    let at = type_chars(&mut doc, at, "x");
    assert_eq!(doc.text_of(leaf).unwrap(), "hello #x");
    assert_eq!(doc.leaf_source(id), "# hello #x");
    assert_eq!(at, caret(leaf, 8));
}

#[test]
fn typing_hash_in_empty_heading_is_visible() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let at = type_chars(&mut doc, caret(leaf, 0), "# ");
    assert_eq!(doc.kind(leaf), Some(BlockKind::Heading(1)));
    let at = type_chars(&mut doc, at, "#");
    assert_eq!(doc.text_of(leaf).unwrap(), "#");
    assert_eq!(doc.leaf_source(id), "# #");
    assert_eq!(at, caret(leaf, 1));
    let at = type_chars(&mut doc, at, "x");
    assert_eq!(doc.text_of(leaf).unwrap(), "#x");
    assert_eq!(doc.leaf_source(id), "# #x");
    assert_eq!(at, caret(leaf, 2));
}

#[test]
fn heading_inline_code_still_projects() {
    use crate::inline::InlineMarks;
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert {
            text: "### title".into(),
        },
    );
    let _ = type_chars(&mut doc, at, "`a`");
    assert_eq!(doc.kind(leaf), Some(BlockKind::Heading(3)));
    assert_eq!(doc.text_of(leaf).unwrap(), "title`a`");
    assert!(has_mark(&doc, leaf, InlineMarks::CODE));
}

#[test]
fn atx_in_list_item_becomes_heading() {
    let mut doc = load_markdown("- x", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert {
            text: "### ".into(),
        },
    );
    assert_eq!(doc.kind(leaf), Some(BlockKind::Heading(3)));
    assert_eq!(doc.text_of(leaf).unwrap(), "x");
    assert_eq!(doc.leaf_source(id), "### x");
    assert_eq!(
        doc.arena
            .get(id)
            .and_then(|n| n.parent)
            .and_then(|p| doc.arena.get(p).map(|n| n.kind)),
        Some(BlockKind::ListItem)
    );
}

#[test]
fn demoted_setext_heading_stays_a_paragraph_after_save() {
    for (source, body) in [("Title\n=====\n", "Title"), ("Sub\n---\n", "Sub")] {
        let mut doc = load_markdown(source, editor_options());
        let leaf = doc.text_leaves()[0];
        let _ = apply(
            &mut doc,
            Sel::collapsed(caret(leaf, 0)),
            Command::DeleteBackward,
        );
        assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
        let markdown = doc.to_markdown();
        assert!(markdown.contains(body), "{markdown:?}");
        assert!(
            !markdown.contains('=') && !markdown.contains("---"),
            "{markdown:?}"
        );
        let reloaded = load_markdown(&markdown, editor_options());
        assert_eq!(
            reloaded.kind(reloaded.text_leaves()[0]),
            Some(BlockKind::Paragraph),
            "reloaded back to a heading: {markdown:?}"
        );
    }
}

#[test]
fn demoted_multiline_setext_heading_stays_a_paragraph_after_save() {
    let mut doc = load_markdown("hello\nworld\n---\n", editor_options());
    let leaf = doc.text_leaves()[0];
    assert_eq!(doc.kind(leaf), Some(BlockKind::Heading(2)));
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::DeleteBackward,
    );
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    let markdown = doc.to_markdown();
    assert!(
        !markdown.contains("---"),
        "separator survived: {markdown:?}"
    );
    let reloaded = load_markdown(&markdown, editor_options());
    assert_eq!(
        reloaded.kind(reloaded.text_leaves()[0]),
        Some(BlockKind::Paragraph),
        "reloaded back to a heading: {markdown:?}"
    );
    assert_eq!(
        reloaded.text_of(reloaded.text_leaves()[0]),
        Some(
            "hello
world"
        )
    );
}

#[test]
fn heading_indent_keeps_inline_run_on_its_text() {
    let mut doc = load_markdown("# z **ab**\n\nafter\n", editor_options());
    let leaves = doc.text_leaves();
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaves[0], 0)),
        Command::Indent,
    );
    doc.retarget_inline_focus(caret(leaves[1], 0));
    let id = doc.live_id(leaves[0]).expect("live");
    let snapshot = doc.leaf_snapshot(id).expect("snapshot");
    assert_eq!(snapshot.display, "\tz ab");
    let strong_text: String = snapshot
        .runs
        .iter()
        .filter(|run| run.marks.contains(InlineMarks::STRONG))
        .filter_map(|run| {
            snapshot
                .display
                .get(run.display_range.start as usize..run.display_range.end as usize)
        })
        .collect();
    assert_eq!(strong_text, "ab", "runs={:?}", snapshot.runs);
}

#[test]
fn merging_a_paragraph_into_a_setext_heading_keeps_heading_text() {
    let mut doc = load_markdown("head\n====\n\ntail\n", editor_options());
    let leaves = doc.text_leaves();
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaves[1], 0)),
        Command::DeleteBackward,
    );
    assert_eq!(doc.text_of(leaves[0]), Some("headtail"));
    let reloaded = load_markdown(&doc.to_markdown(), editor_options());
    assert_eq!(
        reloaded.kind(reloaded.text_leaves()[0]),
        Some(BlockKind::Heading(1))
    );
}

#[test]
fn merging_into_a_whitespace_padded_setext_heading_keeps_heading_text() {
    let mut doc = load_markdown("head\n===  \n\ntail\n", editor_options());
    let leaves = doc.text_leaves();
    assert_eq!(doc.kind(leaves[0]), Some(BlockKind::Heading(1)));
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaves[1], 0)),
        Command::DeleteBackward,
    );
    let reloaded = load_markdown(&doc.to_markdown(), editor_options());
    assert_eq!(
        reloaded.kind(reloaded.text_leaves()[0]),
        Some(BlockKind::Heading(1))
    );
    assert_eq!(
        reloaded.text_of(reloaded.text_leaves()[0]),
        Some("headtail")
    );
}

#[test]
fn joining_a_list_paragraph_to_a_setext_heading_preserves_heading_text() {
    use crate::doc::Doc;
    let mut doc = Doc::new(load_markdown(
        "- head\n  ====\n\n  tail\n",
        editor_options(),
    ));
    let leaves = doc.text_leaves();
    assert_eq!(leaves.len(), 2);
    assert_eq!(doc.kind(leaves[0]), Some(BlockKind::Heading(1)));
    assert_eq!(doc.collapsed_text(leaves[1]), Some("tail"));
    let _ = doc.apply(
        Sel::collapsed(crate::document::Caret {
            block: leaves[1],
            offset: 0,
        }),
        Command::DeleteBackward,
    );
    let reloaded = Doc::new(load_markdown(&doc.document.to_markdown(), editor_options()));
    assert_eq!(
        reloaded.kind(reloaded.text_leaves()[0]),
        Some(BlockKind::Heading(1))
    );
    assert_eq!(
        reloaded.collapsed_text(reloaded.text_leaves()[0]),
        Some("headtail")
    );
}

#[test]
fn tab_delimited_atx_heading_survives_first_save() {
    use crate::doc::Doc;
    let doc = Doc::new(load_markdown("#\theading\n", editor_options()));
    let leaf = doc.text_leaves()[0];
    assert_eq!(doc.kind(leaf), Some(BlockKind::Heading(1)));
    let saved = doc.document.to_markdown();
    let reloaded = Doc::new(load_markdown(&saved, editor_options()));
    assert_eq!(
        reloaded.collapsed_text(reloaded.text_leaves()[0]),
        doc.collapsed_text(leaf)
    );
}

#[test]
fn splitting_a_setext_heading_makes_a_paragraph_tail() {
    let mut doc = load_markdown("heading\n====\n", editor_options());
    let heading = doc.text_leaves()[0];
    let caret = apply(&mut doc, Sel::collapsed(caret(heading, 4)), Command::Break);
    assert_eq!(doc.text_of(caret.block), Some("ing"));
    let reloaded = load_markdown(&doc.to_markdown(), editor_options());
    assert_eq!(
        reloaded.kind(reloaded.text_leaves()[1]),
        Some(BlockKind::Paragraph)
    );
}

#[test]
fn promoting_a_heading_keeps_host_reference_links() {
    use crate::doc::Doc;
    let mut doc = Doc::new(load_markdown(
        "prefix [label][r]\n\n[r]: /target\n",
        editor_options(),
    ));
    let leaf = doc.text_leaves()[0];
    assert_eq!(
        doc.link_at(crate::document::Caret {
            block: leaf,
            offset: 8
        }),
        Some("/target")
    );
    let _ = doc.apply(
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert { text: "# ".into() },
    );
    assert_eq!(doc.kind(leaf), Some(BlockKind::Heading(1)));
    assert_eq!(doc.collapsed_text(leaf), Some("prefix label"));
    assert_eq!(
        doc.link_at(crate::document::Caret {
            block: leaf,
            offset: 8
        }),
        Some("/target")
    );
}

#[test]
fn promoting_a_quote_keeps_host_reference_links() {
    use crate::doc::Doc;
    let mut doc = Doc::new(load_markdown(
        "prefix [label][r]\n\n[r]: /target\n",
        editor_options(),
    ));
    let leaf = doc.text_leaves()[0];
    let _ = doc.apply(
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert { text: "> ".into() },
    );
    assert_eq!(doc.collapsed_text(leaf), Some("prefix label"));
    assert_eq!(
        doc.link_at(crate::document::Caret {
            block: leaf,
            offset: 8
        }),
        Some("/target")
    );
}
