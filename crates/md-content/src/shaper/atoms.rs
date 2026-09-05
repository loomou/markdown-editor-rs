use super::bands::Atom;
use md_core::inline::{InlineMarks, InlineRun, covering_runs};
use std::collections::HashMap;

pub(super) fn line_atoms(
    text: &str,
    runs: &[InlineRun],
    link_dests: &HashMap<u32, String>,
    link_raw: &HashMap<u32, (String, String)>,
) -> Vec<Atom> {
    let covered = covering_runs(text.len() as u32, runs);
    let mut out = Vec::new();
    for r in covered {
        let start = r.display_range.start as usize;
        let end = r.display_range.end as usize;
        if end <= start || start > text.len() || end > text.len() {
            continue;
        }
        if r.marks.is_math() {
            out.push(Atom::Math {
                start,
                end,
                display: r.marks.contains(InlineMarks::MATH_DISPLAY),
            });
        } else if r.marks.is_image() {
            let dest = r
                .link
                .and_then(|id| link_dests.get(&id).cloned())
                .unwrap_or_default();
            let raw = r
                .link
                .and_then(|id| link_raw.get(&id))
                .map(|(d, t)| image_source_text(text.get(start..end).unwrap_or(""), d, t));
            out.push(Atom::Image {
                start,
                end,
                dest,
                raw,
            });
        } else if r.marks.contains(InlineMarks::SUPER) {
            out.push(Atom::Script {
                start,
                end,
                super_script: true,
            });
        } else if r.marks.contains(InlineMarks::SUB) {
            out.push(Atom::Script {
                start,
                end,
                super_script: false,
            });
        } else {
            split_words(text, start, end, &mut out);
        }
    }
    out
}

pub(super) fn image_source_text(alt: &str, dest: &str, title: &str) -> String {
    let alt = alt.trim_end_matches('\u{FFFC}');
    if title.is_empty() {
        format!("![{alt}]({dest})")
    } else {
        format!("![{alt}]({dest} \"{title}\")")
    }
}

pub(super) fn split_words(text: &str, start: usize, end: usize, out: &mut Vec<Atom>) {
    let mut i = start;
    while i < end {
        if text.as_bytes()[i] == b'\n' {
            out.push(Atom::Break { offset: i });
            i += 1;
            continue;
        }
        let chunk = i;
        while i < end {
            let ch = text[i..end].chars().next();
            let Some(ch) = ch else {
                break;
            };
            if ch == '\n' || ch.is_whitespace() {
                break;
            }
            i += ch.len_utf8();
        }
        while i < end {
            let ch = text[i..end].chars().next();
            let Some(ch) = ch else {
                break;
            };
            if ch == '\n' || !ch.is_whitespace() {
                break;
            }
            i += ch.len_utf8();
        }
        if i > chunk {
            out.push(Atom::Text {
                start: chunk,
                end: i,
            });
        } else {
            break;
        }
    }
}

pub(super) fn slice_runs(runs: &[InlineRun], start: u32, end: u32) -> Vec<InlineRun> {
    let mut out = Vec::new();
    for r in covering_runs(end, runs) {
        let s = r.display_range.start.max(start);
        let e = r.display_range.end.min(end);
        if e > s {
            out.push(InlineRun {
                display_range: (s - start)..(e - start),
                source_range: r.source_range.clone(),
                marks: r.marks,
                link: r.link,
            });
        }
    }
    out
}
