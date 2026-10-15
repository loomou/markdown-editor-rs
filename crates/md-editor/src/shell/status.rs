use super::Shell;
use crate::ui::icons;
use crate::ui::theme::{MONO_FONT, STATUS_BAR_H, ShellTheme};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, ClickEvent, Context, Div, Entity, InteractiveElement, MouseDownEvent, ParentElement,
    Stateful, StatefulInteractiveElement, Styled, Window, div, px, svg,
};
use md_core::doc::{Cursor, Doc, floor_char_boundary};
use md_core::document::{ChangeSet, DocChange, NodeId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StatusCounts {
    pub(super) line: usize,
    pub(super) column: usize,
    pub(super) words: usize,
    pub(super) chars: usize,
}

#[cfg(test)]
std::thread_local! {
    pub(super) static LEAF_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    pub(super) static RECOUNTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(super) fn leaf_visits() -> usize {
    LEAF_VISITS.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(super) fn recounts() -> usize {
    RECOUNTS.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(super) fn reset_leaf_visits() {
    LEAF_VISITS.with(|visits| visits.set(0));
    RECOUNTS.with(|count| count.set(0));
}

fn note_leaf_visit() {
    #[cfg(test)]
    LEAF_VISITS.with(|visits| visits.set(visits.get() + 1));
}

fn count_text(text: &str) -> (usize, usize) {
    (text.split_whitespace().count(), text.chars().count())
}

#[derive(Default)]
struct LeafCounts {
    words: Vec<u32>,
    chars: Vec<u32>,
    generation: Vec<u32>,
    total_words: usize,
    total_chars: usize,
}

impl LeafCounts {
    fn totals(&self) -> (usize, usize) {
        (self.total_words, self.total_chars)
    }

    fn cached(&self, node: NodeId) -> Option<(usize, usize)> {
        let slot = node.index as usize;
        if self.generation.get(slot).copied() != Some(node.generation.get()) {
            return None;
        }
        Some((self.words[slot] as usize, self.chars[slot] as usize))
    }

    fn store(&mut self, node: NodeId, words: usize, chars: usize) {
        let slot = node.index as usize;
        if self.generation.len() <= slot {
            self.words.resize(slot + 1, 0);
            self.chars.resize(slot + 1, 0);
            self.generation.resize(slot + 1, 0);
        }
        self.words[slot] = words as u32;
        self.chars[slot] = chars as u32;
        self.generation[slot] = node.generation.get();
    }

    fn adopt(&mut self, node: NodeId, words: usize, chars: usize) {
        let (old_words, old_chars) = self.cached(node).unwrap_or((0, 0));
        self.store(node, words, chars);
        self.total_words = self.total_words + words - old_words;
        self.total_chars = self.total_chars + chars - old_chars;
    }

    fn recount(&mut self, doc: &Doc) {
        #[cfg(test)]
        {
            RECOUNTS.with(|count| count.set(count.get() + 1));
        }
        self.words.clear();
        self.chars.clear();
        self.generation.clear();
        self.total_words = 0;
        self.total_chars = 0;
        self.adopt_subtree(doc, doc.document.root);
    }

    fn adopt_subtree(&mut self, doc: &Doc, root: NodeId) -> bool {
        let arena = &doc.document.arena;
        if arena.get(root).is_none() {
            return false;
        }
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let Some(node) = arena.get(id) else {
                continue;
            };
            if node.kind.is_text_leaf() {
                note_leaf_visit();
                let (words, chars) = count_text(doc.document.display(id));
                self.adopt(id, words, chars);
            }
            let mut child = node.last_child;
            while let Some(c) = child {
                stack.push(c);
                child = arena.get(c).and_then(|n| n.prev_sibling);
            }
        }
        true
    }

    fn forget(&mut self, node: NodeId) -> bool {
        let Some((words, chars)) = self.cached(node) else {
            return false;
        };
        self.total_words -= words;
        self.total_chars -= chars;
        self.store(node, 0, 0);
        true
    }

    fn fold(&mut self, doc: &Doc, pending: &ChangeSet) -> bool {
        let mut forgotten: Vec<u32> = Vec::new();
        for change in &pending.changes {
            match change {
                DocChange::TreeSpliced { removed, .. } => {
                    for node in removed {
                        if !self.forget(*node) {
                            return false;
                        }
                        forgotten.push(node.index);
                    }
                }
                DocChange::DocumentReplaced
                | DocChange::AttrsChanged { .. }
                | DocChange::ReferenceDefsChanged { .. } => return false,
                _ => {}
            }
        }
        for change in &pending.changes {
            match change {
                DocChange::TextChanged { node, .. } => {
                    if doc.document.arena.get(*node).is_none() {
                        if !forgotten.contains(&node.index) {
                            return false;
                        }
                        continue;
                    }
                    if !self.adopt_subtree(doc, *node) {
                        return false;
                    }
                }
                DocChange::TreeSpliced { inserted, .. } => {
                    for node in inserted {
                        if !self.adopt_subtree(doc, *node) {
                            return false;
                        }
                    }
                }
                _ => {}
            }
        }
        true
    }
}

pub(super) struct StatusCache {
    identity: u64,
    revision: u64,
    cursor: Cursor,
    folded: u64,
    leaves: LeafCounts,
    pub(super) counts: StatusCounts,
}

impl Default for StatusCache {
    fn default() -> Self {
        Self {
            identity: u64::MAX,
            revision: u64::MAX,
            cursor: Cursor {
                block: 0,
                offset: 0,
            },
            folded: u64::MAX,
            leaves: LeafCounts::default(),
            counts: StatusCounts {
                line: 1,
                column: 1,
                words: 0,
                chars: 0,
            },
        }
    }
}

impl StatusCache {
    pub(super) fn get(&mut self, doc: &Doc, cursor: Cursor) -> StatusCounts {
        let identity = doc.identity();
        let revision = doc.document.revision();
        if self.identity == identity && self.revision == revision && self.cursor == cursor {
            return self.counts;
        }
        let (words, chars) = self.totals(doc, identity, revision);
        let (line, column) = status_line_column(doc, cursor);
        let counts = StatusCounts {
            line,
            column,
            words,
            chars,
        };
        self.identity = identity;
        self.revision = revision;
        self.cursor = cursor;
        self.counts = counts;
        counts
    }

    fn totals(&mut self, doc: &Doc, identity: u64, revision: u64) -> (usize, usize) {
        if self.identity != identity {
            self.leaves.recount(doc);
            self.folded = revision;
            return self.leaves.totals();
        }
        if self.revision != revision {
            let pending = doc.document.pending_changes();
            let folded = pending.before_revision == self.folded
                && pending.after_revision == revision
                && self.leaves.fold(doc, pending);
            if !folded {
                self.leaves.recount(doc);
            }
            self.folded = revision;
        }
        self.leaves.totals()
    }
}

fn caret_prefix_and_column(doc: &Doc, cursor: Cursor) -> (&str, usize) {
    let text = doc.text(cursor.block).unwrap_or("");
    let offset = floor_char_boundary(text, cursor.offset);
    let before = &text[..offset];
    let column = before
        .rsplit('\n')
        .next()
        .map_or(1, |line| line.chars().count() + 1);
    (before, column)
}

pub(super) fn status_line_column(doc: &Doc, cursor: Cursor) -> (usize, usize) {
    let (before, column) = caret_prefix_and_column(doc, cursor);
    let mut line = 1;
    doc.for_each_text_leaf(|block, leaf| {
        if block == cursor.block {
            line += before.bytes().filter(|byte| *byte == b'\n').count();
            return false;
        }
        line += leaf.bytes().filter(|byte| *byte == b'\n').count() + 1;
        true
    });
    (line, column)
}

#[cfg(test)]
pub(super) fn status_counts(doc: &Doc, cursor: Cursor) -> StatusCounts {
    let (before, column) = caret_prefix_and_column(doc, cursor);
    let mut line = 1;
    let mut words = 0;
    let mut chars = 0;
    let mut before_cursor = true;
    doc.for_each_text_leaf(|block, leaf| {
        words += leaf.split_whitespace().count();
        chars += leaf.chars().count();
        if before_cursor {
            if block == cursor.block {
                line += before.bytes().filter(|byte| *byte == b'\n').count();
                before_cursor = false;
            } else {
                line += leaf.bytes().filter(|byte| *byte == b'\n').count() + 1;
            }
        }
        true
    });
    StatusCounts {
        line,
        column,
        words,
        chars,
    }
}

impl Shell {
    pub(super) fn status_bar(
        &self,
        t: ShellTheme,
        this: Entity<Self>,
        status: Option<StatusCounts>,
    ) -> Div {
        div()
            .h(px(STATUS_BAR_H))
            .flex_none()
            .flex()
            .items_center()
            .px(px(8.))
            .bg(t.bar_bg)
            .border_t_1()
            .border_color(t.border)
            .text_size(px(11.5))
            .text_color(t.text_muted)
            .on_any_mouse_down(|_: &MouseDownEvent, window: &mut Window, _: &mut App| {
                window.prevent_default();
            })
            .when(!self.show_settings, |bar| {
                bar.child(self.outline_toggle(t, this.clone()))
                    .child(self.reading_toggle(t, this))
            })
            .child(div().flex_1())
            .child(match status {
                None => div()
                    .flex()
                    .items_center()
                    .gap(px(1.))
                    .child(self.status_item(concat!("v", env!("CARGO_PKG_VERSION")))),
                Some(StatusCounts {
                    line,
                    column,
                    words,
                    chars,
                }) => div()
                    .flex()
                    .items_center()
                    .gap(px(1.))
                    .child(self.status_item(&md_i18n::fmt::status_line(line)))
                    .child(self.status_item(&md_i18n::fmt::status_column(column)))
                    .child(self.status_sep(t))
                    .child(self.status_item(&md_i18n::fmt::status_words(words)))
                    .child(self.status_item(&md_i18n::fmt::status_chars(chars))),
            })
    }

    pub(super) fn set_reading(&mut self, on: bool, cx: &mut Context<'_, Self>) {
        if self.reading == on {
            return;
        }
        self.reading = on;
        self.editor
            .update(cx, |editor, cx| editor.set_reading(on, cx));
        cx.notify();
    }

    fn reading_toggle(&self, t: ShellTheme, this: Entity<Self>) -> Stateful<Div> {
        let icon_color = if self.reading { t.text } else { t.text_muted };
        let icon = if self.reading {
            icons::READING
        } else {
            icons::PENCIL
        };
        div()
            .id("btn-reading")
            .debug_selector(|| "btn-reading".into())
            .px(px(8.))
            .py(px(2.))
            .rounded(px(4.))
            .hover(move |s| s.bg(t.hover))
            .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                this.update(cx, |shell, cx| {
                    let on = !shell.reading;
                    shell.set_reading(on, cx);
                });
            })
            .child(svg().size(px(13.)).path(icon).text_color(icon_color))
    }

    fn status_item(&self, label: &str) -> Div {
        div()
            .px(px(8.))
            .py(px(2.))
            .rounded(px(4.))
            .flex()
            .items_center()
            .font_family(MONO_FONT)
            .text_size(px(11.))
            .child(label.to_owned())
    }

    fn status_sep(&self, t: ShellTheme) -> Div {
        div().w(px(1.)).h(px(14.)).mx(px(5.)).bg(t.border_variant)
    }
}
