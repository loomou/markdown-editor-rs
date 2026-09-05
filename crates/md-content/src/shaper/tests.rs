use super::artifact::{locate_hard_offset, offset_on_hard_line};
use super::color::color_runs_by_line;

use crate::highlight::Span;
use md_theme::{DocumentTheme, SyntaxRole};

#[test]
fn color_runs_split_on_newline() {
    let text = "fn\n x";
    let spans = vec![
        Span {
            start: 0,
            end: 2,
            role: SyntaxRole::Keyword,
        },
        Span {
            start: 2,
            end: 3,
            role: SyntaxRole::Default,
        },
        Span {
            start: 3,
            end: 5,
            role: SyntaxRole::Default,
        },
    ];
    let syntax = DocumentTheme::formal().syntax;
    let runs = color_runs_by_line(text, &spans, syntax);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].iter().map(|r| r.0).sum::<u32>(), 2);
    assert_eq!(runs[1].iter().map(|r| r.0).sum::<u32>(), 2);
    assert_eq!(runs[0].len(), 1);
    assert_eq!(runs[1].len(), 1);
}

#[test]
fn color_runs_clamp_spans_to_each_line() {
    let text = "abc\ndef";
    let spans = vec![Span {
        start: 0,
        end: text.len(),
        role: SyntaxRole::Keyword,
    }];
    let syntax = DocumentTheme::formal().syntax;
    let runs = color_runs_by_line(text, &spans, syntax);
    assert_eq!(
        runs.iter()
            .map(|line| line.iter().map(|r| r.0).sum::<u32>())
            .collect::<Vec<_>>(),
        vec![3, 3]
    );
}

#[test]
fn hard_lines_map_line_ends_and_line_starts() {
    let one = [3usize];
    let one_starts = [0usize];
    assert_eq!(locate_hard_offset(&one_starts, &one, 3), (0, 3));
    assert_eq!(offset_on_hard_line(&one_starts, &one, 0, 3), 3);

    let trail = [3usize, 0];
    let trail_starts = [0usize, 4];
    assert_eq!(locate_hard_offset(&trail_starts, &trail, 3), (0, 3));
    assert_eq!(locate_hard_offset(&trail_starts, &trail, 4), (1, 0));
    assert_eq!(offset_on_hard_line(&trail_starts, &trail, 1, 0), 4);

    let mid = [3usize, 3];
    let mid_starts = [0usize, 4];
    assert_eq!(locate_hard_offset(&mid_starts, &mid, 3), (0, 3));
    assert_eq!(locate_hard_offset(&mid_starts, &mid, 4), (1, 0));
    assert_eq!(locate_hard_offset(&mid_starts, &mid, 5), (1, 1));
    assert_eq!(offset_on_hard_line(&mid_starts, &mid, 1, 0), 4);
    assert_eq!(offset_on_hard_line(&mid_starts, &mid, 1, 3), 7);

    let blank = [3usize, 0, 3];
    let blank_starts = [0usize, 4, 5];
    assert_eq!(locate_hard_offset(&blank_starts, &blank, 4), (1, 0));
    assert_eq!(locate_hard_offset(&blank_starts, &blank, 5), (2, 0));
    assert_eq!(offset_on_hard_line(&blank_starts, &blank, 2, 3), 8);
    assert_eq!(locate_hard_offset(&blank_starts, &blank, 8), (2, 3));
}

#[test]
fn hard_lines_map_leading_and_double_trailing() {
    let lead = [0usize, 3];
    let lead_starts = [0usize, 1];
    assert_eq!(locate_hard_offset(&lead_starts, &lead, 0), (0, 0));
    assert_eq!(locate_hard_offset(&lead_starts, &lead, 1), (1, 0));
    assert_eq!(offset_on_hard_line(&lead_starts, &lead, 1, 3), 4);

    let two_nl = [3usize, 0];
    let two_nl_starts = [0usize, 4];
    assert_eq!(offset_on_hard_line(&two_nl_starts, &two_nl, 1, 0), 4);
    assert_eq!(locate_hard_offset(&two_nl_starts, &two_nl, 4), (1, 0));
}

#[test]
fn image_source_text_rebuilds_alt_dest_and_title() {
    assert_eq!(
        super::atoms::image_source_text("cat", "./img/a.png", ""),
        "![cat](./img/a.png)"
    );
    assert_eq!(
        super::atoms::image_source_text("cat", "a.png", "tip"),
        "![cat](a.png \"tip\")"
    );
}

#[test]
fn image_source_text_treats_object_replacement_as_empty_alt() {
    assert_eq!(
        super::atoms::image_source_text("\u{FFFC}", "a.png", ""),
        "![](a.png)"
    );
}
