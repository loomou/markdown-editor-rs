use super::builder::{Builder, Frame, FrameKind};
use super::{covers_end, covers_start};
use crate::block::{BlockKind, NodeExtra};
use crate::document::bind::is_html_line_break;
use crate::document::focus::RawConstruct;
use crate::inline::InlineMarks;
use pulldown_cmark::Event;
use std::ops::Range;

fn clamped(source: &str, range: &Range<usize>) -> (usize, usize) {
    let lo = range.start.min(source.len());
    (lo, range.end.min(source.len()).max(lo))
}

impl Builder {
    pub(super) fn handle(&mut self, source: &str, event: Event<'_>, range: Range<usize>) {
        match event {
            Event::Start(tag) => {
                let cover = covers_start(&tag);
                let (lo, _) = clamped(source, &range);
                self.recorder.start(&tag, lo);
                self.start(source, tag, &range);
                if cover {
                    self.cover_source(source, range);
                }
            }
            Event::End(end) => {
                if covers_end(end) {
                    self.cover_source(source, range.clone());
                }

                let (_, hi) = clamped(source, &range);
                if let Some(raw) = self.recorder.end(end, source, hi) {
                    self.record_construct(raw);
                }
                self.end(source, end, &range);
            }
            Event::Text(t) => {
                let (lo, hi) = clamped(source, &range);
                self.recorder.cover(lo, hi);
                self.push_text(source, &t, range.clone());
                self.cover_source(source, range);
            }
            Event::Code(t) => {
                let (lo, hi) = clamped(source, &range);
                let raw = self.recorder.shown(source, lo..hi, &t);
                let st = self.current_inline();
                self.push_inline(st.marks.union(InlineMarks::CODE), st.link);
                self.push_text(source, &t, range.clone());
                self.pop_inline();
                self.cover_source(source, range);
                self.record_construct(raw);
            }
            Event::InlineMath(t) => {
                let (lo, hi) = clamped(source, &range);
                let raw = self.recorder.shown(source, lo..hi, &t);
                self.image_only = false;
                let st = self.current_inline();
                self.push_inline(st.marks.union(InlineMarks::MATH_INLINE), st.link);
                self.push_span(source, &t, range.clone(), false);
                self.pop_inline();
                self.cover_source(source, range);
                self.record_construct(raw);
            }
            Event::DisplayMath(t) => {
                self.image_only = false;

                let latex = display_math_latex(t.as_ref());

                if matches!(self.top().kind, FrameKind::Leaf(BlockKind::Paragraph)) {
                    self.emit_display_math_block(
                        source,
                        latex,
                        range,
                        display_math_fenced(t.as_ref()),
                    );
                    return;
                }
                let (lo, hi) = clamped(source, &range);

                let raw = self.recorder.shown(source, lo..hi, t.as_ref());
                let st = self.current_inline();
                self.push_inline(st.marks.union(InlineMarks::MATH_DISPLAY), st.link);
                self.push_span(source, latex, range.clone(), false);
                self.pop_inline();
                self.cover_source(source, range);
                self.record_construct(raw);
            }
            Event::Html(t) | Event::InlineHtml(t) => {
                if matches!(self.top().kind, FrameKind::Leaf(BlockKind::TableCell))
                    && is_html_line_break(&t)
                {
                    self.push_intern("\n");
                } else {
                    self.push_html(&t);
                }
                self.cover_source(source, range);
            }
            Event::FootnoteReference(t) => {
                let st = self.current_inline();
                self.push_inline(st.marks.union(InlineMarks::FOOTNOTE), st.link);
                self.push_intern(&format!("[^{t}]"));
                self.pop_inline();
                self.cover_source(source, range);
            }
            Event::SoftBreak => {
                let (lo, hi) = clamped(source, &range);
                self.recorder.cover(lo, hi);
                self.push_intern("\n");
                self.cover_source(source, range);
            }
            Event::HardBreak => {
                let (lo, hi) = clamped(source, &range);
                self.recorder.cover(lo, hi);
                self.push_intern("\n");
                self.cover_source(source, range);
            }
            Event::Rule => {
                self.close_implicit(source);
                self.mark_list_loose_before(source, range.start);
                self.push_leaf(BlockKind::ThematicBreak);
                self.cover_source(source, range);
                self.pop(source);
            }
            Event::TaskListMarker(checked) => {
                self.set_list_item_task(checked);
            }
        }
    }

    fn record_construct(&mut self, raw: RawConstruct) {
        if let Some(frame) = self.stack.last_mut()
            && matches!(frame.kind, FrameKind::Leaf(_))
        {
            frame.constructs.push(raw);
        }
    }

    pub(super) fn cover_source(&mut self, source: &str, range: Range<usize>) {
        if range.start > range.end {
            return;
        }
        let pieces = self.split_off_block_prefixes(source, range);
        let Some(frame) = self.stack.last_mut() else {
            return;
        };
        if !matches!(
            frame.kind,
            FrameKind::Leaf(_) | FrameKind::Html | FrameKind::Raw(_)
        ) {
            return;
        }
        for piece in pieces {
            frame.source_ranges.push(piece);
        }
    }

    fn split_off_block_prefixes(&self, source: &str, range: Range<usize>) -> Vec<Range<usize>> {
        let in_quote = self.stack.iter().any(|frame| {
            self.arena.get(frame.id).map(|node| node.kind) == Some(BlockKind::BlockQuote)
        });
        let phrasing = matches!(
            self.stack.last().map(|frame| &frame.kind),
            Some(FrameKind::Leaf(
                BlockKind::Paragraph | BlockKind::Heading(_)
            ))
        );
        let bytes = source.as_bytes();
        let mut out = Vec::new();
        let mut seg_start = range.start;
        let mut i = range.start;
        while i < range.end {
            if bytes[i] == b'\n'
                && let Some(j) = block_prefix_end(bytes, i + 1, range.end, in_quote, phrasing)
            {
                out.push(seg_start..i + 1);
                seg_start = j;
                i = j;
                continue;
            }
            i += 1;
        }
        out.push(seg_start..range.end);
        out
    }

    pub(super) fn assign_leaf_source(&mut self, source: &str, frame: &Frame, kind: BlockKind) {
        if matches!(
            kind,
            BlockKind::CodeBlock | BlockKind::Mermaid | BlockKind::Math
        ) {
            if let Some(leaf) = self.texts.get_mut(frame.id.text_id()) {
                leaf.source = crate::document::text::LeafSource::SameAsDisplay;
                leaf.constructs = Some(Vec::new());
            }
            return;
        }
        let ranges = normalized_source_ranges(source, &frame.source_ranges);
        let Some(leaf) = self.texts.get_mut(frame.id.text_id()) else {
            return;
        };
        if ranges.is_empty() {
            leaf.constructs = Some(Vec::new());
            return;
        }
        let concat = ConcatMap::new(&ranges);
        for run in &mut leaf.snapshot_mut().runs {
            let Some(range) = run.source_range.as_mut() else {
                continue;
            };
            let Some(start) = concat.offset(range.start as usize) else {
                run.source_range = None;
                continue;
            };
            let Some(end) = concat.offset(range.end as usize) else {
                run.source_range = None;
                continue;
            };
            *range = start as u32..end as u32;
        }

        let mut constructs = Vec::with_capacity(frame.constructs.len());
        for raw in &frame.constructs {
            let (Some(start), Some(end)) = (
                concat.offset(raw.source.start),
                concat.offset(raw.source.end),
            ) else {
                continue;
            };
            let (Some(inner_start), Some(inner_end)) =
                (concat.offset(raw.inner.start), concat.offset(raw.inner.end))
            else {
                continue;
            };
            if end - start != raw.source.end - raw.source.start
                || inner_end - inner_start != raw.inner.end - raw.inner.start
            {
                continue;
            }
            constructs.push(RawConstruct {
                source: start..end,
                inner: inner_start..inner_end,
            });
        }
        if ranges.len() == 1 {
            let range = &ranges[0];
            leaf.source =
                crate::document::text::LeafSource::Span(range.start as u32..range.end as u32);
        } else {
            let mut joined = String::with_capacity(ranges.iter().map(Range::len).sum());
            for range in &ranges {
                if let Some(part) = source.get(range.clone()) {
                    joined.push_str(part);
                }
            }
            leaf.set_owned_source(joined);
        }

        leaf.constructs = Some(constructs);
    }

    pub(super) fn mark_enclosing_list_loose(&mut self) {
        let list = self.stack.iter().rev().find_map(|frame| {
            match self.arena.get(frame.id).map(|node| node.kind) {
                Some(BlockKind::List) => Some(Some(frame.id)),
                Some(BlockKind::ListItem) => None,
                _ => Some(None),
            }
        });
        let list = list.flatten();
        let Some(list) = list else {
            return;
        };
        if let Some(n) = self.arena.get_mut(list)
            && let NodeExtra::List {
                loose,
                source_loose,
                ..
            } = &mut n.extra
        {
            *loose = true;
            *source_loose = true;
        }
    }

    pub(super) fn mark_list_loose_before(&mut self, source: &str, at: usize) {
        if blank_line_before(source, at) {
            self.mark_enclosing_list_loose();
        }
    }

    pub(super) fn set_list_item_task(&mut self, checked: bool) {
        let item = self.stack.iter().rev().find_map(|f| {
            self.arena
                .get(f.id)
                .filter(|n| n.kind == BlockKind::ListItem)
                .map(|_| f.id)
        });
        if let Some(id) = item {
            self.set_extra(id, NodeExtra::TaskItem { checked });
        }
    }

    fn emit_display_math_block(
        &mut self,
        source: &str,
        latex: &str,
        range: Range<usize>,
        fenced: bool,
    ) {
        if let Some(leaf) = self.texts.get_mut(self.top().id.text_id()) {
            leaf.trim_trailing_newline();
        }
        let empty = self
            .texts
            .get(self.top().id.text_id())
            .map(|l| l.display().trim().is_empty())
            .unwrap_or(true);
        if empty {
            self.math_continuation = true;
        }
        self.pop(source);
        self.push_leaf(BlockKind::Math);

        if fenced {
            self.set_extra(self.top().id, NodeExtra::MathFence);
        }
        let st = self.current_inline();
        self.push_inline(st.marks.union(InlineMarks::MATH_DISPLAY), st.link);
        self.push_span(source, latex, range.clone(), false);
        self.pop_inline();
        self.cover_source(source, range);
        self.pop(source);
        self.push_leaf(BlockKind::Paragraph);
        self.image_only = false;
        self.math_continuation = true;
    }
}

fn normalized_source_ranges(source: &str, ranges: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut ranges: Vec<_> = ranges
        .iter()
        .map(|range| {
            let start = range.start.min(source.len());
            let end = range.end.min(source.len()).max(start);
            start..end
        })
        .filter(|range| !range.is_empty())
        .collect();
    ranges.sort_unstable_by_key(|range| (range.start, range.end));
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        if let Some(last) = merged.last_mut()
            && range.start <= last.end
        {
            last.end = last.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    if let Some(last) = merged.last_mut()
        && let Some(part) = source.get(last.clone())
    {
        last.end = last.start + part.trim_end_matches(['\n', '\r']).len();
    }
    merged.retain(|range| !range.is_empty());
    merged
}

fn block_prefix_end(
    bytes: &[u8],
    at: usize,
    end: usize,
    in_quote: bool,
    phrasing: bool,
) -> Option<usize> {
    let mut j = at;
    let mut cut = None;
    if in_quote {
        loop {
            let mut k = j;
            let mut indent = 0;
            while indent < 3 && k < end && bytes.get(k) == Some(&b' ') {
                k += 1;
                indent += 1;
            }
            if k >= end || bytes.get(k) != Some(&b'>') {
                break;
            }
            k += 1;

            if k < end && matches!(bytes.get(k), Some(b' ' | b'\t')) {
                k += 1;
            }
            j = k;
            cut = Some(j);
        }
    }
    if !phrasing {
        return cut;
    }
    let mut k = j;
    while k < end && matches!(bytes.get(k), Some(b' ' | b'\t')) {
        k += 1;
    }
    if k > j {
        return Some(k);
    }
    cut
}

struct ConcatMap<'a> {
    ranges: &'a [Range<usize>],

    sums: Vec<usize>,
}

impl<'a> ConcatMap<'a> {
    fn new(ranges: &'a [Range<usize>]) -> Self {
        let mut sums = Vec::with_capacity(ranges.len() + 1);
        sums.push(0);
        let mut acc = 0usize;
        for range in ranges {
            acc += range.len();
            sums.push(acc);
        }
        Self { ranges, sums }
    }

    fn offset(&self, source_offset: usize) -> Option<usize> {
        let i = self
            .ranges
            .partition_point(|range| range.end < source_offset);
        let range = self.ranges.get(i)?;
        (range.start <= source_offset).then(|| self.sums[i] + source_offset - range.start)
    }
}

fn display_math_latex(latex: &str) -> &str {
    latex
        .strip_prefix("\r\n")
        .or_else(|| latex.strip_prefix('\n'))
        .unwrap_or(latex)
}

fn display_math_fenced(t: &str) -> bool {
    t.starts_with('\n') || t.starts_with("\r\n")
}

fn blank_line_before(source: &str, at: usize) -> bool {
    let mut newlines = 0usize;
    for &b in source.as_bytes()[..at].iter().rev() {
        match b {
            b'\n' => {
                newlines += 1;
                if newlines >= 2 {
                    return true;
                }
            }
            b' ' | b'\t' | b'\r' => {}
            _ => return false,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear_offset(ranges: &[Range<usize>], at: usize) -> Option<usize> {
        let mut acc = 0;
        for range in ranges {
            if range.start <= at && at <= range.end {
                return Some(acc + at - range.start);
            }
            acc += range.len();
        }
        None
    }

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
    }

    #[test]
    fn concat_map_agrees_with_the_linear_scan_at_every_endpoint() {
        let mut cases: Vec<Vec<Range<usize>>> = vec![vec![], vec![0..5], vec![3..5, 5..8]];
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for _ in 0..200 {
            let mut at = (rng.next() % 50) as usize;
            let mut ranges = Vec::new();
            for _ in 0..(rng.next() % 12) {
                let len = 1 + (rng.next() % 6) as usize;
                let gap = (rng.next() % 4) as usize;
                ranges.push(at..at + len);
                at += len + gap;
            }
            cases.push(ranges);
        }
        for ranges in cases {
            let map = ConcatMap::new(&ranges);
            let total = ranges.last().map(|r| r.end).unwrap_or(0);
            for at in 0..=(total + 3) {
                assert_eq!(
                    map.offset(at),
                    linear_offset(&ranges, at),
                    "ranges={ranges:?} at={at}"
                );
            }
        }
    }
}
