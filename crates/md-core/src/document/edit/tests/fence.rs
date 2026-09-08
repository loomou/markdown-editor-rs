use super::support::{caret, fence_open};
use crate::block::BlockKind;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{editor_options, load_markdown};

#[test]
fn fence_line_stays_paragraph_until_enter() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = fence_open(&mut doc, leaf, "```rust");
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaf).unwrap(), "```rust");
}

#[test]
fn rust_fence_enter_becomes_empty_code_block() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let at = fence_open(&mut doc, leaf, "```rust");
    let out = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_eq!(out, caret(leaf, 0));
    assert_eq!(doc.kind(leaf), Some(BlockKind::CodeBlock));
    assert_eq!(doc.text_of(leaf).unwrap(), "");
    assert_eq!(doc.live_id(leaf), Some(id));
    let lang = doc.extra(id).code_fence_lang().expect("lang");
    assert_eq!(doc.lang(lang), Some("rust"));
}

#[test]
fn mermaid_fence_enter_becomes_mermaid() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = fence_open(&mut doc, leaf, "```mermaid");
    let out = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_eq!(out.block, leaf);
    assert_eq!(doc.kind(leaf), Some(BlockKind::Mermaid));
    assert_eq!(doc.text_of(leaf).unwrap(), "");
}

#[test]
fn code_block_enter_inserts_newline() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = fence_open(&mut doc, leaf, "```rust");
    let _ = apply(&mut doc, Sel::collapsed(at), Command::Break);
    let mid = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert { text: "fn".into() },
    );
    let out = apply(&mut doc, Sel::collapsed(mid), Command::Break);
    assert_eq!(out, caret(leaf, 3));
    assert_eq!(doc.kind(leaf), Some(BlockKind::CodeBlock));
    assert_eq!(doc.text_of(leaf).unwrap(), "fn\n");
    assert_eq!(
        doc.preorder()
            .into_iter()
            .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::CodeBlock))
            .count(),
        1
    );
}

#[test]
fn closing_fence_enter_exits_to_paragraph() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let at = fence_open(&mut doc, leaf, "```rust");
    let _ = apply(&mut doc, Sel::collapsed(at), Command::Break);
    let mid = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert { text: "fn".into() },
    );
    let _ = apply(&mut doc, Sel::collapsed(mid), Command::Break);
    let close = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 3)),
        Command::Insert { text: "```".into() },
    );
    let out = apply(&mut doc, Sel::collapsed(close), Command::Break);
    assert_eq!(doc.kind(leaf), Some(BlockKind::CodeBlock));
    assert_eq!(doc.live_id(leaf), Some(id));
    assert_eq!(doc.text_of(leaf).unwrap(), "fn");
    assert_ne!(out.block, leaf);
    assert_eq!(out.offset, 0);
    assert_eq!(doc.kind(out.block), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(out.block).unwrap(), "");
    let parent = doc.arena.get(id).and_then(|n| n.parent).expect("parent");
    assert!(doc.arena.get(parent).is_some());
}

#[test]
fn math_breaks_insert_newlines_without_splitting_the_block() {
    for command in [Command::Break, Command::SoftBreak] {
        let mut doc = load_markdown("$$a+b$$\n", editor_options());
        let math = doc
            .text_leaves()
            .into_iter()
            .find(|&block| doc.kind(block) == Some(BlockKind::Math))
            .expect("math block");

        let out = apply(&mut doc, Sel::collapsed(caret(math, 1)), command.clone());

        assert_eq!(out, caret(math, 2));
        assert_eq!(doc.kind(math), Some(BlockKind::Math));
        assert_eq!(doc.text_of(math), Some("a\n+b"));
        assert_eq!(
            doc.preorder()
                .into_iter()
                .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::Math))
                .count(),
            1
        );
    }
}

#[test]
fn math_edits_reject_unserializable_dollar_delimiters() {
    let mut doc = load_markdown("$$a+b$$\n", editor_options());
    let math = doc
        .text_leaves()
        .into_iter()
        .find(|&block| doc.kind(block) == Some(BlockKind::Math))
        .expect("math block");
    let revision = doc.revision;

    let before_rejected = doc.revision;
    let rejected = apply(
        &mut doc,
        Sel::collapsed(caret(math, 1)),
        Command::Insert { text: "$".into() },
    );
    assert_eq!(rejected, caret(math, 1));
    assert_eq!(doc.revision, before_rejected);
    assert_eq!(doc.text_of(math), Some("a+b"));
    assert_eq!(doc.revision, revision);

    let markdown = doc.to_markdown();
    assert_eq!(markdown, "$$a+b$$\n");
    let again = load_markdown(&markdown, editor_options());
    assert_eq!(
        again
            .preorder()
            .into_iter()
            .filter(|&id| again.arena.get(id).map(|node| node.kind) == Some(BlockKind::Math))
            .count(),
        1,
        "{markdown:?}"
    );
    assert_eq!(again.text_of(again.text_leaves()[0]), Some("a+b"));

    let before_rejected = doc.revision;
    let rejected = apply(
        &mut doc,
        Sel::collapsed(caret(math, 1)),
        Command::Insert {
            text: "x$$y".into(),
        },
    );
    assert_eq!(rejected, caret(math, 1));
    assert_eq!(doc.revision, before_rejected);
    assert_eq!(doc.text_of(math), Some("a+b"));
}

#[test]
fn backspace_into_code_block_keeps_inline_markers_literal() {
    let mut doc = load_markdown("```\ncode\n```\n\n**bold**\n", editor_options());
    let code = doc
        .text_leaves()
        .into_iter()
        .find(|&block| doc.kind(block) == Some(BlockKind::CodeBlock))
        .expect("code block");
    let paragraph = doc
        .text_leaves()
        .into_iter()
        .find(|&block| doc.kind(block) == Some(BlockKind::Paragraph))
        .expect("paragraph");

    let out = apply(
        &mut doc,
        Sel::collapsed(caret(paragraph, 0)),
        Command::DeleteBackward,
    );

    assert_eq!(out, caret(code, "code".len()));
    let id = doc.live_id(code).expect("code remains live");
    assert_eq!(doc.display(id), "code**bold**");
    assert_eq!(doc.leaf_source(id), "code**bold**");
    assert_eq!(doc.to_markdown(), "```\ncode**bold**\n```\n");
}

#[test]
fn math_fence_line_stays_paragraph_until_enter() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = fence_open(&mut doc, leaf, "$$");
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaf).unwrap(), "$$");
}

#[test]
fn math_fence_enter_becomes_empty_math_block() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let id = doc.live_id(leaf).expect("live");
    let at = fence_open(&mut doc, leaf, "$$");

    let out = apply(&mut doc, Sel::collapsed(at), Command::Break);

    assert_eq!(out, caret(leaf, 0));
    assert_eq!(doc.kind(leaf), Some(BlockKind::Math));
    assert_eq!(doc.text_of(leaf).unwrap(), "");
    assert_eq!(doc.live_id(leaf), Some(id));
    assert_eq!(doc.text_leaves().len(), 1);
}

#[test]
fn math_fence_enter_then_typing_round_trips() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = fence_open(&mut doc, leaf, "$$");
    let out = apply(&mut doc, Sel::collapsed(at), Command::Break);

    let typed = apply(
        &mut doc,
        Sel::collapsed(out),
        Command::Insert { text: "a+b".into() },
    );

    assert_eq!(typed, caret(leaf, 3));
    assert_eq!(doc.text_of(leaf).unwrap(), "a+b");
    let markdown = doc.to_markdown();
    assert_eq!(markdown, "$$\na+b\n$$\n");
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
}

#[test]
fn math_fence_commit_saves_empty_block_as_fenced() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = fence_open(&mut doc, leaf, "$$");
    let _ = apply(&mut doc, Sel::collapsed(at), Command::Break);
    let markdown = doc.to_markdown();
    assert_eq!(markdown, "$$\n$$\n");
    let again = load_markdown(&markdown, editor_options());
    assert_eq!(again.to_markdown(), markdown);
    assert_eq!(again.text_of(again.text_leaves()[0]), Some(""));
}

#[test]
fn math_save_form_follows_the_source_form() {
    assert_eq!(
        load_markdown("$$a+b$$\n", editor_options()).to_markdown(),
        "$$a+b$$\n"
    );
    assert_eq!(
        load_markdown("$$\na+b\n$$\n", editor_options()).to_markdown(),
        "$$\na+b\n$$\n"
    );
    assert_eq!(
        load_markdown("$$$$\n", editor_options()).to_markdown(),
        "$$$$\n"
    );
    assert_eq!(
        load_markdown("$$\n$$\n", editor_options()).to_markdown(),
        "$$\n$$\n"
    );
}

#[test]
fn empty_math_block_backspaces_back_to_paragraph() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = fence_open(&mut doc, leaf, "$$");
    let out = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_eq!(doc.kind(leaf), Some(BlockKind::Math));

    let back = apply(&mut doc, Sel::collapsed(out), Command::DeleteBackward);

    assert_eq!(back, caret(leaf, 0));
    assert_eq!(doc.kind(leaf), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(leaf).unwrap(), "");
}

#[test]
fn math_fence_needs_the_whole_line() {
    for line in ["$", "$$x", "$$$", "x$$", "$ $"] {
        let mut doc = load_markdown("", editor_options());
        let leaf = doc.text_leaves()[0];
        let at = fence_open(&mut doc, leaf, line);
        let _ = apply(&mut doc, Sel::collapsed(at), Command::Break);
        assert_ne!(doc.kind(leaf), Some(BlockKind::Math), "{line:?}");
    }
}

#[test]
fn math_fence_skips_a_list_items_first_block() {
    let mut doc = load_markdown("- x\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let empty = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 1)),
        Command::DeleteBackward,
    );
    let at = apply(
        &mut doc,
        Sel::collapsed(empty),
        Command::Insert { text: "$$".into() },
    );

    let _ = apply(&mut doc, Sel::collapsed(at), Command::Break);

    assert_ne!(doc.kind(leaf), Some(BlockKind::Math));
    let markdown = doc.to_markdown();
    assert!(markdown.contains("$$"), "{markdown:?}");
}

#[test]
fn math_fence_commits_in_a_list_items_later_block() {
    let mut doc = load_markdown("- item\n\n  x\n", editor_options());
    let second = doc
        .text_leaves()
        .into_iter()
        .find(|&block| doc.text_of(block) == Some("x"))
        .expect("second block");
    let empty = apply(
        &mut doc,
        Sel::collapsed(caret(second, 1)),
        Command::DeleteBackward,
    );
    let at = apply(
        &mut doc,
        Sel::collapsed(empty),
        Command::Insert { text: "$$".into() },
    );

    let out = apply(&mut doc, Sel::collapsed(at), Command::Break);

    assert_eq!(doc.kind(out.block), Some(BlockKind::Math));
    let typed = apply(
        &mut doc,
        Sel::collapsed(out),
        Command::Insert { text: "a+b".into() },
    );
    assert_eq!(typed.offset, 3);
    assert_eq!(doc.text_of(out.block), Some("a+b"));
    let markdown = doc.to_markdown();
    let again = load_markdown(&markdown, editor_options());
    assert_eq!(
        again
            .preorder()
            .into_iter()
            .filter(|&id| again.arena.get(id).map(|node| node.kind) == Some(BlockKind::Math))
            .count(),
        1,
        "{markdown:?}"
    );
    assert_eq!(again.to_markdown(), markdown);
}

#[test]
fn enter_on_nonclosing_fence_content_keeps_that_content() {
    for source in ["```\n~~~\n```\n", "~~~~\n~~~\n~~~~\n", "    ~~~\n"] {
        let mut doc = load_markdown(source, editor_options());
        let block = doc.text_leaves()[0];
        assert_eq!(doc.kind(block), Some(BlockKind::CodeBlock), "{source:?}");
        assert_eq!(doc.collapsed_text_of(block), Some("~~~"), "{source:?}");
        let out = apply(&mut doc, Sel::collapsed(caret(block, 3)), Command::Break);
        assert_eq!(
            doc.collapsed_text_of(block),
            Some("~~~\n"),
            "content consumed: input={source:?}, markdown={:?}",
            doc.to_markdown()
        );
        assert_eq!(out.block, block, "caret left the block: {source:?}");
    }
}

#[test]
fn enter_inside_an_empty_list_code_block_stays_inside_the_code() {
    use crate::doc::Doc;
    let mut doc = Doc::new(load_markdown("- ```rust\n  ```\n", editor_options()));
    let block = doc.text_leaves()[0];
    assert_eq!(doc.kind(block), Some(BlockKind::CodeBlock));
    let _ = doc.apply(Sel::collapsed(caret(block, 0)), Command::Break);
    let id = doc.document.live_id(block).expect("block survives");
    let parent = doc.document.arena.get(id).unwrap().parent.unwrap();
    assert_eq!(doc.text(block), Some("\n"));
    assert_eq!(
        doc.document.arena.get(parent).map(|n| n.kind),
        Some(BlockKind::ListItem)
    );
}
