use super::{Item, Mode, Opp, Params, Placed, break_class, effective_width, is_cjk};
use std::ops::Range;
use unicode_linebreak::BreakClass;
use unicode_segmentation::UnicodeSegmentation;

pub struct Clusters {
    pub bytes: Vec<u32>,
    pub xs: Vec<f32>,
}

impl Clusters {
    pub fn from_pairs<I: IntoIterator<Item = (u32, f32)>>(pairs: I) -> Option<Self> {
        let mut bytes = Vec::new();
        let mut xs = Vec::new();
        let mut previous: Option<u32> = None;
        for (byte, x) in pairs {
            if previous.is_some_and(|p| byte <= p) {
                return None;
            }
            previous = Some(byte);
            bytes.push(byte);
            xs.push(x);
        }
        Some(Self { bytes, xs })
    }

    pub fn x_at(&self, byte: u32) -> f32 {
        match self.bytes.binary_search(&byte) {
            Ok(i) => self.xs[i],
            Err(i) => self.xs.get(i).copied().unwrap_or(0.0),
        }
    }

    pub fn width(&self, range: Range<u32>) -> f32 {
        self.x_at(range.end) - self.x_at(range.start)
    }

    pub fn is_cluster_start(&self, byte: u32) -> bool {
        self.bytes.binary_search(&byte).is_ok()
    }
}

pub struct ItemCtx<'a> {
    pub mode: Mode,
    pub em: f32,
    pub width: f32,
    pub params: &'a Params,
    pub code_ranges: &'a [Range<u32>],
    pub tab_ranges: &'a [Range<u32>],
}

pub fn build_items(
    text: &str,
    clusters: &Clusters,
    opps: &[(u32, Opp)],
    ctx: &ItemCtx<'_>,
) -> Vec<Placed> {
    let mut out = Vec::with_capacity(opps.len() * 2 + 3);
    let mut start = 0u32;
    for &(position, kind) in opps {
        if position <= start {
            continue;
        }
        if kind == Opp::Allowed && !clusters.is_cluster_start(position) {
            continue;
        }
        let (content_end, space_end) = push_chunk(&mut out, text, clusters, start, position, ctx);
        match kind {
            Opp::Mandatory => out.push(placed(
                Item::Penalty {
                    width: 0.0,
                    cost: f32::NEG_INFINITY,
                    flagged: false,
                },
                content_end,
            )),
            Opp::Allowed => {
                if space_end > content_end {
                    let width = clusters.width(content_end..space_end);
                    let (stretch, shrink) =
                        glue_elasticity(width, ctx, is_tab_range(ctx, content_end, space_end));
                    out.push(placed(
                        Item::Glue {
                            width,
                            stretch,
                            shrink,
                        },
                        content_end,
                    ));
                } else {
                    out.push(placed(break_item(text, position, ctx), position));
                }
            }
        }
        start = position;
    }
    let (content_end, _) = push_chunk(&mut out, text, clusters, start, text.len() as u32, ctx);
    if ctx.mode != Mode::Balanced {
        out.push(placed(
            Item::Penalty {
                width: 0.0,
                cost: f32::INFINITY,
                flagged: false,
            },
            content_end,
        ));
        out.push(placed(
            Item::Glue {
                width: 0.0,
                stretch: f32::INFINITY,
                shrink: 0.0,
            },
            content_end,
        ));
    }
    out.push(placed(
        Item::Penalty {
            width: 0.0,
            cost: f32::NEG_INFINITY,
            flagged: false,
        },
        content_end,
    ));
    out
}

fn push_chunk(
    out: &mut Vec<Placed>,
    text: &str,
    clusters: &Clusters,
    from: u32,
    to: u32,
    ctx: &ItemCtx<'_>,
) -> (u32, u32) {
    let content_end = trim_end(text, from, to);
    let space_end = space_run_end(text, content_end, to);
    let mut piece = from;
    for (offset, c) in text[from as usize..content_end as usize].char_indices() {
        if break_class(c) != BreakClass::NonBreakingGlue {
            continue;
        }
        let at = from + offset as u32;
        push_box(out, text, clusters, piece, at, ctx);
        out.push(placed(
            Item::Penalty {
                width: 0.0,
                cost: f32::INFINITY,
                flagged: false,
            },
            at,
        ));
        let width = clusters.width(at..at + c.len_utf8() as u32);
        let (stretch, shrink) = glue_elasticity(width, ctx, false);
        out.push(placed(
            Item::Glue {
                width,
                stretch,
                shrink,
            },
            at,
        ));
        piece = at + c.len_utf8() as u32;
    }
    push_box(out, text, clusters, piece, content_end, ctx);
    (content_end, space_end)
}

fn push_box(
    out: &mut Vec<Placed>,
    text: &str,
    clusters: &Clusters,
    from: u32,
    to: u32,
    ctx: &ItemCtx<'_>,
) {
    if to <= from {
        out.push(placed(Item::Box { width: 0.0 }, from));
        return;
    }
    let width = clusters.width(from..to);
    let limit = effective_width(ctx.width) as f32;
    if width <= limit {
        out.push(placed(Item::Box { width }, from));
        return;
    }
    let mut pieces = Vec::new();
    for (offset, _) in text[from as usize..to as usize]
        .grapheme_indices(true)
        .skip(1)
    {
        let at = from + offset as u32;
        if at < to && clusters.is_cluster_start(at) {
            pieces.push(at);
        }
    }
    if pieces.is_empty() {
        out.push(placed(Item::Box { width }, from));
        return;
    }
    let mut piece = from;
    for cut in pieces.into_iter().chain(std::iter::once(to)) {
        out.push(placed(
            Item::Box {
                width: clusters.width(piece..cut),
            },
            piece,
        ));
        if cut < to {
            out.push(placed(
                Item::Penalty {
                    width: 0.0,
                    cost: ctx.params.emergency_penalty,
                    flagged: false,
                },
                cut,
            ));
        }
        piece = cut;
    }
}

fn break_item(text: &str, position: u32, ctx: &ItemCtx<'_>) -> Item {
    let at = position as usize;
    if ctx
        .code_ranges
        .iter()
        .any(|range| range.start <= position && position < range.end)
    {
        return Item::Penalty {
            width: 0.0,
            cost: ctx.params.code_break_penalty,
            flagged: false,
        };
    }
    let prev = text[..at].chars().next_back();
    let next = text[at..].chars().next();
    if matches!(prev, Some('-') | Some('\u{2010}')) {
        return Item::Penalty {
            width: 0.0,
            cost: ctx.params.hyphen_penalty,
            flagged: true,
        };
    }
    if prev.is_some_and(is_cjk) || next.is_some_and(is_cjk) {
        return match ctx.mode {
            Mode::Justify => Item::Glue {
                width: 0.0,
                stretch: ctx.params.cjk_stretch_em * ctx.em,
                shrink: 0.0,
            },
            Mode::Ragged | Mode::Balanced => Item::Penalty {
                width: 0.0,
                cost: 0.0,
                flagged: false,
            },
        };
    }
    Item::Penalty {
        width: 0.0,
        cost: 0.0,
        flagged: false,
    }
}

fn glue_elasticity(width: f32, ctx: &ItemCtx<'_>, fixed: bool) -> (f32, f32) {
    if ctx.mode == Mode::Justify && !fixed {
        (width * 0.5, width / 3.0)
    } else {
        (0.0, 0.0)
    }
}

fn is_tab_range(ctx: &ItemCtx<'_>, from: u32, to: u32) -> bool {
    ctx.tab_ranges
        .iter()
        .any(|range| range.start < to && from < range.end)
}

fn is_line_break(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

fn is_break_space(c: char) -> bool {
    c.is_whitespace() && !is_line_break(c) && break_class(c) != BreakClass::NonBreakingGlue
}

fn trim_end(text: &str, from: u32, to: u32) -> u32 {
    let start = from as usize;
    let mut end = to as usize;
    while end > start {
        let Some(c) = text[start..end].chars().next_back() else {
            break;
        };
        if is_break_space(c) || is_line_break(c) {
            end -= c.len_utf8();
        } else {
            break;
        }
    }
    end as u32
}

fn space_run_end(text: &str, from: u32, to: u32) -> u32 {
    let mut at = from as usize;
    let end = to as usize;
    while at < end {
        let Some(c) = text[at..end].chars().next() else {
            break;
        };
        if is_break_space(c) {
            at += c.len_utf8();
        } else {
            break;
        }
    }
    at as u32
}

fn placed(item: Item, byte: u32) -> Placed {
    Placed { item, byte }
}
