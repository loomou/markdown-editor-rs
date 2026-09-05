use super::{CursorMotion, EditorView};
use gpui::{ClipboardItem, Context};
use md_core::block::BlockKind;
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
        let mut leaves = self.state.doc.text_leaves();
        if leaves.len() > 1 {
            let last = *leaves.last().expect("last");
            if self.state.doc.kind(last) == Some(BlockKind::Paragraph)
                && self
                    .state
                    .doc
                    .caret_text(last)
                    .is_some_and(|t| t.is_empty())
            {
                leaves.pop();
            }
        }
        let (Some(&first), Some(&last)) = (leaves.first(), leaves.last()) else {
            return;
        };
        let follow = self.follow_caret;
        let end = self.state.doc.caret_text(last).map_or(0, |t| t.len());
        self.select_anchor = Some(Cursor {
            block: first,
            offset: 0,
        });
        self.place_cursor(
            Cursor {
                block: last,
                offset: end,
            },
            CursorMotion::Extend,
        );
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
