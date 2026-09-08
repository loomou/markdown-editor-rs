use crate::snapshot::{CellPiece, TextPiece};
use md_content::shaper::{GpuiShaper, ShapeArtifact};
use md_core::Px;
use md_core::block::BlockId;
use md_core::inline::InlineAlign;
use md_layout::box_tree::{BoxNode, BoxTree, LayoutBoxId};
use std::ops::Range;

pub(super) fn clamp_text_range(
    tree: &BoxTree,
    node: &BoxNode,
    range: Range<usize>,
) -> Range<usize> {
    let len = tree.text_of(node).len();
    range.start.min(len)..range.end.min(len)
}

#[derive(Clone, Copy)]
pub(super) struct PlacedText<'a> {
    pub(super) box_id: LayoutBoxId,
    pub(super) block: BlockId,
    pub(super) origin: (Px, Px),
    pub(super) align: InlineAlign,
    pub(super) inner: Px,
    pub(super) art: &'a ShapeArtifact,
    pub(super) edit_source: bool,
}

impl<'a> PlacedText<'a> {
    pub(super) fn of_text(t: &'a TextPiece) -> PlacedText<'a> {
        PlacedText {
            box_id: t.box_id,
            block: t.block,
            origin: t.content_origin_device,
            align: t.align,
            inner: t.content_width,
            art: t.art.as_ref(),
            edit_source: t.edit_source,
        }
    }

    pub(super) fn of_cell(c: &'a CellPiece) -> PlacedText<'a> {
        PlacedText {
            box_id: c.cell_box,
            block: c.block,
            origin: c.content_origin_device,
            align: c.align,
            inner: c.content_width,
            art: c.art.as_ref(),
            edit_source: false,
        }
    }
}

pub(super) struct RowBand {
    pub(super) row: u32,
    pub(super) start_x: Px,
    pub(super) end_x: Px,

    pub(super) is_first: bool,
    pub(super) is_last: bool,
}

impl RowBand {
    pub(super) fn width(&self) -> Px {
        (self.end_x - self.start_x).max(0.0)
    }
}

pub(super) fn row_bands(
    shaper: &GpuiShaper,
    art: &ShapeArtifact,
    range: Range<usize>,
    align: InlineAlign,
    inner: Px,
) -> RowBands {
    let (x0, first) = shaper.position_for_offset(art, range.start, align, inner);
    let (x1, last) = shaper.position_for_offset(art, range.end, align, inner);
    let bounds = if first > last {
        Vec::new()
    } else {
        (first..=last)
            .map(|row| {
                (
                    shaper.row_content_start(art, row, align, inner),
                    shaper.row_content_end(art, row, align, inner),
                )
            })
            .collect()
    };
    RowBands {
        row: first,
        first,
        last,
        x0,
        x1,
        bounds,
        done: first > last,
    }
}

pub(super) struct RowBands {
    row: u32,
    first: u32,
    last: u32,
    x0: Px,
    x1: Px,
    bounds: Vec<(Px, Px)>,
    done: bool,
}

impl Iterator for RowBands {
    type Item = RowBand;

    fn next(&mut self) -> Option<RowBand> {
        if self.done {
            return None;
        }
        let row = self.row;
        let is_first = row == self.first;
        let is_last = row == self.last;
        let (ink_start, ink_end) = self
            .bounds
            .get((row - self.first) as usize)
            .copied()
            .unwrap_or((self.x0, self.x1));
        let (start_x, end_x) = match (is_first, is_last) {
            (true, true) => (self.x0, self.x1),
            (true, false) => (self.x0, ink_end),
            (false, true) => (ink_start, self.x1),
            (false, false) => (ink_start, ink_end),
        };
        if is_last {
            self.done = true;
        } else {
            self.row += 1;
        }
        Some(RowBand {
            row,
            start_x,
            end_x,
            is_first,
            is_last,
        })
    }
}
