use super::Aligned;
use md_content::shaper::{GpuiShaper, ShapeArtifact};
use md_core::Px;
use md_core::block::{BlockId, BlockKind};
use md_core::doc::Cursor;
use md_layout::box_tree::BoxRole;

fn row_at_y(art: &ShapeArtifact, y: Px) -> u32 {
    if art.bands.is_empty() {
        return (y / art.row_advance).floor().max(0.0) as u32;
    }
    let mut acc: Px = 0.0;
    for (i, band) in art.bands.iter().enumerate() {
        acc += band.height;
        if y < acc {
            return i as u32;
        }
    }
    art.bands.len() as u32
}

pub(super) fn hit_test(
    layout: Aligned<'_>,
    px_pt: (Px, Px),
    shaper: &GpuiShaper,
    well_scroll: impl Fn(BlockId) -> (Px, Px),
) -> Option<Cursor> {
    let (x, y) = px_pt;

    for c in &layout.cells {
        let (rx, ry, rw, rh) = c.rect_device;
        if x >= rx && x < rx + rw && y >= ry && y < ry + rh {
            let (cox, coy) = c.content_origin_device;
            let rows = c.art.rows.max(1);
            let row = row_at_y(&c.art, y - coy).min(rows - 1);
            let off = shaper.offset_for_position(&c.art, x - cox, row, c.align, c.content_width);
            return Some(Cursor {
                block: c.block,
                offset: off,
            });
        }
    }
    for d in &layout.decorations {
        if !matches!(d.role, BoxRole::Bar | BoxRole::Slot) {
            continue;
        }
        let (rx, ry, rw, rh) = d.rect_device;
        if x >= rx && x < rx + rw && y >= ry && y < ry + rh {
            return Some(Cursor {
                block: d.hit_block,
                offset: 0,
            });
        }
    }
    for t in layout.texts.iter().filter(|t| t.accepts_caret()) {
        let (cox, coy) = t.content_origin_device;
        let h = t.view_height;
        if y >= coy && y < coy + h {
            let (sx, sy) = well_scroll(t.block);
            let rows = t.art.rows.max(1);
            let row = row_at_y(&t.art, y - coy + sy).min(rows - 1);
            let off = shaper.offset_for_position(
                &t.art,
                (x - cox + sx).max(0.0),
                row,
                t.align,
                t.content_width,
            );
            return Some(Cursor {
                block: t.block,
                offset: off,
            });
        }
    }

    let mut best: Option<(Px, Px, Cursor)> = None;
    let consider = |best: &mut Option<(Px, Px, Cursor)>, dy: Px, dx: Px, c: Cursor| {
        let better = match *best {
            None => true,
            Some((bdy, bdx, _)) => (dy, dx) < (bdy, bdx),
        };
        if better {
            *best = Some((dy, dx, c));
        }
    };
    for t in layout.texts.iter().filter(|t| t.accepts_caret()) {
        let (cox, coy) = t.content_origin_device;
        let h = t.view_height.max(t.art.row_advance);
        let dy = if y < coy {
            coy - y
        } else if y > coy + h {
            y - coy - h
        } else {
            0.0
        };
        let dx = if x < cox {
            cox - x
        } else if x > cox + t.content_width {
            x - cox - t.content_width
        } else {
            0.0
        };
        let (sx, sy) = well_scroll(t.block);
        let rows = t.art.rows.max(1);
        let row = row_at_y(&t.art, y - coy + sy).min(rows - 1);
        let off = shaper.offset_for_position(
            &t.art,
            (x - cox + sx).max(0.0),
            row,
            t.align,
            t.content_width,
        );
        consider(
            &mut best,
            dy,
            dx,
            Cursor {
                block: t.block,
                offset: off,
            },
        );
    }
    for c in &layout.cells {
        let (cox, coy) = c.content_origin_device;
        let h = c.art.height.max(c.art.row_advance);
        let dy = if y < coy {
            coy - y
        } else if y > coy + h {
            y - coy - h
        } else {
            0.0
        };
        let dx = if x < cox {
            cox - x
        } else if x > cox + c.content_width {
            x - cox - c.content_width
        } else {
            0.0
        };
        let rows = c.art.rows.max(1);
        let row = row_at_y(&c.art, y - coy).min(rows - 1);
        let off =
            shaper.offset_for_position(&c.art, (x - cox).max(0.0), row, c.align, c.content_width);
        consider(
            &mut best,
            dy,
            dx,
            Cursor {
                block: c.block,
                offset: off,
            },
        );
    }
    best.map(|(_, _, c)| c)
}

pub(super) fn hit_list_item_slot(layout: Aligned<'_>, px_pt: (Px, Px)) -> Option<BlockId> {
    let (x, y) = px_pt;
    for d in &layout.decorations {
        if d.role != BoxRole::Slot || d.kind != BlockKind::ListItem || d.task.is_none() {
            continue;
        }
        let (rx, ry, rw, rh) = d.rect_device;
        if x >= rx && x < rx + rw && y >= ry && y < ry + rh {
            return Some(d.hit_block);
        }
    }
    None
}
