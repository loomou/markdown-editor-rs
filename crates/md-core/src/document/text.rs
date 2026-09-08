use super::arena::TextId;
use super::chars::floor_char_boundary;
use crate::inline::{InlineMarks, InlineRun};
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum TextPiece {
    Source(Range<u32>),
    Intern(Range<u32>),
    Owned,
}

#[derive(Clone, Debug, Default)]
pub struct LeafSnapshot {
    pub display: String,
    pub runs: Vec<InlineRun>,
}

#[derive(Clone, Debug)]
pub enum LeafSource {
    Span(Range<u32>),
    SameAsDisplay,
    Owned(Box<str>),
}

impl Default for LeafSource {
    fn default() -> Self {
        LeafSource::Span(0..0)
    }
}

#[derive(Clone, Debug, Default)]
pub struct LeafText {
    pub pieces: Vec<TextPiece>,
    pub source: LeafSource,
    pub(crate) snapshot: Arc<LeafSnapshot>,
    pub s2d: Vec<usize>,
    pub(crate) constructs: Option<Vec<super::focus::RawConstruct>>,
    pub revision: u64,
}

impl LeafText {
    pub fn display(&self) -> &str {
        &self.snapshot.display
    }

    pub fn runs(&self) -> &[InlineRun] {
        &self.snapshot.runs
    }

    pub fn snapshot(&self) -> Arc<LeafSnapshot> {
        Arc::clone(&self.snapshot)
    }

    pub(crate) fn snapshot_mut(&mut self) -> &mut LeafSnapshot {
        Arc::make_mut(&mut self.snapshot)
    }

    pub(crate) fn source_str<'a>(&'a self, doc_source: &'a str) -> &'a str {
        match &self.source {
            LeafSource::Span(range) => {
                let start = range.start as usize;
                let end = range.end as usize;
                doc_source.get(start..end).unwrap_or("")
            }
            LeafSource::SameAsDisplay => self.display(),
            LeafSource::Owned(s) => s,
        }
    }

    pub(crate) fn set_owned_source(&mut self, source: String) {
        self.constructs = None;
        if source == self.display() {
            self.source = LeafSource::SameAsDisplay;
        } else {
            self.source = LeafSource::Owned(source.into_boxed_str());
        }
    }

    pub(crate) fn append(
        &mut self,
        piece: TextPiece,
        text: &str,
        source_range: Option<Range<u32>>,
        marks: InlineMarks,
        link: Option<u32>,
    ) {
        if text.is_empty() {
            return;
        }
        self.pieces.push(piece);
        let snapshot = self.snapshot_mut();
        let start = snapshot.display.len() as u32;
        snapshot.display.push_str(text);
        let end = snapshot.display.len() as u32;
        if let Some(last) = snapshot.runs.last_mut()
            && last.display_range.end == start
            && last.marks == marks
            && last.link == link
            && !marks.is_atomic()
        {
            last.display_range.end = end;
            if last.source_range != source_range {
                last.source_range = None;
            }
            return;
        }
        snapshot.runs.push(InlineRun {
            display_range: start..end,
            source_range,
            marks,
            link,
        });
    }

    pub(crate) fn trim_trailing_newline(&mut self) {
        if self.display().ends_with('\n') {
            self.pop_trailing_char();
        }
    }

    pub(crate) fn trim_trailing_whitespace(&mut self) {
        while self.display().ends_with([' ', '\t', '\n', '\r']) {
            self.pop_trailing_char();
        }
    }

    fn pop_trailing_char(&mut self) {
        let old_len = self.display().len();
        let Some(_) = self.snapshot_mut().display.pop() else {
            return;
        };
        let at = self.display().len();
        let popped = (old_len - at) as u32;
        {
            let snapshot = self.snapshot_mut();
            if let Some(index) = snapshot.runs.iter().position(|run| {
                (run.display_range.start as usize) <= at && at < run.display_range.end as usize
            }) {
                snapshot.runs[index].display_range.end = at as u32;
                if let Some(sr) = &mut snapshot.runs[index].source_range {
                    sr.end = sr.end.saturating_sub(popped);
                }
                if snapshot.runs[index].display_range.is_empty() {
                    snapshot.runs.remove(index);
                }
            }
        }

        let mut display_at = 0usize;
        let owner = self.pieces.iter().enumerate().find_map(|(index, piece)| {
            let len = match piece {
                TextPiece::Source(range) | TextPiece::Intern(range) => range.len(),
                TextPiece::Owned => old_len.saturating_sub(display_at),
            };
            let owns = at < display_at.saturating_add(len);
            display_at = display_at.saturating_add(len);
            owns.then_some(index)
        });
        if let Some(piece) = owner.and_then(|index| self.pieces.get_mut(index)) {
            match piece {
                TextPiece::Source(r) | TextPiece::Intern(r) => {
                    r.end = r.end.saturating_sub(popped);
                }
                TextPiece::Owned => {}
            }
        }
        let display_empty = self.display().is_empty();
        self.pieces.retain(|piece| {
            let empty = match piece {
                TextPiece::Source(r) | TextPiece::Intern(r) => r.start == r.end,
                TextPiece::Owned => display_empty,
            };
            !empty
        });
    }

    fn inherit_at(&self, pos: u32) -> (InlineMarks, Option<u32>) {
        let snapshot = &self.snapshot;
        if snapshot.runs.is_empty() {
            return (InlineMarks::NONE, None);
        }
        let len = snapshot.display.len() as u32;
        if pos == 0 {
            let r = &snapshot.runs[0];
            return (r.marks, r.link);
        }
        if pos >= len {
            let r = snapshot.runs.last().expect("run");
            return (r.marks, r.link);
        }
        if let Some(r) = snapshot.runs.iter().find(|r| r.display_range.start == pos) {
            return (r.marks, r.link);
        }
        if let Some(r) = snapshot
            .runs
            .iter()
            .find(|r| r.display_range.start < pos && pos < r.display_range.end)
        {
            return (r.marks, r.link);
        }
        snapshot
            .runs
            .last()
            .map(|r| (r.marks, r.link))
            .unwrap_or((InlineMarks::NONE, None))
    }

    fn merge_runs(runs: Vec<InlineRun>) -> Vec<InlineRun> {
        let mut out: Vec<InlineRun> = Vec::new();
        for r in runs {
            if r.display_range.start >= r.display_range.end {
                continue;
            }
            if let Some(last) = out.last_mut()
                && last.display_range.end == r.display_range.start
                && last.marks == r.marks
                && last.link == r.link
                && !r.marks.is_atomic()
            {
                last.display_range.end = r.display_range.end;
                if last.source_range != r.source_range {
                    last.source_range = None;
                }
                continue;
            }
            out.push(r);
        }
        out
    }

    pub(crate) fn replace_display(&mut self, range: Range<usize>, s: &str) {
        let display = self.display();
        let start = floor_char_boundary(display, range.start.min(display.len()));
        let end = floor_char_boundary(display, range.end.min(display.len())).max(start);
        let start_u = start as u32;
        let end_u = end as u32;
        let (marks, link) = self.inherit_at(start_u);
        let delta = s.len() as i32 - (end as i32 - start as i32);
        let shift = |v: u32| (v as i32 + delta).max(0) as u32;
        let mut new_runs = Vec::new();
        for r in self.runs() {
            let rs = r.display_range.start;
            let re = r.display_range.end;
            if re <= start_u {
                new_runs.push(r.clone());
                continue;
            }
            if rs >= end_u {
                new_runs.push(InlineRun {
                    display_range: shift(rs)..shift(re),
                    source_range: None,
                    marks: r.marks,
                    link: r.link,
                });
                continue;
            }
            if rs < start_u {
                new_runs.push(InlineRun {
                    display_range: rs..start_u,
                    source_range: None,
                    marks: r.marks,
                    link: r.link,
                });
            }
            if re > end_u {
                new_runs.push(InlineRun {
                    display_range: shift(end_u)..shift(re),
                    source_range: None,
                    marks: r.marks,
                    link: r.link,
                });
            }
        }
        let snapshot = self.snapshot_mut();
        snapshot.display.replace_range(start..end, s);
        if !s.is_empty() {
            let ins = InlineRun {
                display_range: start_u..start_u + s.len() as u32,
                source_range: None,
                marks,
                link,
            };
            let idx = new_runs
                .iter()
                .position(|r| r.display_range.start >= start_u)
                .unwrap_or(new_runs.len());
            new_runs.insert(idx, ins);
        }
        snapshot.runs = Self::merge_runs(new_runs);
        self.pieces.clear();
        self.pieces.push(TextPiece::Owned);
        self.constructs = None;
        self.revision = self.revision.saturating_add(1).max(1);
    }

    pub(crate) fn set_projected(
        &mut self,
        display: String,
        source: String,
        runs: Vec<InlineRun>,
        constructs: Option<Vec<super::focus::RawConstruct>>,
    ) {
        let runs = Self::normalize_projected_runs(&display, runs);
        self.snapshot = Arc::new(LeafSnapshot {
            display,
            runs: Self::merge_runs(runs),
        });
        self.set_owned_source(source);
        self.constructs = constructs;
        self.pieces.clear();
        self.pieces.push(TextPiece::Owned);
        self.revision = self.revision.saturating_add(1).max(1);
    }

    fn normalize_projected_runs(display: &str, runs: Vec<InlineRun>) -> Vec<InlineRun> {
        let len = display.len() as u32;
        if len == 0 {
            return Vec::new();
        }

        let lead = display.len() - display.trim_start_matches('\n').len();
        let shift_lead = lead > 0
            && runs
                .first()
                .is_some_and(|r| r.display_range.start < lead as u32);
        let prepared = runs.into_iter().filter_map(|mut r| {
            let mut start = r.display_range.start;
            let mut end = r.display_range.end;
            if shift_lead {
                start = start.saturating_add(lead as u32);
                end = end.saturating_add(lead as u32);
                r.source_range = None;
            }
            start = start.min(len);
            end = end.min(len).max(start);
            if start == end {
                None
            } else {
                r.display_range = start..end;
                Some(r)
            }
        });

        let mut out = Vec::new();
        let mut at = 0u32;
        for mut run in prepared {
            let start = run.display_range.start.clamp(at, len);
            let end = run.display_range.end.min(len).max(start);
            if start > at {
                out.push(InlineRun {
                    display_range: at..start,
                    source_range: None,
                    marks: InlineMarks::NONE,
                    link: None,
                });
            }
            if end > start {
                if start != run.display_range.start || end != run.display_range.end {
                    run.source_range = None;
                }
                run.display_range = start..end;
                out.push(run);
                at = end;
            }
        }

        let content_end = display.trim_end_matches('\n').len() as u32;
        if at < content_end {
            out.push(InlineRun {
                display_range: at..content_end,
                source_range: None,
                marks: InlineMarks::NONE,
                link: None,
            });
            at = content_end;
        }
        if at < len {
            out.push(InlineRun {
                display_range: at..len,
                source_range: None,
                marks: InlineMarks::NONE,
                link: None,
            });
        }
        out
    }
}

const TEXT_NONE: u32 = u32::MAX;

#[derive(Clone, Debug)]
pub struct TextStore {
    index: Vec<u32>,
    dense: Vec<LeafText>,
    free: Vec<u32>,
}

impl TextStore {
    pub fn new() -> Self {
        TextStore {
            index: vec![TEXT_NONE],
            dense: Vec::new(),
            free: Vec::new(),
        }
    }

    pub(crate) fn push_slot(&mut self) {
        self.index.push(TEXT_NONE);
    }

    fn ensure_index(&mut self, node_index: u32) {
        let i = node_index as usize;
        if i >= self.index.len() {
            self.index.resize(i + 1, TEXT_NONE);
        }
    }

    fn alloc_dense(&mut self, leaf: LeafText) -> u32 {
        if let Some(i) = self.free.pop() {
            self.dense[i as usize] = leaf;
            return i;
        }
        let i = u32::try_from(self.dense.len()).expect("TextStore dense exhausted");
        if i == TEXT_NONE {
            panic!("TextStore dense index would collide with the TEXT_NONE sentinel");
        }
        self.dense.push(leaf);
        i
    }

    pub(crate) fn init_leaf(&mut self, id: TextId) {
        self.ensure_index(id.index);
        let fresh = LeafText {
            revision: 1,
            ..LeafText::default()
        };
        let existing = self.index[id.index as usize];
        if existing != TEXT_NONE {
            self.dense[existing as usize] = fresh;
            return;
        }
        let dense = self.alloc_dense(fresh);
        self.index[id.index as usize] = dense;
    }

    pub(crate) fn clear_slot(&mut self, index: u32) {
        let Some(slot) = self.index.get_mut(index as usize) else {
            return;
        };
        let dense = *slot;
        if dense == TEXT_NONE {
            return;
        }
        *slot = TEXT_NONE;
        self.dense[dense as usize] = LeafText::default();
        self.free.push(dense);
    }

    pub fn get(&self, id: TextId) -> Option<&LeafText> {
        let dense = *self.index.get(id.index as usize)?;
        if dense == TEXT_NONE {
            return None;
        }
        self.dense.get(dense as usize)
    }

    pub fn get_mut(&mut self, id: TextId) -> Option<&mut LeafText> {
        let dense = *self.index.get(id.index as usize)?;
        if dense == TEXT_NONE {
            return None;
        }
        self.dense.get_mut(dense as usize)
    }

    pub(crate) fn shrink_runs_and_pieces(&mut self) {
        for leaf in &mut self.dense {
            if let Some(snapshot) = Arc::get_mut(&mut leaf.snapshot) {
                snapshot.runs.shrink_to_fit();
            }
            leaf.pieces.shrink_to_fit();
        }
    }

    #[cfg(test)]
    pub(crate) fn index_len(&self) -> usize {
        self.index.len()
    }

    #[cfg(test)]
    pub(crate) fn dense_len(&self) -> usize {
        self.dense.len()
    }

    #[cfg(test)]
    pub(crate) fn free_len(&self) -> usize {
        self.free.len()
    }

    #[cfg(test)]
    pub(crate) fn occupied(&self) -> usize {
        self.index.iter().filter(|&&i| i != TEXT_NONE).count()
    }
}

#[cfg(test)]
mod shrink_tests {
    use super::{LeafText, TextId, TextPiece, TextStore};
    use crate::document::arena::NodeId;
    use crate::inline::{InlineMarks, InlineRun};

    fn spare_leaf() -> (TextStore, TextId) {
        let mut store = TextStore::new();
        let id = NodeId::at(1, 1).text_id();
        store.push_slot();
        store.init_leaf(id);
        let leaf = store.get_mut(id).expect("leaf");
        leaf.snapshot_mut().runs.reserve(64);
        leaf.pieces.reserve(64);
        leaf.snapshot_mut().runs.push(InlineRun {
            display_range: 0..1,
            source_range: None,
            marks: InlineMarks::NONE,
            link: None,
        });
        leaf.pieces.push(TextPiece::Owned);
        (store, id)
    }

    #[test]
    fn shrink_runs_and_pieces_drops_spare_capacity() {
        let (mut store, id) = spare_leaf();
        {
            let leaf = store.get(id).expect("before");
            assert!(leaf.snapshot.runs.capacity() >= 64);
            assert!(leaf.pieces.capacity() >= 64);
        }
        store.shrink_runs_and_pieces();
        let leaf = store.get(id).expect("after");
        assert_eq!(leaf.snapshot.runs.capacity(), leaf.runs().len());
        assert_eq!(leaf.pieces.capacity(), leaf.pieces.len());
    }

    #[test]
    fn shrink_runs_and_pieces_keeps_shared_snapshots_shared() {
        let (mut store, id) = spare_leaf();
        let shared = store.get(id).expect("before").snapshot();
        let runs_capacity = shared.runs.capacity();

        store.shrink_runs_and_pieces();

        let leaf = store.get(id).expect("after");
        assert!(std::sync::Arc::ptr_eq(&shared, &leaf.snapshot));
        assert_eq!(leaf.snapshot.runs.capacity(), runs_capacity);
        assert_eq!(leaf.pieces.capacity(), leaf.pieces.len());
    }

    #[test]
    fn containers_do_not_occupy_dense_slots() {
        let mut store = TextStore::new();
        store.push_slot();
        store.push_slot();
        let leaf_id = NodeId::at(2, 1).text_id();
        store.init_leaf(leaf_id);
        assert_eq!(store.index_len(), 3);
        assert_eq!(store.dense_len(), 1);
        assert_eq!(store.occupied(), 1);
        assert!(store.get(NodeId::at(1, 1).text_id()).is_none());
        assert!(store.get(leaf_id).is_some());
    }

    #[test]
    fn clear_slot_reuses_dense_via_free_list() {
        let mut store = TextStore::new();
        store.push_slot();
        let id = NodeId::at(1, 1).text_id();
        store.init_leaf(id);
        assert_eq!(store.dense_len(), 1);
        store.clear_slot(1);
        assert!(store.get(id).is_none());
        assert_eq!(store.free_len(), 1);
        assert_eq!(store.occupied(), 0);
        store.init_leaf(id);
        assert_eq!(store.dense_len(), 1);
        assert_eq!(store.free_len(), 0);
        assert!(store.get(id).is_some());
    }

    #[test]
    fn init_leaf_overwrites_in_place() {
        let mut store = TextStore::new();
        store.push_slot();
        let id = NodeId::at(1, 1).text_id();
        store.init_leaf(id);
        store
            .get_mut(id)
            .expect("leaf")
            .pieces
            .push(TextPiece::Owned);
        store.init_leaf(id);
        assert_eq!(store.dense_len(), 1);
        assert!(store.get(id).expect("leaf").pieces.is_empty());
    }

    #[test]
    fn trim_trailing_newline_finds_the_owning_run_and_piece() {
        let mut leaf = LeafText::default();
        leaf.snapshot_mut().display = "a\n".into();
        leaf.snapshot_mut().runs = vec![
            InlineRun {
                display_range: 1..2,
                source_range: None,
                marks: InlineMarks::NONE,
                link: None,
            },
            InlineRun {
                display_range: 0..1,
                source_range: None,
                marks: InlineMarks::STRONG,
                link: None,
            },
        ];
        leaf.pieces = vec![TextPiece::Source(10..12), TextPiece::Intern(20..20)];

        leaf.trim_trailing_newline();

        assert_eq!(leaf.display(), "a");
        assert_eq!(leaf.runs().len(), 1);
        assert_eq!(leaf.runs()[0].display_range, 0..1);
        assert_eq!(leaf.pieces.len(), 1);
        assert!(matches!(&leaf.pieces[0], TextPiece::Source(range) if *range == (10..11)));
    }
}
