use md_core::block::{
    AlertKind, BlockKind, CodeFenceMarker, ListMarker, NodeExtra, TableCellAlign,
};
use md_core::doc::{Cursor, Doc};
use md_core::document::{editor_options, load_markdown};
use md_core::inline::{InlineAlign, InlineMarks, InlineRun, covering_runs};

#[test]
fn block_and_extra_helpers_cover_all_public_variants() {
    assert!(BlockKind::Paragraph.is_text_leaf());
    assert!(!BlockKind::List.is_text_leaf());
    assert!(BlockKind::List.is_vertical_container());
    assert!(!BlockKind::Heading(1).is_vertical_container());

    assert_eq!(TableCellAlign::from(0), TableCellAlign::Start);
    assert_eq!(TableCellAlign::from(2), TableCellAlign::Center);
    assert_eq!(TableCellAlign::from(3), TableCellAlign::End);
    assert_eq!(TableCellAlign::from(0xff), TableCellAlign::End);
    assert_eq!(AlertKind::Warning.label(), "WARNING");

    let cell = NodeExtra::Cell {
        align: TableCellAlign::Center,
        header: true,
    };
    assert_eq!(cell.inline_align(), InlineAlign::Center);
    assert_eq!(
        NodeExtra::List {
            start: Some(7),
            marker: ListMarker::Parenthesis,
            loose: true,
            source_loose: true,
        }
        .ordered_start(),
        Some(7)
    );
    assert!(
        NodeExtra::QuoteAlert {
            kind: AlertKind::Tip,
            lowercase_mask: 0,
            blank_after_marker: false,
        }
        .quote_alert()
            == Some(AlertKind::Tip)
    );
    assert_eq!(
        NodeExtra::CodeFence {
            lang: None,
            marker: CodeFenceMarker::Tilde,
            len: 4,
        }
        .code_fence_lang(),
        None
    );
}

#[test]
fn covering_runs_fills_gaps_and_clips_out_of_bounds_ranges() {
    let runs = covering_runs(
        10,
        &[
            InlineRun {
                display_range: 2..5,
                source_range: Some(2..5),
                marks: InlineMarks::EM,
                link: Some(1),
            },
            InlineRun {
                display_range: 8..20,
                source_range: None,
                marks: InlineMarks::STRONG,
                link: None,
            },
        ],
    );
    assert_eq!(runs.len(), 4);
    assert_eq!(runs[0].display_range, 0..2);
    assert_eq!(runs[1].display_range, 2..5);
    assert_eq!(runs[2].display_range, 5..8);
    assert_eq!(runs[3].display_range, 8..10);
    assert!(runs[0].marks.is_empty());
    assert!(runs[3].marks.contains(InlineMarks::STRONG));
}

#[test]
fn doc_link_lookup_handles_link_boundaries_and_unknown_blocks() {
    let doc = Doc::new(load_markdown(
        "before [link](https://example.test) after\n",
        editor_options(),
    ));
    let leaf = doc.text_leaves()[0];
    assert_eq!(
        doc.link_at(Cursor {
            block: leaf,
            offset: 7
        }),
        Some("https://example.test")
    );
    assert_eq!(
        doc.link_at(Cursor {
            block: leaf,
            offset: 10
        }),
        Some("https://example.test")
    );
    assert_eq!(
        doc.link_at(Cursor {
            block: leaf,
            offset: 11
        }),
        None
    );
    assert_eq!(
        doc.link_at(Cursor {
            block: leaf,
            offset: 0
        }),
        None
    );
    assert_eq!(doc.kind(u32::MAX), None);
    assert_eq!(doc.text(u32::MAX), None);
}
