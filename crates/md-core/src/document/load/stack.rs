use super::builder::{Builder, Frame, FrameKind, new_frame};
use super::{intern_push, source_piece, strip_html};
use crate::block::{BlockKind, NodeExtra};
use crate::document::Document;
use crate::document::change::ChangeSet;
use crate::document::text::TextPiece;
use crate::inline::InlineMarks;
use std::ops::Range;

impl Builder {
    pub(super) fn push_container(&mut self, kind: BlockKind) {
        let id = self.alloc(kind);
        self.stack.push(new_frame(id, FrameKind::Container));
    }

    pub(super) fn push_leaf(&mut self, kind: BlockKind) {
        let id = self.alloc(kind);
        self.texts.init_leaf(id.text_id());
        if let Some(n) = self.arena.get_mut(id) {
            n.text = Some(id.text_id());
        }
        self.cover_floor = None;
        self.stack.push(new_frame(id, FrameKind::Leaf(kind)));
    }

    pub(super) fn push_html_block(&mut self) {
        let id = self.alloc(BlockKind::Paragraph);
        self.cover_floor = None;
        self.stack.push(new_frame(id, FrameKind::Html));
    }

    pub(super) fn push_raw_block(&mut self, kind: BlockKind) {
        let id = self.alloc(kind);
        self.cover_floor = None;
        self.stack.push(new_frame(id, FrameKind::Raw(kind)));
    }

    pub(super) fn note_plain_text(&mut self) {
        if !self.current_inline().marks.contains(InlineMarks::IMAGE) {
            self.image_only = false;
        }
    }

    pub(super) fn push_text(&mut self, source: &str, s: &str, range: Range<usize>) {
        self.push_span(source, s, range, true);
    }

    pub(super) fn push_span(&mut self, source: &str, s: &str, range: Range<usize>, note: bool) {
        if matches!(self.top().kind, FrameKind::Raw(_)) {
            return;
        }
        if matches!(self.top().kind, FrameKind::Html) {
            self.top_mut().html.push_str(s);
            return;
        }
        if note {
            self.note_plain_text();
        }
        self.ensure_text_sink();
        let id = self.top().id;
        let st = self.current_inline();
        let piece_and_span = source_piece(source, s, range);
        if let Some(leaf) = self.texts.get_mut(id.text_id()) {
            match piece_and_span {
                Some(r) => leaf.append(TextPiece::Source(r.clone()), s, Some(r), st.marks, st.link),
                None => {
                    let intern_range = intern_push(&mut self.intern, s);
                    leaf.append(TextPiece::Intern(intern_range), s, None, st.marks, st.link);
                }
            }
        }
    }

    pub(super) fn push_html(&mut self, s: &str) {
        if matches!(self.top().kind, FrameKind::Raw(_)) {
            return;
        }
        if matches!(self.top().kind, FrameKind::Html) {
            self.top_mut().html.push_str(s);
            return;
        }
        let stripped = strip_html(s);
        if !stripped.is_empty() {
            self.push_intern(&stripped);
        }
    }

    pub(super) fn push_intern(&mut self, s: &str) {
        if matches!(self.top().kind, FrameKind::Raw(_)) {
            return;
        }
        self.note_plain_text();
        self.ensure_text_sink();
        let id = self.top().id;
        let st = self.current_inline();
        let intern_range = intern_push(&mut self.intern, s);
        if let Some(leaf) = self.texts.get_mut(id.text_id()) {
            leaf.append(TextPiece::Intern(intern_range), s, None, st.marks, st.link);
        }
    }

    pub(super) fn ensure_text_sink(&mut self) {
        if !matches!(
            self.top().kind,
            FrameKind::Leaf(_) | FrameKind::Html | FrameKind::Raw(_)
        ) {
            self.push_leaf(BlockKind::Paragraph);
            self.implicit_para = true;
            self.image_only = true;
            self.image_count = 0;
        }
    }

    pub(super) fn close_implicit(&mut self, source: &str) {
        if self.implicit_para {
            self.implicit_para = false;
            if matches!(self.top().kind, FrameKind::Leaf(BlockKind::Paragraph)) {
                self.pop(source);
            }
        }
    }

    pub(super) fn top(&self) -> &Frame {
        self.stack.last().expect("stack")
    }

    pub(super) fn top_mut(&mut self) -> &mut Frame {
        self.stack.last_mut().expect("stack")
    }

    pub(super) fn pop(&mut self, source: &str) {
        let frame = self.stack.pop().expect("balanced tags");
        let Some(parent) = self.stack.last() else {
            self.stack.push(frame);
            return;
        };
        let parent_id = parent.id;
        match frame.kind {
            FrameKind::Root => {}
            FrameKind::Container => {
                self.arena.append_child(parent_id, frame.id);
            }
            FrameKind::Leaf(kind) => {
                let extra = self
                    .arena
                    .get(frame.id)
                    .map(|n| n.extra)
                    .unwrap_or(NodeExtra::None);
                let empty = self
                    .texts
                    .get(frame.id.text_id())
                    .map(|l| l.display().trim().is_empty())
                    .unwrap_or(true);
                let drop_empty = self.math_continuation;
                self.math_continuation = false;
                if drop_empty && kind == BlockKind::Paragraph && extra == NodeExtra::None && empty {
                    self.texts.clear_slot(frame.id.index);
                    self.arena.tombstone(frame.id);
                    return;
                }
                if matches!(
                    kind,
                    BlockKind::CodeBlock | BlockKind::Mermaid | BlockKind::Math
                ) && let Some(leaf) = self.texts.get_mut(frame.id.text_id())
                {
                    leaf.trim_trailing_newline();
                }
                self.assign_leaf_source(source, &frame, kind);
                self.arena.append_child(parent_id, frame.id);
            }
            FrameKind::Html => {
                let text = strip_html(&frame.html);
                self.texts.init_leaf(frame.id.text_id());
                if let Some(n) = self.arena.get_mut(frame.id) {
                    n.kind = BlockKind::Paragraph;
                    n.text = Some(frame.id.text_id());
                }
                if !text.is_empty()
                    && let Some(leaf) = self.texts.get_mut(frame.id.text_id())
                {
                    let intern_range = intern_push(&mut self.intern, &text);
                    leaf.append(
                        TextPiece::Intern(intern_range),
                        &text,
                        None,
                        InlineMarks::NONE,
                        None,
                    );
                }
                self.assign_leaf_source(source, &frame, BlockKind::Paragraph);
                self.arena.append_child(parent_id, frame.id);
            }
            FrameKind::Raw(kind) => {
                let start = frame
                    .source_ranges
                    .iter()
                    .map(|range| range.start)
                    .min()
                    .unwrap_or(0)
                    .min(source.len());
                let mut end = frame
                    .source_ranges
                    .iter()
                    .map(|range| range.end)
                    .max()
                    .unwrap_or(start)
                    .min(source.len())
                    .max(start);
                while source.as_bytes().get(end) == Some(&b'\n') {
                    end += 1;
                }
                let range = start..end;
                let raw = source.get(range.clone()).unwrap_or("");
                self.texts.init_leaf(frame.id.text_id());
                if let Some(node) = self.arena.get_mut(frame.id) {
                    node.kind = kind;
                    node.text = Some(frame.id.text_id());
                }
                if let Some(leaf) = self.texts.get_mut(frame.id.text_id()) {
                    let source_range = range.start as u32..range.end as u32;
                    leaf.append(
                        TextPiece::Source(source_range.clone()),
                        raw,
                        Some(source_range.clone()),
                        InlineMarks::NONE,
                        None,
                    );
                    leaf.source = crate::document::text::LeafSource::Span(source_range);
                }
                self.arena.append_child(parent_id, frame.id);
            }
        }
    }

    pub(super) fn finish(mut self, source: String, reference_definitions: Vec<String>) -> Document {
        self.close_implicit(&source);
        while self.stack.len() > 1 {
            self.pop(&source);
        }
        let root = self.stack.last().expect("root").id;
        if self.arena.get(root).and_then(|n| n.first_child).is_none() {
            self.push_leaf(BlockKind::Paragraph);
            self.pop(&source);
        }
        let root = self.stack.pop().expect("root").id;
        self.texts.shrink_runs_and_pieces();
        let mut document = Document {
            arena: self.arena,
            texts: self.texts,
            source: source.into(),
            intern: std::sync::Arc::new(self.intern),
            links: std::sync::Arc::new(self.links),
            langs: std::sync::Arc::new(self.langs),
            footnotes: std::sync::Arc::new(self.footnotes),
            reference_definitions: std::sync::Arc::new(reference_definitions),
            table_alignment_overflow: self.table_alignment_overflow,
            root,
            revision: 1,
            max_content_revision: 1,
            changes: ChangeSet::document_replaced(1),
            focus: None,
            block_edit: None,
        };
        document.populate_s2d_cache();
        document
    }
}
