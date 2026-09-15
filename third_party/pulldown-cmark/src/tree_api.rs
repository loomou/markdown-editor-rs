//! The parsed document tree: `Parsed` / `NodeRef` / `NodeKind` (feature `tree`).
//!
//! Relation to [`Parser`](crate::Parser): `Parser` tears its internal tree
//! apart into a one-shot event stream and takes the internally allocated
//! strings with it; `Parsed` runs all inline parsing up front at construction
//! and afterwards only lends out references, so the same tree can be walked
//! repeatedly. That is the precondition for "downstream stops re-parsing
//! every leaf".
//!
//! The two paths run side by side and never convert into each other: `Parsed`
//! implements no iterator, and `Parser` exposes no tree. Inline node spans
//! match the Ranges `OffsetIter` yields (block node spans include the
//! container continuation prefixes, same as the event layer).
//!
//! This module adds no new data structures: traversal follows the `child` /
//! `next` chain of `Tree<Item>`, content reads go through the `Index` impl on
//! `Allocations`; the tree is not copied and no parent array is built.

use crate::parse::{eager_parse, Allocations, BrokenLinkCallback, Item, ItemBody, RefDefs};
use crate::strings::CowStr;
use crate::tree::{Tree, TreeIndex};
use crate::{Alignment, BlockQuoteKind, HeadingLevel, LinkType, MetadataBlockKind, Options};
use core::ops::Range;

/// A fully parsed document tree. All inline constructs are resolved; it can
/// be walked read-only, as many times as needed.
///
/// Difference to [`Parser`](crate::Parser): `Parser` is a one-shot event
/// stream that takes the internally allocated strings with it; `Parsed` owns
/// the whole tree and only lends out references. The cost is that all inline
/// parsing runs at construction time instead of lazily on demand like
/// `Parser`.
pub struct Parsed<'input> {
    text: &'input str,
    tree: Tree<Item>,
    allocs: Allocations<'input>,
}

impl core::fmt::Debug for Parsed<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Parsed")
            .field("text_len", &self.text.len())
            .field("node_count", &self.node_count())
            .finish()
    }
}

impl<'input> Parsed<'input> {
    /// Parse the whole document: block structure plus all inlines.
    pub fn new(text: &'input str, options: Options) -> Self {
        Self::new_with_broken_link_callback(text, options, None::<crate::DefaultBrokenLinkCallback>)
    }

    /// The broken-link-callback variant, symmetric to
    /// `Parser::new_with_broken_link_callback`.
    pub fn new_with_broken_link_callback<F: BrokenLinkCallback<'input>>(
        text: &'input str,
        options: Options,
        broken_link_callback: Option<F>,
    ) -> Self {
        let (tree, allocs) = eager_parse(text, options, broken_link_callback);
        Parsed { text, tree, allocs }
    }

    /// Parse inline only: treat `text` as the content of a single paragraph.
    ///
    /// Meant for incremental editing: re-parsing the source of one phrasing
    /// leaf does not need to re-run block structure detection (the block
    /// structure lives in the caller's document model).
    ///
    /// **Implementation contract (important)**: this method does not really
    /// turn block syntax off (CommonMark block rules are not option-gated),
    /// it is a full parse. The caller must decide whether `text` really forms
    /// a single paragraph —
    /// `root().children().count() == 1` with the first child a `Paragraph` /
    /// `TightParagraph`; when that fails (`text` contains `# `, `- `, fences,
    /// or other block syntax) take the "swap the block" branch. What block
    /// syntax inside a leaf means is defined by the caller, not promised by
    /// this API.
    pub fn inline_only(text: &'input str, options: Options) -> Self {
        Self::new(text, options)
    }

    /// The exit check for [`inline_only`](Self::inline_only): whether the
    /// result is a single paragraph. false means `text` contains block syntax
    /// (`# `, `- `, fences, ...) or several root blocks, and the caller
    /// should take the "swap the block / display = source" branch (the
    /// reconcile table in `tree-api-07` §3).
    ///
    /// Empty `text` (a root without children) counts as true: the projection
    /// of a zero-content paragraph is empty, the same result the "swap the
    /// block" branch would produce, so the caller needs no extra case. A
    /// trailing run of reference definitions only also counts as true — the
    /// definitions are collected into `reference_definitions()` and take up
    /// no root children (the `with_definitions` re-parse scenario,
    /// `tree-api-07` §4).
    pub fn is_single_paragraph(&self) -> bool {
        let mut children = self.root().children();
        match children.next() {
            Some(node) => {
                matches!(node.kind(), NodeKind::Paragraph | NodeKind::TightParagraph)
                    && children.next().is_none()
            }
            None => true,
        }
    }

    /// The document root. `NodeKind::Root`; its children are the top-level
    /// blocks.
    pub fn root(&self) -> NodeRef<'_, 'input> {
        NodeRef {
            parsed: self,
            ix: None,
        }
    }

    /// The original input.
    pub fn text(&self) -> &'input str {
        self.text
    }

    /// Same semantics as `Parser::reference_definitions`.
    pub fn reference_definitions(&self) -> &RefDefs<'input> {
        &self.allocs.refdefs
    }

    /// Footnote definitions and their use counts (document-wide exact
    /// values once all inlines have resolved). Sorted by label, so the order
    /// is deterministic.
    pub fn footnote_definitions(&self) -> impl Iterator<Item = (&str, usize)> + '_ {
        let mut defs: Vec<_> = self
            .allocs
            .footdefs
            .0
            .iter()
            .map(|(label, def)| (label.as_ref(), def.use_count))
            .collect();
        defs.sort_unstable_by(|a, b| a.0.cmp(b.0));
        defs.into_iter()
    }

    /// Total node count (including the root and the `TightParagraph` nodes
    /// that stay invisible on the event layer). Handy for capacity
    /// estimates.
    pub fn node_count(&self) -> usize {
        self.tree.nodes().len()
    }

    fn node(&self, ix: TreeIndex) -> &crate::tree::Node<Item> {
        &self.tree.nodes()[ix.get()]
    }
}

/// A read-only handle to one node of the tree. `Copy`; pass it around
/// freely.
///
/// There is no parent lookup: the upstream tree carries no parent pointers,
/// and adding a parent array would cost 8 more bytes per node; a walk knows
/// the parent anyway.
#[derive(Copy, Clone, Debug)]
pub struct NodeRef<'a, 'input> {
    parsed: &'a Parsed<'input>,
    /// `None` = the root (tree index 0 is a dummy; `TreeIndex` is NonZero
    /// and cannot express it).
    ix: Option<TreeIndex>,
}

impl<'a, 'input> NodeRef<'a, 'input> {
    // ── structure ────────────────────────────────────────
    pub fn kind(&self) -> NodeKind<'a, 'input> {
        let Some(ix) = self.ix else {
            return NodeKind::Root;
        };
        let item = self.parsed.node(ix).item;
        let allocs = &self.parsed.allocs;
        match item.body {
            ItemBody::Root => NodeKind::Root,
            ItemBody::Paragraph => NodeKind::Paragraph,
            ItemBody::TightParagraph => NodeKind::TightParagraph,
            ItemBody::Rule => NodeKind::Rule,
            ItemBody::Heading(level, Some(hx)) => {
                let attrs = &allocs[hx];
                NodeKind::Heading(HeadingInfo {
                    level,
                    id: attrs.id.as_ref(),
                    classes: &attrs.classes,
                    attrs: &attrs.attrs,
                    setext: is_setext_span(self.parsed.text, item.start..item.end),
                })
            }
            ItemBody::Heading(level, None) => NodeKind::Heading(HeadingInfo {
                level,
                id: None,
                classes: &[],
                attrs: &[],
                setext: is_setext_span(self.parsed.text, item.start..item.end),
            }),
            ItemBody::FencedCodeBlock(cx) => {
                let (marker, len) = fence_style(self.parsed.text, item.start..item.end);
                NodeKind::CodeBlock(CodeBlockInfo::Fenced {
                    info: &allocs[cx],
                    marker,
                    len,
                })
            }
            ItemBody::IndentCodeBlock => NodeKind::CodeBlock(CodeBlockInfo::Indented),
            ItemBody::HtmlBlock => NodeKind::HtmlBlock,
            ItemBody::BlockQuote(kind) => NodeKind::BlockQuote(kind),
            ItemBody::List(tight, marker, start) => NodeKind::List(ListInfo {
                tight,
                marker,
                start,
                ordered: marker == b'.' || marker == b')',
            }),
            ItemBody::ListItem(indent) => NodeKind::ListItem(ListItemInfo { indent }),
            ItemBody::FootnoteDefinition(cx) => NodeKind::FootnoteDefinition(&allocs[cx]),
            ItemBody::MetadataBlock(kind) => NodeKind::MetadataBlock(kind),
            ItemBody::DefinitionList(tight) => NodeKind::DefinitionList { tight },
            ItemBody::DefinitionListTitle => NodeKind::DefinitionListTitle,
            ItemBody::DefinitionListDefinition(indent) => {
                NodeKind::DefinitionListDefinition { indent }
            }
            ItemBody::Table(ax) => NodeKind::Table(&allocs[ax]),
            ItemBody::TableHead => NodeKind::TableHead,
            ItemBody::TableRow => NodeKind::TableRow,
            ItemBody::TableCell => NodeKind::TableCell,
            ItemBody::Emphasis => NodeKind::Emphasis,
            ItemBody::Strong => NodeKind::Strong,
            ItemBody::Strikethrough => NodeKind::Strikethrough,
            ItemBody::Superscript => NodeKind::Superscript,
            ItemBody::Subscript => NodeKind::Subscript,
            ItemBody::Link(lx) => {
                let (link_type, dest_url, title, id) = &allocs[lx];
                NodeKind::Link(LinkInfo {
                    link_type: *link_type,
                    dest_url,
                    title,
                    id,
                })
            }
            ItemBody::Image(lx) => {
                let (link_type, dest_url, title, id) = &allocs[lx];
                NodeKind::Image(LinkInfo {
                    link_type: *link_type,
                    dest_url,
                    title,
                    id,
                })
            }
            ItemBody::Text { backslash_escaped } => NodeKind::Text { backslash_escaped },
            ItemBody::SynthesizeText(_) | ItemBody::OwnedInlineHtml(_) => NodeKind::TextOwned,
            ItemBody::SynthesizeChar(c) => NodeKind::SynthesizedChar(c),
            ItemBody::Code(cx) => NodeKind::Code(&allocs[cx]),
            ItemBody::Math(cx, display) => NodeKind::Math {
                content: &allocs[cx],
                display,
            },
            ItemBody::InlineHtml => NodeKind::InlineHtml,
            ItemBody::Html => NodeKind::Html,
            ItemBody::FootnoteReference(cx) => NodeKind::FootnoteReference(&allocs[cx]),
            ItemBody::TaskListMarker(checked) => NodeKind::TaskMarker { checked },
            ItemBody::SoftBreak => NodeKind::SoftBreak,
            ItemBody::HardBreak(backslash) => NodeKind::HardBreak { backslash },
            body => unreachable!("unresolved inline body in Parsed: {body:?}"),
        }
    }

    pub fn children(&self) -> Children<'a, 'input> {
        let next = match self.ix {
            None => self.parsed.tree.first_index(),
            Some(ix) => self.parsed.node(ix).child,
        };
        Children {
            parsed: self.parsed,
            next,
        }
    }

    pub fn first_child(&self) -> Option<Self> {
        self.children().next()
    }

    pub fn next_sibling(&self) -> Option<Self> {
        let ix = self.ix?;
        self.parsed.node(ix).next.map(|next| NodeRef {
            parsed: self.parsed,
            ix: Some(next),
        })
    }

    // ── position ─────────────────────────────────────────
    /// The byte range in the source text. **Block nodes include the
    /// container continuation prefixes** (`> `, list indentation).
    pub fn span(&self) -> Range<usize> {
        match self.ix {
            None => 0..self.parsed.text.len(),
            Some(ix) => {
                let item = self.parsed.node(ix).item;
                item.start..item.end
            }
        }
    }

    // ── content ──────────────────────────────────────────
    /// The content of a text node: a slice of the input (`NodeKind::Text`)
    /// or the decoded result borrowed from the arena (`NodeKind::TextOwned`);
    /// non-text nodes return `None`.
    pub fn text(&self) -> Option<&'a str> {
        let ix = self.ix?;
        let item = self.parsed.node(ix).item;
        match item.body {
            ItemBody::Text { .. } | ItemBody::Html | ItemBody::InlineHtml => {
                self.parsed.text.get(item.start..item.end)
            }
            ItemBody::SynthesizeText(cx) | ItemBody::OwnedInlineHtml(cx) => {
                Some(self.parsed.allocs[cx].as_ref())
            }
            _ => None,
        }
    }
}

/// Iterator over the `child` / `next` chain; no allocations.
#[derive(Clone, Debug)]
pub struct Children<'a, 'input> {
    parsed: &'a Parsed<'input>,
    next: Option<TreeIndex>,
}

impl<'a, 'input> Iterator for Children<'a, 'input> {
    type Item = NodeRef<'a, 'input>;

    fn next(&mut self) -> Option<NodeRef<'a, 'input>> {
        let ix = self.next?;
        self.next = self.parsed.node(ix).next;
        Some(NodeRef {
            parsed: self.parsed,
            ix: Some(ix),
        })
    }
}

/// Node classification. All information hangs on the enum, so a consumer
/// can build its own nodes from a single match.
///
/// `Text` and `TextOwned` are split on purpose: for the former, `span()` and
/// content correspond one to one (usable directly as a source range); for
/// the latter the span is the source range but the content is the decoded
/// result (entities, smart quotes, synthesized characters), so the two
/// lengths differ.
#[derive(Clone, Debug)]
pub enum NodeKind<'a, 'input> {
    Root,

    // ── block containers ──────────────────────────────────
    BlockQuote(Option<BlockQuoteKind>),
    List(ListInfo),
    ListItem(ListItemInfo),
    FootnoteDefinition(&'a CowStr<'input>),
    Table(&'a [Alignment]),
    TableHead,
    TableRow,
    TableCell,
    /// Requires `Options::ENABLE_DEFINITION_LIST`. Never appears with the
    /// option off; covering it keeps consumers free of unreachable arms.
    DefinitionList {
        tight: bool,
    },
    DefinitionListTitle,
    DefinitionListDefinition {
        indent: usize,
    },

    // ── block leaves ──────────────────────────────────────
    Paragraph,
    /// A paragraph inside a tight list item. Never emitted on the event
    /// stream, but a real node when walking the tree: **its presence is the
    /// list tightness criterion**.
    TightParagraph,
    Heading(HeadingInfo<'a, 'input>),
    CodeBlock(CodeBlockInfo<'a, 'input>),
    HtmlBlock,
    MetadataBlock(MetadataBlockKind),
    Rule,

    // ── inline containers ─────────────────────────────────
    Emphasis,
    Strong,
    Strikethrough,
    Superscript,
    Subscript,
    Link(LinkInfo<'a, 'input>),
    Image(LinkInfo<'a, 'input>),

    // ── inline leaves ─────────────────────────────────────
    /// A slice borrowed from the input. `span()` and content correspond
    /// one to one (usable as source_range).
    Text {
        backslash_escaped: bool,
    },
    /// Text whose decoding differs from the source (synthesized strings
    /// from entities and smart quotes). `span()` is still the source range,
    /// but its length differs from the content length.
    TextOwned,
    /// A single synthesized character (smart quote etc.); the content is
    /// not in the arena, it is given as a `char`.
    SynthesizedChar(char),
    Code(&'a CowStr<'input>),
    Math {
        content: &'a CowStr<'input>,
        display: bool,
    },
    /// Inline HTML. Content goes through [`NodeRef::text`] (the
    /// `OwnedInlineHtml` content lives in the arena; the event layer
    /// likewise emits `Event::InlineHtml` for it, so it folds into this
    /// variant).
    InlineHtml,
    /// HTML text segments the inline pass produces besides inline HTML
    /// (`Event::Html` on the event layer).
    Html,
    FootnoteReference(&'a CowStr<'input>),
    TaskMarker {
        checked: bool,
    },
    SoftBreak,
    HardBreak {
        backslash: bool,
    },
}

#[derive(Copy, Clone, Debug)]
pub struct ListInfo {
    /// Upstream `ItemBody::List.0`. **Tight = item content carries no
    /// paragraph wrapper.**
    pub tight: bool,
    /// The marker byte: `-` `+` `*` `.` `)`. Upstream `ItemBody::List.1`.
    pub marker: u8,
    /// The start number of an ordered list; meaningful when `marker` is
    /// `.` or `)`.
    pub start: u64,
    /// Whether `marker` is `.` or `)`. A convenience, equivalent to
    /// checking the byte yourself.
    pub ordered: bool,
}

#[derive(Copy, Clone, Debug)]
pub struct ListItemInfo {
    /// Upstream `ItemBody::ListItem.0`: the content indent in columns
    /// relative to the item start.
    pub indent: usize,
}

#[derive(Clone, Debug)]
pub struct HeadingInfo<'a, 'input> {
    pub level: HeadingLevel,
    pub id: Option<&'a CowStr<'input>>,
    pub classes: &'a [CowStr<'input>],
    pub attrs: &'a [(CowStr<'input>, Option<CowStr<'input>>)],
    /// Setext form (`span()` includes the underline row). Detected as "the
    /// first non-whitespace byte of the span is not `#` (or the `#` is not
    /// followed by whitespace/end of line)", matching the ATX rule.
    pub setext: bool,
}

#[derive(Clone, Debug)]
pub enum CodeBlockInfo<'a, 'input> {
    Indented,
    Fenced {
        info: &'a CowStr<'input>,
        /// The fence character: `` ` `` or `~`. Scanned from the first
        /// line of `span()`; upstream does not store it.
        marker: u8,
        /// The number of opening fence characters (>= 3).
        len: u16,
    },
}

#[derive(Clone, Debug)]
pub struct LinkInfo<'a, 'input> {
    pub link_type: LinkType,
    pub dest_url: &'a CowStr<'input>,
    pub title: &'a CowStr<'input>,
    pub id: &'a CowStr<'input>,
}

/// Fence character and length, scanned from the first line of `span()`.
/// Upstream `parse_fenced_code_block` discards them after use; the tree
/// only keeps the info string, so this re-scans.
fn fence_style(text: &str, span: Range<usize>) -> (u8, u16) {
    let line = text.get(span).unwrap_or("").lines().next().unwrap_or("");
    let line = line.trim_start_matches(' ');
    let marker = if line.starts_with('~') { b'~' } else { b'`' };
    let len = line.bytes().take_while(|b| *b == marker).count();
    (marker, len.clamp(3, u16::MAX as usize) as u16)
}

/// Setext detection: the first non-whitespace byte is not `#`, or it is
/// `#` but followed by neither whitespace nor end of line (not a valid ATX
/// opener; the text was lifted into a heading by the setext underline
/// row).
fn is_setext_span(text: &str, span: Range<usize>) -> bool {
    let line = text.get(span).unwrap_or("").lines().next().unwrap_or("");
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix('#') {
        rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t')
    } else {
        true
    }
}
