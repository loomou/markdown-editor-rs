use crate::gpui_theme::ThemeColorExt;
use crate::highlight::Span;
use gpui::Hsla;
use md_theme::{SyntaxRole, SyntaxTokens};

pub(super) fn color_runs_by_line(
    text: &str,
    spans: &[Span],
    syntax: SyntaxTokens,
) -> Vec<Vec<(u32, Hsla)>> {
    if spans.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut byte = 0usize;
    let mut si = 0usize;
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            byte += 1;
        }
        let start = byte;
        let end = start + line.len();
        byte = end;
        while si < spans.len() && spans[si].end <= start {
            si += 1;
        }
        let mut runs = Vec::new();
        let mut at = start;
        let mut sj = si;
        while at < end {
            while sj < spans.len() && spans[sj].end <= at {
                sj += 1;
            }
            let Some(span) = spans.get(sj) else {
                push_color_run(
                    &mut runs,
                    (end - at) as u32,
                    syntax.color(SyntaxRole::Default).hsla(),
                );
                break;
            };
            let s = span.start.max(at).min(end);
            let e = span.end.min(end);
            if s > at {
                push_color_run(
                    &mut runs,
                    (s - at) as u32,
                    syntax.color(SyntaxRole::Default).hsla(),
                );
                at = s;
                continue;
            }
            if e > at {
                push_color_run(&mut runs, (e - at) as u32, syntax.color(span.role).hsla());
                at = e;
            } else {
                sj += 1;
            }
        }
        out.push(runs);
    }
    out
}

pub(super) fn push_color_run(runs: &mut Vec<(u32, Hsla)>, len: u32, color: Hsla) {
    if len == 0 {
        return;
    }
    if let Some(last) = runs.last_mut()
        && last.1 == color
    {
        last.0 += len;
        return;
    }
    runs.push((len, color));
}
