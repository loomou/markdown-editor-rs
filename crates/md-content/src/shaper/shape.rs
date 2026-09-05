use super::GpuiShaper;
use super::artifact::ShapeArtifact;
use super::bands::{mermaid_fit, needs_bands};
use super::cache::Key;
use super::color::color_runs_by_line;
use super::resolved::ResolvedType;
use crate::gpui_theme::{ThemeColorExt, font_style, font_weight};
use gpui::{Hsla, StrikethroughStyle, TextRun, UnderlineStyle, WrappedLine, px};
use md_core::Px;
use md_core::block::BlockKind;
use md_core::inline::{InlineMarks, InlineRun, covering_runs};
use md_layout::shaper::ShapeIdentity;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

impl GpuiShaper {
    pub fn artifact(
        &self,
        text: &str,
        runs: &[InlineRun],
        avail_width: Px,
        block_kind: BlockKind,
        ident: ShapeIdentity,
    ) -> Rc<ShapeArtifact> {
        if block_kind == BlockKind::ThematicBreak {
            let role = self.roles.for_kind(block_kind);
            return Rc::new(ShapeArtifact::plain(
                Vec::new(),
                1,
                1.0,
                1.0,
                role.row_advance,
            ));
        }

        let block_kind = if ident.edit_source {
            BlockKind::CodeBlock
        } else {
            block_kind
        };
        if block_kind == BlockKind::Mermaid {
            let role = self.roles.for_kind(block_kind);
            let (_w, h) = mermaid_fit(
                avail_width,
                self.decoration.mermaid_max_width,
                self.decoration.mermaid_max_height,
                self.mermaid_fitted
                    .get(&(ident.index, ident.generation))
                    .copied(),
            );
            return Rc::new(ShapeArtifact::plain(
                Vec::new(),
                1,
                h,
                role.ascent.min(h),
                h,
            ));
        }
        let role = self.roles.for_slot(block_kind, ident.type_slot);
        let has_math = block_kind == BlockKind::Math || runs.iter().any(|run| run.marks.is_math());
        let has_image =
            block_kind == BlockKind::Image || runs.iter().any(|run| run.marks.is_image());
        let media_q = if needs_bands(block_kind, runs) {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            has_math.hash(&mut h);
            if has_math {
                0x6d617468_u64.hash(&mut h);
                self.math_gen.hash(&mut h);
            }
            has_image.hash(&mut h);
            if has_image {
                0x696d6167_u64.hash(&mut h);
                self.image_gen.hash(&mut h);
            }
            h.finish()
        } else {
            0
        };
        let key = Key {
            ident,
            width_q: if matches!(block_kind, BlockKind::CodeBlock | BlockKind::MetadataBlock) {
                0
            } else {
                (avail_width * 64.0).round() as i64
            },
            style_fp: role.style_fp,
            media_q,
        };
        {
            let mut s = self.stats.borrow_mut();
            s.measure_calls += 1;
            s.total_measure_calls += 1;
        }
        if let Some(hit) = self.cache.get(&key) {
            self.stats.borrow_mut().artifact_reuses += 1;
            return hit;
        }
        {
            let mut s = self.stats.borrow_mut();
            s.shape_calls += 1;
            s.total_shape_calls += 1;
        }
        let out = Rc::new(if needs_bands(block_kind, runs) {
            self.shape_bands(text, runs, avail_width, role, block_kind, ident)
        } else {
            self.shape_uncached(text, runs, avail_width, role, block_kind, ident)
        });
        self.cache.insert(key, Rc::clone(&out));
        out
    }

    pub(super) fn text_runs(
        &self,
        text: &str,
        runs: &[InlineRun],
        role: &ResolvedType,
        block_kind: BlockKind,
    ) -> Vec<TextRun> {
        if matches!(block_kind, BlockKind::CodeBlock | BlockKind::MetadataBlock) {
            return self.code_text_runs(text, role);
        }
        let covered = covering_runs(text.len() as u32, runs);
        if covered.is_empty() {
            return vec![TextRun {
                len: text.len(),
                font: role.font.clone(),
                color: role.color,
                background_color: None,
                underline: None,
                strikethrough: self.role_strike(role),
            }];
        }
        covered
            .into_iter()
            .map(|r| {
                let len = (r.display_range.end - r.display_range.start) as usize;
                if r.marks.is_syntax() {
                    return TextRun {
                        len,
                        font: role.font.clone(),
                        color: self.inline.syntax_marker.hsla(),
                        background_color: None,
                        underline: None,
                        strikethrough: self.role_strike(role),
                    };
                }
                if r.marks.is_empty() && r.link.is_none() {
                    return TextRun {
                        len,
                        font: role.font.clone(),
                        color: role.color,
                        background_color: None,
                        underline: None,
                        strikethrough: self.role_strike(role),
                    };
                }
                let mut font = if r.marks.contains(InlineMarks::CODE) {
                    self.roles.code.font.clone()
                } else {
                    role.font.clone()
                };
                if r.marks.contains(InlineMarks::STRONG) {
                    font.weight = font_weight(self.inline.strong_weight);
                }
                if r.marks.contains(InlineMarks::EM) {
                    font.style = font_style(self.inline.emphasis_style);
                }
                let color = if r.marks.contains(InlineMarks::CODE) {
                    self.inline.inline_code.hsla()
                } else if r.link.is_some() || r.marks.contains(InlineMarks::FOOTNOTE) {
                    self.inline.link.hsla()
                } else if r.marks.contains(InlineMarks::STRIKE) {
                    self.inline.strikethrough.hsla()
                } else {
                    role.color
                };
                let underline = if r.link.is_some() || r.marks.contains(InlineMarks::FOOTNOTE) {
                    Some(UnderlineStyle {
                        thickness: px(self.inline.link_underline_px),
                        color: Some(self.inline.link_underline.hsla()),
                        wavy: false,
                    })
                } else {
                    None
                };
                let strikethrough = if r.marks.contains(InlineMarks::STRIKE) {
                    Some(StrikethroughStyle {
                        thickness: px(self.inline.strikethrough_px),
                        color: Some(self.inline.strikethrough.hsla()),
                    })
                } else {
                    self.role_strike(role)
                };
                TextRun {
                    len,
                    font,
                    color,

                    background_color: None,
                    underline,
                    strikethrough,
                }
            })
            .collect()
    }

    fn role_strike(&self, role: &ResolvedType) -> Option<StrikethroughStyle> {
        role.strikethrough.map(|color| StrikethroughStyle {
            thickness: px(self.inline.strikethrough_px),
            color: Some(color),
        })
    }

    fn code_text_runs(&self, text: &str, role: &ResolvedType) -> Vec<TextRun> {
        vec![TextRun {
            len: text.len(),
            font: role.font.clone(),
            color: role.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }]
    }

    fn code_line_colors(&self, text: &str, ident: ShapeIdentity) -> Vec<Vec<(u32, Hsla)>> {
        let lang = self
            .block_code_lang
            .get(&ident.index)
            .map(|s| s.as_str())
            .unwrap_or("");
        let spans = crate::highlight::spans(lang, text);
        let covered: usize = spans.iter().map(|s| s.end.saturating_sub(s.start)).sum();
        if spans.is_empty() || covered != text.len() {
            return Vec::new();
        }
        color_runs_by_line(text, &spans, self.syntax)
    }

    fn shape_uncached(
        &self,
        text: &str,
        runs: &[InlineRun],
        avail_width: Px,
        role: &ResolvedType,
        block_kind: BlockKind,
        ident: ShapeIdentity,
    ) -> ShapeArtifact {
        if text.is_empty() {
            return ShapeArtifact::plain(
                Vec::new(),
                1,
                role.row_advance,
                role.text_baseline(),
                role.row_advance,
            );
        }

        let styled = self.text_runs(text, runs, role, block_kind);
        let line_colors = if block_kind == BlockKind::CodeBlock {
            let colors = self.code_line_colors(text, ident);
            super::tabs::expand_line_colors(text, colors)
        } else {
            Vec::new()
        };

        let wrap = if matches!(block_kind, BlockKind::CodeBlock | BlockKind::MetadataBlock) {
            None
        } else {
            Some(px(avail_width as f32))
        };
        let (shaped, styled, tab_source) = super::tabs::prepare_shape(text, styled);
        let lines: Vec<WrappedLine> = self
            .text_system
            .shape_text(shaped.into(), role.font_size, &styled, wrap, None)
            .expect("shape_text")
            .into_iter()
            .collect();

        let rows: u32 = lines
            .iter()
            .map(|l| l.wrap_boundaries.len() as u32 + 1)
            .sum();
        let rows = rows.max(1);

        let mut art = ShapeArtifact::plain(
            lines,
            rows,
            rows as Px * role.row_advance,
            role.text_baseline(),
            role.row_advance,
        );
        art.line_colors = line_colors;
        art.tab_source = tab_source;
        art
    }

    pub(super) fn image_slot(
        &self,
        dest: &str,
        avail: Px,
        inline: bool,
        role: &ResolvedType,
    ) -> (f32, f32) {
        let dpr = crate::pixels::dpr_from_q(crate::pixels::dpr_q(self.scale));
        let max_w = avail.min(self.decoration.image_max_width).max(1.0) as f32;
        let mut max_h = self.decoration.image_max_height.max(1.0) as f32;
        if inline {
            max_h = max_h.min((role.row_advance * 3.0) as f32);
        }
        let ph = self.decoration.image_placeholder_height.max(1.0) as f32;
        match self.image_sizes.get(dest).copied() {
            Some((iw, ih)) => {
                let (w, h) = if inline {
                    crate::images::contain_fit(iw as f32, ih as f32, max_w, max_h)
                } else {
                    crate::images::width_fit(iw as f32, ih as f32, max_w)
                };
                crate::images::quantized_slot(
                    crate::images::snap_css(w, dpr),
                    crate::images::snap_css(h, dpr),
                )
            }
            None if inline => {
                let h = ph.min(max_h);
                crate::images::quantized_slot(h.min(max_w), h)
            }
            None => crate::images::quantized_slot(max_w, ph),
        }
    }
}
