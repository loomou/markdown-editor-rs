use super::bands::{Atom, BandFlow, Pending};
use super::resolved::ResolvedType;
use super::{GpuiShaper, SCRIPT_SCALE, SUB_DROP, SUPER_RISE, kp_plain};
use gpui::{WrappedLine, WrappedLineLayout, px};
use md_core::Px;
use md_core::block::BlockKind;
use md_core::inline::{InlineMarks, InlineRun, covering_runs};
use md_layout::linebreak::{
    Clusters, ItemCtx, Mode, OppCtx, Params, break_lines, build_items, glyph_shifts,
    has_complex_context, opportunities,
};
use md_theme::LineBreakTokens;
use smallvec::SmallVec;
use std::ops::{DerefMut, Range};
use std::sync::Arc;

struct Ctx<'a> {
    shaper: &'a GpuiShaper,
    text: &'a str,
    runs: &'a [InlineRun],
    role: &'a ResolvedType,
    font_size: f32,
    avail: f32,
    dpr: f32,
    mode: Mode,
    params: &'a Params,
}

#[derive(Clone, Copy)]
struct Span {
    start: usize,
    end: usize,
    font_size: f32,
    dy: f32,
    atomic: bool,
    sticky: bool,
}

enum Piece {
    Span(Span),
    Math {
        start: usize,
        end: usize,
        width: f32,
        display: bool,
    },
    Image {
        start: usize,
        end: usize,
        width: f32,
        dest: String,
        raw: Option<String>,
    },
}

impl Piece {
    fn start(&self) -> usize {
        match self {
            Piece::Span(span) => span.start,
            Piece::Math { start, .. } | Piece::Image { start, .. } => *start,
        }
    }

    fn end(&self) -> usize {
        match self {
            Piece::Span(span) => span.end,
            Piece::Math { end, .. } | Piece::Image { end, .. } => *end,
        }
    }

    fn width(&self) -> f32 {
        match self {
            Piece::Span(..) => 0.0,
            Piece::Math { width, .. } | Piece::Image { width, .. } => *width,
        }
    }
}

struct RawLine {
    start: usize,
    end: usize,
    emit_empty: bool,
    pieces: Vec<Piece>,
}

struct Planned {
    base: usize,
    emit_empty: bool,
    pieces: Vec<Piece>,
    rows: Vec<(usize, usize)>,
    shifts: Vec<(u32, f32)>,
}

struct Buf {
    lines: Vec<RawLine>,
    pieces: Vec<Piece>,
    start: usize,
}

impl Buf {
    fn cut(&mut self, at: usize) {
        self.lines.push(RawLine {
            start: self.start,
            end: at,
            emit_empty: true,
            pieces: std::mem::take(&mut self.pieces),
        });
        self.start = at + 1;
    }

    fn push(&mut self, span: Span) {
        if !span.atomic
            && !span.sticky
            && let Some(Piece::Span(last)) = self.pieces.last_mut()
            && !last.atomic
            && last.end == span.start
        {
            last.end = span.end;
            return;
        }
        self.pieces.push(Piece::Span(span));
    }

    fn flush_line(&mut self, end: usize, emit_empty: bool) {
        self.lines.push(RawLine {
            start: self.start,
            end: end.max(self.start),
            emit_empty,
            pieces: std::mem::take(&mut self.pieces),
        });
    }
}

pub(super) fn place(
    shaper: &GpuiShaper,
    text: &str,
    runs: &[InlineRun],
    atoms: &[Atom],
    block_kind: BlockKind,
    theme: LineBreakTokens,
    flow: &mut BandFlow<'_>,
) -> bool {
    let ctx = Ctx {
        shaper,
        text,
        runs,
        role: flow.role,
        font_size: flow.parent_size,
        avail: flow.avail as f32,
        dpr: flow.dpr,
        mode: kp_plain::mode_for(block_kind, theme.mode),
        params: &theme.params,
    };
    let Some(lines) = plan(&ctx, atoms) else {
        return false;
    };
    for line in lines {
        place_line(&ctx, flow, line);
    }
    true
}

fn plan(ctx: &Ctx<'_>, atoms: &[Atom]) -> Option<Vec<Planned>> {
    let mut out = Vec::new();
    for line in raw_lines(ctx, atoms)? {
        out.push(planned_line(ctx, line)?);
    }
    Some(out)
}

fn raw_lines(ctx: &Ctx<'_>, atoms: &[Atom]) -> Option<Vec<RawLine>> {
    let covered = covering_runs(ctx.text.len() as u32, ctx.runs);
    let mut buf = Buf {
        lines: Vec::new(),
        pieces: Vec::new(),
        start: 0,
    };
    let mut last_break = false;
    for atom in atoms {
        match atom {
            Atom::Break { offset } => {
                buf.cut(*offset);
                last_break = true;
            }
            Atom::Text { start, end } => {
                last_break = false;
                push_spans(
                    ctx,
                    &covered,
                    &mut buf,
                    *start..*end,
                    ctx.font_size,
                    0.0,
                    false,
                )?;
            }
            Atom::Script {
                start,
                end,
                super_script,
            } => {
                last_break = false;
                let size = (ctx.font_size * SCRIPT_SCALE).max(1.0);
                let dy = if *super_script {
                    -ctx.font_size * SUPER_RISE
                } else {
                    ctx.font_size * SUB_DROP
                };
                push_spans(ctx, &covered, &mut buf, *start..*end, size, dy, true)?;
            }
            Atom::Math {
                start,
                end,
                display,
            } => {
                last_break = false;
                let latex = ctx.text.get(*start..*end).unwrap_or("");
                let measured = ctx.shaper.measure_math(
                    latex,
                    *display,
                    ctx.role,
                    ctx.avail as Px,
                    ctx.font_size,
                    ctx.dpr,
                );
                buf.pieces.push(Piece::Math {
                    start: *start,
                    end: *end,
                    width: measured.width,
                    display: *display,
                });
            }
            Atom::Image {
                start,
                end,
                dest,
                raw,
            } => {
                last_break = false;
                let measured =
                    ctx.shaper
                        .measure_image(dest.clone(), raw.clone(), ctx.role, ctx.avail as Px);
                buf.pieces.push(Piece::Image {
                    start: *start,
                    end: *end,
                    width: measured.slot_w,
                    dest: dest.clone(),
                    raw: raw.clone(),
                });
            }
        }
    }
    if last_break || !buf.pieces.is_empty() {
        buf.flush_line(ctx.text.len(), last_break);
    }
    Some(buf.lines)
}

fn push_spans(
    ctx: &Ctx<'_>,
    covered: &[InlineRun],
    buf: &mut Buf,
    range: Range<usize>,
    font_size: f32,
    dy: f32,
    atomic: bool,
) -> Option<()> {
    let text = ctx.text.get(range.clone())?;
    let mut cursor = range.start;
    for (offset, ch) in text.char_indices() {
        if ch != '\n' {
            continue;
        }
        let at = range.start + offset;
        if at > cursor {
            buf.push(Span {
                start: cursor,
                end: at,
                font_size,
                dy,
                atomic,
                sticky: starts_footnote(covered, cursor),
            });
        }
        buf.cut(at);
        cursor = at + 1;
    }
    if range.end > cursor {
        buf.push(Span {
            start: cursor,
            end: range.end,
            font_size,
            dy,
            atomic,
            sticky: starts_footnote(covered, cursor),
        });
    }
    Some(())
}

fn starts_footnote(covered: &[InlineRun], at: usize) -> bool {
    covered.iter().any(|run| {
        run.display_range.start as usize == at && run.marks.contains(InlineMarks::FOOTNOTE)
    })
}

fn planned_line(ctx: &Ctx<'_>, line: RawLine) -> Option<Planned> {
    let base = line.start;
    if line.pieces.is_empty() {
        return Some(Planned {
            base,
            emit_empty: line.emit_empty,
            pieces: Vec::new(),
            rows: Vec::new(),
            shifts: Vec::new(),
        });
    }
    let line_text = ctx.text.get(base..line.end)?;
    let analysis = analysis_text(line_text, base, &line.pieces)?;
    if has_complex_context(&analysis) {
        return None;
    }
    let clusters = clusters_of(ctx, base, &line.pieces, line_text.len())?;
    let glue = glued_positions(ctx, base, &line.pieces);
    let opps = opportunities(&analysis, &OppCtx { glue_before: &glue });
    let items = build_items(
        &analysis,
        &clusters,
        &opps,
        &ItemCtx {
            mode: ctx.mode,
            em: ctx.font_size,
            width: ctx.avail,
            params: ctx.params,
            code_ranges: &[],
            tab_ranges: &[],
        },
    );
    let plan = break_lines(
        &analysis,
        &items,
        ctx.avail,
        ctx.font_size,
        ctx.mode,
        ctx.params,
    );
    let shifts = if ctx.mode == Mode::Justify {
        glyph_shifts(&plan, &items)
    } else {
        Vec::new()
    };
    let rows = plan
        .lines
        .iter()
        .map(|row| (row.start as usize, row.next as usize))
        .collect();
    Some(Planned {
        base,
        emit_empty: line.emit_empty,
        pieces: line.pieces,
        rows,
        shifts,
    })
}

fn analysis_text(line_text: &str, base: usize, pieces: &[Piece]) -> Option<String> {
    let mut out = String::with_capacity(line_text.len());
    let mut at = 0usize;
    for piece in pieces {
        let start = piece.start().saturating_sub(base).max(at);
        let end = piece.end().saturating_sub(base);
        if end <= start {
            continue;
        }
        out.push_str(line_text.get(at..start)?);
        match piece {
            Piece::Span(..) => out.push_str(line_text.get(start..end)?),
            Piece::Math { .. } | Piece::Image { .. } => out.push_str(&"a".repeat(end - start)),
        }
        at = end;
    }
    out.push_str(line_text.get(at..)?);
    Some(out)
}

fn clusters_of(ctx: &Ctx<'_>, base: usize, pieces: &[Piece], len: usize) -> Option<Clusters> {
    let mut pairs: Vec<(u32, f32)> = Vec::new();
    let mut x = 0.0f32;
    for piece in pieces {
        let start = piece.start().saturating_sub(base);
        let end = piece.end().saturating_sub(base);
        match piece {
            Piece::Span(span) => {
                if ctx.text.get(span.start..span.end)?.contains('\t') {
                    return None;
                }
                let shaped = ctx.shaper.shape_slice(
                    ctx.text,
                    ctx.runs,
                    span.start..span.end,
                    ctx.role,
                    px(span.font_size),
                    None,
                );
                push_pair(&mut pairs, start, x);
                for run in &shaped.unwrapped_layout.runs {
                    for glyph in &run.glyphs {
                        push_pair(
                            &mut pairs,
                            start + glyph.index,
                            x + f32::from(glyph.position.x),
                        );
                    }
                }
                x += f32::from(shaped.width());
            }
            Piece::Math { .. } | Piece::Image { .. } => {
                push_pair(&mut pairs, start, x);
                x += piece.width();
                push_pair(&mut pairs, end, x);
            }
        }
    }
    push_pair(&mut pairs, len, x);
    Clusters::from_pairs(pairs)
}

fn push_pair(pairs: &mut Vec<(u32, f32)>, byte: usize, x: f32) {
    if pairs.last().is_some_and(|&(last, _)| last as usize >= byte) {
        return;
    }
    pairs.push((byte as u32, x));
}

fn glued_positions(ctx: &Ctx<'_>, base: usize, pieces: &[Piece]) -> Vec<u32> {
    let mut out = Vec::new();
    for piece in pieces {
        let Piece::Span(span) = piece else {
            continue;
        };
        let Some(slice) = ctx.text.get(span.start..span.end) else {
            continue;
        };
        let joined = ctx
            .text
            .get(..span.start)
            .and_then(|head| head.chars().next_back())
            .is_some_and(|c| !c.is_whitespace());
        if (span.atomic || span.sticky) && joined {
            out.push(span.start.saturating_sub(base) as u32);
        }
        if span.atomic {
            for (offset, _) in slice.char_indices().skip(1) {
                out.push(span.start.saturating_add(offset).saturating_sub(base) as u32);
            }
        }
    }
    out
}

fn place_line(ctx: &Ctx<'_>, flow: &mut BandFlow<'_>, planned: Planned) {
    let base = planned.base;
    if planned.pieces.is_empty() {
        if planned.emit_empty {
            let line = ctx.shaper.shape_slice(
                ctx.text,
                ctx.runs,
                base..base,
                ctx.role,
                ctx.role.font_size,
                None,
            );
            flow.pending.push(Pending::Text {
                line: Box::new(line),
                x: 0.0,
                start: base,
                end: base,
                dy: 0.0,
            });
            flow.flush();
        }
        return;
    }
    let mut ix = 0usize;
    for (from, to) in planned.rows.iter().copied() {
        let mut x = 0.0f32;
        while ix < planned.pieces.len() {
            let piece = &planned.pieces[ix];
            let head = piece.start().saturating_sub(base);
            let tail = piece.end().saturating_sub(base);
            if tail <= from {
                ix += 1;
                continue;
            }
            if head >= to {
                break;
            }
            let start = head.max(from);
            let end = tail.min(to);
            match piece {
                Piece::Span(span) => {
                    let shaped = ctx.shaper.shape_slice(
                        ctx.text,
                        ctx.runs,
                        base + start..base + end,
                        ctx.role,
                        px(span.font_size),
                        None,
                    );
                    let natural = f32::from(shaped.width());
                    let shaped = justified(shaped, &planned.shifts, start, end);
                    flow.pending.push(Pending::Text {
                        line: Box::new(shaped),
                        x,
                        start: base + start,
                        end: base + end,
                        dy: span.dy,
                    });
                    x += natural + kp_plain::shift_at(&planned.shifts, end)
                        - kp_plain::shift_at(&planned.shifts, start);
                }
                Piece::Math {
                    start: at,
                    end: up_to,
                    display,
                    ..
                } => {
                    let latex = ctx.text.get(*at..*up_to).unwrap_or("");
                    let measured = ctx.shaper.measure_math(
                        latex,
                        *display,
                        ctx.role,
                        ctx.avail as Px,
                        ctx.font_size,
                        ctx.dpr,
                    );
                    flow.pending.push(Pending::Math {
                        x,
                        metrics: measured.metrics,
                        latex: measured.latex,
                        display: measured.display,
                        start: base + start,
                        end: base + end,
                        fallback: measured.fallback,
                    });
                    x += measured.width;
                }
                Piece::Image { dest, raw, .. } => {
                    let measured = ctx.shaper.measure_image(
                        dest.clone(),
                        raw.clone(),
                        ctx.role,
                        ctx.avail as Px,
                    );
                    flow.pending.push(Pending::Image {
                        x,
                        dest: measured.dest,
                        slot_w: measured.slot_w,
                        slot_h: measured.slot_h,
                        start: base + start,
                        end: base + end,
                        fallback: measured.fallback,
                    });
                    x += measured.slot_w;
                }
            }
            if tail > to {
                break;
            }
            ix += 1;
        }
        *flow.x = x;
        flow.flush();
    }
}

fn justified(shaped: WrappedLine, shifts: &[(u32, f32)], start: usize, end: usize) -> WrappedLine {
    if shifts.is_empty() {
        return shaped;
    }
    let base = kp_plain::shift_at(shifts, start);
    let inner: Vec<(u32, f32)> = shifts
        .iter()
        .filter(|&&(at, _)| at as usize > start && (at as usize) < end)
        .map(|&(at, delta)| (at - start as u32, delta - base))
        .collect();
    if inner.is_empty() {
        return shaped;
    }
    let mut shaped = shaped;
    let unwrapped_layout = Arc::new(kp_plain::shifted_layout(&shaped.unwrapped_layout, &inner));
    *DerefMut::deref_mut(&mut shaped) = Arc::new(WrappedLineLayout {
        unwrapped_layout,
        wrap_boundaries: SmallVec::new(),
        wrap_width: None,
    });
    shaped
}
