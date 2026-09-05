use super::artifact::ShapeArtifact;
use super::atoms::line_atoms;
use super::bands::{Atom, BandFlow, Pending, bands_to_artifact};
use super::resolved::ResolvedType;
use super::{GpuiShaper, SCRIPT_SCALE, SUB_DROP, SUPER_RISE};
use gpui::px;
use md_core::Px;
use md_core::block::BlockKind;
use md_core::inline::InlineRun;
use md_layout::shaper::ShapeIdentity;

impl GpuiShaper {
    pub(super) fn shape_bands(
        &self,
        text: &str,
        runs: &[InlineRun],
        avail_width: Px,
        role: &ResolvedType,
        block_kind: BlockKind,
        ident: ShapeIdentity,
    ) -> ShapeArtifact {
        let font_size = f32::from(role.font_size);
        let dpr = crate::pixels::dpr_from_q(crate::pixels::dpr_q(self.scale));
        let avail = avail_width.max(1.0);
        if block_kind == BlockKind::Math {
            return self.shape_display_math(text, avail, role, font_size);
        }
        let mut pending: Vec<Pending> = Vec::new();
        let mut x = 0.0f32;
        let mut bands = Vec::new();
        let mut flow = BandFlow {
            x: &mut x,
            pending: &mut pending,
            bands: &mut bands,
            avail,
            role,
            parent_size: font_size,
            dpr,
        };
        if block_kind == BlockKind::Image {
            let dest = self
                .block_image_dest
                .get(&ident.index)
                .cloned()
                .unwrap_or_default();
            let (slot_w, slot_h) = self.image_slot(&dest, avail, false, role);
            let ix = ((avail as f32 - slot_w) * 0.5).max(0.0);

            flow.pending.push(Pending::Image {
                x: ix,
                dest,
                slot_w,
                slot_h,
                start: 0,
                end: 0,
                fallback: None,
            });
            flow.flush();
        }
        let atoms = line_atoms(text, runs, self.link_dests.as_ref(), self.link_raw.as_ref());
        for atom in atoms {
            match atom {
                Atom::Break { offset } => {
                    if flow.pending.is_empty() && *flow.x == 0.0 {
                        let line = self.shape_slice(
                            text,
                            runs,
                            offset..offset,
                            role,
                            role.font_size,
                            None,
                        );
                        flow.pending.push(Pending::Text {
                            line: Box::new(line),
                            x: 0.0,
                            start: offset,
                            end: offset,
                            dy: 0.0,
                        });
                        flow.flush();
                    } else {
                        flow.flush();
                    }
                }
                Atom::Text { start, end } => {
                    self.place_text_slice(text, runs, start..end, role.font_size, 0.0, &mut flow);
                }
                Atom::Script {
                    start,
                    end,
                    super_script,
                } => {
                    let sz = px((font_size * SCRIPT_SCALE).max(1.0));
                    let dy = if super_script {
                        -font_size * SUPER_RISE
                    } else {
                        font_size * SUB_DROP
                    };
                    self.place_text_slice(text, runs, start..end, sz, dy, &mut flow);
                }
                Atom::Math {
                    start,
                    end,
                    display,
                } => {
                    let latex = text.get(start..end).unwrap_or("");

                    let recorded = crate::math::metric(&self.math_metrics, latex, display);
                    let fallback = if matches!(recorded, Some(None)) {
                        let raw = if display {
                            format!("$${latex}$$")
                        } else {
                            format!("${latex}$")
                        };
                        Some(Box::new(self.shape_fallback(&raw, role, avail)))
                    } else {
                        None
                    };
                    let metrics = recorded
                        .flatten()
                        .unwrap_or_else(|| crate::math::MathEm::estimate(latex));
                    let w = match &fallback {
                        Some(line) => f32::from(line.width()),
                        None => metrics.box_width(font_size, dpr),
                    };
                    if *flow.x > 0.0 && *flow.x + w > flow.avail as f32 {
                        flow.flush();
                    }
                    let x = *flow.x;
                    flow.pending.push(Pending::Math {
                        x,
                        metrics,
                        latex: latex.to_string(),
                        display,
                        start,
                        end,
                        fallback,
                    });
                    *flow.x += w;
                }
                Atom::Image {
                    start,
                    end,
                    dest,
                    raw,
                } => {
                    let failed = !dest.is_empty() && self.image_failed.contains(dest.as_str());
                    let fallback = raw
                        .filter(|_| failed)
                        .map(|raw| Box::new(self.shape_fallback(&raw, role, avail)));
                    let (slot_w, slot_h) = match &fallback {
                        Some(line) => (f32::from(line.width()), role.row_advance as f32),
                        None => self.image_slot(&dest, avail, true, role),
                    };
                    if *flow.x > 0.0 && *flow.x + slot_w > flow.avail as f32 {
                        flow.flush();
                    }
                    let x = *flow.x;
                    flow.pending.push(Pending::Image {
                        x,
                        dest,
                        slot_w,
                        slot_h,
                        start,
                        end,
                        fallback,
                    });
                    *flow.x += slot_w;
                }
            }
        }
        flow.flush();
        let mut art = bands_to_artifact(bands, role);
        if super::tabs::has_tab(text) {
            art.tab_source = Some(std::rc::Rc::from(text));
        }
        art
    }
}
