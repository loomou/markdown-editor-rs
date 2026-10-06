use super::support::{caret, type_chars};
use crate::block::BlockKind;
use crate::doc::Doc;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{editor_options, load_markdown};

#[test]
fn empty_front_matter_backspaces_back_to_paragraph() {
    let mut doc = Doc::new(load_markdown("", editor_options()));
    doc.enable_trailing_blank();
    let leaf = doc.text_leaves()[0];
    let at = type_chars(&mut doc.document, caret(leaf, 0), "---");
    let out = doc.apply(Sel::collapsed(at), Command::Break);
    assert_eq!(doc.kind(leaf), Some(BlockKind::MetadataBlock));

    let back = doc.apply(Sel::collapsed(out), Command::DeleteBackward);

    assert_eq!(back, caret(leaf, 0));
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text(leaf).unwrap(), "");
    assert_eq!(doc.document.to_markdown(), "");
}

#[test]
fn a_reloaded_empty_front_matter_backspaces_back_to_paragraph() {
    let mut doc = Doc::new(load_markdown("---\n\n---\n", editor_options()));
    doc.enable_trailing_blank();
    let leaf = doc.text_leaves()[0];
    assert_eq!(doc.kind(leaf), Some(BlockKind::MetadataBlock));

    let back = doc.apply(Sel::collapsed(caret(leaf, 0)), Command::DeleteBackward);

    assert_eq!(back, caret(leaf, 0));
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text(leaf).unwrap(), "");
    assert_eq!(doc.document.to_markdown(), "");
}

#[test]
fn a_front_matter_with_a_body_keeps_its_fences_on_backspace() {
    let mut doc = load_markdown("---\ntitle: hi\n---\n", editor_options());
    let leaf = doc.text_leaves()[0];
    assert_eq!(doc.kind(leaf), Some(BlockKind::MetadataBlock));

    let back = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::DeleteBackward,
    );

    assert_eq!(back, caret(leaf, 0));
    assert_eq!(doc.kind(leaf), Some(BlockKind::MetadataBlock));
    assert_eq!(doc.text_of(leaf).unwrap(), "title: hi");
    assert_eq!(doc.to_markdown(), "---\ntitle: hi\n---");
}
