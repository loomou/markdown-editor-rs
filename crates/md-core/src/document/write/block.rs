use super::inline::{
    LeadingBlock, escape_leading_block_markers, paragraph_export, trim_end_newlines,
};
use super::{
    MarkdownExport, MarkdownWriter, Prefix, blank_line, push_first_and_rest, write_lazy_prefixed,
    write_prefixed,
};
use crate::block::{BlockKind, CodeFenceMarker, FrontMatterMarker, ListMarker, NodeExtra};
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
    HardBlank {
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
            Step::HardBlank { prefix } => {
                if !prefix.is_plain() {
                    prefix.write_open(out)?;
                }
                out.write_str("\n")?;
            }
            Step::Block { id, prefix } => write_block_step(doc, id, &prefix, out, &mut stack)?,
            Step::Flow { kids, prefix } => write_flow_step(doc, kids, &prefix, out, &mut stack)?,
            Step::List {
                items,
                prefix,
                ordered,
                start,
                marker,
            } => write_list_step(items, &prefix, ordered, start, marker, &mut stack)?,
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
            let mut kids: Vec<NodeId> = doc.children(id).collect();
            let trailing_cursor_line = kids.last().is_some_and(|&last| {
                doc.extra(last) == NodeExtra::CursorLine && is_blank_paragraph(doc, last)
            });
            if trailing_cursor_line {
                kids.pop();
            }
            stack.push(Step::Flow {
                kids,
                prefix: prefix.clone(),
            });
            Ok(())
        }
        BlockKind::DocStart => Ok(()),
        BlockKind::Paragraph => {
            let body = paragraph_export(doc, id);
            if lazy_continuation(prefix, &body) {
                write_lazy_prefixed(out, prefix, &body)
            } else {
                write_prefixed(out, prefix, &body)
            }
        }
        BlockKind::Heading(n) => write_prefixed(out, prefix, &heading_line(doc, id, n)),
        BlockKind::MetadataBlock => write_front_matter(doc, id, out, prefix),
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
                    let kids: Vec<NodeId> = doc.children(id).collect();
                    let leading_blank = kids.first().is_some_and(|&k| is_blank_paragraph(doc, k));
                    if blank_after_marker && !leading_blank {
                        blank_line(out, &inner)?;
                    }
                    stack.push(Step::Flow {
                        kids,
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
            let start = extra.ordered_start().unwrap_or(1);
            let items: Vec<NodeId> = doc.children(id).collect();
            stack.push(Step::List {
                items,
                prefix: prefix.clone(),
                ordered,
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

pub(crate) fn is_blank_paragraph<D: MarkdownExport>(doc: &D, id: NodeId) -> bool {
    doc.kind(id) == Some(BlockKind::Paragraph)
        && matches!(doc.extra(id), NodeExtra::None | NodeExtra::CursorLine)
        && doc.display(id).is_empty()
        && doc.leaf_source(id).trim().is_empty()
}

pub(crate) fn needs_blank_between<D: MarkdownExport>(
    doc: &D,
    previous: NodeId,
    next: NodeId,
) -> bool {
    let previous_kind = doc.kind(previous);
    let next_kind = doc.kind(next);
    if previous_kind == Some(BlockKind::Image) || next_kind == Some(BlockKind::Image) {
        return true;
    }
    match previous_kind {
        Some(BlockKind::Paragraph) => match next_kind {
            Some(BlockKind::Paragraph) => true,
            Some(BlockKind::ThematicBreak) => doc.leaf_source(next).starts_with('-'),
            _ => false,
        },
        Some(BlockKind::BlockQuote) => matches!(
            next_kind,
            Some(BlockKind::Paragraph | BlockKind::BlockQuote | BlockKind::Table | BlockKind::Math)
        ),
        Some(BlockKind::List | BlockKind::Table) => matches!(
            next_kind,
            Some(BlockKind::Paragraph | BlockKind::Table | BlockKind::Math)
        ),
        _ => false,
    }
}

fn shares_the_marker_line<D: MarkdownExport>(doc: &D, id: NodeId, marker: &str) -> bool {
    match doc.kind(id) {
        Some(
            BlockKind::CodeBlock
            | BlockKind::Mermaid
            | BlockKind::Table
            | BlockKind::BlockQuote
            | BlockKind::List
            | BlockKind::Math
            | BlockKind::Image
            | BlockKind::FootnoteDefinition,
        ) => true,
        Some(BlockKind::ThematicBreak) => {
            let line = thematic_break_line(doc, id);
            !crate::document::syntax::is_thematic_break_line(&format!("{marker}{line}"))
        }
        _ => false,
    }
}

fn block_text<D>(doc: &D, id: NodeId) -> String
where
    D: MarkdownExport,
{
    let mut out = MarkdownWriter {
        inner: String::new(),
        written: true,
        nl_run: 1,
    };
    let _ = run_stack(
        doc,
        vec![Step::Block {
            id,
            prefix: Prefix::default(),
        }],
        &mut out,
    );
    out.inner
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
    let count = kids.len();
    let mut index = 0usize;
    let mut previous: Option<NodeId> = None;
    let mut previous_blank = false;
    while index < count {
        let id = kids[index];
        if !is_blank_paragraph(doc, id) {
            if previous.is_some() && !previous_blank {
                if glues_to_next(doc, previous) {
                    steps.push(Step::Newline);
                } else {
                    steps.push(Step::BlankLine {
                        prefix: prefix.clone(),
                    });
                }
            }
            steps.push(Step::Block {
                id,
                prefix: prefix.clone(),
            });
            previous = Some(id);
            previous_blank = false;
            index += 1;
            continue;
        }
        let start = index;
        while index < count && is_blank_paragraph(doc, kids[index]) {
            index += 1;
        }
        if previous.is_some() {
            steps.push(Step::Newline);
        }
        for &blank in &kids[start..index] {
            steps.push(Step::Block {
                id: blank,
                prefix: prefix.clone(),
            });
            steps.push(Step::Newline);
        }
        if start > 0 && index < count {
            steps.push(Step::HardBlank {
                prefix: prefix.clone(),
            });
        }
        previous = Some(kids[index - 1]);
        previous_blank = true;
    }
    push_rev(stack, steps);
    Ok(())
}

fn glues_to_next<D: MarkdownExport>(doc: &D, previous: Option<NodeId>) -> bool {
    let Some(id) = previous else {
        return false;
    };
    doc.kind(id) == Some(BlockKind::MetadataBlock) && !doc.extra(id).front_matter_blank_after()
}

fn write_list_step(
    items: Vec<NodeId>,
    prefix: &Prefix,
    ordered: bool,
    start: u64,
    marker: ListMarker,
    stack: &mut Vec<Step>,
) -> fmt::Result {
    let mut steps = Vec::with_capacity(items.len() * 2);
    let mut num = start;
    for (i, item) in items.iter().copied().enumerate() {
        if i > 0 {
            steps.push(Step::Newline);
        }
        steps.push(Step::Item {
            id: item,
            prefix: prefix.clone(),
            ordered,
            num,
            marker,
        });
        num = num.saturating_add(1).min(999_999_999);
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
            let body;
            let text = if first_kind == Some(BlockKind::Paragraph) {
                body = paragraph_export(doc, first);
                body.as_str()
            } else if let Some(BlockKind::Heading(n)) = first_kind {
                heading = heading_line(doc, first, n);
                heading.as_str()
            } else {
                ""
            };
            let lazy = first_kind == Some(BlockKind::Paragraph) && lazy_continuation(&rest, text);
            push_first_and_rest(out, &rest, text, lazy)?;
        }
        _ => {
            prefix.write_open(out)?;
            out.write_str(&marker)?;
            if task.is_none() && shares_the_marker_line(doc, first, &marker) {
                let text = block_text(doc, first);
                push_first_and_rest(out, &rest, &text, false)?;
            } else {
                out.write_str("\n")?;
                steps.push(Step::Block {
                    id: first,
                    prefix: rest.clone(),
                });
            }
        }
    }
    let loose = doc
        .parent(id)
        .is_some_and(|list| doc.extra(list).list_loose());
    let mut previous = first;
    let mut previous_blank = false;
    let mut index = 1usize;
    while index < kids.len() {
        let rest_id = kids[index];
        if !is_blank_paragraph(doc, rest_id) {
            if !previous_blank {
                steps.push(Step::Newline);
                if loose || needs_blank_between(doc, previous, rest_id) {
                    steps.push(Step::BlankLine {
                        prefix: rest.clone(),
                    });
                }
            }
            steps.push(Step::Block {
                id: rest_id,
                prefix: rest.clone(),
            });
            previous = rest_id;
            previous_blank = false;
            index += 1;
            continue;
        }
        let start = index;
        while index < kids.len() && is_blank_paragraph(doc, kids[index]) {
            index += 1;
        }
        if !previous_blank {
            steps.push(Step::Newline);
        }
        for &blank in &kids[start..index] {
            steps.push(Step::Block {
                id: blank,
                prefix: rest.clone(),
            });
            steps.push(Step::Newline);
        }
        if index < kids.len() {
            steps.push(Step::HardBlank {
                prefix: rest.clone(),
            });
        }
        previous = kids[index - 1];
        previous_blank = true;
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

fn lazy_continuation(prefix: &Prefix, body: &str) -> bool {
    !prefix.is_plain() && needs_a_lazy_continuation(body)
}

fn needs_a_lazy_continuation(body: &str) -> bool {
    if !body.contains('\n') {
        return false;
    }
    let lines: Vec<&str> = body.split('\n').collect();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 && is_setext_underline(line) {
            return true;
        }
        let heads_a_table = if index == 0 {
            has_unescaped_pipe(line)
        } else {
            line.starts_with('|')
        };
        if heads_a_table
            && lines
                .get(index + 1)
                .and_then(|next| table_delimiter_columns(next))
                .is_some_and(|columns| columns == table_cell_count(line))
        {
            return true;
        }
    }
    false
}

fn is_setext_underline(line: &str) -> bool {
    let line = line.trim_matches([' ', '\t']);
    let Some(first) = line.chars().next() else {
        return false;
    };
    matches!(first, '=' | '-')
        && line.chars().all(|c| c == first)
        && (first == '=' || line.chars().count() == 2)
}

fn has_unescaped_pipe(row: &str) -> bool {
    let mut escaped = false;
    for c in row.chars() {
        match c {
            '\\' => {
                escaped = true;
                continue;
            }
            '|' if !escaped => return true,
            _ => {}
        }
        escaped = false;
    }
    false
}

fn table_cell_count(row: &str) -> usize {
    table_cells(row.trim_matches([' ', '\t'])).len()
}

fn table_delimiter_columns(row: &str) -> Option<usize> {
    if row.len() - row.trim_start_matches(' ').len() > 3 {
        return None;
    }
    let row = row.trim_matches([' ', '\t']);
    let cells = table_cells(row);
    if cells.is_empty() || !row.contains('|') {
        return None;
    }
    cells
        .iter()
        .all(|cell| is_table_delimiter_cell(cell))
        .then_some(cells.len())
}

fn table_cells(row: &str) -> Vec<&str> {
    let mut cells: Vec<&str> = row.split('|').collect();
    if row.starts_with('|') {
        cells.remove(0);
    }
    if row.ends_with('|') {
        cells.pop();
    }
    cells
}

fn is_table_delimiter_cell(cell: &str) -> bool {
    let cell = cell.trim_matches([' ', '\t']);
    let dashes = cell.trim_start_matches(':').trim_end_matches(':');
    !dashes.is_empty()
        && dashes.chars().all(|c| c == '-')
        && cell.chars().all(|c| c == '-' || c == ':')
}

fn heading_line<D: MarkdownExport>(doc: &D, id: NodeId, level: u8) -> String {
    let source = trim_end_newlines(doc.leaf_source(id));
    if source.contains('\n') {
        return escape_leading_block_markers(source, LeadingBlock::Heading);
    }
    let hashes = source.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes)
        && (source.len() == hashes
            || matches!(source.as_bytes().get(hashes), Some(&b' ') | Some(&b'\t')))
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
    let marker_byte = marker as u8;
    let mut longest = 0usize;
    let mut run = 0usize;
    for &b in body.as_bytes() {
        if b == marker_byte {
            run += 1;
            if run > longest {
                longest = run;
            }
        } else {
            run = 0;
        }
    }
    let fence = marker.to_string().repeat(min_len.max(3).max(longest + 1));
    let open = if lang.is_empty() {
        fence.clone()
    } else {
        format!("{fence}{}", encode_fence_info(lang, marker))
    };
    write_prefixed(out, prefix, &open)?;
    out.write_str("\n")?;
    if !body.is_empty() {
        write_prefixed(out, prefix, body)?;
        out.write_str("\n")?;
    }
    write_prefixed(out, prefix, &fence)
}

fn encode_fence_info(info: &str, marker: char) -> String {
    let mut out = String::with_capacity(info.len());
    for ch in info.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '\\' => out.push_str("&#92;"),
            '`' if marker == '`' => out.push_str("&#96;"),
            '~' if marker == '~' => out.push_str("&#126;"),
            _ => out.push(ch),
        }
    }
    out
}

fn write_front_matter<D, W>(
    doc: &D,
    id: NodeId,
    out: &mut MarkdownWriter<W>,
    prefix: &Prefix,
) -> fmt::Result
where
    D: MarkdownExport,
    W: fmt::Write,
{
    let (marker, len) = doc
        .extra(id)
        .front_matter_fence()
        .unwrap_or((FrontMatterMarker::Dash, 3));
    let fence = (marker.byte() as char).to_string().repeat(len.max(3));
    let body = doc.display(id);
    write_prefixed(out, prefix, &format!("{fence}\n{body}\n{fence}"))
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
        let raw = doc.leaf_source(cell);
        let body = if raw.is_empty() {
            doc.display(cell)
        } else {
            raw
        };
        let text = encode_edge_spaces(&escape_table_pipes(body).replace('\n', "<br>"));
        out.write_str(&text)?;
        out.write_str(" |")?;
    }
    Ok(())
}

fn encode_edge_spaces(s: &str) -> String {
    let lead = s.len() - s.trim_start_matches(' ').len();
    let trail = (s.len() - s.trim_end_matches(' ').len()).min(s.len() - lead);
    if lead == 0 && trail == 0 {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 4 * (lead + trail));
    for _ in 0..lead {
        out.push_str("&#32;");
    }
    out.push_str(&s[lead..s.len() - trail]);
    for _ in 0..trail {
        out.push_str("&#32;");
    }
    out
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
            let text = paragraph_export(doc, *first);
            if !text.is_empty() {
                out.write_char(' ')?;
            }
            let lazy = lazy_continuation(&rest, &text);
            push_first_and_rest(out, &rest, &text, lazy)?;
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
