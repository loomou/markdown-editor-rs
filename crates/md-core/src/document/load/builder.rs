use super::leaf::LeafLog;
use super::walk::{LeafCtx, LeafSink};
use crate::block::{BlockKind, NodeExtra, alignment_at};
use crate::document::Document;
use crate::document::arena::{DocumentArena, NodeId};
use crate::document::change::ChangeSet;
use crate::document::focus::ConstructRecorder;
use crate::document::text::TextStore;
use crate::inline::InlineMarks;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use super::Link;

#[derive(Clone, Copy, Debug)]
pub(super) enum HostIndent {
    ContentColumn(u16),
    Relative(u16),
}

#[derive(Clone, Copy, Default)]
pub(super) struct InlineCtx {
    pub(super) marks: InlineMarks,
    pub(super) link: Option<u32>,
}

pub(super) struct Builder {
    pub(super) arena: DocumentArena,
    pub(super) texts: TextStore,
    pub(super) intern: String,
    pub(super) parents: Vec<NodeId>,
    pub(super) hosts: Vec<HostIndent>,
    pub(super) current_leaf: Option<LeafCtx>,
    pub(super) recorder: ConstructRecorder,
    pub(super) links: Vec<Link>,
    pub(super) langs: Vec<String>,
    pub(super) footnotes: Vec<String>,
    pub(super) image_only: bool,
    pub(super) image_count: u32,
    pub(super) pending_image_dest: Option<u32>,
    pub(super) table_aligns: u64,
    pub(super) table_alignment_overflow: HashMap<NodeId, Arc<[u8]>>,
    pub(super) table_col: u32,
    pub(super) header_row: bool,
    pub(super) math_continuation: bool,
    pub(super) math_extract_at: Option<usize>,
    pub(super) cover_floor: Option<usize>,
    pub(super) in_image: u32,
}

impl Builder {
    pub(super) fn new() -> Self {
        let mut arena = DocumentArena::new();
        let mut texts = TextStore::new();
        let root = arena.alloc(BlockKind::DocRoot);
        texts.push_slot();
        Builder {
            arena,
            texts,
            intern: String::new(),
            parents: vec![root],
            hosts: Vec::new(),
            current_leaf: None,
            recorder: ConstructRecorder::new(),
            links: Vec::new(),
            langs: Vec::new(),
            footnotes: Vec::new(),
            image_only: false,
            image_count: 0,
            pending_image_dest: None,
            table_aligns: 0,
            table_alignment_overflow: HashMap::new(),
            table_col: 0,
            header_row: false,
            math_continuation: false,
            math_extract_at: None,
            cover_floor: None,
            in_image: 0,
        }
    }

    pub(super) fn alloc(&mut self, kind: BlockKind) -> NodeId {
        let id = self.arena.alloc(kind);
        self.texts.push_slot();
        id
    }

    pub(super) fn attach(&mut self, id: NodeId) {
        let parent = *self.parents.last().expect("parent");
        self.arena.append_child(parent, id);
    }

    pub(super) fn push_lang(&mut self, token: String) -> u32 {
        let id = self.langs.len() as u32;
        self.langs.push(token);
        id
    }

    pub(super) fn push_footnote(&mut self, label: String) -> u32 {
        let id = self.footnotes.len() as u32;
        self.footnotes.push(label);
        id
    }

    pub(super) fn set_extra(&mut self, id: NodeId, extra: NodeExtra) {
        if let Some(n) = self.arena.get_mut(id) {
            n.extra = extra;
        }
    }

    pub(super) fn table_alignment_at(&self, col: usize) -> u8 {
        let table = self.parents.iter().rev().find_map(|id| {
            self.arena
                .get(*id)
                .filter(|n| n.kind == BlockKind::Table)
                .map(|_| *id)
        });
        let Some(table) = table else {
            return 0;
        };
        if let Some(bits) = self.table_alignment_overflow.get(&table) {
            return bits.get(col).copied().unwrap_or(0);
        }
        match self.arena.get(table).map(|node| node.extra) {
            Some(NodeExtra::Table { alignments, .. }) => alignment_at(alignments, col),
            _ => 0,
        }
    }

    pub(super) fn push_text(&mut self, source: &str, s: &str, range: Range<usize>, ctx: InlineCtx) {
        self.push_span(source, s, range, ctx, true);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn push_span(
        &mut self,
        source: &str,
        s: &str,
        range: Range<usize>,
        ctx: InlineCtx,
        note: bool,
    ) {
        let Some(leaf) = self.current_leaf.as_ref() else {
            return;
        };
        if leaf.raw() {
            return;
        }
        if leaf.is_html() {
            if let Some(leaf) = self.current_leaf.as_mut() {
                leaf.html.push_str(s);
            }
            return;
        }
        let id = leaf.id;
        if note {
            self.note_plain_text();
        }
        let piece_and_span = super::source_piece(source, s, range);
        if let Some(l) = self.texts.get_mut(id.text_id()) {
            match piece_and_span {
                Some(r) => l.append(
                    crate::document::text::TextPiece::Source(r.clone()),
                    s,
                    Some(r),
                    ctx.marks,
                    ctx.link,
                ),
                None => {
                    let intern_range = super::intern_push(&mut self.intern, s);
                    l.append(
                        crate::document::text::TextPiece::Intern(intern_range),
                        s,
                        None,
                        ctx.marks,
                        ctx.link,
                    );
                }
            }
        }
    }

    pub(super) fn push_intern(&mut self, s: &str, ctx: InlineCtx) {
        let Some(leaf) = self.current_leaf.as_ref() else {
            return;
        };
        if leaf.raw() {
            return;
        }
        if leaf.is_html() {
            if let Some(leaf) = self.current_leaf.as_mut() {
                leaf.html.push_str(s);
            }
            return;
        }
        let id = leaf.id;
        self.note_plain_text();
        let intern_range = super::intern_push(&mut self.intern, s);
        if let Some(l) = self.texts.get_mut(id.text_id()) {
            l.append(
                crate::document::text::TextPiece::Intern(intern_range),
                s,
                None,
                ctx.marks,
                ctx.link,
            );
        }
    }

    pub(super) fn push_html(&mut self, s: &str, ctx: InlineCtx) {
        let Some(leaf) = self.current_leaf.as_mut() else {
            return;
        };
        if leaf.raw() {
            return;
        }
        if leaf.is_html() {
            leaf.html.push_str(s);
            return;
        }
        let stripped = super::strip_html(s);
        if !stripped.is_empty() {
            self.push_intern(&stripped, ctx);
        }
    }

    pub(super) fn note_plain_text(&mut self) {
        if self.in_image == 0 {
            self.image_only = false;
        }
    }

    pub(super) fn leaf_disp(&self) -> usize {
        self.current_leaf
            .as_ref()
            .map(|l| {
                self.texts
                    .get(l.id.text_id())
                    .map(|t| t.display().len())
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    }

    pub(super) fn log_leaf(&mut self, entry: LeafLog) {
        if let Some(leaf) = self.current_leaf.as_mut() {
            leaf.log.push(entry);
        }
    }

    pub(super) fn log_shown(&mut self, span: Range<usize>, disp_before: usize) {
        let len = self.leaf_disp().saturating_sub(disp_before);
        self.log_leaf(LeafLog::Shown { span, len });
    }

    pub(super) fn log_shown_nonempty(&mut self, span: Range<usize>, disp_before: usize) {
        let len = self.leaf_disp().saturating_sub(disp_before);
        if len > 0 {
            self.log_leaf(LeafLog::Shown { span, len });
        }
    }

    pub(super) fn finish(mut self, source: String, reference_definitions: Vec<String>) -> Document {
        let root = self.parents[0];
        if self.arena.get(root).and_then(|n| n.first_child).is_none() {
            let id = self.alloc(BlockKind::Paragraph);
            self.enter_leaf(id, BlockKind::Paragraph, LeafSink::Text, 0..0);
            self.leave_text_leaf(&source, false);
        }
        self.texts.shrink_runs_and_pieces();
        Document {
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
        }
    }

    pub(super) fn merge_image_runs(
        &mut self,
        at: usize,
        now: usize,
        marks: InlineMarks,
        link: Option<u32>,
    ) {
        let Some(leaf) = self.current_leaf.as_ref() else {
            return;
        };
        let Some(l) = self.texts.get_mut(leaf.id.text_id()) else {
            return;
        };
        let snap = l.snapshot_mut();
        let in_range = |r: &crate::inline::InlineRun| {
            r.marks.is_image()
                && (r.display_range.start as usize) >= at
                && (r.display_range.end as usize) <= now
        };
        let Some(first) = snap.runs.iter().position(&in_range) else {
            return;
        };
        let Some(last) = snap.runs.iter().rposition(&in_range) else {
            return;
        };
        if last == first {
            return;
        }
        let display = snap.runs[first].display_range.start..snap.runs[last].display_range.end;
        snap.runs.splice(
            first..=last,
            std::iter::once(crate::inline::InlineRun {
                display_range: display,
                source_range: None,
                marks,
                link,
            }),
        );
    }
}
