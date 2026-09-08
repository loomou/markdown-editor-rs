use crate::block::{BlockId, BlockKind};
use crate::document::history::History;
use crate::document::{Caret, ChangeSet, Command, Document, Sel, TableLoc, TableStep};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

pub use crate::document::{
    FocusBias, RevealedImage, RevealedMath, floor_char_boundary, next_char_boundary,
    prev_char_boundary,
};

static NEXT_DOC_ID: AtomicU64 = AtomicU64::new(0);

pub struct Doc {
    pub document: Document,
    pub source_path: Option<PathBuf>,
    id: u64,
    saved_edit: u64,
    edit_gen: u64,
    history: History,
    keep_trailing_blank: bool,
}

pub type Cursor = Caret;

impl Doc {
    pub fn identity(&self) -> u64 {
        self.id
    }

    pub fn new(document: Document) -> Self {
        Self::with_path(document, None)
    }

    pub fn with_path(mut document: Document, source_path: Option<PathBuf>) -> Self {
        document.reset_changes();
        Doc {
            document,
            source_path,
            id: NEXT_DOC_ID.fetch_add(1, Ordering::Relaxed),
            saved_edit: 0,
            edit_gen: 0,
            history: History::new(),
            keep_trailing_blank: false,
        }
    }

    pub fn enable_trailing_blank(&mut self) {
        self.keep_trailing_blank = true;
        self.document.ensure_trailing_blank();
        self.document.reset_changes();
    }

    pub fn is_dirty(&self) -> bool {
        self.edit_gen != self.saved_edit
    }

    pub fn edit_gen(&self) -> u64 {
        self.edit_gen
    }

    pub fn mark_saved(&mut self, edit_gen: u64) {
        if self.edit_gen == edit_gen {
            self.saved_edit = edit_gen;
        }
    }

    pub fn mark_unsaved(&mut self) {
        self.saved_edit = self.edit_gen.wrapping_sub(1);
    }

    fn touch_edit(&mut self, before: u64, delta: Option<&ChangeSet>) {
        let edited = match delta {
            Some(delta) => delta.records_edit(),
            None => self.document.revision() != before,
        };
        if edited {
            self.edit_gen = self.edit_gen.saturating_add(1);
        }
    }

    pub fn take_changes(&mut self) -> ChangeSet {
        self.document.take_changes()
    }

    pub fn text(&self, id: BlockId) -> Option<&str> {
        self.document.text_of(id)
    }

    pub fn caret_text(&self, id: BlockId) -> Option<&str> {
        let nid = self.document.live_id(id)?;
        Some(self.document.caret_text(nid))
    }

    pub fn copy_markdown(&self, sel: Sel) -> String {
        self.document.copy_markdown(sel)
    }

    pub fn kind(&self, id: BlockId) -> Option<BlockKind> {
        self.document.kind(id)
    }

    pub fn block_edit(&self) -> Option<BlockId> {
        self.document.block_edit()
    }

    pub fn link_at(&self, at: Cursor) -> Option<&str> {
        let id = self.document.live_id(at.block)?;
        self.document.link_at(id, at.offset)
    }

    pub fn text_leaves(&self) -> Vec<BlockId> {
        self.document.text_leaves()
    }

    pub fn first_text_leaf(&self) -> Option<BlockId> {
        self.document.first_text_leaf()
    }

    pub fn for_each_text_leaf(&self, visit: impl FnMut(BlockId, &str) -> bool) {
        self.document.for_each_text_leaf(visit)
    }

    pub fn for_each_collapsed_text_leaf(&self, visit: impl FnMut(BlockId, &str) -> bool) {
        self.document.for_each_collapsed_text_leaf(visit)
    }

    fn apply_inner(&mut self, sel: Sel, cmd: Command) -> Cursor {
        if self.keep_trailing_blank {
            let c = crate::document::apply(&mut self.document, sel, cmd);
            let c = self.document.ensure_trailing_blank_at(c);
            self.document.clamp_live_caret(c)
        } else {
            crate::document::apply(&mut self.document, sel, cmd)
        }
    }

    pub fn apply(&mut self, sel: Sel, cmd: Command) -> Cursor {
        let before = self.document.revision();
        let change_start = self.document.pending_changes().changes.len();
        let _ = self.history.commit_compose();
        self.history.before_apply(&self.document, sel, &cmd);
        let c = self.apply_inner(sel, cmd);
        let delta = self.document.changes_since(change_start, before);
        self.history.after_apply(&self.document, &delta, c);
        self.touch_edit(before, Some(&delta));
        c
    }

    pub fn apply_marked(&mut self, sel: Sel, cmd: Command) -> Cursor {
        let before = self.document.revision();
        let change_start = self.document.pending_changes().changes.len();
        self.history.begin_compose(&self.document, sel);
        let c = self.apply_inner(sel, cmd);
        let delta = self.document.changes_since(change_start, before);
        self.history.note_compose(&self.document, &delta, c);
        self.touch_edit(before, Some(&delta));
        c
    }

    pub fn apply_ime_commit(&mut self, sel: Sel, cmd: Command) -> Cursor {
        let before = self.document.revision();
        if self.history.is_composing() {
            let change_start = self.document.pending_changes().changes.len();
            let c = self.apply_inner(sel, cmd);
            let delta = self.document.changes_since(change_start, before);
            self.history.note_compose(&self.document, &delta, c);
            let _ = self.history.commit_compose();
            self.touch_edit(before, Some(&delta));
            return c;
        }
        self.apply(sel, cmd)
    }

    pub fn abort_compose(&mut self) -> Option<Sel> {
        let before = self.document.revision();
        let sel = self.history.abort_compose(&mut self.document)?;
        self.touch_edit(before, None);
        Some(sel)
    }

    pub fn commit_compose(&mut self) {
        let _ = self.history.commit_compose();
    }

    pub fn is_composing(&self) -> bool {
        self.history.is_composing()
    }

    pub fn undo(&mut self) -> Option<Sel> {
        let before = self.document.revision();
        if let Some(sel) = self.history.abort_compose(&mut self.document) {
            self.touch_edit(before, None);
            return Some(sel);
        }
        let sel = self.history.undo(&mut self.document)?;
        self.touch_edit(before, None);
        Some(sel)
    }

    pub fn redo(&mut self) -> Option<Sel> {
        let before = self.document.revision();
        if let Some(sel) = self.history.abort_compose(&mut self.document) {
            self.touch_edit(before, None);
            return Some(sel);
        }
        let sel = self.history.redo(&mut self.document)?;
        self.touch_edit(before, None);
        Some(sel)
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    fn absorb_settle(&mut self, start: usize, before: u64, sel: Sel) {
        let delta = self.document.changes_since(start, before);
        self.touch_edit(before, Some(&delta));
        self.history.absorb_side_effect(&self.document, sel, delta);
    }

    pub fn retarget_focus(&mut self, caret: Cursor) -> Cursor {
        let before = self.document.revision();
        let start = self.document.pending_changes().changes.len();
        let c = self.document.retarget_inline_focus(caret);
        self.absorb_settle(start, before, Sel::collapsed(c));
        c
    }

    pub fn retarget_focus_biased(&mut self, caret: Cursor, bias: FocusBias) -> Cursor {
        let before = self.document.revision();
        let start = self.document.pending_changes().changes.len();
        let c = self.document.retarget_inline_focus_biased(caret, bias);
        self.absorb_settle(start, before, Sel::collapsed(c));
        c
    }

    pub fn retarget_focus_without_block_edit(&mut self, caret: Cursor, bias: FocusBias) -> Cursor {
        let before = self.document.revision();
        let start = self.document.pending_changes().changes.len();
        let c = self
            .document
            .retarget_inline_focus_without_block_edit(caret, bias);
        self.absorb_settle(start, before, Sel::collapsed(c));
        c
    }

    pub fn retarget_focus_range(
        &mut self,
        anchor: Cursor,
        head: Cursor,
        bias: FocusBias,
    ) -> (Cursor, Cursor) {
        let before = self.document.revision();
        let start = self.document.pending_changes().changes.len();
        let out = self
            .document
            .retarget_inline_focus_range(anchor, head, bias);
        self.absorb_settle(
            start,
            before,
            Sel {
                anchor: out.0,
                head: out.1,
            },
        );
        out
    }

    pub fn collapsed_text(&self, id: BlockId) -> Option<&str> {
        self.document.collapsed_text_of(id)
    }

    pub fn revealed_image(&self) -> Option<RevealedImage<'_>> {
        self.document.revealed_image()
    }

    pub fn revealed_math(&self) -> Option<RevealedMath<'_>> {
        self.document.revealed_math()
    }

    pub fn visual_offset(&self, id: BlockId, collapsed: usize) -> usize {
        let Some(nid) = self.document.live_id(id) else {
            return collapsed;
        };
        self.document.collapsed_to_visual(nid, collapsed)
    }

    pub fn visual_range(
        &self,
        id: BlockId,
        collapsed: std::ops::Range<usize>,
    ) -> std::ops::Range<usize> {
        let Some(nid) = self.document.live_id(id) else {
            return collapsed;
        };
        self.document.collapsed_range_to_visual(nid, collapsed)
    }

    pub fn sibling_leaf(&self, from: BlockId, delta: i32) -> Option<BlockId> {
        self.document.nth_text_leaf_from(from, delta)
    }

    pub fn cmp_reading_order(&self, a: BlockId, b: BlockId) -> std::cmp::Ordering {
        self.document.cmp_reading_order(a, b)
    }

    pub fn in_table(&self, block: BlockId) -> bool {
        crate::document::in_table(&self.document, block)
    }

    pub fn table_step(&self, caret: Caret, step: TableStep) -> Option<Caret> {
        crate::document::table_step(&self.document, caret, step)
    }

    pub fn table_loc(&self, block: BlockId) -> Option<TableLoc> {
        crate::document::table_loc(&self.document, block)
    }
}

#[cfg(test)]
mod tests {
    use super::{Cursor, Doc, FocusBias};
    use crate::block::BlockKind;
    use crate::document::{Command, Sel, editor_options, load_markdown};

    #[test]
    fn new_document_is_clean() {
        let doc = Doc::new(load_markdown("hi\n", editor_options()));
        assert!(!doc.is_dirty());
        assert_eq!(doc.saved_edit, doc.edit_gen);
    }

    #[test]
    fn identity_is_unique_per_document_and_survives_edits() {
        let mut a = Doc::new(load_markdown("a\n", editor_options()));
        let b = Doc::new(load_markdown("b\n", editor_options()));
        assert_ne!(a.identity(), b.identity());

        let before = a.identity();
        let rev_before = a.document.revision();
        let leaf = a.text_leaves()[0];
        a.apply(
            Sel::collapsed(Cursor {
                block: leaf,
                offset: 0,
            }),
            Command::Insert { text: "x".into() },
        );
        assert_eq!(a.identity(), before);
        assert_ne!(
            a.document.revision(),
            rev_before,
            "the fixture must really have edited"
        );
    }

    #[test]
    fn edit_dirties_until_matching_generation_is_marked_saved() {
        let mut doc = Doc::new(load_markdown("hi\n", editor_options()));
        let leaf = doc.text_leaves()[0];
        let before = doc.edit_gen();
        doc.apply(
            Sel::collapsed(Cursor {
                block: leaf,
                offset: 0,
            }),
            Command::Insert { text: "x".into() },
        );
        let after = doc.edit_gen();
        assert!(doc.is_dirty());
        doc.mark_saved(before);
        assert!(doc.is_dirty());
        doc.mark_saved(after);
        assert!(!doc.is_dirty());
    }

    #[test]
    fn mark_unsaved_dirties_a_freshly_loaded_document() {
        let mut doc = Doc::new(load_markdown("hi\n", editor_options()));
        assert!(!doc.is_dirty());
        doc.mark_unsaved();
        assert!(doc.is_dirty());
        let saved = doc.edit_gen();
        doc.mark_saved(saved);
        assert!(!doc.is_dirty());
    }

    #[test]
    fn revealing_inline_marks_does_not_dirty() {
        let mut doc = Doc::new(load_markdown("# hi\n\npara\n", editor_options()));
        let heading = doc.text_leaves()[0];
        let _ = doc.retarget_focus(Cursor {
            block: heading,
            offset: 0,
        });
        assert!(!doc.is_dirty());
    }

    #[test]
    fn visual_range_excludes_closing_marks_while_visual_offset_takes_them() {
        let mut doc = Doc::new(load_markdown("a**b**c\n", editor_options()));
        let leaf = doc.text_leaves()[0];
        let _ = doc.retarget_focus(Cursor {
            block: leaf,
            offset: 1,
        });
        assert_eq!(
            doc.text(leaf).unwrap(),
            "a**b**c",
            "the fixture must really be revealed"
        );
        assert_eq!(
            doc.visual_offset(leaf, 2),
            6,
            "insertion-point semantics: right after the trailing **"
        );
        assert_eq!(
            doc.visual_range(leaf, 1..2),
            3..4,
            "range semantics: just the b"
        );

        let plain = Doc::new(load_markdown("ab\n", editor_options()));
        let leaf = plain.text_leaves()[0];
        assert_eq!(plain.visual_range(leaf, 0..2), 0..2);
    }

    #[test]
    fn equivalent_text_commands_do_not_dirty_the_document() {
        for (start, end, replacement) in [(2, 2, ""), (0, 5, "hello")] {
            let mut doc = Doc::new(load_markdown("hello\n", editor_options()));
            let leaf = doc.text_leaves()[0];
            doc.apply(
                Sel {
                    anchor: Cursor {
                        block: leaf,
                        offset: start,
                    },
                    head: Cursor {
                        block: leaf,
                        offset: end,
                    },
                },
                Command::Insert {
                    text: replacement.into(),
                },
            );
            assert_eq!(doc.document.to_markdown(), "hello\n");
            assert_eq!(
                doc.edit_gen(),
                0,
                "generation must not move: {start}..{end}"
            );
            assert!(!doc.is_dirty());
            assert!(!doc.can_undo());
        }
        for cmd in [Command::Outdent, Command::DeleteBackward] {
            let mut doc = Doc::new(load_markdown("hello\n", editor_options()));
            let leaf = doc.text_leaves()[0];
            doc.apply(
                Sel::collapsed(Cursor {
                    block: leaf,
                    offset: 0,
                }),
                cmd.clone(),
            );
            assert!(!doc.is_dirty(), "{cmd:?}");
            assert!(!doc.can_undo(), "{cmd:?}");
        }
    }

    #[test]
    fn focus_settle_that_changes_content_advances_the_generation() {
        for wrapper in 0..4 {
            let mut doc = Doc::new(load_markdown("![alt](u)\n\ntail\n", editor_options()));
            let leaves = doc.text_leaves();
            let image = leaves[0];
            let _ = doc.retarget_focus(Cursor {
                block: image,
                offset: 0,
            });
            let end = doc.caret_text(image).unwrap().len();
            doc.apply(
                Sel {
                    anchor: Cursor {
                        block: image,
                        offset: 0,
                    },
                    head: Cursor {
                        block: image,
                        offset: end,
                    },
                },
                Command::Insert { text: "".into() },
            );
            let snapshot = doc.document.write_snapshot().to_markdown();
            let stale = doc.edit_gen();
            let target = Cursor {
                block: leaves[1],
                offset: 0,
            };
            match wrapper {
                0 => {
                    let _ = doc.retarget_focus(target);
                }
                1 => {
                    let _ = doc.retarget_focus_biased(target, FocusBias::Neutral);
                }
                2 => {
                    let _ = doc.retarget_focus_without_block_edit(target, FocusBias::Neutral);
                }
                _ => {
                    let _ = doc.retarget_focus_range(target, target, FocusBias::Neutral);
                }
            }
            assert_eq!(snapshot, "![]()\n\ntail\n", "wrapper={wrapper}");
            assert_eq!(doc.document.to_markdown(), "tail\n", "wrapper={wrapper}");
            assert_eq!(
                doc.kind(image),
                Some(BlockKind::Paragraph),
                "wrapper={wrapper}"
            );
            assert_eq!(doc.edit_gen(), stale + 1, "wrapper={wrapper}");
            doc.mark_saved(stale);
            assert!(
                doc.is_dirty(),
                "old snapshot must not mark the settled doc saved: wrapper={wrapper}"
            );
            doc.mark_saved(doc.edit_gen());
            assert!(!doc.is_dirty(), "wrapper={wrapper}");
        }
    }

    #[test]
    fn trailing_blank_on_editor_docs_is_clean_and_not_serialized() {
        let mut doc = Doc::new(load_markdown("# hi\n", editor_options()));
        doc.enable_trailing_blank();
        let last = *doc.text_leaves().last().expect("last");
        assert_eq!(doc.kind(last), Some(BlockKind::Paragraph));
        assert_eq!(doc.text(last), Some(""));
        assert!(!doc.is_dirty());
        assert_eq!(doc.document.to_markdown(), "# hi\n");
    }

    #[test]
    fn select_all_delete_collapses_to_a_single_blank() {
        let mut doc = Doc::new(load_markdown("a\n\nb\n\nc\n", editor_options()));
        doc.enable_trailing_blank();
        let leaves = doc.text_leaves();
        assert_eq!(leaves.len(), 4);
        let first = leaves[0];
        let last_content = leaves[2];
        let end = doc.caret_text(last_content).unwrap_or("").len();
        let _ = doc.apply(
            Sel {
                anchor: Cursor {
                    block: first,
                    offset: 0,
                },
                head: Cursor {
                    block: last_content,
                    offset: end,
                },
            },
            Command::DeleteBackward,
        );
        let left: Vec<_> = doc
            .text_leaves()
            .iter()
            .map(|&id| doc.text(id).unwrap_or("").to_string())
            .collect();
        assert_eq!(left, vec!["".to_string()], "{left:?}");
    }

    #[test]
    fn enter_on_empty_editor_doc_stays_live_and_accepts_text() {
        let mut doc = Doc::new(load_markdown("", editor_options()));
        doc.enable_trailing_blank();
        let leaf = doc.first_text_leaf().expect("leaf");
        let after_break = doc.apply(
            Sel::collapsed(Cursor {
                block: leaf,
                offset: 0,
            }),
            Command::Break,
        );
        assert!(doc.document.live_id(after_break.block).is_some());
        assert_eq!(doc.text_leaves().len(), 2);
        let again = doc.apply(Sel::collapsed(after_break), Command::Break);
        assert!(doc.document.live_id(again.block).is_some());
        assert_eq!(doc.text_leaves().len(), 3);
        let typed = doc.apply(Sel::collapsed(again), Command::Insert { text: "x".into() });
        assert_eq!(doc.text(typed.block), Some("x"));
        assert!(doc.document.live_id(typed.block).is_some());
        assert_eq!(doc.text(leaf), Some(""));
    }

    #[test]
    fn enter_after_text_then_again_stays_live() {
        let mut doc = Doc::new(load_markdown("hello\n", editor_options()));
        doc.enable_trailing_blank();
        let leaf = doc.text_leaves()[0];
        let end = doc.caret_text(leaf).unwrap().len();
        let mid = doc.apply(
            Sel::collapsed(Cursor {
                block: leaf,
                offset: end,
            }),
            Command::Break,
        );
        assert!(doc.document.live_id(mid.block).is_some());
        assert_eq!(doc.text(leaf), Some("hello"));
        assert_eq!(doc.text_leaves().len(), 2);
        let again = doc.apply(Sel::collapsed(mid), Command::Break);
        assert!(doc.document.live_id(again.block).is_some());
        assert_eq!(doc.text_leaves().len(), 3);
        assert_eq!(doc.text(leaf), Some("hello"));
        let typed = doc.apply(Sel::collapsed(again), Command::Insert { text: "x".into() });
        assert_eq!(doc.text(typed.block), Some("x"));
        assert_eq!(doc.text(leaf), Some("hello"));
    }

    fn leaf_texts(doc: &Doc) -> Vec<String> {
        doc.text_leaves()
            .iter()
            .map(|&id| doc.text(id).unwrap_or("").to_string())
            .collect()
    }

    fn four_empty_lines() -> (Doc, Vec<u32>) {
        let mut doc = Doc::new(load_markdown("", editor_options()));
        doc.enable_trailing_blank();
        let mut c = Cursor {
            block: doc.first_text_leaf().expect("leaf"),
            offset: 0,
        };
        for _ in 0..3 {
            c = doc.apply(Sel::collapsed(c), Command::Break);
        }
        let leaves = doc.text_leaves();
        assert_eq!(leaves.len(), 4);
        assert!(leaf_texts(&doc).iter().all(|t| t.is_empty()));
        (doc, leaves)
    }

    #[test]
    fn typing_in_middle_empty_keeps_blank_lines_around_it() {
        let (mut doc, leaves) = four_empty_lines();
        let _ = doc.apply(
            Sel::collapsed(Cursor {
                block: leaves[1],
                offset: 0,
            }),
            Command::Insert { text: "x".into() },
        );
        assert_eq!(leaf_texts(&doc), vec!["", "x", "", ""]);
    }

    #[test]
    fn wrapping_middle_empty_as_list_keeps_blank_lines_after() {
        let (mut doc, leaves) = four_empty_lines();
        let _ = doc.apply(
            Sel::collapsed(Cursor {
                block: leaves[1],
                offset: 0,
            }),
            Command::Insert { text: "- ".into() },
        );
        let parent = doc
            .document
            .live_id(leaves[1])
            .and_then(|id| doc.document.arena.get(id).and_then(|n| n.parent))
            .and_then(|p| doc.document.arena.get(p).map(|n| n.kind));
        assert_eq!(parent, Some(BlockKind::ListItem));
        assert_eq!(leaf_texts(&doc), vec!["", "", "", ""]);
    }

    #[test]
    fn heading_in_middle_empty_keeps_blank_lines_after() {
        let (mut doc, leaves) = four_empty_lines();
        let _ = doc.apply(
            Sel::collapsed(Cursor {
                block: leaves[1],
                offset: 0,
            }),
            Command::Insert { text: "# ".into() },
        );
        assert_eq!(doc.kind(leaves[1]), Some(BlockKind::Heading(1)));
        let texts = leaf_texts(&doc);
        assert_eq!(texts[0], "");
        assert!(
            texts[2..].iter().all(|t| t.is_empty()) && texts.len() >= 4,
            "blank lines after the heading were swallowed: {texts:?}"
        );
    }

    fn doc_of(md: &str) -> Doc {
        Doc::new(load_markdown(md, editor_options()))
    }

    #[test]
    fn ends_have_no_sibling() {
        let doc = doc_of("first\n\nsecond\n\nthird\n");
        let leaves = doc.text_leaves();
        assert_eq!(leaves.len(), 3);
        assert_eq!(doc.sibling_leaf(leaves[0], -1), None);
        assert_eq!(doc.sibling_leaf(leaves[2], 1), None);
        assert_eq!(doc.sibling_leaf(leaves[0], 1), Some(leaves[1]));
        assert_eq!(doc.sibling_leaf(leaves[2], -1), Some(leaves[1]));
    }

    #[test]
    fn zero_delta_is_identity() {
        let doc = doc_of("a\n\nb\n");
        for leaf in doc.text_leaves() {
            assert_eq!(doc.sibling_leaf(leaf, 0), Some(leaf));
        }
    }

    #[test]
    fn crosses_container_boundaries() {
        let doc = doc_of("head\n\n> inside one\n>\n> inside two\n\ntail\n");
        let leaves = doc.text_leaves();
        assert_eq!(
            leaves
                .iter()
                .map(|&b| doc.text(b).unwrap_or_default())
                .collect::<Vec<_>>(),
            vec!["head", "inside one", "inside two", "tail"],
        );
        assert_eq!(doc.sibling_leaf(leaves[0], 1), Some(leaves[1]));
        assert_eq!(doc.sibling_leaf(leaves[1], 1), Some(leaves[2]));
        assert_eq!(doc.sibling_leaf(leaves[2], 1), Some(leaves[3]));
        assert_eq!(doc.sibling_leaf(leaves[3], -1), Some(leaves[2]));
        assert_eq!(doc.sibling_leaf(leaves[1], -1), Some(leaves[0]));
    }

    #[test]
    fn walks_through_table_cells() {
        let doc = doc_of("before\n\n| a | b |\n| --- | --- |\n| c | d |\n\nafter\n");
        let leaves = doc.text_leaves();
        assert_eq!(leaves.len(), 6, "before + 4 cells + after");
        for i in 0..leaves.len() - 1 {
            assert_eq!(
                doc.sibling_leaf(leaves[i], 1),
                Some(leaves[i + 1]),
                "step {i}"
            );
        }
    }

    #[test]
    fn dead_block_has_no_sibling() {
        let doc = doc_of("a\n\nb\n");
        let stale = doc.text_leaves().iter().max().copied().unwrap_or(0) + 1000;
        assert_eq!(doc.sibling_leaf(stale, 1), None);
        assert_eq!(doc.sibling_leaf(stale, 0), None);
    }

    #[test]
    fn first_leaf_matches_the_table() {
        for md in ["", "---\n", "para\n", "> q\n", "| a |\n| --- |\n| b |\n"] {
            let doc = doc_of(md);
            assert_eq!(
                doc.first_text_leaf(),
                doc.text_leaves().first().copied(),
                "{md:?}"
            );
        }
    }

    #[test]
    fn cancelled_composition_rejects_its_in_flight_save() {
        let mut doc = doc_of("hello\n");
        let leaf = doc.text_leaves()[0];
        let _ = doc.apply_marked(
            Sel::collapsed(Cursor {
                block: leaf,
                offset: 5,
            }),
            Command::Insert { text: "x".into() },
        );
        let saved = doc.document.write_snapshot();
        let saved_generation = doc.edit_gen();
        assert!(doc.abort_compose().is_some());
        assert_ne!(doc.document.to_markdown(), saved.to_markdown());
        doc.mark_saved(saved_generation);
        assert!(
            doc.is_dirty(),
            "cancelled text differs from the saved snapshot but is marked clean"
        );
    }

    #[test]
    fn editing_reference_label_respects_visual_offset() {
        let mut doc = doc_of("[label][ref]\n\n[ref]: /target\n");
        let leaf = doc.first_text_leaf().expect("leaf");
        let caret = doc.retarget_focus(Cursor {
            block: leaf,
            offset: 1,
        });
        let _ = doc.apply(Sel::collapsed(caret), Command::Insert { text: "X".into() });
        let markdown = doc.document.to_markdown();
        let reloaded = load_markdown(&markdown, editor_options());
        assert_eq!(
            reloaded.text_of(reloaded.first_text_leaf().unwrap()),
            Some("lXabel"),
            "markdown={markdown:?}"
        );
    }

    #[test]
    fn repeated_edits_in_a_reference_label_keep_the_projected_text() {
        let mut doc = doc_of("[label][ref]\n\n[ref]: /target\n");
        let leaf = doc.first_text_leaf().expect("leaf");
        let one = doc.retarget_focus(Cursor {
            block: leaf,
            offset: 1,
        });
        let after_x = doc.apply(Sel::collapsed(one), Command::Insert { text: "X".into() });
        let _ = doc.apply(
            Sel::collapsed(after_x),
            Command::Insert { text: "Y".into() },
        );
        assert_eq!(
            doc.collapsed_text(leaf),
            Some("lXYabel"),
            "projection degraded to literal source: {:?}",
            doc.collapsed_text(leaf)
        );
        let markdown = doc.document.to_markdown();
        let reloaded = load_markdown(&markdown, editor_options());
        assert_eq!(
            reloaded.text_of(reloaded.first_text_leaf().unwrap()),
            Some("lXYabel"),
            "markdown={markdown:?}"
        );
    }
}
