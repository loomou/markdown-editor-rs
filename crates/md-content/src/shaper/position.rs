use super::GpuiShaper;
use super::artifact::{ShapeArtifact, ShapePart, locate_hard_offset, offset_on_hard_line};
use super::tabs::{exp_to_orig, orig_to_exp};
use gpui::{WrapBoundary, WrappedLine, px};
use md_core::Px;
use md_core::inline::InlineAlign;

fn pixels_to_px(p: gpui::Pixels) -> Px {
    f32::from(p) as Px
}

pub(crate) fn align_shift(align: InlineAlign, align_width: Px, row_width: Px) -> Px {
    match align {
        InlineAlign::Start => 0.0,
        InlineAlign::Center => (align_width - row_width) / 2.0,
        InlineAlign::End => align_width - row_width,
    }
}

fn wrap_boundary_x(line: &WrappedLine, b: &WrapBoundary) -> Px {
    line.unwrapped_layout
        .runs
        .get(b.run_ix)
        .and_then(|run| run.glyphs.get(b.glyph_ix))
        .map(|g| pixels_to_px(g.position.x))
        .unwrap_or(0.0)
}

fn row_ink_width(art: &ShapeArtifact, row: u32) -> Px {
    art.row_map
        .get(row as usize)
        .map(|&(line, local)| wrap_row_width(&art.lines[line], local))
        .unwrap_or(0.0)
}

fn band_width(band: &super::artifact::ShapeBand, align: InlineAlign) -> Px {
    let mut end: Px = 0.0;
    for part in &band.parts {
        match part {
            ShapePart::Text {
                line,
                x,
                byte_start,
                byte_end,
                ..
            } => {
                if byte_start == byte_end {
                    continue;
                }
                let n = line.wrap_boundaries.len() as u32 + 1;
                for local in 0..n {
                    let dx = line_align_shift(line, align, local);
                    end = end.max(*x + dx + wrap_row_width(line, local));
                }
            }
            ShapePart::Math {
                x, width, fallback, ..
            }
            | ShapePart::Image {
                x, width, fallback, ..
            } => {
                if let Some(line) = fallback {
                    let n = line.wrap_boundaries.len() as u32 + 1;
                    for local in 0..n {
                        let dx = line_align_shift(line, align, local);
                        end = end.max(*x + dx + wrap_row_width(line, local));
                    }
                } else {
                    end = end.max(*x + *width);
                }
            }
        }
    }
    end
}

pub fn band_align_shift(art: &ShapeArtifact, row: u32, align: InlineAlign, inner: Px) -> Px {
    art.bands
        .get(row as usize)
        .map(|band| align_shift(align, inner, band_width(band, align)))
        .unwrap_or(0.0)
}

fn band_content_end(art: &ShapeArtifact, row: u32, align: InlineAlign, inner: Px) -> Px {
    let Some(band) = art.bands.get(row as usize) else {
        return 0.0;
    };
    band_width(band, align) + band_align_shift(art, row, align, inner)
}

fn band_content_start(art: &ShapeArtifact, row: u32, align: InlineAlign, inner: Px) -> Px {
    let Some(band) = art.bands.get(row as usize) else {
        return 0.0;
    };
    let mut start = Px::INFINITY;
    for part in &band.parts {
        let x = match part {
            ShapePart::Text {
                line,
                x,
                byte_start,
                byte_end,
                ..
            } => {
                if byte_start == byte_end {
                    continue;
                }
                *x + line_align_shift(line, align, 0)
            }
            ShapePart::Math { x, fallback, .. } | ShapePart::Image { x, fallback, .. } => {
                *x + fallback
                    .as_deref()
                    .map(|line| line_align_shift(line, align, 0))
                    .unwrap_or(0.0)
            }
        };
        start = start.min(x);
    }
    let start = if start.is_finite() { start } else { 0.0 };
    start + band_align_shift(art, row, align, inner)
}

fn wrap_row_width(line: &WrappedLine, local_row: u32) -> Px {
    let n = line.wrap_boundaries.len() as u32 + 1;
    let local_row = local_row.min(n.saturating_sub(1));
    let start_x = if local_row == 0 {
        0.0
    } else {
        wrap_boundary_x(line, &line.wrap_boundaries[(local_row - 1) as usize])
    };
    let end_x = match line.wrap_boundaries.get(local_row as usize) {
        Some(b) => wrap_boundary_x(line, b),
        None => pixels_to_px(line.unwrapped_layout.width),
    };
    (end_x - start_x).max(0.0)
}

fn line_align_width(line: &WrappedLine) -> Px {
    match line.wrap_width {
        Some(w) => pixels_to_px(w),
        None => pixels_to_px(line.unwrapped_layout.width),
    }
}

fn line_align_shift(line: &WrappedLine, align: InlineAlign, local_row: u32) -> Px {
    align_shift(
        align,
        line_align_width(line),
        wrap_row_width(line, local_row),
    )
}

fn row_align_dx(art: &ShapeArtifact, row: u32, align: InlineAlign, inner: Px) -> Px {
    if matches!(align, InlineAlign::Start) {
        return 0.0;
    }
    if art.row_map.is_empty() {
        return align_shift(align, inner, 0.0);
    }
    let &(line, local) = art
        .row_map
        .get(row as usize)
        .or_else(|| art.row_map.last())
        .expect("non-empty row map");
    line_align_shift(&art.lines[line], align, local)
}

fn shaped_offset(art: &ShapeArtifact, orig: usize) -> usize {
    match art.tab_source.as_deref() {
        Some(src) => orig_to_exp(src, orig),
        None => orig,
    }
}

fn source_offset(art: &ShapeArtifact, shaped: usize) -> usize {
    match art.tab_source.as_deref() {
        Some(src) => exp_to_orig(src, shaped),
        None => shaped,
    }
}

fn slice_shaped_offset(
    art: &ShapeArtifact,
    byte_start: usize,
    byte_end: usize,
    local: usize,
) -> usize {
    match art.tab_source.as_deref() {
        Some(src) => {
            let slice = src.get(byte_start..byte_end).unwrap_or("");
            orig_to_exp(slice, local)
        }
        None => local,
    }
}

fn slice_source_offset(
    art: &ShapeArtifact,
    byte_start: usize,
    byte_end: usize,
    local: usize,
) -> usize {
    match art.tab_source.as_deref() {
        Some(src) => {
            let slice = src.get(byte_start..byte_end).unwrap_or("");
            exp_to_orig(slice, local)
        }
        None => local,
    }
}

impl GpuiShaper {
    pub fn row_content_start(
        &self,
        art: &ShapeArtifact,
        row: u32,
        align: InlineAlign,
        inner: Px,
    ) -> Px {
        if !art.bands.is_empty() {
            return band_content_start(art, row, align, inner);
        }
        row_align_dx(art, row, align, inner)
    }

    pub fn row_content_end(
        &self,
        art: &ShapeArtifact,
        row: u32,
        align: InlineAlign,
        inner: Px,
    ) -> Px {
        if !art.bands.is_empty() {
            return band_content_end(art, row, align, inner);
        }
        row_align_dx(art, row, align, inner) + row_ink_width(art, row)
    }

    pub fn position_for_offset(
        &self,
        art: &ShapeArtifact,
        offset: usize,
        align: InlineAlign,
        inner: Px,
    ) -> (Px, u32) {
        if !art.bands.is_empty() {
            for (i, band) in art.bands.iter().enumerate() {
                let band_dx = band_align_shift(art, i as u32, align, inner);
                for part in &band.parts {
                    match part {
                        ShapePart::Text {
                            line,
                            x,
                            byte_start,
                            byte_end,
                            ..
                        } => {
                            if offset <= *byte_end {
                                let dx = band_dx + line_align_shift(line, align, 0);
                                if offset <= *byte_start {
                                    return (*x + dx, i as u32);
                                }
                                let local = slice_shaped_offset(
                                    art,
                                    *byte_start,
                                    *byte_end,
                                    offset - *byte_start,
                                );
                                if let Some(p) =
                                    line.position_for_index(local, px(art.row_advance as f32))
                                {
                                    return (*x + f32::from(p.x) as Px + dx, i as u32);
                                }
                                return (*x + dx, i as u32);
                            }
                        }
                        ShapePart::Math {
                            x,
                            width,
                            byte_start,
                            byte_end,
                            fallback,
                            ..
                        }
                        | ShapePart::Image {
                            x,
                            width,
                            byte_start,
                            byte_end,
                            fallback,
                            ..
                        } => {
                            if *byte_start == *byte_end {
                                continue;
                            }
                            if offset <= *byte_end {
                                if offset <= *byte_start {
                                    let dx = band_dx
                                        + fallback
                                            .as_deref()
                                            .map(|line| line_align_shift(line, align, 0))
                                            .unwrap_or(0.0);
                                    return (*x + dx, i as u32);
                                }
                                let dx = band_dx
                                    + fallback
                                        .as_deref()
                                        .map(|line| line_align_shift(line, align, 0))
                                        .unwrap_or(0.0);
                                return (*x + dx + *width, i as u32);
                            }
                        }
                    }
                }
            }
            let last = art.bands.len().saturating_sub(1) as u32;
            return (0.0, last);
        }
        let lh = px(art.row_advance as f32);
        if art.lines.is_empty() {
            return (row_align_dx(art, 0, align, inner), 0);
        }
        let offset = shaped_offset(art, offset);
        let (li, local) = locate_hard_offset(&art.line_starts, &art.line_lens, offset);
        let line = &art.lines[li];
        let row_base: u32 = art
            .lines
            .iter()
            .take(li)
            .map(|l| l.wrap_boundaries.len() as u32 + 1)
            .sum();
        if let Some(p) = line.position_for_index(local, lh) {
            let row = (f32::from(p.y) / art.row_advance as f32).round() as u32;
            let row = row_base + row;
            return (
                f32::from(p.x) as Px + row_align_dx(art, row, align, inner),
                row,
            );
        }
        let row = row_base;
        (row_align_dx(art, row, align, inner), row)
    }

    pub fn offset_for_position(
        &self,
        art: &ShapeArtifact,
        x: Px,
        row: u32,
        align: InlineAlign,
        inner: Px,
    ) -> usize {
        if !art.bands.is_empty() {
            let Some(band) = art.bands.get(row as usize) else {
                return art
                    .bands
                    .last()
                    .and_then(|b| b.parts.last())
                    .map(|p| match p {
                        ShapePart::Text { byte_end, .. }
                        | ShapePart::Math { byte_end, .. }
                        | ShapePart::Image { byte_end, .. } => *byte_end,
                    })
                    .unwrap_or(0);
            };
            let band_dx = band_align_shift(art, row, align, inner);
            for part in &band.parts {
                match part {
                    ShapePart::Text {
                        line,
                        x: px0,
                        byte_start,
                        byte_end,
                        ..
                    } => {
                        let dx = line_align_shift(line, align, 0);
                        let w = f32::from(line.width()) as Px;
                        if *byte_start == *byte_end {
                            return *byte_start;
                        }
                        if x <= *px0 + band_dx + dx + w {
                            let local = line
                                .closest_index_for_position(
                                    gpui::point(px((x - *px0 - band_dx - dx) as f32), px(1.0)),
                                    px(art.row_advance as f32),
                                )
                                .unwrap_or_else(|i| i);
                            let local = slice_source_offset(art, *byte_start, *byte_end, local);
                            return (*byte_start + local).min(*byte_end);
                        }
                    }
                    ShapePart::Math {
                        x: px0,
                        width,
                        byte_start,
                        byte_end,
                        fallback,
                        ..
                    }
                    | ShapePart::Image {
                        x: px0,
                        width,
                        byte_start,
                        byte_end,
                        fallback,
                        ..
                    } => {
                        if *byte_start == *byte_end {
                            continue;
                        }
                        let dx = band_dx
                            + fallback
                                .as_deref()
                                .map(|line| line_align_shift(line, align, 0))
                                .unwrap_or(0.0);
                        if x <= *px0 + dx + *width {
                            if x < *px0 + dx + *width * 0.5 {
                                return *byte_start;
                            }
                            return *byte_end;
                        }
                    }
                }
            }
            return band
                .parts
                .last()
                .map(|p| match p {
                    ShapePart::Text { byte_end, .. }
                    | ShapePart::Math { byte_end, .. }
                    | ShapePart::Image { byte_end, .. } => *byte_end,
                })
                .unwrap_or(0);
        }
        let x = x - row_align_dx(art, row, align, inner);
        let lh = px(art.row_advance as f32);
        let mut row_base = 0u32;
        for (i, l) in art.lines.iter().enumerate() {
            let n_rows = l.wrap_boundaries.len() as u32 + 1;
            if row < row_base + n_rows {
                let local_row = row - row_base;
                let p = gpui::point(
                    px(x as f32),
                    px(local_row as f32 * art.row_advance as f32 + 1.0),
                );
                let local = l.closest_index_for_position(p, lh).unwrap_or_else(|i| i);
                return source_offset(
                    art,
                    offset_on_hard_line(&art.line_starts, &art.line_lens, i, local),
                );
            }
            row_base += n_rows;
        }
        let last = art.line_lens.len().saturating_sub(1);
        source_offset(
            art,
            offset_on_hard_line(
                &art.line_starts,
                &art.line_lens,
                last,
                art.line_lens.get(last).copied().unwrap_or(0),
            ),
        )
    }
}

#[cfg(test)]
mod align_tests {
    use super::align_shift;
    use md_core::inline::InlineAlign;

    #[test]
    fn align_shift_matches_gpui_paint() {
        assert_eq!(align_shift(InlineAlign::Start, 100.0, 40.0), 0.0);
        assert_eq!(align_shift(InlineAlign::Center, 100.0, 40.0), 30.0);
        assert_eq!(align_shift(InlineAlign::End, 100.0, 40.0), 60.0);
        assert_eq!(align_shift(InlineAlign::Center, 100.0, 0.0), 50.0);
        assert_eq!(align_shift(InlineAlign::End, 100.0, 0.0), 100.0);
    }
}
