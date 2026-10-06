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
    CloseBlock {
        host: bool,
        end_row: bool,
        gaps: Vec<usize>,
        trailing: usize,
    },
    CloseRoot {
        gaps: Vec<usize>,
    },
    CloseLeaf {
        allow_standalone: bool,
    },
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
    let blanks: Vec<Range<usize>> = parsed.blank_lines().map(|line| line.span).collect();
    let mut builder = Builder::new();
    walk_all(&mut builder, &source, parsed, &blanks);
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

fn gap_between_blanks(count: usize, quoted: bool) -> usize {
    if quoted {
        count.saturating_sub(1) / 2
    } else {
        count.saturating_sub(1)
    }
}

fn gap_edge_blanks(count: usize, quoted: bool) -> usize {
    if quoted { (count + 1) / 2 } else { count }
}

fn walk_all(builder: &mut Builder, source: &str, parsed: &Parsed<'_>, blanks: &[Range<usize>]) {
    let mut stack: Vec<Item<'_, '_>> = Vec::new();
    stack.push(Item::CloseRoot {
        gaps: root_gaps(blanks, source, &parsed.root()),
    });
    if let Some(first) = parsed.root().first_child() {
        stack.push(Item::Node(first, InlineCtx::default()));
    }
    while let Some(item) = stack.pop() {
        match item {
            Item::CloseBlock {
                host,
                end_row,
                gaps,
                trailing,
            } => {
                builder.close_gaps(source, &gaps, false);
                builder.close_trailing_blanks(source, trailing);
                builder.parents.pop();
                if host {
                    builder.hosts.pop();
                }
                if end_row {
                    builder.header_row = false;
                }
            }
            Item::CloseRoot { gaps } => {
                builder.close_gaps(source, &gaps, true);
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
                visit(builder, source, node, ctx, &mut stack, blanks);
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
    blanks: &[Range<usize>],
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
                gaps: Vec::new(),
                trailing: 0,
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
            let trailing = match innermost_trailing_item(source, &node) {
                Some(item) if item.span() == span => {
                    item_trailing_blanks(blanks, source, span, width as usize)
                }
                _ => 0,
            };
            let mut gaps = container_gaps(blanks, source, &node, false);
            if let Some(last) = gaps.last_mut() {
                *last = 0;
            }
            stack.push(Item::CloseBlock {
                host: true,
                end_row: false,
                gaps,
                trailing,
            });
            push_children(stack, node, ctx);
        }
        NodeKind::BlockQuote(kind) => {
            builder.mark_list_loose_before(source, span.start);
            let id = builder.alloc(BlockKind::BlockQuote);
            let alert = kind.is_some();
            if let Some(k) = kind {
                builder.set_extra(id, super::quote_alert(source, &span, k));
            }
            builder.attach(id);
            builder.parents.push(id);
            stack.push(Item::CloseBlock {
                host: false,
                end_row: false,
                gaps: container_gaps(blanks, source, &node, alert),
                trailing: 0,
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
                gaps: Vec::new(),
                trailing: 0,
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
                gaps: Vec::new(),
                trailing: 0,
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
                gaps: Vec::new(),
                trailing: 0,
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
            let allow_standalone = matches!(node.kind(), NodeKind::Paragraph);
            builder.leaf_allows_standalone = allow_standalone;
            builder.image_only = true;
            builder.image_count = 0;
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
            if let Some(extra) = super::front_matter_extra(source, &span) {
                builder.set_extra(id, extra);
            }
            builder.enter_leaf(id, BlockKind::MetadataBlock, LeafSink::Text, span);
            builder.image_only = false;
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
            if !builder.at_math_seam() {
                builder.push_intern("\n", ctx);
                builder.log_leaf(LeafLog::Break { span: span.clone() });
            }
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
        if !empty
            && builder
                .texts
                .get(tid)
                .is_some_and(|l| l.runs().iter().all(|r| r.marks.is_image()))
        {
            builder.image_only = true;
        }
    }
    let standalone = builder.leaf_allows_standalone;
    builder.leave_text_leaf(source, standalone);
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
    while floor < source.len() && matches!(bytes[floor], b' ' | b'\t' | b'\n' | b'\r') {
        floor += 1;
    }
    let id = builder.alloc(BlockKind::Paragraph);
    builder.enter_leaf(id, BlockKind::Paragraph, LeafSink::Text, range);
    builder.image_only = true;
    builder.image_count = 0;
    builder.math_extract_at = Some(floor);
    builder.math_continuation = true;
    builder.math_seam_lead = true;
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

fn hosts_a_caret(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::ListItem | BlockKind::BlockQuote | BlockKind::FootnoteDefinition
    )
}

fn blank_in_container(line: &str) -> bool {
    line.chars().all(|c| c == '>' || c == ' ' || c == '\t')
}

fn is_a_line(source: &str, span: &Range<usize>) -> bool {
    span.end > span.start && source.as_bytes()[span.end - 1] == b'\n'
}

fn count_blank(blanks: &[Range<usize>], source: &str, range: Range<usize>) -> usize {
    let start = blanks.partition_point(|span| span.start < range.start);
    let end = blanks.partition_point(|span| span.end <= range.end);
    blanks[start..end]
        .iter()
        .filter(|span| is_a_line(source, span))
        .count()
}

fn leading_blank_run(blanks: &[Range<usize>], source: &str, range: Range<usize>) -> usize {
    let mut at = range.start;
    let mut count = 0;
    for span in &blanks[blanks.partition_point(|span| span.start < range.start)..] {
        if span.start != at || span.end > range.end || !is_a_line(source, span) {
            break;
        }
        at = span.end;
        count += 1;
    }
    count
}

fn trailing_blank_run(blanks: &[Range<usize>], range: Range<usize>) -> (usize, usize) {
    let mut at = range.end;
    let mut count = 0;
    let head = blanks.partition_point(|span| span.end <= range.end);
    for span in blanks[..head].iter().rev() {
        if span.end != at || span.start < range.start {
            break;
        }
        if span.is_empty() {
            continue;
        }
        at = span.start;
        count += 1;
    }
    (at, count)
}

fn after_the_previous_line(source: &str, at: usize) -> usize {
    if !ends_with_newline(source, at) && source.as_bytes().get(at) == Some(&b'\n') {
        at + 1
    } else {
        at
    }
}

fn first_line_end(source: &str, at: usize) -> Option<usize> {
    let rest = source.get(at..)?;
    rest.find('\n').map(|offset| at + offset + 1)
}

fn blank_line_width(source: &str, span: &Range<usize>) -> Option<usize> {
    let line = source.get(span.clone())?;
    let line = line.strip_suffix('\n').unwrap_or(line);
    if !blank_in_container(line) {
        return None;
    }
    Some(line.len())
}

fn ends_with_a_list(node: &NodeRef<'_, '_>) -> bool {
    node.children()
        .last()
        .is_some_and(|last| matches!(last.kind(), NodeKind::List(_)))
}

fn innermost_trailing_item<'a, 'i>(
    source: &str,
    node: &NodeRef<'a, 'i>,
) -> Option<NodeRef<'a, 'i>> {
    let mut current = *node;
    loop {
        if current.next_sibling().is_some() {
            return None;
        }
        if matches!(current.kind(), NodeKind::List(_)) || ends_with_a_list(&current) {
            current = current.children().last()?;
            continue;
        }
        if !matches!(current.kind(), NodeKind::ListItem(_)) {
            return None;
        }
        if current.span().end != source.len() {
            return None;
        }
        return Some(current);
    }
}

fn item_trailing_blanks(
    blanks: &[Range<usize>],
    source: &str,
    range: Range<usize>,
    indent: usize,
) -> usize {
    let mut count = 0usize;
    let mut at = range.end;
    let head = blanks.partition_point(|span| span.end <= range.end);
    for span in blanks[..head].iter().rev() {
        if span.end != at || span.start < range.start {
            break;
        }
        match blank_line_width(source, span) {
            Some(width) if width >= indent => count += 1,
            _ => break,
        }
        at = span.start;
    }
    count
}

fn ends_with_newline(source: &str, end: usize) -> bool {
    end > 0 && source.as_bytes().get(end - 1) == Some(&b'\n')
}

fn content_end(source: &str, child: &NodeRef<'_, '_>) -> Option<usize> {
    if !matches!(
        child.kind(),
        NodeKind::List(_) | NodeKind::FootnoteDefinition(_)
    ) {
        return None;
    }
    let span = child.span();
    let region = source.get(span.clone())?;
    let mut at = span.start;
    let mut end = span.start;
    for line in region.split('\n') {
        let line_end = at + line.len();
        if !blank_in_container(line) {
            end = line_end;
        }
        at = line_end + 1;
    }
    Some(end)
}

fn container_gaps(
    blanks: &[Range<usize>],
    source: &str,
    node: &NodeRef<'_, '_>,
    alert: bool,
) -> Vec<usize> {
    gaps_for(blanks, source, node, node.span(), alert)
}

fn root_gaps(blanks: &[Range<usize>], source: &str, root: &NodeRef<'_, '_>) -> Vec<usize> {
    let span = root.span();
    let span = if span.end == source.len() {
        span
    } else {
        0..source.len()
    };
    gaps_for(blanks, source, root, span, false)
}

fn gaps_for(
    blanks: &[Range<usize>],
    source: &str,
    node: &NodeRef<'_, '_>,
    span: Range<usize>,
    alert: bool,
) -> Vec<usize> {
    let children: Vec<NodeRef<'_, '_>> = node.children().collect();
    let quoted = matches!(node.kind(), NodeKind::BlockQuote(..));
    if children.is_empty() {
        let count = count_blank(blanks, source, span);
        return vec![if quoted { (count + 1) / 2 } else { count }];
    }
    let kids: Vec<Range<usize>> = children.iter().map(|child| child.span()).collect();
    let mut gaps = Vec::with_capacity(kids.len() + 1);
    let head = if alert {
        first_line_end(source, span.start).unwrap_or(kids[0].start)
    } else {
        span.start
    };
    gaps.push(gap_edge_blanks(
        leading_blank_run(blanks, source, head..kids[0].start),
        quoted,
    ));
    for (index, pair) in kids.windows(2).enumerate() {
        let end = content_end(source, &children[index]).unwrap_or(pair[0].end);
        let start = after_the_previous_line(source, end);
        let count = leading_blank_run(blanks, source, start..pair[1].start);
        gaps.push(gap_between_blanks(count, quoted));
    }
    let last = kids[kids.len() - 1].end;
    let last_child = &children[children.len() - 1];
    let last = content_end(source, last_child).unwrap_or(last);
    let claimed = innermost_trailing_item(source, last_child)
        .map(|item| {
            let span = item.span();
            let indent = super::item_host_indent(source, span.start) as usize;
            item_trailing_blanks(blanks, source, span, indent)
        })
        .unwrap_or(0);
    let start = after_the_previous_line(source, last);
    let (trail_start, trailing) = trailing_blank_run(blanks, start..span.end);
    let trailing = if trail_start == start { trailing } else { 0 };
    gaps.push(gap_edge_blanks(trailing, quoted).saturating_sub(claimed));
    gaps
}

impl Builder {
    pub(super) fn close_gaps(&mut self, source: &str, gaps: &[usize], is_root: bool) {
        let Some(closed) = self.parents.last().copied() else {
            return;
        };
        let Some(node) = self.arena.get(closed) else {
            return;
        };
        let kids: Vec<crate::document::arena::NodeId> = self.arena.children(closed).collect();
        if kids.is_empty() {
            if !is_root && !hosts_a_caret(node.kind) {
                return;
            }
            let count = gaps.first().copied().unwrap_or(0);
            let count = if is_root { count } else { count.max(1) };
            let mut anchor = None;
            for _ in 0..count {
                anchor = Some(self.synthesize_blank_paragraph(source, closed, anchor));
            }
            return;
        }
        let mut anchor = None;
        for (index, kid) in kids.iter().enumerate() {
            let count = gaps.get(index).copied().unwrap_or(0);
            for _ in 0..count {
                anchor = Some(self.synthesize_blank_paragraph(source, closed, anchor));
            }
            anchor = Some(*kid);
        }
        let count = gaps.get(kids.len()).copied().unwrap_or(0);
        for _ in 0..count {
            anchor = Some(self.synthesize_blank_paragraph(source, closed, anchor));
        }
    }

    pub(super) fn close_trailing_blanks(&mut self, source: &str, count: usize) {
        if count == 0 {
            return;
        }
        let Some(closed) = self.parents.last().copied() else {
            return;
        };
        let mut anchor = self.arena.children(closed).last();
        for _ in 0..count {
            anchor = Some(self.synthesize_blank_paragraph(source, closed, anchor));
        }
    }

    fn synthesize_blank_paragraph(
        &mut self,
        source: &str,
        parent: crate::document::arena::NodeId,
        after: Option<crate::document::arena::NodeId>,
    ) -> crate::document::arena::NodeId {
        let id = self.alloc(BlockKind::Paragraph);
        self.enter_leaf(id, BlockKind::Paragraph, LeafSink::Text, 0..0);
        self.leave_text_leaf(source, false);
        self.arena.insert_after(parent, after, id);
        id
    }

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
        self.leaf_allows_standalone = false;
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
        self.math_seam_lead = false;
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
            BlockKind::CodeBlock | BlockKind::Mermaid | BlockKind::Math | BlockKind::MetadataBlock
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
        let image_span = leaf
            .log
            .iter()
            .rev()
            .find_map(|entry| match entry {
                LeafLog::ImageEnd { span } => Some(span.clone()),
                _ => None,
            })
            .unwrap_or_else(|| leaf.node_span.clone());
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
            let (lo, hi) = super::standalone_image_source_range(source, &image_span);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn blanks(pairs: &[(usize, usize)]) -> Vec<Range<usize>> {
        pairs.iter().map(|&(start, end)| start..end).collect()
    }

    const LINES: &str = "aa\n\nbbbbb\n";

    #[test]
    fn count_blank_counts_every_blank_line_in_the_range() {
        let list = blanks(&[(2, 3), (3, 4), (9, 10)]);
        assert_eq!(count_blank(&list, LINES, 0..10), 3);
        assert_eq!(count_blank(&list, LINES, 2..4), 2);
        assert_eq!(count_blank(&list, LINES, 4..9), 0);
        assert_eq!(count_blank(&list, LINES, 0..3), 1);
    }

    #[test]
    fn a_blank_line_that_the_file_never_terminates_is_not_a_line() {
        let list = blanks(&[(2, 3), (3, 6)]);
        assert_eq!(count_blank(&list, "a\n\n   ", 0..6), 1);
        assert_eq!(leading_blank_run(&list, "a\n\n   ", 2..6), 1);
    }

    #[test]
    fn a_leading_run_stops_at_the_first_non_blank_line() {
        let list = blanks(&[(0, 3), (3, 6), (9, 10)]);
        let source = "  \n  \n   \n";
        assert_eq!(leading_blank_run(&list, source, 0..10), 2);
        assert_eq!(leading_blank_run(&list, source, 3..10), 1);
        assert_eq!(leading_blank_run(&list, source, 6..10), 0);
    }

    #[test]
    fn a_leading_run_ignores_a_blank_line_that_is_not_at_the_edge() {
        let list = blanks(&[(9, 10)]);
        let source = "  \n  \n   \n";
        assert_eq!(leading_blank_run(&list, source, 0..10), 0);
        assert_eq!(leading_blank_run(&list, source, 9..10), 1);
    }

    #[test]
    fn a_trailing_run_stops_at_the_first_non_blank_line() {
        let list = blanks(&[(0, 1), (4, 7), (7, 10)]);
        assert_eq!(trailing_blank_run(&list, 0..10), (4, 2));
        assert_eq!(trailing_blank_run(&list, 7..10), (7, 1));
        assert_eq!(trailing_blank_run(&list, 0..7), (4, 1));
    }

    #[test]
    fn a_trailing_run_that_does_not_reach_the_content_is_ignored() {
        let list = blanks(&[(4, 5), (12, 13), (13, 14)]);
        assert_eq!(trailing_blank_run(&list, 4..14), (12, 2));
    }

    #[test]
    fn a_trailing_run_keeps_a_line_the_file_never_terminates() {
        let list = blanks(&[(2, 3), (3, 6)]);
        assert_eq!(trailing_blank_run(&list, 2..6), (2, 2));
    }

    #[test]
    fn a_blank_line_that_only_terminates_the_previous_line_is_stepped_over() {
        let source = "```\nx\n```\n\n\np\n";
        assert_eq!(after_the_previous_line(source, 9), 10);
        assert_eq!(after_the_previous_line(source, 10), 10);
        assert_eq!(after_the_previous_line(source, 12), 12);
    }

    #[test]
    fn a_terminated_line_is_not_stepped_over_twice() {
        assert_eq!(after_the_previous_line("a\n\nb\n", 2), 2);
        assert_eq!(after_the_previous_line("a\n\nb\n", 1), 2);
    }

    #[test]
    fn the_alert_marker_line_is_not_part_of_the_leading_run() {
        assert_eq!(first_line_end("> [!NOTE]\n> hi\n", 0), Some(10));
        assert_eq!(first_line_end("> [!NOTE]", 0), None);
        assert_eq!(first_line_end("> hi\n", 0), Some(5));
    }

    fn tag_of(node: &NodeRef<'_, '_>) -> &'static str {
        match node.kind() {
            NodeKind::BlockQuote(_) => "quote",
            NodeKind::ListItem(_) => "item",
            NodeKind::Paragraph => "para",
            NodeKind::CodeBlock(_) => "code",
            NodeKind::FootnoteDefinition(_) => "footnote",
            _ => "other",
        }
    }

    fn find<'a, 'i>(node: &NodeRef<'a, 'i>, tag: &str) -> Option<NodeRef<'a, 'i>> {
        if tag_of(node) == tag {
            return Some(*node);
        }
        node.children().find_map(|child| find(&child, tag))
    }

    fn gaps_of(source: &str, tag: Option<&str>) -> Vec<usize> {
        let normalized = super::super::normalize_markdown_source(source);
        let mut opts = crate::document::editor_options();
        opts.remove(Options::ENABLE_DEFINITION_LIST);
        let parsed = Parsed::new(&normalized, opts);
        let blanks: Vec<Range<usize>> = parsed.blank_lines().map(|line| line.span).collect();
        let root = parsed.root();
        match tag {
            None => root_gaps(&blanks, &normalized, &root),
            Some(tag) => {
                let node = find(&root, tag).expect("the tag must name a node in the tree");
                let alert = matches!(node.kind(), NodeKind::BlockQuote(Some(_)));
                container_gaps(&blanks, &normalized, &node, alert)
            }
        }
    }

    fn count_synthesized(total: &mut usize, gaps: &[usize], childless: bool, is_root: bool) {
        if childless {
            let count = gaps.first().copied().unwrap_or(0);
            *total += if is_root { count } else { count.max(1) };
            return;
        }
        *total += gaps.iter().sum::<usize>();
    }

    fn walk_containers(
        node: &NodeRef<'_, '_>,
        blanks: &[Range<usize>],
        source: &str,
        total: &mut usize,
    ) {
        for child in node.children() {
            let childless = child.first_child().is_none();
            match child.kind() {
                NodeKind::ListItem(_) => {
                    let mut gaps = container_gaps(blanks, source, &child, false);
                    if let Some(last) = gaps.last_mut() {
                        *last = 0;
                    }
                    count_synthesized(total, &gaps, childless, false);
                }
                NodeKind::BlockQuote(kind) => {
                    let gaps = container_gaps(blanks, source, &child, kind.is_some());
                    count_synthesized(total, &gaps, childless, false);
                }
                _ => {}
            }
            walk_containers(&child, blanks, source, total);
        }
    }

    fn synthesized(source: &str) -> usize {
        let normalized = super::super::normalize_markdown_source(source);
        let mut opts = crate::document::editor_options();
        opts.remove(Options::ENABLE_DEFINITION_LIST);
        let parsed = Parsed::new(&normalized, opts);
        let blanks: Vec<Range<usize>> = parsed.blank_lines().map(|line| line.span).collect();
        let root = parsed.root();
        let mut total = 0;
        let root_gaps = root_gaps(&blanks, &normalized, &root);
        count_synthesized(&mut total, &root_gaps, root.first_child().is_none(), true);
        walk_containers(&root, &blanks, &normalized, &mut total);
        total
    }

    fn blank_paragraphs(source: &str) -> usize {
        let doc = crate::document::load_markdown(source, crate::document::editor_options());
        doc.preorder()
            .into_iter()
            .filter(|&id| {
                doc.kind(id.index) == Some(BlockKind::Paragraph) && doc.display(id).is_empty()
            })
            .count()
    }

    #[test]
    fn every_container_counts_the_blank_lines_it_owns() {
        for (source, tag, want) in [
            ("a\n\n\nb\n", None, vec![0, 1, 0]),
            ("a\n\nb\n", None, vec![0, 0, 0]),
            ("> a\n>\n> \n>\n> b\n", Some("quote"), vec![0, 1, 0]),
            ("> a\n>\n> b\n", Some("quote"), vec![0, 0, 0]),
            ("- a\n  \n  \n  b\n", Some("item"), vec![0, 1, 0]),
            ("    code\n\n\npara\n", None, vec![0, 1, 0]),
            ("[^1]: x\n\n\nz\n", None, vec![0, 1, 0]),
            ("> [!NOTE]\n> \n>\n> hi\n", Some("quote"), vec![1, 0]),
        ] {
            assert_eq!(gaps_of(source, tag), want, "{source:?} {tag:?}");
        }
    }

    #[test]
    fn the_gaps_the_walk_hands_to_the_builder_add_up_to_the_blank_paragraphs() {
        for source in [
            "a\n\n\nb\n",
            "a\n\nb\n",
            "> a\n> \n> \n> b\n",
            "- a\n  \n  \n  b\n",
            "    code\n\n\npara\n",
            "[^1]: x\n\n\nz\n",
            "> [!NOTE]\n> \n> hi\n",
            "```\nx\n```\n\n\np\n",
            "para\n\n[a]: u\n\n\n",
            "a\n\n\n",
            "\n\n\n",
            "- p\n  \n  \n  p\n",
            "> \n> \n",
        ] {
            assert_eq!(synthesized(source), blank_paragraphs(source), "{source:?}");
        }
    }
}
