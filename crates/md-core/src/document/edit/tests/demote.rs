use super::support::{caret, fence_open, type_chars};
use crate::block::BlockKind;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{editor_options, load_markdown};

#[test]
fn empty_fence_backspace_demotes() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = fence_open(&mut doc, leaf, "```rust");
    let _ = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_eq!(doc.kind(leaf), Some(BlockKind::CodeBlock));
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::DeleteBackward,
    );
    assert_eq!(out, caret(leaf, 0));
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaf).unwrap(), "");
}

#[test]
fn empty_mermaid_backspace_demotes() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = fence_open(&mut doc, leaf, "```mermaid");
    let _ = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_eq!(doc.kind(leaf), Some(BlockKind::Mermaid));
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::DeleteBackward,
    );
    assert_eq!(out, caret(leaf, 0));
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
}

#[test]
fn empty_math_backspace_demotes() {
    let mut doc = load_markdown("$$x$$\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::DeleteForward,
    );
    assert_eq!(doc.kind(leaf), Some(BlockKind::Math));
    assert_eq!(doc.text_of(leaf), Some(""));

    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::DeleteBackward,
    );

    assert_eq!(out, caret(leaf, 0));
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaf), Some(""));
}

#[test]
fn thematic_break_backspace_demotes() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = type_chars(&mut doc, caret(leaf, 0), "---");
    let _ = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_eq!(doc.kind(leaf), Some(BlockKind::ThematicBreak));
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::DeleteBackward,
    );
    assert_eq!(out, caret(leaf, 0));
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaf).unwrap(), "");
}

#[test]
fn footnote_definition_first_backspace_lifts() {
    let mut doc = load_markdown("hi[^1]\n\n[^1]: note\n", editor_options());
    let note = doc
        .preorder()
        .into_iter()
        .find(|&id| {
            doc.arena.get(id).is_some_and(|n| {
                n.kind.is_text_leaf()
                    && n.parent.is_some_and(|p| {
                        doc.arena.get(p).map(|pn| pn.kind) == Some(BlockKind::FootnoteDefinition)
                    })
            })
        })
        .expect("fn leaf");
    let out = apply(
        &mut doc,
        Sel::collapsed(caret(note.index, 0)),
        Command::DeleteBackward,
    );
    assert_eq!(out.block, note.index);
    let id = doc.live_id(out.block).expect("live");
    assert_ne!(
        doc.arena
            .get(id)
            .and_then(|n| n.parent)
            .and_then(|p| doc.arena.get(p).map(|n| n.kind)),
        Some(BlockKind::FootnoteDefinition)
    );
}
