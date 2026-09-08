mod matcher;

pub(crate) use matcher::find_in_display;
use matcher::{Needle, find_with};

use md_core::block::BlockId;
use md_core::doc::Doc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchMatch {
    pub block: BlockId,
    pub start: usize,
    pub end: usize,
}

const SEARCH_MATCH_CAP: usize = 5000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchScan {
    Empty,
    Hits(Vec<SearchMatch>),
    Capped,
}

pub fn scan_document(doc: &Doc, query: &str) -> SearchScan {
    if query.is_empty() {
        return SearchScan::Empty;
    }
    let needle = Needle::new(query);
    let mut out = Vec::new();
    let mut capped = false;
    doc.for_each_collapsed_text_leaf(|id, text| {
        let room = SEARCH_MATCH_CAP + 1 - out.len();
        for r in find_with(text, &needle, room) {
            out.push(SearchMatch {
                block: id,
                start: r.start,
                end: r.end,
            });
        }
        if out.len() > SEARCH_MATCH_CAP {
            capped = true;
            false
        } else {
            true
        }
    });
    if capped {
        SearchScan::Capped
    } else if out.is_empty() {
        SearchScan::Empty
    } else {
        SearchScan::Hits(out)
    }
}

#[cfg(test)]
mod tests {
    use super::{SEARCH_MATCH_CAP, SearchScan, scan_document};
    use md_core::doc::{Cursor, Doc};
    use md_core::document::{editor_options, load_markdown};

    #[test]
    fn scan_caps_above_limit() {
        let md = "x".repeat(SEARCH_MATCH_CAP + 1);
        let doc = Doc::new(load_markdown(&md, editor_options()));
        assert_eq!(scan_document(&doc, "x"), SearchScan::Capped);
        let md = "x".repeat(SEARCH_MATCH_CAP);
        let doc = Doc::new(load_markdown(&md, editor_options()));
        match scan_document(&doc, "x") {
            SearchScan::Hits(v) => assert_eq!(v.len(), SEARCH_MATCH_CAP),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn scan_uses_collapsed_display_while_focused() {
        let mut doc = Doc::new(load_markdown("a**b**c\n", editor_options()));
        let leaf = doc.text_leaves()[0];
        let _ = doc.retarget_focus(Cursor {
            block: leaf,
            offset: 1,
        });
        assert_eq!(doc.text(leaf).unwrap(), "a**b**c");
        assert_eq!(scan_document(&doc, "**"), SearchScan::Empty);
        match scan_document(&doc, "b") {
            SearchScan::Hits(v) => {
                assert_eq!(v.len(), 1);
                assert_eq!(v[0].start, 1);
                assert_eq!(v[0].end, 2);
            }
            other => panic!("{other:?}"),
        }
    }
}
