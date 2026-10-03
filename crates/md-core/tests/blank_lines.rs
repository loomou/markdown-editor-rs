use md_core::document::editor_options;
use pulldown_cmark::{NodeKind, Parsed};
use std::ops::Range;

fn spans(source: &str) -> Vec<Range<usize>> {
    Parsed::new(source, editor_options())
        .blank_lines()
        .map(|line| line.span)
        .collect()
}

fn want(pairs: &[(usize, usize)]) -> Vec<Range<usize>> {
    pairs.iter().map(|&(start, end)| start..end).collect()
}

fn owners(source: &str) -> Vec<&'static str> {
    let parsed = Parsed::new(source, editor_options());
    parsed
        .blank_lines()
        .map(|line| match line.container.map(|node| node.kind()) {
            None => "root",
            Some(NodeKind::BlockQuote(_)) => "quote",
            Some(NodeKind::ListItem(_)) => "item",
            Some(NodeKind::FootnoteDefinition(_)) => "footnote",
            Some(_) => "other",
        })
        .collect()
}

#[test]
fn a_blank_line_covers_the_whole_line_it_sits_on() {
    for (source, expected) in [
        ("para\n", want(&[])),
        ("\n", want(&[(0, 1)])),
        ("\n\n", want(&[(0, 1), (1, 2)])),
        ("a\n\nb\n", want(&[(2, 3)])),
        ("a\n\n", want(&[(2, 3)])),
        ("a\n\n\n", want(&[(2, 3), (3, 4)])),
        ("> hi\n> \n", want(&[(5, 8)])),
        ("> hi\n> \n> \n", want(&[(5, 8), (8, 11)])),
        ("> a\n> \n> b\n", want(&[(4, 7)])),
        ("- item\n  \n  x\n", want(&[(7, 10)])),
        ("- p\n  \n  \n  p\n", want(&[(4, 7), (7, 10)])),
        ("- a\n\n- b\n", want(&[(4, 5)])),
        ("```\ncode\n```\n\npara\n", want(&[(13, 14)])),
        ("```\ncode\n```\n\n\npara\n", want(&[(13, 14), (14, 15)])),
        ("    code\n\n\npara\n", want(&[(9, 10), (10, 11)])),
        (
            "---\ntitle: hi\n---\n\n\npara\n",
            want(&[(18, 19), (19, 20)]),
        ),
        ("[^1]: x\n\n\npara\n", want(&[(8, 9), (9, 10)])),
        (
            "| a | b |\n| --- | --- |\n| c | d |\n\npara\n",
            want(&[(34, 35)]),
        ),
    ] {
        assert_eq!(spans(source), expected, "{source:?}");
    }
}

#[test]
fn a_blank_line_names_the_container_it_sits_in() {
    for (source, expected) in [
        ("a\n\nb\n", vec!["root"]),
        ("> hi\n> \n", vec!["quote"]),
        ("> hi\n> \n> \n", vec!["quote", "quote"]),
        ("- item\n  \n  x\n", vec!["item"]),
        ("- a\n\n- b\n", vec!["item"]),
        ("[^1]: x\n\n\npara\n", vec!["footnote", "footnote"]),
    ] {
        assert_eq!(owners(source), expected, "{source:?}");
    }
}

#[test]
fn a_literal_block_keeps_its_blank_lines_to_itself() {
    for source in [
        "```\na\n\nb\n```\n",
        "```\na\n\n\nb\n```\n",
        "    a\n\n    b\n",
        "    a\n\n\n    b\n",
    ] {
        assert_eq!(spans(source), Vec::new(), "{source:?}");
    }
}

#[test]
fn the_reported_spans_are_ordered_and_disjoint() {
    for source in [
        "\n\n\n",
        "a\n\n\nb\n\n\nc\n",
        "> a\n> \n> \n> b\n",
        "- a\n  \n  \n- b\n",
        "```\nc\n```\n\n\n    code\n\n\np\n",
        "---\nt\n---\n\n\n[^1]: x\n\n\np\n",
    ] {
        let mut previous_end = 0;
        for span in spans(source) {
            assert!(span.start >= previous_end, "{source:?}: {span:?}");
            assert!(span.end <= source.len(), "{source:?}: {span:?}");
            previous_end = span.end;
        }
    }
}
