mod a11y;
mod hit;

#[cfg(test)]
mod hit_tests;
#[cfg(test)]
mod tests;

use crate::snapshot::LayoutSnapshot;
use md_content::shaper::GpuiShaper;
use md_core::Px;
use md_core::block::BlockId;
use md_core::doc::Cursor;
use std::ops::Deref;

pub(crate) struct Aligned<'a>(&'a LayoutSnapshot);

impl<'a> Aligned<'a> {
    fn new(layout: &'a LayoutSnapshot, paint_revision: u64) -> Option<Self> {
        if layout.geometry_revision == paint_revision {
            Some(Self(layout))
        } else {
            md_layout::hot_path::add_geometry_mismatch();
            None
        }
    }
}

impl Deref for Aligned<'_> {
    type Target = LayoutSnapshot;

    fn deref(&self) -> &LayoutSnapshot {
        self.0
    }
}

pub fn hit_test(
    layout: &LayoutSnapshot,
    paint_revision: u64,
    px_pt: (Px, Px),
    shaper: &GpuiShaper,
    well_scroll: impl Fn(BlockId) -> (Px, Px),
) -> Option<Cursor> {
    hit::hit_test(
        Aligned::new(layout, paint_revision)?,
        px_pt,
        shaper,
        well_scroll,
    )
}

pub fn hit_list_item_slot(
    layout: &LayoutSnapshot,
    paint_revision: u64,
    px_pt: (Px, Px),
) -> Option<BlockId> {
    hit::hit_list_item_slot(Aligned::new(layout, paint_revision)?, px_pt)
}

pub fn a11y_bounds(
    layout: &LayoutSnapshot,
    paint_revision: u64,
) -> Vec<(BlockId, (Px, Px, Px, Px))> {
    match Aligned::new(layout, paint_revision) {
        Some(aligned) => a11y::a11y_bounds(aligned),
        None => Vec::new(),
    }
}
