use super::geometry::VisibleGeom;
use super::request::Pass;
use super::rows::{PlacedText, clamp_text_range, row_bands};
use crate::snapshot::DeviceRect;
use md_core::Px;
use md_core::inline::{InlineMarks, InlineRun};
use std::ops::Range;

fn code_ranges(runs: &[InlineRun]) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    for r in runs {
        if !r.marks.contains(InlineMarks::CODE) || r.marks.is_syntax() {
            continue;
        }
        let range = r.display_range.start as usize..r.display_range.end as usize;
        if range.start >= range.end {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.end == range.start => last.end = range.end,
            _ => out.push(range),
        }
    }
    out
}

pub(super) fn inline_code_plates_device(pass: &Pass<'_>, geom: &VisibleGeom) -> Vec<DeviceRect> {
    let pad = (
        pass.theme.inline.inline_code_pad_x,
        pass.theme.inline.inline_code_pad_y,
    );
    let mut out = Vec::new();
    let placed = geom
        .texts
        .iter()
        .filter(|t| t.is_display_surface())
        .map(PlacedText::of_text)
        .chain(geom.cells.iter().map(PlacedText::of_cell));
    for p in placed {
        push_code_plates(pass, p, pad, &mut out);
    }
    out
}

fn push_code_plates(pass: &Pass<'_>, p: PlacedText<'_>, pad: (Px, Px), out: &mut Vec<DeviceRect>) {
    let (assembly, shaper, art) = (pass.assembly, pass.shaper, p.art);
    let node = assembly.tree.get(p.box_id);
    let ranges = code_ranges(assembly.tree.runs_of(node));
    if ranges.is_empty() {
        return;
    }
    let (pad_x, pad_y) = pad;
    let (ox, oy) = p.origin;
    for range in ranges {
        let range = clamp_text_range(&assembly.tree, node, range);
        let (a, b) = (range.start, range.end);
        if a >= b {
            continue;
        }
        for band in row_bands(shaper, art, a..b, p.align, p.inner) {
            let w = band.width();
            if w <= 0.0 {
                continue;
            }
            let left = if band.is_first { pad_x } else { 0.0 };
            let right = if band.is_last { pad_x } else { 0.0 };
            let (dy, h) = shaper.caret_ink(node.shape_kind(), node.type_slot(), art, band.row);
            out.push((
                ox + band.start_x - left,
                oy + art.row_top(band.row) + dy - pad_y,
                w + left + right,
                h + pad_y * 2.0,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::code_ranges;
    use md_core::inline::{InlineMarks, InlineRun};

    fn run(range: std::ops::Range<u32>, marks: InlineMarks) -> InlineRun {
        InlineRun {
            display_range: range,
            source_range: None,
            marks,
            link: None,
        }
    }

    #[test]
    fn a_span_split_by_other_marks_stays_one_plate() {
        let runs = vec![
            run(0..3, InlineMarks::NONE),
            run(3..7, InlineMarks::CODE),
            run(7..9, InlineMarks::CODE.union(InlineMarks::STRONG)),
            run(9..12, InlineMarks::NONE),
        ];
        assert_eq!(code_ranges(&runs), vec![3..9]);
    }

    #[test]
    fn separate_spans_get_separate_plates() {
        let runs = vec![
            run(0..2, InlineMarks::CODE),
            run(2..5, InlineMarks::NONE),
            run(5..8, InlineMarks::CODE),
        ];
        assert_eq!(code_ranges(&runs), vec![0..2, 5..8]);
    }

    #[test]
    fn revealed_backticks_stay_outside_the_plate() {
        let runs = vec![
            run(0..1, InlineMarks::CODE.union(InlineMarks::SYNTAX)),
            run(1..5, InlineMarks::CODE),
            run(5..6, InlineMarks::CODE.union(InlineMarks::SYNTAX)),
        ];
        assert_eq!(code_ranges(&runs), vec![1..5]);
    }

    #[test]
    fn a_paragraph_without_code_asks_for_nothing() {
        let runs = vec![run(0..4, InlineMarks::NONE), run(4..8, InlineMarks::EM)];
        assert!(code_ranges(&runs).is_empty());
    }
}
