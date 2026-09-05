use crate::block::{NodeExtra, alignment_at};
use pulldown_cmark::Options;
use std::collections::HashMap;
use std::fmt::Write;
use std::sync::Arc;
use text::TextStore;

mod alloc;
mod arena;
mod bind;
mod change;
mod chars;
mod copy;
pub mod edit;
mod export;
mod focus;
pub(crate) mod history;
mod ledger;
mod load;
mod nav;
mod paste;
mod promote;
mod query;
mod replay;
mod syntax;
mod text;
mod text_edit;
mod word;
mod write;

#[cfg(test)]
mod tests;

pub use arena::{DocumentArena, NodeId};
pub use change::{ChangeSet, DocChange};
pub use chars::{floor_char_boundary, next_char_boundary, prev_char_boundary};
pub use edit::{
    Caret, Command, Sel, TABLE_INSERT_MAX_COLS, TABLE_INSERT_MAX_ROWS, TABLE_INSERT_MIN_COLS,
    TABLE_INSERT_MIN_ROWS, TABLE_PICKER_MAX_COLS, TABLE_PICKER_MAX_ROWS, TableLoc, TableOp,
    TableStep, apply, in_table, table_loc, table_step,
};
pub use export::WriteSnapshot;
pub use focus::{FocusBias, RevealedImage, RevealedMath};
pub use load::Link;
pub use text::LeafSnapshot;
pub use word::{
    next_grapheme_boundary, next_word_boundary, prev_grapheme_boundary, prev_word_boundary,
    word_span,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasteIntent {
    PlainText,
    IndependentFragment,
}

#[derive(Clone)]
pub struct Document {
    pub arena: DocumentArena,
    pub(crate) texts: TextStore,
    pub source: Arc<str>,
    intern: Arc<String>,
    pub links: Arc<Vec<Link>>,
    pub langs: Arc<Vec<String>>,
    pub(crate) footnotes: Arc<Vec<String>>,
    pub(crate) reference_definitions: Arc<Vec<String>>,

    pub(crate) table_alignment_overflow: HashMap<NodeId, Arc<[u8]>>,
    pub root: NodeId,
    revision: u64,
    max_content_revision: u64,
    pub(crate) changes: ChangeSet,
    pub(crate) focus: Option<focus::InlineFocus>,

    pub(crate) block_edit: Option<NodeId>,
}

impl Document {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn reset_changes(&mut self) {
        self.changes = ChangeSet::empty(self.revision);
    }

    pub(crate) fn table_alignment_at(&self, table: NodeId, col: usize) -> u8 {
        if let Some(bits) = self.table_alignment_overflow.get(&table) {
            return bits.get(col).copied().unwrap_or(0);
        }
        match self.arena.get(table).map(|node| node.extra) {
            Some(NodeExtra::Table { alignments, .. }) => alignment_at(alignments, col),
            _ => 0,
        }
    }

    pub(crate) fn populate_s2d_cache(&mut self) {
        let mut stack = vec![self.root];
        let mut leaves = Vec::new();
        while let Some(id) = stack.pop() {
            let Some(node) = self.arena.get(id) else {
                continue;
            };
            if node.kind.is_text_leaf() {
                leaves.push((id, node.kind));
            }
            let mut child = node.last_child;
            while let Some(id) = child {
                stack.push(id);
                child = self.arena.get(id).and_then(|n| n.prev_sibling);
            }
        }
        for (id, kind) in leaves {
            let Some(tid) = self.arena.get(id).and_then(|n| n.text) else {
                continue;
            };
            let Some(leaf) = self.texts.get(tid) else {
                continue;
            };
            let s2d = crate::document::bind::bind_map(
                leaf.source_str(&self.source),
                leaf.display(),
                kind,
            )
            .1;
            if let Some(leaf) = self.texts.get_mut(tid) {
                leaf.s2d = s2d;
            }
        }
    }
}

pub fn load_markdown(md: &str, opts: Options) -> Document {
    load::load(md, opts)
}

pub fn editor_options() -> Options {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    opts.insert(Options::ENABLE_GFM);
    opts.insert(Options::ENABLE_SUPERSCRIPT);
    opts.insert(Options::ENABLE_SUBSCRIPT);
    opts.insert(Options::ENABLE_MATH);
    opts.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
    opts.insert(Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS);
    opts
}

pub(crate) fn sanitized_editor_options() -> Options {
    let mut opts = editor_options();
    opts.remove(Options::ENABLE_DEFINITION_LIST);
    opts
}

pub fn dump_structure(doc: &Document) -> String {
    let mut out = String::new();
    for id in doc.preorder() {
        let node = doc.arena.get(id).expect("live node");
        let _ = doc.leaf_source(id);
        let _ = writeln!(
            out,
            "{}\t{:?}\t{}",
            id.index,
            node.kind,
            escape_field(doc.display(id))
        );
    }
    out
}

pub(crate) fn escape_field(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}
