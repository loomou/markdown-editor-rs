use super::resolved::ResolvedType;
use gpui::{WrapBoundary, WrappedLine, WrappedLineLayout, px};
use md_core::Px;
use md_core::block::BlockKind;
use md_core::inline::{InlineMarks, InlineRun};
use md_layout::linebreak::{
    Clusters, ItemCtx, Mode, OppCtx, Params, Plan, break_lines, build_items, has_complex_context,
    opportunities,
};
use md_theme::{LineBreakMode, LineBreakTokens};
use smallvec::SmallVec;
use std::ops::{DerefMut, Range};
use std::sync::Arc;

struct Ctx<'a> {
    code: &'a [Range<u32>],
    tabs: &'a [Range<u32>],
    mode: Mode,
    em: f32,
    width: f32,
    params: &'a Params,
}

pub(super) fn apply(
    lines: &mut [WrappedLine],
    avail_width: Px,
    role: &ResolvedType,
    block_kind: BlockKind,
    theme: LineBreakTokens,
    runs: &[InlineRun],
    tab_source: Option<&str>,
) -> bool {
    if lines.is_empty() {
        return true;
    }
    let ctx = Ctx {
        code: &code_ranges(runs, tab_source),
        tabs: &tab_ranges(tab_source),
        mode: mode_for(block_kind, theme.mode),
        em: f32::from(role.font_size),
        width: avail_width as f32,
        params: &theme.params,
    };
    let mut spans = Vec::with_capacity(lines.len());
    let mut start = 0usize;
    for line in lines.iter() {
        let end = start + line.text.len();
        spans.push(start..end);
        start = end + 1;
    }
    let mut planned = Vec::with_capacity(lines.len());
    for (line, span) in lines.iter().zip(&spans) {
        let Some(boundaries) = plan_line(line, span, &ctx) else {
            return false;
        };
        planned.push(boundaries);
    }
    for (line, boundaries) in lines.iter_mut().zip(planned) {
        let layout = WrappedLineLayout {
            unwrapped_layout: Arc::clone(&line.unwrapped_layout),
            wrap_boundaries: boundaries,
            wrap_width: Some(px(ctx.width)),
        };
        *DerefMut::deref_mut(line) = Arc::new(layout);
    }
    true
}

fn plan_line(
    line: &WrappedLine,
    span: &Range<usize>,
    ctx: &Ctx<'_>,
) -> Option<SmallVec<[WrapBoundary; 1]>> {
    let text: &str = line.text.as_ref();
    if text.is_empty() {
        return Some(SmallVec::new());
    }
    if has_complex_context(text) {
        return None;
    }
    let clusters = clusters_of(line)?;
    let code: Vec<Range<u32>> = ctx
        .code
        .iter()
        .filter_map(|range| local(range, span))
        .collect();
    let tabs: Vec<Range<u32>> = ctx
        .tabs
        .iter()
        .filter_map(|range| local(range, span))
        .collect();
    let item_ctx = ItemCtx {
        mode: ctx.mode,
        em: ctx.em,
        width: ctx.width,
        params: ctx.params,
        code_ranges: &code,
        tab_ranges: &tabs,
    };
    let opps = opportunities(text, &OppCtx { glue_before: &[] });
    let items = build_items(text, &clusters, &opps, &item_ctx);
    let plan = break_lines(text, &items, ctx.width, ctx.em, ctx.mode, ctx.params);
    boundaries_of(line, &plan)
}

fn boundaries_of(line: &WrappedLine, plan: &Plan) -> Option<SmallVec<[WrapBoundary; 1]>> {
    if plan.lines.is_empty() {
        return None;
    }
    let mut out = SmallVec::new();
    for planned in &plan.lines[..plan.lines.len() - 1] {
        out.push(boundary_at(line, planned.next)?);
    }
    Some(out)
}

fn boundary_at(line: &WrappedLine, byte: u32) -> Option<WrapBoundary> {
    for (run_ix, run) in line.unwrapped_layout.runs.iter().enumerate() {
        for (glyph_ix, glyph) in run.glyphs.iter().enumerate() {
            if glyph.index as u32 >= byte {
                return Some(WrapBoundary { run_ix, glyph_ix });
            }
        }
    }
    None
}

fn clusters_of(line: &WrappedLine) -> Option<Clusters> {
    let mut pairs: Vec<(u32, f32)> = Vec::new();
    for run in &line.unwrapped_layout.runs {
        for glyph in &run.glyphs {
            pairs.push((glyph.index as u32, f32::from(glyph.position.x)));
        }
    }
    pairs.push((
        line.text.len() as u32,
        f32::from(line.unwrapped_layout.width),
    ));
    Clusters::from_pairs(pairs)
}

fn local(range: &Range<u32>, span: &Range<usize>) -> Option<Range<u32>> {
    let start = (range.start as usize).max(span.start);
    let end = (range.end as usize).min(span.end);
    if start >= end {
        return None;
    }
    Some((start - span.start) as u32..(end - span.start) as u32)
}

fn code_ranges(runs: &[InlineRun], tab_source: Option<&str>) -> Vec<Range<u32>> {
    runs.iter()
        .filter(|run| run.marks.contains(InlineMarks::CODE))
        .map(|run| expanded(run.display_range.clone(), tab_source))
        .collect()
}

fn tab_ranges(tab_source: Option<&str>) -> Vec<Range<u32>> {
    let Some(source) = tab_source else {
        return Vec::new();
    };
    source
        .match_indices('\t')
        .map(|(at, _)| {
            let start = super::tabs::orig_to_exp(source, at) as u32;
            start..start + super::tabs::TAB_COLS as u32
        })
        .collect()
}

fn expanded(range: Range<u32>, tab_source: Option<&str>) -> Range<u32> {
    match tab_source {
        Some(source) => {
            let start = super::tabs::orig_to_exp(source, range.start as usize) as u32;
            let end = super::tabs::orig_to_exp(source, range.end as usize) as u32;
            start..end.max(start)
        }
        None => range,
    }
}

fn mode_for(kind: BlockKind, setting: LineBreakMode) -> Mode {
    match (kind, setting) {
        (BlockKind::Heading(_), _) => Mode::Balanced,
        (BlockKind::TableCell, _) => Mode::Ragged,
        (_, LineBreakMode::Greedy | LineBreakMode::Optimal | LineBreakMode::Justify) => {
            Mode::Ragged
        }
    }
}
