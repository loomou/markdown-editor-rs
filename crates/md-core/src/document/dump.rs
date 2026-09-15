use super::bind::UNMAPPED;
use super::text::{LeafSource, TextPiece};
use super::{Document, NodeId};
use crate::inline::InlineMarks;
use std::fmt::Write;

const MARK_NAMES: [(InlineMarks, &str); 11] = [
    (InlineMarks::EM, "EM"),
    (InlineMarks::STRONG, "STRONG"),
    (InlineMarks::STRIKE, "STRIKE"),
    (InlineMarks::CODE, "CODE"),
    (InlineMarks::FOOTNOTE, "FOOTNOTE"),
    (InlineMarks::IMAGE, "IMAGE"),
    (InlineMarks::SUPER, "SUPER"),
    (InlineMarks::SUB, "SUB"),
    (InlineMarks::MATH_INLINE, "MATH_INLINE"),
    (InlineMarks::MATH_DISPLAY, "MATH_DISPLAY"),
    (InlineMarks::SYNTAX, "SYNTAX"),
];

fn marks_name(marks: InlineMarks) -> String {
    let mut out = String::new();
    for (flag, name) in MARK_NAMES {
        if marks.contains(flag) {
            if !out.is_empty() {
                out.push('|');
            }
            out.push_str(name);
        }
    }
    if out.is_empty() {
        out.push_str("NONE");
    }
    out
}

fn depth_of(doc: &Document, id: NodeId) -> usize {
    let mut depth = 0;
    let mut cur = doc.arena.get(id).and_then(|n| n.parent);
    while let Some(p) = cur {
        depth += 1;
        cur = doc.arena.get(p).and_then(|n| n.parent);
    }
    depth
}

fn pieces_field(pieces: &[TextPiece]) -> String {
    let mut out = String::from("[");
    for (i, piece) in pieces.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        match piece {
            TextPiece::Source(r) => {
                let _ = write!(out, "Source({}..{})", r.start, r.end);
            }
            TextPiece::Intern(r) => {
                let _ = write!(out, "Intern({}..{})", r.start, r.end);
            }
            TextPiece::Owned => out.push_str("Owned"),
        }
    }
    out.push(']');
    out
}

pub fn dump_document(doc: &Document) -> String {
    let mut out = String::new();
    for id in doc.preorder() {
        let Some(node) = doc.arena.get(id) else {
            continue;
        };
        let depth = depth_of(doc, id);
        let _ = writeln!(
            out,
            "{}#{} {:?} extra={:?}",
            "  ".repeat(depth),
            id.index,
            node.kind,
            node.extra
        );
        let Some(tid) = node.text else {
            continue;
        };
        let Some(leaf) = doc.texts.get(tid) else {
            continue;
        };
        let pad = format!("{}  ", "  ".repeat(depth));
        let _ = writeln!(out, "{pad}display={:?}", leaf.display());
        match &leaf.source {
            LeafSource::Span(r) => {
                let _ = writeln!(
                    out,
                    "{pad}source=Span({}..{}) -> {:?}",
                    r.start,
                    r.end,
                    leaf.source_str(&doc.source)
                );
            }
            LeafSource::SameAsDisplay => {
                let _ = writeln!(out, "{pad}source=SameAsDisplay -> {:?}", leaf.display());
            }
            LeafSource::Owned(s) => {
                let _ = writeln!(out, "{pad}source=Owned({s:?})");
            }
        }
        let _ = writeln!(out, "{pad}pieces={}", pieces_field(&leaf.pieces));
        let mut runs = String::from("[");
        for run in leaf.runs() {
            let src = match &run.source_range {
                None => "None".to_string(),
                Some(r) => format!("Some({}..{})", r.start, r.end),
            };
            let _ = write!(
                runs,
                "({}..{} marks={} link={:?} src={})",
                run.display_range.start,
                run.display_range.end,
                marks_name(run.marks),
                run.link,
                src
            );
        }
        runs.push(']');
        let _ = writeln!(out, "{pad}runs={runs}");
        let mut s2d = String::from("[");
        for (i, v) in leaf.s2d.iter().enumerate() {
            if i > 0 {
                s2d.push_str(", ");
            }
            if *v == UNMAPPED {
                s2d.push('_');
            } else {
                let _ = write!(s2d, "{v}");
            }
        }
        s2d.push(']');
        let _ = writeln!(out, "{pad}s2d={s2d}");
        match &leaf.constructs {
            None => {
                let _ = writeln!(out, "{pad}constructs=None");
            }
            Some(list) => {
                let mut cs = String::from("[");
                for (i, c) in list.iter().enumerate() {
                    if i > 0 {
                        cs.push_str(", ");
                    }
                    let _ = write!(
                        cs,
                        "({}..{} src {}..{})",
                        c.source.start, c.source.end, c.inner.start, c.inner.end
                    );
                }
                cs.push(']');
                let _ = writeln!(out, "{pad}constructs={cs}");
            }
        }
    }
    let _ = writeln!(out, "links:");
    for (i, link) in doc.links.iter().enumerate() {
        let _ = writeln!(out, "  {i}: {link:?}");
    }
    let _ = writeln!(out, "langs:");
    for (i, lang) in doc.langs.iter().enumerate() {
        let _ = writeln!(out, "  {i}: {lang:?}");
    }
    let _ = writeln!(out, "footnotes:");
    for (i, label) in doc.footnotes.iter().enumerate() {
        let _ = writeln!(out, "  {i}: {label:?}");
    }
    let _ = writeln!(out, "reference_definitions:");
    for (i, def) in doc.reference_definitions.iter().enumerate() {
        let _ = writeln!(out, "  {i}: {def:?}");
    }
    out
}
