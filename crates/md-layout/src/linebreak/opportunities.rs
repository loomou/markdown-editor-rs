use super::{break_class, is_cjk};
use unicode_linebreak::BreakClass::{
    Alphabetic, CloseParenthesis, ClosePunctuation, ComplexContext, OpenPunctuation,
};
use unicode_linebreak::{BreakOpportunity, linebreaks};
use unicode_segmentation::UnicodeSegmentation;

pub struct OppCtx<'a> {
    pub glue_before: &'a [u32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opp {
    Allowed,
    Mandatory,
}

pub fn opportunities(text: &str, ctx: &OppCtx<'_>) -> Vec<(u32, Opp)> {
    let mut points: Vec<(u32, Opp)> = linebreaks(text)
        .filter(|&(i, _)| i < text.len())
        .map(|(i, kind)| {
            let opp = match kind {
                BreakOpportunity::Mandatory => Opp::Mandatory,
                BreakOpportunity::Allowed => Opp::Allowed,
            };
            (i as u32, opp)
        })
        .collect();
    apply_quotes(text, &mut points);
    points.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| rank(a.1).cmp(&rank(b.1))));
    points.dedup_by_key(|point| point.0);

    let boundaries = grapheme_boundaries(text);
    points.retain(|point| boundaries.binary_search(&point.0).is_ok());

    let first = first_non_whitespace(text);
    points.retain(|point| point.0 > first);

    if !ctx.glue_before.is_empty() {
        points.retain(|point| !ctx.glue_before.contains(&point.0));
    }
    points
}

pub fn has_complex_context(text: &str) -> bool {
    text.chars().any(|c| break_class(c) == ComplexContext)
}

fn rank(opp: Opp) -> u8 {
    match opp {
        Opp::Mandatory => 0,
        Opp::Allowed => 1,
    }
}

fn grapheme_boundaries(text: &str) -> Vec<u32> {
    let mut out: Vec<u32> = text.grapheme_indices(true).map(|(i, _)| i as u32).collect();
    out.push(text.len() as u32);
    out
}

fn first_non_whitespace(text: &str) -> u32 {
    text.char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map_or(text.len() as u32, |(i, _)| i as u32)
}

fn apply_quotes(text: &str, points: &mut Vec<(u32, Opp)>) {
    for (i, c) in text.char_indices() {
        let opening = matches!(c, '\u{201C}' | '\u{2018}');
        if !opening && !matches!(c, '\u{201D}' | '\u{2019}') {
            continue;
        }
        let start = i as u32;
        let end = start + c.len_utf8() as u32;
        let prev = text[..i].chars().next_back();
        let next = text[end as usize..].chars().next();
        if !prev.is_some_and(is_cjk) && !next.is_some_and(is_cjk) {
            continue;
        }
        if opening {
            remove_at(points, end);
            if prev.is_some_and(|p| is_cjk(p) || is_closer(p)) {
                add_at(points, start);
            }
        } else {
            remove_at(points, start);
            if next.is_some_and(|n| {
                is_cjk(n) || break_class(n) == Alphabetic || break_class(n) == OpenPunctuation
            }) {
                add_at(points, end);
            }
        }
    }
}

fn is_closer(c: char) -> bool {
    matches!(break_class(c), ClosePunctuation | CloseParenthesis)
}

fn remove_at(points: &mut Vec<(u32, Opp)>, at: u32) {
    points.retain(|point| point.0 != at);
}

fn add_at(points: &mut Vec<(u32, Opp)>, at: u32) {
    if !points.iter().any(|point| point.0 == at) {
        points.push((at, Opp::Allowed));
    }
}
