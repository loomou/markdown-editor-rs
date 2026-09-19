use super::{CursorMotion, EditorView, WellHeadHit};
use gpui::{ClipboardItem, Context};
use md_core::Px;
use md_core::block::{BlockId, BlockKind};
use md_core::doc::Cursor;
use md_core::document::{Command, PasteIntent, Sel};

impl EditorView {
    pub(crate) fn copy(&mut self, cx: &mut Context<'_, Self>) {
        let Some((anchor, head)) = self.state.selection else {
            return;
        };
        let md = self.state.doc.copy_markdown(Sel { anchor, head });
        if md.is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(md));
    }

    pub(super) fn copy_well_source(&mut self, block: BlockId, cx: &mut Context<'_, Self>) {
        let end = self.state.doc.caret_text(block).map_or(0, |t| t.len());
        let md = self.state.doc.copy_markdown(Sel {
            anchor: Cursor { block, offset: 0 },
            head: Cursor { block, offset: end },
        });
        if !md.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(md));
        }
        self.well_copy_done = Some(block);
        self.well_copy_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1500))
                .await;
            let _ = this.update(cx, |v, cx| {
                if v.well_copy_done == Some(block) {
                    v.well_copy_done = None;
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    pub(super) fn hover_well_copy(
        &mut self,
        heads: &[WellHeadHit],
        pos: (Px, Px),
        cx: &mut Context<'_, Self>,
    ) {
        let hit = heads
            .iter()
            .copied()
            .find(|h| h.contains(pos.0, pos.1))
            .map(|h| h.id);
        if self.well_copy_hover != hit {
            self.well_copy_hover = hit;
            cx.notify();
        }
    }

    pub(crate) fn cut(&mut self, cx: &mut Context<'_, Self>) {
        if self.state.selection.is_none() {
            return;
        }
        self.copy(cx);
        self.apply_cmd(Command::DeleteBackward);
        self.note_edit(cx);
    }

    pub(crate) fn paste(&mut self, cx: &mut Context<'_, Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|it| it.text()) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let intent = match self.paste_host() {
            Some(
                BlockKind::CodeBlock
                | BlockKind::Mermaid
                | BlockKind::Math
                | BlockKind::TableCell
                | BlockKind::Image,
            ) => PasteIntent::PlainText,
            _ => PasteIntent::IndependentFragment,
        };
        self.apply_cmd(Command::Paste { text, intent });
        self.note_edit(cx);
    }

    pub(crate) fn select_all(&mut self) {
        let Some(sel) = self.state.doc.whole_document_sel() else {
            return;
        };
        let follow = self.follow_caret;
        self.select_anchor = Some(sel.anchor);
        self.place_cursor(sel.head, CursorMotion::Extend);
        self.follow_caret = follow;
    }

    pub(super) fn paste_host(&self) -> Option<BlockKind> {
        let sel = self.editing_sel();
        if sel.anchor.block == sel.head.block {
            return self.state.doc.kind(sel.head.block);
        }
        let leaves = self.state.doc.text_leaves();
        let a = leaves.iter().position(|&b| b == sel.anchor.block)?;
        let h = leaves.iter().position(|&b| b == sel.head.block)?;
        let first = leaves[a.min(h)];
        match self.state.doc.kind(first) {
            Some(BlockKind::TableCell) => Some(BlockKind::Paragraph),
            other => other,
        }
    }
}
