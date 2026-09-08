use super::{FocusBias, FocusQuery, InlineFocus, RawConstruct, project_focus};
use crate::block::TextEditStrategy;
use crate::document::bind::{display_to_source_inner, source_to_display};
use crate::document::{Caret, Document, NodeId};

impl Document {
    pub(crate) fn retarget_inline_focus(&mut self, caret: Caret) -> Caret {
        self.retarget_inline_focus_biased(caret, FocusBias::Neutral)
    }

    pub fn retarget_inline_focus_biased(&mut self, caret: Caret, bias: FocusBias) -> Caret {
        self.retarget_caret(caret, bias, true)
    }

    pub(crate) fn retarget_inline_focus_without_block_edit(
        &mut self,
        caret: Caret,
        bias: FocusBias,
    ) -> Caret {
        self.retarget_caret(caret, bias, false)
    }

    fn retarget_caret(&mut self, caret: Caret, bias: FocusBias, enter_block_edit: bool) -> Caret {
        let Some(id) = self.live_id(caret.block) else {
            self.set_block_edit(None);
            return caret;
        };
        let kind = self.arena.get(id).map(|n| n.kind);
        if enter_block_edit {
            self.set_block_edit(kind.filter(|k| super::bind::is_block_edit(*k)).map(|_| id));
        } else if self.block_edit != Some(id) {
            self.set_block_edit(None);
        }
        if !kind.is_some_and(|kind| kind.text_edit_strategy() == TextEditStrategy::Phrasing) {
            if let Some(prev) = self.focus.take() {
                self.emit_focus_text(prev.node);
            }
            return caret;
        }
        let (collapsed_caret, src_hint) = self.focus_input(id, caret.offset);
        let caret_out = self.apply_inline_focus(id, collapsed_caret, src_hint, bias, true);
        Caret {
            block: caret.block,
            offset: caret_out,
        }
    }

    fn sync_display_math_block_edit(&mut self, id: NodeId) {
        let want = self.phrasing_is_only_display_math(id);
        if want {
            self.set_block_edit(Some(id));
        } else if self.block_edit == Some(id) {
            self.set_block_edit(None);
        }
    }

    fn phrasing_is_only_display_math(&self, id: NodeId) -> bool {
        let Some(f) = self.focus.as_ref().filter(|f| f.node == id) else {
            return false;
        };
        if !f.math.as_ref().is_some_and(|m| m.display_math) {
            return false;
        }
        let src = self.leaf_source(id);
        src.get(..f.span.start).unwrap_or("").trim().is_empty()
            && src.get(f.span.end..).unwrap_or("").trim().is_empty()
    }

    pub(crate) fn retarget_inline_focus_range(
        &mut self,
        anchor: Caret,
        head: Caret,
        bias: FocusBias,
    ) -> (Caret, Caret) {
        if anchor == head {
            let c = self.retarget_inline_focus_biased(head, bias);
            return (c, c);
        }
        let head_id = self.live_id(head.block);
        let kind = head_id.and_then(|id| self.arena.get(id).map(|n| n.kind));
        let keep_block_edit = anchor.block == head.block
            && head_id.is_some_and(|id| self.block_edit == Some(id))
            && kind.is_some_and(super::bind::is_block_edit);
        if keep_block_edit {
            return (anchor, head);
        }
        let focus_is_endpoint = self.focus.as_ref().is_some_and(|focus| {
            focus.node.index == anchor.block || focus.node.index == head.block
        });
        if focus_is_endpoint {
            return (anchor, head);
        }
        if let Some(prev) = self.focus.take() {
            self.emit_focus_text(prev.node);
        }
        if anchor.block != head.block {
            self.set_block_edit(None);
        }
        (anchor, head)
    }

    fn focus_input(&self, id: NodeId, visual_off: usize) -> (usize, Option<usize>) {
        let visual = self.display(id);
        let off = super::floor_char_boundary(visual, visual_off.min(visual.len()));
        let s2d = self.visual_s2d(id);
        let src = if s2d.last().copied() == Some(visual.len()) {
            display_to_source_inner(&s2d, off)
        } else {
            off.min(self.leaf_source(id).len())
        };
        let already = self.focus.as_ref().is_some_and(|f| f.node == id);
        if already {
            (source_to_display(&self.collapsed_s2d(id), src), Some(src))
        } else {
            (off, None)
        }
    }

    fn focus_constructs(&mut self, id: NodeId, source: &str) -> Vec<RawConstruct> {
        let cached = self
            .arena
            .get(id)
            .and_then(|n| n.text)
            .and_then(|tid| self.texts.get(tid))
            .and_then(|leaf| leaf.constructs.clone());
        if let Some(recorded) = cached {
            return recorded;
        }
        let fresh = super::raw_constructs(source, &self.reference_definitions);
        if let Some(leaf) = self
            .arena
            .get(id)
            .and_then(|n| n.text)
            .and_then(|tid| self.texts.get_mut(tid))
        {
            leaf.constructs = Some(fresh.clone());
        }
        fresh
    }

    pub(crate) fn apply_inline_focus(
        &mut self,
        id: NodeId,
        collapsed_caret: usize,
        src_hint: Option<usize>,
        bias: FocusBias,
        emit: bool,
    ) -> usize {
        let source = self.leaf_source(id).to_string();
        let collapsed_display = self.collapsed_display(id).to_string();
        let collapsed_runs = self.collapsed_runs(id).to_vec();
        let collapsed_s2d = self.collapsed_s2d(id);
        let constructs = self.focus_constructs(id, &source);
        let next = project_focus(
            &source,
            &collapsed_display,
            &collapsed_runs,
            &collapsed_s2d,
            &constructs,
            FocusQuery {
                caret: collapsed_caret,
                src_hint,
                bias,
            },
        );
        let prev_span = self.focus.as_ref().and_then(|f| {
            if f.node == id {
                Some(f.span.clone())
            } else {
                None
            }
        });
        let other = self
            .focus
            .as_ref()
            .and_then(|f| if f.node != id { Some(f.node) } else { None });
        if let Some(other) = other {
            self.focus = None;
            if emit {
                self.emit_focus_text(other);
            }
        }
        match next {
            Some(proj) => {
                let same = prev_span.as_ref() == Some(&proj.span)
                    && self
                        .focus
                        .as_ref()
                        .is_some_and(|f| f.display == proj.display);
                let caret = proj.caret;
                self.focus = Some(InlineFocus {
                    node: id,
                    display: proj.display,
                    runs: proj.runs,
                    s2d: proj.s2d,
                    span: proj.span,
                    image: proj.image,
                    math: proj.math,
                });
                if emit && !same {
                    self.emit_focus_text(id);
                }
                self.sync_display_math_block_edit(id);
                caret
            }
            None => {
                let had = prev_span.is_some() || self.focus.as_ref().is_some_and(|f| f.node == id);
                self.focus = None;
                if emit && had {
                    self.emit_focus_text(id);
                }
                self.sync_display_math_block_edit(id);
                collapsed_caret.min(collapsed_display.len())
            }
        }
    }
}
