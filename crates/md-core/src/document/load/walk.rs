use super::builder::{Builder, HostIndent, InlineCtx};
use super::leaf::LeafLog;
use crate::block::{BlockKind, NodeExtra};
use crate::document::Document;
use crate::document::focus::RawConstruct;
use crate::inline::InlineMarks;
use pulldown_cmark::{NodeKind, NodeRef, Options, Parsed};
use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LeafSink {
    Text,
    Html,
    Raw,
}

pub(super) struct LeafCtx {
    pub(super) id: crate::document::arena::NodeId,
    pub(super) kind: BlockKind,
    pub(super) node_span: Range<usize>,
    pub(super) sink: LeafSink,
    pub(super) html: String,
    pub(super) source_ranges: Vec<Range<usize>>,
    pub(super) constructs: Vec<RawConstruct>,
    pub(super) log: Vec<LeafLog>,
}

impl LeafCtx {
    pub(super) fn raw(&self) -> bool {
        self.sink == LeafSink::Raw
    }

    pub(super) fn is_html(&self) -> bool {
        self.sink == LeafSink::Html
    }
}

enum Item<'a, 'input> {
    Node(NodeRef<'a, 'input>, InlineCtx),
    CloseBlock { host: bool, end_row: bool },
    CloseLeaf { allow_standalone: bool },
    Inline(InlineEnd),
}

struct InlineEnd {
    kind: InlineEndKind,
    span: Range<usize>,
    marks: InlineMarks,
    link: Option<u32>,
    image_display_at: usize,
}

#[derive(Clone, Copy)]
enum InlineEndKind {
    Emphasis,
    Strong,
    Strikethrough,
    Link,
    Image,
    Superscript,
    Subscript,
}

impl InlineEndKind {
    fn tag(self) -> crate::document::focus::ConstructTag {
        use crate::document::focus::ConstructTag;
        match self {
            InlineEndKind::Emphasis => ConstructTag::Emphasis,
            InlineEndKind::Strong => ConstructTag::Strong,
            InlineEndKind::Strikethrough => ConstructTag::Strike,
            InlineEndKind::Link => ConstructTag::Link,
            InlineEndKind::Image => ConstructTag::Image,
            InlineEndKind::Superscript | InlineEndKind::Subscript => {
                unreachable!("sup/sub never tags")
            }
        }
    }
}

pub(super) fn load_via_tree(md: &str, mut opts: Options) -> Document {
    opts.remove(Options::ENABLE_DEFINITION_LIST);
    let source = super::normalize_markdown_source(md);
    crate::document::metrics::note_parser();
    let parsed = Parsed::new(&source, opts);
    document_of(&parsed, source.clone())
}

pub(super) fn document_of(parsed: &Parsed<'_>, source: String) -> Document {
    let mut builder = Builder::new();
    walk_all(&mut builder, &source, parsed);
    let mut spans: Vec<_> = parsed
        .reference_definitions()
        .iter()
        .map(|(_, definition)| definition.span.clone())
        .collect();
    spans.sort_unstable_by_key(|span| (span.start, span.end));
    spans.dedup();
    let reference_definitions = spans
        .into_iter()
        .filter_map(|span| source.get(span).map(str::to_string))
        .collect();
    builder.finish(source, reference_definitions)
}

fn walk_all(builder: &mut Builder, source: &str, parsed: &Parsed<'_>) {
    let mut stack: Vec<Item<'_, '_>> = Vec::new();
    if let Some(first) = parsed.root().first_child() {
        stack.push(Item::Node(first, InlineCtx::default()));
    }
    while let Some(item) = stack.pop() {
        match item {
            Item::CloseBlock { host, end_row } => {
                builder.parents.pop();
                if host {
                    builder.hosts.pop();
                }
                if end_row {
                    builder.header_row = false;
                }
            }
            Item::CloseLeaf { allow_standalone } => {
                builder.leave_text_leaf(source, allow_standalone);
            }
            Item::Inline(end) => {
                close_inline(builder, source, end);
            }
            Item::Node(node, ctx) => {
                if let Some(sibling) = node.next_sibling() {
                    stack.push(Item::Node(sibling, ctx));
                }
                visit(builder, source, node, ctx, &mut stack);
            }
        }
    }
}

fn visit<'a, 'i>(
    builder: &mut Builder,
    source: &str,
    node: NodeRef<'a, 'i>,
    ctx: InlineCtx,
    stack: &mut Vec<Item<'a, 'i>>,
) {
    let span = node.span();
    if matches!(
        node.kind(),
        NodeKind::List(_)
            | NodeKind::ListItem(_)
            | NodeKind::BlockQuote(_)
            | NodeKind::FootnoteDefinition(_)
            | NodeKind::Table(_)
            | NodeKind::TableHead
            | NodeKind::TableRow
            | NodeKind::TableCell
            | NodeKind::Paragraph
            | NodeKind::TightParagraph
            | NodeKind::Heading(_)
            | NodeKind::CodeBlock(_)
            | NodeKind::HtmlBlock
            | NodeKind::MetadataBlock(_)
            | NodeKind::Rule
    ) {
        builder.math_extract_at = None;
    }
    match node.kind() {
        NodeKind::List(info) => {
            builder.mark_list_loose_before(source, span.start);
            let id = builder.alloc(BlockKind::List);
            builder.set_extra(
                id,
                NodeExtra::List {
                    start: info.ordered.then_some(info.start),
                    marker: list_marker_of(source, &span, info),
                    loose: !info.tight,
                    source_loose: !info.tight,
                },
            );
            builder.attach(id);
            builder.parents.push(id);
            stack.push(Item::CloseBlock {
                host: false,
                end_row: false,
            });
            push_children(stack, node, ctx);
        }
        NodeKind::ListItem(_) => {
            let first = builder
                .parents
                .last()
                .is_some_and(|p| builder.arena.get(*p).map(|n| n.kind) == Some(BlockKind::List));
            if !first {
                builder.mark_list_loose_before(source, span.start);
            }
            let id = builder.alloc(BlockKind::ListItem);
            let width = super::item_host_indent(source, span.start);
            builder.hosts.push(HostIndent::ContentColumn(width));
            builder.attach(id);
            builder.parents.push(id);
            stack.push(Item::CloseBlock {
                host: true,
                end_row: false,
            });
            push_children(stack, node, ctx);
        }
        NodeKind::BlockQuote(kind) => {
            builder.mark_list_loose_before(source, span.start);
            let id = builder.alloc(BlockKind::BlockQuote);
            if let Some(k) = kind {
                builder.set_extra(id, super::quote_alert(source, &span, k));
            }
            builder.attach(id);
            builder.parents.push(id);
            stack.push(Item::CloseBlock {
                host: false,
                end_row: false,
            });
            push_children(stack, node, ctx);
        }
        NodeKind::FootnoteDefinition(label) => {
            let id = builder.alloc(BlockKind::FootnoteDefinition);
            builder.hosts.push(HostIndent::Relative(4));
            builder.attach(id);
            builder.parents.push(id);
            let label_id = builder.push_footnote(label.to_string());
            builder.set_extra(id, NodeExtra::FootnoteLabel { label: label_id });
            stack.push(Item::CloseBlock {
                host: true,
                end_row: false,
            });
            push_children(stack, node, ctx);
        }
        NodeKind::Table(aligns) => {
            builder.mark_list_loose_before(source, span.start);
            let id = builder.alloc(BlockKind::Table);
            let bits: Vec<u8> = aligns.iter().map(super::align_bits).collect();
            let packed = crate::block::pack_alignments(bits.iter().copied());
            builder.table_aligns = packed;
            if bits.len() > crate::block::TABLE_ALIGN_COLS {
                builder.table_alignment_overflow.insert(id, bits.into());
            }
            builder.set_extra(
                id,
                NodeExtra::Table {
                    alignments: packed,
                    source: Some((span.start as u32, span.end as u32)),
                },
            );
            builder.attach(id);
            builder.parents.push(id);
            stack.push(Item::CloseBlock {
                host: false,
                end_row: false,
            });
            push_children(stack, node, ctx);
        }
        NodeKind::TableHead | NodeKind::TableRow => {
            let id = builder.alloc(BlockKind::TableRow);
            let header = matches!(node.kind(), NodeKind::TableHead);
            if header {
                builder.set_extra(id, NodeExtra::HeaderRow);
            }
            builder.header_row = header;
            builder.table_col = 0;
            builder.attach(id);
            builder.parents.push(id);
            stack.push(Item::CloseBlock {
                host: false,
                end_row: true,
            });
            push_children(stack, node, ctx);
        }
        NodeKind::TableCell => {
            let id = builder.alloc(BlockKind::TableCell);
            let col = builder.table_col;
            builder.table_col += 1;
            let align =
                crate::block::TableCellAlign::from(builder.table_alignment_at(col as usize));
            builder.set_extra(
                id,
                NodeExtra::Cell {
                    align,
                    header: builder.header_row,
                },
            );
            builder.enter_leaf(id, BlockKind::TableCell, LeafSink::Text, span);
            stack.push(Item::CloseLeaf {
                allow_standalone: false,
            });
            let mut inner = ctx;
            if builder.header_row {
                inner.marks = inner.marks.union(InlineMarks::STRONG);
            }
            push_children(stack, node, inner);
        }
        NodeKind::Paragraph | NodeKind::TightParagraph => {
            if matches!(node.kind(), NodeKind::Paragraph) {
                builder.mark_enclosing_list_loose();
            }
            let id = builder.alloc(BlockKind::Paragraph);
            builder.enter_leaf(id, BlockKind::Paragraph, LeafSink::Text, span);
            builder.image_only = true;
            builder.image_count = 0;
            let allow_standalone = matches!(node.kind(), NodeKind::Paragraph);
            stack.push(Item::CloseLeaf { allow_standalone });
            push_children(stack, node, ctx);
        }
        NodeKind::Heading(info) => {
            builder.mark_list_loose_before(source, span.start);
            let level = super::heading_level(info.level);
            let id = builder.alloc(BlockKind::Heading(level));
            builder.enter_leaf(id, BlockKind::Heading(level), LeafSink::Text, span.clone());
            builder.image_only = false;
            builder.cover_source(source, span);
            stack.push(Item::CloseLeaf {
                allow_standalone: false,
            });
            push_children(stack, node, ctx);
        }
        NodeKind::CodeBlock(info) => {
            builder.mark_list_loose_before(source, span.start);
            let (fence_info, indented) = match info {
                pulldown_cmark::CodeBlockInfo::Fenced { info, .. } => {
                    ((!info.is_empty()).then(|| info.to_string()), false)
                }
                pulldown_cmark::CodeBlockInfo::Indented => (None, true),
            };
            let mermaid = fence_info
                .as_deref()
                .and_then(|info| info.split_whitespace().next())
                .is_some_and(|token| token.eq_ignore_ascii_case("mermaid"));
            let kind = if mermaid {
                BlockKind::Mermaid
            } else {
                BlockKind::CodeBlock
            };
            let id = builder.alloc(kind);
            builder.enter_leaf(id, kind, LeafSink::Text, span.clone());
            builder.image_only = false;
            if indented {
                builder.set_extra(id, NodeExtra::IndentedCode);
            } else {
                let lang = fence_info.map(|info| builder.push_lang(info));
                let (marker, len) = fence_style(source, &span);
                builder.set_extra(id, NodeExtra::CodeFence { lang, marker, len });
            }
            stack.push(Item::CloseLeaf {
                allow_standalone: false,
            });
            push_children(stack, node, ctx);
        }
        NodeKind::HtmlBlock => {
            builder.mark_list_loose_before(source, span.start);
            let id = builder.alloc(BlockKind::Paragraph);
            builder.enter_leaf(id, BlockKind::Paragraph, LeafSink::Html, span);
            builder.image_only = false;
            stack.push(Item::CloseLeaf {
                allow_standalone: false,
            });
            push_children(stack, node, ctx);
        }
        NodeKind::MetadataBlock(_) => {
            builder.mark_list_loose_before(source, span.start);
            let id = builder.alloc(BlockKind::MetadataBlock);
            builder.enter_leaf(id, BlockKind::MetadataBlock, LeafSink::Raw, span.clone());
            builder.image_only = false;
            builder.cover_source(source, span);
            stack.push(Item::CloseLeaf {
                allow_standalone: false,
            });
            push_children(stack, node, ctx);
        }
        NodeKind::Rule => {
            builder.mark_list_loose_before(source, span.start);
            let id = builder.alloc(BlockKind::ThematicBreak);
            builder.enter_leaf(id, BlockKind::ThematicBreak, LeafSink::Text, span.clone());
            builder.cover_source(source, span);
            builder.leave_text_leaf(source, false);
        }

        NodeKind::Emphasis | NodeKind::Strong | NodeKind::Strikethrough => {
            use crate::document::focus::ConstructTag;
            let (tag, bit) = match node.kind() {
                NodeKind::Emphasis => (ConstructTag::Emphasis, InlineMarks::EM),
                NodeKind::Strong => (ConstructTag::Strong, InlineMarks::STRONG),
                _ => (ConstructTag::Strike, InlineMarks::STRIKE),
            };
            let (lo, _) = clamp(source, &span);
            builder.recorder.start_tag(tag, lo);
            builder.cover_source(source, span.clone());
            let kind = match tag {
                ConstructTag::Emphasis => InlineEndKind::Emphasis,
                ConstructTag::Strong => InlineEndKind::Strong,
                _ => InlineEndKind::Strikethrough,
            };
            stack.push(Item::Inline(InlineEnd {
                kind,
                span,
                marks: ctx.marks,
                link: ctx.link,
                image_display_at: 0,
            }));
            let mut inner = ctx;
            inner.marks = inner.marks.union(bit);
            push_children(stack, node, inner);
        }
        NodeKind::Superscript | NodeKind::Subscript => {
            builder.cover_source(source, span.clone());
            stack.push(Item::Inline(InlineEnd {
                kind: if matches!(node.kind(), NodeKind::Superscript) {
                    InlineEndKind::Superscript
                } else {
                    InlineEndKind::Subscript
                },
                span,
                marks: ctx.marks,
                link: ctx.link,
                image_display_at: 0,
            }));
            let mut inner = ctx;
            inner.marks = inner.marks.union(match node.kind() {
                NodeKind::Superscript => InlineMarks::SUPER,
                _ => InlineMarks::SUB,
            });
            push_children(stack, node, inner);
        }
        NodeKind::Link(info) => {
            let id = builder.links.len() as u32;
            builder.links.push(super::Link {
                dest: info.dest_url.as_ref().to_string(),
                title: info.title.as_ref().to_string(),
            });
            let (lo, _) = clamp(source, &span);
            builder
                .recorder
                .start_tag(crate::document::focus::ConstructTag::Link, lo);
            builder.cover_source(source, span.clone());
            stack.push(Item::Inline(InlineEnd {
                kind: InlineEndKind::Link,
                span,
                marks: ctx.marks,
                link: ctx.link,
                image_display_at: 0,
            }));
            let mut inner = ctx;
            inner.link = Some(id);
            push_children(stack, node, inner);
        }
        NodeKind::Image(info) => {
            let id = builder.links.len() as u32;
            builder.links.push(super::Link {
                dest: info.dest_url.as_ref().to_string(),
                title: info.title.as_ref().to_string(),
            });
            builder.in_image += 1;
            let at = builder.leaf_disp();
            if at > 0 {
                builder.image_only = false;
            }
            builder.image_count = builder.image_count.saturating_add(1);
            builder.pending_image_dest = Some(id);
            builder.log_leaf(LeafLog::ImageStart);
            let (lo, _) = clamp(source, &span);
            builder
                .recorder
                .start_tag(crate::document::focus::ConstructTag::Image, lo);
            builder.cover_source(source, span.clone());
            stack.push(Item::Inline(InlineEnd {
                kind: InlineEndKind::Image,
                span,
                marks: ctx.marks.union(InlineMarks::IMAGE),
                link: Some(id),
                image_display_at: at,
            }));
            let mut inner = ctx;
            inner.marks = inner.marks.union(InlineMarks::IMAGE);
            inner.link = Some(id);
            push_children(stack, node, inner);
        }

        NodeKind::Text { .. } | NodeKind::TextOwned => {
            let text = node.text().unwrap_or("");
            let (lo, hi) = clamp(source, &span);
            builder.recorder.cover(lo, hi);
            let before = builder.leaf_disp();
            builder.push_text(source, text, span.clone(), ctx);
            builder.log_shown(span.clone(), before);
            builder.cover_source(source, span);
        }
        NodeKind::SynthesizedChar(c) => {
            let mut buf = [0u8; 4];
            let text = c.encode_utf8(&mut buf);
            let (lo, hi) = clamp(source, &span);
            builder.recorder.cover(lo, hi);
            let before = builder.leaf_disp();
            builder.push_text(source, text, span.clone(), ctx);
            builder.log_shown(span.clone(), before);
            builder.cover_source(source, span);
        }
        NodeKind::Code(content) => {
            let (lo, hi) = clamp(source, &span);
            let raw = builder.recorder.shown(source, lo..hi, content.as_ref());
            builder.image_only = false;
            let before = builder.leaf_disp();
            let mut inner = ctx;
            inner.marks = inner.marks.union(InlineMarks::CODE);
            builder.push_span(source, content.as_ref(), span.clone(), inner, true);
            builder.log_shown(span.clone(), before);
            builder.cover_verbatim(source, span);
            record_construct(builder, raw);
        }
        NodeKind::Math { content, display } => {
            if display {
                visit_display_math(builder, source, ctx, span, content.as_ref());
            } else {
                let (lo, hi) = clamp(source, &span);
                let raw = builder.recorder.shown(source, lo..hi, content.as_ref());
                builder.image_only = false;
                let before = builder.leaf_disp();
                let mut inner = ctx;
                inner.marks = inner.marks.union(InlineMarks::MATH_INLINE);
                builder.push_span(source, content.as_ref(), span.clone(), inner, false);
                builder.log_shown(span.clone(), before);
                builder.cover_verbatim(source, span);
                record_construct(builder, raw);
            }
        }
        NodeKind::InlineHtml => {
            let text = node.text().unwrap_or("");
            if builder
                .current_leaf
                .as_ref()
                .is_some_and(|l| l.kind == BlockKind::TableCell)
                && crate::document::bind::is_html_line_break(text)
            {
                builder.push_intern("\n", ctx);
                builder.log_leaf(LeafLog::TableBr { span: span.clone() });
            } else {
                let before = builder.leaf_disp();
                builder.push_html(text, ctx);
                builder.log_shown_nonempty(span.clone(), before);
            }
            builder.cover_source(source, span);
        }
        NodeKind::Html => {
            let text = node.text().unwrap_or("");
            if let Some(leaf) = builder.current_leaf.as_mut() {
                leaf.html.push_str(text);
            }
            builder.cover_source(source, span);
        }
        NodeKind::FootnoteReference(label) => {
            let mut inner = ctx;
            inner.marks = inner.marks.union(InlineMarks::FOOTNOTE);
            let shown = format!("[^{label}]");
            let before = builder.leaf_disp();
            builder.push_intern(&shown, inner);
            builder.log_shown(span.clone(), before);
            builder.cover_source(source, span);
        }
        NodeKind::TaskMarker { checked } => {
            builder.set_list_item_task(checked);
        }
        NodeKind::SoftBreak | NodeKind::HardBreak { .. } => {
            let (lo, hi) = clamp(source, &span);
            builder.recorder.cover(lo, hi);
            builder.push_intern("\n", ctx);
            builder.log_leaf(LeafLog::Break { span: span.clone() });
            builder.cover_source(source, span);
        }
        NodeKind::Root => {
            push_children(stack, node, ctx);
        }
        NodeKind::DefinitionList { .. }
        | NodeKind::DefinitionListTitle
        | NodeKind::DefinitionListDefinition { .. } => {
            unreachable!("definition lists are off; see load()")
        }
    }
}

fn push_children<'a, 'i>(stack: &mut Vec<Item<'a, 'i>>, node: NodeRef<'a, 'i>, ctx: InlineCtx) {
    if let Some(child) = node.first_child() {
        stack.push(Item::Node(child, ctx));
    }
}

fn clamp(source: &str, range: &Range<usize>) -> (usize, usize) {
    let lo = range.start.min(source.len());
    (lo, range.end.min(source.len()).max(lo))
}

fn record_construct(builder: &mut Builder, raw: RawConstruct) {
    if let Some(leaf) = builder.current_leaf.as_mut() {
        leaf.constructs.push(raw);
    }
}

fn close_inline(builder: &mut Builder, source: &str, end: InlineEnd) {
    builder.cover_source(source, end.span.clone());
    if matches!(
        end.kind,
        InlineEndKind::Superscript | InlineEndKind::Subscript
    ) {
        return;
    }
    let (_, hi) = clamp(source, &end.span);
    let raw = builder.recorder.end_tag(end.kind.tag(), source, hi);
    if let Some(raw) = raw {
        record_construct(builder, raw);
    }
    if matches!(end.kind, InlineEndKind::Image) {
        let now = builder.leaf_disp();
        if now <= end.image_display_at {
            let ctx = InlineCtx {
                marks: end.marks,
                link: end.link,
            };
            builder.push_intern(crate::document::bind::IMAGE_PLACEHOLDER, ctx);
        }
        builder.in_image = builder.in_image.saturating_sub(1);
        builder.log_leaf(LeafLog::ImageEnd {
            span: end.span.clone(),
        });
        builder.merge_image_runs(end.image_display_at, now, end.marks, end.link);
    }
}

fn visit_display_math(
    builder: &mut Builder,
    source: &str,
    ctx: InlineCtx,
    span: Range<usize>,
    content: &str,
) {
    builder.image_only = false;
    let latex = display_math_latex(content);
    let crossed = ctx.marks != InlineMarks::NONE || ctx.link.is_some();
    let in_paragraph = builder
        .current_leaf
        .as_ref()
        .is_some_and(|l| l.kind == BlockKind::Paragraph);
    if !crossed && in_paragraph {
        extract_display_math(builder, source, latex, span, display_math_fenced(content));
        return;
    }
    let (lo, hi) = clamp(source, &span);
    let raw = builder.recorder.shown(source, lo..hi, content);
    let before = builder.leaf_disp();
    let mut inner = ctx;
    inner.marks = inner.marks.union(InlineMarks::MATH_DISPLAY);
    builder.push_span(source, latex, span.clone(), inner, false);
    builder.log_shown(span.clone(), before);
    builder.cover_verbatim(source, span);
    record_construct(builder, raw);
}

fn extract_display_math(
    builder: &mut Builder,
    source: &str,
    latex: &str,
    range: Range<usize>,
    fenced: bool,
) {
    if let Some(leaf) = builder.current_leaf.as_mut() {
        for r in &mut leaf.source_ranges {
            if r.end >= range.start {
                r.end = range.start.min(r.end);
                while r.end > r.start
                    && matches!(
                        source.as_bytes().get(r.end - 1),
                        Some(b' ' | b'\t' | b'\n' | b'\r')
                    )
                {
                    r.end -= 1;
                }
            }
            if r.start > r.end {
                r.start = r.end;
            }
        }
        let tid = leaf.id.text_id();
        let pre = builder
            .texts
            .get(tid)
            .map(|l| l.display().len())
            .unwrap_or(0);
        if let Some(l) = builder.texts.get_mut(tid) {
            l.trim_trailing_whitespace();
        }
        let post = builder
            .texts
            .get(tid)
            .map(|l| l.display().len())
            .unwrap_or(0);
        let mut over = pre.saturating_sub(post);
        while over > 0 {
            match leaf.log.last_mut() {
                Some(LeafLog::Shown { len, .. }) if *len > 0 => {
                    if *len <= over {
                        over -= *len;
                        leaf.log.pop();
                    } else {
                        *len -= over;
                        over = 0;
                    }
                }
                Some(LeafLog::Break { .. }) | Some(LeafLog::TableBr { .. }) => {
                    over -= 1;
                    leaf.log.pop();
                }
                _ => break,
            }
        }
        let empty = builder
            .texts
            .get(tid)
            .map(|l| l.display().trim().is_empty())
            .unwrap_or(true);
        if empty {
            builder.math_continuation = true;
        }
    }
    builder.leave_text_leaf(source, false);
    let id = builder.alloc(BlockKind::Math);
    builder.enter_leaf(id, BlockKind::Math, LeafSink::Text, range.clone());
    if fenced {
        builder.set_extra(id, NodeExtra::MathFence);
    }
    let ctx = InlineCtx {
        marks: InlineMarks::MATH_DISPLAY,
        link: None,
    };
    let before = builder.leaf_disp();
    builder.push_span(source, latex, range.clone(), ctx, false);
    builder.log_shown(range.clone(), before);
    builder.cover_source(source, range.clone());
    builder.leave_text_leaf(source, false);
    let mut floor = range.end;
    let bytes = source.as_bytes();
    while floor < source.len() && matches!(bytes[floor], b' ' | b'\t') {
        floor += 1;
    }
    let id = builder.alloc(BlockKind::Paragraph);
    builder.enter_leaf(id, BlockKind::Paragraph, LeafSink::Text, range);
    builder.image_only = true;
    builder.image_count = 0;
    builder.math_extract_at = Some(floor);
    builder.math_continuation = true;
}

fn list_marker_of(
    source: &str,
    range: &Range<usize>,
    info: pulldown_cmark::ListInfo,
) -> crate::block::ListMarker {
    use crate::block::ListMarker;
    if info.ordered {
        match info.marker {
            b'.' => return ListMarker::Period,
            b')' => return ListMarker::Parenthesis,
            _ => return super::list_marker(source, range, true),
        }
    }
    match info.marker {
        b'+' => ListMarker::Plus,
        b'*' => ListMarker::Star,
        b'-' => ListMarker::Dash,
        _ => super::list_marker(source, range, false),
    }
}

fn fence_style(source: &str, span: &Range<usize>) -> (crate::block::CodeFenceMarker, u16) {
    super::code_fence_style(source, span)
}

fn display_math_latex(latex: &str) -> &str {
    latex
        .strip_prefix("\r\n")
        .or_else(|| latex.strip_prefix('\n'))
        .unwrap_or(latex)
}

fn display_math_fenced(t: &str) -> bool {
    t.starts_with('\n') || t.starts_with("\r\n")
}

impl Builder {
    pub(super) fn enter_leaf(
        &mut self,
        id: crate::document::arena::NodeId,
        kind: BlockKind,
        sink: LeafSink,
        node_span: Range<usize>,
    ) {
        self.texts.init_leaf(id.text_id());
        if let Some(n) = self.arena.get_mut(id) {
            n.text = Some(id.text_id());
        }
        self.cover_floor = None;
        self.current_leaf = Some(LeafCtx {
            id,
            kind,
            node_span,
            sink,
            html: String::new(),
            source_ranges: Vec::new(),
            constructs: Vec::new(),
            log: Vec::new(),
        });
    }

    pub(super) fn leave_text_leaf(&mut self, source: &str, allow_standalone: bool) {
        let Some(mut leaf) = self.current_leaf.take() else {
            return;
        };
        match leaf.sink {
            LeafSink::Html => {
                let parent = *self.parents.last().expect("parent");
                self.leave_html_leaf(source, &mut leaf);
                self.arena.append_child(parent, leaf.id);
                return;
            }
            LeafSink::Raw => {
                let parent = *self.parents.last().expect("parent");
                self.leave_raw_leaf(source, &mut leaf);
                self.arena.append_child(parent, leaf.id);
                return;
            }
            LeafSink::Text => {}
        }
        let kind = leaf.kind;
        let extra = self
            .arena
            .get(leaf.id)
            .map(|n| n.extra)
            .unwrap_or(NodeExtra::None);
        let empty = self
            .texts
            .get(leaf.id.text_id())
            .map(|l| l.display().trim().is_empty())
            .unwrap_or(true);
        let drop_empty = self.math_continuation;
        self.math_continuation = false;
        if drop_empty && kind == BlockKind::Paragraph && extra == NodeExtra::None && empty {
            self.texts.clear_slot(leaf.id.index);
            self.arena.tombstone(leaf.id);
            return;
        }
        if allow_standalone && self.image_only && self.image_count == 1 {
            self.standalone_image(source, &mut leaf, kind);
        }
        if matches!(
            kind,
            BlockKind::CodeBlock | BlockKind::Mermaid | BlockKind::Math
        ) && let Some(l) = self.texts.get_mut(leaf.id.text_id())
        {
            l.trim_trailing_newline();
        }
        if kind.is_text_leaf() {
            crate::document::metrics::note_leaf();
        }
        self.leave_leaf_ranges(source, &mut leaf);
        let parent = *self.parents.last().expect("parent");
        self.arena.append_child(parent, leaf.id);
        self.image_only = false;
        self.image_count = 0;
        self.pending_image_dest = None;
    }

    fn standalone_image(&mut self, source: &str, leaf: &mut LeafCtx, kind: BlockKind) {
        let id = leaf.id;
        let dest = self
            .texts
            .get(id.text_id())
            .and_then(|l| {
                l.runs()
                    .iter()
                    .find(|r| r.marks.is_image())
                    .and_then(|r| r.link)
            })
            .or(self.pending_image_dest)
            .unwrap_or(0);
        let caption = self
            .texts
            .get(id.text_id())
            .map(|l| {
                l.display()
                    .trim_start_matches(crate::document::bind::IMAGE_PLACEHOLDER)
                    .trim()
                    .to_string()
            })
            .unwrap_or_default();
        if let Some(l) = self.texts.get_mut(id.text_id()) {
            let snap = l.snapshot_mut();
            snap.display.clear();
            snap.runs.clear();
            l.pieces.clear();
        }
        if !caption.is_empty() {
            self.push_intern_owned(leaf, &caption);
        }
        let alt_span = leaf.log.iter().find_map(|entry| match entry {
            LeafLog::Shown { span, .. } => Some(span.clone()),
            _ => None,
        });
        leaf.log.clear();
        if let Some(span) = alt_span
            && !caption.is_empty()
        {
            leaf.log.push(LeafLog::Shown {
                span,
                len: caption.len(),
            });
        }
        if let Some(n) = self.arena.get_mut(id) {
            let (lo, hi) = super::standalone_image_source_range(source, &leaf.node_span);
            n.kind = BlockKind::Image;
            n.extra = NodeExtra::Image {
                dest,
                source: Some((lo, hi)),
            };
        }
        let _ = kind;
    }

    fn push_intern_owned(&mut self, leaf: &LeafCtx, s: &str) {
        let intern_range = super::intern_push(&mut self.intern, s);
        if let Some(l) = self.texts.get_mut(leaf.id.text_id()) {
            l.append(
                crate::document::text::TextPiece::Intern(intern_range),
                s,
                None,
                InlineMarks::NONE,
                None,
            );
        }
    }

    pub(super) fn mark_enclosing_list_loose(&mut self) {
        let list = self.parents.iter().rev().find_map(|frame| {
            match self.arena.get(*frame).map(|node| node.kind) {
                Some(BlockKind::List) => Some(Some(*frame)),
                Some(BlockKind::ListItem) => None,
                _ => Some(None),
            }
        });
        let Some(list) = list.flatten() else {
            return;
        };
        if let Some(n) = self.arena.get_mut(list)
            && let NodeExtra::List {
                loose,
                source_loose,
                ..
            } = &mut n.extra
        {
            *loose = true;
            *source_loose = true;
        }
    }

    pub(super) fn mark_list_loose_before(&mut self, source: &str, at: usize) {
        if blank_line_before(source, at) {
            self.mark_enclosing_list_loose();
        }
    }

    pub(super) fn set_list_item_task(&mut self, checked: bool) {
        let item = self.parents.iter().rev().find_map(|id| {
            self.arena
                .get(*id)
                .filter(|n| n.kind == BlockKind::ListItem)
                .map(|_| *id)
        });
        if let Some(id) = item {
            self.set_extra(id, NodeExtra::TaskItem { checked });
        }
    }
}

fn blank_line_before(source: &str, at: usize) -> bool {
    let mut newlines = 0usize;
    for &b in source.as_bytes()[..at].iter().rev() {
        match b {
            b'\n' => {
                newlines += 1;
                if newlines >= 2 {
                    return true;
                }
            }
            b' ' | b'\t' | b'\r' => {}
            _ => return false,
        }
    }
    false
}
