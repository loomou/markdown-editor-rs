use super::support::caret;
use crate::document::edit::{Command, Sel, apply};
use crate::document::{
    Document, editor_options, floor_char_boundary, load_markdown, next_char_boundary,
    prev_char_boundary, word_span,
};
use unicode_segmentation::UnicodeSegmentation;

fn doc_with(text: &str) -> (Document, u32) {
    let doc = load_markdown(&format!("{text}\n"), editor_options());
    let leaf = doc.text_leaves()[0];
    (doc, leaf)
}

fn delete_backward(doc: &mut Document, block: u32, offset: usize) {
    let _ = apply(
        doc,
        Sel::collapsed(caret(block, offset)),
        Command::DeleteBackward,
    );
}

fn text_of(doc: &Document, leaf: u32) -> &str {
    doc.text_of(leaf).expect("live text")
}

#[test]
fn backspace_deletes_the_combining_cluster_whole() {
    let (mut doc, leaf) = doc_with("ae\u{301}b");
    let len = "ae\u{301}b".len();

    delete_backward(&mut doc, leaf, len);
    assert_eq!(text_of(&doc, leaf), "ae\u{301}");

    delete_backward(&mut doc, leaf, len - 1);
    assert_eq!(text_of(&doc, leaf), "a");
}

#[test]
fn backspace_deletes_the_zwj_family_whole() {
    let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
    let (mut doc, leaf) = doc_with(family);
    delete_backward(&mut doc, leaf, family.len());
    assert_eq!(text_of(&doc, leaf), "");
}

#[test]
fn backspace_deletes_the_flag_whole() {
    let flag = "\u{1f1e8}\u{1f1f3}";
    let (mut doc, leaf) = doc_with(flag);
    delete_backward(&mut doc, leaf, flag.len());
    assert_eq!(text_of(&doc, leaf), "");
}

#[test]
fn delete_forward_eats_the_cluster_whole() {
    let (mut doc, leaf) = doc_with("ae\u{301}b");
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 1)),
        Command::DeleteForward,
    );
    assert_eq!(text_of(&doc, leaf), "ab");
}

#[test]
fn selection_delete_respects_the_range_edges() {
    let (mut doc, leaf) = doc_with("ae\u{301}b");
    let _ = apply(
        &mut doc,
        Sel {
            anchor: caret(leaf, 1),
            head: caret(leaf, 4),
        },
        Command::DeleteBackward,
    );
    assert_eq!(text_of(&doc, leaf), "ab");
}

#[test]
fn backspace_in_a_code_block_deletes_the_cluster_whole() {
    let doc = load_markdown("```\ne\u{301}\n```\n", editor_options());
    let block = doc
        .text_leaves()
        .into_iter()
        .find(|&id| doc.kind(id) == Some(crate::block::BlockKind::CodeBlock))
        .expect("code block");
    let mut doc = doc;
    let len = "e\u{301}".len();
    delete_backward(&mut doc, block, len);
    assert_eq!(doc.text_of(block).unwrap(), "");
}

#[test]
fn word_span_never_cuts_a_cluster() {
    let text = "é🇨🇳 wor\u{301}d";
    let edges: Vec<usize> = text
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();
    for offset in 0..=text.len() {
        if let Some((lo, hi)) = word_span(text, offset) {
            assert!(
                edges.contains(&lo),
                "word_span start {lo} from {offset} cuts a cluster"
            );
            assert!(
                edges.contains(&hi),
                "word_span end {hi} from {offset} cuts a cluster"
            );
        }
    }
}

#[test]
fn char_boundary_helpers_clamp_arbitrary_byte_offsets() {
    let text = "a€b";
    assert_eq!(floor_char_boundary(text, 0), 0);
    assert_eq!(
        floor_char_boundary(text, 2),
        1,
        "a byte inside the middle char snaps back to its start"
    );
    assert_eq!(floor_char_boundary(text, 3), 1, "same as above");
    assert_eq!(floor_char_boundary(text, 4), 4);
    assert_eq!(
        floor_char_boundary(text, usize::MAX),
        5,
        "out of range clamps back to len"
    );

    assert_eq!(prev_char_boundary(text, 0), 0);
    assert_eq!(
        prev_char_boundary(text, 2),
        1,
        "a byte inside the middle char steps back to its start"
    );
    assert_eq!(
        prev_char_boundary(text, 4),
        1,
        "one step back from before b clears the whole middle char (the previous char's start)"
    );
    assert_eq!(prev_char_boundary(text, 5), 4);
    assert_eq!(
        prev_char_boundary(text, 999),
        5,
        "out of range starts from len and converges"
    );

    assert_eq!(next_char_boundary(text, 0), 1);
    assert_eq!(
        next_char_boundary(text, 1),
        4,
        "a byte inside the middle char advances to its end"
    );
    assert_eq!(next_char_boundary(text, 3), 4);
    assert_eq!(next_char_boundary(text, 5), 5, "endpoints do not move");
}
