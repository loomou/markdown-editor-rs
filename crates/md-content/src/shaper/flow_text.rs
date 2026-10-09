use super::GpuiShaper;
use super::artifact::ShapeArtifact;
use super::atoms::slice_runs;
use super::bands::{BandFlow, Pending, bands_to_artifact, flush_band};
use super::resolved::ResolvedType;
use crate::math::MathEm;
use gpui::{Pixels, TextRun, WrappedLine, px};
use md_core::Px;
use md_core::block::BlockKind;
use md_core::inline::InlineRun;
use std::borrow::Cow;
use std::ops::Range;

impl GpuiShaper {
    pub(super) fn place_text_slice(
        &self,
        text: &str,
        runs: &[InlineRun],
        range: Range<usize>,
        font_size: Pixels,
        dy: f32,
        flow: &mut BandFlow<'_>,
    ) {
        let start = range.start;
        let end = range.end;
        if start >= end {
            return;
        }
        let Some(slice) = text.get(start..end) else {
            return;
        };
        if slice.contains('\n') {
            let mut seg_start = start;
            for (i, b) in slice.bytes().enumerate() {
                if b != b'\n' {
                    continue;
                }
                let seg_end = start + i;
                self.place_text_slice(text, runs, seg_start..seg_end, font_size, dy, flow);
                if flow.pending.is_empty() && *flow.x == 0.0 {
                    let line =
                        self.shape_slice(text, runs, seg_end..seg_end, flow.role, font_size, None);
                    flow.pending.push(Pending::Text {
                        line: Box::new(line),
                        x: 0.0,
                        start: seg_end,
                        end: seg_end,
                        dy,
                    });
                }
                flow.flush();
                seg_start = seg_end + 1;
            }
            self.place_text_slice(text, runs, seg_start..end, font_size, dy, flow);
            return;
        }
        let is_ws = slice.chars().all(char::is_whitespace);
        let line = self.shape_slice(text, runs, start..end, flow.role, font_size, None);
        let w = f32::from(line.width());
        if *flow.x > 0.0 && *flow.x + w > flow.avail as f32 {
            flow.flush();
            if is_ws {
                return;
            }
        }
        if w > flow.avail as f32 && !is_ws {
            if *flow.x > 0.0 {
                flow.flush();
            }
            let wrapped = self.wrap_slice(text, runs, start..end, flow.avail, flow.role, font_size);
            let last = wrapped.len().saturating_sub(1);
            for (i, (line, s, e)) in wrapped.into_iter().enumerate() {
                let frag_w = f32::from(line.width());
                flow.pending.push(Pending::Text {
                    line: Box::new(line),
                    x: 0.0,
                    start: s,
                    end: e,
                    dy,
                });
                if i == last {
                    *flow.x = frag_w;
                    break;
                }
                flow.flush();
            }
            return;
        }
        flow.pending.push(Pending::Text {
            line: Box::new(line),
            x: *flow.x,
            start,
            end,
            dy,
        });
        *flow.x += w;
    }

    pub(super) fn shape_display_math(
        &self,
        latex: &str,
        avail: Px,
        role: &ResolvedType,
        font_size: f32,
    ) -> ShapeArtifact {
        let dpr = crate::pixels::dpr_from_q(crate::pixels::dpr_q(self.scale));
        let metrics = crate::math::metric(&self.math_metrics, latex, true)
            .flatten()
            .unwrap_or_else(|| MathEm::estimate(latex));
        let w = metrics.box_width(font_size, dpr);
        let x = ((avail as f32 - w) * 0.5).max(0.0);
        let mut pending = vec![Pending::Math {
            x,
            metrics,
            latex: latex.to_string(),
            display: true,
            start: 0,
            end: latex.len(),
            fallback: None,
        }];
        let mut bands = Vec::new();
        flush_band(&mut pending, role, font_size, dpr, &mut bands, None);
        bands_to_artifact(bands, role)
    }

    pub(super) fn shape_fallback(&self, text: &str, role: &ResolvedType, avail: Px) -> WrappedLine {
        let line = self.shape_plain(text, role, role.font_size, None);
        if f32::from(line.width()) <= avail as f32 {
            return line;
        }
        self.shape_plain(text, role, role.font_size, Some(px(avail as f32)))
    }

    pub(super) fn shape_plain(
        &self,
        text: &str,
        role: &ResolvedType,
        font_size: Pixels,
        wrap: Option<Pixels>,
    ) -> WrappedLine {
        let single_line = if text.contains('\n') {
            Cow::Owned(text.replace('\n', " "))
        } else {
            Cow::Borrowed(text)
        };
        let text: &str = &single_line;
        let run = TextRun {
            len: text.len(),
            font: role.font.clone(),
            color: role.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let (shaped, runs, _) = super::tabs::prepare_shape(text, vec![run]);
        self.text_system
            .shape_text(shaped.into(), font_size, &runs, wrap, None)
            .expect("shape_text")
            .into_iter()
            .last()
            .unwrap_or_default()
    }

    pub(super) fn shape_slice(
        &self,
        text: &str,
        runs: &[InlineRun],
        range: Range<usize>,
        role: &ResolvedType,
        font_size: Pixels,
        wrap: Option<Pixels>,
    ) -> WrappedLine {
        let slice = text.get(range.start..range.end).unwrap_or("");
        if slice.is_empty() {
            return self
                .text_system
                .shape_text(" ".to_string().into(), font_size, &[], wrap, None)
                .expect("shape_text")
                .into_iter()
                .last()
                .unwrap_or_default();
        }
        let local_runs = slice_runs(runs, range.start as u32, range.end as u32);
        let styled = self.text_runs(slice, &local_runs, role, BlockKind::Paragraph);
        let (shaped, styled, _) = super::tabs::prepare_shape(slice, styled);
        self.text_system
            .shape_text(shaped.into(), font_size, &styled, wrap, None)
            .expect("shape_text")
            .into_iter()
            .last()
            .unwrap_or_default()
    }

    pub(super) fn wrap_slice(
        &self,
        text: &str,
        runs: &[InlineRun],
        range: Range<usize>,
        avail: Px,
        role: &ResolvedType,
        font_size: Pixels,
    ) -> Vec<(WrappedLine, usize, usize)> {
        let start = range.start;
        let end = range.end;
        let wrapped = self.shape_slice(
            text,
            runs,
            start..end,
            role,
            font_size,
            Some(px(avail as f32)),
        );
        let slice = text.get(start..end).unwrap_or("");
        let mut cuts = vec![0usize];
        for b in &wrapped.wrap_boundaries {
            let run = wrapped.unwrapped_layout.runs.get(b.run_ix);
            let Some(run) = run else {
                continue;
            };
            let Some(glyph) = run.glyphs.get(b.glyph_ix) else {
                continue;
            };
            if glyph.index > 0 {
                cuts.push(super::tabs::exp_to_orig(slice, glyph.index));
            }
        }
        cuts.push(end - start);
        cuts.sort_unstable();
        cuts.dedup();
        let mut out = Vec::new();
        for pair in cuts.windows(2) {
            let rel_s = pair[0];
            let rel_e = pair[1];
            if rel_e <= rel_s {
                continue;
            }
            let abs_s = start + rel_s;
            let abs_e = start + rel_e;
            let line = self.shape_slice(text, runs, abs_s..abs_e, role, font_size, None);
            out.push((line, abs_s, abs_e));
        }
        if out.is_empty() {
            out.push((wrapped, start, end));
        }
        out
    }
}
