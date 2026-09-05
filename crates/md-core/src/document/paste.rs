use super::arena::NodeId;
use super::change::{ChangeSet, DocChange};
use super::chars::floor_char_boundary;
use super::{Document, PasteIntent, bind, editor_options, load, load_markdown, write};
use crate::block::{BlockId, BlockKind, NodeExtra, TextEditStrategy};
use std::ops::Range;

impl Document {
    pub fn paste(
        &mut self,
        index: BlockId,
        range: Range<usize>,
        text: &str,
        intent: PasteIntent,
    ) -> (ChangeSet, BlockId, usize) {
        let Some(id) = self
            .live_id(index)
            .filter(|&id| self.arena.get(id).is_some_and(|n| n.kind.is_text_leaf()))
        else {
            return (ChangeSet::empty(self.revision), index, range.start);
        };

        let folded;
        let text = if text.contains('\r') {
            folded = load::fold_cr(text);
            folded.as_str()
        } else {
            text
        };
        match intent {
            PasteIntent::PlainText => {
                let before = self.revision;
                if self.math_edit_would_close(id, range.clone(), text) {
                    return (ChangeSet::empty(self.revision), index, range.start);
                }

                let parts = split_plain_paragraphs(text);
                let split_host = matches!(
                    self.arena.get(id).map(|n| n.kind),
                    Some(BlockKind::Paragraph | BlockKind::Heading(_) | BlockKind::Math)
                );
                if parts.len() > 1 && split_host {
                    let (changes, last_block, caret) = self.paste_plain_lines(id, range, &parts);
                    let set = self.commit(before, changes);
                    return (set, last_block, caret);
                }

                let keeps_trailing_blank = matches!(
                    self.arena.get(id).map(|n| n.kind),
                    Some(BlockKind::CodeBlock | BlockKind::Mermaid | BlockKind::TableCell)
                );
                let text = if parts.len() > 1 || keeps_trailing_blank {
                    text
                } else {
                    parts[0]
                };
                let (change, caret) = self.rewrite_text(id, range, text);
                let set = self.commit(before, vec![change]);
                (set, index, caret)
            }
            PasteIntent::IndependentFragment => self.paste_fragment(index, range, text),
        }
    }

    fn paste_plain_lines(
        &mut self,
        id: NodeId,
        range: Range<usize>,
        parts: &[&str],
    ) -> (Vec<DocChange>, BlockId, usize) {
        let (first, mut caret) = self.rewrite_text(id, range, parts[0]);
        let mut changes = vec![first];
        let parent = self.arena.get(id).and_then(|n| n.parent);
        let mut anchor = Some(id);
        let mut last_block = id.index;
        for part in &parts[1..] {
            let Some(parent) = parent else {
                break;
            };
            let leaf = self.alloc_leaf(BlockKind::Paragraph);
            if let Some(t) = self.texts.get_mut(leaf.text_id()) {
                t.set_projected(
                    (*part).to_string(),
                    (*part).to_string(),
                    Vec::new(),
                    Some(Vec::new()),
                );
                t.s2d = bind::identity_map(part.len());
            }
            self.arena.insert_after(parent, anchor, leaf);
            self.bump_structure(parent);
            changes.push(DocChange::TreeSpliced {
                parent,
                before: anchor,
                removed: Vec::new(),
                inserted: vec![leaf],
            });
            anchor = Some(leaf);
            last_block = leaf.index;
            caret = part.len();
        }
        (changes, last_block, caret)
    }

    fn paste_fragment(
        &mut self,
        index: BlockId,
        range: Range<usize>,
        text: &str,
    ) -> (ChangeSet, BlockId, usize) {
        if text.is_empty() {
            let start = range.start;
            let set = self.replace_text(index, range, text);
            return (set, index, start);
        }
        let Some(id) = self.live_id(index) else {
            return (ChangeSet::empty(self.revision), index, range.start);
        };
        if self.arena.get(id).map(|n| n.kind) == Some(BlockKind::TableCell) {
            let start = range.start;
            let set = self.replace_text(index, range, text);
            return (set, index, start + text.len());
        }
        let fragment = load_markdown(text, editor_options());
        let roots: Vec<NodeId> = fragment.arena.children(fragment.root).collect();
        if roots.is_empty() {
            let start = range.start;
            let set = self.replace_text(index, range, text);
            return (set, index, start + text.len());
        }
        if let Some(caret) = self.merge_paragraph(&fragment, id, index, range.clone()) {
            return caret;
        }
        let before = self.revision;
        let mut changes = Vec::new();
        let parent = self.arena.get(id).and_then(|n| n.parent).expect("parent");
        let off = if range.start != range.end {
            let (ch, start) = self.rewrite_text(id, range, "");
            changes.push(ch);
            start
        } else {
            let display = self.display(id);
            floor_char_boundary(display, range.start.min(display.len()))
        };
        let len = self.display(id).len();
        let splice_before;
        let caret_src;
        let mut inserted;
        if off > 0 && off < len {
            let (tc, tail) = self.split_leaf_nodes(id, off);
            changes.push(tc);
            splice_before = Some(id);
            let grafted = self.graft_roots(&fragment, &roots, parent, Some(id));
            caret_src = grafted.1;
            inserted = grafted.0;
            inserted.push(tail);
        } else if off == 0 {
            splice_before = self.arena.get(id).and_then(|n| n.prev_sibling);
            let grafted = self.graft_roots(&fragment, &roots, parent, splice_before);
            inserted = grafted.0;
            caret_src = grafted.1;
        } else {
            splice_before = Some(id);
            let grafted = self.graft_roots(&fragment, &roots, parent, Some(id));
            inserted = grafted.0;
            caret_src = grafted.1;
        }
        self.bump_structure(parent);
        let caret = caret_src
            .and_then(|n| self.last_text_caret(n))
            .unwrap_or((index, off));
        changes.push(DocChange::TreeSpliced {
            parent,
            before: splice_before,
            removed: Vec::new(),
            inserted,
        });
        let set = self.commit(before, changes);
        (set, caret.0, caret.1)
    }

    fn merge_paragraph(
        &mut self,
        fragment: &Document,
        id: NodeId,
        index: BlockId,
        range: Range<usize>,
    ) -> Option<(ChangeSet, BlockId, usize)> {
        let (root, kind) = bind::unique_root(fragment)?;
        if kind != BlockKind::Paragraph {
            return None;
        }
        if !self
            .arena
            .get(id)
            .map(|n| n.kind)
            .is_some_and(|kind| kind.text_edit_strategy() == TextEditStrategy::Phrasing)
        {
            return None;
        }

        let text = write::phrasing_source(fragment, root);
        let before = self.revision;
        let (change, caret) = self.rewrite_text(id, range, &text);
        let set = self.commit(before, vec![change]);
        Some((set, index, caret))
    }

    fn graft_roots(
        &mut self,
        src: &Document,
        roots: &[NodeId],
        parent: NodeId,
        mut after: Option<NodeId>,
    ) -> (Vec<NodeId>, Option<NodeId>) {
        let mut inserted = Vec::new();
        let mut caret_src = None;
        for r in roots {
            let cloned = self.clone_subtree(src, *r);
            self.arena.insert_after(parent, after, cloned);
            inserted.push(cloned);
            caret_src = Some(cloned);
            after = Some(cloned);
        }
        (inserted, caret_src)
    }

    pub(super) fn remap_link(&mut self, src: &Document, idx: u32) -> u32 {
        let dest = src.link_dest(idx).unwrap_or("").to_string();
        let title = src
            .links
            .get(idx as usize)
            .map(|l| l.title.clone())
            .unwrap_or_default();
        let links = std::sync::Arc::make_mut(&mut self.links);
        let id = links.len() as u32;
        links.push(load::Link { dest, title });
        id
    }

    pub(super) fn remap_lang(&mut self, src: &Document, idx: u32) -> u32 {
        let lang = src.lang(idx).unwrap_or("").to_string();
        let langs = std::sync::Arc::make_mut(&mut self.langs);
        let id = langs.len() as u32;
        langs.push(lang);
        id
    }

    fn remap_footnote(&mut self, src: &Document, idx: u32) -> u32 {
        let label = src.footnote_label(idx).unwrap_or("").to_string();
        let footnotes = std::sync::Arc::make_mut(&mut self.footnotes);
        let id = footnotes.len() as u32;
        footnotes.push(label);
        id
    }

    fn copy_extra(&mut self, src: &Document, src_id: NodeId, dest: NodeId) {
        let extra = match src.extra(src_id) {
            NodeExtra::Image { dest, .. } => NodeExtra::Image {
                dest: self.remap_link(src, dest),
                source: None,
            },
            NodeExtra::CodeFence { lang, marker, len } => NodeExtra::CodeFence {
                lang: lang.map(|lang| self.remap_lang(src, lang)),
                marker,
                len,
            },
            NodeExtra::Table { alignments, .. } => NodeExtra::Table {
                alignments,
                source: None,
            },
            NodeExtra::FootnoteLabel { label } => NodeExtra::FootnoteLabel {
                label: self.remap_footnote(src, label),
            },
            other => other,
        };
        if let Some(n) = self.arena.get_mut(dest) {
            n.extra = extra;
        }
        if let Some(bits) = src.table_alignment_overflow.get(&src_id) {
            self.table_alignment_overflow
                .insert(dest, std::sync::Arc::clone(bits));
        }
    }

    fn clone_subtree(&mut self, src: &Document, src_id: NodeId) -> NodeId {
        let kind = src.arena.get(src_id).map(|n| n.kind).expect("src node");
        let root = if kind.is_vertical_container() || kind == BlockKind::TableRow {
            self.alloc_container(kind)
        } else {
            self.alloc_leaf(kind)
        };
        self.copy_extra(src, src_id, root);
        if kind.is_text_leaf() {
            self.clone_leaf_text(src, src_id, root);
        }

        let mut stack = vec![(src_id, root)];
        while let Some((source_parent, dest_parent)) = stack.pop() {
            let children: Vec<NodeId> = src.arena.children(source_parent).collect();
            for child in children {
                let kind = src.arena.get(child).map(|n| n.kind).expect("src child");
                let dest = if kind.is_vertical_container() || kind == BlockKind::TableRow {
                    self.alloc_container(kind)
                } else {
                    self.alloc_leaf(kind)
                };
                self.copy_extra(src, child, dest);
                if kind.is_text_leaf() {
                    self.clone_leaf_text(src, child, dest);
                } else {
                    stack.push((child, dest));
                }
                self.arena.append_child(dest_parent, dest);
            }
        }
        root
    }

    fn clone_leaf_text(&mut self, src: &Document, src_id: NodeId, dest: NodeId) {
        let Some(src_leaf) = src.texts.get(src_id.text_id()) else {
            return;
        };

        let display = src_leaf.display().to_string();
        let source = src.leaf_source(src_id).to_string();
        let s2d = src_leaf.s2d.clone();

        let constructs = src_leaf.constructs.clone();
        let runs = self.remap_runs(src, src_id);
        if let Some(dst) = self.texts.get_mut(dest.text_id()) {
            dst.set_projected(display, source, runs, constructs);
            dst.s2d = s2d;
        }
        let _ = self.bump_content(dest);
    }
}

fn split_plain_paragraphs(text: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = Vec::new();
    for (index, part) in text.split("\n\n").enumerate() {
        if index == 0 {
            parts.push(part);
        } else {
            let stripped = part.trim_start_matches('\n');
            if !stripped.is_empty() {
                parts.push(stripped);
            }
        }
    }
    parts
}
