use super::Aligned;
use md_core::Px;
use md_core::block::BlockId;

pub(super) fn a11y_bounds(layout: Aligned<'_>) -> Vec<(BlockId, (Px, Px, Px, Px))> {
    let mut out = Vec::new();
    for t in layout.texts.iter().filter(|t| t.is_display_surface()) {
        let (cox, coy) = t.content_origin_device;
        out.push((t.block, (cox, coy, t.content_width, t.view_height)));
    }
    for c in &layout.cells {
        let (cox, _) = c.content_origin_device;
        let (_, ry, _, rh) = c.rect_device;
        out.push((c.block, (cox, ry, c.content_width, rh)));
    }
    out
}
