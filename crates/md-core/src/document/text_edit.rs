use super::arena::NodeId;
use super::change::{ChangeSet, DocChange};
use super::chars::floor_char_boundary;
use super::edit::Caret;
use super::word::prev_grapheme_boundary;
use super::{Document, bind, editor_options, load_markdown, syntax, text};
use crate::block::{BlockId, BlockKind, CodeFenceMarker, NodeExtra, TextEditStrategy};
use crate::inline::InlineRun;
use std::ops::Range;

fn marker_fence_char(marker: CodeFenceMarker) -> char {
    match marker {
        CodeFenceMarker::Backtick => '`',
        CodeFenceMarker::Tilde => '~',
    }
}

type CellProjection = (
    String,
    Vec<InlineRun>,
    Vec<usize>,
    Option<Vec<super::focus::RawConstruct>>,
);

impl Document {
    pub(super) fn remap_runs(&mut self, frag: &Document, leaf: NodeId) -> Vec<InlineRun> {
        frag.runs(leaf)
            .iter()
            .map(|r| InlineRun {
                display_range: r.display_range.clone(),
                source_range: r.source_range.clone(),
                marks: r.marks,
                link: r.link.map(|i| self.remap_link(frag, i)),
            })
            .collect()
    }

    pub(super) fn project_phrasing_at(
        &mut self,
        id: NodeId,
        source: String,
        caret_src: usize,
        edit: (Range<u32>, String, String),
        focus: bool,
    ) -> (DocChange, usize) {
        self.reproject(id, source, caret_src, edit, focus)
    }

    pub(super) fn reproject(
        &mut self,
        id: NodeId,
        source: String,
        caret_src: usize,
        edit: (Range<u32>, String, String),
        focus: bool,
    ) -> (DocChange, usize) {
        let old_revision = self.arena.get(id).map(|n| n.content_revision).unwrap_or(1);
        let current = self
            .arena
            .get(id)
            .map(|n| n.kind)
            .unwrap_or(BlockKind::Paragraph);
        let (display, runs, s2d, constructs) = if current == BlockKind::TableCell
            && let Some(cell) = self.table_cell_projection(&source)
        {
            cell
        } else {
            let defs = std::sync::Arc::clone(&self.reference_definitions);
            let stripped = if matches!(current, BlockKind::Paragraph | BlockKind::Heading(_)) {
                source.trim_start_matches([' ', '\t'])
            } else {
                source.as_str()
            };
            let frag = load_markdown(&bind::with_definitions(stripped, &defs), editor_options());
            let parsed_kind = match current {
                BlockKind::TableCell => BlockKind::Paragraph,
                kind => kind,
            };
            if let Some(leaf) = bind::matching_leaf(&frag, parsed_kind) {
                let display = frag.display(leaf).to_string();
                let runs = self.remap_runs(&frag, leaf);
                let s2d = if current == BlockKind::TableCell {
                    bind::source_to_display_map_for_kind(&source, current, &defs)
                } else {
                    let mut s2d = bind::source_to_display_map(stripped, &defs);
                    let pad = source.len() - stripped.len();
                    s2d.splice(0..0, std::iter::repeat_n(0, pad));
                    s2d
                };
                let pad = source.len() - stripped.len();
                let constructs = if frag.leaf_source(leaf) == stripped {
                    frag.recorded_constructs(leaf).map(|cs| {
                        cs.iter()
                            .map(|c| super::focus::RawConstruct {
                                source: c.source.start + pad..c.source.end + pad,
                                inner: c.inner.start + pad..c.inner.end + pad,
                            })
                            .collect()
                    })
                } else {
                    Some(
                        super::focus::raw_constructs(stripped, &defs)
                            .into_iter()
                            .map(|c| super::focus::RawConstruct {
                                source: c.source.start + pad..c.source.end + pad,
                                inner: c.inner.start + pad..c.inner.end + pad,
                            })
                            .collect(),
                    )
                };
                let (display, s2d, runs) =
                    bind::restore_visible_ws(current, &source, display, s2d, runs);
                (display, runs, s2d, constructs)
            } else {
                let n = source.len();
                (
                    source.clone(),
                    bind::identity_runs(n),
                    bind::identity_map(n),
                    Some(Vec::new()),
                )
            }
        };
        let caret_src = caret_src.min(source.len());
        let caret = if s2d.last().copied().unwrap_or(0) == display.len() {
            bind::source_to_display(&s2d, caret_src)
        } else {
            caret_src.min(display.len())
        };
        let caret = floor_char_boundary(&display, caret);
        let header_cell = current == BlockKind::TableCell
            && self
                .arena
                .get(id)
                .map(|n| n.extra.table_header())
                .unwrap_or(false);
        if let Some(leaf) = self.texts.get_mut(id.text_id()) {
            leaf.set_projected(display, source, runs, constructs);
            leaf.s2d = s2d;
            if header_cell {
                let snapshot = leaf.snapshot_mut();
                for run in &mut snapshot.runs {
                    run.marks = run.marks.union(crate::inline::InlineMarks::STRONG);
                }
            }
        }
        let caret = if focus {
            self.apply_inline_focus(id, caret, Some(caret_src), super::FocusBias::Neutral, true)
        } else if let Some(shifted) = self
            .focus
            .as_ref()
            .filter(|f| f.node == id)
            .map(|f| Self::shift_source_offset(f.span.start, &edit))
        {
            self.apply_inline_focus(id, caret, Some(shifted), super::FocusBias::Neutral, true)
        } else {
            caret
        };
        let new_revision = self.bump_content(id);
        let (range, deleted, inserted) = edit;
        (
            DocChange::text(id, old_revision, new_revision, range, deleted, inserted),
            caret,
        )
    }

    pub(super) fn shift_source_offset(offset: usize, edit: &(Range<u32>, String, String)) -> usize {
        let (range, _, inserted) = edit;
        let start = range.start as usize;
        let end = range.end as usize;
        if offset <= start {
            offset
        } else if offset < end {
            start
        } else {
            offset + inserted.len() + start - end
        }
    }

    fn table_cell_projection(&mut self, source: &str) -> Option<CellProjection> {
        let escaped = escape_cell_source(source).replace('\n', "<br>");
        let wrapped = format!("| h |\n| --- |\n| {escaped} |\n");
        let defs = std::sync::Arc::clone(&self.reference_definitions);
        let frag = load_markdown(&bind::with_definitions(&wrapped, &defs), editor_options());
        let mut cells = frag
            .preorder()
            .into_iter()
            .filter(|&id| frag.arena.get(id).map(|n| n.kind) == Some(BlockKind::TableCell));
        cells.next()?;
        let cell = cells.next()?;
        let cell_source = frag.leaf_source(cell);
        let lead = source.len() - source.trim_start_matches(' ').len();
        let trail = (source.len() - source.trim_end_matches(' ').len()).min(source.len() - lead);
        let esc_core = escaped.trim_matches(' ');
        if cell_source.trim_matches(' ') != esc_core {
            return None;
        }
        let mut display = frag.display(cell).to_string();
        let mut runs = self.remap_runs(&frag, cell);
        let frag_s2d = frag.collapsed_s2d(cell);
        let core_len = display.len();
        let mut padded = String::with_capacity(source.len() + core_len);
        padded.push_str(&" ".repeat(lead));
        padded.push_str(&display);
        padded.push_str(&" ".repeat(trail));
        display = padded;
        let core = &source[lead..source.len() - trail];
        let mut esc2src = vec![0usize; esc_core.len() + 1];
        {
            let mut e = 0usize;
            let mut s = 0usize;
            let mut chars = core.chars().peekable();
            while let Some(ch) = chars.next() {
                let (sw, ew) = match ch {
                    '\\' if matches!(chars.peek(), Some(p) if p.is_ascii_punctuation()) => {
                        chars.next();
                        (2, 2)
                    }
                    '\n' => (1, 4),
                    '\\' | '|' => (1, 2),
                    _ => {
                        let n = ch.len_utf8();
                        (n, n)
                    }
                };
                if sw == ew {
                    for t in 0..ew {
                        esc2src[e + t] = s + t;
                    }
                } else {
                    for t in 1..ew {
                        esc2src[e + t] = s;
                    }
                }
                e += ew;
                s += sw;
                esc2src[e] = s;
            }
        }
        let last = esc2src.len() - 1;
        for run in &mut runs {
            if let Some(r) = run.source_range.as_mut() {
                *r = (lead + esc2src[(r.start as usize).min(last)]) as u32
                    ..(lead + esc2src[(r.end as usize).min(last)]) as u32;
            }
            run.display_range.start += lead as u32;
            run.display_range.end += lead as u32;
        }
        let mut s2d = Vec::with_capacity(source.len() + 1);
        s2d.extend(0..lead);
        {
            let mut e = 0usize;
            let mut chars = core.chars().peekable();
            while let Some(ch) = chars.next() {
                match ch {
                    '\\' if matches!(chars.peek(), Some(p) if p.is_ascii_punctuation()) => {
                        chars.next();
                        s2d.push(frag_s2d[e] + lead);
                        s2d.push(frag_s2d[e + 1] + lead);
                        e += 2;
                    }
                    '\n' => {
                        s2d.push(frag_s2d[e] + lead);
                        e += 4;
                    }
                    '\\' | '|' => {
                        s2d.push(frag_s2d[e] + lead);
                        e += 2;
                    }
                    _ => {
                        let n = ch.len_utf8();
                        s2d.extend(frag_s2d[e..e + n].iter().map(|&d| d + lead));
                        e += n;
                    }
                }
            }
            s2d.push(frag_s2d.last().copied().unwrap_or(0) + lead);
        }
        for k in 1..=trail {
            s2d.push(lead + core_len + k);
        }
        let to_src = |r: Range<usize>| -> Range<usize> {
            lead + esc2src[r.start.min(last)]..lead + esc2src[r.end.min(last)]
        };
        let constructs = frag.recorded_constructs(cell).map(|cs| {
            cs.iter()
                .map(|c| super::focus::RawConstruct {
                    source: to_src(c.source.clone()),
                    inner: to_src(c.inner.clone()),
                })
                .collect()
        });
        Some((display, runs, s2d, constructs))
    }

    pub(super) fn reproject_as_paragraph(&mut self, id: NodeId) -> DocChange {
        let source = self.leaf_source(id).to_string();
        let n = source.len() as u32;
        if let Some(n) = self.arena.get_mut(id) {
            n.kind = BlockKind::Paragraph;
            n.extra = NodeExtra::None;
        }
        self.reproject(id, source.clone(), 0, (0..n, source.clone(), source), false)
            .0
    }

    fn apply_source_edit(
        &mut self,
        id: NodeId,
        s0: usize,
        s1: usize,
        s: &str,
        focus: bool,
    ) -> (DocChange, usize) {
        let mut source = self.leaf_source(id).to_string();
        let s0 = floor_char_boundary(&source, s0.min(source.len()));
        let s1 = floor_char_boundary(&source, s1.min(source.len())).max(s0);
        let deleted = source.get(s0..s1).unwrap_or("").to_string();
        let inserted = s.to_string();
        let range = s0 as u32..s1 as u32;
        source.replace_range(s0..s1, s);
        let caret_src = (s0 + s.len()).min(source.len());
        self.project_phrasing_at(id, source, caret_src, (range, deleted, inserted), focus)
    }

    fn display_range_to_source(
        display_len: usize,
        source_len: usize,
        s2d: &[usize],
        start: usize,
        end: usize,
        leading_construct: bool,
    ) -> (usize, usize) {
        let mapped = s2d.last().copied().unwrap_or(0) == display_len;
        if start == end && start == display_len && !mapped {
            (source_len, source_len)
        } else if start == end {
            let a = bind::display_to_source_inner(s2d, start);
            (a, a)
        } else {
            let mut a = if leading_construct {
                bind::display_to_source_first(s2d, start)
            } else {
                bind::display_to_source_inner(s2d, start)
            };
            let mut b = if end == display_len {
                bind::display_to_source_outer(s2d, end)
            } else {
                bind::display_to_source_inner(s2d, end)
            };
            if a > b {
                std::mem::swap(&mut a, &mut b);
            }
            (a, b)
        }
    }

    fn ensure_heading_delimiter(source: &mut String, kind: BlockKind, display: &str) -> bool {
        matches!(kind, BlockKind::Heading(_))
            && display != source.as_str()
            && !source.is_empty()
            && source.bytes().all(|b| b == b'#')
            && {
                source.push(' ');
                true
            }
    }

    fn rewrite_phrasing(
        &mut self,
        id: NodeId,
        range: Range<usize>,
        s: &str,
        leading_construct: bool,
    ) -> (DocChange, usize) {
        let display = self.display(id).to_string();
        let start = floor_char_boundary(&display, range.start.min(display.len()));
        let end = floor_char_boundary(&display, range.end.min(display.len())).max(start);
        let kind = self
            .arena
            .get(id)
            .map(|n| n.kind)
            .unwrap_or(BlockKind::Paragraph);
        let mut source = self.leaf_source(id).to_string();
        let mut s2d = self.visual_s2d(id);
        let padded = Self::ensure_heading_delimiter(&mut source, kind, &display);
        if padded {
            s2d.push(s2d.last().copied().unwrap_or(0));
            if let Some(leaf) = self.texts.get_mut(id.text_id()) {
                leaf.set_owned_source(source.clone());
            }
        }
        let (s0, s1) = if start == 0 && end == display.len() && s.is_empty() {
            let keep = if matches!(kind, BlockKind::Heading(_)) {
                bind::display_to_source_inner(&s2d, 0)
            } else {
                0
            };
            (keep, source.len())
        } else {
            Self::display_range_to_source(
                display.len(),
                source.len(),
                &s2d,
                start,
                end,
                leading_construct,
            )
        };
        let (mut change, caret) = self.apply_source_edit(id, s0, s1, s, true);
        if padded
            && let DocChange::TextChanged {
                range, inserted, ..
            } = &mut change
            && range.start == range.end
        {
            range.start -= 1;
            range.end -= 1;
            inserted.insert(0, ' ');
        }
        (change, caret)
    }

    fn rewrite_literal(&mut self, id: NodeId, range: Range<usize>, s: &str) -> (DocChange, usize) {
        let old_revision = self.arena.get(id).map(|n| n.content_revision).unwrap_or(1);
        let display = self.display(id).to_string();
        let start = floor_char_boundary(&display, range.start.min(display.len()));
        let end = floor_char_boundary(&display, range.end.min(display.len())).max(start);
        let deleted = display.get(start..end).unwrap_or("").to_string();
        let inserted = s.to_string();
        if let Some(leaf) = self.texts.get_mut(id.text_id()) {
            leaf.replace_display(start..end, s);
            leaf.source = text::LeafSource::SameAsDisplay;
        }
        let new_revision = self.bump_content(id);
        (
            DocChange::text(
                id,
                old_revision,
                new_revision,
                start as u32..end as u32,
                deleted,
                inserted,
            ),
            start + s.len(),
        )
    }

    pub(super) fn rewrite_block_source(
        &mut self,
        id: NodeId,
        range: Range<usize>,
        s: &str,
    ) -> (Vec<DocChange>, usize) {
        let old_revision = self.arena.get(id).map(|n| n.content_revision).unwrap_or(1);
        let old_extra = self
            .arena
            .get(id)
            .map(|n| n.extra)
            .unwrap_or(NodeExtra::None);
        let kind = self
            .arena
            .get(id)
            .map(|n| n.kind)
            .unwrap_or(BlockKind::Image);
        let mut source = self.leaf_source(id).to_string();
        let start = floor_char_boundary(&source, range.start.min(source.len()));
        let end = floor_char_boundary(&source, range.end.min(source.len())).max(start);
        let deleted = source.get(start..end).unwrap_or("").to_string();
        let inserted = s.to_string();
        source.replace_range(start..end, s);

        let defs = std::sync::Arc::clone(&self.reference_definitions);
        let frag = load_markdown(&bind::with_definitions(&source, &defs), editor_options());
        let (display, runs, extra, constructs) = match bind::matching_leaf(&frag, BlockKind::Image)
        {
            Some(leaf) => {
                let display = frag.display(leaf).to_string();
                let runs = self.remap_runs(&frag, leaf);
                let extra = match frag.extra(leaf).image_dest() {
                    Some(i) => NodeExtra::Image {
                        dest: self.remap_link(&frag, i),
                        source: None,
                    },
                    None => NodeExtra::None,
                };
                let constructs = frag.recorded_constructs(leaf).map(|c| c.to_vec());
                (display, runs, extra, constructs)
            }
            None => (String::new(), Vec::new(), NodeExtra::None, Some(Vec::new())),
        };
        if let Some(n) = self.arena.get_mut(id) {
            n.extra = extra;
        }
        let s2d = bind::identity_map(source.len());
        if let Some(leaf) = self.texts.get_mut(id.text_id()) {
            leaf.set_projected(display, source, runs, constructs);
            leaf.s2d = s2d;
        }
        let new_revision = self.bump_content(id);
        let mut changes = vec![DocChange::text(
            id,
            old_revision,
            new_revision,
            start as u32..end as u32,
            deleted,
            inserted,
        )];
        if extra != old_extra {
            changes.push(DocChange::attrs(id, kind, kind, old_extra, extra));
        }
        (changes, start + s.len())
    }

    fn delete_phrasing_back(&mut self, id: NodeId, display_off: usize) -> (DocChange, usize) {
        let display = self.display(id).to_string();
        let source = self.leaf_source(id).to_string();
        let s2d = self.visual_s2d(id);
        let s_caret = bind::display_to_source_outer(&s2d, display_off);
        let s_caret = floor_char_boundary(&source, s_caret.min(source.len()));
        if self.arena.get(id).map(|node| node.kind) == Some(BlockKind::TableCell)
            && let Some(range) = bind::html_line_break_before(&source, s_caret)
        {
            return self.apply_source_edit(id, range.start, range.end, "", true);
        }
        if s2d.last().copied() == Some(display.len()) && display_off > 0 {
            let prev_d = prev_grapheme_boundary(&display, display_off);
            if prev_d < display_off {
                let s0 = bind::display_to_source_inner(&s2d, prev_d);
                let s1 = bind::display_to_source_outer(&s2d, display_off);
                let s0 = floor_char_boundary(&source, s0.min(source.len()));
                let s1 = floor_char_boundary(&source, s1.min(source.len())).max(s0);
                let s_prev = prev_grapheme_boundary(&source, s_caret);
                if s0 < s1 && s0 < s_prev {
                    return self.apply_source_edit(id, s0, s1, "", true);
                }
            }
        }
        if s_caret == 0 {
            let prev = prev_grapheme_boundary(&display, display_off);
            return self.rewrite_phrasing(id, prev..display_off, "", false);
        }
        let s_prev = prev_grapheme_boundary(&source, s_caret);
        self.apply_source_edit(id, s_prev, s_caret, "", true)
    }

    pub(crate) fn rewrite_text(
        &mut self,
        id: NodeId,
        range: Range<usize>,
        s: &str,
    ) -> (Vec<DocChange>, usize) {
        self.ensure_leaf_text(id);
        let kind = self
            .arena
            .get(id)
            .map(|n| n.kind)
            .unwrap_or(BlockKind::Paragraph);
        match kind.text_edit_strategy() {
            TextEditStrategy::Phrasing => {
                let (change, caret) = self.rewrite_phrasing(id, range, s, false);
                (vec![change], caret)
            }
            TextEditStrategy::BlockSource => self.rewrite_block_source(id, range, s),
            TextEditStrategy::Literal => {
                let (change, caret) = self.rewrite_literal(id, range, s);
                (vec![change], caret)
            }
        }
    }

    pub(crate) fn rewrite_text_spanning_constructs(
        &mut self,
        id: NodeId,
        range: Range<usize>,
        s: &str,
    ) -> (Vec<DocChange>, usize) {
        self.ensure_leaf_text(id);
        let kind = self
            .arena
            .get(id)
            .map(|n| n.kind)
            .unwrap_or(BlockKind::Paragraph);
        match kind.text_edit_strategy() {
            TextEditStrategy::Phrasing => {
                let (change, caret) = self.rewrite_phrasing(id, range, s, true);
                (vec![change], caret)
            }
            TextEditStrategy::BlockSource => self.rewrite_block_source(id, range, s),
            TextEditStrategy::Literal => {
                let (change, caret) = self.rewrite_literal(id, range, s);
                (vec![change], caret)
            }
        }
    }

    pub(crate) fn append_leaf_source(
        &mut self,
        target: NodeId,
        tail: NodeId,
    ) -> (Vec<DocChange>, usize) {
        if self
            .focus
            .as_ref()
            .is_some_and(|focus| focus.node == target || focus.node == tail)
        {
            self.focus = None;
        }
        let join_at = self.collapsed_display(target).len();
        let inserted = self.leaf_source(tail).to_string();
        let strategy = self
            .arena
            .get(target)
            .map(|node| node.kind.text_edit_strategy())
            .unwrap_or(TextEditStrategy::Literal);
        let change = match strategy {
            TextEditStrategy::Phrasing => {
                let tail_kind = self
                    .arena
                    .get(tail)
                    .map(|n| n.kind)
                    .unwrap_or(BlockKind::Paragraph);
                let tail_full = self.leaf_source(tail).to_string();
                let tail_src = if matches!(tail_kind, BlockKind::Heading(_)) {
                    bind::setext_body(&tail_full)
                        .map(str::to_string)
                        .unwrap_or_else(|| bind::heading_body(&tail_full).to_string())
                } else {
                    tail_full
                };
                let target_kind = self
                    .arena
                    .get(target)
                    .map(|n| n.kind)
                    .unwrap_or(BlockKind::Paragraph);
                let mut source = self.leaf_source(target).to_string();
                let setext_sep = if matches!(target_kind, BlockKind::Heading(_)) {
                    match bind::setext_body(&source) {
                        Some(body) => {
                            let sep = source[body.len()..].to_string();
                            source.truncate(body.len());
                            Some(sep)
                        }
                        None => None,
                    }
                } else {
                    None
                };
                let padded = if setext_sep.is_some() {
                    false
                } else {
                    Self::ensure_heading_delimiter(
                        &mut source,
                        target_kind,
                        self.collapsed_display(target),
                    )
                };
                let mut at = source.len();
                source.push_str(&tail_src);
                if let Some(sep) = setext_sep {
                    source.push_str(&sep);
                }
                let inserted = if padded {
                    at -= 1;
                    format!(" {tail_src}")
                } else {
                    tail_src
                };
                vec![
                    self.reproject(
                        target,
                        source,
                        at + inserted.len(),
                        (at as u32..at as u32, String::new(), inserted),
                        false,
                    )
                    .0,
                ]
            }
            TextEditStrategy::BlockSource => {
                let at = self.leaf_source(target).len();
                let (changes, _) = self.rewrite_block_source(target, at..at, &inserted);
                changes
            }
            TextEditStrategy::Literal => {
                vec![self.rewrite_literal(target, join_at..join_at, &inserted).0]
            }
        };
        (change, join_at)
    }

    pub(crate) fn break_literal_leaf(&mut self, id: NodeId, offset: usize) -> Caret {
        let text = self.caret_text(id).to_string();
        let off = floor_char_boundary(&text, offset.min(text.len()));
        if matches!(self.arena.get(id).map(|n| n.kind), Some(BlockKind::Math))
            && (off == 0 || off == text.len())
        {
            let parent = self.arena.get(id).and_then(|n| n.parent).expect("parent");
            let before = self.revision;
            let para = self.alloc_leaf(BlockKind::Paragraph);
            let anchor = if off == 0 {
                self.arena.get(id).and_then(|n| n.prev_sibling)
            } else {
                Some(id)
            };
            self.arena.insert_after(parent, anchor, para);
            self.bump_structure(parent);
            let _ = self.commit(
                before,
                vec![DocChange::TreeSpliced {
                    parent,
                    before: anchor,
                    removed: Vec::new(),
                    inserted: vec![para],
                }],
            );
            return Caret {
                block: para.index,
                offset: 0,
            };
        }
        let can_exit_fence = self
            .arena
            .get(id)
            .is_some_and(|node| matches!(node.kind, BlockKind::CodeBlock | BlockKind::Mermaid));
        if can_exit_fence {
            let open = self
                .extra(id)
                .code_fence_style()
                .map(|(marker, len)| (marker_fence_char(marker), len));
            let matching = if let Some(open) = open {
                syntax::close_fence_delete_range_matching(&text, off, Some(open))
            } else if matches!(self.extra(id), NodeExtra::IndentedCode) {
                None
            } else {
                syntax::close_fence_delete_range(&text, off)
            };
            if let Some(range) = matching {
                return self.exit_fence(id, range);
            }
        }
        let before = self.revision;
        let (changes, caret) = self.rewrite_text(id, off..off, "\n");
        let _ = self.commit(before, changes);
        Caret {
            block: id.index,
            offset: caret,
        }
    }

    fn exit_fence(&mut self, id: NodeId, range: Range<usize>) -> Caret {
        let parent = self.arena.get(id).and_then(|n| n.parent).expect("parent");
        let before = self.revision;
        let (mut changes, _) = self.rewrite_text(id, range, "");
        let para = self.alloc_leaf(BlockKind::Paragraph);
        self.arena.insert_after(parent, Some(id), para);
        self.bump_structure(parent);
        changes.push(DocChange::TreeSpliced {
            parent,
            before: Some(id),
            removed: Vec::new(),
            inserted: vec![para],
        });
        let _ = self.commit(before, changes);
        Caret {
            block: para.index,
            offset: 0,
        }
    }

    pub fn replace_text(&mut self, index: BlockId, range: Range<usize>, s: &str) -> ChangeSet {
        self.replace_text_with_caret(index, range, s).0
    }

    pub(crate) fn replace_text_with_caret(
        &mut self,
        index: BlockId,
        range: Range<usize>,
        s: &str,
    ) -> (ChangeSet, usize) {
        let Some(id) = self.live_id(index) else {
            return (ChangeSet::empty(self.revision), range.start);
        };
        if self.math_edit_would_close(id, range.clone(), s) {
            return (ChangeSet::empty(self.revision), range.start);
        }
        let before = self.revision;
        let (changes, caret) = self.rewrite_text(id, range, s);
        (self.commit(before, changes), caret)
    }

    pub fn replace_text_at_source(
        &mut self,
        index: BlockId,
        range: Range<usize>,
        s: &str,
    ) -> ChangeSet {
        let Some(id) = self.live_id(index) else {
            return ChangeSet::empty(self.revision);
        };
        self.ensure_leaf_text(id);
        let before = self.revision;
        let kind = self
            .arena
            .get(id)
            .map(|n| n.kind)
            .unwrap_or(BlockKind::Paragraph);
        let (changes, _) = match kind.text_edit_strategy() {
            TextEditStrategy::Phrasing => {
                let (change, caret) = self.apply_source_edit(id, range.start, range.end, s, false);
                (vec![change], caret)
            }
            TextEditStrategy::BlockSource => self.rewrite_block_source(id, range, s),
            TextEditStrategy::Literal => {
                let (change, caret) = self.rewrite_literal(id, range, s);
                (vec![change], caret)
            }
        };
        self.commit(before, changes)
    }

    pub(super) fn math_edit_would_close(
        &self,
        id: NodeId,
        _range: Range<usize>,
        inserted: &str,
    ) -> bool {
        if self.arena.get(id).map(|node| node.kind) != Some(BlockKind::Math) {
            return false;
        }
        inserted.contains('$')
    }

    pub(super) fn last_text_caret(&self, id: NodeId) -> Option<(BlockId, usize)> {
        let mut last = None;
        let mut stack = vec![id];
        while let Some(id) = stack.pop() {
            if matches!(
                self.arena.get(id).map(|n| n.kind),
                Some(k) if k.is_text_leaf()
            ) {
                last = Some((id.index, self.caret_text(id).len()));
            }
            let mut child = self.arena.get(id).and_then(|node| node.last_child);
            while let Some(id) = child {
                stack.push(id);
                child = self.arena.get(id).and_then(|node| node.prev_sibling);
            }
        }
        last
    }

    pub(crate) fn delete_back(&mut self, index: BlockId, offset: usize) -> (ChangeSet, usize) {
        let Some(id) = self.live_id(index) else {
            return (ChangeSet::empty(self.revision), offset);
        };
        let kind = self.arena.get(id).map(|n| n.kind);
        let text = self.caret_text(id);
        let off = floor_char_boundary(text, offset.min(text.len()));
        if off == 0 {
            return (ChangeSet::empty(self.revision), off);
        }
        let before = self.revision;
        let (changes, caret) =
            if kind.is_some_and(|kind| kind.text_edit_strategy() == TextEditStrategy::Phrasing) {
                let (change, caret) = self.delete_phrasing_back(id, off);
                (vec![change], caret)
            } else {
                let prev = prev_grapheme_boundary(text, off);
                self.rewrite_text(id, prev..off, "")
            };
        (self.commit(before, changes), caret)
    }

    pub(super) fn apply_recorded_text(&mut self, id: NodeId, range: Range<u32>, inserted: &str) {
        if self.arena.get(id).is_none() && range.start == range.end && inserted.is_empty() {
            return;
        }
        let kind = self
            .arena
            .get(id)
            .map(|n| n.kind)
            .unwrap_or(BlockKind::Paragraph);
        let start = range.start as usize;
        let end = range.end as usize;
        match kind.text_edit_strategy() {
            TextEditStrategy::Phrasing => {
                let _ = self.apply_source_edit(id, start, end, inserted, true);
            }
            TextEditStrategy::BlockSource => {
                let _ = self.rewrite_block_source(id, start..end, inserted);
            }
            TextEditStrategy::Literal => {
                let _ = self.rewrite_literal(id, start..end, inserted);
            }
        }
    }

    pub(super) fn reproject_current(&mut self, id: NodeId) -> DocChange {
        let source = self.leaf_source(id).to_string();
        let n = source.len() as u32;
        let caret = source.len();
        self.reproject(
            id,
            source.clone(),
            caret,
            (0..n, source.clone(), source),
            false,
        )
        .0
    }

    pub(super) fn retable_header_cell(
        &mut self,
        id: NodeId,
        header: bool,
        changes: &mut Vec<DocChange>,
    ) {
        changes.push(self.reproject_current(id));
        if !header {
            return;
        }
        if let Some(leaf) = self.texts.get_mut(id.text_id()) {
            let snapshot = leaf.snapshot_mut();
            for run in &mut snapshot.runs {
                run.marks = run.marks.union(crate::inline::InlineMarks::STRONG);
            }
        }
        let _ = self.bump_content(id);
    }

    pub(crate) fn split_leaf_nodes(&mut self, id: NodeId, offset: usize) -> (DocChange, NodeId) {
        let node = self.arena.get(id).expect("live node");
        let kind = node.kind;
        let new_kind = match kind {
            BlockKind::Heading(_) => BlockKind::Paragraph,
            k => k,
        };
        let parent = node.parent.expect("leaf parent");
        let display = self.display(id).to_string();
        let source = self.leaf_source(id).to_string();
        let off = floor_char_boundary(&display, offset.min(display.len()));
        let s2d = self.visual_s2d(id);
        let src_off = if s2d.last().copied().unwrap_or(0) == display.len() {
            bind::display_to_source_inner(&s2d, off)
        } else {
            off.min(source.len())
        };
        let mut src_off = floor_char_boundary(&source, src_off.min(source.len()));
        let head = &source[..src_off];
        let trailing = head.bytes().rev().take_while(|&b| b == b'\\').count();
        if trailing % 2 == 1 && source[src_off..].starts_with(|c: char| c.is_ascii_punctuation()) {
            src_off -= 1;
        }
        let setext_body_len = if matches!(kind, BlockKind::Heading(_)) {
            bind::setext_body(&source).map(|body| body.len())
        } else {
            None
        };
        let head_s;
        let tail_s = if let Some(body_len) = setext_body_len {
            let sep = source[body_len..].to_string();
            let cut = src_off.min(body_len);
            src_off = cut;
            head_s = format!("{}{}", &source[..cut], sep);
            source[cut..body_len].to_string()
        } else {
            head_s = source[..src_off].to_string();
            source[src_off..].to_string()
        };
        let tail_d = display[off..].to_string();
        let head_len = head_s.len();
        let src_end = source.len() as u32;
        let head_edit = if setext_body_len.is_some() {
            (
                src_off as u32..src_end,
                source[src_off..].to_string(),
                head_s[src_off.min(head_s.len())..].to_string(),
            )
        } else {
            (src_off as u32..src_end, tail_s.clone(), String::new())
        };
        let text_changed = if kind.text_edit_strategy() == TextEditStrategy::Phrasing {
            self.project_phrasing_at(id, head_s, head_len, head_edit, true)
                .0
        } else {
            let (change, _) = self.rewrite_literal(id, off..display.len(), "");
            change
        };
        let new_id = self.alloc_leaf(new_kind);
        if new_kind.text_edit_strategy() == TextEditStrategy::Phrasing {
            let (fill, caret) = self.project_phrasing_at(
                new_id,
                tail_s.clone(),
                0,
                (0..0, String::new(), tail_s),
                false,
            );
            self.push_change(fill);
            self.apply_inline_focus(new_id, caret, Some(0), super::FocusBias::Neutral, true);
        } else {
            let (fill, _) = self.rewrite_literal(new_id, 0..0, &tail_d);
            self.push_change(fill);
        }
        self.arena.insert_after(parent, Some(id), new_id);
        (text_changed, new_id)
    }

    pub fn split_leaf(&mut self, index: BlockId, offset: usize) -> (ChangeSet, BlockId) {
        let before = self.revision;
        let Some(id) = self.live_id(index) else {
            return (ChangeSet::empty(self.revision), index);
        };
        let Some(node) = self.arena.get(id) else {
            return (ChangeSet::empty(self.revision), index);
        };
        if !node.kind.is_text_leaf() || node.kind == BlockKind::Image {
            return (ChangeSet::empty(self.revision), index);
        }
        let parent = node.parent.expect("leaf parent");
        let (text_changed, new_id) = self.split_leaf_nodes(id, offset);
        self.bump_structure(parent);
        let set = self.commit(
            before,
            vec![
                text_changed,
                DocChange::TreeSpliced {
                    parent,
                    before: Some(id),
                    removed: Vec::new(),
                    inserted: vec![new_id],
                },
            ],
        );
        (set, new_id.index)
    }

    fn merge_phrasing_tail(
        &mut self,
        target: NodeId,
        tail: NodeId,
        old_revision: u64,
    ) -> (DocChange, usize) {
        let join_at = self.collapsed_display(target).len();
        let tail_kind = self
            .arena
            .get(tail)
            .map(|n| n.kind)
            .unwrap_or(BlockKind::Paragraph);
        let tail_full = self.leaf_source(tail).to_string();
        let tail_src = if matches!(tail_kind, BlockKind::Heading(_)) {
            bind::setext_body(&tail_full)
                .map(str::to_string)
                .unwrap_or_else(|| bind::heading_body(&tail_full).to_string())
        } else {
            tail_full
        };
        let mut source = self.leaf_source(target).to_string();
        let target_kind = self
            .arena
            .get(target)
            .map(|n| n.kind)
            .unwrap_or(BlockKind::Paragraph);
        let setext_sep = if matches!(target_kind, BlockKind::Heading(_)) {
            match bind::setext_body(&source) {
                Some(body) => {
                    let sep = source[body.len()..].to_string();
                    source.truncate(body.len());
                    Some(sep)
                }
                None => None,
            }
        } else {
            None
        };
        let padded = if setext_sep.is_some() {
            false
        } else {
            Self::ensure_heading_delimiter(&mut source, target_kind, self.collapsed_display(target))
        };
        let at = source.len();
        source.push_str(&tail_src);
        if let Some(sep) = setext_sep {
            source.push_str(&sep);
        }
        let inserted = if padded {
            format!(" {tail_src}")
        } else {
            tail_src
        };
        let at = if padded { at - 1 } else { at };
        let change = self
            .reproject(
                target,
                source,
                at + inserted.len(),
                (at as u32..at as u32, String::new(), inserted),
                false,
            )
            .0;
        let _ = old_revision;
        (change, join_at)
    }

    pub fn merge_into_prev(&mut self, index: BlockId) -> Option<(ChangeSet, BlockId, usize)> {
        let cur_id = self.live_id(index)?;
        if !self.arena.get(cur_id)?.kind.is_text_leaf() {
            return None;
        }
        let prev_id = self.prev_text_leaf(cur_id)?;
        let prev = prev_id.index;
        if matches!(self.kind(prev), Some(BlockKind::TableCell))
            || matches!(self.kind(index), Some(BlockKind::TableCell))
        {
            return None;
        }
        if matches!(
            self.arena.get(prev_id).map(|n| n.kind),
            Some(BlockKind::Math)
        ) {
            let tail = self.leaf_source(cur_id);
            if tail.contains('$') || tail.contains("\n\n") || tail.starts_with('\n') {
                return None;
            }
        }
        let before = self.revision;
        let parent = self.arena.get(cur_id)?.parent?;
        let before_sibling = self.arena.get(cur_id)?.prev_sibling;
        let focused = self.focus.as_ref().map(|focus| focus.node);
        let focus_change = if focused == Some(prev_id) {
            self.focus = None;
            Some(self.focus_text_change(prev_id))
        } else {
            if focused == Some(cur_id) {
                self.focus = None;
            }
            None
        };
        let old_revision = self.arena.get(prev_id)?.content_revision;
        let tail_src = self.leaf_source(cur_id).to_string();
        let strategy = self
            .arena
            .get(prev_id)
            .map(|node| node.kind.text_edit_strategy())
            .unwrap_or(TextEditStrategy::Literal);
        let (mut changes, caret) = match strategy {
            TextEditStrategy::Phrasing => {
                let (change, join_at) = self.merge_phrasing_tail(prev_id, cur_id, old_revision);
                let changes = vec![change];
                (changes, join_at)
            }
            TextEditStrategy::BlockSource | TextEditStrategy::Literal => {
                let tail = tail_src.clone();
                let mut joined = self.leaf_source(prev_id).to_string();
                let prev_kind = self.arena.get(prev_id)?.kind;
                let padded = Self::ensure_heading_delimiter(
                    &mut joined,
                    prev_kind,
                    self.collapsed_display(prev_id),
                );
                let src_at = joined.len();
                let join_at = self.collapsed_display(prev_id).len();
                joined.push_str(&tail_src);
                if let Some(leaf) = self.texts.get_mut(prev_id.text_id()) {
                    let end = leaf.display().len();
                    leaf.replace_display(end..end, &tail);
                    leaf.set_owned_source(joined);
                }
                let new_revision = self.bump_content(prev_id);
                let (change_at, recorded) = if padded {
                    (src_at - 1, format!(" {tail_src}"))
                } else {
                    (src_at, tail_src)
                };
                let changes = vec![DocChange::text(
                    prev_id,
                    old_revision,
                    new_revision,
                    change_at as u32..change_at as u32,
                    String::new(),
                    recorded,
                )];
                let caret = match strategy {
                    TextEditStrategy::BlockSource => src_at,
                    _ => join_at,
                };
                (changes, caret)
            }
        };
        if let Some(change) = focus_change {
            changes.insert(0, change);
        }
        self.arena.snapshot(cur_id);
        self.arena.detach(cur_id);
        self.arena.tombstone(cur_id);
        self.bump_structure(parent);
        changes.push(DocChange::TreeSpliced {
            parent,
            before: before_sibling,
            removed: vec![cur_id],
            inserted: Vec::new(),
        });
        let set = self.commit(before, changes);
        Some((set, prev, caret))
    }

    pub fn merge_into_next(&mut self, index: BlockId) -> Option<(ChangeSet, BlockId, usize)> {
        let cur_id = self.live_id(index)?;
        let (cur_kind, parent, next_id, old_revision) = {
            let cur_node = self.arena.get(cur_id)?;
            (
                cur_node.kind,
                cur_node.parent?,
                cur_node.next_sibling?,
                cur_node.content_revision,
            )
        };
        let next_kind = self.arena.get(next_id)?.kind;
        if !cur_kind.is_text_leaf() {
            return None;
        }
        if !next_kind.is_text_leaf()
            || matches!(cur_kind, BlockKind::TableCell)
            || matches!(next_kind, BlockKind::TableCell)
            || self.arena.get(next_id).and_then(|node| node.parent) != Some(parent)
        {
            return None;
        }
        if matches!(cur_kind, BlockKind::Math) {
            let tail = self.leaf_source(next_id);
            if tail.contains('$') || tail.contains("\n\n") || tail.starts_with('\n') {
                return None;
            }
        }
        let before = self.revision;
        let focused = self.focus.as_ref().map(|focus| focus.node);
        let focus_change = if focused == Some(cur_id) {
            self.focus = None;
            Some(self.focus_text_change(cur_id))
        } else {
            if focused == Some(next_id) {
                self.focus = None;
            }
            None
        };
        let tail_src = self.leaf_source(next_id).to_string();
        let strategy = cur_kind.text_edit_strategy();
        let (mut changes, caret) = match strategy {
            TextEditStrategy::Phrasing => {
                let (change, join_at) = self.merge_phrasing_tail(cur_id, next_id, old_revision);
                (vec![change], join_at)
            }
            TextEditStrategy::BlockSource | TextEditStrategy::Literal => {
                let tail = match strategy {
                    TextEditStrategy::Phrasing => self.collapsed_display(next_id).to_string(),
                    TextEditStrategy::BlockSource | TextEditStrategy::Literal => tail_src.clone(),
                };
                let mut joined = self.leaf_source(cur_id).to_string();
                let padded = Self::ensure_heading_delimiter(
                    &mut joined,
                    cur_kind,
                    self.collapsed_display(cur_id),
                );
                let src_at = joined.len();
                let join_at = self.collapsed_display(cur_id).len();
                joined.push_str(&tail_src);
                if let Some(leaf) = self.texts.get_mut(cur_id.text_id()) {
                    let end = leaf.display().len();
                    leaf.replace_display(end..end, &tail);
                    leaf.set_owned_source(joined);
                }
                let new_revision = self.bump_content(cur_id);
                let (change_at, recorded) = if padded {
                    (src_at - 1, format!(" {tail_src}"))
                } else {
                    (src_at, tail_src)
                };
                let changes = vec![DocChange::text(
                    cur_id,
                    old_revision,
                    new_revision,
                    change_at as u32..change_at as u32,
                    String::new(),
                    recorded,
                )];
                let caret = match strategy {
                    TextEditStrategy::BlockSource => src_at,
                    _ => join_at,
                };
                (changes, caret)
            }
        };
        if let Some(change) = focus_change {
            changes.insert(0, change);
        }
        self.arena.snapshot(next_id);
        self.arena.detach(next_id);
        self.arena.tombstone(next_id);
        self.bump_structure(parent);
        changes.push(DocChange::TreeSpliced {
            parent,
            before: Some(cur_id),
            removed: vec![next_id],
            inserted: Vec::new(),
        });
        let set = self.commit(before, changes);
        Some((set, cur_id.index, caret))
    }
}

fn escape_cell_source(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => match chars.peek() {
                Some(p) if p.is_ascii_punctuation() => {
                    out.push('\\');
                    out.push(*p);
                    chars.next();
                }
                _ => out.push_str(r"\\"),
            },
            '|' => out.push_str(r"\|"),
            _ => out.push(ch),
        }
    }
    out
}
