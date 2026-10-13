use md_core::Px;
use md_core::block::{BlockId, BlockKind};
use md_core::doc::Doc;
use md_core::document::{Caret, Command, Sel, editor_options, load_markdown};
use md_core::inline::InlineRun;
use md_layout::compose::LayoutTheme;
use md_layout::shaper::{MeasureKind, MeasureResult, ShapeIdentity, TextMeasure};
use md_layout::style::BoxLayoutEnvironment;
use md_theme::DocumentTheme;

pub fn doc(text: &str) -> Doc {
    let mut doc = Doc::new(load_markdown(text, editor_options()));
    doc.enable_trailing_blank();
    doc
}

pub fn document_theme() -> DocumentTheme {
    DocumentTheme::one_dark()
}

pub fn layout_theme() -> LayoutTheme {
    document_theme().layout_theme()
}

pub fn environment() -> BoxLayoutEnvironment {
    BoxLayoutEnvironment::default()
}

pub struct StubMeasure;

impl TextMeasure for StubMeasure {
    fn begin_island(&self) {}

    fn measure(
        &self,
        text: &str,
        _runs: &[InlineRun],
        avail_width: Px,
        _kind: MeasureKind,
        _block_kind: BlockKind,
        _ident: ShapeIdentity,
    ) -> MeasureResult {
        MeasureResult {
            width: (text.chars().count() as Px * 8.0).min(avail_width.max(0.0)),
            height: 20.0,
            rows: 1,
            first_baseline: 15.0,
        }
    }
}

pub fn caret(leaf: BlockId, offset: usize) -> Sel {
    Sel::collapsed(Caret {
        block: leaf,
        offset,
    })
}

pub fn leaf_end(doc: &Doc, leaf: BlockId) -> usize {
    doc.caret_text(leaf).map(str::len).unwrap_or(0)
}

pub fn leaf_count(doc: &Doc) -> usize {
    doc.text_leaves().len()
}

pub fn first_leaf(doc: &Doc) -> BlockId {
    doc.first_text_leaf().unwrap_or(0)
}

pub fn first_in_table(doc: &Doc) -> Option<BlockId> {
    doc.text_leaves()
        .into_iter()
        .find(|&leaf| doc.in_table(leaf))
}

pub fn table_cell(doc: &Doc, row: usize, col: usize) -> Option<BlockId> {
    let leaves = doc.text_leaves();
    let start = leaves.iter().position(|&leaf| {
        doc.table_loc(leaf)
            .is_some_and(|loc| loc.row == 0 && loc.col == 0)
    })?;
    let cols = doc.table_loc(leaves[start])?.cols;
    leaves.get(start + row * cols + col).copied()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    DocStart,
    DocMiddle,
    DocEnd,
    Whole,
}

impl Spot {
    pub const ALL: [Spot; 4] = [Spot::DocStart, Spot::DocMiddle, Spot::DocEnd, Spot::Whole];

    pub fn name(self) -> &'static str {
        match self {
            Spot::DocStart => "doc-start",
            Spot::DocMiddle => "doc-middle",
            Spot::DocEnd => "doc-end",
            Spot::Whole => "whole-doc",
        }
    }

    pub fn sel(self, doc: &Doc) -> Sel {
        let leaves = doc.text_leaves();
        let first = leaves.first().copied().unwrap_or(0);
        match self {
            Spot::DocStart => caret(first, 0),
            Spot::DocMiddle => {
                let leaf = leaves.get(leaves.len() / 2).copied().unwrap_or(first);
                let offset = leaf_end(doc, leaf) / 2;
                caret(leaf, offset)
            }
            Spot::DocEnd => {
                let leaf = leaves.last().copied().unwrap_or(first);
                let offset = leaf_end(doc, leaf);
                caret(leaf, offset)
            }
            Spot::Whole => doc.whole_document_sel().unwrap_or_else(|| caret(first, 0)),
        }
    }
}

pub fn assert_fixture(text: &str, label: &str) {
    assert!(!text.is_empty(), "{label}: the corpus is empty");
    let doc = doc(text);
    assert!(
        leaf_count(&doc) > 0,
        "{label}: the corpus parsed to no text leaves"
    );
    assert!(
        !doc.document.to_markdown().is_empty(),
        "{label}: the corpus serialized to nothing"
    );
}

pub fn assert_editable(text: &str, spot: Spot) {
    let mut doc = doc(text);
    let before = doc.document.revision();
    let sel = spot.sel(&doc);
    doc.apply(
        sel,
        Command::Insert {
            text: "x".to_owned(),
        },
    );
    assert!(
        doc.document.revision() > before,
        "{}: the fixture did not change, so the benchmark would measure nothing",
        spot.name()
    );
}

pub fn change_count(doc: &mut Doc) -> usize {
    doc.take_changes().changes.len()
}
