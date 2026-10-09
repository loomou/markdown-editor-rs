use super::EditorView;
use crate::ui::text_input::{
    InputMark, InputStyle, KeyOutcome, TextInput, TextInputElement, TextInputHost,
};
use crate::ui::theme::{RADIUS, ShellTheme};
use gpui::{
    AppContext, Context, CursorStyle, EntityInputHandler, FocusHandle, InteractiveElement,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement, Render, Styled, UTF16Selection, WeakEntity, Window, div, px,
};
use md_content::gpui_theme::TypeRoleExt;
use md_core::block::BlockId;
use md_core::document::Command;
use std::ops::Range;
use std::time::Duration;

const PAD_X: f32 = 5.0;
const PAD_Y: f32 = 3.0;
const CARET_WIDTH: f64 = 1.5;

fn rect_with_padding(rect: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
    (
        rect.0 - PAD_X,
        rect.1 - PAD_Y,
        rect.2 + PAD_X * 2.0,
        rect.3 + PAD_Y * 2.0,
    )
}

pub(crate) struct CodeLangInput {
    input: TextInput,
    focus: FocusHandle,
    editor: WeakEntity<EditorView>,
    block: BlockId,
    token: u64,
    rect: (f32, f32, f32, f32),
    finished: bool,
}

impl CodeLangInput {
    pub(crate) fn new(
        editor: WeakEntity<EditorView>,
        block: BlockId,
        token: u64,
        rect: (f32, f32, f32, f32),
        text: String,
        caret_blink_ms: u64,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let mut input = TextInput::default();
        input.set_text(text);
        input.select_all();
        input.mouse_up();
        input.start_blink(
            Duration::from_millis(caret_blink_ms),
            cx,
            |this: &mut Self| Some(&mut this.input),
        );
        Self {
            input,
            focus: cx.focus_handle(),
            editor,
            block,
            token,
            rect,
            finished: false,
        }
    }

    pub(crate) fn focus_now(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        window.focus(&self.focus, cx);
        self.input.wake();
        cx.notify();
    }

    pub(crate) fn focus(&self) -> &FocusHandle {
        &self.focus
    }

    pub(crate) fn block(&self) -> BlockId {
        self.block
    }

    pub(crate) fn text(&self) -> &str {
        self.input.text()
    }

    fn finish(&mut self, apply: bool, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.finished {
            return;
        }
        self.finished = true;
        let text = self.input.text().to_string();
        let block = self.block;
        let token = self.token;
        let _ = self.editor.update(cx, |editor, cx| {
            if apply {
                editor.commit_code_lang_edit(token, block, text, window, cx);
            } else {
                editor.cancel_code_lang_edit(token, window, cx);
            }
        });
    }

    fn on_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let m = &ev.keystroke.modifiers;
        if !crate::ui::chord::has_chord(m) {
            match ev.keystroke.key.as_str() {
                "enter" => {
                    if !self.input.composing() {
                        self.finish(true, window, cx);
                    }
                    cx.stop_propagation();
                    return true;
                }
                "escape" => {
                    self.finish(false, window, cx);
                    cx.stop_propagation();
                    return true;
                }
                _ => {}
            }
        }
        match self.input.nav_key(&ev.keystroke.key, m, cx) {
            KeyOutcome::Ignored => false,
            KeyOutcome::Moved | KeyOutcome::Edited => {
                cx.notify();
                cx.stop_propagation();
                true
            }
        }
    }
}

impl EntityInputHandler for CodeLangInput {
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

impl TextInputHost for CodeLangInput {
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

impl EditorView {
    #[cfg(test)]
    pub(crate) fn code_lang_rect(&self, cx: &gpui::App) -> Option<(f32, f32, f32, f32)> {
        let field = self.code_lang.as_ref()?.read(cx);
        Some(rect_with_padding(field.rect))
    }

    pub(crate) fn fence_info(&self, block: BlockId) -> String {
        self.state
            .doc
            .document
            .live_id(block)
            .and_then(|id| self.state.doc.document.extra(id).code_fence_lang())
            .and_then(|lang| self.state.doc.document.lang(lang))
            .unwrap_or("")
            .to_string()
    }

    pub(crate) fn begin_code_lang_edit(
        &mut self,
        window: &mut Window,
        block: BlockId,
        rect: (f32, f32, f32, f32),
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(open) = self.code_lang.as_ref() {
            if open.read(cx).block() == block {
                let focus = open.read(cx).focus().clone();
                window.focus(&focus, cx);
                return;
            }
            self.close_code_lang(true, window, cx);
        }
        let text = self.fence_info(block);
        let caret_blink_ms = self.state.theme.paint.caret_blink_ms as u64;
        let token = self.code_lang_token.wrapping_add(1);
        self.code_lang_token = token;
        let editor = cx.entity().downgrade();
        let field =
            cx.new(|cx| CodeLangInput::new(editor, block, token, rect, text, caret_blink_ms, cx));
        field.update(cx, |field, cx| field.focus_now(window, cx));
        cx.observe(&field, |_, _, cx| cx.notify()).detach();
        self.code_lang = Some(field);
        cx.notify();
    }

    pub(crate) fn close_code_lang(
        &mut self,
        apply: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(field) = self.code_lang.take() else {
            return;
        };
        self.code_lang_token = self.code_lang_token.wrapping_add(1);
        let (block, text) = {
            let field = field.read(cx);
            (field.block(), field.text().to_string())
        };
        field.update(cx, |field, _| field.finished = true);
        if apply {
            self.apply_code_lang(block, text, cx);
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(crate) fn commit_code_lang_edit(
        &mut self,
        token: u64,
        block: BlockId,
        text: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.code_lang_token == token {
            self.code_lang = None;
            self.code_lang_token = self.code_lang_token.wrapping_add(1);
        }
        self.apply_code_lang(block, text, cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(crate) fn cancel_code_lang_edit(
        &mut self,
        token: u64,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.code_lang_token == token {
            self.code_lang = None;
            self.code_lang_token = self.code_lang_token.wrapping_add(1);
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn apply_code_lang(&mut self, block: BlockId, text: String, cx: &mut Context<'_, Self>) {
        let before = self.state.doc.document.revision();
        let sel = self.editing_sel();
        let _ = self
            .state
            .doc
            .apply(sel, Command::SetFenceLang { block, lang: text });
        if self.state.doc.document.revision() != before {
            self.note_edit(cx);
        }
    }
}

impl Render for CodeLangInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        if !self.focus.is_focused(window) {
            self.finish(true, window, cx);
            return div().into_any_element();
        }
        let Some(editor) = self.editor.upgrade() else {
            return div().into_any_element();
        };
        let t = ShellTheme::from_app(&editor.read(cx).state.theme.app);
        let font = editor.read(cx).state.theme.type_scale.body.font();
        let size = editor.read(cx).state.theme.decoration.well_lang_size;
        let (x, y, w, h) = rect_with_padding(self.rect);
        let host = cx.entity();
        div()
            .id("code-lang-input")
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(w))
            .h(px(h))
            .px(px(PAD_X))
            .flex()
            .items_center()
            .overflow_hidden()
            .rounded(px(RADIUS))
            .border_1()
            .border_color(t.accent)
            .bg(t.panel_bg)
            .text_size(px(size))
            .cursor(CursorStyle::IBeam)
            .track_focus(&self.focus)
            .occlude()
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                this.on_key(ev, window, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
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
                    font,
                    font_size: px(size),
                    line_height: px(size * 1.4),
                    text: t.text,
                    placeholder_color: t.text_disabled,
                    placeholder: "text".into(),
                    selection: t.selected_bg,
                    ime: t.selected_bg,
                    caret: t.accent,
                    caret_width: CARET_WIDTH,
                },
            ))
            .into_any_element()
    }
}
