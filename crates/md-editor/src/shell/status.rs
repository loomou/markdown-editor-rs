use super::Shell;
use crate::ui::theme::{MONO_FONT, STATUS_BAR_H, ShellTheme};
use gpui::prelude::FluentBuilder;
use gpui::{Div, Entity, ParentElement, Styled, div, px};
use md_core::doc::{Cursor, Doc, floor_char_boundary};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StatusCounts {
    pub(super) line: usize,
    pub(super) column: usize,
    pub(super) words: usize,
    pub(super) chars: usize,
}

pub(super) struct StatusCache {
    identity: u64,
    revision: u64,
    cursor: Cursor,
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
    pub(super) fn refresh_with(
        &mut self,
        identity: u64,
        revision: u64,
        cursor: Cursor,
        full: impl FnOnce() -> StatusCounts,
        line_col: impl FnOnce() -> (usize, usize),
    ) -> StatusCounts {
        if self.identity == identity && self.revision == revision && self.cursor == cursor {
            return self.counts;
        }
        let counts = if self.identity == identity && self.revision == revision {
            let (line, column) = line_col();
            StatusCounts {
                line,
                column,
                words: self.counts.words,
                chars: self.counts.chars,
            }
        } else {
            full()
        };
        self.identity = identity;
        self.revision = revision;
        self.cursor = cursor;
        self.counts = counts;
        counts
    }

    pub(super) fn get(&mut self, doc: &Doc, cursor: Cursor) -> StatusCounts {
        self.refresh_with(
            doc.identity(),
            doc.document.revision(),
            cursor,
            || status_counts(doc, cursor),
            || status_line_column(doc, cursor),
        )
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
            .child(div().flex_1())
            .when(!self.show_settings, |bar| {
                bar.child(self.outline_toggle(t, this))
            })
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
