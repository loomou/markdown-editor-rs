use super::arena::NodeId;
use super::{Caret, ChangeSet, Command, Document, Sel};
use std::collections::VecDeque;

const MAX_DEPTH: usize = 128;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stroke {
    InsertWord,
    InsertWs,
    DeleteBack,
    DeleteFwd,
}

struct Entry {
    sel_before: Sel,
    sel_after: Sel,
    delta: ChangeSet,
}

struct LastEdit {
    stroke: Stroke,
    caret: Caret,
}

struct Compose {
    sel_before: Sel,
    sel_after: Sel,
    delta: ChangeSet,
}

pub(crate) struct History {
    undo: VecDeque<Entry>,
    redo: VecDeque<Entry>,
    last: Option<LastEdit>,
    pending: Option<Stroke>,
    pending_sel: Option<Sel>,
    compose: Option<Compose>,
}

impl History {
    pub(crate) fn new() -> Self {
        History {
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            last: None,
            pending: None,
            pending_sel: None,
            compose: None,
        }
    }

    pub(crate) fn is_composing(&self) -> bool {
        self.compose.is_some()
    }

    pub(crate) fn begin_compose(&mut self, doc: &Document, sel: Sel) {
        if self.compose.is_some() {
            return;
        }
        let sel = collapsed_sel(doc, sel);
        self.compose = Some(Compose {
            sel_before: sel,
            sel_after: sel,
            delta: ChangeSet::empty(0),
        });
        self.last = None;
        self.pending = None;
    }

    pub(crate) fn note_compose(&mut self, doc: &mut Document, cs: &ChangeSet, caret: Caret) {
        let Some(compose) = self.compose.as_mut() else {
            return;
        };
        let caret = doc.visual_caret_to_collapsed(caret);
        let inv = cs.invert();
        if !inv.records_edit() {
            compose.sel_after = Sel::collapsed(caret);
            return;
        }
        retain_refs(doc, &inv);
        compose.delta.prepend(inv);
        compose.sel_after = Sel::collapsed(caret);
    }

    pub(crate) fn abort_compose(&mut self, doc: &mut Document) -> Option<Sel> {
        let compose = self.compose.take()?;
        if !compose.delta.is_empty() && !doc.apply_changes(compose.delta.clone()) {
            self.compose = Some(compose);
            return None;
        }
        release_refs(doc, &compose.delta);
        self.last = None;
        self.pending = None;
        Some(compose.sel_before)
    }

    pub(crate) fn commit_compose(&mut self, doc: &mut Document) -> bool {
        let Some(compose) = self.compose.take() else {
            return false;
        };
        if compose.delta.records_edit() {
            self.push_undo_moved(
                doc,
                Entry {
                    sel_before: compose.sel_before,
                    sel_after: compose.sel_after,
                    delta: compose.delta,
                },
            );
            self.clear_redo(doc);
        } else {
            release_refs(doc, &compose.delta);
        }
        self.last = None;
        self.pending = None;
        true
    }

    pub(crate) fn before_apply(&mut self, doc: &Document, sel: Sel, cmd: &Command) {
        let sel = collapsed_sel(doc, sel);
        let stroke = stroke_of(cmd, sel);
        let join = matches!(
            (&self.last, stroke, sel),
            (Some(last), Some(s), sel)
                if last.stroke == s
                    && sel.anchor == sel.head
                    && sel.head == last.caret
        );
        self.pending = stroke;
        self.pending_sel = if join { None } else { Some(sel) };
    }

    pub(crate) fn after_apply(&mut self, doc: &mut Document, delta: &ChangeSet, caret: Caret) {
        let caret = doc.visual_caret_to_collapsed(caret);
        let stroke = self.pending.take();
        let sel_before = self.pending_sel.take();
        let inv = delta.invert();
        if !inv.records_edit() {
            return;
        }
        if let Some(sel_before) = sel_before {
            self.push_undo_fresh(
                doc,
                Entry {
                    sel_before,
                    sel_after: Sel::collapsed(caret),
                    delta: inv,
                },
            );
        } else if let Some(last) = self.undo.back_mut() {
            retain_refs(doc, &inv);
            last.delta.prepend(inv);
            last.sel_after = Sel::collapsed(caret);
        }
        self.clear_redo(doc);
        self.last = stroke.map(|stroke| LastEdit { stroke, caret });
    }

    pub(crate) fn absorb_side_effect(&mut self, doc: &mut Document, sel: Sel, delta: ChangeSet) {
        let inv = delta.invert();
        if !inv.records_edit() {
            return;
        }
        let sel = collapsed_sel(doc, sel);
        if let Some(compose) = self.compose.as_mut() {
            retain_refs(doc, &inv);
            compose.delta.prepend(inv);
        } else if let Some(last) = self.undo.back_mut() {
            retain_refs(doc, &inv);
            last.delta.prepend(inv);
        } else {
            self.push_undo_fresh(
                doc,
                Entry {
                    sel_before: sel,
                    sel_after: sel,
                    delta: inv,
                },
            );
        }
        self.clear_redo(doc);
    }

    pub(crate) fn undo(&mut self, doc: &mut Document) -> Option<Sel> {
        let entry = self.undo.pop_back()?;
        if !doc.apply_changes(entry.delta.clone()) {
            self.push_undo_moved(doc, entry);
            return None;
        }
        self.push_redo(
            doc,
            Entry {
                sel_before: entry.sel_before,
                sel_after: entry.sel_after,
                delta: entry.delta.invert(),
            },
        );
        let sel = entry.sel_before;
        self.last = None;
        self.pending = None;
        Some(sel)
    }

    pub(crate) fn redo(&mut self, doc: &mut Document) -> Option<Sel> {
        let entry = self.redo.pop_back()?;
        if !doc.apply_changes(entry.delta.clone()) {
            self.push_redo(doc, entry);
            return None;
        }
        self.push_undo_moved(
            doc,
            Entry {
                sel_before: entry.sel_before,
                sel_after: entry.sel_after,
                delta: entry.delta.invert(),
            },
        );
        let sel = entry.sel_after;
        self.last = None;
        self.pending = None;
        Some(sel)
    }

    pub(crate) fn can_undo(&self) -> bool {
        self.compose.is_some() || !self.undo.is_empty()
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn push_undo_fresh(&mut self, doc: &mut Document, entry: Entry) {
        retain_refs(doc, &entry.delta);
        self.push_undo_moved(doc, entry);
    }

    fn push_undo_moved(&mut self, doc: &mut Document, entry: Entry) {
        if self.undo.len() >= MAX_DEPTH
            && let Some(evicted) = self.undo.pop_front()
        {
            release_refs(doc, &evicted.delta);
        }
        self.undo.push_back(entry);
    }

    fn push_redo(&mut self, doc: &mut Document, entry: Entry) {
        if self.redo.len() >= MAX_DEPTH
            && let Some(evicted) = self.redo.pop_front()
        {
            release_refs(doc, &evicted.delta);
        }
        self.redo.push_back(entry);
    }

    fn clear_redo(&mut self, doc: &mut Document) {
        if self.redo.is_empty() {
            return;
        }
        for entry in self.redo.drain(..) {
            release_refs(doc, &entry.delta);
        }
        doc.reclaim(&self.collect_protected());
    }

    fn collect_protected(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut push = |cs: &ChangeSet| out.extend(cs.node_refs());
        for entry in &self.undo {
            push(&entry.delta);
        }
        for entry in &self.redo {
            push(&entry.delta);
        }
        if let Some(compose) = &self.compose {
            push(&compose.delta);
        }
        out
    }
}

fn retain_refs(doc: &mut Document, cs: &ChangeSet) {
    for id in cs.node_refs() {
        doc.arena.retain(id);
    }
}

fn release_refs(doc: &mut Document, cs: &ChangeSet) {
    for id in cs.node_refs() {
        doc.arena.release(id);
    }
}

fn collapsed_sel(doc: &Document, sel: Sel) -> Sel {
    Sel {
        anchor: doc.visual_caret_to_collapsed(sel.anchor),
        head: doc.visual_caret_to_collapsed(sel.head),
    }
}

fn stroke_of(cmd: &Command, sel: Sel) -> Option<Stroke> {
    let collapsed = sel.anchor == sel.head;
    match cmd {
        Command::Insert { text } if collapsed && !text.is_empty() && !text.contains('\n') => {
            if text.chars().all(char::is_whitespace) {
                Some(Stroke::InsertWs)
            } else if text.chars().any(char::is_whitespace) {
                None
            } else {
                Some(Stroke::InsertWord)
            }
        }
        Command::DeleteBackward if collapsed => Some(Stroke::DeleteBack),
        Command::DeleteForward if collapsed => Some(Stroke::DeleteFwd),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, History, MAX_DEPTH};
    use crate::block::BlockKind;
    use crate::doc::Doc;
    use crate::document::{
        Caret, ChangeSet, Command, DocChange, Sel, editor_options, load_markdown,
    };

    #[test]
    fn undo_redo_transfer_keeps_retained_counts_flat() {
        let mut doc = Doc::new(load_markdown("hello\n", editor_options()));
        let root = doc.document.root;
        let para = doc.document.arena.children(root).next().unwrap();

        doc.apply(
            Sel::collapsed(Caret {
                block: para.index,
                offset: 5,
            }),
            Command::Insert {
                text: "!".to_string(),
            },
        );
        assert_eq!(doc.document.arena.retained_count(para.index), 1);

        doc.undo();
        assert!(doc.can_redo());
        assert_eq!(doc.document.arena.retained_count(para.index), 1);
        doc.redo();
        assert_eq!(doc.document.arena.retained_count(para.index), 1);

        doc.apply(
            Sel::collapsed(Caret {
                block: para.index,
                offset: 6,
            }),
            Command::DeleteBackward,
        );
        assert!(!doc.can_redo());
        assert_eq!(doc.document.arena.retained_count(para.index), 2);

        doc.undo();
        doc.undo();
        assert!(!doc.can_undo());
        assert_eq!(doc.document.arena.retained_count(para.index), 2);
    }

    #[test]
    fn depth_truncation_releases_the_evicted_entry() {
        let mut doc = Doc::new(load_markdown("hello\n", editor_options()));
        let root = doc.document.root;
        let para = doc.document.arena.children(root).next().unwrap();
        let mut caret = Caret {
            block: para.index,
            offset: 5,
        };

        for i in 0..MAX_DEPTH + 2 {
            let text = if i % 2 == 0 { " " } else { "x" };
            caret = doc.apply(
                Sel::collapsed(caret),
                Command::Insert {
                    text: text.to_string(),
                },
            );
        }
        assert_eq!(
            doc.document.arena.retained_count(para.index),
            MAX_DEPTH as u32
        );
    }

    #[test]
    fn a_freshly_buried_slot_is_protected_by_its_own_entry() {
        let mut doc = Doc::new(load_markdown("one\n\ntwo\n", editor_options()));
        let leaves = doc.text_leaves();
        assert_eq!(leaves.len(), 2);

        let c = doc.apply(
            Sel::collapsed(Caret {
                block: leaves[0],
                offset: 3,
            }),
            Command::Insert {
                text: "!".to_string(),
            },
        );
        let undone = doc.undo().expect("undo the text edit");
        assert!(doc.can_redo());

        let merged = doc.apply(
            Sel::collapsed(Caret {
                block: leaves[1],
                offset: 0,
            }),
            Command::DeleteBackward,
        );
        assert_eq!(
            doc.document.arena.free_list_len(),
            0,
            "the slot buried by this very edit must not be reclaimed"
        );

        assert!(doc.undo().is_some());
        assert_eq!(doc.text_leaves().len(), 2);
        let _ = (c, undone, merged);
    }

    #[test]
    fn failed_undo_keeps_the_entry_and_does_not_create_redo() {
        let mut doc = load_markdown("a\n\nb\n", editor_options());
        let root = doc.root;
        let paragraphs = doc.arena.children(root).collect::<Vec<_>>();
        let quote = doc.arena.alloc(BlockKind::BlockQuote);
        doc.arena.append_child(root, quote);
        doc.arena.detach(paragraphs[1]);
        doc.arena.append_child(quote, paragraphs[1]);
        let revision = doc.revision();
        let sel = Sel::collapsed(Caret {
            block: paragraphs[0].index,
            offset: 0,
        });
        let mut history = History::new();
        history.push_undo_fresh(
            &mut doc,
            Entry {
                sel_before: sel,
                sel_after: sel,
                delta: ChangeSet {
                    before_revision: revision,
                    after_revision: revision.saturating_add(1),
                    changes: vec![DocChange::TreeSpliced {
                        parent: root,
                        before: Some(paragraphs[0]),
                        removed: vec![paragraphs[1]],
                        inserted: Vec::new(),
                    }],
                },
            },
        );
        let before = doc.to_markdown();

        assert_eq!(history.undo(&mut doc), None);
        assert!(history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(doc.revision(), revision);
        assert_eq!(doc.to_markdown(), before);
    }
}
