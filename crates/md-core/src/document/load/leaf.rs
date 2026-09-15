use super::builder::{Builder, HostIndent};
use super::walk::LeafCtx;
use crate::block::BlockKind;
use crate::document::bind;
use crate::document::focus::RawConstruct;
use crate::inline::InlineMarks;
use std::ops::Range;

impl Builder {
    pub(super) fn cover_source(&mut self, source: &str, range: Range<usize>) {
        self.cover_impl(source, range, false);
    }

    pub(super) fn cover_verbatim(&mut self, source: &str, range: Range<usize>) {
        self.cover_impl(source, range, true);
    }

    fn cover_impl(&mut self, source: &str, range: Range<usize>, verbatim: bool) {
        if range.start > range.end {
            return;
        }
        let mut range = range;
        let floor = self.cover_floor;
        if let Some(floor) = floor
            && floor < range.start
        {
            let bytes = source.as_bytes();
            let prev_is_newline = floor > 0 && bytes[floor - 1] == b'\n';
            let seam_has_newline = bytes[floor..range.start].contains(&b'\n');
            if !prev_is_newline && !seam_has_newline {
                range.start = floor;
            } else if bytes.get(range.start - 1) == Some(&b'\\') {
                range.start -= 1;
            }
        } else if range.start > 0 && source.as_bytes().get(range.start - 1) == Some(&b'\\') {
            range.start -= 1;
        }
        self.cover_floor = Some(range.end.max(floor.unwrap_or(0)));
        if let Some(floor) = self.math_extract_at
            && range.start < floor
        {
            range.start = floor.min(range.end);
        }
        let pieces = self.split_off_block_prefixes(source, range, verbatim);
        let Some(leaf) = self.current_leaf.as_mut() else {
            return;
        };
        for piece in pieces {
            leaf.source_ranges.push(piece);
        }
    }

    fn split_off_block_prefixes(
        &self,
        source: &str,
        range: Range<usize>,
        verbatim: bool,
    ) -> Vec<Range<usize>> {
        let in_quote = self
            .parents
            .iter()
            .any(|id| self.arena.get(*id).map(|n| n.kind) == Some(BlockKind::BlockQuote));
        let phrasing = !verbatim
            && matches!(
                self.current_leaf.as_ref().map(|l| l.kind),
                Some(BlockKind::Paragraph | BlockKind::Heading(_))
            );
        let host = if verbatim {
            self.hosts.last().copied()
        } else {
            None
        };
        let bytes = source.as_bytes();
        let mut out = Vec::new();
        let mut seg_start = range.start;
        let mut i = range.start;
        while i < range.end {
            if bytes[i] == b'\n'
                && let Some(j) = block_prefix_end(bytes, i + 1, range.end, in_quote, phrasing, host)
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

    pub(super) fn leave_leaf_ranges(&mut self, source: &str, leaf: &mut LeafCtx) {
        if matches!(
            leaf.kind,
            BlockKind::CodeBlock | BlockKind::Mermaid | BlockKind::Math
        ) {
            if let Some(l) = self.texts.get_mut(leaf.id.text_id()) {
                l.source = crate::document::text::LeafSource::SameAsDisplay;
                l.constructs = Some(Vec::new());
                l.s2d = bind::identity_map(l.display().len());
            }
            return;
        }
        let ranges = normalized_source_ranges(source, &leaf.source_ranges);
        let Some(l) = self.texts.get_mut(leaf.id.text_id()) else {
            return;
        };
        if ranges.is_empty() {
            l.constructs = Some(Vec::new());
            if leaf.kind.is_text_leaf() {
                l.s2d = bind::identity_map(l.display().len());
            }
            return;
        }
        let concat = ConcatMap::new(&ranges);
        for run in &mut l.snapshot_mut().runs {
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
        let mut constructs = Vec::with_capacity(leaf.constructs.len());
        let mut lost_construct = false;
        for raw in &leaf.constructs {
            let (Some(start), Some(end)) = (
                concat.offset(raw.source.start),
                concat.offset(raw.source.end),
            ) else {
                lost_construct = true;
                continue;
            };
            let (Some(inner_start), Some(inner_end)) =
                (concat.offset(raw.inner.start), concat.offset(raw.inner.end))
            else {
                lost_construct = true;
                continue;
            };
            if end - start != raw.source.end - raw.source.start
                || inner_end - inner_start != raw.inner.end - raw.inner.start
            {
                lost_construct = true;
                continue;
            }
            constructs.push(RawConstruct {
                source: start..end,
                inner: inner_start..inner_end,
            });
        }
        if ranges.len() == 1 {
            let range = &ranges[0];
            l.source =
                crate::document::text::LeafSource::Span(range.start as u32..range.end as u32);
        } else {
            let mut joined = String::with_capacity(ranges.iter().map(Range::len).sum());
            for range in &ranges {
                if let Some(part) = source.get(range.clone()) {
                    joined.push_str(part);
                }
            }
            l.set_owned_source(joined);
        }
        l.constructs = if lost_construct {
            None
        } else {
            Some(constructs)
        };
        if !leaf.kind.is_text_leaf() {
            return;
        }
        let s2d = {
            let display = l.display();
            let src: &str = match &l.source {
                crate::document::text::LeafSource::Span(r) => {
                    source.get(r.start as usize..r.end as usize).unwrap_or("")
                }
                crate::document::text::LeafSource::SameAsDisplay => display,
                crate::document::text::LeafSource::Owned(s) => s,
            };
            if src.is_empty() && !display.is_empty() || src == display {
                bind::identity_map(display.len())
            } else if leaf.is_html() && !display.is_empty() {
                let span = ranges.first().map(|r| r.start).unwrap_or(0)
                    ..ranges.last().map(|r| r.end).unwrap_or(0);
                replay_s2d(
                    src,
                    display,
                    &[LeafLog::Shown {
                        span,
                        len: display.len(),
                    }],
                    &concat,
                )
            } else {
                replay_s2d(src, display, &leaf.log, &concat)
            }
        };
        if let Some(l) = self.texts.get_mut(leaf.id.text_id()) {
            l.s2d = s2d;
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum LeafLog {
    Shown { span: Range<usize>, len: usize },
    Break { span: Range<usize> },
    TableBr { span: Range<usize> },
    ImageStart,
    ImageEnd { span: Range<usize> },
}

fn replay_s2d(src: &str, display: &str, log: &[LeafLog], concat: &ConcatMap<'_>) -> Vec<usize> {
    use crate::document::bind::{IMAGE_PLACEHOLDER, UNMAPPED, assign, fill, fill_gaps, map_shown};

    let fold = |span: &Range<usize>| -> Option<(usize, usize)> {
        let lo = concat.offset(span.start)?;
        let hi = concat.offset(span.end)?;
        (hi >= lo).then_some((lo, hi))
    };
    let mut s2d = vec![UNMAPPED; src.len() + 1];
    let mut disp = 0usize;
    let mut image_disp_at: Option<usize> = None;
    for entry in log {
        match entry {
            LeafLog::Shown { span, len } => {
                let len = (*len).min(display.len() - disp);
                let shown = &display[disp..disp + len];
                match fold(span) {
                    Some((lo, hi)) => map_shown(src, &mut s2d, lo, hi, shown, &mut disp),
                    None => disp += len,
                }
            }
            LeafLog::Break { span } => {
                let folded = fold(span);
                if let Some((lo, hi)) = folded {
                    fill(&mut s2d, lo, hi, disp);
                }
                disp += 1;
                if let Some((_, hi)) = folded {
                    assign(&mut s2d, hi, disp);
                }
            }
            LeafLog::TableBr { span } => {
                let folded = fold(span);
                if let Some((lo, _)) = folded {
                    assign(&mut s2d, lo, disp);
                }
                disp += 1;
                if let Some((lo, hi)) = folded {
                    fill(&mut s2d, lo.saturating_add(1), hi, disp);
                }
            }
            LeafLog::ImageStart => image_disp_at = Some(disp),
            LeafLog::ImageEnd { span } => {
                let folded = fold(span);
                if image_disp_at.take() == Some(disp) {
                    if let Some((lo, hi)) = folded {
                        fill(&mut s2d, lo, hi, disp);
                    }
                    disp = disp.saturating_add(IMAGE_PLACEHOLDER.len());
                    if let Some((_, hi)) = folded {
                        assign(&mut s2d, hi, disp);
                    }
                }
            }
        }
    }
    debug_assert_eq!(
        disp,
        display.len(),
        "s2d replay lost sync with the display length (a log gap or a rebuilt display)"
    );
    fill_gaps(&mut s2d);
    s2d
}

impl Builder {
    pub(super) fn leave_html_leaf(&mut self, source: &str, leaf: &mut LeafCtx) {
        let text = super::strip_html(&leaf.html);
        self.texts.init_leaf(leaf.id.text_id());
        if let Some(n) = self.arena.get_mut(leaf.id) {
            n.kind = BlockKind::Paragraph;
            n.text = Some(leaf.id.text_id());
        }
        if !text.is_empty()
            && let Some(l) = self.texts.get_mut(leaf.id.text_id())
        {
            let intern_range = super::intern_push(&mut self.intern, &text);
            l.append(
                crate::document::text::TextPiece::Intern(intern_range),
                &text,
                None,
                InlineMarks::NONE,
                None,
            );
        }
        crate::document::metrics::note_leaf();
        self.leave_leaf_ranges(source, leaf);
    }

    pub(super) fn leave_raw_leaf(&mut self, source: &str, leaf: &mut LeafCtx) {
        let start = leaf
            .source_ranges
            .iter()
            .map(|range| range.start)
            .min()
            .unwrap_or(0)
            .min(source.len());
        let mut end = leaf
            .source_ranges
            .iter()
            .map(|range| range.end)
            .max()
            .unwrap_or(start)
            .min(source.len())
            .max(start);
        while source.as_bytes().get(end) == Some(&b'\n') {
            end += 1;
        }
        let range = start..end;
        let raw = source.get(range.clone()).unwrap_or("");
        self.texts.init_leaf(leaf.id.text_id());
        if let Some(node) = self.arena.get_mut(leaf.id) {
            node.kind = leaf.kind;
            node.text = Some(leaf.id.text_id());
        }
        if let Some(l) = self.texts.get_mut(leaf.id.text_id()) {
            let source_range = range.start as u32..range.end as u32;
            l.append(
                crate::document::text::TextPiece::Source(source_range.clone()),
                raw,
                Some(source_range.clone()),
                InlineMarks::NONE,
                None,
            );
            l.source = crate::document::text::LeafSource::Span(source_range);
            l.s2d = bind::identity_map(raw.len());
        }
        crate::document::metrics::note_leaf();
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

#[allow(clippy::too_many_arguments)]
fn block_prefix_end(
    bytes: &[u8],
    at: usize,
    end: usize,
    in_quote: bool,
    phrasing: bool,
    host: Option<HostIndent>,
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
    if let Some(host) = host {
        let mut k = j;
        let mut col = 0usize;
        for &b in &bytes[at..k] {
            col = super::expand_column(col, b);
        }
        let target = match host {
            HostIndent::ContentColumn(target) => target as usize,
            HostIndent::Relative(n) => col + n as usize,
        };
        while k < end && col < target && matches!(bytes.get(k), Some(b' ' | b'\t')) {
            let step = if bytes[k] == b'\t' {
                super::next_tab_stop(col) - col
            } else {
                1
            };
            if col + step > target {
                if bytes[k] == b'\t' {
                    k += 1;
                }
                break;
            }
            col += step;
            k += 1;
        }
        if k > j {
            return Some(k);
        }
        return cut;
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
