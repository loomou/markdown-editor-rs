use md_core::Px;
use md_core::block::BlockKind;
use md_layout::box_tree::{BoxChildren, BoxTree, LayoutBoxId, TypeSlot};
use md_layout::compose::{LayoutTheme, compose};

use crate::incremental::engine::IncrementalEngine;
use crate::incremental::estimate::Estimator;
use md_core::document::{editor_options, load_markdown};
use md_layout::shaper::{MeasureKind, MeasureResult, ShapeIdentity, TextMeasure};
use md_layout::style::{BoxDisplay, BoxLayoutStyle, Edges};
use std::cell::Cell;
use std::fmt::Write;

#[derive(Debug, PartialEq)]
enum ChildrenShape {
    Vertical(Vec<LayoutBoxId>),
    Island(Vec<LayoutBoxId>),
    None,
}

#[derive(Debug, PartialEq)]
struct NodeShape {
    key: LayoutBoxId,
    node_id: LayoutBoxId,
    kind: BlockKind,
    parent: Option<LayoutBoxId>,
    children: ChildrenShape,
    text: String,
    edit_source: bool,
    type_slot: TypeSlot,
}

fn tree_shape(tree: &BoxTree) -> Vec<NodeShape> {
    tree.nodes()
        .into_iter()
        .map(|(key, node)| NodeShape {
            key: *key,
            node_id: node.id(),
            kind: node.kind(),
            parent: node.parent(),
            children: match node.children() {
                BoxChildren::Vertical(ids) => ChildrenShape::Vertical(ids.clone()),
                BoxChildren::Island(ids) => ChildrenShape::Island(ids.clone()),
                BoxChildren::None => ChildrenShape::None,
            },
            text: tree.text_of(node).to_string(),
            edit_source: node.edit_source(),
            type_slot: node.type_slot(),
        })
        .collect()
}

pub(super) fn assert_tree_matches_cold(
    engine: &IncrementalEngine,
    doc: &md_core::document::Document,
) {
    let cold = compose(doc, &dummy_layout());
    assert_eq!(tree_shape(&engine.tree), tree_shape(&cold));
}

pub(super) struct CountingMeasure {
    pub(super) calls: Cell<u64>,
}

impl TextMeasure for CountingMeasure {
    fn begin_island(&self) {}

    fn measure(
        &self,
        text: &str,
        _runs: &[md_core::inline::InlineRun],
        avail_width: Px,
        _kind: MeasureKind,
        _block_kind: BlockKind,
        _ident: ShapeIdentity,
    ) -> MeasureResult {
        self.calls.set(self.calls.get() + 1);
        let rows = ((text.len() as f64 * 7.0 / avail_width.max(1.0)).ceil() as u32).max(1);
        MeasureResult {
            width: avail_width,
            height: f64::from(rows) * 20.0,
            rows,
            first_baseline: 16.0,
        }
    }
}

pub(super) fn loaded(md: &str) -> md_core::document::Document {
    let mut doc = load_markdown(md, editor_options());
    let _ = doc.take_changes();
    doc
}

pub(super) fn dummy_layout() -> LayoutTheme {
    LayoutTheme::from_resolver(|_| BoxLayoutStyle {
        display: BoxDisplay::FlowStack,
        margin: Edges::ZERO,
        padding: Edges::ZERO,
        border: Edges::ZERO,
        gap: 0.0,
    })
}

pub(super) fn estimator() -> Estimator {
    Estimator {
        line_height: 20.0,
        em_width: 16.0,
        heading1_mult: 1.0,
        heading_mult: 1.0,
        table_row_mult: 1.2,
        mermaid_max_height: 420.0,
        image_placeholder_height: 80.0,
        code_max_height: 420.0,
        math_max_height: 420.0,
        image_max_height: 720.0,
    }
}

pub(super) fn long_doc() -> md_core::document::Document {
    let mut md = String::new();
    for i in 0..200 {
        let _ = writeln!(md, "paragraph {i} {}\n", "word ".repeat(8));
    }
    loaded(&md)
}

pub(super) fn long_list() -> md_core::document::Document {
    let mut md = String::new();
    for i in 0..80 {
        let _ = writeln!(md, "- item {i} {}", "word ".repeat(8));
    }
    loaded(&md)
}

pub(super) fn tall_doc() -> md_core::document::Document {
    let mut md = String::new();
    for i in 0..80 {
        let _ = writeln!(md, "paragraph {i} {}\n", "word ".repeat(120));
    }
    loaded(&md)
}

pub(super) struct HalfMeasure;

impl TextMeasure for HalfMeasure {
    fn begin_island(&self) {}

    fn measure(
        &self,
        text: &str,
        _runs: &[md_core::inline::InlineRun],
        avail_width: Px,
        _kind: MeasureKind,
        _block_kind: BlockKind,
        _ident: ShapeIdentity,
    ) -> MeasureResult {
        let est = ((text.len() as f64 * 7.0 / avail_width.max(1.0)).ceil() as u32).max(1);
        let rows = (est / 2).max(1);
        MeasureResult {
            width: avail_width,
            height: f64::from(rows) * 20.0,
            rows,
            first_baseline: 16.0,
        }
    }
}
