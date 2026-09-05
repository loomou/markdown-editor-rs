use super::Document;
use super::arena::NodeId;
use crate::block::{BlockKind, ListMarker, NodeExtra};
use std::fmt::{self, Write};
use std::rc::Rc;

mod block;
mod inline;

#[cfg(test)]
mod tests;

use block::{run, run_item};

pub(super) use inline::phrasing_source;

pub(crate) trait MarkdownExport {
    fn root(&self) -> NodeId;
    fn kind(&self, id: NodeId) -> Option<BlockKind>;
    fn extra(&self, id: NodeId) -> NodeExtra;
    fn parent(&self, id: NodeId) -> Option<NodeId>;
    fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_;
    fn table_alignment(&self, id: NodeId, col: usize) -> u8;
    fn leaf_source(&self, id: NodeId) -> &str;
    fn display(&self, id: NodeId) -> &str;
    fn lang(&self, id: u32) -> Option<&str>;
    fn footnote_label(&self, id: u32) -> Option<&str>;
    fn link_dest(&self, id: u32) -> Option<&str>;
    fn raw_block(&self, id: NodeId) -> Option<&str>;
    fn reference_definitions(&self) -> &[String];
}

struct MarkdownWriter<W> {
    inner: W,
    written: bool,
    nl_run: u8,
}

impl<W: fmt::Write> MarkdownWriter<W> {
    fn new(inner: W) -> Self {
        MarkdownWriter {
            inner,
            written: false,
            nl_run: 0,
        }
    }
}

impl<'a> MarkdownWriter<&'a mut String> {
    fn for_string(s: &'a mut String) -> Self {
        let written = !s.is_empty();
        let nl_run = if s.ends_with("\n\n") {
            2
        } else if s.ends_with('\n') {
            1
        } else {
            0
        };
        MarkdownWriter {
            inner: s,
            written,
            nl_run,
        }
    }
}

impl<W: fmt::Write> fmt::Write for MarkdownWriter<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if s.is_empty() {
            return Ok(());
        }
        self.inner.write_str(s)?;
        self.written = true;
        for b in s.bytes() {
            if b == b'\n' {
                self.nl_run = (self.nl_run + 1).min(2);
            } else {
                self.nl_run = 0;
            }
        }
        Ok(())
    }
}

impl MarkdownExport for Document {
    fn root(&self) -> NodeId {
        self.root
    }

    fn kind(&self, id: NodeId) -> Option<BlockKind> {
        self.arena.get(id).map(|n| n.kind)
    }

    fn extra(&self, id: NodeId) -> NodeExtra {
        Document::extra(self, id)
    }

    fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.arena.get(id).and_then(|n| n.parent)
    }

    fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.arena.children(id)
    }

    fn table_alignment(&self, id: NodeId, col: usize) -> u8 {
        self.table_alignment_at(id, col)
    }

    fn leaf_source(&self, id: NodeId) -> &str {
        Document::leaf_source(self, id)
    }

    fn display(&self, id: NodeId) -> &str {
        Document::display(self, id)
    }

    fn lang(&self, id: u32) -> Option<&str> {
        Document::lang(self, id)
    }

    fn footnote_label(&self, id: u32) -> Option<&str> {
        Document::footnote_label(self, id)
    }

    fn link_dest(&self, id: u32) -> Option<&str> {
        Document::link_dest(self, id)
    }

    fn raw_block(&self, id: NodeId) -> Option<&str> {
        raw_block_source(self, id)
    }

    fn reference_definitions(&self) -> &[String] {
        &self.reference_definitions
    }
}

pub(super) fn raw_block_source(doc: &Document, id: NodeId) -> Option<&str> {
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
    if node.parent != Some(doc.root) || !pristine_subtree(doc, id) {
        return None;
    }
    doc.source.get(start as usize..end as usize)
}

pub(super) fn pristine_subtree(doc: &Document, id: NodeId) -> bool {
    let mut stack = vec![id];
    while let Some(id) = stack.pop() {
        let Some(node) = doc.arena.get(id) else {
            return false;
        };
        if node.content_revision != 1 || node.structure_revision != 1 {
            return false;
        }
        stack.extend(doc.arena.children(id));
    }
    true
}

#[derive(Clone, Copy, PartialEq)]
enum PrefixSeg {
    Quote,

    Indent(u16),
}

struct PrefixNode {
    parent: Option<Rc<PrefixNode>>,
    seg: PrefixSeg,
}

impl Drop for PrefixNode {
    fn drop(&mut self) {
        let mut cur = self.parent.take();
        while let Some(node) = cur {
            match Rc::try_unwrap(node) {
                Ok(mut owned) => cur = owned.parent.take(),
                Err(shared) => {
                    drop(shared);
                    break;
                }
            }
        }
    }
}

#[derive(Clone, Default)]
struct Prefix {
    tip: Option<Rc<PrefixNode>>,
}

impl Prefix {
    fn quoted(&self) -> Self {
        Prefix {
            tip: Some(Rc::new(PrefixNode {
                parent: self.tip.clone(),
                seg: PrefixSeg::Quote,
            })),
        }
    }

    fn indented(&self, extra: usize) -> Self {
        let extra = u16::try_from(extra).unwrap_or(u16::MAX);
        if let Some(tip) = &self.tip
            && let PrefixSeg::Indent(n) = tip.seg
        {
            return Prefix {
                tip: Some(Rc::new(PrefixNode {
                    parent: tip.parent.clone(),
                    seg: PrefixSeg::Indent(n.saturating_add(extra)),
                })),
            };
        }
        Prefix {
            tip: Some(Rc::new(PrefixNode {
                parent: self.tip.clone(),
                seg: PrefixSeg::Indent(extra),
            })),
        }
    }

    fn is_plain(&self) -> bool {
        self.tip.is_none()
    }

    fn flat(&self) -> String {
        let mut segs = Vec::new();
        let mut node = self.tip.as_deref();
        while let Some(n) = node {
            segs.push(n.seg);
            node = n.parent.as_deref();
        }
        segs.reverse();
        let mut out = String::new();
        for seg in segs {
            match seg {
                PrefixSeg::Quote => out.push_str("> "),
                PrefixSeg::Indent(n) => {
                    for _ in 0..n {
                        out.push(' ');
                    }
                }
            }
        }
        out
    }

    fn write_open(&self, out: &mut impl fmt::Write) -> fmt::Result {
        out.write_str(&self.flat())
    }
}

impl Document {
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let _ = write_markdown(self, &mut out);
        out
    }
}

pub(crate) fn write_markdown<D, W>(doc: &D, out: &mut W) -> fmt::Result
where
    D: MarkdownExport,
    W: fmt::Write,
{
    let mut w = MarkdownWriter::new(out);
    run(doc, doc.root(), &mut w, &Prefix::default())?;
    if !doc.reference_definitions().is_empty() {
        blank_line(&mut w, &Prefix::default())?;
        for (index, definition) in doc.reference_definitions().iter().enumerate() {
            if index > 0 && w.nl_run == 0 {
                w.write_str("\n")?;
            }
            w.write_str(definition.trim_end_matches(['\n', '\r']))?;
        }
    }
    if w.written && w.nl_run == 0 {
        w.write_str("\n")?;
    }
    Ok(())
}

pub(super) fn write_node(doc: &Document, id: NodeId, out: &mut String) {
    let mut w = MarkdownWriter::for_string(out);
    let _ = run(doc, id, &mut w, &Prefix::default());
}

pub(super) fn write_list_item(
    doc: &Document,
    id: NodeId,
    out: &mut String,
    marker: ListMarker,
    num: u64,
) {
    let ordered = matches!(marker, ListMarker::Period | ListMarker::Parenthesis);
    let mut w = MarkdownWriter::for_string(out);
    let _ = run_item(doc, id, &mut w, ordered, num, marker);
}

fn write_prefixed<W: fmt::Write>(
    out: &mut MarkdownWriter<W>,
    prefix: &Prefix,
    text: &str,
) -> fmt::Result {
    if text.is_empty() {
        return prefix.write_open(out);
    }

    let flat = prefix.flat();
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.write_str("\n")?;
        }
        out.write_str(&flat)?;
        out.write_str(line)?;
    }
    Ok(())
}

fn blank_line<W: fmt::Write>(out: &mut MarkdownWriter<W>, prefix: &Prefix) -> fmt::Result {
    if !out.written {
        return Ok(());
    }
    if out.nl_run == 0 {
        out.write_str("\n")?;
    }
    if !prefix.is_plain() {
        prefix.write_open(out)?;
        out.write_str("\n")?;
    } else if out.nl_run < 2 {
        out.write_str("\n")?;
    }
    Ok(())
}

fn push_first_and_rest<W: fmt::Write>(
    out: &mut MarkdownWriter<W>,
    rest: &Prefix,
    text: &str,
) -> fmt::Result {
    let flat = rest.flat();
    let mut lines = text.split('\n');
    if let Some(first) = lines.next() {
        out.write_str(first)?;
    }
    for line in lines {
        out.write_str("\n")?;
        out.write_str(&flat)?;
        out.write_str(line)?;
    }
    Ok(())
}
