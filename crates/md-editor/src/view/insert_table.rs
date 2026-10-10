use super::EditorView;
use crate::ui::text_input::{
    InputMark, InputStyle, KeyOutcome, TextInput, TextInputElement, TextInputHost,
};
use crate::ui::theme::{DLG_W, MONO_FONT, RADIUS, ShellTheme};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, AppContext, ClickEvent, Context, CursorStyle, Div, Entity, EntityInputHandler,
    FocusHandle, Font, FontFeatures, FontStyle, FontWeight, InteractiveElement, IntoElement,
    KeyDownEvent, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement, Render, Stateful, StatefulInteractiveElement, Styled, UTF16Selection,
    WeakEntity, Window, div, px, rgba,
};
use md_core::document::Command;
use md_core::document::{
    TABLE_INSERT_MAX_COLS, TABLE_INSERT_MAX_ROWS, TABLE_INSERT_MIN_COLS, TABLE_INSERT_MIN_ROWS,
    TableOp,
};
use md_i18n::{Key, t as t18};
use std::ops::Range;
use std::time::Duration;

const DIM_MAX_LEN: usize = 3;
const DEFAULT_DIM: &str = "2";
const LABEL_W: f32 = 72.0;
const FIELD_W: f32 = 88.0;
const FIELD_H: f32 = 28.0;
const FIELD_TEXT_SIZE: f32 = 13.0;
const FIELD_LINE_HEIGHT: f32 = 16.0;
const FIELD_CARET_WIDTH: f64 = 1.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InsertTableField {
    Rows,
    Cols,
}

impl InsertTableField {
    fn other(self) -> Self {
        match self {
            InsertTableField::Rows => InsertTableField::Cols,
            InsertTableField::Cols => InsertTableField::Rows,
        }
    }

    fn label(self) -> Key {
        match self {
            InsertTableField::Rows => Key::DlgTableRows,
            InsertTableField::Cols => Key::DlgTableCols,
        }
    }

    fn element_id(self) -> &'static str {
        match self {
            InsertTableField::Rows => "insert-table-rows",
            InsertTableField::Cols => "insert-table-cols",
        }
    }
}

fn digits_only(text: &str) -> String {
    text.chars().filter(char::is_ascii_digit).collect()
}

fn parse_dim(s: &str, min: usize, max: usize) -> Option<usize> {
    let n = s.parse::<usize>().ok()?;
    (min..=max).contains(&n).then_some(n)
}

fn mono_font() -> Font {
    Font {
        family: MONO_FONT.into(),
        features: FontFeatures::default(),
        fallbacks: None,
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    }
}

fn shell_theme(editor: &WeakEntity<EditorView>, cx: &App) -> Option<ShellTheme> {
    let editor = editor.upgrade()?;
    Some(ShellTheme::from_app(&editor.read(cx).state.theme.app))
}

pub(crate) struct InsertTableFieldInput {
    field: InsertTableField,
    input: TextInput,
    focus: FocusHandle,
    editor: WeakEntity<EditorView>,
}

impl InsertTableFieldInput {
    fn new(
        field: InsertTableField,
        caret_blink_ms: u64,
        editor: &Entity<EditorView>,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let mut input = TextInput::constrained(digits_only, DIM_MAX_LEN);
        input.set_text(DEFAULT_DIM);
        input.start_blink(
            Duration::from_millis(caret_blink_ms),
            cx,
            |this: &mut Self| Some(&mut this.input),
        );
        cx.observe(editor, |_, _, cx| cx.notify()).detach();
        Self {
            field,
            input,
            focus: cx.focus_handle(),
            editor: editor.downgrade(),
        }
    }

    pub(crate) fn focus(&self) -> &FocusHandle {
        &self.focus
    }

    fn text(&self) -> &str {
        self.input.text()
    }

    fn nav_key(&mut self, key: &str, m: &Modifiers, cx: &mut App) -> KeyOutcome {
        self.input.nav_key(key, m, cx)
    }

    fn select_all(&mut self, cx: &mut Context<'_, Self>) {
        self.input.select_all();
        self.input.mouse_up();
        cx.notify();
    }

    fn activate(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        window.focus(&self.focus, cx);
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn set_dim(&mut self, value: &str) {
        self.input.set_text(value);
    }
}

impl EntityInputHandler for InsertTableFieldInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<String> {
        Some(self.input.text_for_utf16(range_utf16, adjusted))
    }

    fn selected_text_range(
        &mut self,
        _ignore: bool,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<UTF16Selection> {
        Some(self.input.selected_utf16())
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<Range<usize>> {
        self.input.marked_utf16()
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<'_, Self>) {
        self.input.clear_mark();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let range = self.input.edit_range(range_utf16);
        self.input.replace(range, text, InputMark::Plain);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let range = self.input.edit_range(range_utf16);
        self.input.replace(range, new_text, InputMark::Marked);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: gpui::Bounds<gpui::Pixels>,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.input.ime_bounds(element_bounds)
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<gpui::Pixels>,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<usize> {
        Some(self.input.utf16_index_for_position(point))
    }
}

impl TextInputHost for InsertTableFieldInput {
    fn input(&self) -> Option<&TextInput> {
        Some(&self.input)
    }

    fn input_mut(&mut self) -> Option<&mut TextInput> {
        Some(&mut self.input)
    }

    fn input_focus(&self) -> FocusHandle {
        self.focus.clone()
    }

    fn caret_live(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }
}

impl Render for InsertTableFieldInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let Some(t) = shell_theme(&self.editor, cx) else {
            return div().into_any_element();
        };
        let focused = self.focus.is_focused(window);
        let host = cx.entity();
        div()
            .flex()
            .items_center()
            .gap(px(12.))
            .pt(px(14.))
            .child(
                div()
                    .w(px(LABEL_W))
                    .text_size(px(12.5))
                    .text_color(t.text_muted)
                    .child(t18(self.field.label())),
            )
            .child(
                div()
                    .id(self.field.element_id())
                    .h(px(FIELD_H))
                    .w(px(FIELD_W))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .rounded(px(RADIUS))
                    .border_1()
                    .border_color(if focused { t.accent } else { t.border })
                    .bg(t.panel_bg)
                    .cursor(CursorStyle::IBeam)
                    .track_focus(&self.focus)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, ev: &MouseDownEvent, window, cx| {
                            this.activate(window, cx);
                            this.input
                                .mouse_down(ev.position, ev.click_count, ev.modifiers.shift);
                            cx.notify();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                        if this.input.mouse_move(ev.position) {
                            cx.notify();
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseUpEvent, _, _| {
                            this.input.mouse_up();
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseUpEvent, _, _| {
                            this.input.mouse_up();
                        }),
                    )
                    .child(TextInputElement::new(
                        host,
                        InputStyle {
                            font: mono_font(),
                            font_size: px(FIELD_TEXT_SIZE),
                            line_height: px(FIELD_LINE_HEIGHT),
                            text: t.text,
                            placeholder_color: t.text_disabled,
                            placeholder: "".into(),
                            selection: t.selected_bg,
                            ime: t.selected_bg,
                            caret: t.accent,
                            caret_width: FIELD_CARET_WIDTH,
                        },
                    )),
            )
            .into_any_element()
    }
}

pub(crate) struct InsertTableDialog {
    rows: Entity<InsertTableFieldInput>,
    cols: Entity<InsertTableFieldInput>,
    editor: WeakEntity<EditorView>,
}

impl InsertTableDialog {
    pub(crate) fn new(
        editor: &Entity<EditorView>,
        caret_blink_ms: u64,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let rows = cx.new(|cx| {
            InsertTableFieldInput::new(InsertTableField::Rows, caret_blink_ms, editor, cx)
        });
        let cols = cx.new(|cx| {
            InsertTableFieldInput::new(InsertTableField::Cols, caret_blink_ms, editor, cx)
        });
        cx.observe(&rows, |_, _, cx| cx.notify()).detach();
        cx.observe(&cols, |_, _, cx| cx.notify()).detach();
        Self {
            rows,
            cols,
            editor: editor.downgrade(),
        }
    }

    fn field_of(&self, field: InsertTableField) -> &Entity<InsertTableFieldInput> {
        match field {
            InsertTableField::Rows => &self.rows,
            InsertTableField::Cols => &self.cols,
        }
    }

    fn focused(&self, window: &Window, cx: &App) -> InsertTableField {
        if self.rows.read(cx).focus().is_focused(window) {
            InsertTableField::Rows
        } else {
            InsertTableField::Cols
        }
    }

    fn focus_field(
        &mut self,
        field: InsertTableField,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let focus = self.field_of(field).read(cx).focus().clone();
        window.focus(&focus, cx);
        let target = self.field_of(field).clone();
        target.update(cx, |input, cx| input.select_all(cx));
    }

    fn toggle_field(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let next = self.focused(window, cx).other();
        self.focus_field(next, window, cx);
    }

    fn parsed(&self, cx: &App) -> Option<(usize, usize)> {
        Some((
            parse_dim(
                self.rows.read(cx).text(),
                TABLE_INSERT_MIN_ROWS,
                TABLE_INSERT_MAX_ROWS,
            )?,
            parse_dim(
                self.cols.read(cx).text(),
                TABLE_INSERT_MIN_COLS,
                TABLE_INSERT_MAX_COLS,
            )?,
        ))
    }

    fn can_create(&self, cx: &App) -> bool {
        self.parsed(cx).is_some()
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let _ = self
            .editor
            .update(cx, |editor, cx| editor.close_insert_table(window, cx));
    }

    pub(crate) fn confirm(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some((rows, cols)) = self.parsed(cx) else {
            return;
        };
        let _ = self.editor.update(cx, |editor, cx| {
            editor.insert_table_sized(rows, cols, window, cx)
        });
    }

    fn on_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        if ev.is_held {
            return true;
        }
        let m = ev.keystroke.modifiers;
        let key = ev.keystroke.key.as_str();
        if !crate::ui::chord::has_chord(&m) {
            match key {
                "escape" => {
                    self.close(window, cx);
                    return true;
                }
                "enter" => {
                    if !m.shift {
                        self.confirm(window, cx);
                    }
                    return true;
                }
                "tab" => {
                    self.toggle_field(window, cx);
                    return true;
                }
                _ => {}
            }
        }
        let target = self.field_of(self.focused(window, cx)).clone();
        match target.update(cx, |input, cx| input.nav_key(key, &m, cx)) {
            KeyOutcome::Ignored => crate::ui::chord::has_chord(&m),
            KeyOutcome::Moved | KeyOutcome::Edited => {
                cx.notify();
                true
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn set_dim(
        &mut self,
        field: InsertTableField,
        value: &str,
        cx: &mut Context<'_, Self>,
    ) {
        let target = self.field_of(field).clone();
        target.update(cx, |input, cx| {
            input.set_dim(value);
            cx.notify();
        });
    }
}

impl Render for InsertTableDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let Some(t) = shell_theme(&self.editor, cx) else {
            return div().into_any_element();
        };
        let can_create = self.can_create(cx);
        let this = cx.entity();
        div()
            .id("insert-table-mask")
            .absolute()
            .top(px(0.))
            .left(px(0.))
            .right(px(0.))
            .bottom(px(0.))
            .flex()
            .items_center()
            .justify_center()
            .bg(t.overlay)
            .occlude()
            .on_mouse_down(MouseButton::Left, {
                let this = this.clone();
                move |_, window, cx| {
                    this.update(cx, |dialog, cx| dialog.close(window, cx));
                    cx.stop_propagation();
                }
            })
            .on_key_down({
                let this = this.clone();
                move |ev: &KeyDownEvent, window, cx| {
                    let handled = this.update(cx, |dialog, cx| dialog.on_key(ev, window, cx));
                    if handled {
                        cx.stop_propagation();
                    }
                }
            })
            .child(
                div()
                    .id("insert-table-dialog")
                    .w(px(DLG_W))
                    .flex()
                    .flex_col()
                    .px(px(16.))
                    .pt(px(22.))
                    .pb(px(16.))
                    .bg(t.editor_bg)
                    .border_1()
                    .border_color(t.border)
                    .rounded(px(RADIUS))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight(600.0))
                            .text_color(t.text)
                            .child(t18(Key::DlgInsertTable)),
                    )
                    .child(self.rows.clone())
                    .child(self.cols.clone())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .pt(px(22.))
                            .child(div().flex_1())
                            .child(dialog_btn(
                                t,
                                "insert-table-cancel",
                                Key::DlgCancel,
                                false,
                                true,
                                this.clone(),
                            ))
                            .child(dialog_btn(
                                t,
                                "insert-table-create",
                                Key::DlgCreate,
                                true,
                                can_create,
                                this,
                            )),
                    ),
            )
            .into_any_element()
    }
}

fn dialog_btn(
    t: ShellTheme,
    id: &'static str,
    label: Key,
    primary: bool,
    enabled: bool,
    this: Entity<InsertTableDialog>,
) -> Stateful<Div> {
    let fg = if !enabled {
        t.text_disabled
    } else if primary {
        t.on_accent
    } else {
        t.text_muted
    };
    let border = if primary {
        rgba(0x00000000).into()
    } else {
        t.border_variant
    };
    div()
        .id(id)
        .h(px(26.))
        .px(px(12.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(RADIUS))
        .text_size(px(12.5))
        .text_color(fg)
        .border_1()
        .border_color(border)
        .when(primary, |d| d.font_weight(FontWeight(500.0)))
        .when(primary && enabled, |d| d.bg(t.accent))
        .when(!primary && enabled, |d| {
            d.hover(move |s| s.bg(t.hover).text_color(t.text))
        })
        .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            if !enabled {
                return;
            }
            this.update(cx, |dialog, cx| {
                if primary {
                    dialog.confirm(window, cx);
                } else {
                    dialog.close(window, cx);
                }
            });
        })
        .child(t18(label))
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
        let editor = cx.entity();
        let caret_blink_ms = self.state.theme.paint.caret_blink_ms as u64;
        let dialog = cx.new(|cx| InsertTableDialog::new(&editor, caret_blink_ms, cx));
        dialog.update(cx, |dialog, cx| {
            dialog.focus_field(InsertTableField::Rows, window, cx);
        });
        self.insert_table = Some(dialog);
        cx.notify();
    }

    pub(crate) fn close_insert_table(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.insert_table.take().is_none() {
            return;
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(crate) fn insert_table_sized(
        &mut self,
        rows: usize,
        cols: usize,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.insert_table.take().is_none() {
            return;
        }
        self.apply_cmd(Command::Table(TableOp::Insert { rows, cols }));
        self.note_edit(cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::parse_dim;

    #[test]
    fn empty_and_out_of_range_are_invalid() {
        assert!(parse_dim("", 2, 100).is_none());
        assert!(parse_dim("0", 2, 100).is_none());
        assert!(parse_dim("1", 2, 100).is_none());
        assert!(parse_dim("2", 2, 100).is_some());
        assert!(parse_dim("101", 2, 100).is_none());
        assert!(parse_dim("100", 2, 100).is_some());
    }
}
