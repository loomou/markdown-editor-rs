use md_core::doc::Doc;
use md_core::document::{Caret, Command, Sel, editor_options, load_markdown};
use md_core::inline::InlineMarks;

fn fresh(source: &str) -> Doc {
    Doc::new(load_markdown(source, editor_options()))
}

fn cell_doc(value: &str) -> (Doc, u32) {
    let doc = Doc::new(load_markdown(
        &format!("| h |\n| --- |\n| {value} |\n"),
        editor_options(),
    ));
    let block = *doc.text_leaves().last().unwrap();
    (doc, block)
}

#[test]
fn delete_covers_the_middle_table_head_partial() {
    let source =
        "| A | B |\n| --- | --- |\n| C | D |\n\n| E | F |\n| --- | --- |\n| G | H |\n\nend\n";
    let mut doc = Doc::new(load_markdown(source, editor_options()));
    let leaves = doc.text_leaves();
    let first = leaves[1];
    let last = *leaves.last().unwrap();
    doc.apply(
        Sel {
            anchor: Caret {
                block: first,
                offset: 0,
            },
            head: Caret {
                block: last,
                offset: 1,
            },
        },
        Command::DeleteForward,
    );
    let saved = doc.document.to_markdown();
    println!("saved={saved:?}");
    assert_eq!(
        saved.matches("| --- |").count(),
        1,
        "the partially covered endpoint table must survive, the fully covered middle table must not"
    );
    assert!(!saved.contains('E') && !saved.contains('G'));
    assert!(saved.contains("nd"));
}

#[test]
fn delete_covers_the_middle_table_tail_partial() {
    let source =
        "start\n\n| A | B |\n| --- | --- |\n| C | D |\n\n| E | F |\n| --- | --- |\n| G | H |\n";
    let mut doc = Doc::new(load_markdown(source, editor_options()));
    let leaves = doc.text_leaves();
    let first = leaves[0];
    let last = leaves[5];
    doc.apply(
        Sel {
            anchor: Caret {
                block: first,
                offset: 1,
            },
            head: Caret {
                block: last,
                offset: 1,
            },
        },
        Command::DeleteForward,
    );
    let saved = doc.document.to_markdown();
    println!("saved={saved:?}");
    assert_eq!(
        saved.matches("| --- |").count(),
        1,
        "the partially covered endpoint table must survive, the fully covered middle table must not"
    );
    assert!(
        !saved.contains('A') && !saved.contains('C'),
        "saved={saved:?}"
    );
    assert!(
        saved.contains('H'),
        "the unselected tail cells must survive: {saved:?}"
    );
    assert!(saved.contains("s"));
}

#[test]
fn cell_space_edits_are_visible_and_stay_put() {
    let (mut doc, block) = cell_doc("a<br>b");
    let caret = doc.apply(
        Sel::collapsed(Caret { block, offset: 0 }),
        Command::Insert { text: " ".into() },
    );
    println!(
        "br source={:?}\ncollapsed={:?}\ncaret={caret:?}",
        doc.document
            .block_source(doc.document.live_id(block).unwrap()),
        doc.collapsed_text(block)
    );
    assert_eq!(
        doc.collapsed_text(block).unwrap(),
        " a\nb",
        "the typed space must be visible"
    );
    let caret = doc.retarget_focus(caret);
    doc.apply(Sel::collapsed(caret), Command::Insert { text: "X".into() });
    assert_eq!(
        doc.collapsed_text(block).unwrap(),
        " Xa\nb",
        "typing must continue at the caret"
    );

    let (mut doc, block) = cell_doc("ab");
    doc.apply(
        Sel::collapsed(Caret { block, offset: 2 }),
        Command::Insert { text: " ".into() },
    );
    println!("plain collapsed={:?}", doc.collapsed_text(block));
    assert_eq!(
        doc.collapsed_text(block).unwrap(),
        "ab ",
        "a trailing space must be visible"
    );
}

#[test]
fn table_cell_retains_new_trailing_space() {
    let mut d = Doc::new(load_markdown("| h |\n| --- |\n| a |\n", editor_options()));
    let id = d
        .text_leaves()
        .into_iter()
        .find(|&id| d.text(id) == Some("a"))
        .unwrap();
    let caret = d.apply(
        Sel::collapsed(Caret {
            block: id,
            offset: 1,
        }),
        Command::Insert { text: " ".into() },
    );
    assert_eq!(d.text(id), Some("a "));
    assert_eq!(caret.offset, 2);
}

#[test]
fn typing_a_pipe_keeps_cell_semantics() {
    let mut doc = Doc::new(load_markdown(
        "| h |\n| --- |\n| a<br>b |\n",
        editor_options(),
    ));
    let block = *doc.text_leaves().last().unwrap();
    doc.apply(
        Sel::collapsed(Caret { block, offset: 1 }),
        Command::Insert { text: "|".into() },
    );
    println!(
        "source={:?}\ncollapsed={:?}\nsaved={:?}",
        doc.document
            .block_source(doc.document.live_id(block).unwrap()),
        doc.collapsed_text(block),
        doc.document.to_markdown()
    );
    assert_eq!(
        doc.collapsed_text(block).unwrap(),
        "a|\nb",
        "a typed pipe must not drop the cell's <br>"
    );
    let saved = doc.document.to_markdown();
    let reloaded = Doc::new(load_markdown(&saved, editor_options()));
    let reloaded_block = *reloaded.text_leaves().last().unwrap();
    assert_eq!(
        reloaded.collapsed_text(reloaded_block),
        doc.collapsed_text(block),
        "edit state and reloaded document must agree (saved={saved:?})"
    );
}

#[test]
fn wrapping_keeps_existing_escape_sequences() {
    let mut doc = Doc::new(load_markdown(
        "| h |\n| --- |\n| a\\*b<br>c |\n",
        editor_options(),
    ));
    let block = *doc.text_leaves().last().unwrap();
    println!("loaded collapsed={:?}", doc.collapsed_text(block));
    assert_eq!(doc.collapsed_text(block).unwrap(), "a*b\nc");
    doc.apply(
        Sel::collapsed(Caret { block, offset: 0 }),
        Command::Insert { text: " ".into() },
    );
    println!("after collapsed={:?}", doc.collapsed_text(block));
    assert_eq!(
        doc.collapsed_text(block).unwrap(),
        " a*b\nc",
        "wrapping must not tear existing escape sequences apart"
    );
}

#[test]
fn pipe_header_commit_keeps_edge_empty_columns() {
    let mut d = Doc::new(load_markdown("| | b | |\n", editor_options()));
    let id = d.first_text_leaf().unwrap();
    let caret = d.apply(
        Sel::collapsed(Caret {
            block: id,
            offset: d.caret_text(id).unwrap().len(),
        }),
        Command::Break,
    );
    let loc = d.table_loc(caret.block).unwrap();
    println!("loc={loc:?}; saved={:?}", d.document.to_markdown());
    assert_eq!(loc.cols, 3);
}

#[test]
fn pipe_header_commit_keeps_leading_empty_column() {
    let mut d = Doc::new(load_markdown("| | b |\n", editor_options()));
    let id = d.first_text_leaf().unwrap();
    let caret = d.apply(
        Sel::collapsed(Caret {
            block: id,
            offset: d.caret_text(id).unwrap().len(),
        }),
        Command::Break,
    );
    let loc = d.table_loc(caret.block).unwrap();
    println!("leading={loc:?}; saved={:?}", d.document.to_markdown());
    assert_eq!(loc.cols, 2);
}

#[test]
fn pipe_header_commit_plain_two_columns() {
    let mut d = Doc::new(load_markdown("| a | b |\n", editor_options()));
    let id = d.first_text_leaf().unwrap();
    let caret = d.apply(
        Sel::collapsed(Caret {
            block: id,
            offset: d.caret_text(id).unwrap().len(),
        }),
        Command::Break,
    );
    let loc = d.table_loc(caret.block).unwrap();
    println!("plain={loc:?}; saved={:?}", d.document.to_markdown());
    assert_eq!(loc.cols, 2);
}

#[test]
fn table_trailing_soft_break_survives_save() {
    let mut d = Doc::new(load_markdown("| h |\n| --- |\n| a |\n", editor_options()));
    let block = d.text_leaves()[1];
    d.apply(
        Sel::collapsed(Caret { block, offset: 1 }),
        Command::SoftBreak,
    );
    assert_eq!(
        d.text(block),
        Some("a\n"),
        "the in-memory display has the new line"
    );
    let saved = d.document.to_markdown();
    let reread = Doc::new(load_markdown(&saved, editor_options()));
    let body = reread.text_leaves()[1];
    println!("saved={saved:?}; reloaded={:?}", reread.text(body));
    assert_eq!(reread.text(body), Some("a\n"));
}

#[test]
fn table_soft_break_retains_existing_html_break() {
    let mut d = Doc::new(load_markdown(
        "| h |\n| --- |\n| a<br>b |\n",
        editor_options(),
    ));
    let block = d.text_leaves()[1];
    assert_eq!(d.text(block), Some("a\nb"));
    d.apply(
        Sel::collapsed(Caret { block, offset: 3 }),
        Command::SoftBreak,
    );
    println!(
        "saved={:?}; display={:?}",
        d.document.to_markdown(),
        d.text(block)
    );
    assert_eq!(d.text(block), Some("a\nb\n"));
}

#[test]
fn cell_with_mixed_breaks_round_trips() {
    let mut d = Doc::new(load_markdown(
        "| h |\n| --- |\n| a<br>b |\n",
        editor_options(),
    ));
    let block = d.text_leaves()[1];
    d.apply(
        Sel::collapsed(Caret { block, offset: 3 }),
        Command::SoftBreak,
    );
    let saved = d.document.to_markdown();
    println!("mixed saved={saved:?}");
    let reread = Doc::new(load_markdown(&saved, editor_options()));
    let body = reread.text_leaves()[1];
    assert_eq!(
        reread.text(body),
        Some("a\nb\n"),
        "both the middle and the trailing break must survive the save"
    );
}

#[test]
fn table_leading_space_added_by_edit_is_lost_on_reload() {
    let mut doc = fresh("| h |\n| --- |\n| a |\n");
    let leaf = *doc.text_leaves().last().expect("body cell");
    let caret = doc.apply(
        Sel::collapsed(Caret {
            block: leaf,
            offset: 0,
        }),
        Command::Insert { text: " ".into() },
    );
    assert_eq!(
        doc.caret_text(caret.block),
        Some(" a"),
        "fixture: the live projection has the space"
    );
    let saved = doc.document.to_markdown();
    let reloaded = Doc::new(load_markdown(&saved, editor_options()));
    let reloaded_leaf = *reloaded.text_leaves().last().expect("reloaded body cell");
    assert_eq!(
        reloaded.caret_text(reloaded_leaf),
        Some(" a"),
        "saved={saved:?}"
    );
}

#[test]
fn table_trailing_space_added_by_edit_is_lost_on_reload() {
    let mut doc = fresh("| h |\n| --- |\n| a |\n");
    let leaf = *doc.text_leaves().last().expect("body cell");
    let caret = doc.apply(
        Sel::collapsed(Caret {
            block: leaf,
            offset: 1,
        }),
        Command::Insert { text: " ".into() },
    );
    assert_eq!(
        doc.caret_text(caret.block),
        Some("a "),
        "fixture: the live projection has the space"
    );
    let saved = doc.document.to_markdown();
    let reloaded = Doc::new(load_markdown(&saved, editor_options()));
    let reloaded_leaf = *reloaded.text_leaves().last().expect("reloaded body cell");
    assert_eq!(
        reloaded.caret_text(reloaded_leaf),
        Some("a "),
        "saved={saved:?}"
    );
}

#[test]
fn repeated_edge_spaces_round_trip_and_survive_further_edits() {
    let mut doc = fresh("| h |\n| --- |\n| a |\n");
    let leaf = *doc.text_leaves().last().expect("body cell");
    doc.apply(
        Sel::collapsed(Caret {
            block: leaf,
            offset: 0,
        }),
        Command::Insert { text: "  ".into() },
    );
    doc.apply(
        Sel::collapsed(Caret {
            block: leaf,
            offset: 3,
        }),
        Command::Insert { text: " ".into() },
    );
    assert_eq!(doc.caret_text(leaf), Some("  a "));
    let saved = doc.document.to_markdown();
    println!("saved={saved:?}");
    let mut reloaded = Doc::new(load_markdown(&saved, editor_options()));
    let reloaded_leaf = *reloaded.text_leaves().last().expect("reloaded body cell");
    assert_eq!(
        reloaded.caret_text(reloaded_leaf),
        Some("  a "),
        "saved={saved:?}"
    );
    reloaded.apply(
        Sel::collapsed(Caret {
            block: reloaded_leaf,
            offset: 4,
        }),
        Command::Insert { text: "!".into() },
    );
    assert_eq!(reloaded.caret_text(reloaded_leaf), Some("  a !"));
    let resaved = reloaded.document.to_markdown();
    let reread = Doc::new(load_markdown(&resaved, editor_options()));
    let reread_leaf = *reread.text_leaves().last().expect("reread body cell");
    assert_eq!(
        reread.caret_text(reread_leaf),
        Some("  a !"),
        "resaved={resaved:?}"
    );
}

#[test]
fn leading_html_break_does_not_shift_bold_after_edit() {
    let mut doc = Doc::new(load_markdown(
        "| h |\n| --- |\n| <br>**ab** |\n",
        editor_options(),
    ));
    let block = doc.text_leaves()[1];
    doc.apply(
        Sel::collapsed(Caret { block, offset: 3 }),
        Command::Insert { text: "!".into() },
    );
    assert_eq!(doc.text(block), Some("\nab!"));
    let id = doc.document.live_id(block).unwrap();
    let bold = doc
        .document
        .runs(id)
        .iter()
        .find(|run| run.marks.contains(InlineMarks::STRONG))
        .unwrap();
    assert_eq!(bold.display_range, 1..3);
}

#[test]
fn leading_html_break_does_not_move_link_after_edit() {
    let mut doc = Doc::new(load_markdown(
        "| h |\n| --- |\n| <br><br>[ab](/target) |\n",
        editor_options(),
    ));
    let block = doc.text_leaves()[1];
    doc.apply(
        Sel::collapsed(Caret { block, offset: 4 }),
        Command::Insert { text: "!".into() },
    );
    assert_eq!(doc.text(block), Some("\n\nab!"));
    assert_eq!(doc.link_at(Caret { block, offset: 2 }), Some("/target"));
}
