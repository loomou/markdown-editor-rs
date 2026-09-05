use super::support::{caret, type_chars};
use crate::block::{BlockKind, NodeExtra, TABLE_ALIGN_COLS, TableCellAlign, alignment_at};
use crate::doc::Doc;
use crate::document::arena::NodeId;
use crate::document::edit::table::{
    TablePath, delete_column_unchecked, resize_table_unchecked, retarget_header,
};
use crate::document::edit::{
    Caret, Command, Sel, TABLE_INSERT_MAX_COLS, TABLE_INSERT_MAX_ROWS, TABLE_INSERT_MIN_COLS,
    TABLE_INSERT_MIN_ROWS, TABLE_PICKER_MAX_COLS, TABLE_PICKER_MAX_ROWS, TableOp, TableStep, apply,
    table_loc, table_step,
};
use crate::document::{Document, editor_options, load_markdown};
use crate::inline::InlineMarks;

fn first_table(doc: &Document) -> NodeId {
    doc.preorder()
        .into_iter()
        .find(|&id| doc.arena.get(id).map(|n| n.kind) == Some(BlockKind::Table))
        .expect("table")
}

fn rows(doc: &Document, table: NodeId) -> Vec<NodeId> {
    doc.arena.children(table).collect()
}

fn cells(doc: &Document, row: NodeId) -> Vec<NodeId> {
    doc.arena.children(row).collect()
}

fn dims(doc: &Document) -> (usize, usize) {
    let table = first_table(doc);
    let rows = rows(doc, table);
    let cols = rows.first().map(|row| cells(doc, *row).len()).unwrap_or(0);
    (rows.len(), cols)
}

fn kind_count(doc: &Document, kind: BlockKind) -> usize {
    doc.preorder()
        .into_iter()
        .filter(|&id| doc.arena.get(id).map(|n| n.kind) == Some(kind))
        .count()
}

fn leaf_named(doc: &Document, text: &str) -> u32 {
    doc.text_leaves()
        .into_iter()
        .find(|&id| doc.text_of(id) == Some(text))
        .unwrap_or_else(|| panic!("leaf {text:?}"))
}

fn apply_op(doc: &mut Document, block: u32, op: TableOp) -> Caret {
    apply(doc, Sel::collapsed(caret(block, 0)), Command::Table(op))
}

fn assert_grid(doc: &Document, n_rows: usize, n_cols: usize) {
    let table = first_table(doc);
    let packed = match doc.extra(table) {
        NodeExtra::Table { alignments, .. } => alignments,
        other => panic!("table extra {other:?}"),
    };
    let rows = rows(doc, table);
    assert_eq!(rows.len(), n_rows, "{}", doc.to_markdown());
    for (i, row) in rows.iter().copied().enumerate() {
        assert_eq!(cells(doc, row).len(), n_cols, "row {i}");
        let header = i == 0;
        assert_eq!(
            doc.extra(row).table_header(),
            header,
            "row {i} header extra"
        );
        for (ci, cell) in cells(doc, row).into_iter().enumerate() {
            match doc.extra(cell) {
                NodeExtra::Cell {
                    header: h, align, ..
                } => {
                    assert_eq!(h, header, "cell {i},{ci} header");
                    assert_eq!(
                        align,
                        TableCellAlign::from(alignment_at(packed, ci)),
                        "cell {i},{ci} align"
                    );
                }
                other => panic!("cell extra {other:?}"),
            }
        }
    }
}

#[test]
fn insert_replaces_empty_paragraph_with_2x2() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let out = apply_op(&mut doc, leaf, TableOp::Insert { rows: 2, cols: 2 });
    assert_eq!(dims(&doc), (2, 2));
    assert_grid(&doc, 2, 2);
    assert_eq!(doc.kind(out.block), Some(BlockKind::TableCell));
    assert_eq!(out.offset, 0);
    assert_eq!(doc.to_markdown(), "|  |  |\n| --- | --- |\n|  |  |\n");
    let table = first_table(&doc);
    let first = cells(&doc, rows(&doc, table)[0])[0];
    assert_eq!(out.block, first.index);
}

#[test]
fn insert_after_nonempty_paragraph_keeps_text() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let out = apply_op(&mut doc, leaf, TableOp::Insert { rows: 2, cols: 2 });
    assert_eq!(doc.text_of(leaf).unwrap(), "hello");
    assert_eq!(kind_count(&doc, BlockKind::Table), 1);
    assert_eq!(dims(&doc), (2, 2));
    assert_eq!(doc.kind(out.block), Some(BlockKind::TableCell));
    let md = doc.to_markdown();
    assert!(md.contains("hello"), "{md:?}");
    assert!(md.contains("| --- | --- |"), "{md:?}");
}

#[test]
fn insert_inside_table_is_noop() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let cell = doc.text_leaves()[0];
    let rev = doc.revision;
    let _ = doc.take_changes();
    let out = apply_op(&mut doc, cell, TableOp::Insert { rows: 3, cols: 4 });
    assert_eq!(out.block, cell);
    assert_eq!(doc.revision, rev);
    assert!(doc.take_changes().is_empty());
    assert_eq!(dims(&doc), (2, 2));
}

#[test]
fn editing_a_cell_preserves_inline_source_and_marks() {
    let mut doc = load_markdown(
        "| value | other |\n| --- | --- |\n| **bold** [link](https://example.com) `code` | x |\n",
        editor_options(),
    );
    let cell = leaf_named(&doc, "bold link code");
    let at = doc.text_of(cell).expect("cell text").len();
    let _ = type_chars(&mut doc, caret(cell, at), "!");

    let id = doc.live_id(cell).expect("cell remains live");
    let source = doc.leaf_source(id);
    assert!(source.contains("**bold**"), "{source:?}");
    assert!(source.contains("[link](https://example.com)"), "{source:?}");
    assert!(source.contains("`code`"), "{source:?}");
    assert!(
        doc.runs(id)
            .iter()
            .any(|run| run.marks.contains(InlineMarks::STRONG))
    );
    assert!(
        doc.runs(id)
            .iter()
            .any(|run| run.marks.contains(InlineMarks::CODE))
    );

    let markdown = doc.to_markdown();
    let again = load_markdown(&markdown, editor_options());
    let cell = leaf_named(&again, "bold link code!");
    let id = again.live_id(cell).expect("reloaded cell");
    assert!(
        again
            .runs(id)
            .iter()
            .any(|run| run.marks.contains(InlineMarks::STRONG))
    );
    assert!(
        again
            .runs(id)
            .iter()
            .any(|run| run.marks.contains(InlineMarks::CODE))
    );
    assert_eq!(
        again.link_at(id, "bold ".len()),
        Some("https://example.com")
    );
}

#[test]
fn fence_line_break_in_a_cell_keeps_the_row_rectangular() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let cell = leaf_named(&doc, "c");
    let _ = doc.replace_text(cell, 0..1, "```");
    let out = apply(&mut doc, Sel::collapsed(caret(cell, 3)), Command::Break);

    assert_eq!(out, caret(cell, 3));
    assert_eq!(doc.kind(cell), Some(BlockKind::TableCell));
    assert_eq!(doc.text_of(cell), Some("```"));
    assert_eq!(dims(&doc), (2, 2));
    let table = first_table(&doc);
    for row in rows(&doc, table) {
        assert!(
            cells(&doc, row)
                .into_iter()
                .all(|child| doc.arena.get(child).map(|node| node.kind)
                    == Some(BlockKind::TableCell))
        );
    }
}

#[test]
fn table_ops_outside_table_are_noop() {
    let mut doc = load_markdown("hello\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let rev = doc.revision;
    let _ = doc.take_changes();
    for op in [
        TableOp::InsertRowBelow,
        TableOp::DeleteRow,
        TableOp::DeleteColumn,
        TableOp::DeleteTable,
        TableOp::MoveRowDown,
        TableOp::MoveColumnRight,
        TableOp::MoveRowTo { index: 1 },
        TableOp::MoveColumnTo { index: 1 },
        TableOp::SetColumnAlign(TableCellAlign::Center),
        TableOp::Resize { rows: 3, cols: 3 },
    ] {
        let out = apply_op(&mut doc, leaf, op);
        assert_eq!(out.block, leaf);
    }
    assert_eq!(doc.revision, rev);
    assert!(doc.take_changes().is_empty());
    assert_eq!(kind_count(&doc, BlockKind::Table), 0);
}

#[test]
fn insert_and_delete_row_keeps_column_count() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let body = leaf_named(&doc, "c");
    let out = apply_op(&mut doc, body, TableOp::InsertRowBelow);
    assert_eq!(dims(&doc), (3, 2));
    assert_grid(&doc, 3, 2);
    assert_eq!(doc.kind(out.block), Some(BlockKind::TableCell));
    assert_eq!(doc.text_of(out.block).unwrap(), "");
    let _ = apply_op(&mut doc, out.block, TableOp::DeleteRow);
    assert_eq!(dims(&doc), (2, 2));
    assert_grid(&doc, 2, 2);
}

#[test]
fn insert_row_above_header_promotes_new_row() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let header = leaf_named(&doc, "a");
    let _ = apply_op(&mut doc, header, TableOp::InsertRowAbove);
    assert_eq!(dims(&doc), (3, 2));
    assert_grid(&doc, 3, 2);
    let table = first_table(&doc);
    let first_cells = cells(&doc, rows(&doc, table)[0]);
    assert!(doc.display(first_cells[0]).is_empty());
    let second = cells(&doc, rows(&doc, table)[1]);
    assert_eq!(doc.display(second[0]), "a");
    assert!(!doc.extra(second[0]).table_header());
}

#[test]
fn insert_and_delete_column_rewrites_separator() {
    let mut doc = load_markdown("| a | b |\n| :---: | ---: |\n| c | d |\n", editor_options());
    let body = leaf_named(&doc, "c");
    let out = apply_op(&mut doc, body, TableOp::InsertColumnRight);
    assert_eq!(dims(&doc), (2, 3));
    assert_grid(&doc, 2, 3);
    let md = doc.to_markdown();
    assert!(md.contains("| :---: | --- | ---: |"), "{md}");
    let next = apply_op(&mut doc, body, TableOp::DeleteColumn);
    assert_eq!(dims(&doc), (2, 2));
    assert_grid(&doc, 2, 2);
    assert!(doc.live_id(next.block).is_some());
    assert_eq!(next.block, out.block);
}

#[test]
fn deleting_from_a_ragged_row_returns_a_surviving_cell() {
    let mut doc = load_markdown(
        "| a | b | c |\n| --- | --- | --- |\n| d | e | f |\n",
        editor_options(),
    );
    let table = first_table(&doc);
    let body = rows(&doc, table)[1];
    let body_cells = cells(&doc, body);
    doc.arena.detach(body_cells[2]);
    let target = body_cells[1];
    let survivor = body_cells[0];

    let path = TablePath::at(&doc, caret(target.index, 0)).expect("table path");
    let out = delete_column_unchecked(&mut doc, path);

    assert_eq!(out, caret(survivor.index, 0));
    assert_eq!(doc.live_id(out.block), Some(survivor));
    assert_eq!(
        doc.arena.get(survivor).and_then(|node| node.parent),
        Some(body)
    );
    assert_eq!(cells(&doc, body), vec![survivor]);
}

#[test]
fn resize_on_a_shape_broken_table_falls_back_to_the_original_caret() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let table = first_table(&doc);
    let cell = cells(&doc, rows(&doc, table)[0])[0];
    let path = TablePath::at(&doc, caret(cell.index, 0)).expect("table path");

    for row in rows(&doc, table) {
        doc.arena.detach(row);
    }

    let out = resize_table_unchecked(&mut doc, path, 2, 2);
    assert_eq!(out, caret(path.cell.index, 0));
}

#[test]
fn retarget_header_ignores_non_cell_row_children() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let table = first_table(&doc);
    let body = rows(&doc, table)[1];
    let paragraph = doc.alloc_leaf(BlockKind::Paragraph);
    doc.arena.append_child(body, paragraph);
    let mut changes = Vec::new();
    retarget_header(&mut doc, table, &mut changes);

    assert_eq!(doc.kind(paragraph.index), Some(BlockKind::Paragraph));
    assert_eq!(doc.extra(paragraph), NodeExtra::None);
}

#[test]
#[should_panic(expected = "table rows must have the same number of cells")]
fn table_operations_assert_rectangular_structure() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let table = first_table(&doc);
    let body = rows(&doc, table)[1];
    let paragraph = doc.alloc_leaf(BlockKind::Paragraph);
    doc.arena.append_child(body, paragraph);
    let cell = cells(&doc, body)[0];

    let _ = apply_op(
        &mut doc,
        cell.index,
        TableOp::SetColumnAlign(TableCellAlign::Center),
    );
}

#[test]
fn move_header_row_down_retargets_header() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let header = leaf_named(&doc, "a");
    let out = apply_op(&mut doc, header, TableOp::MoveRowDown);
    assert_eq!(dims(&doc), (2, 2));
    assert_grid(&doc, 2, 2);
    assert_eq!(out.block, header);
    let table = first_table(&doc);
    let top = cells(&doc, rows(&doc, table)[0]);
    assert_eq!(doc.display(top[0]), "c");
    assert!(doc.extra(top[0]).table_header());
    let bottom = cells(&doc, rows(&doc, table)[1]);
    assert_eq!(doc.display(bottom[0]), "a");
    assert!(!doc.extra(bottom[0]).table_header());
}

#[test]
fn move_column_left_takes_alignment_with_it() {
    let mut doc = load_markdown(
        "| L | C | R |\n| --- | :---: | ---: |\n| a | b | c |\n",
        editor_options(),
    );
    let right = leaf_named(&doc, "R");
    let out = apply_op(&mut doc, right, TableOp::MoveColumnLeft);
    assert_eq!(out.block, right);
    assert_grid(&doc, 2, 3);
    let md = doc.to_markdown();
    assert!(md.contains("| --- | ---: | :---: |"), "{md}");
    let table = first_table(&doc);
    let headers = cells(&doc, rows(&doc, table)[0]);
    assert_eq!(doc.display(headers[0]), "L");
    assert_eq!(doc.display(headers[1]), "R");
    assert_eq!(doc.display(headers[2]), "C");
    assert_eq!(
        doc.extra(headers[1]),
        NodeExtra::Cell {
            align: TableCellAlign::End,
            header: true,
        }
    );
}

#[test]
fn set_column_align_writes_center_separator() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let body = leaf_named(&doc, "d");
    let out = apply_op(
        &mut doc,
        body,
        TableOp::SetColumnAlign(TableCellAlign::Center),
    );
    assert_eq!(out.block, body);
    assert_grid(&doc, 2, 2);
    let md = doc.to_markdown();
    assert!(md.contains("| --- | :---: |"), "{md}");
}

#[test]
fn delete_last_row_or_column_is_noop() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let a = leaf_named(&doc, "a");
    let _ = apply_op(&mut doc, a, TableOp::DeleteRow);
    assert_eq!(dims(&doc), (1, 2));
    let c = leaf_named(&doc, "c");
    let _ = apply_op(&mut doc, c, TableOp::DeleteColumn);
    assert_eq!(dims(&doc), (1, 1));
    assert_grid(&doc, 1, 1);
    let cell = doc
        .text_leaves()
        .into_iter()
        .find(|&id| doc.kind(id) == Some(BlockKind::TableCell))
        .expect("cell");
    let rev = doc.revision;
    let _ = doc.take_changes();
    let _ = apply_op(&mut doc, cell, TableOp::DeleteRow);
    let _ = apply_op(&mut doc, cell, TableOp::DeleteColumn);
    assert_eq!(doc.revision, rev);
    assert!(doc.take_changes().is_empty());
    assert_eq!(dims(&doc), (1, 1));
}

#[test]
fn delete_table_replaces_with_empty_paragraph() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let cell = leaf_named(&doc, "a");
    let out = apply_op(&mut doc, cell, TableOp::DeleteTable);
    assert_eq!(kind_count(&doc, BlockKind::Table), 0);
    assert_eq!(doc.kind(out.block), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(out.block).unwrap(), "");
}

#[test]
fn backspace_at_origin_of_empty_table_deletes_it() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let origin = apply_op(&mut doc, leaf, TableOp::Insert { rows: 2, cols: 2 });
    assert_eq!(dims(&doc), (2, 2));
    let out = apply(&mut doc, Sel::collapsed(origin), Command::DeleteBackward);
    assert_eq!(kind_count(&doc, BlockKind::Table), 0);
    assert_eq!(doc.kind(out.block), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(out.block).unwrap(), "");
}

#[test]
fn backspace_at_origin_keeps_table_if_any_cell_has_text() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let origin = apply_op(&mut doc, leaf, TableOp::Insert { rows: 2, cols: 2 });
    let table = first_table(&doc);
    let other = cells(&doc, rows(&doc, table)[0])[1];
    let _ = doc.replace_text(other.index, 0..0, "x");
    let out = apply(&mut doc, Sel::collapsed(origin), Command::DeleteBackward);
    assert_eq!(kind_count(&doc, BlockKind::Table), 1);
    assert_eq!(out.block, origin.block);
    assert_eq!(doc.text_of(other.index).unwrap(), "x");
}

#[test]
fn backspace_in_empty_table_off_origin_is_noop() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let origin = apply_op(&mut doc, leaf, TableOp::Insert { rows: 2, cols: 2 });
    let next = table_step(&doc, origin, TableStep::NextCell).expect("next");
    let out = apply(&mut doc, Sel::collapsed(next), Command::DeleteBackward);
    assert_eq!(kind_count(&doc, BlockKind::Table), 1);
    assert_eq!(dims(&doc), (2, 2));
    assert_eq!(out, next);
}

#[test]
fn backspace_at_cell_start_in_a_list_does_not_join_cells() {
    let mut doc = load_markdown(
        "- item\n\n  | a | b |\n  | --- | --- |\n  | c | d |\n",
        editor_options(),
    );
    let cell = leaf_named(&doc, "d");
    let before = doc.to_markdown();
    let table = first_table(&doc);
    let before_rows = rows(&doc, table)
        .into_iter()
        .map(|row| cells(&doc, row).len())
        .collect::<Vec<_>>();

    let out = apply(
        &mut doc,
        Sel::collapsed(caret(cell, 0)),
        Command::DeleteBackward,
    );

    assert_eq!(out, caret(cell, 0));
    assert_eq!(doc.to_markdown(), before);
    assert_eq!(
        rows(&doc, table)
            .into_iter()
            .map(|row| cells(&doc, row).len())
            .collect::<Vec<_>>(),
        before_rows
    );
}

#[test]
fn backspace_empty_table_undo_restores_table() {
    let mut d = Doc::new(load_markdown("", editor_options()));
    let leaf = d.text_leaves()[0];
    let origin = d.apply(
        Sel::collapsed(caret(leaf, 0)),
        Command::Table(TableOp::Insert { rows: 2, cols: 2 }),
    );
    let table = first_table(&d.document);
    let _ = d.apply(Sel::collapsed(origin), Command::DeleteBackward);
    assert_eq!(kind_count(&d.document, BlockKind::Table), 0);
    let _ = d.undo().expect("undo");
    assert_eq!(kind_count(&d.document, BlockKind::Table), 1);
    assert_eq!(d.document.live_id(table.index).expect("table"), table);
    let _ = d.redo().expect("redo");
    assert_eq!(kind_count(&d.document, BlockKind::Table), 0);
}

#[test]
fn insert_column_stops_at_packed_width() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let cell = leaf_named(&doc, "a");
    for _ in 0..(TABLE_ALIGN_COLS - 2) {
        let _ = apply_op(&mut doc, cell, TableOp::InsertColumnRight);
    }
    assert_eq!(dims(&doc), (2, TABLE_ALIGN_COLS));
    let rev = doc.revision;
    let _ = doc.take_changes();
    let _ = apply_op(&mut doc, cell, TableOp::InsertColumnRight);
    assert_eq!(doc.revision, rev);
    assert!(doc.take_changes().is_empty());
    assert_eq!(dims(&doc), (2, TABLE_ALIGN_COLS));
}

#[test]
fn overwide_table_column_delete_and_move_are_noops() {
    let cols = TABLE_ALIGN_COLS + 1;
    let header = (0..cols)
        .map(|col| format!("h{col}"))
        .collect::<Vec<_>>()
        .join(" | ");
    let separator = vec!["---"; cols].join(" | ");
    let body = (0..cols)
        .map(|col| format!("b{col}"))
        .collect::<Vec<_>>()
        .join(" | ");
    let source = format!("| {header} |\n| {separator} |\n| {body} |\n");
    let mut doc = load_markdown(&source, editor_options());
    let cell = leaf_named(&doc, &format!("h{}", cols - 1));
    assert_eq!(dims(&doc), (2, cols));

    let before = doc.to_markdown();
    let revision = doc.revision;
    let _ = doc.take_changes();
    let _ = apply_op(&mut doc, cell, TableOp::DeleteColumn);
    assert_eq!(doc.revision, revision);
    assert!(doc.take_changes().is_empty());
    assert_eq!(doc.to_markdown(), before);

    let _ = apply_op(&mut doc, cell, TableOp::MoveColumnLeft);
    assert_eq!(doc.revision, revision);
    assert!(doc.take_changes().is_empty());
    assert_eq!(doc.to_markdown(), before);
}

#[test]
fn insert_table_undo_redo_restores_paragraph() {
    let mut d = Doc::new(load_markdown("", editor_options()));
    let leaf = d.text_leaves()[0];
    let id = d.document.live_id(leaf).expect("live");
    let _ = d.apply(
        Sel::collapsed(caret(leaf, 0)),
        Command::Table(TableOp::Insert { rows: 2, cols: 2 }),
    );
    assert_eq!(kind_count(&d.document, BlockKind::Table), 1);
    let table = first_table(&d.document);
    let table_id = table;
    let _ = d.undo().expect("undo");
    assert_eq!(kind_count(&d.document, BlockKind::Table), 0);
    assert_eq!(d.document.live_id(leaf).expect("para"), id);
    let _ = d.redo().expect("redo");
    assert_eq!(kind_count(&d.document, BlockKind::Table), 1);
    assert_eq!(d.document.live_id(table_id.index).expect("table"), table_id);
}

#[test]
fn insert_row_undo_redo_keeps_cell_ids() {
    let mut d = Doc::new(load_markdown(
        "| a | b |\n| --- | --- |\n| c | d |\n",
        editor_options(),
    ));
    let body = leaf_named(&d.document, "c");
    let id = d.document.live_id(body).expect("live");
    let after = d.apply(
        Sel::collapsed(caret(body, 0)),
        Command::Table(TableOp::InsertRowBelow),
    );
    let new_id = d.document.live_id(after.block).expect("new");
    let _ = d.undo().expect("undo");
    assert_eq!(dims(&d.document), (2, 2));
    assert_eq!(d.document.live_id(body).expect("old"), id);
    let _ = d.redo().expect("redo");
    assert_eq!(dims(&d.document), (3, 2));
    assert_eq!(
        d.document.live_id(after.block).expect("resurrected"),
        new_id
    );
}

#[test]
fn table_step_walks_row_major_and_stops_at_edges() {
    let doc = load_markdown(
        "| a | b |\n| --- | --- |\n| c | d |\n\nafter\n",
        editor_options(),
    );
    let a = leaf_named(&doc, "a");
    let b = leaf_named(&doc, "b");
    let c = leaf_named(&doc, "c");
    let d = leaf_named(&doc, "d");
    assert!(crate::document::in_table(&doc, a));
    assert!(!crate::document::in_table(&doc, leaf_named(&doc, "after")));
    let at_a = caret(a, 0);
    let next = table_step(&doc, at_a, TableStep::NextCell).expect("b");
    assert_eq!(next.block, b);
    assert_eq!(next.offset, 0);
    let next = table_step(&doc, caret(b, 0), TableStep::NextCell).expect("c");
    assert_eq!(next.block, c);
    assert_eq!(next.offset, 0);
    let last = table_step(&doc, caret(c, 0), TableStep::NextCell).expect("d");
    assert_eq!(last.block, d);
    assert!(table_step(&doc, caret(d, 0), TableStep::NextCell).is_none());
    let prev = table_step(&doc, caret(d, 0), TableStep::PrevCell).expect("c");
    assert_eq!(prev.block, c);
    assert_eq!(prev.offset, doc.text_of(c).unwrap().len());
    assert!(table_step(&doc, at_a, TableStep::PrevCell).is_none());
    assert!(table_step(&doc, at_a, TableStep::Above).is_none());
    let below = table_step(&doc, at_a, TableStep::Below).expect("c");
    assert_eq!(below.block, c);
    assert_eq!(below.offset, 0);
    let home = table_step(&doc, caret(b, 0), TableStep::RowHome).expect("home");
    assert_eq!(home.block, a);
    let after = table_step(&doc, at_a, TableStep::ExitAfter).expect("after");
    assert_eq!(doc.text_of(after.block).unwrap(), "after");
    assert_eq!(after.offset, 0);
}

#[test]
fn table_step_exit_before_lands_on_the_previous_block() {
    let doc = load_markdown(
        "before\n\n| a | b |\n| --- | --- |\n| c | d |\n",
        editor_options(),
    );
    let a = leaf_named(&doc, "a");
    let before = table_step(&doc, caret(a, 0), TableStep::ExitBefore).expect("before");
    assert_eq!(doc.text_of(before.block).unwrap(), "before");
    assert_eq!(before.offset, "before".len());
}

#[test]
fn last_cell_insert_row_then_row_home_lands_on_first_cell() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let d = leaf_named(&doc, "d");
    let out = apply_op(&mut doc, d, TableOp::InsertRowBelow);
    assert_eq!(dims(&doc), (3, 2));
    let home = table_step(&doc, out, TableStep::RowHome).expect("home");
    let table = first_table(&doc);
    let first = cells(&doc, rows(&doc, table)[2])[0];
    assert_eq!(home.block, first.index);
    assert_eq!(home.offset, 0);
}

#[test]
fn table_loc_reports_dims_col_and_align() {
    let doc = load_markdown(
        "| a | b |\n| --- | :---: |\n| c | d |\n\nafter\n",
        editor_options(),
    );
    let a = leaf_named(&doc, "a");
    let loc = table_loc(&doc, a).expect("a");
    assert_eq!(loc.table, first_table(&doc).index);
    assert_eq!(loc.rows, 2);
    assert_eq!(loc.cols, 2);
    assert_eq!(loc.row, 0);
    assert_eq!(loc.col, 0);
    assert_eq!(loc.align, TableCellAlign::Start);
    let wrapped = Doc::new(doc.clone());
    assert_eq!(wrapped.table_loc(a), Some(loc));
    let b = leaf_named(&doc, "b");
    let loc = table_loc(&doc, b).expect("b");
    assert_eq!(loc.col, 1);
    assert_eq!(loc.row, 0);
    assert_eq!(loc.align, TableCellAlign::Center);
    assert!(table_loc(&doc, leaf_named(&doc, "after")).is_none());
}

#[test]
fn resize_grows_from_the_tail() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let a = leaf_named(&doc, "a");
    let out = apply_op(&mut doc, a, TableOp::Resize { rows: 4, cols: 3 });
    assert_eq!(dims(&doc), (4, 3));
    assert_grid(&doc, 4, 3);
    assert_eq!(out.block, a);
    assert_eq!(doc.text_of(a), Some("a"));
    assert_eq!(doc.text_of(leaf_named(&doc, "d")), Some("d"));
}

#[test]
fn resize_shrinks_from_the_tail_and_keeps_origin() {
    let mut doc = load_markdown(
        "| a | b | x |\n| :---: | --- | ---: |\n| c | d | y |\n",
        editor_options(),
    );
    let a = leaf_named(&doc, "a");
    let packed_before = match doc.extra(first_table(&doc)) {
        NodeExtra::Table { alignments, .. } => alignments,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        TableCellAlign::from(alignment_at(packed_before, 0)),
        TableCellAlign::Center
    );
    assert_eq!(
        TableCellAlign::from(alignment_at(packed_before, 2)),
        TableCellAlign::End
    );
    let _ = apply_op(&mut doc, a, TableOp::Resize { rows: 1, cols: 1 });
    assert_eq!(dims(&doc), (1, 1));
    assert_grid(&doc, 1, 1);
    assert_eq!(doc.text_of(a), Some("a"));
    let packed = match doc.extra(first_table(&doc)) {
        NodeExtra::Table { alignments, .. } => alignments,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        TableCellAlign::from(alignment_at(packed, 0)),
        TableCellAlign::Center
    );
    assert_eq!(alignment_at(packed, 1), 0);
    assert_eq!(alignment_at(packed, 2), 0);
}

#[test]
fn resize_retargets_a_removed_origin_to_a_surviving_cell() {
    let mut doc = load_markdown(
        "| a | b | x |\n| --- | --- | --- |\n| c | d | y |\n",
        editor_options(),
    );
    let a = leaf_named(&doc, "a");
    let y = leaf_named(&doc, "y");

    let out = apply_op(&mut doc, y, TableOp::Resize { rows: 1, cols: 1 });

    assert_eq!(out, caret(a, 0));
    assert!(table_loc(&doc, out.block).is_some());
    assert_grid(&doc, 1, 1);
}

#[test]
fn resize_is_one_undo_step() {
    let mut d = Doc::new(load_markdown(
        "| a | b |\n| --- | --- |\n| c | d |\n",
        editor_options(),
    ));
    let a = leaf_named(&d.document, "a");
    let before = d.document.to_markdown();
    let _ = d.apply(
        Sel::collapsed(caret(a, 0)),
        Command::Table(TableOp::Resize { rows: 4, cols: 3 }),
    );
    assert_eq!(dims(&d.document), (4, 3));
    let undone = d.undo().expect("undo");
    assert_eq!(dims(&d.document), (2, 2));
    assert_eq!(d.document.to_markdown(), before);
    assert_eq!(undone.head.block, a);
    let _ = d.redo().expect("redo");
    assert_eq!(dims(&d.document), (4, 3));
}

#[test]
fn resize_clamps_to_insert_limits() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let a = leaf_named(&doc, "a");
    let _ = apply_op(
        &mut doc,
        a,
        TableOp::Resize {
            rows: TABLE_PICKER_MAX_ROWS + 4,
            cols: TABLE_PICKER_MAX_COLS + 4,
        },
    );
    assert_eq!(
        dims(&doc),
        (TABLE_PICKER_MAX_ROWS + 4, TABLE_PICKER_MAX_COLS + 4)
    );
    let _ = apply_op(
        &mut doc,
        a,
        TableOp::Resize {
            rows: usize::MAX,
            cols: usize::MAX,
        },
    );
    assert_eq!(dims(&doc), (TABLE_INSERT_MAX_ROWS, TABLE_INSERT_MAX_COLS));
}

fn grid_texts(doc: &Document) -> Vec<Vec<String>> {
    let table = first_table(doc);
    rows(doc, table)
        .into_iter()
        .map(|row| {
            cells(doc, row)
                .into_iter()
                .map(|cell| doc.display(cell).to_string())
                .collect()
        })
        .collect()
}

#[test]
fn move_row_to_skips_middle_in_one_step() {
    let mut d = Doc::new(load_markdown(
        "| h1 | h2 |\n| --- | --- |\n| a | b |\n| c | d |\n| e | f |\n",
        editor_options(),
    ));
    let e = leaf_named(&d.document, "e");
    let before = d.document.to_markdown();
    let after = d.apply(
        Sel::collapsed(caret(e, 0)),
        Command::Table(TableOp::MoveRowTo { index: 0 }),
    );
    assert_eq!(
        grid_texts(&d.document),
        vec![
            vec!["e".to_string(), "f".to_string()],
            vec!["h1".to_string(), "h2".to_string()],
            vec!["a".to_string(), "b".to_string()],
            vec!["c".to_string(), "d".to_string()],
        ]
    );
    assert_grid(&d.document, 4, 2);
    assert_eq!(after.block, e);
    let top = cells(&d.document, rows(&d.document, first_table(&d.document))[0]);
    assert_eq!(d.document.display(top[0]), "e");
    assert!(d.document.extra(top[0]).table_header());
    let undone = d.undo().expect("undo");
    assert_eq!(d.document.to_markdown(), before);
    assert_eq!(undone.head.block, e);
    let _ = d.redo().expect("redo");
    assert_eq!(grid_texts(&d.document)[0][0], "e");
}

#[test]
fn move_header_row_to_middle_retargets_header() {
    let mut doc = load_markdown(
        "| a | b |\n| --- | --- |\n| c | d |\n| e | f |\n",
        editor_options(),
    );
    let header = leaf_named(&doc, "a");
    let out = apply_op(&mut doc, header, TableOp::MoveRowTo { index: 1 });
    assert_eq!(out.block, header);
    assert_eq!(
        grid_texts(&doc),
        vec![
            vec!["c".to_string(), "d".to_string()],
            vec!["a".to_string(), "b".to_string()],
            vec!["e".to_string(), "f".to_string()],
        ]
    );
    assert_grid(&doc, 3, 2);
    let table = first_table(&doc);
    let top = cells(&doc, rows(&doc, table)[0]);
    assert_eq!(doc.display(top[0]), "c");
    assert!(doc.extra(top[0]).table_header());
    let mid = cells(&doc, rows(&doc, table)[1]);
    assert_eq!(doc.display(mid[0]), "a");
    assert!(!doc.extra(mid[0]).table_header());
}

#[test]
fn move_column_to_takes_alignment_and_is_one_undo() {
    let mut d = Doc::new(load_markdown(
        "| L | C | R |\n| --- | :---: | ---: |\n| a | b | c |\n",
        editor_options(),
    ));
    let right = leaf_named(&d.document, "R");
    let before = d.document.to_markdown();
    let after = d.apply(
        Sel::collapsed(caret(right, 0)),
        Command::Table(TableOp::MoveColumnTo { index: 0 }),
    );
    assert_eq!(after.block, right);
    assert_eq!(
        grid_texts(&d.document),
        vec![
            vec!["R".to_string(), "L".to_string(), "C".to_string()],
            vec!["c".to_string(), "a".to_string(), "b".to_string()],
        ]
    );
    let md = d.document.to_markdown();
    assert!(md.contains("| ---: | --- | :---: |"), "{md}");
    assert_grid(&d.document, 2, 3);
    let undone = d.undo().expect("undo");
    assert_eq!(d.document.to_markdown(), before);
    assert_eq!(undone.head.block, right);
}

#[test]
fn move_row_to_same_index_is_noop() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let c = leaf_named(&doc, "c");
    let rev = doc.revision;
    let _ = doc.take_changes();
    let out = apply_op(&mut doc, c, TableOp::MoveRowTo { index: 1 });
    assert_eq!(out.block, c);
    assert_eq!(doc.revision, rev);
    assert!(doc.take_changes().is_empty());
}

#[test]
fn move_to_clamps_past_end() {
    let mut doc = load_markdown(
        "| a | b | x |\n| --- | :---: | ---: |\n| c | d | y |\n| e | f | z |\n",
        editor_options(),
    );
    let a = leaf_named(&doc, "a");
    let out = apply_op(&mut doc, a, TableOp::MoveRowTo { index: 99 });
    assert_eq!(out.block, a);
    assert_eq!(
        grid_texts(&doc),
        vec![
            vec!["c".to_string(), "d".to_string(), "y".to_string()],
            vec!["e".to_string(), "f".to_string(), "z".to_string()],
            vec!["a".to_string(), "b".to_string(), "x".to_string()],
        ]
    );
    assert_grid(&doc, 3, 3);
    let c = leaf_named(&doc, "c");
    let _ = apply_op(&mut doc, c, TableOp::MoveColumnTo { index: 99 });
    assert_eq!(
        grid_texts(&doc),
        vec![
            vec!["d".to_string(), "y".to_string(), "c".to_string()],
            vec!["f".to_string(), "z".to_string(), "e".to_string()],
            vec!["b".to_string(), "x".to_string(), "a".to_string()],
        ]
    );
    let md = doc.to_markdown();
    assert!(md.contains("| :---: | ---: | --- |"), "{md}");
    assert_grid(&doc, 3, 3);
}

#[test]
fn insert_sized_table_replaces_empty_paragraph() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let out = apply_op(&mut doc, leaf, TableOp::Insert { rows: 3, cols: 4 });
    assert_eq!(dims(&doc), (3, 4));
    assert_grid(&doc, 3, 4);
    let table = first_table(&doc);
    let first = cells(&doc, rows(&doc, table)[0])[0];
    assert_eq!(out.block, first.index);
    assert_eq!(out.offset, 0);
}

#[test]
fn insert_clamps_rows_and_cols() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = apply_op(&mut doc, leaf, TableOp::Insert { rows: 1, cols: 0 });
    assert_eq!(dims(&doc), (TABLE_INSERT_MIN_ROWS, TABLE_INSERT_MIN_COLS));
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let _ = apply_op(
        &mut doc,
        leaf,
        TableOp::Insert {
            rows: 10_000,
            cols: 10_000,
        },
    );
    assert_eq!(dims(&doc), (TABLE_INSERT_MAX_ROWS, TABLE_INSERT_MAX_COLS));
}

#[test]
fn parse_pipe_header_splits_unescaped_cells() {
    assert_eq!(
        super::super::table::parse_pipe_header("|a|b|"),
        Some(vec!["a".into(), "b".into()])
    );
    assert_eq!(
        super::super::table::parse_pipe_header("| a | b |"),
        Some(vec!["a".into(), "b".into()])
    );
    assert_eq!(
        super::super::table::parse_pipe_header("|a\\|b|c|"),
        Some(vec!["a|b".into(), "c".into()])
    );
    assert_eq!(
        super::super::table::parse_pipe_header("|a\\\\|b|c|"),
        Some(vec!["a|b".into(), "c".into()])
    );
    assert_eq!(
        super::super::table::parse_pipe_header("|a|"),
        Some(vec!["a".into()])
    );
    assert_eq!(super::super::table::parse_pipe_header("a|b"), None);
    assert_eq!(
        super::super::table::parse_pipe_header("| --- | --- |"),
        None
    );
    assert_eq!(super::super::table::parse_pipe_header("|---|"), None);
    assert_eq!(super::super::table::parse_pipe_header("||"), None);
}

#[test]
fn pipe_header_double_backslash_matches_reload_column_splitting() {
    let source = "|a\\\\|b|c|\n|---|---|\n";
    let loaded = load_markdown(source, editor_options());
    assert_eq!(dims(&loaded), (1, 2));
    let loaded_table = first_table(&loaded);
    let loaded_header = cells(&loaded, rows(&loaded, loaded_table)[0]);
    assert_eq!(
        loaded_header
            .into_iter()
            .map(|cell| loaded.display(cell))
            .collect::<Vec<_>>(),
        vec!["a|b", "c"]
    );

    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = type_chars(&mut doc, caret(leaf, 0), "|a\\\\|b|c|");
    let _ = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_eq!(dims(&doc), (2, 2));
    let table = first_table(&doc);
    let header = cells(&doc, rows(&doc, table)[0]);
    assert_eq!(
        header
            .into_iter()
            .map(|cell| doc.display(cell))
            .collect::<Vec<_>>(),
        vec!["a|b", "c"]
    );

    let reloaded = load_markdown(&doc.to_markdown(), editor_options());
    assert_eq!(dims(&reloaded), (2, 2));
    let reloaded_table = first_table(&reloaded);
    let reloaded_header = cells(&reloaded, rows(&reloaded, reloaded_table)[0]);
    assert_eq!(
        reloaded_header
            .into_iter()
            .map(|cell| reloaded.display(cell))
            .collect::<Vec<_>>(),
        vec!["a|b", "c"]
    );
}

#[test]
fn pipe_header_enter_replaces_paragraph_with_table() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = type_chars(&mut doc, caret(leaf, 0), "|a|b|");
    let out = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_eq!(dims(&doc), (2, 2));
    assert_grid(&doc, 2, 2);
    let table = first_table(&doc);
    let header = cells(&doc, rows(&doc, table)[0]);
    let body = cells(&doc, rows(&doc, table)[1]);
    assert_eq!(doc.text_of(header[0].index).unwrap(), "a");
    assert_eq!(doc.text_of(header[1].index).unwrap(), "b");
    assert_eq!(doc.text_of(body[0].index).unwrap(), "");
    assert_eq!(out.block, body[0].index);
    assert_eq!(out.offset, 0);
}

#[test]
fn pipe_separator_enter_does_not_create_table() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = type_chars(&mut doc, caret(leaf, 0), "| --- | --- |");
    let _ = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_eq!(kind_count(&doc, BlockKind::Table), 0);
}

#[test]
fn pipe_header_without_leading_pipe_stays_paragraph() {
    let mut doc = load_markdown("", editor_options());
    let leaf = doc.text_leaves()[0];
    let at = type_chars(&mut doc, caret(leaf, 0), "a|b");
    let _ = apply(&mut doc, Sel::collapsed(at), Command::Break);
    assert_eq!(kind_count(&doc, BlockKind::Table), 0);
}

#[test]
fn pipe_header_in_list_or_quote_does_not_create_table() {
    for md in ["- |a|b|\n", "> |a|b|\n"] {
        let mut doc = load_markdown(md, editor_options());
        let leaf = doc.text_leaves()[0];
        let n = doc.text_of(leaf).unwrap().len();
        let _ = apply(&mut doc, Sel::collapsed(caret(leaf, n)), Command::Break);
        assert_eq!(kind_count(&doc, BlockKind::Table), 0, "{md:?}");
    }
}

#[test]
fn pipe_header_in_heading_does_not_create_table() {
    let mut doc = load_markdown("# |a|b|\n", editor_options());
    let leaf = doc.text_leaves()[0];
    let n = doc.text_of(leaf).unwrap().len();
    let _ = apply(&mut doc, Sel::collapsed(caret(leaf, n)), Command::Break);
    assert_eq!(kind_count(&doc, BlockKind::Table), 0);
    assert_eq!(doc.kind(leaf), Some(BlockKind::Heading(1)));
}

#[test]
fn pipe_header_enter_undo_restores_paragraph() {
    let mut d = Doc::new(load_markdown("", editor_options()));
    let leaf = d.text_leaves()[0];
    let id = d.document.live_id(leaf).expect("live");
    let mut at = caret(leaf, 0);
    for ch in "|a|b|".chars() {
        at = d.apply(
            Sel::collapsed(at),
            Command::Insert {
                text: ch.to_string(),
            },
        );
    }
    let _ = d.apply(Sel::collapsed(at), Command::Break);
    assert_eq!(kind_count(&d.document, BlockKind::Table), 1);
    let _ = d.undo().expect("undo");
    assert_eq!(kind_count(&d.document, BlockKind::Table), 0);
    assert_eq!(d.document.live_id(leaf).expect("para"), id);
    assert_eq!(d.document.text_of(leaf), Some("|a|b|"));
    let _ = d.redo().expect("redo");
    assert_eq!(kind_count(&d.document, BlockKind::Table), 1);
}

fn span(doc: &Document, a: usize, ao: usize, b: usize, bo: usize) -> Sel {
    let leaves = doc.text_leaves();
    Sel {
        anchor: caret(leaves[a], ao),
        head: caret(leaves[b], bo),
    }
}

#[test]
fn cross_cell_backspace_clears_selected_cell_text() {
    let mut doc = load_markdown(
        "| aa | bb |\n| --- | --- |\n| cc | dd |\n",
        editor_options(),
    );
    let leaves = doc.text_leaves();
    assert_eq!(leaves.len(), 4);
    let sel = span(&doc, 0, 1, 1, 2);
    let out = apply(&mut doc, sel, Command::DeleteBackward);
    assert_eq!(kind_count(&doc, BlockKind::Table), 1);
    assert_eq!(doc.text_of(leaves[0]).unwrap(), "a");
    assert_eq!(doc.text_of(leaves[1]).unwrap(), "");
    assert_eq!(doc.text_of(leaves[2]).unwrap(), "cc");
    assert_eq!(doc.text_of(leaves[3]).unwrap(), "dd");
    assert_eq!(out, caret(leaves[0], 1));
}

#[test]
fn cross_cell_select_whole_table_deletes_it() {
    let mut doc = load_markdown("| a | b |\n| --- | --- |\n| c | d |\n", editor_options());
    let last = doc.text_leaves()[3];
    let end = doc.text_of(last).unwrap().len();
    let sel = span(&doc, 0, 0, 3, end);
    let out = apply(&mut doc, sel, Command::DeleteBackward);
    assert_eq!(kind_count(&doc, BlockKind::Table), 0);
    assert_eq!(doc.kind(out.block), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(out.block).unwrap(), "");
    assert_eq!(out.offset, 0);
}

#[test]
fn select_all_mixed_table_and_prose_deletes_to_empty_paragraph() {
    let mut doc = load_markdown(
        "hello\n\n| a | b |\n| --- | --- |\n| c | d |\n\nworld\n",
        editor_options(),
    );
    let last = *doc.text_leaves().last().expect("last");
    let end = doc.text_of(last).unwrap().len();
    let sel = span(&doc, 0, 0, 5, end);
    let out = apply(&mut doc, sel, Command::DeleteBackward);
    assert_eq!(kind_count(&doc, BlockKind::Table), 0);
    assert_eq!(doc.text_leaves(), vec![out.block]);
    assert_eq!(doc.kind(out.block), Some(BlockKind::Paragraph));
    assert_eq!(doc.text_of(out.block).unwrap(), "");
}

#[test]
fn select_across_table_joins_neighbor_text() {
    let mut doc = load_markdown(
        "hello\n\n| a | b |\n| --- | --- |\n| c | d |\n\nworld\n",
        editor_options(),
    );
    let sel = span(&doc, 0, 2, 5, 2);
    let out = apply(&mut doc, sel, Command::DeleteBackward);
    assert_eq!(kind_count(&doc, BlockKind::Table), 0);
    assert_eq!(doc.text_of(out.block).unwrap(), "herld");
    assert_eq!(out.offset, 2);
    assert_eq!(doc.text_leaves().len(), 1);
}

#[test]
fn select_from_table_into_following_paragraph_drops_the_table() {
    let mut doc = load_markdown(
        "keep\n\n| a | b |\n| --- | --- |\n| c | d |\n\nworld\n",
        editor_options(),
    );
    let keep = doc.text_leaves()[0];
    let sel = span(&doc, 1, 0, 5, 2);
    let out = apply(&mut doc, sel, Command::DeleteBackward);
    assert_eq!(kind_count(&doc, BlockKind::Table), 0);
    assert_eq!(doc.text_of(keep).unwrap(), "keep");
    assert_eq!(doc.text_of(out.block).unwrap(), "rld");
    assert_eq!(out.offset, 0);
}

#[test]
fn partial_table_start_endpoint_keeps_table_and_clears_selected_cells() {
    let mut doc = load_markdown(
        "keep\n\n| aa | bb |\n| --- | --- |\n| cc | dd |\n\nworld\n",
        editor_options(),
    );
    let leaves = doc.text_leaves();
    let sel = span(&doc, 1, 1, 5, 2);
    let out = apply(&mut doc, sel, Command::DeleteBackward);

    assert_eq!(kind_count(&doc, BlockKind::Table), 1);
    assert_eq!(doc.text_of(leaves[1]), Some("a"));
    assert_eq!(doc.text_of(leaves[2]), Some(""));
    assert_eq!(doc.text_of(leaves[3]), Some(""));
    assert_eq!(doc.text_of(leaves[4]), Some(""));
    assert_eq!(doc.text_of(*doc.text_leaves().last().unwrap()), Some("rld"));
    assert_eq!(out, caret(leaves[1], 1));
}

#[test]
fn partial_table_end_endpoint_keeps_table_and_prefix_text() {
    let mut doc = load_markdown(
        "hello\n\n| aa | bb |\n| --- | --- |\n| cc | dd |\n",
        editor_options(),
    );
    let leaves = doc.text_leaves();
    let sel = span(&doc, 0, 2, 2, 1);
    let out = apply(&mut doc, sel, Command::DeleteBackward);

    assert_eq!(kind_count(&doc, BlockKind::Table), 1);
    assert_eq!(doc.text_of(leaves[0]), Some("he"));
    assert_eq!(doc.text_of(leaves[1]), Some(""));
    assert_eq!(doc.text_of(leaves[2]), Some("b"));
    assert_eq!(doc.text_of(leaves[3]), Some("cc"));
    assert_eq!(doc.text_of(leaves[4]), Some("dd"));
    assert_eq!(out, caret(leaves[0], 2));
}

#[test]
fn leading_table_select_all_deletes_to_empty_paragraph() {
    let mut doc = load_markdown(
        "| a | b |\n| --- | --- |\n| c | d |\n\nworld\n",
        editor_options(),
    );
    let last = *doc.text_leaves().last().expect("last");
    let end = doc.text_of(last).unwrap().len();
    let sel = span(&doc, 0, 0, 4, end);
    let out = apply(&mut doc, sel, Command::DeleteBackward);
    assert_eq!(kind_count(&doc, BlockKind::Table), 0);
    assert_eq!(doc.text_leaves(), vec![out.block]);
    assert_eq!(doc.text_of(out.block).unwrap(), "");
}

#[test]
fn trailing_table_select_all_deletes_to_empty_paragraph() {
    let mut doc = load_markdown(
        "hello\n\n| a | b |\n| --- | --- |\n| c | d |\n",
        editor_options(),
    );
    let last = *doc.text_leaves().last().expect("last");
    let end = doc.text_of(last).unwrap().len();
    let sel = span(&doc, 0, 0, 4, end);
    let out = apply(&mut doc, sel, Command::DeleteBackward);
    assert_eq!(kind_count(&doc, BlockKind::Table), 0);
    assert_eq!(doc.text_leaves(), vec![out.block]);
    assert_eq!(doc.text_of(out.block).unwrap(), "");
}

#[test]
fn insert_over_cross_cell_span_lands_in_first_cell() {
    let mut doc = load_markdown(
        "| aa | bb |\n| --- | --- |\n| cc | dd |\n",
        editor_options(),
    );
    let first = doc.text_leaves()[0];
    let sel = span(&doc, 0, 1, 1, 2);
    let out = apply(&mut doc, sel, Command::Insert { text: "x".into() });
    assert_eq!(kind_count(&doc, BlockKind::Table), 1);
    assert_eq!(doc.text_of(first).unwrap(), "ax");
    assert_eq!(doc.text_of(doc.text_leaves()[1]).unwrap(), "");
    assert_eq!(out, caret(first, 2));
}

#[test]
fn mixed_table_delete_undo_restores_table() {
    let mut d = Doc::new(load_markdown(
        "hello\n\n| a | b |\n| --- | --- |\n| c | d |\n\nworld\n",
        editor_options(),
    ));
    let last = *d.text_leaves().last().expect("last");
    let end = d.document.text_of(last).unwrap().len();
    let sel = span(&d.document, 0, 0, 5, end);
    let _ = d.apply(sel, Command::DeleteBackward);
    assert_eq!(kind_count(&d.document, BlockKind::Table), 0);
    let _ = d.undo().expect("undo");
    assert_eq!(kind_count(&d.document, BlockKind::Table), 1);
    assert_eq!(d.document.text_of(d.text_leaves()[0]).unwrap(), "hello");
    let _ = d.redo().expect("redo");
    assert_eq!(kind_count(&d.document, BlockKind::Table), 0);
}

#[test]
fn cross_row_span_clears_middle_cells_and_keeps_table() {
    let mut doc = load_markdown(
        "| aa | bb |\n| --- | --- |\n| cc | dd |\n",
        editor_options(),
    );
    let leaves = doc.text_leaves();
    let sel = span(&doc, 0, 0, 2, 2);
    let out = apply(&mut doc, sel, Command::DeleteBackward);
    assert_eq!(kind_count(&doc, BlockKind::Table), 1);
    assert_eq!(doc.text_of(leaves[0]).unwrap(), "");
    assert_eq!(doc.text_of(leaves[1]).unwrap(), "");
    assert_eq!(doc.text_of(leaves[2]).unwrap(), "");
    assert_eq!(doc.text_of(leaves[3]).unwrap(), "dd");
    assert_eq!(out, caret(leaves[0], 0));
}

#[test]
fn select_across_two_tables_deletes_both() {
    let mut doc = load_markdown(
        "| a | b |\n| --- | --- |\n| c | d |\n\nmid\n\n| e | f |\n| --- | --- |\n| g | h |\n",
        editor_options(),
    );
    let last_i = doc.text_leaves().len() - 1;
    let last = doc.text_leaves()[last_i];
    let end = doc.text_of(last).unwrap().len();
    let sel = span(&doc, 0, 0, last_i, end);
    let out = apply(&mut doc, sel, Command::DeleteBackward);
    assert_eq!(kind_count(&doc, BlockKind::Table), 0);
    assert_eq!(doc.text_leaves(), vec![out.block]);
    assert_eq!(doc.text_of(out.block).unwrap(), "");
}
