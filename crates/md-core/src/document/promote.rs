use super::arena::NodeId;
use super::change::DocChange;
use super::chars::floor_char_boundary;
use super::edit::Caret;
use super::focus::RawConstruct;
use super::{Document, bind, editor_options, load_markdown, syntax};
use crate::block::{BlockKind, NodeExtra};
use crate::inline::InlineRun;

struct LeafTransition {
    id: NodeId,
    parent: NodeId,
    prev: Option<NodeId>,
    before: u64,
    old_revision: u64,
    old_source: String,
    old_kind: BlockKind,
    old_extra: NodeExtra,
}

impl Document {
    fn leaf_transition(&self, id: NodeId) -> Option<LeafTransition> {
        let node = self.arena.get(id)?;
        Some(LeafTransition {
            id,
            parent: node.parent?,
            prev: node.prev_sibling,
            before: self.revision,
            old_revision: node.content_revision,
            old_source: self.leaf_source(id).to_string(),
            old_kind: node.kind,
            old_extra: node.extra,
        })
    }

    fn set_leaf_shape(&mut self, transition: &LeafTransition, kind: BlockKind, extra: NodeExtra) {
        if let Some(node) = self.arena.get_mut(transition.id) {
            node.kind = kind;
            node.extra = extra;
        }
        self.ensure_leaf_text(transition.id);
    }

    fn replace_leaf_projection(
        &mut self,
        transition: &LeafTransition,
        display: String,
        source: String,
        runs: Vec<InlineRun>,
        s2d: Option<Vec<usize>>,
        constructs: Option<Vec<RawConstruct>>,
    ) -> DocChange {
        let inserted = source.clone();
        if let Some(leaf) = self.texts.get_mut(transition.id.text_id()) {
            leaf.set_projected(display, source, runs, constructs);
            if let Some(s2d) = s2d {
                leaf.s2d = s2d;
            }
        }
        let new_revision = self.bump_content(transition.id);
        DocChange::text(
            transition.id,
            transition.old_revision,
            new_revision,
            0..transition.old_source.len() as u32,
            transition.old_source.clone(),
            inserted,
        )
    }

    fn finish_leaf_transition(
        &mut self,
        transition: LeafTransition,
        text_change: DocChange,
        mut additional_changes: Vec<DocChange>,
    ) {
        self.bump_structure(transition.parent);
        let mut changes = vec![
            text_change,
            self.attrs_change(transition.id, transition.old_kind, transition.old_extra),
            DocChange::TreeSpliced {
                parent: transition.parent,
                before: transition.prev,
                removed: vec![transition.id],
                inserted: vec![transition.id],
            },
        ];
        changes.append(&mut additional_changes);
        let _ = self.commit(transition.before, changes);
    }

    pub(crate) fn try_commit_open_fence(&mut self, id: NodeId) -> Option<Caret> {
        let transition = self.leaf_transition(id)?;
        if transition.old_kind != BlockKind::Paragraph {
            return None;
        }
        let source = syntax::normalize_source(&transition.old_source);
        if !syntax::is_open_fence_line(source) {
            return None;
        }
        let frag = load_markdown(&format!("{source}\n"), editor_options());
        let (frag_leaf, next) = bind::unique_root(&frag)?;
        if !matches!(next, BlockKind::CodeBlock | BlockKind::Mermaid) {
            return None;
        }
        let frag_extra = frag.extra(frag_leaf);
        let (marker, len) = frag_extra.code_fence_style().unwrap_or_default();
        let extra = NodeExtra::CodeFence {
            lang: frag_extra
                .code_fence_lang()
                .map(|lang| self.remap_lang(&frag, lang)),
            marker,
            len: len.max(3).min(u16::MAX as usize) as u16,
        };
        self.set_leaf_shape(&transition, next, extra);
        let text_change = self.replace_leaf_projection(
            &transition,
            String::new(),
            String::new(),
            Vec::new(),
            None,
            Some(Vec::new()),
        );
        self.finish_leaf_transition(transition, text_change, Vec::new());
        Some(Caret {
            block: id.index,
            offset: 0,
        })
    }

    pub(crate) fn try_commit_math_fence(&mut self, id: NodeId) -> Option<Caret> {
        let transition = self.leaf_transition(id)?;
        if transition.old_kind != BlockKind::Paragraph {
            return None;
        }
        let source = syntax::normalize_source(&transition.old_source);
        if !syntax::is_math_fence_line(source) {
            return None;
        }
        if self.is_list_item_first_child(id) {
            return None;
        }
        let frag = load_markdown("$$$$\n", editor_options());
        let (_, next) = bind::unique_root(&frag)?;
        if next != BlockKind::Math {
            return None;
        }
        self.set_leaf_shape(&transition, BlockKind::Math, NodeExtra::MathFence);
        let text_change = self.replace_leaf_projection(
            &transition,
            String::new(),
            String::new(),
            Vec::new(),
            None,
            Some(Vec::new()),
        );
        self.finish_leaf_transition(transition, text_change, Vec::new());
        Some(Caret {
            block: id.index,
            offset: 0,
        })
    }

    pub(crate) fn try_commit_thematic_break(&mut self, id: NodeId) -> Option<Caret> {
        let transition = self.leaf_transition(id)?;
        if transition.old_kind != BlockKind::Paragraph {
            return None;
        }
        if self.enclosed_by(id, BlockKind::ListItem) {
            return None;
        }
        let source = syntax::normalize_source(&transition.old_source);
        if !syntax::is_thematic_break_line(source) {
            return None;
        }
        let frag = load_markdown(&format!("{source}\n"), editor_options());
        let (_, next) = bind::unique_root(&frag)?;
        if next != BlockKind::ThematicBreak {
            return None;
        }
        self.set_leaf_shape(&transition, BlockKind::ThematicBreak, transition.old_extra);
        let text_change = self.replace_leaf_projection(
            &transition,
            String::new(),
            String::new(),
            Vec::new(),
            None,
            Some(Vec::new()),
        );
        let mut additional_changes = Vec::new();
        let caret = if let Some(next) = self.next_text_leaf(id) {
            next
        } else {
            let paragraph = self.alloc_leaf(BlockKind::Paragraph);
            self.arena
                .insert_after(transition.parent, Some(id), paragraph);
            additional_changes.push(DocChange::TreeSpliced {
                parent: transition.parent,
                before: Some(id),
                removed: Vec::new(),
                inserted: vec![paragraph],
            });
            paragraph
        };
        self.finish_leaf_transition(transition, text_change, additional_changes);
        Some(Caret {
            block: caret.index,
            offset: 0,
        })
    }

    pub(crate) fn is_list_item_first_child(&self, id: NodeId) -> bool {
        let Some(node) = self.arena.get(id) else {
            return false;
        };
        if node.prev_sibling.is_some() {
            return false;
        }
        node.parent
            .and_then(|p| self.arena.get(p))
            .map(|p| p.kind == BlockKind::ListItem)
            .unwrap_or(false)
    }

    pub(crate) fn enclosed_by(&self, id: NodeId, kind: BlockKind) -> bool {
        let mut cur = self.arena.get(id).and_then(|n| n.parent);
        while let Some(p) = cur {
            if self.arena.get(p).map(|n| n.kind) == Some(kind) {
                return true;
            }
            cur = self.arena.get(p).and_then(|n| n.parent);
        }
        false
    }

    pub(crate) fn try_commit_atx_heading(
        &mut self,
        id: NodeId,
        display_caret: usize,
    ) -> Option<Caret> {
        let transition = self.leaf_transition(id)?;
        if transition.old_kind != BlockKind::Paragraph {
            return None;
        }
        let source = syntax::normalize_source(&transition.old_source);
        if !syntax::is_atx_commit(source) {
            return None;
        }
        let frag = load_markdown(
            &super::bind::with_definitions(source, &self.reference_definitions),
            editor_options(),
        );
        let (frag_leaf, next) = bind::unique_root(&frag)?;
        if !matches!(next, BlockKind::Heading(_)) {
            return None;
        }
        let display = frag.display(frag_leaf).to_string();
        let runs = self.remap_runs(&frag, frag_leaf);
        let s2d = bind::source_to_display_map(source, &self.reference_definitions);
        let constructs = frag.recorded_constructs(frag_leaf).map(|c| c.to_vec());
        let (display, s2d, runs) = bind::restore_visible_ws(next, source, display, s2d, runs);
        let caret = if s2d.last().copied().unwrap_or(0) == display.len() {
            bind::source_to_display(&s2d, display_caret.min(source.len()))
        } else {
            display_caret.min(display.len())
        };
        let caret = floor_char_boundary(&display, caret);
        self.set_leaf_shape(&transition, next, transition.old_extra);
        let text_change = self.replace_leaf_projection(
            &transition,
            display,
            source.to_string(),
            runs,
            Some(s2d),
            constructs,
        );
        self.finish_leaf_transition(transition, text_change, Vec::new());
        Some(Caret {
            block: id.index,
            offset: caret,
        })
    }

    pub(crate) fn try_demote_heading(&mut self, id: NodeId) -> Option<Caret> {
        let transition = self.leaf_transition(id)?;
        if !matches!(transition.old_kind, BlockKind::Heading(_)) {
            return None;
        }
        let body = bind::setext_body(&transition.old_source)
            .map(str::to_string)
            .unwrap_or_else(|| bind::heading_body(&transition.old_source).to_string());
        let source_len = transition.old_source.len() as u32;
        self.set_leaf_shape(&transition, BlockKind::Paragraph, transition.old_extra);
        let (text_change, caret) = self.project_phrasing_at(
            id,
            body.clone(),
            0,
            (0..source_len, transition.old_source.clone(), body),
            true,
        );
        self.finish_leaf_transition(transition, text_change, Vec::new());
        Some(Caret {
            block: id.index,
            offset: caret,
        })
    }

    pub(crate) fn try_demote_empty_block(&mut self, id: NodeId) -> Option<Caret> {
        let transition = self.leaf_transition(id)?;
        let empty = self.display(id).is_empty();
        if !matches!(
            transition.old_kind,
            BlockKind::ThematicBreak | BlockKind::CodeBlock | BlockKind::Mermaid | BlockKind::Math
        ) {
            return None;
        }
        if matches!(
            transition.old_kind,
            BlockKind::CodeBlock | BlockKind::Mermaid | BlockKind::Math
        ) && !empty
        {
            return None;
        }
        self.set_leaf_shape(&transition, BlockKind::Paragraph, NodeExtra::None);
        let text_change = self.replace_leaf_projection(
            &transition,
            String::new(),
            String::new(),
            Vec::new(),
            None,
            Some(Vec::new()),
        );
        self.finish_leaf_transition(transition, text_change, Vec::new());
        Some(Caret {
            block: id.index,
            offset: 0,
        })
    }

    pub(crate) fn try_lift_wrapper(&mut self, id: NodeId) -> Option<Caret> {
        let wrapper = self.arena.get(id).and_then(|n| n.parent)?;
        let kind = self.arena.get(wrapper).map(|n| n.kind)?;
        if !Self::is_liftable_wrapper(kind) {
            return None;
        }
        if self.arena.get(wrapper).and_then(|n| n.first_child) != Some(id) {
            return None;
        }
        let host = self.arena.get(wrapper).and_then(|n| n.parent)?;
        let wrapper_prev = self.arena.get(wrapper).and_then(|n| n.prev_sibling);
        let before = self.revision;
        let old_extra = self.extra(id);
        let old_kind = self
            .arena
            .get(id)
            .map(|n| n.kind)
            .unwrap_or(BlockKind::Paragraph);
        self.arena.detach(id);
        self.arena.insert_after(host, wrapper_prev, id);
        let empty = self
            .arena
            .get(wrapper)
            .is_some_and(|n| n.first_child.is_none());
        self.bump_structure(host);
        let mut changes = Vec::new();
        if old_extra != self.extra(id) {
            changes.push(self.attrs_change(id, old_kind, old_extra));
        }
        if empty {
            self.arena.snapshot(wrapper);
            self.arena.detach(wrapper);
            self.arena.tombstone(wrapper);
            changes.push(DocChange::TreeSpliced {
                parent: wrapper,
                before: None,
                removed: vec![id],
                inserted: Vec::new(),
            });
            changes.push(DocChange::TreeSpliced {
                parent: host,
                before: wrapper_prev,
                removed: vec![wrapper],
                inserted: vec![id],
            });
        } else {
            self.bump_structure(wrapper);
            changes.push(DocChange::TreeSpliced {
                parent: wrapper,
                before: None,
                removed: vec![id],
                inserted: Vec::new(),
            });
            changes.push(DocChange::TreeSpliced {
                parent: host,
                before: wrapper_prev,
                removed: Vec::new(),
                inserted: vec![id],
            });
        }
        let _ = self.commit(before, changes);
        Some(Caret {
            block: id.index,
            offset: 0,
        })
    }

    fn is_liftable_wrapper(kind: BlockKind) -> bool {
        matches!(kind, BlockKind::BlockQuote | BlockKind::FootnoteDefinition)
    }

    pub(crate) fn try_lift_empty_wrapper(&mut self, id: NodeId) -> Option<Caret> {
        if !self.display(id).is_empty() {
            return None;
        }
        let wrapper = self.arena.get(id).and_then(|n| n.parent)?;
        let kind = self.arena.get(wrapper).map(|n| n.kind)?;
        if !Self::is_liftable_wrapper(kind) {
            return None;
        }
        if self.arena.get(id).and_then(|n| n.next_sibling).is_some() {
            return None;
        }
        if self.arena.get(wrapper).and_then(|n| n.first_child) == Some(id) {
            return self.try_lift_wrapper(id);
        }
        let host = self.arena.get(wrapper).and_then(|n| n.parent)?;
        let leaf_prev = self.arena.get(id).and_then(|n| n.prev_sibling);
        let before = self.revision;
        self.arena.detach(id);
        self.arena.insert_after(host, Some(wrapper), id);
        self.bump_structure(wrapper);
        self.bump_structure(host);
        let _ = self.commit(
            before,
            vec![
                DocChange::TreeSpliced {
                    parent: wrapper,
                    before: leaf_prev,
                    removed: vec![id],
                    inserted: Vec::new(),
                },
                DocChange::TreeSpliced {
                    parent: host,
                    before: Some(wrapper),
                    removed: Vec::new(),
                    inserted: vec![id],
                },
            ],
        );
        Some(Caret {
            block: id.index,
            offset: 0,
        })
    }

    pub(crate) fn try_break_commonmark(&mut self, id: NodeId, offset: usize) -> Option<Caret> {
        let kind = self.arena.get(id).map(|n| n.kind)?;
        if kind != BlockKind::Paragraph {
            return None;
        }
        let display = self.display(id).to_string();
        let off = floor_char_boundary(&display, offset.min(display.len()));
        let before = self.revision;
        let (changes, caret) = self.rewrite_text(id, off..off, "\n");
        let _ = self.commit(before, changes);
        Some(Caret {
            block: id.index,
            offset: caret,
        })
    }

    pub(crate) fn try_commit_block_quote(
        &mut self,
        id: NodeId,
        display_caret: usize,
    ) -> Option<Caret> {
        let kind = self.arena.get(id).map(|n| n.kind)?;
        if kind != BlockKind::Paragraph {
            return None;
        }
        let parent = self.arena.get(id).and_then(|n| n.parent)?;
        let old_source = self.leaf_source(id).to_string();
        let old_extra = self.extra(id);
        let source = syntax::normalize_source(&old_source);
        if !syntax::is_quote_commit(source) {
            return None;
        }
        let frag = load_markdown(
            &super::bind::with_definitions(source, &self.reference_definitions),
            editor_options(),
        );
        let lead = bind::quote_lead_paragraph(&frag)?;
        let prev = self.arena.get(id).and_then(|n| n.prev_sibling);
        let old_revision = self.arena.get(id).map(|n| n.content_revision).unwrap_or(1);
        let before = self.revision;
        let (display, inner, runs, constructs) = match lead {
            Some(lead) => (
                frag.display(lead).to_string(),
                syntax::normalize_source(frag.leaf_source(lead)).to_string(),
                self.remap_runs(&frag, lead),
                frag.recorded_constructs(lead).map(|c| c.to_vec()),
            ),
            None => (String::new(), String::new(), Vec::new(), Some(Vec::new())),
        };
        let s2d = bind::source_to_display_map(source, &self.reference_definitions);
        let caret = if s2d.last().copied().unwrap_or(0) == display.len() {
            bind::source_to_display(&s2d, display_caret.min(source.len()))
        } else {
            display_caret.min(display.len())
        };
        let caret = floor_char_boundary(&display, caret);
        self.ensure_leaf_text(id);
        if let Some(leaf) = self.texts.get_mut(id.text_id()) {
            let collapsed = bind::bind_map(
                &inner,
                &display,
                BlockKind::Paragraph,
                &self.reference_definitions,
            )
            .1;
            leaf.set_projected(display, inner.clone(), runs, constructs);
            leaf.s2d = collapsed;
        }
        let quote = self.alloc_container(BlockKind::BlockQuote);
        self.arena.detach(id);
        self.arena.append_child(quote, id);
        self.arena.insert_after(parent, prev, quote);
        let new_revision = self.bump_content(id);
        self.bump_structure(parent);
        self.bump_structure(quote);
        let _ = self.commit(
            before,
            vec![
                DocChange::text(
                    id,
                    old_revision,
                    new_revision,
                    0..old_source.len() as u32,
                    old_source,
                    inner,
                ),
                self.attrs_change(id, BlockKind::Paragraph, old_extra),
                DocChange::TreeSpliced {
                    parent,
                    before: prev,
                    removed: vec![id],
                    inserted: vec![quote],
                },
            ],
        );
        Some(Caret {
            block: id.index,
            offset: caret,
        })
    }
}
