use super::{Link, intern_push, source_piece, strip_html};
use crate::document::text::{LeafText, TextPiece};
use crate::inline::{InlineMarks, InlineRun};
use pulldown_cmark::{NodeKind, NodeRef, Parsed};
use std::ops::Range;

pub(crate) struct Projection {
    pub(crate) display: String,
    pub(crate) runs: Vec<InlineRun>,
    pub(crate) links: Vec<Link>,
}

#[derive(Clone, Copy, Default)]
struct Ctx {
    marks: InlineMarks,
    link: Option<u32>,
}

pub(crate) fn project_paragraph(parsed: &Parsed<'_>, source: &str) -> Option<Projection> {
    let para = parsed.root().first_child()?;
    if para.next_sibling().is_some() || !matches!(para.kind(), NodeKind::Paragraph) {
        return None;
    }
    let mut leaf = LeafText::default();
    let mut intern = String::new();
    let mut links: Vec<Link> = Vec::new();
    let mut stack: Vec<(NodeRef<'_, '_>, Ctx)> = Vec::new();
    if let Some(child) = para.first_child() {
        stack.push((child, Ctx::default()));
    }
    while let Some((node, ctx)) = stack.pop() {
        if let Some(sibling) = node.next_sibling() {
            stack.push((sibling, ctx));
        }
        let span = node.span();
        let lo = span.start.min(source.len());
        let range = lo..span.end.min(source.len()).max(lo);
        match node.kind() {
            NodeKind::Text { .. } | NodeKind::TextOwned => {
                let text = node.text().unwrap_or("");
                span_of(&mut leaf, &mut intern, source, text, range, ctx);
            }
            NodeKind::SynthesizedChar(c) => {
                let mut buf = [0u8; 4];
                let text = c.encode_utf8(&mut buf);
                span_of(&mut leaf, &mut intern, source, text, range, ctx);
            }
            NodeKind::Code(content) => {
                let inner = Ctx {
                    marks: ctx.marks.union(InlineMarks::CODE),
                    ..ctx
                };
                span_of(
                    &mut leaf,
                    &mut intern,
                    source,
                    content.as_ref(),
                    range,
                    inner,
                );
            }
            NodeKind::Emphasis | NodeKind::Strong | NodeKind::Strikethrough => {
                let bit = match node.kind() {
                    NodeKind::Emphasis => InlineMarks::EM,
                    NodeKind::Strong => InlineMarks::STRONG,
                    _ => InlineMarks::STRIKE,
                };
                let inner = Ctx {
                    marks: ctx.marks.union(bit),
                    ..ctx
                };
                push_child(&mut stack, node, inner);
            }
            NodeKind::Superscript | NodeKind::Subscript => {
                let bit = match node.kind() {
                    NodeKind::Superscript => InlineMarks::SUPER,
                    _ => InlineMarks::SUB,
                };
                let inner = Ctx {
                    marks: ctx.marks.union(bit),
                    ..ctx
                };
                push_child(&mut stack, node, inner);
            }
            NodeKind::Link(info) => {
                let id = links.len() as u32;
                links.push(Link {
                    dest: info.dest_url.as_ref().to_string(),
                    title: info.title.as_ref().to_string(),
                });
                let inner = Ctx {
                    link: Some(id),
                    ..ctx
                };
                push_child(&mut stack, node, inner);
            }
            NodeKind::SoftBreak | NodeKind::HardBreak { .. } => {
                intern_of(&mut leaf, &mut intern, "\n", ctx);
            }
            NodeKind::FootnoteReference(label) => {
                let inner = Ctx {
                    marks: ctx.marks.union(InlineMarks::FOOTNOTE),
                    ..ctx
                };
                intern_of(&mut leaf, &mut intern, &format!("[^{label}]"), inner);
            }
            NodeKind::InlineHtml => {
                let text = node.text().unwrap_or("");
                let stripped = strip_html(text);
                if !stripped.is_empty() {
                    intern_of(&mut leaf, &mut intern, &stripped, ctx);
                }
            }
            _ => return None,
        }
    }
    let snapshot = leaf.snapshot();
    Some(Projection {
        display: snapshot.display.clone(),
        runs: snapshot.runs.clone(),
        links,
    })
}

fn push_child<'a, 'i>(stack: &mut Vec<(NodeRef<'a, 'i>, Ctx)>, node: NodeRef<'a, 'i>, ctx: Ctx) {
    if let Some(child) = node.first_child() {
        stack.push((child, ctx));
    }
}

fn span_of(
    leaf: &mut LeafText,
    intern: &mut String,
    source: &str,
    text: &str,
    range: Range<usize>,
    ctx: Ctx,
) {
    if text.is_empty() {
        return;
    }
    match source_piece(source, text, range) {
        Some(r) => leaf.append(
            TextPiece::Source(r.clone()),
            text,
            Some(r),
            ctx.marks,
            ctx.link,
        ),
        None => {
            let r = intern_push(intern, text);
            leaf.append(TextPiece::Intern(r), text, None, ctx.marks, ctx.link);
        }
    }
}

fn intern_of(leaf: &mut LeafText, intern: &mut String, text: &str, ctx: Ctx) {
    if text.is_empty() {
        return;
    }
    let r = intern_push(intern, text);
    leaf.append(TextPiece::Intern(r), text, None, ctx.marks, ctx.link);
}

#[cfg(test)]
mod tests {
    use super::project_paragraph;
    use crate::block::BlockKind;
    use crate::document::{bind, sanitized_editor_options};
    use crate::inline::InlineRun;
    use pulldown_cmark::Parsed;

    type Leaf = (String, Vec<InlineRun>, usize);

    fn slow(source: &str) -> Option<Leaf> {
        let parsed = Parsed::inline_only(source, sanitized_editor_options());
        let frag = super::super::load_parsed(&parsed, source.to_string());
        let leaf = bind::matching_leaf(&frag, BlockKind::Paragraph)?;
        Some((
            frag.display(leaf).to_string(),
            frag.runs(leaf).to_vec(),
            frag.links.len(),
        ))
    }

    fn fast(source: &str) -> Option<Leaf> {
        let parsed = Parsed::inline_only(source, sanitized_editor_options());
        let projection = project_paragraph(&parsed, source)?;
        Some((projection.display, projection.runs, projection.links.len()))
    }

    #[test]
    fn the_fast_projection_matches_the_full_loader() {
        let cases = [
            "plain text here",
            "text with `code` in it",
            "*em* and **strong** and ~~strike~~",
            "[link](https://example.com) tail",
            "[a](https://one.example) and [b](https://two.example)",
            "**bold `code` inside**",
            "escaped \\*not em\\* here",
            "entities &amp; and &#65; and &quot;q&quot;",
            "line one\nline two",
            "trailing spaces   ",
            "unicode ok tail",
            "<span>html</span> tail",
            "text with [^note] reference",
            "a^sup^ and b~sub~",
            "`a` `b` `c` `d`",
            "**a** *b* `c` [d](https://example.com)",
        ];
        let mut compared = 0usize;
        for case in cases {
            let Some(slow) = slow(case) else {
                continue;
            };
            let fast = fast(case).unwrap_or_else(|| {
                panic!("{case:?} is a plain paragraph, so the fast path must take it")
            });
            assert_eq!(fast.0, slow.0, "display differs for {case:?}");
            assert_eq!(fast.1, slow.1, "runs differ for {case:?}");
            assert_eq!(fast.2, slow.2, "links differ for {case:?}");
            compared += 1;
        }
        assert!(
            compared >= 14,
            "only {compared} cases were compared, so the sweep stopped covering the shapes it claims"
        );
    }

    #[test]
    fn constructs_the_fast_path_refuses_fall_back_cleanly() {
        for case in [
            "an ![image](https://example.com/a.png) here",
            "inline $x + y$ math",
            "$$display math$$",
        ] {
            assert!(
                fast(case).is_none(),
                "{case:?} must be refused so the full loader keeps it"
            );
        }
    }
}
