use super::EditorView;
use gpui::{Context, KeyDownEvent, Window};
use md_core::document::Command;
use md_core::document::{
    TABLE_INSERT_MAX_COLS, TABLE_INSERT_MAX_ROWS, TABLE_INSERT_MIN_COLS, TABLE_INSERT_MIN_ROWS,
    TableOp,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InsertTableField {
    Rows,
    Cols,
}

#[derive(Clone, Debug)]
pub(crate) struct InsertTableState {
    pub rows: String,
    pub cols: String,
    pub field: InsertTableField,
    pub selected: bool,
}

impl InsertTableState {
    fn new() -> Self {
        Self {
            rows: "2".into(),
            cols: "2".into(),
            field: InsertTableField::Rows,
            selected: true,
        }
    }

    pub(crate) fn can_create(&self) -> bool {
        self.parsed().is_some()
    }

    fn parsed(&self) -> Option<(usize, usize)> {
        Some((
            parse_dim(&self.rows, TABLE_INSERT_MIN_ROWS, TABLE_INSERT_MAX_ROWS)?,
            parse_dim(&self.cols, TABLE_INSERT_MIN_COLS, TABLE_INSERT_MAX_COLS)?,
        ))
    }

    fn focused_mut(&mut self) -> &mut String {
        match self.field {
            InsertTableField::Rows => &mut self.rows,
            InsertTableField::Cols => &mut self.cols,
        }
    }

    fn push_digit(&mut self, d: char) {
        let selected = self.selected;
        let value = self.focused_mut();
        if selected {
            value.clear();
            value.push(d);
        } else if value.len() < 3 {
            value.push(d);
        }
        self.selected = false;
    }

    fn backspace(&mut self) {
        if self.selected {
            self.focused_mut().clear();
        } else {
            self.focused_mut().pop();
        }
        self.selected = false;
    }

    fn toggle_field(&mut self) {
        self.field = match self.field {
            InsertTableField::Rows => InsertTableField::Cols,
            InsertTableField::Cols => InsertTableField::Rows,
        };
        self.selected = true;
    }
}

fn parse_dim(s: &str, min: usize, max: usize) -> Option<usize> {
    let n = s.parse::<usize>().ok()?;
    (min..=max).contains(&n).then_some(n)
}

impl EditorView {
    pub(crate) fn open_insert_table(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.unsaved_nav.is_some() || self.save_conflict.is_some() || self.caret_in_table() {
            return;
        }
        if self.search_open {
            if let Some(find) = self.find_bar.as_ref().and_then(|w| w.upgrade()) {
                find.update(cx, |f, cx| {
                    f.dismiss(cx);
                });
            }
            self.clear_search();
        }
        self.insert_table = Some(InsertTableState::new());
        window.focus(&self.insert_table_focus);
        cx.notify();
    }

    pub(crate) fn close_insert_table(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.insert_table.take().is_none() {
            return;
        }
        window.focus(&self.focus);
        cx.notify();
    }

    pub(crate) fn confirm_insert_table(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some((rows, cols)) = self
            .insert_table
            .as_ref()
            .and_then(InsertTableState::parsed)
        else {
            return;
        };
        self.insert_table = None;
        self.apply_cmd(Command::Table(TableOp::Insert { rows, cols }));
        self.note_edit(cx);
        window.focus(&self.focus);
        cx.notify();
    }

    pub(crate) fn set_insert_table_field(
        &mut self,
        field: InsertTableField,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(state) = self.insert_table.as_mut() else {
            return;
        };
        state.field = field;
        state.selected = true;
        cx.notify();
    }

    pub(crate) fn on_insert_table_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        if self.insert_table.is_none() {
            return false;
        }
        if ev.is_held {
            return true;
        }
        let m = &ev.keystroke.modifiers;
        if crate::ui::chord::has_chord(m) {
            return true;
        }
        let key = ev.keystroke.key.as_str();
        match key {
            "escape" => self.close_insert_table(window, cx),
            "enter" if !m.shift => self.confirm_insert_table(window, cx),
            "tab" => {
                if let Some(state) = self.insert_table.as_mut() {
                    state.toggle_field();
                    cx.notify();
                }
            }
            "backspace" => {
                if let Some(state) = self.insert_table.as_mut() {
                    state.backspace();
                    cx.notify();
                }
            }
            _ => {
                if let Some(d) = digit_key(key)
                    && let Some(state) = self.insert_table.as_mut()
                {
                    state.push_digit(d);
                    cx.notify();
                }
            }
        }
        true
    }
}

fn digit_key(key: &str) -> Option<char> {
    let mut chars = key.chars();
    let c = chars.next()?;
    if chars.next().is_none() && c.is_ascii_digit() {
        return Some(c);
    }
    key.strip_prefix("numpad")
        .and_then(|rest| rest.chars().next())
        .filter(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::InsertTableState;

    #[test]
    fn first_digit_replaces_selected_default() {
        let mut state = InsertTableState::new();
        assert_eq!(state.parsed(), Some((2, 2)));
        state.push_digit('3');
        assert_eq!(state.rows, "3");
        state.push_digit('0');
        assert_eq!(state.rows, "30");
        state.toggle_field();
        state.push_digit('4');
        assert_eq!(state.cols, "4");
        assert_eq!(state.parsed(), Some((30, 4)));
    }

    #[test]
    fn empty_and_out_of_range_are_invalid() {
        let mut state = InsertTableState::new();
        state.rows.clear();
        assert!(state.parsed().is_none());
        state.rows = "0".into();
        assert!(state.parsed().is_none());
        state.rows = "1".into();
        assert!(state.parsed().is_none());
        state.rows = "2".into();
        state.cols = "0".into();
        assert!(state.parsed().is_none());
        state.cols = "33".into();
        assert!(state.parsed().is_none());
        state.cols = "1".into();
        state.rows = "101".into();
        assert!(state.parsed().is_none());
        state.rows = "100".into();
        state.cols = "32".into();
        assert_eq!(state.parsed(), Some((100, 32)));
    }

    #[test]
    fn backspace_clears_selected_then_pops() {
        let mut state = InsertTableState::new();
        state.backspace();
        assert_eq!(state.rows, "");
        state.push_digit('4');
        state.push_digit('2');
        state.backspace();
        assert_eq!(state.rows, "4");
    }
}
