use super::bands::line_max_width;
use crate::math::MathMetrics;
use gpui::{Hsla, WrappedLine};
use md_core::Px;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub struct ShapeMedia {
    pub mermaid_fitted: Rc<HashMap<(u32, u32), (f32, f32)>>,
    pub math_metrics: Rc<MathMetrics>,
    pub math_gen: u64,
    pub image_sizes: Rc<HashMap<crate::images::SourceKey, (u32, u32)>>,

    pub image_failed: Rc<HashSet<crate::images::SourceKey>>,
    pub image_gen: u64,
    pub link_dests: Rc<HashMap<u32, String>>,

    pub link_raw: Rc<HashMap<u32, (String, String)>>,
    pub block_image_dest: Rc<HashMap<u32, String>>,
    pub block_code_lang: Rc<HashMap<u32, String>>,
}

#[derive(Clone)]
pub struct ShapeBand {
    pub height: Px,
    pub text_dy: Px,
    pub parts: Vec<ShapePart>,
}

#[derive(Clone)]
pub enum ShapePart {
    Text {
        line: Box<WrappedLine>,
        x: Px,
        byte_start: usize,
        byte_end: usize,
        dy: Px,
    },
    Math {
        x: Px,
        width: Px,
        paint_y: Px,
        latex: String,
        display: bool,
        em: f32,
        slot_h: Px,
        byte_start: usize,
        byte_end: usize,

        fallback: Option<Box<WrappedLine>>,
    },
    Image {
        x: Px,
        width: Px,
        paint_y: Px,
        dest: String,
        slot_w: Px,
        slot_h: Px,
        byte_start: usize,
        byte_end: usize,

        fallback: Option<Box<WrappedLine>>,
    },
}

#[derive(Clone)]
pub struct ShapeArtifact {
    pub lines: Vec<WrappedLine>,
    pub(crate) row_map: Vec<(usize, u32)>,
    pub(crate) line_lens: Vec<usize>,
    pub(crate) line_starts: Vec<usize>,
    pub line_colors: Vec<Vec<(u32, Hsla)>>,
    pub bands: Vec<ShapeBand>,
    pub rows: u32,
    pub height: Px,
    pub first_baseline: Px,
    pub row_advance: Px,
    pub max_line_width: Px,

    pub tab_source: Option<Rc<str>>,
}

impl ShapeArtifact {
    pub fn plain(
        lines: Vec<WrappedLine>,
        rows: u32,
        height: Px,
        first_baseline: Px,
        row_advance: Px,
    ) -> Self {
        let max_line_width = line_max_width(&lines);
        let row_map = build_row_map(
            lines
                .iter()
                .map(|line| line.wrap_boundaries.len() as u32 + 1),
        );
        let line_lens: Vec<usize> = lines.iter().map(|line| line.unwrapped_layout.len).collect();
        let line_starts = build_line_starts(&line_lens);
        debug_assert!(
            row_map.len() == rows as usize || (lines.is_empty() && row_map.is_empty()),
            "visual row map must cover every shaped text row"
        );
        ShapeArtifact {
            lines,
            row_map,
            line_lens,
            line_starts,
            line_colors: Vec::new(),
            bands: Vec::new(),
            rows,
            height,
            first_baseline,
            row_advance,
            max_line_width,
            tab_source: None,
        }
    }

    pub fn row_top(&self, row: u32) -> Px {
        if self.bands.is_empty() {
            return row as Px * self.row_advance;
        }
        self.bands.iter().take(row as usize).map(|b| b.height).sum()
    }

    pub fn row_height(&self, row: u32) -> Px {
        if self.bands.is_empty() {
            return self.row_advance;
        }
        self.bands
            .get(row as usize)
            .map(|b| b.height)
            .unwrap_or(self.row_advance)
    }

    pub(crate) fn weight_bytes(&self) -> usize {
        let mut n = wrapped_lines_bytes(&self.lines);
        n = n.saturating_add(
            self.row_map
                .len()
                .saturating_mul(std::mem::size_of::<(usize, u32)>()),
        );
        n = n.saturating_add(
            self.line_lens
                .len()
                .saturating_mul(std::mem::size_of::<usize>()),
        );
        n = n.saturating_add(
            self.line_starts
                .len()
                .saturating_mul(std::mem::size_of::<usize>()),
        );
        for colors in &self.line_colors {
            n = n.saturating_add(colors.len().saturating_mul(8));
        }
        for band in &self.bands {
            for part in &band.parts {
                n = n.saturating_add(part_weight_bytes(part));
            }
        }
        if let Some(tab) = &self.tab_source {
            n = n.saturating_add(tab.len());
        }
        n.max(1)
    }
}

fn wrapped_line_bytes(line: &WrappedLine) -> usize {
    let text = line.unwrapped_layout.len;
    let runs = line.unwrapped_layout.runs.len();
    let wraps = line.wrap_boundaries.len();
    text.saturating_mul(64)
        .saturating_add(runs.saturating_mul(128))
        .saturating_add(wraps.saturating_mul(32))
        .saturating_add(256)
}

fn wrapped_lines_bytes(lines: &[WrappedLine]) -> usize {
    lines.iter().map(wrapped_line_bytes).sum()
}

fn part_weight_bytes(part: &ShapePart) -> usize {
    match part {
        ShapePart::Text { line, .. } => wrapped_line_bytes(line),
        ShapePart::Math {
            latex, fallback, ..
        } => latex.len().saturating_add(
            fallback
                .as_ref()
                .map(|line| wrapped_line_bytes(line))
                .unwrap_or(0),
        ),
        ShapePart::Image { dest, fallback, .. } => dest.len().saturating_add(
            fallback
                .as_ref()
                .map(|line| wrapped_line_bytes(line))
                .unwrap_or(0),
        ),
    }
}

fn build_row_map(row_counts: impl IntoIterator<Item = u32>) -> Vec<(usize, u32)> {
    let mut rows = Vec::new();
    for (line, count) in row_counts.into_iter().enumerate() {
        rows.extend((0..count).map(|local| (line, local)));
    }
    rows
}

fn build_line_starts(line_lens: &[usize]) -> Vec<usize> {
    let mut starts = Vec::with_capacity(line_lens.len());
    let mut start = 0usize;
    for (line, &len) in line_lens.iter().enumerate() {
        starts.push(start);
        start = start
            .saturating_add(len)
            .saturating_add((line + 1 < line_lens.len()) as usize);
    }
    starts
}

pub(crate) fn locate_hard_offset(
    line_starts: &[usize],
    line_lens: &[usize],
    offset: usize,
) -> (usize, usize) {
    if line_lens.is_empty() || line_starts.is_empty() {
        return (0, 0);
    }
    let line = line_starts
        .partition_point(|&start| start <= offset)
        .saturating_sub(1);
    let line = line.min(line_lens.len() - 1);
    let local = offset.saturating_sub(line_starts[line]);
    (line, local.min(line_lens[line]))
}

pub(crate) fn offset_on_hard_line(
    line_starts: &[usize],
    line_lens: &[usize],
    line: usize,
    local: usize,
) -> usize {
    if line_lens.is_empty() || line_starts.is_empty() {
        return 0;
    }
    let i = line.min(line_lens.len() - 1);
    line_starts[i] + local.min(line_lens[i])
}

#[cfg(test)]
mod row_map_tests {
    use super::build_row_map;

    #[test]
    fn visual_rows_map_to_hard_lines_and_wrap_rows() {
        assert_eq!(
            build_row_map([1, 3, 2]),
            vec![(0, 0), (1, 0), (1, 1), (1, 2), (2, 0), (2, 1)]
        );
    }
}
