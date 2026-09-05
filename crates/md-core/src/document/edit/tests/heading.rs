use super::support::{caret, has_mark, type_chars};
use crate::block::BlockKind;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{editor_options, load_markdown};

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
