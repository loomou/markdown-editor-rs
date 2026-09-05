use super::NodeId;
use super::bind::{
    display_to_source_first, display_to_source_inner, identity_map, source_to_display,
};
use crate::block::BlockId;
use crate::inline::{InlineMarks, InlineRun, covering_runs};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use std::ops::Range;

use super::{bind, editor_options, floor_char_boundary, load_markdown, sanitized_editor_options};

mod block_edit;
mod collapsed;
mod retarget;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusBias {
    Left,
    Right,
    Neutral,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConstructTag {
    Emphasis,
    Strong,
    Strike,
    Link,
    Image,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InlineConstruct {
    pub source: Range<usize>,
    pub inner: Range<usize>,
    pub display: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RawConstruct {
    pub source: Range<usize>,
    pub inner: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FocusImage {
    pub display: Range<usize>,
    pub link: u32,
}

#[derive(Clone, Debug)]
pub struct RevealedImage<'a> {
    pub block: BlockId,
    pub display: Range<usize>,
    pub link: u32,
    pub dest: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FocusMath {
    pub display: Range<usize>,
    pub latex: String,
    pub display_math: bool,
}

#[derive(Clone, Debug)]
pub struct RevealedMath<'a> {
    pub block: BlockId,
    pub display: Range<usize>,
    pub latex: &'a str,
    pub display_math: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct InlineFocus {
    pub node: NodeId,
    pub display: String,
    pub runs: Vec<InlineRun>,
    pub s2d: Vec<usize>,
    pub span: Range<usize>,
    pub image: Option<FocusImage>,
    pub math: Option<FocusMath>,
}

#[derive(Clone, Debug)]
pub(crate) struct FocusProjection {
    pub display: String,
    pub runs: Vec<InlineRun>,
    pub s2d: Vec<usize>,
    pub caret: usize,
    pub span: Range<usize>,
    pub image: Option<FocusImage>,
    pub math: Option<FocusMath>,
}

struct OpenMark {
    tag: ConstructTag,
    start: usize,
    inner: Option<Range<usize>>,
}

struct PlacedHit {
    hit: InlineConstruct,
    vis_start: usize,
}

#[cfg(test)]
pub(crate) fn inline_constructs(source: &str, s2d: &[usize]) -> Vec<InlineConstruct> {
    finish_constructs(&raw_constructs(source), s2d)
}

pub(crate) fn raw_constructs(source: &str) -> Vec<RawConstruct> {
    let n = source.len();
    let mut recorder = ConstructRecorder::new();
    let mut out = Vec::new();
    for (event, range) in Parser::new_ext(source, sanitized_editor_options()).into_offset_iter() {
        let lo = range.start.min(n);
        let hi = range.end.min(n).max(lo);
        match event {
            Event::Start(tag) => recorder.start(&tag, lo),
            Event::End(end) => {
                if let Some(raw) = recorder.end(end, source, hi) {
                    out.push(raw);
                }
            }
            Event::Text(_) | Event::SoftBreak | Event::HardBreak => recorder.cover(lo, hi),
            Event::Code(t) | Event::InlineMath(t) | Event::DisplayMath(t) => {
                out.push(recorder.shown(source, lo..hi, t.as_ref()));
            }
            _ => {}
        }
    }
    out
}

pub(crate) fn finish_constructs(raws: &[RawConstruct], s2d: &[usize]) -> Vec<InlineConstruct> {
    raws.iter()
        .filter_map(|raw| finish_construct(s2d, raw.source.clone(), raw.inner.clone()))
        .collect()
}

pub(crate) struct ConstructRecorder {
    stack: Vec<OpenMark>,
}

impl ConstructRecorder {
    pub(crate) fn new() -> Self {
        ConstructRecorder { stack: Vec::new() }
    }

    pub(crate) fn start(&mut self, tag: &Tag<'_>, lo: usize) {
        if let Some(tag) = construct_tag(tag) {
            self.stack.push(OpenMark {
                tag,
                start: lo,
                inner: None,
            });
        }
    }

    pub(crate) fn cover(&mut self, lo: usize, hi: usize) {
        cover_inners(&mut self.stack, lo, hi);
    }

    pub(crate) fn end(&mut self, end: TagEnd, source: &str, hi: usize) -> Option<RawConstruct> {
        let tag = construct_tag_end(end)?;
        let i = self.stack.iter().rposition(|o| o.tag == tag)?;
        let open = self.stack.remove(i);
        let span = open.start..hi.max(open.start);
        let inner = tighten_inner(source, span.clone(), open.inner.unwrap_or(open.start..hi));
        Some(RawConstruct {
            source: span,
            inner,
        })
    }

    pub(crate) fn shown(&mut self, source: &str, span: Range<usize>, shown: &str) -> RawConstruct {
        self.cover(span.start, span.end);
        let inner = find_shown(source, span.clone(), shown)
            .unwrap_or_else(|| tighten_inner(source, span.clone(), span.clone()));
        RawConstruct {
            source: span,
            inner,
        }
    }
}

impl Default for ConstructRecorder {
    fn default() -> Self {
        Self::new()
    }
}

fn cover_inners(stack: &mut [OpenMark], lo: usize, hi: usize) {
    for open in stack.iter_mut() {
        open.inner = Some(match &open.inner {
            None => lo..hi,
            Some(r) => r.start.min(lo)..r.end.max(hi),
        });
    }
}

fn construct_tag(tag: &Tag<'_>) -> Option<ConstructTag> {
    match tag {
        Tag::Emphasis => Some(ConstructTag::Emphasis),
        Tag::Strong => Some(ConstructTag::Strong),
        Tag::Strikethrough => Some(ConstructTag::Strike),
        Tag::Link { .. } => Some(ConstructTag::Link),
        Tag::Image { .. } => Some(ConstructTag::Image),
        _ => None,
    }
}

fn construct_tag_end(end: TagEnd) -> Option<ConstructTag> {
    match end {
        TagEnd::Emphasis => Some(ConstructTag::Emphasis),
        TagEnd::Strong => Some(ConstructTag::Strong),
        TagEnd::Strikethrough => Some(ConstructTag::Strike),
        TagEnd::Link => Some(ConstructTag::Link),
        TagEnd::Image => Some(ConstructTag::Image),
        _ => None,
    }
}

fn finish_construct(
    s2d: &[usize],
    source: Range<usize>,
    inner: Range<usize>,
) -> Option<InlineConstruct> {
    if source.end <= source.start {
        return None;
    }
    let d0 = source_to_display(s2d, source.start);
    let d1 = source_to_display(s2d, source.end);
    let display = d0.min(d1)..d0.max(d1);
    if source.end - source.start <= display.end - display.start {
        return None;
    }
    Some(InlineConstruct {
        source,
        inner,
        display,
    })
}

fn tighten_inner(source: &str, span: Range<usize>, inner: Range<usize>) -> Range<usize> {
    if inner.start > span.start && inner.end < span.end && inner.start <= inner.end {
        return inner;
    }
    let Some(slice) = source.get(span.clone()) else {
        return inner;
    };
    if let Some(rel) = slice.find("](") {
        let at = span.start + rel;
        if inner.start > span.start && inner.start <= at {
            return inner.start..at;
        }
        return at..at;
    }
    inner
}

fn find_shown(source: &str, span: Range<usize>, shown: &str) -> Option<Range<usize>> {
    if shown.is_empty() {
        return None;
    }
    let slice = source.get(span.start..span.end)?;
    let rel = slice.find(shown)?;
    let lo = span.start + rel;
    Some(lo..lo + shown.len())
}

fn outermost(
    constructs: &[InlineConstruct],
    pred: impl Fn(&InlineConstruct) -> bool,
) -> Option<&InlineConstruct> {
    constructs
        .iter()
        .filter(|c| pred(c))
        .max_by_key(|c| (c.source.end - c.source.start, usize::MAX - c.source.start))
}

fn collapse_nested(mut hits: Vec<InlineConstruct>) -> Vec<InlineConstruct> {
    hits.sort_by_key(|c| (c.source.start, usize::MAX - (c.source.end - c.source.start)));
    let mut kept: Vec<InlineConstruct> = Vec::new();
    for h in hits {
        if kept
            .iter()
            .any(|k| k.source.start <= h.source.start && h.source.end <= k.source.end)
        {
            continue;
        }
        kept.push(h);
    }
    kept
}

fn pick_hits(
    constructs: &[InlineConstruct],
    caret: usize,
    src: usize,
    bias: FocusBias,
) -> Vec<InlineConstruct> {
    let containing: Vec<_> = constructs
        .iter()
        .filter(|c| c.source.start <= src && src <= c.source.end)
        .cloned()
        .collect();
    if !containing.is_empty() {
        return seam_or_outer(containing, src, bias);
    }
    let starts: Vec<_> = constructs
        .iter()
        .filter(|c| c.display.start == caret)
        .cloned()
        .collect();
    let ends: Vec<_> = constructs
        .iter()
        .filter(|c| c.display.end == caret)
        .cloned()
        .collect();
    match bias {
        FocusBias::Right => collapse_nested(if starts.is_empty() { ends } else { starts }),
        FocusBias::Left => collapse_nested(if ends.is_empty() { starts } else { ends }),
        FocusBias::Neutral => {
            let mut both = starts;
            both.extend(ends);
            collapse_nested(both)
        }
    }
}

fn seam_or_outer(
    containing: Vec<InlineConstruct>,
    src: usize,
    bias: FocusBias,
) -> Vec<InlineConstruct> {
    let collapsed = collapse_nested(containing);
    if collapsed.len() <= 1 {
        return collapsed;
    }
    let nested = collapsed.windows(2).any(|w| {
        w[0].source.start <= w[1].source.start && w[1].source.end <= w[0].source.end
            || w[1].source.start <= w[0].source.start && w[0].source.end <= w[1].source.end
    });
    if nested {
        return outermost(&collapsed, |_| true)
            .into_iter()
            .cloned()
            .collect();
    }
    match bias {
        FocusBias::Right => collapsed
            .iter()
            .filter(|c| c.source.start == src)
            .max_by_key(|c| c.source.end)
            .or_else(|| outermost(&collapsed, |_| true))
            .cloned()
            .into_iter()
            .collect(),
        FocusBias::Left => collapsed
            .iter()
            .filter(|c| c.source.end == src)
            .max_by_key(|c| usize::MAX - c.source.start)
            .or_else(|| outermost(&collapsed, |_| true))
            .cloned()
            .into_iter()
            .collect(),
        FocusBias::Neutral => collapsed,
    }
}

pub(crate) struct FocusQuery {
    caret: usize,
    src_hint: Option<usize>,
    bias: FocusBias,
}

pub(crate) fn project_focus(
    source: &str,
    collapsed_display: &str,
    collapsed_runs: &[InlineRun],
    collapsed_s2d: &[usize],
    recorded: &[RawConstruct],
    query: FocusQuery,
) -> Option<FocusProjection> {
    let FocusQuery {
        caret,
        src_hint,
        bias,
    } = query;
    let constructs = finish_constructs(recorded, collapsed_s2d);
    let src = if let Some(src) = src_hint {
        src
    } else if let Some(hit) = match bias {
        FocusBias::Left => outermost(&constructs, |c| c.display.end == caret)
            .or_else(|| outermost(&constructs, |c| c.display.start == caret)),
        FocusBias::Right => outermost(&constructs, |c| c.display.start == caret)
            .or_else(|| outermost(&constructs, |c| c.display.end == caret)),
        FocusBias::Neutral => outermost(&constructs, |c| c.display.start == caret)
            .or_else(|| outermost(&constructs, |c| c.display.end == caret)),
    }
    .or_else(|| {
        outermost(&constructs, |c| {
            c.display.start < caret && caret < c.display.end
        })
    }) {
        if caret == hit.display.start {
            hit.source.start
        } else if caret == hit.display.end {
            hit.source.end
        } else {
            display_to_source_inner(collapsed_s2d, caret)
        }
    } else {
        display_to_source_inner(collapsed_s2d, caret)
    };
    let hits = pick_hits(&constructs, caret, src, bias);
    if hits.is_empty() {
        return None;
    }
    expand_many(
        source,
        collapsed_display,
        collapsed_runs,
        collapsed_s2d,
        &hits,
        src,
    )
}

fn expand_many(
    source: &str,
    collapsed_display: &str,
    collapsed_runs: &[InlineRun],
    collapsed_s2d: &[usize],
    hits: &[InlineConstruct],
    src: usize,
) -> Option<FocusProjection> {
    let mut hits: Vec<_> = hits.to_vec();
    hits.sort_by_key(|h| h.source.start);
    let mut display = String::new();
    let mut out_runs = Vec::new();
    let mut placed = Vec::new();
    let mut collapsed_at = 0usize;
    for hit in &hits {
        if hit.display.start > collapsed_at {
            push_collapsed_slice(
                &mut display,
                &mut out_runs,
                collapsed_display,
                collapsed_runs,
                collapsed_at..hit.display.start,
            );
        }
        let vis_start = display.len();
        display.push_str(source.get(hit.source.clone())?);
        push_hit_runs(&mut out_runs, collapsed_runs, collapsed_s2d, hit, vis_start);
        placed.push(PlacedHit {
            hit: hit.clone(),
            vis_start,
        });
        collapsed_at = hit.display.end;
    }
    if collapsed_at < collapsed_display.len() {
        push_collapsed_slice(
            &mut display,
            &mut out_runs,
            collapsed_display,
            collapsed_runs,
            collapsed_at..collapsed_display.len(),
        );
    }
    let s2d = focus_s2d_many(collapsed_s2d, source.len(), &placed);
    let src = src.min(source.len());
    let caret = source_to_display(&s2d, src);
    let span = placed.first()?.hit.source.start..placed.last()?.hit.source.end;
    let image = placed.iter().find_map(|p| focus_image(collapsed_runs, p));
    let math = placed
        .iter()
        .find_map(|p| focus_math(source, collapsed_runs, p));
    Some(FocusProjection {
        display: display.clone(),
        runs: covering_runs(display.len() as u32, &out_runs),
        s2d,
        caret,
        span,
        image,
        math,
    })
}

fn focus_image(collapsed_runs: &[InlineRun], placed: &PlacedHit) -> Option<FocusImage> {
    let hit = &placed.hit;
    let link = collapsed_runs
        .iter()
        .find(|r| {
            r.marks.is_image()
                && (r.display_range.start as usize) <= hit.display.start
                && hit.display.start < (r.display_range.end as usize)
        })
        .and_then(|r| r.link)?;
    let len = hit.source.end.saturating_sub(hit.source.start);
    Some(FocusImage {
        display: placed.vis_start..placed.vis_start + len,
        link,
    })
}

fn focus_math(source: &str, collapsed_runs: &[InlineRun], placed: &PlacedHit) -> Option<FocusMath> {
    let hit = &placed.hit;
    let run = collapsed_runs.iter().find(|r| {
        r.marks.is_math()
            && (r.display_range.start as usize) <= hit.display.start
            && hit.display.start < (r.display_range.end as usize)
    })?;
    let latex = source.get(hit.inner.clone())?;
    if latex.trim().is_empty() {
        return None;
    }
    let len = hit.source.end.saturating_sub(hit.source.start);
    Some(FocusMath {
        display: placed.vis_start..placed.vis_start + len,
        latex: latex.to_string(),
        display_math: run.marks.contains(InlineMarks::MATH_DISPLAY),
    })
}

fn push_collapsed_slice(
    display: &mut String,
    out_runs: &mut Vec<InlineRun>,
    collapsed_display: &str,
    collapsed_runs: &[InlineRun],
    range: Range<usize>,
) {
    let Some(slice) = collapsed_display.get(range.clone()) else {
        return;
    };
    if slice.is_empty() {
        return;
    }
    let vis_base = display.len();
    display.push_str(slice);
    for r in collapsed_runs {
        let rs = r.display_range.start as usize;
        let re = r.display_range.end as usize;
        let is = rs.max(range.start);
        let ie = re.min(range.end);
        if is < ie {
            let ns = (vis_base + (is - range.start)) as u32;
            let ne = (vis_base + (ie - range.start)) as u32;
            out_runs.push(InlineRun {
                display_range: ns..ne,
                source_range: if is == rs && ie == re {
                    r.source_range.clone()
                } else {
                    None
                },
                marks: r.marks,
                link: r.link,
            });
        }
    }
}

fn reveal_marks(marks: InlineMarks) -> InlineMarks {
    marks.without(InlineMarks::ATOMIC)
}

fn reveal_link(run: &InlineRun) -> Option<u32> {
    if run.marks.is_image() { None } else { run.link }
}

fn push_hit_runs(
    out_runs: &mut Vec<InlineRun>,
    collapsed_runs: &[InlineRun],
    collapsed_s2d: &[usize],
    hit: &InlineConstruct,
    vis_start: usize,
) {
    let opener_len = hit.inner.start.saturating_sub(hit.source.start);

    let inner_len = hit.inner.end.saturating_sub(hit.inner.start);
    let closer_len = hit.source.end.saturating_sub(hit.inner.end);
    if opener_len > 0 {
        out_runs.push(InlineRun {
            display_range: vis_start as u32..vis_start as u32 + opener_len as u32,
            source_range: Some(hit.source.start as u32..hit.inner.start as u32),
            marks: InlineMarks::SYNTAX,
            link: None,
        });
    }
    let inner_base = vis_start + opener_len;
    for r in collapsed_runs {
        let rs = r.display_range.start as usize;
        let re = r.display_range.end as usize;
        let is = rs.max(hit.display.start);
        let ie = re.min(hit.display.end);
        if is < ie {
            let source_start =
                display_to_source_inner(collapsed_s2d, is).clamp(hit.inner.start, hit.inner.end);
            let source_end =
                display_to_source_first(collapsed_s2d, ie).clamp(source_start, hit.inner.end);
            let ns = (vis_start + source_start.saturating_sub(hit.source.start)) as u32;
            let ne = (vis_start + source_end.saturating_sub(hit.source.start)) as u32;
            if ns >= ne {
                continue;
            }
            out_runs.push(InlineRun {
                display_range: ns..ne,
                source_range: if is == rs && ie == re {
                    r.source_range.clone()
                } else {
                    None
                },
                marks: reveal_marks(r.marks),
                link: reveal_link(r),
            });
        }
    }
    let closer_at = (inner_base + inner_len) as u32;
    if closer_len > 0 {
        out_runs.push(InlineRun {
            display_range: closer_at..closer_at + closer_len as u32,
            source_range: Some(hit.inner.end as u32..hit.source.end as u32),
            marks: InlineMarks::SYNTAX,
            link: None,
        });
    }
}

fn focus_s2d_many(collapsed_s2d: &[usize], source_len: usize, placed: &[PlacedHit]) -> Vec<usize> {
    let mut s2d = if collapsed_s2d.len() == source_len + 1 {
        collapsed_s2d.to_vec()
    } else {
        identity_map(source_len)
    };
    let mut hit_index = 0;
    let mut extra = 0usize;
    for (i, slot) in s2d.iter_mut().enumerate().take(source_len + 1) {
        while let Some(p) = placed.get(hit_index)
            && p.hit.source.end < i
        {
            extra = extra.saturating_add(
                (p.hit.source.end - p.hit.source.start)
                    .saturating_sub(p.hit.display.end - p.hit.display.start),
            );
            hit_index += 1;
        }
        if let Some(p) = placed.get(hit_index)
            && i >= p.hit.source.start
            && i <= p.hit.source.end
        {
            *slot = p.vis_start + (i - p.hit.source.start);
        } else {
            *slot = slot.saturating_add(extra);
        }
    }
    s2d
}
