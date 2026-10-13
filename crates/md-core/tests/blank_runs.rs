use md_core::document::editor_options;
use pulldown_cmark::{BlankPosition, NodeKind, Parsed};
use std::ops::Range;

fn runs(source: &str) -> Vec<(BlankPosition, usize, usize)> {
    Parsed::new(source, editor_options())
        .blank_runs()
        .map(|run| (run.position, run.newlines, run.empty_paragraphs()))
        .collect()
}

fn spans(source: &str) -> Vec<Range<usize>> {
    Parsed::new(source, editor_options())
        .blank_runs()
        .map(|run| run.span)
        .collect()
}

fn owners(source: &str) -> Vec<&'static str> {
    Parsed::new(source, editor_options())
        .blank_runs()
        .map(|run| match run.container.map(|node| node.kind()) {
            None => "root",
            Some(NodeKind::BlockQuote(_)) => "quote",
            Some(NodeKind::ListItem(_)) => "item",
            Some(NodeKind::FootnoteDefinition(_)) => "footnote",
            Some(_) => "other",
        })
        .collect()
}

#[test]
fn a_run_before_the_content_folds_every_pair_of_newlines() {
    for (source, want) in [
        ("\nb", vec![(BlankPosition::Head, 1, 0)]),
        ("\n\nb", vec![(BlankPosition::Head, 2, 1)]),
        ("\n\n\nb", vec![(BlankPosition::Head, 3, 1)]),
        ("\n\n\n\nb", vec![(BlankPosition::Head, 4, 2)]),
        ("\n\n\n\n\nb", vec![(BlankPosition::Head, 5, 2)]),
    ] {
        assert_eq!(runs(source), want, "{source:?}");
    }
}

#[test]
fn a_run_after_the_content_folds_every_pair_of_newlines() {
    for (source, want) in [
        ("a\n", Vec::new()),
        ("a\n\n", vec![(BlankPosition::Tail, 2, 1)]),
        ("a\n\n\n", vec![(BlankPosition::Tail, 3, 1)]),
        ("a\n\n\n\n", vec![(BlankPosition::Tail, 4, 2)]),
        ("a\n\n\n\n\n", vec![(BlankPosition::Tail, 5, 2)]),
    ] {
        assert_eq!(runs(source), want, "{source:?}");
    }
}

#[test]
fn a_document_with_no_content_at_all_folds_like_an_edge_run() {
    for (source, want) in [
        ("", Vec::new()),
        ("\n", vec![(BlankPosition::Whole, 1, 0)]),
        ("\n\n", vec![(BlankPosition::Whole, 2, 1)]),
        ("\n\n\n", vec![(BlankPosition::Whole, 3, 1)]),
        ("\n\n\n\n", vec![(BlankPosition::Whole, 4, 2)]),
        ("\n\n\n\n\n\n", vec![(BlankPosition::Whole, 6, 3)]),
    ] {
        assert_eq!(runs(source), want, "{source:?}");
    }
}

#[test]
fn a_run_between_two_blocks_spends_one_pair_on_the_separator() {
    for (source, want) in [
        ("a\n\nb", vec![(BlankPosition::Between, 2, 0)]),
        ("a\n\n\nb", vec![(BlankPosition::Between, 3, 0)]),
        ("a\n\n\n\nb", vec![(BlankPosition::Between, 4, 1)]),
        ("a\n\n\n\n\nb", vec![(BlankPosition::Between, 5, 1)]),
        ("a\n\n\n\n\n\nb", vec![(BlankPosition::Between, 6, 2)]),
    ] {
        assert_eq!(runs(source), want, "{source:?}");
    }
}

#[test]
fn a_run_names_the_container_it_belongs_to() {
    for (source, want) in [
        ("a\n\nb", vec!["root"]),
        ("> hi\n> \n", vec!["quote"]),
        ("> \n> \n> a\n", vec!["quote"]),
        ("- a\n\n- b\n", vec!["item"]),
        ("    code\n\n\npara\n", vec!["root"]),
        ("[^1]: x\n\n\npara\n", vec!["footnote"]),
    ] {
        assert_eq!(owners(source), want, "{source:?}");
    }
}

#[test]
fn a_run_keeps_the_spans_of_the_blank_lines_it_groups() {
    let cases: [(&str, &[(usize, usize)]); 6] = [
        ("\n\n\n", &[(0, 3)]),
        ("a\n\n\n", &[(2, 4)]),
        ("a\n\n\nb", &[(2, 4)]),
        ("a\n\n\n\n\nb", &[(2, 6)]),
        ("a\n\n\nb\n\n\nc", &[(2, 4), (6, 8)]),
        ("> \n> \n> a\n", &[(0, 6)]),
    ];
    for (source, want) in cases {
        let got: Vec<(usize, usize)> = spans(source)
            .into_iter()
            .map(|span| (span.start, span.end))
            .collect();
        assert_eq!(got, want, "{source:?}");
    }
}

#[test]
fn a_soft_break_is_not_a_blank_run() {
    for source in ["a\nb", "a\n", "a", "> a\n> b\n"] {
        assert_eq!(spans(source), Vec::new(), "{source:?}");
    }
}

#[test]
fn every_run_is_one_of_the_four_positions_and_the_runs_do_not_overlap() {
    for source in [
        "\n\n\n",
        "a\n\n\nb\n\n\nc\n\n\n",
        "> \n> \n> a\n> \n> \n",
        "\n\na\n\n\n",
        "    code\n\n\npara\n\n\n",
    ] {
        let mut previous_end = 0;
        for run in Parsed::new(source, editor_options()).blank_runs() {
            assert!(run.span.start >= previous_end, "{source:?}: {:?}", run.span);
            assert!(run.span.end <= source.len(), "{source:?}: {:?}", run.span);
            assert!(
                run.empty_paragraphs() <= run.newlines,
                "{source:?}: {:?}",
                run.position
            );
            previous_end = run.span.end;
        }
    }
}
