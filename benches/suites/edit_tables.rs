use std::time::Duration;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use md_bench::corpus::{self, Shape};
use md_bench::harness;
use md_core::block::TableCellAlign;
use md_core::document::{Command, TableOp};

const SEED: u64 = 0x5eed_0f17;
const SIZE: usize = 16 << 10;
const SMALL_ROWS: usize = 5;
const SMALL_COLS: usize = 5;
const BIG_ROWS: usize = 200;
const BIG_COLS: usize = 8;

fn config() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_millis(1000))
        .measurement_time(Duration::from_secs(2))
        .sample_size(50)
}

fn ops() -> Vec<(&'static str, TableOp)> {
    vec![
        ("insert-row-above", TableOp::InsertRowAbove),
        ("insert-row-below", TableOp::InsertRowBelow),
        ("move-row-up", TableOp::MoveRowUp),
        ("move-row-down", TableOp::MoveRowDown),
        ("insert-col-left", TableOp::InsertColumnLeft),
        ("insert-col-right", TableOp::InsertColumnRight),
        ("move-col-left", TableOp::MoveColumnLeft),
        ("move-col-right", TableOp::MoveColumnRight),
        ("move-row-to-0", TableOp::MoveRowTo { index: 0 }),
        ("move-col-to-0", TableOp::MoveColumnTo { index: 0 }),
        ("delete-row", TableOp::DeleteRow),
        ("delete-column", TableOp::DeleteColumn),
        ("delete-table", TableOp::DeleteTable),
        (
            "set-column-align",
            TableOp::SetColumnAlign(TableCellAlign::Center),
        ),
        ("resize-4x4", TableOp::Resize { rows: 4, cols: 4 }),
    ]
}

fn assert_op_changes(text: &str, name: &str, op: TableOp, row: usize, col: usize) {
    let mut doc = harness::doc(text);
    let cell = harness::table_cell(&doc, row, col).expect("the fixture must hold the cell");
    let before = doc.document.revision();
    doc.apply(harness::caret(cell, 0), Command::Table(op));
    assert!(
        doc.document.revision() > before,
        "{name}: the table operation did not change the document at row {row} column {col}"
    );
}

fn ops_at(c: &mut Criterion, group_name: &str, rows: usize, cols: usize, row: usize, col: usize) {
    let text = corpus::table(rows, cols, SEED);
    let cell = {
        let doc = harness::doc(&text);
        harness::table_cell(&doc, row, col).expect("the fixture must hold the cell")
    };
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    for (name, op) in ops() {
        assert_op_changes(&text, name, op, row, col);
        group.bench_with_input(BenchmarkId::from_parameter(name), &text, |b, text| {
            b.iter_batched_ref(
                || harness::doc(text),
                |doc| doc.apply(harness::caret(cell, 0), Command::Table(op)),
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

fn in_table(c: &mut Criterion) {
    ops_at(
        c,
        "edit_tables/5x5",
        SMALL_ROWS,
        SMALL_COLS,
        SMALL_ROWS / 2,
        SMALL_COLS / 2,
    );
}

fn scale(c: &mut Criterion) {
    ops_at(
        c,
        "edit_tables/200x8",
        BIG_ROWS,
        BIG_COLS,
        BIG_ROWS / 2,
        BIG_COLS / 2,
    );
}

fn create_table(c: &mut Criterion) {
    let text = corpus::generate(Shape::PlainEn, SIZE, SEED);
    harness::assert_fixture(&text, "plain-en");
    let leaf = {
        let doc = harness::doc(&text);
        harness::first_leaf(&doc)
    };
    let mut group = c.benchmark_group("edit_tables/create");
    group.throughput(Throughput::Elements(1));
    for (name, op) in [
        ("insert-2x3", TableOp::Insert { rows: 2, cols: 3 }),
        ("insert-10x8", TableOp::Insert { rows: 10, cols: 8 }),
    ] {
        group.bench_with_input(BenchmarkId::from_parameter(name), &text, |b, text| {
            b.iter_batched_ref(
                || harness::doc(text),
                |doc| doc.apply(harness::caret(leaf, 0), Command::Table(op)),
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = config();
    targets = in_table, scale, create_table
}
criterion_main!(benches);
