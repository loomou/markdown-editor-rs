use super::Document;
use super::arena::NodeId;
use super::load::Link;
use super::text::{LeafSnapshot, LeafSource};
use super::write::{MarkdownExport, write_markdown};
use crate::block::{BlockKind, NodeExtra, alignment_at};
use std::collections::HashMap;
use std::fmt;
use std::ops::Range;
use std::sync::Arc;

#[derive(Clone)]
enum SnapText {
    Empty,
    Source {
        owner: Arc<str>,
        range: Range<usize>,
    },
    Shared(Arc<str>),
    Display(Arc<LeafSnapshot>),
}

impl SnapText {
    fn as_str(&self) -> &str {
        match self {
            SnapText::Empty => "",
            SnapText::Source { owner, range } => owner.get(range.clone()).unwrap_or(""),
            SnapText::Shared(text) => text,
            SnapText::Display(snapshot) => snapshot.display.as_str(),
        }
    }
}

struct SnapNode {
    kind: BlockKind,
    extra: NodeExtra,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    source: SnapText,
    display: SnapText,
    raw: Option<SnapText>,
}

pub struct WriteSnapshot {
    root: NodeId,
    revision: u64,
    nodes: HashMap<NodeId, SnapNode>,
    langs: Arc<Vec<String>>,
    footnotes: Arc<Vec<String>>,
    links: Arc<Vec<Link>>,
    reference_definitions: Arc<Vec<String>>,
    table_alignment_overflow: HashMap<NodeId, Arc<[u8]>>,
}

impl WriteSnapshot {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn write_markdown(&self, out: &mut impl fmt::Write) -> fmt::Result {
        write_markdown(self, out)
    }

    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let _ = self.write_markdown(&mut out);
        out
    }
}

impl Document {
    pub fn write_snapshot(&self) -> WriteSnapshot {
        let mut nodes = HashMap::new();
        for id in self.preorder() {
            let Some(node) = self.arena.get(id) else {
                continue;
            };
            let display = snapshot_display(self, id);
            nodes.insert(
                id,
                SnapNode {
                    kind: node.kind,
                    extra: node.extra,
                    parent: node.parent,
                    children: self.arena.children(id).collect(),
                    source: snapshot_source(self, id, &display),
                    display,
                    raw: snapshot_raw(self, id),
                },
            );
        }
        WriteSnapshot {
            root: self.root,
            revision: self.revision(),
            nodes,
            langs: Arc::clone(&self.langs),
            footnotes: Arc::clone(&self.footnotes),
            links: Arc::clone(&self.links),
            reference_definitions: Arc::clone(&self.reference_definitions),
            table_alignment_overflow: self.table_alignment_overflow.clone(),
        }
    }
}

impl MarkdownExport for WriteSnapshot {
    fn root(&self) -> NodeId {
        self.root
    }

    fn kind(&self, id: NodeId) -> Option<BlockKind> {
        self.nodes.get(&id).map(|n| n.kind)
    }

    fn extra(&self, id: NodeId) -> NodeExtra {
        self.nodes
            .get(&id)
            .map(|n| n.extra)
            .unwrap_or(NodeExtra::None)
    }

    fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.nodes.get(&id).and_then(|n| n.parent)
    }

    fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes
            .get(&id)
            .map(|n| n.children.as_slice())
            .unwrap_or(&[])
            .iter()
            .copied()
    }

    fn table_alignment(&self, id: NodeId, col: usize) -> u8 {
        self.table_alignment_overflow
            .get(&id)
            .and_then(|bits| bits.get(col).copied())
            .unwrap_or_else(|| match self.extra(id) {
                NodeExtra::Table { alignments, .. } => alignment_at(alignments, col),
                _ => 0,
            })
    }

    fn leaf_source(&self, id: NodeId) -> &str {
        self.nodes.get(&id).map(|n| n.source.as_str()).unwrap_or("")
    }

    fn display(&self, id: NodeId) -> &str {
        self.nodes
            .get(&id)
            .map(|n| n.display.as_str())
            .unwrap_or("")
    }

    fn lang(&self, id: u32) -> Option<&str> {
        self.langs.get(id as usize).map(|s| s.as_str())
    }

    fn footnote_label(&self, id: u32) -> Option<&str> {
        self.footnotes.get(id as usize).map(|s| s.as_str())
    }

    fn link_dest(&self, id: u32) -> Option<&str> {
        self.links.get(id as usize).map(|l| l.dest.as_str())
    }

    fn raw_block(&self, id: NodeId) -> Option<&str> {
        self.nodes
            .get(&id)
            .and_then(|node| node.raw.as_ref().map(SnapText::as_str))
    }

    fn reference_definitions(&self) -> &[String] {
        &self.reference_definitions
    }
}

fn snapshot_display(doc: &Document, id: NodeId) -> SnapText {
    if doc.focus.as_ref().is_some_and(|focus| focus.node == id) {
        return SnapText::Shared(Arc::from(doc.display(id)));
    }
    doc.leaf_snapshot(id)
        .map(SnapText::Display)
        .unwrap_or(SnapText::Empty)
}

fn snapshot_source(doc: &Document, id: NodeId, display: &SnapText) -> SnapText {
    let Some(leaf) = doc
        .arena
        .get(id)
        .and_then(|node| node.text)
        .and_then(|text| doc.texts.get(text))
    else {
        return SnapText::Empty;
    };
    match &leaf.source {
        LeafSource::Span(range) => SnapText::Source {
            owner: Arc::clone(&doc.source),
            range: range.start as usize..range.end as usize,
        },
        LeafSource::SameAsDisplay => display.clone(),
        LeafSource::Owned(source) => SnapText::Shared(Arc::from(source.as_ref())),
    }
}

fn snapshot_raw(doc: &Document, id: NodeId) -> Option<SnapText> {
    let node = doc.arena.get(id)?;
    let (NodeExtra::Table {
        source: Some((start, end)),
        ..
    }
    | NodeExtra::Image {
        source: Some((start, end)),
        ..
    }) = node.extra
    else {
        return None;
    };
    if node.parent != Some(doc.root) || !super::write::pristine_subtree(doc, id) {
        return None;
    }
    Some(SnapText::Source {
        owner: Arc::clone(&doc.source),
        range: start as usize..end as usize,
    })
}
