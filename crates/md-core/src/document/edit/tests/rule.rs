use super::support::{caret, type_chars};
use crate::block::BlockKind;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{editor_options, load_markdown};

#[test]
fn rule_line_stays_paragraph_until_enter() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = type_chars(&mut doc, caret(leaf, 0), "---");
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaf).unwrap(), "---");
}

#[test]
fn rule_enter_becomes_thematic_break() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let nodes = doc.preorder().len();
    let at = type_chars(&mut doc, caret(leaf, 0), "---");
    let out = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_ne!(out.block, leaf);
    assert_eq!(out.offset, 0);
    assert_eq!(doc.kind(leaf), Some(BlockKind::ThematicBreak));
    assert_eq!(doc.live_id(leaf), Some(id));
    assert_eq!(doc.text_of(leaf).unwrap(), "");
    assert_eq!(doc.preorder().len(), nodes + 1);
    assert_eq!(doc.text_leaves(), vec![out.block]);
    assert_eq!(
        doc.preorder()
            .into_iter()
            .filter(|&n| doc.arena.get(n).map(|n| n.kind) == Some(BlockKind::Paragraph))
            .count(),
        1
    );
}

#[test]
fn star_and_underscore_rules_become_thematic_break() {
    for line in ["***", "___"] {
        let mut doc = load_markdown("", editor_options());
        let leaf = doc.text_leaves()[0];
        let at = apply(
            &mut doc,
            Sel::collapsed(caret(leaf, 0)),
            Command::Insert { text: line.into() },
        );
        let out = apply(&mut doc, Sel::collapsed(at), Command::Break);
        assert_eq!(doc.kind(leaf), Some(BlockKind::ThematicBreak), "{line}");
        assert_eq!(doc.text_leaves(), vec![out.block], "{line}");
    }
}

#[test]
fn typing_after_a_rule_stays_after_the_rule() {
    let mut doc = load_markdown("intro\n\nscratch\n", editor_options());
    let intro = doc.text_leaves()[0];
    let line = doc.text_leaves()[1];
    let _ = doc.replace_text(line, 0.."scratch".len(), "---");
    let after_rule = apply(&mut doc, Sel::collapsed(caret(line, 3)), Command::Break);
    assert_eq!(doc.kind(line), Some(BlockKind::ThematicBreak));

    let out = apply(
        &mut doc,
        Sel::collapsed(after_rule),
        Command::Insert { text: "x".into() },
    );

    assert_eq!(doc.text_of(intro), Some("intro"));
    assert_eq!(out.block, after_rule.block);
    assert_eq!(doc.text_of(out.block), Some("x"));
}

#[test]
fn line_breaks_do_not_clone_an_existing_rule() {
    for command in [Command::Break, Command::SoftBreak] {
        let mut doc = load_markdown("before\n\n---\n\nafter\n", editor_options());
        let rule = doc
            .preorder()
            .into_iter()
            .find(|&id| doc.arena.get(id).map(|node| node.kind) == Some(BlockKind::ThematicBreak))
            .expect("rule");
        let at = caret(rule.index, 0);

        let out = apply(&mut doc, Sel::collapsed(at), command.clone());

        assert_eq!(out, at);
        assert_eq!(
            doc.preorder()
                .into_iter()
                .filter(
                    |&id| doc.arena.get(id).map(|node| node.kind) == Some(BlockKind::ThematicBreak)
                )
                .count(),
            1
        );
        assert_eq!(doc.to_markdown(), "before\n\n---\n\nafter\n");
    }
}

#[test]
fn backspace_after_a_rule_deletes_the_rule_without_merging_text() {
    let mut doc = load_markdown("a\n\n---\n\nb\n", editor_options());
    let rule = doc
        .preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|node| node.kind) == Some(BlockKind::ThematicBreak))
        .expect("rule");
    let b = doc
        .text_leaves()
        .into_iter()
        .find(|&block| doc.text_of(block) == Some("b"))
        .expect("following paragraph");

    let out = apply(
        &mut doc,
        Sel::collapsed(caret(b, 0)),
        Command::DeleteBackward,
    );

    assert_eq!(out, caret(b, 0));
    assert!(doc.arena.get(rule).is_none());
    assert_eq!(doc.text_of(b), Some("b"));
    assert_eq!(doc.to_markdown(), "a\n\nb\n");
}

#[test]
fn rule_in_list_item_does_not_become_break() {
    let mut doc = load_markdown("- x", editor_options());
    let leaf = doc.text_leaves()[0];
    let n = doc.text_of(leaf).unwrap().len();
    let _ = doc.replace_text(leaf, 0..n, "---");
    let _ = apply(&mut doc, Sel::collapsed(caret(leaf, 3)), Command::Break);
    assert_ne!(doc.kind(leaf), Some(BlockKind::ThematicBreak));
    assert!(
        doc.preorder()
            .into_iter()
            .all(|n| doc.arena.get(n).map(|n| n.kind) != Some(BlockKind::ThematicBreak))
    );
    assert!(
        doc.preorder()
            .into_iter()
            .any(|n| doc.arena.get(n).map(|n| n.kind) == Some(BlockKind::List))
    );
}
