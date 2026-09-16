use crate::block::BlockKind;
use crate::document::{
    Document, editor_options, find_anchor, heading_anchors, load_markdown, slug,
};

fn doc_of(md: &str) -> Document {
    load_markdown(md, editor_options())
}

fn names(md: &str) -> Vec<String> {
    heading_anchors(&doc_of(md))
        .into_iter()
        .map(|a| a.slug)
        .collect()
}

#[test]
fn a_slug_lowercases_and_hyphenates() {
    assert_eq!(slug("This is a title"), "this-is-a-title");
    assert_eq!(slug("  Trimmed  "), "trimmed");
    assert_eq!(slug("UPPER"), "upper");
}

#[test]
fn a_slug_drops_punctuation_but_keeps_separators() {
    assert_eq!(slug("Hello, world!"), "hello-world");
    assert_eq!(slug("a.b/c"), "abc");
    assert_eq!(
        slug("snake_case and kebab-case"),
        "snake_case-and-kebab-case"
    );
    assert_eq!(slug("`code`"), "code");
    assert_eq!(slug(""), "");
}

#[test]
fn a_slug_keeps_ideographs() {
    assert_eq!(slug("中文标题"), "中文标题");
    assert_eq!(slug("标题 One"), "标题-one");
}

#[test]
fn a_single_heading_keeps_the_bare_slug() {
    assert_eq!(names("# Hello\n\n# Other\n"), ["hello", "other"]);
}

#[test]
fn duplicate_headings_are_numbered_from_one() {
    assert_eq!(
        names("# Hello\n\n# Hello\n\n# Hello\n"),
        ["hello-1", "hello-2", "hello-3"]
    );
}

#[test]
fn duplicate_headings_do_not_shift_their_neighbours() {
    assert_eq!(
        names("# Alpha\n\n# Beta\n\n# Alpha\n\n# Gamma\n"),
        ["alpha-1", "beta", "alpha-2", "gamma"]
    );
}

#[test]
fn a_nested_heading_is_reachable() {
    assert_eq!(names("> # Quoted\n"), ["quoted"]);
}

#[test]
fn a_document_without_headings_has_no_anchors() {
    assert!(heading_anchors(&doc_of("just a paragraph\n")).is_empty());
}

#[test]
fn an_anchor_resolves_to_the_right_heading() {
    let doc = doc_of("# One\n\ntext\n\n# Two\n\ntext\n\n# Three\n");
    let anchors = heading_anchors(&doc);
    assert_eq!(anchors.len(), 3);
    assert_eq!(find_anchor(&anchors, "three"), Some(anchors[2].block));
    assert_eq!(doc.kind(anchors[2].block), Some(BlockKind::Heading(1)));
}

#[test]
fn a_percent_encoded_anchor_resolves() {
    let doc = doc_of("# 中文标题\n");
    let anchors = heading_anchors(&doc);
    let block = Some(anchors[0].block);
    assert_eq!(anchors[0].slug, "中文标题");
    assert_eq!(find_anchor(&anchors, "中文标题"), block);
    assert_eq!(
        find_anchor(&anchors, "%E4%B8%AD%E6%96%87%E6%A0%87%E9%A2%98"),
        block
    );
}

#[test]
fn anchor_matching_ignores_case() {
    let doc = doc_of("# Hello\n");
    let anchors = heading_anchors(&doc);
    assert_eq!(find_anchor(&anchors, "HELLO"), Some(anchors[0].block));
}

#[test]
fn a_bare_anchor_falls_back_to_the_first_duplicate() {
    let doc = doc_of("# Hello\n\n# Hello\n");
    let anchors = heading_anchors(&doc);
    assert_eq!(find_anchor(&anchors, "hello"), Some(anchors[0].block));
    assert_eq!(find_anchor(&anchors, "hello-2"), Some(anchors[1].block));
}

#[test]
fn an_unknown_anchor_resolves_to_nothing() {
    let doc = doc_of("# Hello\n");
    let anchors = heading_anchors(&doc);
    assert_eq!(find_anchor(&anchors, ""), None);
    assert_eq!(find_anchor(&anchors, "   "), None);
    assert_eq!(find_anchor(&anchors, "nope"), None);
    assert_eq!(find_anchor(&anchors, "hello-9"), None);
}
