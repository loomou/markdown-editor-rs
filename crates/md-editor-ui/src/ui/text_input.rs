use crate::ui::blink::Blink;
use gpui::ClipboardItem;
use gpui::{
    App, Bounds, Context, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    FocusHandle, Font, GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Modifiers,
    PaintQuad, Pixels, Point, ShapedLine, SharedString, TextRun, Window, fill, point, px, size,
};
use md_core::Px;
use md_core::doc::floor_char_boundary;
use md_core::document::{
    next_grapheme_boundary, next_word_boundary, prev_grapheme_boundary, prev_word_boundary,
    word_span,
};
use std::ops::Range;
use std::time::Duration;

pub fn offset_from_utf16(text: &str, target: usize) -> usize {
    let mut utf8 = 0;
    let mut utf16 = 0;
    for ch in text.chars() {
        if utf16 >= target {
            break;
        }
        utf16 += ch.len_utf16();
        utf8 += ch.len_utf8();
    }
    utf8
}

pub fn offset_to_utf16(text: &str, target: usize) -> usize {
    let mut utf8 = 0;
    let mut utf16 = 0;
    for ch in text.chars() {
        if utf8 >= target {
            break;
        }
        utf8 += ch.len_utf8();
        utf16 += ch.len_utf16();
    }
    utf16
}

pub fn range_from_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
    let start = offset_from_utf16(text, range.start);
    start..offset_from_utf16(text, range.end).max(start)
}

pub fn range_to_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
    offset_to_utf16(text, range.start)..offset_to_utf16(text, range.end)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputMark {
    Plain,
    Marked,
}

fn apply_text_input(
    text_before: &str,
    range: Range<usize>,
    text: &str,
    mark: InputMark,
) -> (String, usize, Option<Range<usize>>, bool) {
    let start = floor_char_boundary(text_before, range.start.min(text_before.len()));
    let end = floor_char_boundary(text_before, range.end.min(text_before.len())).max(start);
    let mut next = text_before.to_string();
    next.replace_range(start..end, text);
    let cursor = start + text.len();
    let marked = if mark == InputMark::Marked && !text.is_empty() {
        Some(start..start + text.len())
    } else {
        None
    };
    (next, cursor, marked, mark == InputMark::Plain)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Deletion {
    Backward,
    Forward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Grapheme,
    Word,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyOutcome {
    Ignored,
    Moved,
    Edited,
}

#[derive(Clone)]
pub struct InputStyle {
    pub font: Font,
    pub font_size: Pixels,
    pub line_height: Pixels,
    pub text: Hsla,
    pub placeholder_color: Hsla,
    pub placeholder: SharedString,
    pub selection: Hsla,
    pub ime: Hsla,
    pub caret: Hsla,
    pub caret_width: Px,
}

pub trait TextInputHost: EntityInputHandler {
    fn input(&self) -> Option<&TextInput>;
    fn input_mut(&mut self) -> Option<&mut TextInput>;
    fn input_focus(&self) -> FocusHandle;
    fn caret_live(&self, window: &Window) -> bool;
}

pub struct TextInput {
    text: String,
    selection: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    shaped: Option<ShapedLine>,
    bounds: Option<Bounds<Pixels>>,
    ime_caret: Option<(Px, Px, Px, Px)>,
    selecting: bool,
    scroll_x: Pixels,
    filter: Option<fn(&str) -> String>,
    max_len: Option<usize>,
    pub(crate) blink: Blink,
}

impl Default for TextInput {
    fn default() -> Self {
        Self {
            text: String::new(),
            selection: 0..0,
            reversed: false,
            marked: None,
            shaped: None,
            bounds: None,
            ime_caret: None,
            selecting: false,
            scroll_x: px(0.),
            filter: None,
            max_len: None,
            blink: Blink::default(),
        }
    }
}

impl TextInput {
    pub fn constrained(filter: fn(&str) -> String, max_len: usize) -> Self {
        Self {
            filter: Some(filter),
            max_len: Some(max_len),
            ..Self::default()
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        let all = 0..self.text.len();
        self.replace(all, &text.into(), InputMark::Plain);
        self.marked = None;
    }

    pub fn caret(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    fn anchor(&self) -> usize {
        if self.reversed {
            self.selection.end
        } else {
            self.selection.start
        }
    }

    pub fn selection(&self) -> Range<usize> {
        self.selection.clone()
    }

    pub fn marked(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }

    pub fn composing(&self) -> bool {
        self.marked.is_some()
    }

    pub fn clear_mark(&mut self) {
        self.marked = None;
    }

    #[doc(hidden)]
    pub fn bounds(&self) -> Option<Bounds<Pixels>> {
        self.bounds
    }

    #[doc(hidden)]
    pub fn shaped(&self) -> Option<&ShapedLine> {
        self.shaped.as_ref()
    }

    #[doc(hidden)]
    pub fn ime_caret(&self) -> Option<(Px, Px, Px, Px)> {
        self.ime_caret
    }

    pub(crate) fn collapse_to(&mut self, off: usize) {
        let off = floor_char_boundary(&self.text, off.min(self.text.len()));
        self.selection = off..off;
        self.reversed = false;
    }

    pub(crate) fn extend_to(&mut self, off: usize) {
        let off = floor_char_boundary(&self.text, off.min(self.text.len()));
        let anchor = self.anchor();
        if off < anchor {
            self.selection = off..anchor;
            self.reversed = true;
        } else {
            self.selection = anchor..off;
            self.reversed = false;
        }
    }

    pub fn select_all(&mut self) {
        self.selection = 0..self.text.len();
        self.reversed = false;
    }

    pub fn wake(&mut self) {
        self.blink.wake();
    }

    pub fn start_blink<T: 'static>(
        &mut self,
        period: Duration,
        cx: &mut Context<'_, T>,
        pick: fn(&mut T) -> Option<&mut TextInput>,
    ) {
        self.blink
            .start(period, cx, move |host| pick(host).map(|ti| &mut ti.blink));
    }

    #[doc(hidden)]
    pub fn blink_visible(&self) -> bool {
        self.blink.visible()
    }

    pub(crate) fn index_for_position(&self, position: Point<Pixels>) -> usize {
        if self.text.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(line)) = (self.bounds.as_ref(), self.shaped.as_ref()) else {
            return 0;
        };
        let text_left = bounds.left() + self.scroll_x;
        if position.x <= text_left {
            return 0;
        }
        if position.x >= text_left + line.width {
            return self.text.len();
        }
        floor_char_boundary(
            &self.text,
            line.closest_index_for_x(position.x - text_left)
                .min(self.text.len()),
        )
    }

    pub fn scroll_x(&self) -> Pixels {
        self.scroll_x
    }

    pub fn scrolled(prev: Pixels, caret_x: Pixels, line_w: Pixels, field_w: Pixels) -> Pixels {
        if line_w <= field_w {
            return px(0.);
        }
        const EDGE: f32 = 2.0;
        let max_scroll = line_w - field_w;
        let mut at = prev;
        if caret_x + at > field_w - px(EDGE) {
            at = field_w - px(EDGE) - caret_x;
        }
        if caret_x + at < px(EDGE) {
            at = px(EDGE) - caret_x;
        }
        at.clamp(-max_scroll, px(0.))
    }

    #[cfg(test)]
    fn offset_from_utf16(&self, target: usize) -> usize {
        offset_from_utf16(&self.text, target)
    }

    pub(crate) fn offset_to_utf16(&self, target: usize) -> usize {
        offset_to_utf16(&self.text, target)
    }

    pub(crate) fn range_from_utf16(&self, r: &Range<usize>) -> Range<usize> {
        range_from_utf16(&self.text, r)
    }

    pub(crate) fn range_to_utf16(&self, r: &Range<usize>) -> Range<usize> {
        range_to_utf16(&self.text, r)
    }

    pub fn selected_utf16(&self) -> gpui::UTF16Selection {
        gpui::UTF16Selection {
            range: self.range_to_utf16(&self.selection),
            reversed: self.reversed,
        }
    }

    pub fn marked_utf16(&self) -> Option<Range<usize>> {
        let r = self.marked.clone()?;
        Some(self.range_to_utf16(&r))
    }

    pub fn utf16_index_for_position(&self, position: Point<Pixels>) -> usize {
        self.offset_to_utf16(self.index_for_position(position))
    }

    pub fn ime_bounds(&self, element_bounds: Bounds<Pixels>) -> Option<Bounds<Pixels>> {
        let (x, y, w, h) = self.ime_caret?;
        Some(Bounds {
            origin: point(
                element_bounds.origin.x + px(x as f32),
                element_bounds.origin.y + px(y as f32),
            ),
            size: size(px(w.max(1.0) as f32), px(h as f32)),
        })
    }

    pub fn text_for_utf16(
        &self,
        range_utf16: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
    ) -> String {
        let r = self.range_from_utf16(&range_utf16);
        let back = self.range_to_utf16(&r);
        if back != range_utf16 {
            *adjusted = Some(back);
        }
        self.text.get(r).unwrap_or("").to_string()
    }

    pub fn edit_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        match range_utf16 {
            Some(r) => self.range_from_utf16(&r),
            None => self
                .marked
                .clone()
                .unwrap_or_else(|| self.selection.clone()),
        }
    }

    pub fn replace(&mut self, range: Range<usize>, text: &str, mark: InputMark) -> bool {
        let filtered = match self.filter {
            Some(f) => f(text),
            None => text.to_string(),
        };
        let text = match self.max_len {
            Some(max) => {
                let start = floor_char_boundary(&self.text, range.start.min(self.text.len()));
                let end =
                    floor_char_boundary(&self.text, range.end.min(self.text.len())).max(start);
                let room = max.saturating_sub(self.text.len() - (end - start));
                let cut = floor_char_boundary(&filtered, room.min(filtered.len()));
                filtered[..cut].to_string()
            }
            None => filtered,
        };
        let (next, cursor, marked, committed) = apply_text_input(&self.text, range, &text, mark);
        self.text = next;
        self.selection = cursor..cursor;
        self.reversed = false;
        self.marked = marked;
        self.wake();
        committed
    }

    fn step_from(&self, at: usize, dir: Deletion, step: Step) -> usize {
        match (dir, step) {
            (Deletion::Backward, Step::Grapheme) => prev_grapheme_boundary(&self.text, at),
            (Deletion::Forward, Step::Grapheme) => next_grapheme_boundary(&self.text, at),
            (Deletion::Backward, Step::Word) => prev_word_boundary(&self.text, at),
            (Deletion::Forward, Step::Word) => next_word_boundary(&self.text, at),
        }
    }

    pub(crate) fn delete_by(&mut self, dir: Deletion, step: Step) -> bool {
        if let Some(r) = self.marked.take() {
            return self.replace(r, "", InputMark::Plain);
        }
        if !self.selection.is_empty() {
            let r = self.selection.clone();
            return self.replace(r, "", InputMark::Plain);
        }
        let at = self.caret();
        let to = self.step_from(at, dir, step);
        let r = match dir {
            Deletion::Backward => to..at,
            Deletion::Forward => at..to,
        };
        if r.is_empty() {
            return false;
        }
        self.replace(r, "", InputMark::Plain)
    }

    pub(crate) fn selected_text(&self) -> Option<String> {
        if self.selection.is_empty() {
            return None;
        }
        Some(self.text.get(self.selection.clone())?.to_string())
    }

    pub(crate) fn one_line(text: &str) -> String {
        text.chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect()
    }

    fn move_or_extend(&mut self, to: usize, shift: bool) {
        self.marked = None;
        if shift {
            self.extend_to(to);
        } else {
            self.collapse_to(to);
        }
        self.wake();
    }

    pub fn nav_key(&mut self, key: &str, m: &Modifiers, cx: &mut App) -> KeyOutcome {
        let primary = crate::ui::chord::primary_down(m);
        if primary && !m.shift {
            match key.to_ascii_lowercase().as_str() {
                "a" => {
                    self.select_all();
                    self.marked = None;
                    self.wake();
                    return KeyOutcome::Moved;
                }
                "c" => {
                    self.copy(cx);
                    return KeyOutcome::Moved;
                }
                "x" => return outcome(self.cut(cx)),
                "v" => return outcome(self.paste(cx)),
                _ => {}
            }
        }
        #[cfg(target_os = "macos")]
        if primary {
            match key {
                "left" => {
                    self.move_or_extend(0, m.shift);
                    return KeyOutcome::Moved;
                }
                "right" => {
                    self.move_or_extend(self.text.len(), m.shift);
                    return KeyOutcome::Moved;
                }
                "backspace" => {
                    if self.marked.is_some() || !self.selection.is_empty() {
                        return outcome(self.delete_by(Deletion::Backward, Step::Grapheme));
                    }
                    let at = self.caret();
                    if at == 0 {
                        return KeyOutcome::Moved;
                    }
                    return outcome(self.replace(0..at, "", InputMark::Plain));
                }
                _ => {}
            }
        }
        let step = if crate::ui::chord::word_mod_down(m) {
            Step::Word
        } else {
            Step::Grapheme
        };
        let chord_ok = step == Step::Word || !crate::ui::chord::has_chord(m);
        if !chord_ok {
            return KeyOutcome::Ignored;
        }
        let shift = m.shift;
        match key {
            "left" => {
                let to = if shift || self.selection.is_empty() || step == Step::Word {
                    self.step_from(self.caret(), Deletion::Backward, step)
                } else {
                    self.selection.start
                };
                self.move_or_extend(to, shift);
                KeyOutcome::Moved
            }
            "right" => {
                let to = if shift || self.selection.is_empty() || step == Step::Word {
                    self.step_from(self.caret(), Deletion::Forward, step)
                } else {
                    self.selection.end
                };
                self.move_or_extend(to, shift);
                KeyOutcome::Moved
            }
            "home" => {
                self.move_or_extend(0, shift);
                KeyOutcome::Moved
            }
            "end" => {
                self.move_or_extend(self.text.len(), shift);
                KeyOutcome::Moved
            }
            "backspace" => outcome(self.delete_by(Deletion::Backward, step)),
            "delete" => outcome(self.delete_by(Deletion::Forward, step)),
            _ => KeyOutcome::Ignored,
        }
    }

    pub fn copy(&mut self, cx: &mut App) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    pub fn cut(&mut self, cx: &mut App) -> bool {
        let Some(text) = self.selected_text() else {
            return false;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        let r = self.selection.clone();
        self.replace(r, "", InputMark::Plain)
    }

    pub fn paste(&mut self, cx: &mut App) -> bool {
        let Some(text) = cx.read_from_clipboard().and_then(|it| it.text()) else {
            return false;
        };
        let text = Self::one_line(&text);
        if text.is_empty() {
            return false;
        }
        let r = self.marked.take().unwrap_or_else(|| self.selection.clone());
        self.replace(r, &text, InputMark::Plain)
    }

    pub fn mouse_down(&mut self, at: Point<Pixels>, click_count: usize, shift: bool) {
        let off = self.index_for_position(at);
        self.marked = None;
        self.selecting = true;
        match click_count {
            1 if shift => self.extend_to(off),
            1 => self.collapse_to(off),
            2 => match word_span(&self.text, off) {
                Some((lo, hi)) => {
                    self.selection = lo..hi;
                    self.reversed = false;
                }
                None => self.select_all(),
            },
            _ => self.select_all(),
        }
        self.wake();
    }

    pub fn mouse_move(&mut self, at: Point<Pixels>) -> bool {
        if !self.selecting {
            return false;
        }
        let off = self.index_for_position(at);
        if off == self.caret() {
            return false;
        }
        self.extend_to(off);
        self.wake();
        true
    }

    pub fn mouse_up(&mut self) {
        self.selecting = false;
    }

    pub fn dismiss(&mut self) {
        self.marked = None;
        self.selecting = false;
        self.blink.stop();
    }
}

pub struct TextInputElement<V: TextInputHost> {
    host: Entity<V>,
    style: InputStyle,
}

impl<V: TextInputHost> TextInputElement<V> {
    pub fn new(host: Entity<V>, style: InputStyle) -> Self {
        Self { host, style }
    }
}

impl<V: TextInputHost> IntoElement for TextInputElement<V> {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

pub struct InputVisuals {
    line: Option<ShapedLine>,
    placeholder: bool,
    marked: Option<PaintQuad>,
    selection: Option<PaintQuad>,
    caret: Option<PaintQuad>,
    row_origin: Point<Pixels>,
    row_h: Pixels,
    scroll_x: Pixels,
    ime_caret: (Px, Px, Px, Px),
}

impl InputVisuals {
    fn absent() -> Self {
        Self {
            line: None,
            placeholder: false,
            marked: None,
            selection: None,
            caret: None,
            row_origin: point(px(0.), px(0.)),
            row_h: px(0.),
            scroll_x: px(0.),
            ime_caret: (0.0, 0.0, 0.0, 0.0),
        }
    }
}

impl<V: TextInputHost> Element for TextInputElement<V> {
    type RequestLayoutState = ();
    type PrepaintState = InputVisuals;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = gpui::Style::default();
        style.size.width = gpui::relative(1.0).into();
        style.size.height = gpui::relative(1.0).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _rl: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let st = &self.style;
        let Some((text, selection, caret_at, marked, scroll_x)) =
            self.host.read(cx).input().map(|input| {
                (
                    input.text.clone(),
                    input.selection(),
                    input.caret(),
                    input.marked(),
                    input.scroll_x(),
                )
            })
        else {
            return InputVisuals::absent();
        };
        let scale = window.scale_factor();
        let row_h = snap_px(st.line_height, scale);
        let row_origin = point(
            bounds.left(),
            snap_px(bounds.top() + row_offset(bounds.size.height, row_h), scale),
        );
        let placeholder = text.is_empty() && !st.placeholder.is_empty();
        let display: SharedString = if placeholder {
            st.placeholder.clone()
        } else {
            text.into()
        };
        let run = TextRun {
            len: display.len(),
            font: st.font.clone(),
            color: if placeholder {
                st.placeholder_color
            } else {
                st.text
            },
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = window
            .text_system()
            .shape_line(display, st.font_size, &[run], None);
        let caret_in_text = if placeholder {
            px(0.)
        } else {
            line.x_for_index(caret_at)
        };
        let scroll_x = if placeholder {
            px(0.)
        } else {
            TextInput::scrolled(scroll_x, caret_in_text, line.width, bounds.size.width)
        };
        let scroll_x = snap_px(scroll_x, scale);
        let row_origin = point(row_origin.x + scroll_x, row_origin.y);
        let caret_x = if placeholder {
            px(0.)
        } else {
            snap_px(row_origin.x + caret_in_text, scale) - row_origin.x
        };
        let caret_w = px(st.caret_width as f32);
        let row_quad = |r: &Range<usize>, color| {
            fill(
                Bounds::from_corners(
                    point(row_origin.x + line.x_for_index(r.start), row_origin.y),
                    point(row_origin.x + line.x_for_index(r.end), row_origin.y + row_h),
                ),
                color,
            )
        };
        let selection =
            (!placeholder && !selection.is_empty()).then(|| row_quad(&selection, st.selection));
        let marked = marked
            .filter(|m| !placeholder && !m.is_empty() && m.end <= line.len())
            .map(|m| row_quad(&m, st.ime));
        let caret = fill(
            Bounds {
                origin: point(row_origin.x + caret_x, row_origin.y),
                size: size(caret_w, row_h),
            },
            st.caret,
        );
        InputVisuals {
            line: Some(line),
            placeholder,
            marked,
            selection,
            caret: Some(caret),
            row_origin,
            row_h,
            scroll_x,
            ime_caret: (
                f32::from(caret_x + scroll_x) as Px,
                f32::from(row_origin.y - bounds.top()) as Px,
                st.caret_width,
                f32::from(row_h) as Px,
            ),
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _rl: &mut Self::RequestLayoutState,
        st: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(line) = st.line.take() else {
            return;
        };
        let (focus, live, blink_visible) = {
            let host = self.host.read(cx);
            (
                host.input_focus(),
                host.caret_live(window),
                host.input().is_some_and(|i| i.blink.visible()),
            )
        };
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.host.clone()),
            cx,
        );
        for quad in [st.marked.take(), st.selection.take()]
            .into_iter()
            .flatten()
        {
            window.paint_quad(quad);
        }
        let _ = line.paint(st.row_origin, st.row_h, window, cx);
        if live
            && blink_visible
            && let Some(caret) = st.caret.take()
        {
            window.paint_quad(caret);
        }
        let shaped = (!st.placeholder).then_some(line);
        let ime_caret = st.ime_caret;
        let scroll_x = st.scroll_x;
        self.host.update(cx, |host, _| {
            if let Some(input) = host.input_mut() {
                input.shaped = shaped;
                input.bounds = Some(bounds);
                input.ime_caret = Some(ime_caret);
                input.scroll_x = scroll_x;
                input.blink.set_live(live);
            }
        });
    }
}

fn outcome(edited: bool) -> KeyOutcome {
    if edited {
        KeyOutcome::Edited
    } else {
        KeyOutcome::Moved
    }
}

pub(crate) fn row_offset(field_h: Pixels, row_h: Pixels) -> Pixels {
    ((field_h - row_h) / 2.).max(px(0.))
}

pub(crate) fn snap_px(v: Pixels, scale: f32) -> Pixels {
    if scale <= 0.0 {
        return v;
    }
    px((f32::from(v) * scale).round() / scale)
}

#[cfg(test)]
mod tests {
    use super::{
        Deletion, InputMark, KeyOutcome, Step, TextInput, apply_text_input, row_offset, snap_px,
    };
    use gpui::TestAppContext;
    use gpui::{ClipboardItem, Modifiers, point, px};

    fn input(text: &str) -> TextInput {
        let mut i = TextInput::default();
        i.set_text(text);
        i
    }

    #[test]
    fn utf16_offsets_round_trip_through_astral_planes() {
        let i = input("a😀b");
        assert_eq!(i.offset_to_utf16(0), 0);
        assert_eq!(i.offset_to_utf16(1), 1);
        assert_eq!(
            i.offset_to_utf16(5),
            3,
            "a surrogate pair counts as two units"
        );
        assert_eq!(i.offset_to_utf16(6), 4);
        assert_eq!(i.offset_from_utf16(3), 5);
        assert_eq!(i.offset_from_utf16(4), 6);
        assert_eq!(i.offset_from_utf16(2), 5);
        assert_eq!(i.range_to_utf16(&(1..5)), 1..3);
        assert_eq!(i.range_from_utf16(&(1..3)), 1..5);
    }

    #[test]
    fn text_for_range_reports_the_range_it_actually_took() {
        let i = input("a😀b");
        let mut adjusted = None;
        assert_eq!(i.text_for_utf16(1..3, &mut adjusted), "😀");
        assert_eq!(
            adjusted, None,
            "landing exactly on the boundary needs no report"
        );
        let mut adjusted = None;
        assert_eq!(i.text_for_utf16(1..2, &mut adjusted), "😀");
        assert_eq!(
            adjusted,
            Some(1..3),
            "half a surrogate pair was asked for but the whole was returned"
        );
    }

    #[test]
    fn marked_input_does_not_commit() {
        let (t, cursor, marked, committed) = apply_text_input("", 0..0, "ni", InputMark::Marked);
        assert_eq!(t, "ni");
        assert_eq!(cursor, 2);
        assert_eq!(marked, Some(0..2));
        assert!(!committed);
    }

    #[test]
    fn plain_input_commits() {
        let (t, cursor, marked, committed) = apply_text_input("ni", 0..2, "你", InputMark::Plain);
        assert_eq!(t, "你");
        assert_eq!(cursor, "你".len());
        assert_eq!(marked, None);
        assert!(committed);
    }

    #[test]
    fn input_snaps_invalid_utf8_ranges() {
        let (t, cursor, marked, committed) = apply_text_input("a😀b", 2..5, "X", InputMark::Plain);
        assert_eq!(t, "aXb");
        assert_eq!(cursor, 2);
        assert_eq!(marked, None);
        assert!(committed);
    }

    #[test]
    fn only_a_committed_edit_asks_the_host_to_react() {
        let mut i = TextInput::default();
        assert!(
            !i.replace(0..0, "ni", InputMark::Marked),
            "mid-composition must not commit"
        );
        assert_eq!(i.text(), "ni");
        assert_eq!(i.marked(), Some(0..2));
        assert!(i.composing());
        assert!(
            i.replace(0..2, "你", InputMark::Plain),
            "only the confirm keystroke commits it"
        );
        assert_eq!(i.text(), "你");
        assert_eq!(i.marked(), None);
        assert_eq!(i.caret(), "你".len());
    }

    #[test]
    fn deleting_eats_the_mark_then_the_selection_then_one_char() {
        let mut i = input("a😀b");
        i.replace(5..5, "ni", InputMark::Marked);
        assert_eq!(i.text(), "a😀nib");
        assert!(i.delete_by(Deletion::Backward, Step::Grapheme));
        assert_eq!(
            i.text(),
            "a😀b",
            "the marked range is eaten whole, not one letter back"
        );

        let mut i = input("hello");
        i.collapse_to(1);
        i.extend_to(4);
        assert!(i.delete_by(Deletion::Forward, Step::Grapheme));
        assert_eq!(
            i.text(),
            "ho",
            "with a selection, both forward and backward delete eat the selection"
        );
        assert_eq!(i.caret(), 1);

        let mut i = input("a😀");
        assert!(i.delete_by(Deletion::Backward, Step::Grapheme));
        assert_eq!(i.text(), "a");
        assert!(
            !i.delete_by(Deletion::Forward, Step::Grapheme),
            "caret at the end, forward delete has nothing to do"
        );
        i.collapse_to(0);
        assert!(
            !i.delete_by(Deletion::Backward, Step::Grapheme),
            "caret at the start, backspace has nothing to do"
        );
    }

    #[test]
    fn extending_snaps_to_character_boundaries() {
        let mut i = input("a😀b");
        i.collapse_to(0);
        i.extend_to(3);
        assert_eq!(
            i.selection(),
            0..1,
            "3 is inside 😀, so it clamps back in front of it"
        );
        i.extend_to(9999);
        assert_eq!(i.selection(), 0..6, "past the end it clamps to the end");
        i.collapse_to(6);
        i.extend_to(1);
        assert_eq!(i.selection(), 1..6);
        assert_eq!(i.caret(), 1);
    }

    fn none() -> Modifiers {
        Modifiers::default()
    }

    fn shift() -> Modifiers {
        Modifiers {
            shift: true,
            ..Default::default()
        }
    }

    fn primary() -> Modifiers {
        Modifiers {
            control: cfg!(not(target_os = "macos")),
            platform: cfg!(target_os = "macos"),
            ..Default::default()
        }
    }

    fn word_mod() -> Modifiers {
        Modifiers {
            alt: cfg!(target_os = "macos"),
            control: cfg!(not(target_os = "macos")),
            ..Default::default()
        }
    }

    #[gpui::test]
    fn nav_keys_separate_moving_from_editing(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let mut i = input("hello");
            assert_eq!(i.nav_key("home", &none(), cx), KeyOutcome::Moved);
            assert_eq!(i.caret(), 0);
            assert_eq!(i.nav_key("end", &none(), cx), KeyOutcome::Moved);
            assert_eq!(i.caret(), 5);
            assert_eq!(i.nav_key("backspace", &none(), cx), KeyOutcome::Edited);
            assert_eq!(i.text(), "hell");
            i.collapse_to(0);
            assert_eq!(
                i.nav_key("backspace", &none(), cx),
                KeyOutcome::Moved,
                "backspace at the start deletes nothing: handled, but not edited"
            );
            assert_eq!(
                i.nav_key("enter", &none(), cx),
                KeyOutcome::Ignored,
                "Enter is for the host to interpret"
            );
            assert_eq!(i.nav_key("escape", &none(), cx), KeyOutcome::Ignored);
            assert_eq!(i.nav_key("f3", &none(), cx), KeyOutcome::Ignored);

            i.collapse_to(0);
            assert_eq!(i.nav_key("right", &shift(), cx), KeyOutcome::Moved);
            assert_eq!(
                i.selection(),
                0..1,
                "Shift+Right extends instead of collapsing"
            );

            assert_eq!(i.nav_key("a", &primary(), cx), KeyOutcome::Moved);
            assert_eq!(
                i.selection(),
                0..i.text().len(),
                "Ctrl+A selects everything"
            );
            assert_eq!(
                i.nav_key("f", &primary(), cx),
                KeyOutcome::Ignored,
                "Ctrl+F is the host's business"
            );
        });
    }

    #[gpui::test]
    fn arrows_step_over_whole_graphemes(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let mut i = input("ae\u{301}b");
            i.collapse_to(0);
            i.nav_key("right", &none(), cx);
            assert_eq!(i.caret(), 1, "passes a first");
            i.nav_key("right", &none(), cx);
            assert_eq!(
                i.caret(),
                4,
                "the whole é is crossed in one step, not stopping before the accent"
            );
            i.nav_key("left", &none(), cx);
            assert_eq!(i.caret(), 1, "backward is one step too");

            let mut i = input("👨‍👩‍👧x");
            i.collapse_to(0);
            i.nav_key("right", &none(), cx);
            assert_eq!(
                i.caret(),
                "👨‍👩‍👧".len(),
                "the whole cluster is crossed in one step"
            );
        });
    }

    #[gpui::test]
    fn the_word_modifier_moves_and_deletes_by_word(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let mut i = input("hello world foo");
            i.collapse_to(0);
            assert_eq!(i.nav_key("right", &word_mod(), cx), KeyOutcome::Moved);
            assert_eq!(i.caret(), 6, "lands at the start of world");
            i.nav_key("right", &word_mod(), cx);
            assert_eq!(i.caret(), 12);
            i.nav_key("left", &word_mod(), cx);
            assert_eq!(i.caret(), 6);

            i.collapse_to(11);
            assert_eq!(i.nav_key("backspace", &word_mod(), cx), KeyOutcome::Edited);
            assert_eq!(i.text(), "hello  foo", "the whole world is gone");

            let mut i = input("hello world");
            i.collapse_to(0);
            let mut m = word_mod();
            m.shift = true;
            i.nav_key("right", &m, cx);
            assert_eq!(i.selection(), 0..6, "the selection extends by word");
        });
    }

    #[cfg(target_os = "macos")]
    #[gpui::test]
    fn primary_arrows_jump_field_edges_and_backspace_clears_the_prefix(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let mut i = input("hello world");
            i.collapse_to(6);
            assert_eq!(i.nav_key("left", &primary(), cx), KeyOutcome::Moved);
            assert_eq!(i.caret(), 0, "⌘← should reach the start");
            assert_eq!(i.nav_key("right", &primary(), cx), KeyOutcome::Moved);
            assert_eq!(i.caret(), 11, "⌘→ should reach the end");

            i.collapse_to(6);
            let mut m = primary();
            m.shift = true;
            i.nav_key("left", &m, cx);
            assert_eq!(i.selection(), 0..6, "⌘⇧← extends to the start");

            let mut i = input("hello world");
            i.collapse_to(6);
            assert_eq!(i.nav_key("backspace", &primary(), cx), KeyOutcome::Edited);
            assert_eq!(
                i.text(),
                "world",
                "⌘⌫ deletes the whole run before the caret"
            );
            assert_eq!(i.caret(), 0);

            i.collapse_to(0);
            assert_eq!(
                i.nav_key("backspace", &primary(), cx),
                KeyOutcome::Moved,
                "already at the start: counts as handled but must not change the text"
            );

            let mut i = input("hello world");
            i.collapse_to(0);
            i.extend_to(5);
            assert_eq!(i.nav_key("backspace", &primary(), cx), KeyOutcome::Edited);
            assert_eq!(
                i.text(),
                " world",
                "with a selection it only eats the selection"
            );
        });
    }

    #[gpui::test]
    fn clipboard_round_trips_through_the_field(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let mut i = input("hello world");
            i.collapse_to(0);
            i.extend_to(5);
            assert_eq!(
                i.nav_key("c", &primary(), cx),
                KeyOutcome::Moved,
                "copy must not change the text"
            );
            assert_eq!(i.text(), "hello world");
            assert_eq!(
                cx.read_from_clipboard().and_then(|it| it.text()).as_deref(),
                Some("hello")
            );

            i.collapse_to(6);
            i.extend_to(11);
            assert_eq!(i.nav_key("x", &primary(), cx), KeyOutcome::Edited);
            assert_eq!(i.text(), "hello ");
            assert_eq!(
                cx.read_from_clipboard().and_then(|it| it.text()).as_deref(),
                Some("world")
            );

            assert_eq!(i.nav_key("v", &primary(), cx), KeyOutcome::Edited);
            assert_eq!(i.text(), "hello world");
            assert_eq!(i.caret(), 11);

            i.collapse_to(0);
            i.copy(cx);
            assert_eq!(
                i.nav_key("x", &primary(), cx),
                KeyOutcome::Moved,
                "without a selection there is nothing to cut"
            );
            assert_eq!(i.text(), "hello world");
            assert_eq!(
                cx.read_from_clipboard().and_then(|it| it.text()).as_deref(),
                Some("world"),
                "copy/cut with no selection must not touch the clipboard"
            );
        });
    }

    #[gpui::test]
    fn pasting_multiple_lines_flattens_them(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("a\nb\tc\r\n".into()));
            let mut i = TextInput::default();
            assert!(i.paste(cx));
            assert_eq!(i.text(), "a b c  ", "newlines and tabs become spaces");
            assert!(!i.text().contains('\n'));

            let mut i = input("keep");
            cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
            assert!(!i.paste(cx), "an empty string pastes nothing");
            assert_eq!(i.text(), "keep");
        });
    }

    #[test]
    fn double_click_takes_a_word_and_triple_takes_the_lot() {
        let mut i = input("hello world");
        i.mouse_down(point(px(0.), px(0.)), 2, false);
        assert_eq!(
            i.selection(),
            0..5,
            "a double click selects the word under the caret"
        );
        i.mouse_down(point(px(0.), px(0.)), 3, false);
        assert_eq!(
            i.selection(),
            0..i.text().len(),
            "a triple click selects everything"
        );
        let mut e = TextInput::default();
        e.mouse_down(point(px(0.), px(0.)), 2, false);
        assert_eq!(e.selection(), 0..0);
    }

    #[test]
    fn a_long_value_scrolls_to_keep_the_caret_in_view() {
        let field = px(140.);
        assert_eq!(
            TextInput::scrolled(px(0.), px(30.), px(100.), field),
            px(0.)
        );
        assert_eq!(
            TextInput::scrolled(px(-50.), px(30.), px(100.), field),
            px(0.),
            "once the text shrinks to fit, any earlier scroll must reset to zero"
        );

        let at = TextInput::scrolled(px(0.), px(260.), px(260.), field);
        assert!(at < px(0.), "it should scroll left; measured {at:?}");
        assert!(
            f32::from(px(260.) + at) <= f32::from(field),
            "after scrolling the caret is still outside the field: {at:?}"
        );

        let clamped = TextInput::scrolled(px(-9999.), px(260.), px(260.), field);
        assert_eq!(
            clamped,
            -(px(260.) - field),
            "scrolling too far would leave blank space at the end"
        );

        let back = TextInput::scrolled(px(-120.), px(0.), px(260.), field);
        assert!(
            f32::from(back).abs() < 3.0,
            "with the caret back at the row start it should scroll back; measured {back:?}"
        );
    }

    #[test]
    fn a_row_taller_than_its_field_sticks_to_the_top() {
        assert!((f32::from(row_offset(px(22.), px(15.4))) - 3.3).abs() < 0.01);
        assert_eq!(
            row_offset(px(10.), px(16.)),
            px(0.),
            "when the text shrinks to fit, earlier scrolling resets to zero"
        );
    }

    #[test]
    fn snapping_lands_the_caret_on_a_device_pixel() {
        assert_eq!(snap_px(px(13.37), 1.0), px(13.));
        assert_eq!(
            snap_px(px(13.37), 2.0),
            px(13.5),
            "at 2x, half a logical pixel is a whole pixel"
        );
        assert_eq!(
            snap_px(px(5.), 0.0),
            px(5.),
            "without a scale it should return as-is"
        );
    }
}
