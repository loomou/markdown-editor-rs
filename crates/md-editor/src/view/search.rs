use super::EditorView;
use md_core::doc::Cursor;
use md_render::search::{SearchMatch, SearchScan, scan_document};

fn active_for_cursor(matches: &[SearchMatch], cur: Cursor) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    matches
        .iter()
        .position(|m| m.block > cur.block || (m.block == cur.block && m.end > cur.offset))
        .or(Some(0))
}

impl EditorView {
    fn match_index_for_selection(&self) -> Option<usize> {
        let (a, b) = self.state.selection?;
        if a.block != b.block {
            return None;
        }
        let (s, e) = if a.offset <= b.offset {
            (a.offset, b.offset)
        } else {
            (b.offset, a.offset)
        };
        self.search
            .matches
            .iter()
            .position(|m| m.block == a.block && m.start == s && m.end == e)
    }

    pub(crate) fn set_search_query(&mut self, query: &str) {
        self.search.query = query.to_string();
        match scan_document(&self.state.doc, query) {
            SearchScan::Hits(hits) => {
                self.search.matches = hits;
                self.search.active = Some(0);
                self.search.capped = false;
                self.jump_to_active(1);
            }
            SearchScan::Capped => {
                self.search.matches.clear();
                self.search.active = None;
                self.search.capped = true;
                self.search.reveal = None;
                self.search.refresh = 0;
            }
            SearchScan::Empty => {
                self.search.matches.clear();
                self.search.active = None;
                self.search.capped = false;
                self.search.reveal = None;
                self.search.refresh = 0;
            }
        }
    }

    pub(crate) fn refresh_search(&mut self, query: &str) {
        self.search.query = query.to_string();
        match scan_document(&self.state.doc, query) {
            SearchScan::Hits(hits) => {
                self.search.matches = hits;
                self.search.capped = false;
                self.search.active = self
                    .match_index_for_selection()
                    .or_else(|| active_for_cursor(&self.search.matches, self.state.cursor));
            }
            SearchScan::Capped => {
                self.search.matches.clear();
                self.search.active = None;
                self.search.capped = true;
                self.search.reveal = None;
                self.search.refresh = 0;
            }
            SearchScan::Empty => {
                self.search.matches.clear();
                self.search.active = None;
                self.search.capped = false;
                self.search.reveal = None;
                self.search.refresh = 0;
            }
        }
    }

    pub(crate) fn search_step(&mut self, dir: i32) {
        let n = self.search.matches.len();
        if n == 0 {
            return;
        }
        let i = match self.search.active {
            Some(i) if self.match_index_for_selection() == Some(i) => {
                (i as i32 + dir).rem_euclid(n as i32) as usize
            }

            _ => {
                let i = active_for_cursor(&self.search.matches, self.state.cursor).unwrap_or(0);
                if dir < 0 {
                    (i as i32 - 1).rem_euclid(n as i32) as usize
                } else {
                    i
                }
            }
        };
        self.search.active = Some(i);
        self.jump_to_active(dir);
    }

    pub(crate) fn clear_search(&mut self) {
        self.search_open = false;
        self.search.query.clear();
        self.search.matches.clear();
        self.search.active = None;
        self.search.capped = false;
        self.search.reveal = None;
        self.search.refresh = 0;
    }

    fn jump_to_active(&mut self, dir: i32) {
        self.abort_composing();
        let Some(i) = self.search.active else {
            return;
        };
        let m = self.search.matches[i];

        let range = self.state.doc.visual_range(m.block, m.start..m.end);
        let a = Cursor {
            block: m.block,
            offset: range.start,
        };
        let b = Cursor {
            block: m.block,
            offset: range.end,
        };
        let (a, b) = self
            .state
            .doc
            .retarget_focus_range(a, b, md_core::doc::FocusBias::Neutral);
        self.state.cursor = b;
        self.select_anchor = Some(a);
        self.state.selection = if a == b { None } else { Some((a, b)) };
        self.wake_caret();
        self.follow_caret = false;
        self.park_caret_top = None;
        self.search.reveal = Some((if dir < 0 { -1 } else { 1 }, 0));
        self.search.refresh = 4;
    }
}

#[cfg(test)]
mod tests {
    use super::active_for_cursor;
    use md_core::doc::Cursor;
    use md_render::search::SearchMatch;

    #[test]
    fn active_wraps_from_end() {
        let matches = [
            SearchMatch {
                block: 1,
                start: 0,
                end: 1,
            },
            SearchMatch {
                block: 2,
                start: 0,
                end: 1,
            },
        ];
        assert_eq!(
            active_for_cursor(
                &matches,
                Cursor {
                    block: 2,
                    offset: 1
                }
            ),
            Some(0)
        );
        assert_eq!(
            active_for_cursor(
                &matches,
                Cursor {
                    block: 1,
                    offset: 0
                }
            ),
            Some(0)
        );
    }
}
