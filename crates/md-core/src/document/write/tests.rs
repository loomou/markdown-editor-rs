use crate::block::NodeExtra;
use crate::doc::Doc;
use crate::document::Document;
use crate::document::arena::NodeId;
use std::fmt::Write;
use std::path::Path;

use crate::block::BlockKind;
use crate::document::edit::{Caret, Command, Sel, apply};
use crate::document::{editor_options, escape_field, load_markdown};
use crate::inline::InlineMarks;

fn dump_shape(doc: &Document) -> String {
    let mut out = String::new();
    for id in doc.preorder() {
        if id == doc.root {
            continue;
        }
        let Some(node) = doc.arena.get(id) else {
            continue;
        };
        let extra = extra_tag(doc, id);
        let _ = writeln!(
            out,
            "{:?}{}\t{}",
            node.kind,
            extra,
            escape_field(doc.display(id))
        );
        for run in doc.runs(id) {
            if run.marks.is_empty() {
                continue;
            }
            let _ = writeln!(
                out,
                "  {}..{} {}",
                run.display_range.start,
                run.display_range.end,
                mark_names(run.marks)
            );
        }
    }
    out
}

fn extra_tag(doc: &Document, id: NodeId) -> String {
    match doc.extra(id) {
        NodeExtra::List { start, loose, .. } => format!(" list:{start:?}:{loose}"),
        NodeExtra::TaskItem { checked } => format!(" task:{checked}"),
        NodeExtra::CodeFence { lang, .. } => {
            format!(
                " lang:{}",
                lang.and_then(|lang| doc.lang(lang)).unwrap_or("")
            )
        }
        NodeExtra::QuoteAlert { kind, .. } => format!(" alert:{kind:?}"),
        NodeExtra::Image { dest, .. } => format!(" img:{}", doc.link_dest(dest).unwrap_or("")),
        _ => String::new(),
    }
}

fn mark_names(marks: InlineMarks) -> String {
    let flags = [
        (InlineMarks::EM, "EM"),
        (InlineMarks::STRONG, "STRONG"),
        (InlineMarks::STRIKE, "STRIKE"),
        (InlineMarks::CODE, "CODE"),
        (InlineMarks::FOOTNOTE, "FOOTNOTE"),
        (InlineMarks::IMAGE, "IMAGE"),
        (InlineMarks::SUPER, "SUPER"),
        (InlineMarks::SUB, "SUB"),
        (InlineMarks::MATH_INLINE, "MATH_INLINE"),
        (InlineMarks::MATH_DISPLAY, "MATH_DISPLAY"),
        (InlineMarks::SYNTAX, "SYNTAX"),
    ];
    let mut names = Vec::new();
    for (flag, name) in flags {
        if marks.contains(flag) {
            names.push(name);
        }
    }
    names.join("+")
}

const SAMPLE: &str =
    "# title\n\nhello `d` and *em*\n\n```rust\nfn x() {}\n```\n\n> hi\n\n---\n\n- a\n- b\n";

fn caret(block: u32, offset: usize) -> Caret {
    Caret { block, offset }
}

fn type_chars(doc: &mut Document, at: Caret, s: &str) -> Caret {
    let mut c = at;
    for ch in s.chars() {
        c = apply(
            doc,
            Sel::collapsed(c),
            Command::Insert {
                text: ch.to_string(),
            },
        );
    }
    c
}

#[test]
fn trailing_blank_paragraph_is_present_but_not_serialized() {
    let mut doc = load_markdown("# hi\n", editor_options());
    doc.ensure_trailing_blank();
    let leaves = doc.text_leaves();
    assert!(leaves.len() >= 2);
    let last = *leaves.last().expect("last");
    assert_eq!(doc.kind(last), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(last).unwrap_or("x"), "");
    assert_eq!(doc.to_markdown(), "# hi\n");
}

#[test]
fn cold_sample_round_trips_shape() {
    let doc = load_markdown(SAMPLE, editor_options());
    let md = doc.to_markdown();
    let again = load_markdown(&md, editor_options());
    assert_eq!(dump_shape(&doc), dump_shape(&again), "md={md:?}");
    assert!(md.contains("```rust"));
    assert!(md.contains("# title") || md.contains("# title\n"));
}

#[test]
fn block_quotes_inside_list_items_do_not_make_the_list_loose() {
    let source = "- > q\n- b\n";
    let doc = load_markdown(source, editor_options());
    let list = doc
        .preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|node| node.kind) == Some(BlockKind::List))
        .expect("list");

    assert!(!doc.extra(list).list_loose());
    let markdown = doc.to_markdown();
    assert!(!markdown.contains("\n\n- b"), "{markdown:?}");
}

fn list_nodes(doc: &Document) -> Vec<NodeId> {
    doc.preorder()
        .into_iter()
        .filter(|&id| doc.arena.get(id).map(|node| node.kind) == Some(BlockKind::List))
        .collect()
}

#[test]
fn quote_inside_list_item_round_trips_shape() {
    let doc = load_markdown("- > q\n- > r\n", editor_options());
    let md = doc.to_markdown();
    let again = load_markdown(&md, editor_options());
    assert_eq!(dump_shape(&doc), dump_shape(&again), "md={md:?}");
}

#[test]
fn blank_separated_nonparagraph_items_mark_the_list_loose() {
    for source in [
        "- > q\n\n- > r\n",
        "- > q\n \n- > r\n",
        "- ```\n  x\n  ```\n\n- ```\n  y\n  ```\n",
        "- # h\n\n- # h2\n",
    ] {
        let doc = load_markdown(source, editor_options());
        let list = *list_nodes(&doc).first().expect("list");
        assert!(doc.extra(list).list_loose(), "source={source:?}");
        let md = doc.to_markdown();
        let again = load_markdown(&md, editor_options());
        let list = *list_nodes(&again).first().expect("list");
        assert!(again.extra(list).list_loose(), "md={md:?}");
    }
}

#[test]
fn blank_between_blocks_inside_one_item_marks_the_list_loose() {
    for source in ["- > q\n\n  > r\n", "- # h\n\n  # h2\n"] {
        let doc = load_markdown(source, editor_options());
        let list = *list_nodes(&doc).first().expect("list");
        assert!(doc.extra(list).list_loose(), "source={source:?}");
    }
}

#[test]
fn blanks_bordering_a_list_do_not_mark_it_loose() {
    for source in ["- > q\n- > r\n", "para\n\n- > q\n- > r\n"] {
        let doc = load_markdown(source, editor_options());
        let list = *list_nodes(&doc).first().expect("list");
        assert!(!doc.extra(list).list_loose(), "source={source:?}");
    }
}

#[test]
fn blank_between_nested_items_marks_only_the_inner_list_loose() {
    let doc = load_markdown("- - a\n\n  - b\n- - c\n", editor_options());
    let lists = list_nodes(&doc);
    assert_eq!(lists.len(), 3);
    assert!(!doc.extra(lists[0]).list_loose(), "outer");
    assert!(doc.extra(lists[1]).list_loose(), "inner of item 1");
    assert!(!doc.extra(lists[2]).list_loose(), "inner of item 2");
}

#[test]
fn fenced_code_info_strings_round_trip_in_full() {
    for source in [
        "```Python title=\"setup.py\" {1,3-4}\nprint('x')\n```\n",
        "```mermaid theme=neutral\nflowchart TD\nA-->B\n```\n",
    ] {
        let doc = load_markdown(source, editor_options());
        assert_eq!(doc.to_markdown(), source);
        let fence = doc
            .preorder()
            .into_iter()
            .find(|&id| {
                matches!(
                    doc.arena.get(id).map(|node| node.kind),
                    Some(BlockKind::CodeBlock | BlockKind::Mermaid)
                )
            })
            .expect("fence");
        let info = doc.extra(fence).code_fence_lang().expect("info string");
        assert_eq!(
            doc.lang(info),
            source
                .lines()
                .next()
                .and_then(|line| line.strip_prefix("```"))
        );
    }
}

#[test]
fn fenced_code_marker_styles_round_trip() {
    for source in [
        "~~~\ncode\n~~~\n",
        "````rust\nfn main() {}\n````\n",
        "~~~~mermaid theme=neutral\nflowchart TD\nA-->B\n~~~~\n",
        "````\n```\n````\n",
    ] {
        let doc = load_markdown(source, editor_options());
        assert_eq!(doc.to_markdown(), source, "source={source:?}");
    }
}

#[test]
fn tight_list_table_after_a_paragraph_keeps_a_blank_line() {
    use crate::document::TableOp;
    let mut doc = load_markdown("- para\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = apply(
        &mut doc,
        Sel::collapsed(Caret {
            block: leaf,
            offset: 0,
        }),
        Command::Table(TableOp::Insert { rows: 2, cols: 2 }),
    );
    let md = doc.to_markdown();
    let lines: Vec<&str> = md.lines().collect();
    let table_line = lines
        .iter()
        .position(|l| l.trim_start().starts_with('|'))
        .expect("table row");
    assert!(
        table_line > 0 && lines[table_line - 1].trim().is_empty(),
        "table needs a blank line before it: {md:?}"
    );
    let reloaded = load_markdown(&md, editor_options());
    let tables = reloaded
        .preorder()
        .into_iter()
        .filter(|&id| reloaded.arena.get(id).map(|n| n.kind) == Some(BlockKind::Table))
        .count();
    assert_eq!(tables, 1, "the table must survive a reload: {md:?}");
}

#[test]
fn indented_code_blocks_stay_indented() {
    for source in [
        "    code\n",
        "    first\n    second\n",
        "before\n\n    code\n\nafter\n",
    ] {
        let doc = load_markdown(source, editor_options());
        assert_eq!(doc.to_markdown(), source);
        let again = load_markdown(&doc.to_markdown(), editor_options());
        assert_eq!(again.to_markdown(), source);
    }
}

#[test]
fn setext_headings_round_trip_byte_for_byte() {
    for md in ["Title\n=====\n", "Title\n-----\n"] {
        let doc = load_markdown(md, editor_options());
        assert_eq!(doc.to_markdown(), md, "source={md:?}");
        let again = load_markdown(&doc.to_markdown(), editor_options());
        assert_eq!(again.text_leaves().len(), 1, "source={md:?}");
        assert!(matches!(
            again.kind(again.text_leaves()[0]),
            Some(BlockKind::Heading(_))
        ));
    }
}

#[test]
fn thematic_breaks_round_trip_byte_for_byte() {
    for md in ["***\n", "___\n", "- - -\n", "* * *\n"] {
        let doc = load_markdown(md, editor_options());
        assert_eq!(doc.to_markdown(), md, "source={md:?}");
        assert!(doc.preorder().into_iter().any(|id| {
            doc.arena.get(id).map(|node| node.kind) == Some(BlockKind::ThematicBreak)
        }));
    }
}

#[test]
fn table_escaped_pipes_survive_save_and_reload() {
    let source = "| a \\| b | c |\n| --- | --- |\n| d | e |\n";
    let doc = load_markdown(source, editor_options());
    let markdown = doc.to_markdown();
    assert_eq!(markdown, source);

    let again = load_markdown(&markdown, editor_options());
    assert_eq!(again.text_leaves().len(), 4, "{markdown:?}");
    assert_eq!(again.text_of(again.text_leaves()[0]), Some("a | b"));
}

#[test]
fn table_cell_soft_break_round_trips_as_br() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| cd | e |\n", editor_options());
    let cell = doc
        .text_leaves()
        .into_iter()
        .find(|&id| doc.kind(id) == Some(BlockKind::TableCell) && doc.text_of(id) == Some("cd"))
        .expect("cell");

    let out = apply(&mut doc, Sel::collapsed(caret(cell, 1)), Command::SoftBreak);

    assert_eq!(out, caret(cell, 2));
    assert_eq!(doc.text_of(cell), Some("c\nd"));
    let markdown = doc.to_markdown();
    assert_eq!(markdown, "| a | b |\n| --- | --- |\n| c<br>d | e |\n");

    let mut again = load_markdown(&markdown, editor_options());
    let cell = again
        .text_leaves()
        .into_iter()
        .find(|&id| {
            again.kind(id) == Some(BlockKind::TableCell) && again.text_of(id) == Some("c\nd")
        })
        .expect("reloaded cell");
    assert_eq!(again.text_of(cell), Some("c\nd"));
    assert_eq!(again.to_markdown(), markdown);

    let out = apply(
        &mut again,
        Sel::collapsed(caret(cell, 2)),
        Command::DeleteBackward,
    );
    assert_eq!(out, caret(cell, 1));
    assert_eq!(again.text_of(cell), Some("cd"));
    assert_eq!(
        again.to_markdown(),
        "| a | b |\n| --- | --- |\n| cd | e |\n"
    );

    let mut again = load_markdown(&markdown, editor_options());
    let cell = again
        .text_leaves()
        .into_iter()
        .find(|&id| {
            again.kind(id) == Some(BlockKind::TableCell) && again.text_of(id) == Some("c\nd")
        })
        .expect("reloaded cell");
    let out = apply(
        &mut again,
        Sel::collapsed(caret(cell, 1)),
        Command::DeleteForward,
    );
    assert_eq!(out, caret(cell, 1));
    assert_eq!(again.text_of(cell), Some("cd"));
}

#[test]
fn pristine_tables_keep_source_padding_and_separator_style() {
    let source = "|  a  |b|\n|:---|---:|\n| c |  d  |\n";
    let mut doc = load_markdown(source, editor_options());

    assert_eq!(doc.to_markdown(), source);
    assert_eq!(doc.write_snapshot().to_markdown(), source);

    let first_cell = doc.text_leaves()[0];
    let _ = doc.replace_text(first_cell, 0..1, "changed");
    let markdown = doc.to_markdown();
    assert!(markdown.contains("changed"), "{markdown:?}");
    assert_ne!(markdown, source);
}

#[test]
fn pristine_standalone_images_keep_line_padding() {
    let source = "  ![cap](u)  \n";
    let mut doc = load_markdown(source, editor_options());

    assert_eq!(doc.to_markdown(), source);
    assert_eq!(doc.write_snapshot().to_markdown(), source);

    let image = doc.text_leaves()[0];
    let _ = doc.replace_text(image, 2..5, "changed");
    let markdown = doc.to_markdown();
    assert!(markdown.contains("![changed](u)"), "{markdown:?}");
    assert_ne!(markdown, source);
}

#[test]
fn table_pipes_typed_in_a_cell_are_escaped_once() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let cell = doc.text_leaves()[2];
    let _ = doc.replace_text(cell, 1..1, "|");

    let markdown = doc.to_markdown();
    assert!(markdown.contains("| c\\| | d |"), "{markdown:?}");
    let again = load_markdown(&markdown, editor_options());
    assert_eq!(again.text_leaves().len(), 4, "{markdown:?}");
    assert_eq!(again.text_of(again.text_leaves()[2]), Some("c|"));
}

#[test]
fn ragged_tables_serialize_without_dropping_extra_cells() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let table = doc
        .preorder()
        .into_iter()
        .find(|id| doc.arena.get(*id).map(|node| node.kind) == Some(BlockKind::Table))
        .expect("table");
    let body = doc.arena.children(table).nth(1).expect("body row");
    let extra = doc.alloc_leaf(BlockKind::TableCell);
    doc.set_extra(
        extra,
        NodeExtra::Cell {
            align: crate::block::TableCellAlign::Start,
            header: false,
        },
    );
    doc.arena.append_child(body, extra);
    let _ = doc.replace_text(extra.index, 0..0, "kept");

    let markdown = doc.to_markdown();
    assert!(markdown.contains("| --- | --- |"), "{markdown:?}");
    assert!(markdown.contains("| c | d | kept |"), "{markdown:?}");
}

#[test]
fn wide_table_alignment_survives_an_edit_and_snapshot_write() {
    let mut header = Vec::new();
    let mut separator = Vec::new();
    let mut body = Vec::new();
    for col in 0..33 {
        header.push(format!("h{col}"));
        separator.push(if col == 32 { ":---:" } else { "---" });
        body.push(format!("b{col}"));
    }
    let source = format!(
        "| {} |\n| {} |\n| {} |\n",
        header.join(" | "),
        separator.join(" | "),
        body.join(" | ")
    );
    let mut doc = load_markdown(&source, editor_options());
    let first = doc
        .text_leaves()
        .into_iter()
        .find(|id| doc.text_of(*id) == Some("h0"))
        .expect("header cell");
    let wide = doc
        .text_leaves()
        .into_iter()
        .find(|id| doc.text_of(*id) == Some("h32"))
        .expect("wide header cell");
    assert!(matches!(
        doc.extra(doc.live_id(wide).expect("wide id")),
        NodeExtra::Cell {
            align: crate::block::TableCellAlign::Center,
            ..
        }
    ));
    let _ = doc.replace_text(first, 0..2, "H0");

    let markdown = doc.to_markdown();
    let separator_line = markdown.lines().nth(1).expect("separator");
    let cells = separator_line.split('|').map(str::trim).collect::<Vec<_>>();
    assert_eq!(cells.len(), 35, "{markdown}");
    assert_eq!(cells[33], ":---:", "{markdown}");
    assert_eq!(doc.write_snapshot().to_markdown(), markdown);
}

#[test]
fn loose_task_markers_do_not_accumulate_on_save() {
    let source = "- [ ] a\n\n- [x] b\n";
    let mut markdown = source.to_string();
    for _ in 0..3 {
        let doc = load_markdown(&markdown, editor_options());
        assert_eq!(doc.text_of(doc.text_leaves()[0]), Some("a"));
        assert_eq!(doc.text_of(doc.text_leaves()[1]), Some("b"));
        markdown = doc.to_markdown();
        assert_eq!(markdown, source);
    }
}

#[test]
fn ordered_task_items_keep_their_numbers() {
    let source = "3. [ ] todo\n4. [x] done\n";
    let doc = load_markdown(source, editor_options());

    assert_eq!(doc.to_markdown(), source);
}

#[test]
fn list_marker_styles_round_trip() {
    for source in [
        "* a\n* b\n",
        "+ a\n+ b\n",
        "3) a\n4) b\n",
        "* [ ] todo\n* [x] done\n",
        "3) [ ] todo\n4) [x] done\n",
        "> * quoted\n> * list\n",
        "+ outer\n  + inner\n",
    ] {
        let doc = load_markdown(source, editor_options());
        assert_eq!(doc.to_markdown(), source, "source={source:?}");
    }
}

#[test]
fn alert_label_case_and_blank_separator_round_trip() {
    for source in [
        "> [!note]\n> body\n",
        "> [!NoTe]\n> body\n",
        "> [!warning]\n> \n> body\n",
    ] {
        let doc = load_markdown(source, editor_options());
        assert_eq!(doc.to_markdown(), source, "source={source:?}");
    }
}

#[test]
fn block_quote_continuation_prefixes_stay_out_of_leaf_source() {
    let source = "> line one\n> **line two**\n";
    let doc = load_markdown(source, editor_options());
    let leaf = doc.live_id(doc.text_leaves()[0]).expect("quote leaf");
    assert_eq!(doc.leaf_source(leaf), "line one\n**line two**");
    assert_eq!(doc.to_markdown(), source);

    let again = load_markdown(&doc.to_markdown(), editor_options());
    assert_eq!(again.to_markdown(), source);
    assert_eq!(
        again.text_of(again.text_leaves()[0]),
        Some("line one\nline two")
    );
}

#[test]
fn continuation_indents_do_not_accumulate_across_saves() {
    for (source, canonical) in [
        ("- a\n  b\n", "- a\n  b\n"),
        ("[^1]: a\n    b\n", "[^1]: a\n    b\n"),
    ] {
        let mut markdown = source.to_string();
        for _ in 0..3 {
            let doc = load_markdown(&markdown, editor_options());
            markdown = doc.to_markdown();
            assert_eq!(markdown, canonical, "source={source:?}");
        }
    }
}

#[test]
fn footnote_labels_are_metadata_not_editable_leaves() {
    let mut doc = load_markdown("text[^1]\n\n[^1]: note\n", editor_options());
    assert!(
        doc.text_leaves()
            .into_iter()
            .all(|block| doc.text_of(block) != Some("[^1]"))
    );
    let note = doc
        .text_leaves()
        .into_iter()
        .find(|&block| doc.text_of(block) == Some("note"))
        .expect("footnote body");
    let _ = doc.replace_text(note, 0..0, "x");
    assert_eq!(doc.to_markdown(), "text[^1]\n\n[^1]: xnote\n");
    let again = load_markdown(&doc.to_markdown(), editor_options());
    assert!(again.preorder().into_iter().any(|id| {
        again.arena.get(id).map(|node| node.kind) == Some(BlockKind::FootnoteDefinition)
    }));
}

#[test]
fn reference_link_definitions_survive_save_and_reload() {
    for source in [
        "[docs]: https://example.com/docs \"Docs\"\n",
        "See [the docs][docs].\n\n[docs]: https://example.com/docs \"Docs\"\n",
        "![logo][asset]\n\n[asset]: images/logo.png\n",
    ] {
        let doc = load_markdown(source, editor_options());
        assert_eq!(doc.to_markdown(), source, "{source:?}");
        assert_eq!(doc.write_snapshot().to_markdown(), source, "{source:?}");

        let again = load_markdown(&doc.to_markdown(), editor_options());
        assert_eq!(again.to_markdown(), source, "{source:?}");
        if source.contains("[docs]") && source.starts_with("See") {
            let id = again.live_id(again.text_leaves()[0]).expect("link leaf");
            let link = again
                .runs(id)
                .iter()
                .find_map(|run| run.link)
                .expect("reference link");
            assert_eq!(again.link_dest(link), Some("https://example.com/docs"));
        }
    }
}

#[test]
fn html_blocks_keep_their_original_source() {
    for source in [
        "<div align=\"center\">\n</div>\n",
        "<div align=\"center\">\nhello\n</div>\n",
        "before\n\n<section data-x=\"1\">\ninside\n</section>\n\nafter\n",
    ] {
        let doc = load_markdown(source, editor_options());
        assert_eq!(doc.to_markdown(), source, "{source:?}");
        assert_eq!(doc.write_snapshot().to_markdown(), source, "{source:?}");
        let again = load_markdown(&doc.to_markdown(), editor_options());
        assert_eq!(again.to_markdown(), source, "{source:?}");
    }
}

#[test]
fn metadata_blocks_keep_delimiters_and_content() {
    for source in [
        "---\ntitle: hello\ntags:\n  - rust\n---\n",
        "---\ntitle: hello\n---\n# Body\n",
        "---\ntitle: hello\n---\n\n# Body\n",
        "+++\ntitle = \"hello\"\ndraft = false\n+++\n",
        "+++\ntitle = \"hello\"\n+++\nBody\n",
    ] {
        let doc = load_markdown(source, editor_options());
        let block = doc.text_leaves()[0];
        assert_eq!(doc.kind(block), Some(BlockKind::MetadataBlock));
        assert_eq!(doc.to_markdown(), source);
        assert_eq!(doc.write_snapshot().to_markdown(), source);

        let again = load_markdown(&doc.to_markdown(), editor_options());
        assert_eq!(
            again.kind(again.text_leaves()[0]),
            Some(BlockKind::MetadataBlock)
        );
        assert_eq!(again.to_markdown(), source);
    }
}

#[test]
fn colon_lines_round_trip_byte_for_byte() {
    for md in [
        "term\n: desc\n",
        "term\n\n: desc\n",
        "Alpha\n: one\n: two\n",
        "term\n\n: desc\n\nmore\n",
    ] {
        let doc = load_markdown(md, editor_options());
        assert_eq!(doc.to_markdown(), md, "source {md:?}");
        let via_all = load_markdown(md, pulldown_cmark::Options::all());
        assert_eq!(via_all.to_markdown(), md, "Options::all() source {md:?}");
    }
}

#[test]
fn colon_lines_are_plain_paragraphs() {
    let doc = load_markdown("term\n\n: desc\n", editor_options());
    for id in doc.preorder() {
        let node = doc.arena.get(id).expect("live");
        assert!(
            matches!(node.kind, BlockKind::DocRoot | BlockKind::Paragraph),
            "extras: {:?}",
            node.kind
        );
        if node.kind == BlockKind::Paragraph {
            assert_eq!(
                doc.arena.children(id).count(),
                0,
                "the paragraph contains blocks"
            );
        }
    }
    assert_eq!(doc.text_leaves().len(), 2);
}

#[test]
fn typed_code_span_export_keeps_backticks() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = type_chars(&mut doc, caret(leaf, 0), "`a`");
    let md = doc.to_markdown();
    assert!(md.contains("`a`"), "{md:?}");
    let again = load_markdown(&md, editor_options());
    let id = again.live_id(again.text_leaves()[0]).expect("leaf");
    assert_eq!(again.display(id), "a");
    assert!(
        again
            .runs(id)
            .iter()
            .any(|r| r.marks.contains(InlineMarks::CODE))
    );
}

#[test]
fn typed_heading_quote_fence_and_rule_reload() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = apply(
        &mut doc,
        Sel::collapsed(caret(leaf, 0)),
        Command::Insert {
            text: "### title".into(),
        },
    );
    assert_eq!(doc.kind(leaf), Some(BlockKind::Heading(3)));
    let md = doc.to_markdown();
    let again = load_markdown(&md, editor_options());
    assert_eq!(
        again.kind(again.text_leaves()[0]),
        Some(BlockKind::Heading(3))
    );
    assert_eq!(again.text_of(again.text_leaves()[0]).unwrap(), "title");

    let mut q = load_markdown("", editor_options());
    let qleaf = q.text_leaves()[0];
    let _ = type_chars(&mut q, caret(qleaf, 0), "> hi");
    let qmd = q.to_markdown();
    let q2 = load_markdown(&qmd, editor_options());
    let qid = q2.live_id(q2.text_leaves()[0]).expect("q");
    assert_eq!(
        q2.arena.get(qid).and_then(|n| n.parent).map(|p| q2
            .arena
            .get(p)
            .map(|n| n.kind)
            .expect("kind")),
        Some(BlockKind::BlockQuote)
    );
    assert_eq!(q2.display(qid), "hi");

    let mut f = load_markdown("", editor_options());
    let fleaf = f.text_leaves()[0];
    let fat = apply(
        &mut f,
        Sel::collapsed(caret(fleaf, 0)),
        Command::Insert {
            text: "```rust".into(),
        },
    );
    let _ = apply(&mut f, Sel::collapsed(fat), Command::Break);
    let fmd = f.to_markdown();
    let f2 = load_markdown(&fmd, editor_options());
    let code = f2
        .preorder()
        .into_iter()
        .find(|&id| f2.arena.get(id).map(|n| n.kind) == Some(BlockKind::CodeBlock))
        .expect("code");
    let lang = f2.extra(code).code_fence_lang().expect("lang");
    assert_eq!(f2.lang(lang), Some("rust"));

    let mut r = load_markdown("", editor_options());
    let rleaf = r.text_leaves()[0];
    let rat = type_chars(&mut r, caret(rleaf, 0), "---");
    let _ = apply(&mut r, Sel::collapsed(rat), Command::Break);
    let rmd = r.to_markdown();
    let r2 = load_markdown(&rmd, editor_options());
    assert!(
        r2.preorder()
            .into_iter()
            .any(|id| r2.arena.get(id).map(|n| n.kind) == Some(BlockKind::ThematicBreak))
    );
}

#[test]
fn write_snapshot_matches_to_markdown() {
    let samples = [
        SAMPLE,
        "",
        "hello `d` and *em*\n",
        "```rust\nfn x() {}\n```\n",
        "> [!NOTE]\n> hi\n",
        "- [x] a\n- b\n",
        "| a | b |\n| --- | --- |\n| 1 | 2 |\n",
        "para\n\n[^1]: note\n",
        "$$\nE=mc^2\n$$\n",
        "![a](u)\n",
        "```mermaid\ngraph TD\nA-->B\n```\n",
        "term\n: def\n",
    ];
    for md in samples {
        let doc = load_markdown(md, editor_options());
        let snap = doc.write_snapshot();
        assert_eq!(
            snap.to_markdown(),
            doc.to_markdown(),
            "md={md:?} revision={}",
            snap.revision()
        );
        assert_eq!(snap.revision(), doc.revision());
    }

    let mut typed = load_markdown("", editor_options());
    let leaf = typed.text_leaves()[0];
    let _ = type_chars(&mut typed, caret(leaf, 0), "**bold**");
    assert_eq!(typed.write_snapshot().to_markdown(), typed.to_markdown());
}

#[test]
fn write_snapshot_round_trips_kind_and_marks() {
    let doc = load_markdown(SAMPLE, editor_options());
    let md = doc.write_snapshot().to_markdown();
    let again = load_markdown(&md, editor_options());
    assert_eq!(dump_shape(&doc), dump_shape(&again), "md={md:?}");
}

#[test]
fn footnote_nonparagraph_first_block_round_trips_shape() {
    for source in [
        "[^1]: - a
",
        "[^1]: > q
",
        "[^1]: # h
",
        "[^1]: $$m$$
",
        "[^1]:
    ```
    x
    ```
",
    ] {
        let doc = load_markdown(source, editor_options());
        let md = doc.to_markdown();
        let again = load_markdown(&md, editor_options());
        assert_eq!(
            dump_shape(&doc),
            dump_shape(&again),
            "source={source:?} md={md:?}"
        );
        assert_eq!(again.to_markdown(), md, "source={source:?} not idempotent");
    }
}

#[test]
fn footnote_blocks_after_a_blank_line_stay_inside() {
    for source in [
        "[^1]: para

    two
",
        "[^1]: para

    - a
",
        "[^1]:
    > q

    > r
",
    ] {
        let doc = load_markdown(source, editor_options());
        let md = doc.to_markdown();
        let again = load_markdown(&md, editor_options());
        assert_eq!(
            dump_shape(&doc),
            dump_shape(&again),
            "source={source:?} md={md:?}"
        );
        assert_eq!(again.to_markdown(), md, "source={source:?} not idempotent");
    }
}

#[test]
fn footnote_list_first_block_gets_the_indented_form() {
    let doc = load_markdown(
        "[^1]: - a
",
        editor_options(),
    );
    assert_eq!(
        doc.to_markdown(),
        "[^1]:
    - a
"
    );
}

#[test]
fn empty_quotes_keep_their_marker_across_a_reload() {
    let doc = load_markdown("# h\n\n>\n\ntail\n", editor_options());

    let markdown = doc.to_markdown();
    assert!(markdown.contains('\n'), "{markdown:?}");
    assert!(markdown.contains('>'), "{markdown:?}");

    let again = load_markdown(&markdown, editor_options());
    let kept_empty_quote = again.preorder().into_iter().any(|id| {
        again
            .arena
            .get(id)
            .is_some_and(|n| n.kind == BlockKind::BlockQuote)
            && again.arena.children(id).next().is_none()
    });
    assert!(
        kept_empty_quote,
        "reloaded document lost its empty quote: {markdown:?}"
    );
    assert_eq!(again.to_markdown(), markdown, "not idempotent");
}

#[test]
fn quotes_emptied_by_editing_keep_their_marker_too() {
    let mut doc = load_markdown("> text\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = doc.replace_text(leaf, 0..4, "");

    let markdown = doc.to_markdown();
    assert!(markdown.contains('>'), "{markdown:?}");

    let again = load_markdown(&markdown, editor_options());
    let kept_empty_quote = again.preorder().into_iter().any(|id| {
        again
            .arena
            .get(id)
            .is_some_and(|n| n.kind == BlockKind::BlockQuote)
            && again.arena.children(id).next().is_none()
    });
    assert!(
        kept_empty_quote,
        "reloaded document lost its emptied quote: {markdown:?}"
    );
}

#[test]
fn cross_line_constructs_round_trip_stable_in_every_host() {
    let hosts: &[(&str, &str, &str)] = &[
        ("- a ", "  ", " d"),
        ("1. a ", "   ", " d"),
        ("- - a ", "    ", " d"),
        ("- [ ] a ", "  ", " d"),
        ("> a ", "> ", " d"),
        ("> > a ", "> > ", " d"),
        ("[^1]: a ", "    ", " d"),
    ];
    let make: Vec<&dyn Fn(&str) -> String> = vec![
        &|i: &str| format!("**b\n{i}c**"),
        &|i: &str| format!("`b\n{i}c`"),
        &|i: &str| format!("$b\n{i}c$"),
        &|i: &str| format!("[b\n{i}c](u)"),
    ];
    for (head, prefix, tail) in hosts {
        for body in &make {
            let src = format!("{head}{}{tail}", body(prefix));
            let mut markdown = src.clone();
            let mut displays: Vec<String> = Vec::new();
            let mut prev = String::new();
            for round in 0..4 {
                let doc = load_markdown(&markdown, editor_options());
                displays.push(
                    doc.text_leaves()
                        .into_iter()
                        .map(|b| doc.text_of(b).unwrap_or("").to_string())
                        .collect::<Vec<_>>()
                        .join("\u{1}"),
                );
                let next = doc.to_markdown();
                if round >= 1 {
                    assert_eq!(
                        next, prev,
                        "saving is not a fixed point: src={src:?} round={round} md={next:?}"
                    );
                }
                markdown = next.clone();
                prev = next;
            }
            for (round, d) in displays.iter().enumerate() {
                assert_eq!(
                    d, &displays[0],
                    "display was rewritten at round {round}: src={src:?} displays={displays:?}"
                );
            }
        }
    }
}

#[test]
fn inline_code_and_math_keep_continuation_whitespace() {
    for source in [
        "`a\n    b`\n",
        "- `a\n      b`\n",
        "> `a\n>     b`\n",
        "a $x\n    y$ b\n",
    ] {
        let first = load_markdown(source, editor_options());
        let display: Vec<String> = first
            .text_leaves()
            .into_iter()
            .map(|b| first.text_of(b).unwrap_or("").to_string())
            .collect();
        let mut markdown = source.to_string();
        for round in 0..3 {
            let doc = load_markdown(&markdown, editor_options());
            let now: Vec<String> = doc
                .text_leaves()
                .into_iter()
                .map(|b| doc.text_of(b).unwrap_or("").to_string())
                .collect();
            assert_eq!(
                now, display,
                "display was rewritten at round {round}: src={source:?}"
            );
            let saved = doc.to_markdown();
            if round >= 1 {
                assert_eq!(
                    saved, markdown,
                    "saving is not a fixed point: src={source:?} round={round}"
                );
            }
            markdown = saved;
        }
    }
}

#[test]
fn display_math_extraction_does_not_duplicate_enclosing_source() {
    for source in [
        "**before $$x$$ after**\n",
        "a **b $$x$$ c** d\n",
        "[before $$x$$ after](u)\n",
    ] {
        let first = load_markdown(source, editor_options());
        let leaves = first.text_leaves().len();
        let mut markdown = first.to_markdown();
        for round in 0..3 {
            let doc = load_markdown(&markdown, editor_options());
            assert_eq!(
                doc.text_leaves().len(),
                leaves,
                "the leaf count ballooned at round {round}: src={source:?} md={markdown:?}"
            );
            assert_eq!(
                doc.to_markdown().matches("$$x$$").count(),
                1,
                "a formula was duplicated: src={source:?} md={markdown:?}"
            );
            let saved = doc.to_markdown();
            if round >= 1 {
                assert_eq!(
                    saved, markdown,
                    "saving is not a fixed point: src={source:?}"
                );
            }
            markdown = saved;
        }
    }
}

#[test]
fn escaped_punctuation_stays_literal_across_saves() {
    for (source, first_display) in [
        (r"\*word\*".to_string() + "\n", "*word*"),
        (r"a\*word\*z".to_string() + "\n", "a*word*z"),
        (r"\# title".to_string() + "\n", "# title"),
    ] {
        let first = load_markdown(&source, editor_options());
        assert_eq!(
            first.text_of(first.text_leaves()[0]),
            Some(first_display),
            "src={source:?}"
        );
        let mut markdown = source.clone();
        for round in 0..3 {
            let doc = load_markdown(&markdown, editor_options());
            let reloaded_display = doc
                .text_leaves()
                .into_iter()
                .map(|b| doc.text_of(b).unwrap_or("").to_string())
                .collect::<Vec<_>>()
                .join("\u{1}");
            let first_all: String = first_display.to_string();
            assert_eq!(
                reloaded_display, first_all,
                "display was rewritten at round {round}: src={source:?}"
            );
            let saved = doc.to_markdown();
            if round >= 1 {
                assert_eq!(
                    saved, markdown,
                    "saving is not a fixed point: src={source:?}"
                );
            }
            markdown = saved;
        }
    }
}

#[test]
fn over_indented_continuations_normalize_once_then_hold() {
    let hosts: &[(&str, &str, &str)] = &[
        ("- a ", "    ", " d"),
        ("- [ ] a ", "      ", " d"),
        ("[^1]: a ", "       ", " d"),
    ];
    let make: Vec<&dyn Fn(&str) -> String> = vec![
        &|i: &str| format!("**b\n{i}c**"),
        &|i: &str| format!("`b\n{i}c`"),
        &|i: &str| format!("$b\n{i}c$"),
        &|i: &str| format!("[b\n{i}c](u)"),
    ];
    for (head, prefix, tail) in hosts {
        for body in &make {
            let src = format!("{head}{}{tail}", body(prefix));
            let mut markdown = src.clone();
            let mut displays: Vec<String> = Vec::new();
            let mut prev = String::new();
            for round in 0..4 {
                let doc = load_markdown(&markdown, editor_options());
                displays.push(
                    doc.text_leaves()
                        .into_iter()
                        .map(|b| doc.text_of(b).unwrap_or("").to_string())
                        .collect::<Vec<_>>()
                        .join("\u{1}"),
                );
                let next = doc.to_markdown();
                if round >= 1 {
                    assert_eq!(
                        next, prev,
                        "saving is still unstable at round {round}: src={src:?} md={next:?}"
                    );
                }
                markdown = next.clone();
                prev = next;
            }
            for (round, d) in displays.iter().enumerate().skip(1) {
                assert_eq!(
                    d, &displays[1],
                    "display is still drifting at round {round}: src={src:?} displays={displays:?}"
                );
            }
        }
    }
}

#[test]
fn repo_corpus_reaches_a_text_level_save_fixed_point() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut paths = vec![root.join("README.md")];
    if let Ok(entries) = std::fs::read_dir(root.join("docs")) {
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|e| e == "md") {
                paths.push(entry.path());
            }
        }
    }
    paths.retain(|p| p.is_file());
    if paths.is_empty() {
        return;
    }
    for path in &paths {
        let Ok(md) = std::fs::read_to_string(path) else {
            panic!("cannot read the corpus: {}", path.display());
        };
        let mut markdown = md;
        let mut prev = String::new();
        let mut displays: Option<String> = None;
        for round in 0..3 {
            let doc = load_markdown(&markdown, editor_options());
            let display = doc
                .text_leaves()
                .into_iter()
                .map(|b| doc.text_of(b).unwrap_or("").to_string())
                .collect::<Vec<_>>()
                .join("\u{1}");
            if let Some(d) = &displays {
                assert_eq!(
                    &display,
                    d,
                    "display drifted at round {round}: {}",
                    path.display()
                );
            } else {
                displays = Some(display);
            }
            let next = doc.to_markdown();
            if round >= 1 {
                assert_eq!(
                    next,
                    prev,
                    "saving is not a fixed point at round {round}: {}",
                    path.display()
                );
            }
            markdown = next.clone();
            prev = next;
        }
    }
}

#[test]
fn indented_and_nested_quote_markers_survive_saves() {
    for src in [
        "> **a\n > b**\n",
        "> **a\n  > b**\n",
        "> **a\n   > b**\n",
        "> > **a\n> > b**\n",
        "> > > **a\n> > > b**\n",
        "> `a\n > b`\n",
        "> > [a\n> > b](u)\n",
    ] {
        let mut markdown = src.to_string();
        let mut fixed: Option<String> = None;
        for round in 0..3 {
            let doc = load_markdown(&markdown, editor_options());
            let leaves = doc
                .text_leaves()
                .into_iter()
                .map(|b| doc.text_of(b).unwrap_or("").to_string())
                .collect::<Vec<_>>();
            assert_eq!(
                leaves.len(),
                1,
                "one leaf split into several: src={src:?} round={round} leaves={leaves:?}"
            );
            assert!(
                !leaves[0].contains("**") && !leaves[0].contains('`'),
                "markers turned literal: src={src:?} round={round} leaves={leaves:?}"
            );
            let next = doc.to_markdown();
            if let Some(f) = &fixed {
                assert_eq!(next, f.as_str(), "not idempotent: src={src:?} md={next:?}");
            }
            fixed = Some(next.clone());
            markdown = next;
        }
    }
}

#[test]
fn ordered_list_numbers_stay_within_marker_limits() {
    let doc = load_markdown("999999999. a\n1. b\n", editor_options());
    let saved = doc.to_markdown();
    let reloaded = load_markdown(&saved, editor_options());
    assert_eq!(saved, "999999999. a\n999999999. b\n");
    let texts: Vec<_> = reloaded
        .text_leaves()
        .into_iter()
        .map(|b| reloaded.text_of(b).unwrap_or("").to_string())
        .collect();
    assert_eq!(texts, ["a", "b"], "saved={saved:?}");
}

#[test]
fn reference_definitions_escape_a_trailing_unclosed_html_block() {
    for (tail, tail_text) in [
        ("<script>\ntext\n", "text"),
        ("<!-- open\n", ""),
        ("<pre>\nx\n", "x"),
    ] {
        let source = format!("[r]: /url\n\n[go][r]\n\n{tail}");
        let doc = load_markdown(&source, editor_options());
        let saved = doc.to_markdown();
        let reloaded = load_markdown(&saved, editor_options());
        let texts: Vec<_> = reloaded
            .text_leaves()
            .into_iter()
            .map(|b| reloaded.text_of(b).unwrap_or("").to_string())
            .collect();
        assert_eq!(texts, ["go", tail_text], "tail={tail:?} saved={saved:?}");
        assert_eq!(reloaded.to_markdown(), saved, "tail={tail:?}");
    }
}

#[test]
fn closed_html_blocks_keep_definitions_at_the_tail() {
    let doc = load_markdown(
        "[r]: /url\n\n[go][r]\n\n<div>\nx\n</div>\n",
        editor_options(),
    );
    let saved = doc.to_markdown();
    assert_eq!(saved, "[go][r]\n\n<div>\nx\n</div>\n\n[r]: /url\n");
    let reloaded = load_markdown(&saved, editor_options());
    let texts: Vec<_> = reloaded
        .text_leaves()
        .into_iter()
        .map(|b| reloaded.text_of(b).unwrap_or("").to_string())
        .collect();
    assert_eq!(texts, ["go", "x"]);
}

#[test]
fn prefix_close_tags_do_not_count_as_closed() {
    for tail in [
        "<script>\nfoo\n</scripture>\n",
        "<pre>\nx\n</pretty>\n",
        "<style>\ns\n</stylus>\n",
        "<textarea>\nt\n</textareax>\n",
        "<pre>\nx\n</pre >\n",
    ] {
        let source = format!("[r]: target\n\n[r]\n\n{tail}");
        let doc = load_markdown(&source, editor_options());
        let saved = doc.to_markdown();
        let reloaded = load_markdown(&saved, editor_options());
        let leaf = reloaded.live_id(reloaded.text_leaves()[0]).expect("leaf");
        assert_eq!(
            reloaded.link_at(leaf, 0),
            Some("target"),
            "tail={tail:?} saved={saved:?}"
        );
        assert_eq!(reloaded.to_markdown(), saved, "tail={tail:?}");
    }

    {
        let tail = "<script>\nclosed\n</script>\n";
        let source = format!("[r]: target\n\n[r]\n\n{tail}");
        let doc = load_markdown(&source, editor_options());
        let saved = doc.to_markdown();
        assert!(
            saved.ends_with("[r]: target\n"),
            "closed tail keeps definitions at the end: {saved:?}"
        );
        let reloaded = load_markdown(&saved, editor_options());
        let leaf = reloaded.live_id(reloaded.text_leaves()[0]).expect("leaf");
        assert_eq!(reloaded.link_at(leaf, 0), Some("target"), "tail={tail:?}");
    }
}

#[test]
fn fence_marker_lines_inside_a_paragraph_are_escaped_on_save() {
    let mut doc = load_markdown("before\n\nafter\n", editor_options());
    let leaves = doc.text_leaves();
    let _ = doc.replace_text(leaves[0], 6..6, "\n\n```rust\nfn\n```");
    let saved = doc.to_markdown();
    assert_eq!(saved, "before\n\n\\```rust\nfn\n\\```\n\nafter\n");

    let reloaded = load_markdown(&saved, editor_options());
    let kinds: Vec<BlockKind> = reloaded
        .preorder()
        .into_iter()
        .filter_map(|id| reloaded.arena.get(id).map(|n| n.kind))
        .collect();
    assert_eq!(
        kinds,
        vec![
            BlockKind::DocRoot,
            BlockKind::Paragraph,
            BlockKind::Paragraph,
            BlockKind::Paragraph
        ]
    );
    assert_eq!(reloaded.to_markdown(), saved);
}

#[test]
fn indented_and_tilde_fence_lines_are_escaped_too() {
    let mut doc = load_markdown("body\n", editor_options());
    let leaves = doc.text_leaves();
    let _ = doc.replace_text(leaves[0], 4..4, "\n\na\n~~~txt\nx\n~~~");
    let saved = doc.to_markdown();
    assert_eq!(saved, "body\n\na\n\\~~~txt\nx\n\\~~~\n");

    let reloaded = load_markdown(&saved, editor_options());
    let kinds: Vec<BlockKind> = reloaded
        .preorder()
        .into_iter()
        .filter_map(|id| reloaded.arena.get(id).map(|n| n.kind))
        .collect();
    assert_eq!(
        kinds,
        vec![
            BlockKind::DocRoot,
            BlockKind::Paragraph,
            BlockKind::Paragraph
        ]
    );
    assert_eq!(reloaded.to_markdown(), saved);
}

#[test]
fn fence_marker_text_inside_a_table_cell_stays_literal() {
    let doc = load_markdown("| ``` |\n| --- |\n| ```x``` |\n", editor_options());
    assert_eq!(doc.to_markdown(), "| ``` |\n| --- |\n| ```x``` |\n");
}

#[test]
fn trailing_blank_does_not_hide_an_unterminated_html_tail_from_the_writer() {
    let mut doc = Doc::new(load_markdown(
        "[r]: /target\n\n[label][r]\n\n<!-- unclosed\n",
        editor_options(),
    ));
    let control = Doc::new(load_markdown(&doc.document.to_markdown(), editor_options()));
    assert_eq!(
        control.link_at(Caret {
            block: control.text_leaves()[0],
            offset: 1
        }),
        Some("/target")
    );
    doc.enable_trailing_blank();
    let saved = doc.document.to_markdown();
    let reloaded = Doc::new(load_markdown(&saved, editor_options()));
    let live = doc.link_at(Caret {
        block: doc.text_leaves()[0],
        offset: 1,
    });
    let after_reload = reloaded.link_at(Caret {
        block: reloaded.text_leaves()[0],
        offset: 1,
    });
    assert_eq!(after_reload, live, "saved={saved:?}");
}
