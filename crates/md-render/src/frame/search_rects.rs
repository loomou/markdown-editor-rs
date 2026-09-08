use super::geometry::VisibleGeom;
use super::request::Pass;
use super::rows::{PlacedText, row_bands};
use crate::search::{SearchMatch, find_in_display};
use crate::snapshot::DeviceRect;
use md_core::doc::Doc;

pub(super) struct SearchHighlights {
    pub(super) rest: Vec<DeviceRect>,
    pub(super) active: Vec<DeviceRect>,
}

pub(super) fn search_highlights_device(
    doc: &Doc,
    pass: &Pass<'_>,
    geom: &VisibleGeom,
    query: &str,
    skip: Option<SearchMatch>,
) -> SearchHighlights {
    let mut hl = SearchHighlights {
        rest: Vec::new(),
        active: Vec::new(),
    };
    if query.is_empty() {
        return hl;
    }
    let placed = geom
        .texts
        .iter()
        .filter(|t| t.accepts_caret())
        .map(PlacedText::of_text)
        .chain(geom.cells.iter().map(PlacedText::of_cell));
    for p in placed {
        push_search_rects(doc, pass, p, query, skip, &mut hl);
    }
    hl
}

fn push_search_rects(
    doc: &Doc,
    pass: &Pass<'_>,
    p: PlacedText<'_>,
    query: &str,
    skip: Option<SearchMatch>,
    out: &mut SearchHighlights,
) {
    let hit_is_active = |m: &SearchMatch| {
        skip.is_some_and(|s| s.block == m.block && s.start == m.start && s.end == m.end)
    };
    if p.edit_source {
        let hay = pass.assembly.tree.text(p.box_id);
        for r in find_in_display(hay, query) {
            let m = SearchMatch {
                block: p.block,
                start: r.start,
                end: r.end,
            };
            push_bands(pass, p, r.start..r.end, hit_is_active(&m), out);
        }
        return;
    }
    let Some(collapsed) = doc.collapsed_text(p.block) else {
        return;
    };
    for r in find_in_display(collapsed, query) {
        let m = SearchMatch {
            block: p.block,
            start: r.start,
            end: r.end,
        };
        let visual = doc.visual_range(p.block, r.start..r.end);
        if visual.is_empty() {
            continue;
        }
        push_bands(pass, p, visual, hit_is_active(&m), out);
    }
}

fn push_bands(
    pass: &Pass<'_>,
    p: PlacedText<'_>,
    range: std::ops::Range<usize>,
    is_active: bool,
    out: &mut SearchHighlights,
) {
    let (ox, oy) = p.origin;
    for band in row_bands(pass.shaper, p.art, range, p.align, p.inner) {
        let target = if is_active {
            &mut out.active
        } else {
            &mut out.rest
        };
        target.push((
            ox + band.start_x,
            oy + p.art.row_top(band.row),
            band.width(),
            p.art.row_height(band.row),
        ));
    }
}
