use super::kp::Plan;
use super::{Item, Placed};

pub fn glyph_shifts(plan: &Plan, items: &[Placed]) -> Vec<(u32, f32)> {
    let mut out = Vec::new();
    let mut accumulated = 0.0f32;
    let count = plan.lines.len();
    for (index, line) in plan.lines.iter().enumerate() {
        if line.overfull || line.forced || index + 1 == count || !line.ratio.is_finite() {
            continue;
        }
        let ratio = line.ratio.clamp(-1.0, f32::INFINITY);
        if ratio == 0.0 {
            continue;
        }
        let start = line.item_start as usize;
        let end = (line.item_end as usize).max(start);
        for at in start..end {
            let Item::Glue {
                stretch, shrink, ..
            } = items[at].item
            else {
                continue;
            };
            let shift = if ratio > 0.0 {
                ratio * stretch
            } else {
                ratio * shrink
            };
            if shift == 0.0 {
                continue;
            }
            accumulated += shift;
            out.push((items[at + 1].byte, accumulated));
        }
    }
    out
}
