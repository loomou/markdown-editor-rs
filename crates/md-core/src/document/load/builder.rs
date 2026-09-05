use super::Link;
use crate::block::{BlockKind, NodeExtra, alignment_at};
use crate::document::arena::{DocumentArena, NodeId};
use crate::document::focus::{ConstructRecorder, RawConstruct};
use crate::document::text::TextStore;
use crate::inline::InlineMarks;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

pub(super) struct Frame {
    pub(super) id: NodeId,
    pub(super) kind: FrameKind,
    pub(super) html: String,
    pub(super) source_ranges: Vec<Range<usize>>,

    pub(super) constructs: Vec<RawConstruct>,
}

pub(super) fn new_frame(id: NodeId, kind: FrameKind) -> Frame {
    Frame {
        id,
        kind,
        html: String::new(),
        source_ranges: Vec::new(),
        constructs: Vec::new(),
    }
}

pub(super) enum FrameKind {
    Root,
    Container,
    Leaf(BlockKind),
    Html,
    Raw(BlockKind),
}

#[derive(Clone, Copy, Default)]
pub(super) struct InlineState {
    pub(super) marks: InlineMarks,
    pub(super) link: Option<u32>,
}

pub(super) struct Builder {
    pub(super) arena: DocumentArena,
    pub(super) texts: TextStore,
    pub(super) intern: String,
    pub(super) stack: Vec<Frame>,
    pub(super) implicit_para: bool,
    pub(super) inline: Vec<InlineState>,

    pub(super) recorder: ConstructRecorder,
    pub(super) links: Vec<Link>,
    pub(super) langs: Vec<String>,
    pub(super) footnotes: Vec<String>,
    pub(super) image_only: bool,
    pub(super) image_count: u32,
    pub(super) table_aligns: u64,
    pub(super) table_alignment_overflow: HashMap<NodeId, Arc<[u8]>>,
    pub(super) table_col: u32,
    pub(super) header_row: bool,
    pub(super) pending_image_dest: Option<u32>,
    pub(super) image_display_at: usize,

    pub(super) list_item_pending: bool,

    pub(super) math_continuation: bool,
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
            stack: vec![new_frame(root, FrameKind::Root)],
            implicit_para: false,
            inline: Vec::new(),
            recorder: ConstructRecorder::new(),
            links: Vec::new(),
            langs: Vec::new(),
            footnotes: Vec::new(),
            image_only: false,
            image_count: 0,
            table_aligns: 0,
            table_alignment_overflow: HashMap::new(),
            table_col: 0,
            pending_image_dest: None,
            image_display_at: 0,
            list_item_pending: false,
            header_row: false,
            math_continuation: false,
        }
    }

    pub(super) fn alloc(&mut self, kind: BlockKind) -> NodeId {
        let id = self.arena.alloc(kind);
        self.texts.push_slot();
        id
    }

    pub(super) fn current_inline(&self) -> InlineState {
        self.inline.last().copied().unwrap_or_default()
    }

    pub(super) fn push_inline(&mut self, marks: InlineMarks, link: Option<u32>) {
        self.inline.push(InlineState { marks, link });
    }

    pub(super) fn pop_inline(&mut self) {
        self.inline.pop();
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
        let table = self.stack.iter().rev().find_map(|frame| {
            self.arena
                .get(frame.id)
                .is_some_and(|node| node.kind == BlockKind::Table)
                .then_some(frame.id)
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
}
