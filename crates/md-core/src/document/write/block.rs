use super::inline::{phrasing_export, trim_end_newlines};
use super::{
    MarkdownExport, MarkdownWriter, Prefix, blank_line, push_first_and_rest, write_prefixed,
};
use crate::block::{BlockKind, CodeFenceMarker, ListMarker, NodeExtra};
use crate::document::arena::NodeId;
use std::fmt;
use std::fmt::Write;

enum Step {
    Block {
        id: NodeId,
        prefix: Prefix,
    },

    Flow {
        kids: Vec<NodeId>,
        prefix: Prefix,
    },

    List {
        items: Vec<NodeId>,
        prefix: Prefix,
        ordered: bool,
        loose: bool,
        start: u64,
        marker: ListMarker,
    },

    Item {
        id: NodeId,
        prefix: Prefix,
        ordered: bool,
        num: u64,
        marker: ListMarker,
    },

    Footnote {
        id: NodeId,
        prefix: Prefix,
    },

    BlankLine {
        prefix: Prefix,
    },

    Newline,
}

pub(crate) fn run<D, W>(
    doc: &D,
    root: NodeId,
    out: &mut MarkdownWriter<W>,
    prefix: &Prefix,
) -> fmt::Result
where
    D: MarkdownExport,
    W: fmt::Write,
{
    run_stack(
        doc,
        vec![Step::Block {
            id: root,
            prefix: prefix.clone(),
        }],
        out,
    )
}

pub(crate) fn run_item<D, W>(
    doc: &D,
    id: NodeId,
    out: &mut MarkdownWriter<W>,
    ordered: bool,
    num: u64,
    marker: ListMarker,
) -> fmt::Result
where
    D: MarkdownExport,
    W: fmt::Write,
{
    run_stack(
        doc,
        vec![Step::Item {
            id,
            prefix: Prefix::default(),
            ordered,
            num,
            marker,
        }],
        out,
    )
}

fn push_rev(stack: &mut Vec<Step>, steps: Vec<Step>) {
    stack.extend(steps.into_iter().rev());
}

fn run_stack<D, W>(doc: &D, mut stack: Vec<Step>, out: &mut MarkdownWriter<W>) -> fmt::Result
where
    D: MarkdownExport,
    W: fmt::Write,
{
    while let Some(step) = stack.pop() {
        match step {
            Step::Newline => out.write_str("\n")?,
            Step::BlankLine { prefix } => blank_line(out, &prefix)?,
            Step::Block { id, prefix } => write_block_step(doc, id, &prefix, out, &mut stack)?,
            Step::Flow { kids, prefix } => write_flow_step(doc, kids, &prefix, out, &mut stack)?,
            Step::List {
                items,
                prefix,
                ordered,
                loose,
                start,
                marker,
            } => write_list_step(items, &prefix, ordered, loose, start, marker, &mut stack)?,
            Step::Item {
                id,
                prefix,
                ordered,
                num,
                marker,
            } => write_item_step(doc, id, &prefix, ordered, num, marker, out, &mut stack)?,
            Step::Footnote { id, prefix } => {
                write_footnote_step(doc, id, &prefix, out, &mut stack)?
            }
        }
    }
    Ok(())
}

fn write_block_step<D, W>(
    doc: &D,
    id: NodeId,
    prefix: &Prefix,
    out: &mut MarkdownWriter<W>,
    stack: &mut Vec<Step>,
) -> fmt::Result
where
    D: MarkdownExport,
    W: fmt::Write,
{
    let Some(kind) = doc.kind(id) else {
        return Ok(());
    };
    match kind {
        BlockKind::DocRoot => {
            let kids: Vec<NodeId> = doc.children(id).collect();

            let kids = match kids.last().copied().filter(|&k| is_blank_paragraph(doc, k)) {
                Some(_) => kids[..kids.len() - 1].to_vec(),
                None => kids,
            };
            stack.push(Step::Flow {
                kids,
                prefix: prefix.clone(),
            });
            Ok(())
        }
        BlockKind::DocStart => Ok(()),
        BlockKind::Paragraph => write_prefixed(out, prefix, phrasing_export(doc, id)),
        BlockKind::Heading(n) => write_prefixed(out, prefix, &heading_line(doc, id, n)),
        BlockKind::MetadataBlock => write_prefixed(out, prefix, doc.leaf_source(id)),
        BlockKind::CodeBlock | BlockKind::Mermaid => write_fence(doc, id, out, prefix),
        BlockKind::Math => write_math(doc, id, out, prefix),
        BlockKind::BlockQuote => {
            let inner = prefix.quoted();
            let has_children = doc.children(id).next().is_some();
            if let Some((kind, lowercase_mask, blank_after_marker)) =
                doc.extra(id).quote_alert_style()
            {
                let label = kind
                    .label()
                    .bytes()
                    .enumerate()
                    .map(|(index, byte)| {
                        if lowercase_mask & (1 << index) != 0 {
                            byte.to_ascii_lowercase() as char
                        } else {
                            byte as char
                        }
                    })
                    .collect::<String>();
                write_prefixed(out, &inner, &format!("[!{label}]"))?;
                if has_children {
                    out.write_str("\n")?;
                    if blank_after_marker {
                        blank_line(out, &inner)?;
                    }
                    stack.push(Step::Flow {
                        kids: doc.children(id).collect(),
                        prefix: inner,
                    });
                }
            } else if has_children {
                stack.push(Step::Flow {
                    kids: doc.children(id).collect(),
                    prefix: inner,
                });
            } else {
                write_prefixed(out, &inner, "")?;
            }
            Ok(())
        }
        BlockKind::List => {
            let extra = doc.extra(id);
            let ordered = extra.ordered_start().is_some();
            let marker = extra.list_marker();
            let loose = extra.list_loose();
            let start = extra.ordered_start().unwrap_or(1);
            let items: Vec<NodeId> = doc.children(id).collect();
            stack.push(Step::List {
                items,
                prefix: prefix.clone(),
                ordered,
                loose,
                start,
                marker,
            });
            Ok(())
        }
        BlockKind::ListItem => {
            stack.push(Step::Item {
                id,
                prefix: prefix.clone(),
                ordered: false,
                num: 1,
                marker: ListMarker::Dash,
            });
            Ok(())
        }
        BlockKind::ThematicBreak => write_prefixed(out, prefix, &thematic_break_line(doc, id)),
        BlockKind::Image => write_image(doc, id, out, prefix),
        BlockKind::Table => write_table(doc, id, out, prefix),
        BlockKind::TableRow | BlockKind::TableCell => Ok(()),
        BlockKind::FootnoteDefinition => {
            stack.push(Step::Footnote {
                id,
                prefix: prefix.clone(),
            });
            Ok(())
        }
    }
}

fn is_blank_paragraph<D: MarkdownExport>(doc: &D, id: NodeId) -> bool {
    doc.kind(id) == Some(BlockKind::Paragraph)
        && matches!(doc.extra(id), NodeExtra::None)
        && doc.display(id).is_empty()
        && doc.leaf_source(id).trim().is_empty()
}

fn write_flow_step<D, W>(
    doc: &D,
    kids: Vec<NodeId>,
    prefix: &Prefix,
    _out: &mut MarkdownWriter<W>,
    stack: &mut Vec<Step>,
) -> fmt::Result
where
    D: MarkdownExport,
    W: fmt::Write,
{
    let mut steps = Vec::with_capacity(kids.len() * 2);
    let mut previous = None;
    for (i, id) in kids.into_iter().enumerate() {
        if i > 0 && previous != Some(BlockKind::MetadataBlock) {
            steps.push(Step::BlankLine {
                prefix: prefix.clone(),
            });
        }
        steps.push(Step::Block {
            id,
            prefix: prefix.clone(),
        });
        previous = doc.kind(id);
    }
    push_rev(stack, steps);
    Ok(())
}

fn write_list_step(
    items: Vec<NodeId>,
    prefix: &Prefix,
    ordered: bool,
    loose: bool,
    start: u64,
    marker: ListMarker,
    stack: &mut Vec<Step>,
) -> fmt::Result {
    let mut steps = Vec::with_capacity(items.len() * 3);
    let mut num = start;
    for (i, item) in items.iter().copied().enumerate() {
        if i > 0 {
            steps.push(Step::Newline);
            if loose {
                steps.push(Step::BlankLine {
                    prefix: prefix.clone(),
                });
            }
        }
        steps.push(Step::Item {
            id: item,
            prefix: prefix.clone(),
            ordered,
            num,
            marker,
        });
        num = num.saturating_add(1);
    }
    push_rev(stack, steps);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_item_step<D, W>(
    doc: &D,
    id: NodeId,
    prefix: &Prefix,
    ordered: bool,
    num: u64,
    marker_style: ListMarker,
    out: &mut MarkdownWriter<W>,
    stack: &mut Vec<Step>,
) -> fmt::Result
where
    D: MarkdownExport,
    W: fmt::Write,
{
    let task = doc.extra(id).task_checked();
    let bullet = match marker_style {
        ListMarker::Plus => '+',
        ListMarker::Star => '*',
        _ => '-',
    };
    let delimiter = if marker_style == ListMarker::Parenthesis {
        ')'
    } else {
        '.'
    };
    let marker = match (task, ordered) {
        (Some(checked), true) => {
            format!("{num}{delimiter} [{}] ", if checked { 'x' } else { ' ' })
        }
        (Some(checked), false) => {
            format!("{bullet} [{}] ", if checked { 'x' } else { ' ' })
        }
        (None, true) => format!("{num}{delimiter} "),
        (None, false) => format!("{bullet} "),
    };

    let list_marker_width = if task.is_some() {
        if ordered {
            format!("{num}{delimiter} ").len()
        } else {
            2
        }
    } else {
        marker.len()
    };
    let rest = prefix.indented(list_marker_width);
    let kids: Vec<NodeId> = doc.children(id).collect();
    if kids.is_empty() {
        prefix.write_open(out)?;
        out.write_str(&marker)?;
        return Ok(());
    }
    let first = kids[0];
    let first_kind = doc.kind(first);
    let mut steps = Vec::with_capacity(kids.len() * 3);
    match first_kind {
        Some(BlockKind::Paragraph | BlockKind::Heading(_)) => {
            prefix.write_open(out)?;
            out.write_str(&marker)?;
            let heading;
            let text = if first_kind == Some(BlockKind::Paragraph) {
                phrasing_export(doc, first)
            } else if let Some(BlockKind::Heading(n)) = first_kind {
                heading = heading_line(doc, first, n);
                heading.as_str()
            } else {
                ""
            };
            push_first_and_rest(out, &rest, text)?;
        }
        _ => {
            prefix.write_open(out)?;
            out.write_str(&marker)?;
            out.write_str("\n")?;
            steps.push(Step::Block {
                id: first,
                prefix: rest.clone(),
            });
        }
    }
    let loose = doc
        .parent(id)
        .is_some_and(|list| doc.extra(list).list_loose());
    for rest_id in kids.into_iter().skip(1) {
        steps.push(Step::Newline);

        if loose
            || matches!(
                doc.kind(rest_id),
                Some(
                    BlockKind::Table
                        | BlockKind::Paragraph
                        | BlockKind::Heading(_)
                        | BlockKind::ThematicBreak
                        | BlockKind::Image
                )
            )
        {
            steps.push(Step::BlankLine {
                prefix: rest.clone(),
            });
        }
        steps.push(Step::Block {
            id: rest_id,
            prefix: rest.clone(),
        });
    }
    push_rev(stack, steps);
    Ok(())
}

fn thematic_break_line<D: MarkdownExport>(doc: &D, id: NodeId) -> String {
    let source = trim_end_newlines(doc.leaf_source(id));
    if crate::document::syntax::is_thematic_break_line(source) {
        source.to_string()
    } else {
        "---".to_string()
    }
}

fn heading_line<D: MarkdownExport>(doc: &D, id: NodeId, level: u8) -> String {
    let source = trim_end_newlines(doc.leaf_source(id));
    if source.contains('\n') {
        return source.to_string();
    }
    let hashes = source.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes)
        && (source.len() == hashes || source.as_bytes().get(hashes) == Some(&b' '))
    {
        return source.to_string();
    }
    let body = if source.is_empty() {
        doc.display(id)
    } else {
        source
    };
    let marks = "#".repeat(level.max(1) as usize);
    if body.is_empty() {
        marks
    } else {
        format!("{marks} {body}")
    }
}

fn write_fence<D, W>(
    doc: &D,
    id: NodeId,
    out: &mut MarkdownWriter<W>,
    prefix: &Prefix,
) -> fmt::Result
where
    D: MarkdownExport,
    W: fmt::Write,
{
    let body = doc.display(id);
    if doc.extra(id).code_is_indented() {
        return write_prefixed(out, &prefix.indented(4), body);
    }
    let lang = doc
        .extra(id)
        .code_fence_lang()
        .and_then(|i| doc.lang(i))
        .unwrap_or_else(|| {
            if doc.kind(id) == Some(BlockKind::Mermaid) {
                "mermaid"
            } else {
                ""
            }
        });
    let (marker, min_len) = doc.extra(id).code_fence_style().unwrap_or_default();
    let marker = if marker == CodeFenceMarker::Tilde {
        '~'
    } else {
        '`'
    };
    let mut fence = marker.to_string().repeat(min_len.max(3));
    while body.contains(&fence) {
        fence.push(marker);
    }
    let open = if lang.is_empty() {
        fence.clone()
    } else {
        format!("{fence}{lang}")
    };
    write_prefixed(out, prefix, &open)?;
    out.write_str("\n")?;
    if !body.is_empty() {
        write_prefixed(out, prefix, body)?;
        out.write_str("\n")?;
    }
    write_prefixed(out, prefix, &fence)
}

fn write_math<D, W>(
    doc: &D,
    id: NodeId,
    out: &mut MarkdownWriter<W>,
    prefix: &Prefix,
) -> fmt::Result
where
    D: MarkdownExport,
    W: fmt::Write,
{
    let body = doc.display(id);

    if doc.extra(id).math_fenced() || body.contains('\n') {
        write_prefixed(out, prefix, "$$")?;
        out.write_str("\n")?;
        if !body.is_empty() {
            write_prefixed(out, prefix, body)?;
            out.write_str("\n")?;
        }
        write_prefixed(out, prefix, "$$")
    } else {
        write_prefixed(out, prefix, &format!("$${body}$$"))
    }
}

fn write_image<D, W>(
    doc: &D,
    id: NodeId,
    out: &mut MarkdownWriter<W>,
    prefix: &Prefix,
) -> fmt::Result
where
    D: MarkdownExport,
    W: fmt::Write,
{
    if let Some(raw) = doc.raw_block(id) {
        return out.write_str(raw);
    }

    let source = doc.leaf_source(id);
    if !source.is_empty() {
        return write_prefixed(out, prefix, source);
    }
    let dest = doc
        .extra(id)
        .image_dest()
        .and_then(|i| doc.link_dest(i))
        .unwrap_or("");
    let cap = doc.display(id);
    write_prefixed(out, prefix, &format!("![{cap}]({dest})"))
}

fn write_table<D, W>(
    doc: &D,
    id: NodeId,
    out: &mut MarkdownWriter<W>,
    prefix: &Prefix,
) -> fmt::Result
where
    D: MarkdownExport,
    W: fmt::Write,
{
    if let Some(raw) = doc.raw_block(id) {
        return out.write_str(raw);
    }
    let rows: Vec<NodeId> = doc.children(id).collect();
    let cols = rows
        .first()
        .map(|row| doc.children(*row).count())
        .unwrap_or(0);
    for (ri, row) in rows.iter().copied().enumerate() {
        write_table_row(doc, row, out, prefix)?;
        if ri == 0 {
            out.write_str("\n")?;
            write_table_sep(doc, id, cols, out, prefix)?;
        }
        if ri + 1 < rows.len() {
            out.write_str("\n")?;
        }
    }
    Ok(())
}

fn write_table_row<D, W>(
    doc: &D,
    row: NodeId,
    out: &mut MarkdownWriter<W>,
    prefix: &Prefix,
) -> fmt::Result
where
    D: MarkdownExport,
    W: fmt::Write,
{
    prefix.write_open(out)?;
    out.write_str("|")?;
    for cell in doc.children(row) {
        out.write_str(" ")?;
        let text = escape_table_pipes(phrasing_export(doc, cell)).replace('\n', "<br>");
        out.write_str(&text)?;
        out.write_str(" |")?;
    }
    Ok(())
}

fn escape_table_pipes(source: &str) -> String {
    let mut escaped = String::with_capacity(source.len());
    let mut backslashes = 0usize;
    for ch in source.chars() {
        if ch == '\\' {
            backslashes += 1;
            escaped.push(ch);
            continue;
        }
        if ch == '|' && backslashes.is_multiple_of(2) {
            escaped.push('\\');
        }
        escaped.push(ch);
        backslashes = 0;
    }
    escaped
}

fn write_table_sep<D, W>(
    doc: &D,
    table: NodeId,
    cols: usize,
    out: &mut MarkdownWriter<W>,
    prefix: &Prefix,
) -> fmt::Result
where
    D: MarkdownExport,
    W: fmt::Write,
{
    prefix.write_open(out)?;
    out.write_str("|")?;
    for i in 0..cols {
        let cell = match doc.table_alignment(table, i) {
            2 => ":---:",
            3 => "---:",
            _ => "---",
        };
        write!(out, " {cell} |")?;
    }
    Ok(())
}

fn write_footnote_step<D, W>(
    doc: &D,
    id: NodeId,
    prefix: &Prefix,
    out: &mut MarkdownWriter<W>,
    stack: &mut Vec<Step>,
) -> fmt::Result
where
    D: MarkdownExport,
    W: fmt::Write,
{
    let Some(label) = doc
        .extra(id)
        .footnote_label()
        .and_then(|label| doc.footnote_label(label))
    else {
        return Ok(());
    };
    let line = format!("[^{label}]:");
    let body: Vec<NodeId> = doc.children(id).collect();
    prefix.write_open(out)?;
    out.write_str(&line)?;

    let rest = prefix.indented(4);
    let mut steps = Vec::with_capacity(body.len() * 3);
    if let Some((first, more)) = body.split_first() {
        if doc.kind(*first) == Some(BlockKind::Paragraph) {
            out.write_char(' ')?;
            let text = phrasing_export(doc, *first);
            push_first_and_rest(out, &rest, text)?;
        } else {
            out.write_str("\n")?;
            steps.push(Step::Block {
                id: *first,
                prefix: rest.clone(),
            });
        }
        for next in more {
            steps.push(Step::Newline);
            steps.push(Step::BlankLine {
                prefix: rest.clone(),
            });
            steps.push(Step::Block {
                id: *next,
                prefix: rest.clone(),
            });
        }
    }
    push_rev(stack, steps);
    Ok(())
}
