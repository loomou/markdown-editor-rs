use super::artifact::{ShapeArtifact, ShapeBand, ShapePart};
use super::resolved::ResolvedType;
use crate::math::MathEm;
use gpui::WrappedLine;
use md_core::Px;
use md_core::block::BlockKind;
use md_core::inline::InlineRun;

pub(super) fn line_max_width(lines: &[WrappedLine]) -> Px {
    lines
        .iter()
        .map(|l| f32::from(l.width()) as Px)
        .fold(0.0, f64::max)
}

pub(super) fn needs_bands(block_kind: BlockKind, runs: &[InlineRun]) -> bool {
    block_kind == BlockKind::Math
        || block_kind == BlockKind::Image
        || runs
            .iter()
            .any(|r| r.marks.is_atomic() || r.marks.is_script())
}

pub(super) fn mermaid_fit(avail: Px, max_w: Px, max_h: Px, fitted: Option<(f32, f32)>) -> (Px, Px) {
    let cap_w = avail.min(max_w).max(1.0);
    let cap_h = max_h.max(1.0);
    let Some((fw, fh)) = fitted else {
        return (cap_w, cap_h);
    };
    let fw = fw as Px;
    let fh = fh as Px;
    let scale = (cap_w / fw.max(1.0)).min(1.0);
    ((fw * scale).max(1.0), (fh * scale).max(1.0))
}

pub(super) enum Atom {
    Break {
        offset: usize,
    },
    Text {
        start: usize,
        end: usize,
    },
    Script {
        start: usize,
        end: usize,
        super_script: bool,
    },
    Math {
        start: usize,
        end: usize,
        display: bool,
    },
    Image {
        start: usize,
        end: usize,
        dest: String,

        raw: Option<String>,
    },
}

pub(super) enum Pending {
    Text {
        line: Box<WrappedLine>,
        x: f32,
        start: usize,
        end: usize,
        dy: f32,
    },
    Math {
        x: f32,
        metrics: MathEm,
        latex: String,
        display: bool,
        start: usize,
        end: usize,

        fallback: Option<Box<WrappedLine>>,
    },
    Image {
        x: f32,
        dest: String,
        slot_w: f32,
        slot_h: f32,
        start: usize,
        end: usize,

        fallback: Option<Box<WrappedLine>>,
    },
}

pub(super) struct BandFlow<'a> {
    pub(super) x: &'a mut f32,
    pub(super) pending: &'a mut Vec<Pending>,
    pub(super) bands: &'a mut Vec<ShapeBand>,
    pub(super) avail: Px,
    pub(super) role: &'a ResolvedType,
    pub(super) parent_size: f32,
    pub(super) dpr: f32,
}

impl BandFlow<'_> {
    pub(super) fn flush(&mut self) {
        flush_band(
            self.pending,
            self.role,
            self.parent_size,
            self.dpr,
            self.bands,
            Some(self.avail),
        );
        *self.x = 0.0;
    }
}

pub(super) fn bands_to_artifact(bands: Vec<ShapeBand>, role: &ResolvedType) -> ShapeArtifact {
    if bands.is_empty() {
        return ShapeArtifact::plain(
            Vec::new(),
            1,
            role.row_advance,
            role.text_baseline(),
            role.row_advance,
        );
    }
    let height: Px = bands.iter().map(|b| b.height).sum();
    let first_baseline = bands[0].text_dy + role.text_baseline();
    let rows = bands.len() as u32;

    let max_line_width = band_max_width(&bands);
    ShapeArtifact {
        lines: Vec::new(),
        row_map: Vec::new(),
        line_lens: Vec::new(),
        line_starts: Vec::new(),
        line_colors: Vec::new(),
        bands,
        rows,
        height,
        first_baseline,
        row_advance: role.row_advance,
        max_line_width,
        tab_source: None,
    }
}

fn band_max_width(bands: &[ShapeBand]) -> Px {
    bands
        .iter()
        .flat_map(|band| band.parts.iter())
        .map(|part| match part {
            ShapePart::Text { x, line, .. } => *x + f32::from(line.width()) as Px,
            ShapePart::Math { x, width, .. } | ShapePart::Image { x, width, .. } => *x + *width,
        })
        .fold(0.0, f64::max)
}

fn fallback_below(line: &WrappedLine, role: &ResolvedType, gpui_bl: Px) -> Px {
    (role.row_advance - gpui_bl).max(0.0) + fallback_rows(line, role) - role.row_advance
}

fn fallback_rows(line: &WrappedLine, role: &ResolvedType) -> Px {
    (line.wrap_boundaries.len() + 1) as Px * role.row_advance
}

fn is_lone_display(pending: &[Pending]) -> bool {
    matches!(
        pending,
        [Pending::Math {
            display: true,
            fallback: None,
            ..
        }]
    )
}

pub(super) fn flush_band(
    pending: &mut Vec<Pending>,
    role: &ResolvedType,
    font_size: f32,
    dpr: f32,
    bands: &mut Vec<ShapeBand>,
    lone_avail: Option<Px>,
) {
    if pending.is_empty() {
        return;
    }
    let lone_avail = lone_avail.filter(|_| is_lone_display(pending));

    if let Some(avail) = lone_avail
        && let [Pending::Math { x, metrics, .. }] = pending.as_mut_slice()
    {
        *x = ((avail as f32 - metrics.box_width(font_size, dpr)) * 0.5).max(0.0);
    }
    let gpui_bl = role.text_baseline();
    let mut above = gpui_bl;
    let mut below = (role.row_advance - gpui_bl).max(0.0);
    for p in pending.iter() {
        match p {
            Pending::Math {
                metrics, fallback, ..
            } => {
                if let Some(line) = fallback {
                    below = below.max(fallback_below(line, role, gpui_bl));
                    continue;
                }
                let ascent = metrics.css_ascent(font_size) as Px;
                let png_h = metrics.box_height(font_size, dpr) as Px;
                above = above.max(ascent);
                below = below.max((png_h - ascent).max(0.0));
            }
            Pending::Image {
                slot_h, fallback, ..
            } => {
                if let Some(line) = fallback {
                    below = below.max(fallback_below(line, role, gpui_bl));
                    continue;
                }
                above = above.max(*slot_h as Px);
            }
            Pending::Text { dy, .. } => {
                if *dy < 0.0 {
                    above = above.max(gpui_bl - *dy as Px);
                } else if *dy > 0.0 {
                    below = below.max((role.row_advance - gpui_bl) + *dy as Px);
                }
            }
        }
    }

    let air = if lone_avail.is_some() {
        role.row_advance
    } else {
        0.0
    };
    above += air;
    below += air;
    let height = (above + below).max(role.row_advance);
    let text_dy = above - gpui_bl;
    let parts = pending
        .drain(..)
        .map(|p| match p {
            Pending::Text {
                line,
                x,
                start,
                end,
                dy,
            } => ShapePart::Text {
                line,
                x: x as Px,
                byte_start: start,
                byte_end: end,
                dy: dy as Px,
            },
            Pending::Math {
                x,
                metrics,
                latex,
                display,
                start,
                end,
                fallback,
            } => match fallback {
                Some(line) => ShapePart::Math {
                    x: x as Px,
                    width: f32::from(line.width()) as Px,
                    paint_y: text_dy,
                    latex,
                    display,
                    em: font_size,
                    slot_h: fallback_rows(&line, role),
                    byte_start: start,
                    byte_end: end,
                    fallback: Some(line),
                },
                None => ShapePart::Math {
                    x: x as Px,
                    width: metrics.box_width(font_size, dpr) as Px,
                    paint_y: above - metrics.css_ascent(font_size) as Px,
                    latex,
                    display,
                    em: font_size,
                    slot_h: metrics.box_height(font_size, dpr) as Px,
                    byte_start: start,
                    byte_end: end,
                    fallback: None,
                },
            },
            Pending::Image {
                x,
                dest,
                slot_w,
                slot_h,
                start,
                end,
                fallback,
            } => match fallback {
                Some(line) => ShapePart::Image {
                    x: x as Px,
                    width: f32::from(line.width()) as Px,
                    paint_y: text_dy,
                    dest,
                    slot_w: f32::from(line.width()) as Px,
                    slot_h: fallback_rows(&line, role),
                    byte_start: start,
                    byte_end: end,
                    fallback: Some(line),
                },
                None => ShapePart::Image {
                    x: x as Px,
                    width: slot_w as Px,
                    paint_y: above - slot_h as Px,
                    dest,
                    slot_w: slot_w as Px,
                    slot_h: slot_h as Px,
                    byte_start: start,
                    byte_end: end,
                    fallback: None,
                },
            },
        })
        .collect();
    bands.push(ShapeBand {
        height,
        text_dy,
        parts,
    });
}

#[cfg(test)]
mod fit_tests {
    use super::{ShapeBand, ShapePart, band_max_width, mermaid_fit};

    #[test]
    fn mermaid_fit_does_not_shrink_height_into_max() {
        let (w, h) = mermaid_fit(400.0, 804.0, 420.0, Some((400.0, 800.0)));
        assert!((w - 400.0).abs() < 1e-6);
        assert!((h - 800.0).abs() < 1e-6);
    }

    #[test]
    fn mermaid_fit_scales_by_width_only() {
        let (w, h) = mermaid_fit(400.0, 804.0, 420.0, Some((800.0, 400.0)));
        assert!((w - 400.0).abs() < 1e-6);
        assert!((h - 200.0).abs() < 1e-6);
    }

    #[test]
    fn band_width_includes_part_origin() {
        let bands = vec![ShapeBand {
            height: 20.0,
            text_dy: 0.0,
            parts: vec![ShapePart::Math {
                x: 300.0,
                width: 50.0,
                paint_y: 0.0,
                latex: "x".to_string(),
                display: false,
                em: 16.0,
                slot_h: 20.0,
                byte_start: 0,
                byte_end: 1,
                fallback: None,
            }],
        }];
        assert_eq!(band_max_width(&bands), 350.0);
    }
}
