use super::builder::{Builder, FrameKind};
use super::{Link, align_bits, code_fence_style, heading_level, list_marker, quote_alert};
use crate::block::{BlockKind, NodeExtra, TABLE_ALIGN_COLS, TableCellAlign, pack_alignments};
use crate::inline::InlineMarks;
use pulldown_cmark::{CodeBlockKind, Tag, TagEnd};
use std::ops::Range;

impl Builder {
    pub(super) fn start(&mut self, source: &str, tag: Tag<'_>, range: &Range<usize>) {
        match tag {
            Tag::Paragraph => {
                self.close_implicit(source);
                self.mark_enclosing_list_loose();
                self.push_leaf(BlockKind::Paragraph);
                self.image_only = true;
                self.image_count = 0;
            }
            Tag::Heading { level, .. } => {
                self.close_implicit(source);
                self.mark_list_loose_before(source, range.start);
                self.push_leaf(BlockKind::Heading(heading_level(level)));
                self.image_only = false;
            }
            Tag::BlockQuote(kind) => {
                self.close_implicit(source);
                self.mark_list_loose_before(source, range.start);
                self.push_container(BlockKind::BlockQuote);
                if let Some(kind) = kind {
                    self.set_extra(self.top().id, quote_alert(source, range, kind));
                }
            }
            Tag::CodeBlock(kind) => {
                self.close_implicit(source);
                self.mark_list_loose_before(source, range.start);
                let (info, indented) = match &kind {
                    CodeBlockKind::Fenced(info) => {
                        let info = info.to_string();
                        ((!info.is_empty()).then_some(info), false)
                    }
                    CodeBlockKind::Indented => (None, true),
                };
                let mermaid = info
                    .as_deref()
                    .and_then(|info| info.split_whitespace().next())
                    .is_some_and(|token| token.eq_ignore_ascii_case("mermaid"));
                self.push_leaf(if mermaid {
                    BlockKind::Mermaid
                } else {
                    BlockKind::CodeBlock
                });
                if indented {
                    self.set_extra(self.top().id, NodeExtra::IndentedCode);
                } else {
                    let lang = info.map(|info| self.push_lang(info));
                    let (marker, len) = code_fence_style(source, range);
                    self.set_extra(self.top().id, NodeExtra::CodeFence { lang, marker, len });
                }
                self.image_only = false;
            }
            Tag::HtmlBlock => {
                self.close_implicit(source);
                self.mark_list_loose_before(source, range.start);
                self.push_html_block();
                self.image_only = false;
            }
            Tag::List(start) => {
                self.close_implicit(source);

                self.mark_list_loose_before(source, range.start);
                self.push_container(BlockKind::List);
                self.list_item_pending = true;
                self.set_extra(
                    self.top().id,
                    NodeExtra::List {
                        start,
                        marker: list_marker(source, range, start.is_some()),
                        loose: false,
                        source_loose: false,
                    },
                );
            }
            Tag::Item => {
                self.close_implicit(source);

                if self.list_item_pending {
                    self.list_item_pending = false;
                } else {
                    self.mark_list_loose_before(source, range.start);
                }
                self.push_container(BlockKind::ListItem);
            }
            Tag::FootnoteDefinition(label) => {
                self.close_implicit(source);
                self.push_container(BlockKind::FootnoteDefinition);
                let label = self.push_footnote(label.to_string());
                self.set_extra(self.top().id, NodeExtra::FootnoteLabel { label });
            }

            Tag::DefinitionList | Tag::DefinitionListTitle | Tag::DefinitionListDefinition => {
                unreachable!("definition lists are off; see load()")
            }
            Tag::Table(aligns) => {
                self.close_implicit(source);
                self.mark_list_loose_before(source, range.start);
                self.push_container(BlockKind::Table);
                let bits: Vec<u8> = aligns.iter().map(align_bits).collect();
                let packed = pack_alignments(bits.iter().copied());
                self.table_aligns = packed;
                if bits.len() > TABLE_ALIGN_COLS {
                    self.table_alignment_overflow
                        .insert(self.top().id, bits.into());
                }
                self.set_extra(
                    self.top().id,
                    NodeExtra::Table {
                        alignments: packed,
                        source: Some((range.start as u32, range.end as u32)),
                    },
                );
            }
            Tag::TableHead => {
                self.close_implicit(source);
                self.push_container(BlockKind::TableRow);
                self.set_extra(self.top().id, NodeExtra::HeaderRow);
                self.header_row = true;
                self.table_col = 0;
            }
            Tag::TableRow => {
                self.close_implicit(source);
                self.push_container(BlockKind::TableRow);
                self.header_row = false;
                self.table_col = 0;
            }
            Tag::TableCell => {
                self.close_implicit(source);
                self.push_leaf(BlockKind::TableCell);
                let col = self.table_col;
                self.table_col += 1;
                let align = TableCellAlign::from(self.table_alignment_at(col as usize));
                self.set_extra(
                    self.top().id,
                    NodeExtra::Cell {
                        align,
                        header: self.header_row,
                    },
                );
                if self.header_row {
                    let st = self.current_inline();
                    self.push_inline(st.marks.union(InlineMarks::STRONG), st.link);
                }
            }
            Tag::MetadataBlock(_) => {
                self.close_implicit(source);
                self.mark_list_loose_before(source, range.start);
                self.push_raw_block(BlockKind::MetadataBlock);
            }
            Tag::Emphasis => {
                let st = self.current_inline();
                self.push_inline(st.marks.union(InlineMarks::EM), st.link);
            }
            Tag::Strong => {
                let st = self.current_inline();
                self.push_inline(st.marks.union(InlineMarks::STRONG), st.link);
            }
            Tag::Strikethrough => {
                let st = self.current_inline();
                self.push_inline(st.marks.union(InlineMarks::STRIKE), st.link);
            }
            Tag::Superscript => {
                let st = self.current_inline();
                self.push_inline(st.marks.union(InlineMarks::SUPER), st.link);
            }
            Tag::Subscript => {
                let st = self.current_inline();
                self.push_inline(st.marks.union(InlineMarks::SUB), st.link);
            }
            Tag::Link {
                dest_url, title, ..
            } => {
                let id = self.links.len() as u32;
                self.links.push(Link {
                    dest: dest_url.as_ref().to_string(),
                    title: title.as_ref().to_string(),
                });
                let st = self.current_inline();
                self.push_inline(st.marks, Some(id));
            }
            Tag::Image {
                dest_url, title, ..
            } => {
                if !matches!(self.top().kind, FrameKind::Leaf(_)) {
                    self.ensure_text_sink();
                }
                let empty = self
                    .texts
                    .get(self.top().id.text_id())
                    .is_none_or(|l| l.display().is_empty());
                if !empty {
                    self.image_only = false;
                }
                self.image_count = self.image_count.saturating_add(1);
                let id = self.links.len() as u32;
                self.links.push(Link {
                    dest: dest_url.as_ref().to_string(),
                    title: title.as_ref().to_string(),
                });
                self.pending_image_dest = Some(id);
                self.image_display_at = self
                    .texts
                    .get(self.top().id.text_id())
                    .map(|l| l.display().len())
                    .unwrap_or(0);
                let st = self.current_inline();
                self.push_inline(st.marks.union(InlineMarks::IMAGE), Some(id));
            }
        }
    }

    pub(super) fn end(&mut self, source: &str, end: TagEnd, range: &Range<usize>) {
        match end {
            TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Superscript
            | TagEnd::Subscript
            | TagEnd::Link => {
                self.pop_inline();
            }
            TagEnd::Image => {
                let now = self
                    .texts
                    .get(self.top().id.text_id())
                    .map(|l| l.display().len())
                    .unwrap_or(0);
                if now <= self.image_display_at {
                    self.push_intern("\u{FFFC}");
                }
                self.pop_inline();
            }
            TagEnd::TableCell => {
                if self.header_row {
                    self.pop_inline();
                }
                self.close_implicit(source);
                self.pop(source);
            }
            TagEnd::TableHead | TagEnd::TableRow => {
                self.header_row = false;
                self.close_implicit(source);
                self.pop(source);
            }
            TagEnd::Paragraph => {
                self.close_implicit(source);
                if self.image_only
                    && self.image_count == 1
                    && matches!(self.top().kind, FrameKind::Leaf(BlockKind::Paragraph))
                {
                    let id = self.top().id;
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
                                .trim_start_matches('\u{FFFC}')
                                .trim()
                                .to_string()
                        })
                        .unwrap_or_default();
                    if let Some(leaf) = self.texts.get_mut(id.text_id()) {
                        let snap = leaf.snapshot_mut();
                        snap.display.clear();
                        snap.runs.clear();
                        leaf.pieces.clear();
                    }
                    if !caption.is_empty() {
                        self.push_intern(&caption);
                    }
                    if let Some(n) = self.arena.get_mut(id) {
                        let source_range = standalone_image_source_range(source, range);
                        n.kind = BlockKind::Image;
                        n.extra = NodeExtra::Image {
                            dest,
                            source: Some(source_range),
                        };
                    }
                    if let FrameKind::Leaf(k) = &mut self.top_mut().kind {
                        *k = BlockKind::Image;
                    }
                }
                self.image_only = false;
                self.image_count = 0;
                self.pending_image_dest = None;
                self.pop(source);
            }
            _ => {
                self.close_implicit(source);
                self.pop(source);
            }
        }
    }
}

fn standalone_image_source_range(source: &str, range: &Range<usize>) -> (u32, u32) {
    let start = source
        .get(..range.start)
        .and_then(|prefix| prefix.rfind('\n').map(|newline| newline + 1))
        .unwrap_or(0);
    let end = range
        .end
        .checked_sub(1)
        .filter(|end| source.as_bytes().get(*end) == Some(&b'\n'))
        .unwrap_or(range.end);
    (start as u32, end as u32)
}
