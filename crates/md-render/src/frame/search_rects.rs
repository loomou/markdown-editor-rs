use super::geometry::VisibleGeom;
use super::request::Pass;
use super::rows::{PlacedText, row_bands};
use crate::search::{SearchMatch, find_in_display};
use crate::snapshot::DeviceRect;

pub(super) struct SearchHighlights {
    pub(super) rest: Vec<DeviceRect>,
    pub(super) active: Vec<DeviceRect>,
}

pub(super) fn search_highlights_device(
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
        push_search_rects(pass, p, query, skip, &mut hl);
    }
    hl
}

fn push_search_rects(
    pass: &Pass<'_>,
    p: PlacedText<'_>,
    query: &str,
    skip: Option<SearchMatch>,
    out: &mut SearchHighlights,
) {
    let hay = pass.assembly.tree.text(p.box_id);
    for r in find_in_display(hay, query) {
        let is_active =
            skip.is_some_and(|s| s.block == p.block && s.start == r.start && s.end == r.end);
        let start = r.start;
        let end = r.end;
        let (ox, oy) = p.origin;
        for band in row_bands(pass.shaper, p.art, start..end, p.align, p.inner) {
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
}
