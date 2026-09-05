use super::{EditorView, PendingNav};
use crate::keymap::Cmd;
use crate::ui::text_input::{
    InputMark, InputStyle, KeyOutcome, TextInput, TextInputElement, TextInputHost,
};
use gpui::{
    App, Bounds, ClickEvent, Context, CursorStyle, Div, Entity, EntityInputHandler, FocusHandle,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement, Pixels, Point, Render, SharedString, Stateful,
    StatefulInteractiveElement, Styled, UTF16Selection, Window, div, point, px, rgba, svg,
};
use md_content::gpui_theme::ThemeColorExt;
use std::ops::Range;
use std::time::Duration;

const PLACEHOLDER: &str = "Find";

const QUERY_W: f32 = 140.0;

const COUNT_SLOT_W: f32 = 68.0;

const GAP: f32 = 6.0;

const FIND_DEBOUNCE: Duration = Duration::from_millis(120);

#[derive(Clone, Copy, PartialEq, Eq)]
enum QueryPush {
    Seed,
    Refresh,
}

pub struct FindBar {
    pub focus: FocusHandle,
    pub open: bool,

    query: TextInput,
    editor: Entity<EditorView>,
    synced_rev: u64,

    debounce: Option<gpui::Task<()>>,
}

impl FindBar {
    pub fn new(editor: Entity<EditorView>, cx: &mut Context<'_, Self>) -> Self {
        cx.observe(&editor, |this, editor, cx| {
            if !this.open {
                return;
            }
            let editor = editor.downgrade();
            let find = cx.weak_entity();
            cx.defer(move |cx| {
                let Some(find) = find.upgrade() else {
                    return;
                };
                let Some(editor) = editor.upgrade() else {
                    return;
                };
                find.update(cx, |this, cx| {
                    if !this.open {
                        return;
                    }
                    let rev = editor.read(cx).state.doc.document.revision();
                    if rev != this.synced_rev {
                        this.push_query(QueryPush::Refresh, cx);
                    }
                    cx.notify();
                });
            });
        })
        .detach();
        FindBar {
            focus: cx.focus_handle(),
            open: false,
            query: TextInput::default(),
            editor,
            synced_rev: 0,
            debounce: None,
        }
    }

    pub fn open(&mut self, window: &mut Window, seed: String, cx: &mut Context<'_, Self>) {
        self.open = true;
        if !seed.is_empty() {
            self.query.set_text(seed);
        }

        self.query.select_all();
        self.query.mouse_up();
        window.focus(&self.focus);
        self.start_blink(cx);
        cx.notify();
    }

    fn start_blink(&mut self, cx: &mut Context<'_, Self>) {
        let ms = self.editor.read(cx).state.theme.paint.caret_blink_ms;
        self.query
            .start_blink(Duration::from_millis(ms as u64), cx, |v: &mut Self| {
                Some(&mut v.query)
            });
    }

    pub fn query(&self) -> &str {
        self.query.text()
    }

    #[cfg(test)]
    pub(crate) fn caret(&self) -> usize {
        self.query.caret()
    }

    #[cfg(test)]
    pub(crate) fn selection(&self) -> Range<usize> {
        self.query.selection()
    }

    pub fn dismiss(&mut self, cx: &mut Context<'_, Self>) {
        if !self.open {
            return;
        }
        self.open = false;
        self.query.dismiss();

        self.debounce = None;
        cx.notify();
    }

    pub fn close(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if !self.open {
            return;
        }
        self.dismiss(cx);
        let editor = self.editor.clone();
        editor.update(cx, |v, cx| {
            v.clear_search();
            window.focus(&v.focus);
            cx.notify();
        });
    }

    fn push_query(&mut self, push: QueryPush, cx: &mut Context<'_, Self>) {
        let q = self.query.text().to_string();
        self.synced_rev = self.editor.read(cx).state.doc.document.revision();

        if push == QueryPush::Refresh || q.is_empty() {
            self.debounce = None;
            let find = cx.weak_entity();
            cx.defer(move |cx| {
                if let Some(find) = find.upgrade() {
                    find.update(cx, |this, cx| this.run_query(push, &q, cx));
                }
            });
            return;
        }

        self.debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FIND_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| this.run_query(QueryPush::Seed, &q, cx));
        }));
    }

    fn run_query(&mut self, push: QueryPush, q: &str, cx: &mut Context<'_, Self>) {
        if !self.open || self.query.text() != q || self.query.composing() {
            return;
        }
        self.editor.update(cx, |v, cx| {
            v.search_open = true;
            match push {
                QueryPush::Seed => v.set_search_query(q),
                QueryPush::Refresh => v.refresh_search(q),
            }
            cx.notify();
        });
    }

    fn apply_edit(
        &mut self,
        range: Range<usize>,
        text: &str,
        mark: InputMark,
        cx: &mut Context<'_, Self>,
    ) {
        if self.query.replace(range, text, mark) {
            self.push_query(QueryPush::Seed, cx);
        }
        cx.notify();
    }

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, cx: &mut Context<'_, Self>) {
        self.query
            .mouse_down(ev.position, ev.click_count, ev.modifiers.shift);
        cx.notify();
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, cx: &mut Context<'_, Self>) {
        if self.query.mouse_move(ev.position) {
            cx.notify();
        }
    }

    fn on_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        if !self.open {
            return false;
        }
        let m = &ev.keystroke.modifiers;

        let cmd = self
            .editor
            .read(cx)
            .keymap()
            .lookup(&ev.keystroke)
            .filter(|c| matches!(c, Cmd::Find | Cmd::Quit | Cmd::FindNext | Cmd::FindPrev));

        if cmd == Some(Cmd::Find) {
            window.focus(&self.focus);
            self.query.wake();
            cx.stop_propagation();
            return true;
        }
        if cmd == Some(Cmd::Quit) && !ev.is_held {
            self.editor.update(cx, |v, cx| {
                v.request_nav(PendingNav::Close, window, cx);
            });
            cx.stop_propagation();
            return true;
        }

        if let Some(cmd @ (Cmd::FindNext | Cmd::FindPrev)) = cmd {
            if self.query.composing() {
                cx.stop_propagation();
                return true;
            }
            let dir = if cmd == Cmd::FindPrev { -1 } else { 1 };
            self.editor.update(cx, |v, cx| {
                v.search_step(dir);
                cx.notify();
            });
            cx.stop_propagation();
            return true;
        }

        let shift = m.shift;
        if !crate::ui::chord::has_chord(m) {
            match ev.keystroke.key.as_str() {
                "escape" => {
                    self.close(window, cx);
                    cx.stop_propagation();
                    return true;
                }

                "enter" => {
                    if self.query.composing() {
                        cx.stop_propagation();
                        return true;
                    }
                    self.editor.update(cx, |v, cx| {
                        v.search_step(if shift { -1 } else { 1 });
                        cx.notify();
                    });
                    cx.stop_propagation();
                    return true;
                }
                _ => {}
            }
        }

        match self.query.nav_key(&ev.keystroke.key, m, cx) {
            KeyOutcome::Ignored => false,
            KeyOutcome::Moved => {
                cx.notify();
                cx.stop_propagation();
                true
            }
            KeyOutcome::Edited => {
                self.push_query(QueryPush::Seed, cx);
                cx.notify();
                cx.stop_propagation();
                true
            }
        }
    }
}

impl Render for FindBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        if !self.open {
            return div().id("find-bar");
        }
        let editor = self.editor.read(cx);
        let theme = editor.state.theme;
        let chrome = theme.chrome;

        let app = theme.app;
        let n = editor.search.matches.len();
        let composing = self.query.composing();
        let empty = self.query.text().is_empty();
        let count = count_label(
            CountState {
                empty,
                composing,
                capped: editor.search.capped,
            },
            n,
            editor.search.active,
        );
        let count_color = if !empty && !composing && (editor.search.capped || n == 0) {
            if editor.search.capped {
                app.warn.hsla()
            } else {
                app.syn_red.hsla()
            }
        } else {
            app.text_disabled.hsla()
        };
        let query_entity = cx.entity();
        div()
            .id("find-bar")
            .absolute()
            .top(px(14.))
            .right(px(18.))
            .h(px(32.))
            .flex()
            .items_center()
            .gap(px(GAP))
            .pl(px(10.))
            .pr(px(6.))
            .bg(app.bar_bg.hsla())
            .border_1()
            .border_color(app.border.hsla())
            .rounded(px(6.))
            .shadow(vec![gpui::BoxShadow {
                color: rgba(0x00000066).into(),
                offset: point(px(0.), px(10.)),
                blur_radius: px(28.),
                spread_radius: px(0.),
            }])
            .font_family(chrome.status_font)
            .text_size(px(chrome.status_size_px))
            .track_focus(&self.focus)
            .occlude()
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                this.on_key(ev, window, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus);
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                this.on_mouse_move(ev, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| {
                    this.query.mouse_up();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| {
                    this.query.mouse_up();
                }),
            )
            .child(
                svg()
                    .size(px(13.))
                    .path(crate::ui::icons::SEARCH)
                    .text_color(app.text_disabled.hsla()),
            )
            .child(
                div()
                    .id("find-query")
                    .w(px(QUERY_W))
                    .h(px(22.))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .cursor(CursorStyle::IBeam)
                    .text_color(app.text.hsla())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                            this.on_mouse_down(ev, cx);
                        }),
                    )
                    .child(TextInputElement::new(
                        query_entity.clone(),
                        InputStyle {
                            font: status_font(&chrome),
                            font_size: px(chrome.status_size_px),
                            line_height: px(chrome.status_size_px * chrome.status_line_height_em),
                            text: app.text.hsla(),
                            placeholder_color: app.text_disabled.hsla(),
                            placeholder: PLACEHOLDER.into(),
                            selection: theme.paint.selection.hsla(),
                            ime: theme.paint.ime.hsla(),
                            caret: theme.paint.caret.hsla(),
                            caret_width: theme.paint.caret_width,
                        },
                    )),
            )
            .child(
                div()
                    .w(px(COUNT_SLOT_W))
                    .flex_none()
                    .flex()
                    .justify_end()
                    .overflow_hidden()
                    .font_family(chrome.status_font)
                    .text_size(px(chrome.status_size_px))
                    .px(px(2.))
                    .text_color(count_color)
                    .child(SharedString::from(count)),
            )
            .child(div().w(px(1.)).h(px(16.)).bg(app.border.hsla()))
            .child(
                self.find_btn(app, "find-prev", crate::ui::icons::CHEV_UP)
                    .on_click({
                        let editor = self.editor.clone();
                        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                            editor.update(cx, |editor, cx| {
                                editor.search_step(-1);
                                cx.notify();
                            });
                        }
                    }),
            )
            .child(
                self.find_btn(app, "find-next", crate::ui::icons::CHEV_DOWN)
                    .on_click({
                        let editor = self.editor.clone();
                        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                            editor.update(cx, |editor, cx| {
                                editor.search_step(1);
                                cx.notify();
                            });
                        }
                    }),
            )
            .child(
                self.find_btn(app, "find-close", crate::ui::icons::FIND_CLOSE)
                    .on_click({
                        let this = query_entity;
                        move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                            this.update(cx, |find, cx| find.close(window, cx));
                        }
                    }),
            )
    }
}

impl FindBar {
    fn find_btn(
        &self,
        app: md_theme::AppTokens,
        id: &'static str,
        path: &'static str,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .size(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .hover(move |s| s.bg(app.border.hsla()))
            .child(
                svg()
                    .size(px(12.))
                    .path(path)
                    .text_color(app.text_disabled.hsla())
                    .hover(move |s| s.text_color(app.text.hsla())),
            )
    }
}

impl EntityInputHandler for FindBar {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<String> {
        Some(self.query.text_for_utf16(range_utf16, adjusted))
    }

    fn selected_text_range(
        &mut self,
        _ignore: bool,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<UTF16Selection> {
        Some(self.query.selected_utf16())
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<Range<usize>> {
        self.query.marked_utf16()
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<'_, Self>) {
        self.query.clear_mark();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let r = self.query.edit_range(range_utf16);
        self.apply_edit(r, text, InputMark::Plain, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let r = self.query.edit_range(range_utf16);
        self.apply_edit(r, new_text, InputMark::Marked, cx);
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<Bounds<Pixels>> {
        self.query.ime_bounds(element_bounds)
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<usize> {
        Some(self.query.utf16_index_for_position(point))
    }
}

impl TextInputHost for FindBar {
    fn input(&self) -> Option<&TextInput> {
        Some(&self.query)
    }

    fn input_mut(&mut self) -> Option<&mut TextInput> {
        Some(&mut self.query)
    }

    fn input_focus(&self) -> FocusHandle {
        self.focus.clone()
    }

    fn caret_live(&self, window: &Window) -> bool {
        self.open && self.focus.is_focused(window)
    }
}

fn status_font(chrome: &md_theme::ChromeTokens) -> gpui::Font {
    gpui::Font {
        family: chrome.status_font.into(),
        features: gpui::FontFeatures::default(),
        fallbacks: None,
        weight: gpui::FontWeight::NORMAL,
        style: gpui::FontStyle::Normal,
    }
}

#[derive(Clone, Copy)]
struct CountState {
    empty: bool,
    composing: bool,
    capped: bool,
}

fn count_label(state: CountState, matches: usize, active: Option<usize>) -> String {
    count_label_in(md_i18n::current(), state, matches, active)
}

fn count_label_in(
    lang: md_i18n::Lang,
    state: CountState,
    matches: usize,
    active: Option<usize>,
) -> String {
    if state.empty || state.composing {
        String::new()
    } else if state.capped {
        md_i18n::t_in(lang, md_i18n::Key::FindTooMany).to_string()
    } else if matches == 0 {
        md_i18n::t_in(lang, md_i18n::Key::FindNoMatch).to_string()
    } else {
        let i = active.map_or(0, |i| i + 1);
        format!("{i}/{matches}")
    }
}

#[cfg(test)]
mod tests {
    use super::{COUNT_SLOT_W, CountState, FindBar, QUERY_W, count_label_in, status_font};
    use crate::keymap::Cmd;
    use crate::shell::Shell;
    use gpui::{Bounds, Entity, EntityInputHandler, MouseButton, Pixels, point, px};
    use gpui::{Modifiers, TestAppContext, TextRun, VisualTestContext};
    use md_core::doc::Doc;
    use md_core::document::{editor_options, load_markdown};
    use md_theme::DocumentTheme;
    use std::time::Duration;

    fn shell_with<'a>(
        markdown: &str,
        cx: &'a mut TestAppContext,
    ) -> (Entity<Shell>, &'a mut VisualTestContext) {
        let doc = Doc::new(load_markdown(markdown, editor_options()));
        cx.add_window_view(|_, cx| Shell::new(doc, cx))
    }

    fn open_find(
        shell: &Entity<Shell>,
        seed: &str,
        cx: &mut VisualTestContext,
    ) -> (Entity<FindBar>, Bounds<Pixels>) {
        let find = cx.update(|_, app| shell.read(app).find_bar().clone());
        let seed = seed.to_string();
        cx.update(|window, app| {
            find.update(app, |bar, cx| bar.open(window, seed, cx));
        });
        cx.run_until_parked();
        let bounds = cx
            .update(|_, app| find.read(app).query.bounds())
            .expect("the query field should be painted and its bounds written back this frame");
        (find, bounds)
    }

    const PAST_WINDOW: Duration = Duration::from_millis(200);

    const WITHIN_WINDOW: Duration = Duration::from_millis(40);

    fn type_into_find(find: &Entity<FindBar>, text: &str, cx: &mut VisualTestContext) {
        for ch in text.chars() {
            let s = ch.to_string();
            cx.update(|window, app| {
                find.update(app, |f, cx| {
                    f.replace_text_in_range(None, &s, window, cx);
                });
            });
        }
    }

    fn clear_query(find: &Entity<FindBar>, cx: &mut VisualTestContext) {
        let n = cx.update(|_, app| find.read(app).query().len());
        cx.update(|window, app| {
            find.update(app, |f, cx| {
                f.replace_text_in_range(Some(0..n), "", window, cx);
            });
        });
    }

    fn editor_query(
        editor: &Entity<crate::view::EditorView>,
        cx: &mut VisualTestContext,
    ) -> String {
        cx.update(|_, app| editor.read(app).search.query.clone())
    }

    fn editor_matches(
        editor: &Entity<crate::view::EditorView>,
        cx: &mut VisualTestContext,
    ) -> usize {
        cx.update(|_, app| editor.read(app).search.matches.len())
    }

    #[gpui::test]
    fn clicking_the_query_keeps_focus_in_the_find_bar(cx: &mut TestAppContext) {
        let (shell, cx) = shell_with("needle and needle\n", cx);
        let (find, bounds) = open_find(&shell, "needle", cx);

        cx.update(|window, app| {
            shell.read(app).editor_focus().clone().focus(window);
        });
        cx.run_until_parked();
        assert!(
            cx.update(|window, app| shell.read(app).editor_focus().is_focused(window)),
            "precondition: focus should be on the editor first"
        );

        cx.simulate_mouse_down(
            point(bounds.left() + px(8.), bounds.center().y),
            MouseButton::Left,
            Modifiers::none(),
        );
        assert!(
            cx.update(|window, app| find.read(app).focus.is_focused(window)),
            "clicking the query field should bring focus back to the find bar, not have the editor snatch it at the bubble tail"
        );
    }

    #[gpui::test]
    fn clicking_the_query_places_the_caret_at_that_index(cx: &mut TestAppContext) {
        let (shell, cx) = shell_with("x\n", cx);
        let (find, bounds) = open_find(&shell, "hello world", cx);
        assert_eq!(
            cx.update(|_, app| find.read(app).selection()),
            0.."hello world".len(),
            "opening with a word should select it all, so the next keystroke can replace the old word"
        );
        let x = cx.update(|_, app| {
            let bar = find.read(app);
            let line = bar
                .query
                .shaped()
                .expect("there should be a shape cache this frame");
            bounds.left() + line.x_for_index(6) + px(1.)
        });

        cx.simulate_mouse_down(
            point(x, bounds.center().y),
            MouseButton::Left,
            Modifiers::none(),
        );
        let (caret, selection) = cx.update(|_, app| {
            let bar = find.read(app);
            (bar.caret(), bar.selection())
        });
        assert_eq!(
            caret, 6,
            "clicking just before the w of world should land on 6"
        );
        assert!(
            selection.is_empty(),
            "a single click should collapse the selection to a caret"
        );
    }

    #[test]
    fn count_label_reads_in_english() {
        use md_i18n::Lang::En;
        let count_label = |state, matches, active| count_label_in(En, state, matches, active);
        let base = CountState {
            empty: false,
            composing: false,
            capped: false,
        };
        assert_eq!(
            count_label(
                CountState {
                    empty: true,
                    ..base
                },
                0,
                None
            ),
            ""
        );
        assert_eq!(
            count_label(
                CountState {
                    composing: true,
                    ..base
                },
                3,
                Some(0)
            ),
            "",
            "no count while composing, or every pinyin keystroke would make it jump"
        );
        assert_eq!(
            count_label(
                CountState {
                    capped: true,
                    ..base
                },
                5000,
                Some(0)
            ),
            "Too many"
        );
        assert_eq!(count_label(base, 0, None), "No match");
        assert_eq!(count_label(base, 9, Some(2)), "3/9");
    }

    #[gpui::test]
    fn count_slot_holds_every_label(cx: &mut TestAppContext) {
        let (_shell, cx) = shell_with("x\n", cx);
        let chrome = DocumentTheme::one_dark().chrome;

        let room = px(COUNT_SLOT_W - 4.0);
        let capped = CountState {
            empty: false,
            composing: false,
            capped: true,
        };
        let plain = CountState {
            capped: false,
            ..capped
        };
        for lang in md_i18n::Lang::ALL {
            for label in [
                count_label_in(lang, capped, 5000, Some(0)),
                count_label_in(lang, plain, 0, None),
                count_label_in(lang, plain, 5000, Some(4999)),
            ] {
                let width = cx.update(|window, _| {
                    let run = TextRun {
                        len: label.len(),
                        font: status_font(&chrome),
                        color: gpui::black(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    window
                        .text_system()
                        .shape_line(
                            label.clone().into(),
                            px(chrome.status_size_px),
                            &[run],
                            None,
                        )
                        .width
                });
                assert!(
                    width <= room,
                    "under {}, `{label}` measures {width:?}, past the count slot's {room:?}; it would widen the bar",
                    lang.key()
                );
            }
        }
    }

    #[gpui::test]
    fn the_query_row_is_centred_in_the_field(cx: &mut TestAppContext) {
        let (shell, cx) = shell_with(
            "x
", cx,
        );
        let (find, bounds) = open_find(&shell, "hello", cx);
        let (_, y, w, h) = cx
            .update(|_, app| find.read(app).query.ime_caret())
            .expect("there should be a caret rect once a frame has been painted");
        let chrome = DocumentTheme::one_dark().chrome;
        let paint = DocumentTheme::one_dark().paint;
        assert!(
            (h as f32 - chrome.status_size_px * chrome.status_line_height_em).abs() <= 0.5,
            "line height should be the theme's status line spacing (snapped to the pixel grid); measured {h}"
        );
        assert!(
            (w - paint.caret_width).abs() < 0.01,
            "caret width should come from the theme; measured {w}"
        );
        let field_h = f32::from(bounds.size.height);

        assert!(
            (y as f32 + h as f32 / 2.0 - field_h / 2.0).abs() <= 0.5,
            "the row's midline {} should sit on the field box's midline {}, or the text would sit a notch above the icon",
            y as f32 + h as f32 / 2.0,
            field_h / 2.0
        );
    }

    #[gpui::test]
    fn the_caret_blinks_while_the_bar_has_focus(cx: &mut TestAppContext) {
        let (shell, cx) = shell_with(
            "x
", cx,
        );
        let (find, _) = open_find(&shell, "hello", cx);
        let period = Duration::from_millis(DocumentTheme::one_dark().paint.caret_blink_ms as u64);
        let visible =
            |cx: &mut VisualTestContext| cx.update(|_, app| find.read(app).query.blink_visible());
        assert!(
            visible(cx),
            "the caret should be solid right after the bar opens"
        );

        cx.executor()
            .advance_clock(period + Duration::from_millis(20));
        cx.run_until_parked();
        assert!(!visible(cx), "it should blink off after one period");
        cx.executor()
            .advance_clock(period + Duration::from_millis(20));
        cx.run_until_parked();
        assert!(visible(cx), "after another period it should blink back on");

        cx.executor()
            .advance_clock(period + Duration::from_millis(20));
        cx.run_until_parked();
        assert!(!visible(cx));
        cx.update(|window, app| {
            find.update(app, |bar, cx| {
                bar.replace_text_in_range(None, "z", window, cx);
            });
        });
        assert!(
            visible(cx),
            "input should put the caret back into its on phase"
        );
        cx.executor()
            .advance_clock(period + Duration::from_millis(20));
        cx.run_until_parked();
        assert!(
            visible(cx),
            "the heartbeat right after the input should be swallowed, or a keystroke just before a beat would flash the caret off"
        );
        cx.executor()
            .advance_clock(period + Duration::from_millis(20));
        cx.run_until_parked();
        assert!(
            !visible(cx),
            "blinking off should wait for the next heartbeat"
        );
    }

    #[gpui::test]
    fn the_document_caret_parks_while_the_find_bar_has_focus(cx: &mut TestAppContext) {
        let (shell, cx) = shell_with(
            "needle
", cx,
        );
        let editor = cx.update(|_, app| shell.read(app).editor().clone());
        let period = Duration::from_millis(DocumentTheme::one_dark().paint.caret_blink_ms as u64);
        cx.update(|window, app| {
            shell.read(app).editor_focus().clone().focus(window);
        });
        cx.run_until_parked();
        assert!(
            cx.update(|_, app| editor.read(app).blink.live()),
            "precondition: the document caret is live while focus is on the editor"
        );

        let (_find, _) = open_find(&shell, "needle", cx);
        assert!(
            !cx.update(|_, app| editor.read(app).blink.live()),
            "after focus is handed to the find bar, the document caret should no longer be live, and `caret_on` should be false with it"
        );
        cx.executor()
            .advance_clock(period + Duration::from_millis(20));
        cx.run_until_parked();
        assert!(
            cx.update(|_, app| editor.read(app).blink.visible()),
            "the blurred document caret should hold its on phase rather than keep blinking"
        );
    }

    #[gpui::test]
    fn an_unfocused_bar_stops_blinking(cx: &mut TestAppContext) {
        let (shell, cx) = shell_with(
            "x
", cx,
        );
        let (find, _) = open_find(&shell, "hello", cx);
        cx.update(|window, app| {
            shell.read(app).editor_focus().clone().focus(window);
        });
        cx.run_until_parked();
        let period = Duration::from_millis(DocumentTheme::one_dark().paint.caret_blink_ms as u64);
        cx.executor()
            .advance_clock(period + Duration::from_millis(20));
        cx.run_until_parked();
        assert!(
            cx.update(|_, app| find.read(app).query.blink_visible()),
            "after blur the phase should stay on; the caret is solid again only once focus returns"
        );
    }

    #[gpui::test]
    fn the_query_takes_the_usual_editing_chords(cx: &mut TestAppContext) {
        let (shell, cx) = shell_with("x\n", cx);
        let (find, _) = open_find(&shell, "hello world", cx);
        let query =
            |cx: &mut VisualTestContext| cx.update(|_, app| find.read(app).query().to_string());
        let caret = |cx: &mut VisualTestContext| cx.update(|_, app| find.read(app).caret());
        let word_right = if cfg!(target_os = "macos") {
            "alt-right"
        } else {
            "ctrl-right"
        };
        let word_backspace = if cfg!(target_os = "macos") {
            "alt-backspace"
        } else {
            "ctrl-backspace"
        };

        cx.simulate_keystrokes("home");
        cx.run_until_parked();
        assert_eq!(caret(cx), 0);
        cx.simulate_keystrokes(word_right);
        cx.run_until_parked();
        assert_eq!(
            caret(cx),
            6,
            "word-right should land on the start of the next word"
        );

        cx.simulate_keystrokes("secondary-a");
        cx.run_until_parked();
        cx.simulate_keystrokes("secondary-c");
        cx.run_until_parked();
        assert_eq!(
            cx.update(|_, app| app.read_from_clipboard().and_then(|it| it.text())),
            Some("hello world".to_string()),
            "copy should have read from the query field"
        );
        cx.simulate_keystrokes("end");
        cx.run_until_parked();
        cx.simulate_keystrokes("secondary-v");
        cx.run_until_parked();
        assert_eq!(
            query(cx),
            "hello worldhello world",
            "paste should have landed in the query field"
        );

        cx.simulate_keystrokes(word_backspace);
        cx.run_until_parked();
        assert_eq!(
            query(cx),
            "hello worldhello ",
            "word backspace should have deleted the whole word"
        );

        cx.simulate_keystrokes("secondary-a");
        cx.run_until_parked();
        cx.simulate_keystrokes("secondary-x");
        cx.run_until_parked();
        assert_eq!(query(cx), "", "cut should have read from the query field");
        assert_eq!(
            cx.update(|_, app| app.read_from_clipboard().and_then(|it| it.text())),
            Some("hello worldhello ".to_string())
        );
    }

    #[cfg(target_os = "macos")]
    #[gpui::test]
    fn the_query_takes_primary_to_the_field_edges(cx: &mut TestAppContext) {
        use crate::keymap::{Chord, Mods};
        let (shell, cx) = shell_with("x\n", cx);
        let (find, _) = open_find(&shell, "hello world", cx);
        let caret = |cx: &mut VisualTestContext| cx.update(|_, app| find.read(app).caret());
        let left = Chord::new(Mods::primary(), "left").unparse();
        let right = Chord::new(Mods::primary(), "right").unparse();
        let backspace = Chord::new(Mods::primary(), "backspace").unparse();

        cx.simulate_keystrokes("end");
        cx.run_until_parked();
        assert_eq!(caret(cx), "hello world".len());
        cx.simulate_keystrokes(&left);
        cx.run_until_parked();
        assert_eq!(caret(cx), 0, "⌘← should move to the start of the query");
        cx.simulate_keystrokes(&right);
        cx.run_until_parked();
        assert_eq!(
            caret(cx),
            "hello world".len(),
            "⌘→ should move to the end of the query"
        );
        cx.simulate_keystrokes(&backspace);
        cx.run_until_parked();
        let query = cx.update(|_, app| find.read(app).query().to_string());
        assert_eq!(query, "", "⌘⌫ should clear the query");
        assert_eq!(caret(cx), 0);
    }

    #[gpui::test]
    fn a_long_query_scrolls_to_keep_the_caret_in_the_field(cx: &mut TestAppContext) {
        let (shell, cx) = shell_with("x\n", cx);
        let (find, bounds) = open_find(&shell, "", cx);
        cx.simulate_input("this is a very long search query indeed");
        cx.run_until_parked();

        let (line_w, caret, scroll) = cx.update(|_, app| {
            let bar = find.read(app);
            (
                bar.query
                    .shaped()
                    .map(|l| l.width)
                    .expect("there should be a shape cache"),
                bar.query
                    .ime_caret()
                    .expect("there should be a caret rect")
                    .0 as f32,
                f32::from(bar.query.scroll_x()),
            )
        });
        let field_w = f32::from(bounds.size.width);
        assert!(
            f32::from(line_w) > field_w,
            "precondition: the query text should be wider than the field; measured line width {line_w:?} / field width {field_w}"
        );
        assert!(
            scroll < 0.0,
            "the text is wider than the field but nothing scrolled: {scroll}"
        );

        assert!(
            (0.0..=field_w).contains(&caret),
            "the caret ran off the field: field width {field_w}, caret at {caret}"
        );

        cx.simulate_keystrokes("home");
        cx.run_until_parked();
        let (caret, scroll) = cx.update(|_, app| {
            let bar = find.read(app);
            (
                bar.query.ime_caret().expect("caret rect").0 as f32,
                f32::from(bar.query.scroll_x()),
            )
        });
        assert!(
            scroll.abs() < 3.0,
            "the caret is back at the line start; it should scroll back to the left: {scroll}"
        );
        assert!(
            (0.0..=field_w).contains(&caret),
            "the caret at the line start is outside the field: {caret}"
        );
    }

    #[gpui::test]
    fn the_find_bar_reads_its_keys_from_the_table(cx: &mut TestAppContext) {
        let (shell, cx) = shell_with("needle needle needle\n", cx);

        let (find, _) = open_find(&shell, "", cx);
        cx.simulate_input("needle");
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
        let editor = cx.update(|_, app| shell.read(app).editor().clone());
        let active =
            |cx: &mut VisualTestContext| cx.update(|_, app| editor.read(app).search.active);
        assert_eq!(
            active(cx),
            Some(0),
            "precondition: the opening keystrokes should stop at the first match"
        );

        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(
            active(cx),
            Some(1),
            "Enter in the find bar should jump to the next match"
        );

        let old_next = Cmd::FindNext
            .default_chord()
            .expect("there should be a default chord")
            .unparse();
        cx.update(|_, app| {
            shell.update(app, |s, cx| {
                s.rebind(Cmd::Find, "ctrl-alt-k", cx);
                s.rebind(Cmd::FindNext, "f7", cx);
            });
        });
        cx.run_until_parked();

        cx.simulate_keystrokes(&old_next);
        cx.run_until_parked();
        assert_eq!(
            active(cx),
            Some(1),
            "the old key {old_next} that was unbound still steps to the next match"
        );
        cx.simulate_keystrokes("f7");
        cx.run_until_parked();
        assert_eq!(
            active(cx),
            Some(2),
            "the new key should step to the next match"
        );

        cx.simulate_keystrokes("home");
        cx.run_until_parked();
        assert!(
            cx.update(|_, app| find.read(app).selection().is_empty()),
            "precondition: after Home there should be no selection"
        );
        assert!(
            cx.update(|window, app| find.read(app).focus.is_focused(window)),
            "precondition: focus should be in the find bar"
        );
        cx.simulate_keystrokes("ctrl-alt-k");
        cx.run_until_parked();
        assert!(
            cx.update(|window, app| find.read(app).focus.is_focused(window)),
            "the rebound find key should keep focus in the query field"
        );
        assert!(
            cx.update(|_, app| find.read(app).selection().is_empty()),
            "pressing the find key inside the bar should only wake it, not select the whole word — as if the press leaked through to the shell"
        );
    }

    #[gpui::test]
    fn a_composing_query_swallows_the_step_keys(cx: &mut TestAppContext) {
        let (shell, cx) = shell_with("needle needle needle\n", cx);
        let (find, _) = open_find(&shell, "", cx);
        type_into_find(&find, "needle", cx);
        cx.executor().advance_clock(PAST_WINDOW);
        cx.run_until_parked();
        let editor = cx.update(|_, app| shell.read(app).editor().clone());
        let active =
            |cx: &mut VisualTestContext| cx.update(|_, app| editor.read(app).search.active);
        assert_eq!(
            active(cx),
            Some(0),
            "precondition: the seeded typing should stop at the first match"
        );

        cx.update(|window, app| {
            find.update(app, |f, cx| {
                f.replace_and_mark_text_in_range(None, "ni", None, window, cx);
            });
        });
        cx.run_until_parked();
        assert!(
            cx.update(|_, app| find.read(app).query.composing()),
            "precondition: the query field should be composing"
        );
        assert!(
            cx.update(|window, app| find.read(app).focus.is_focused(window)),
            "precondition: focus should be in the find bar"
        );
        let before = cx.update(|_, app| find.read(app).query().to_string());

        let next = Cmd::FindNext
            .default_chord()
            .expect("there should be a default chord")
            .unparse();
        cx.simulate_keystrokes(&next);
        cx.run_until_parked();
        assert_eq!(
            cx.update(|_, app| find.read(app).query().to_string()),
            before,
            "the composing string was replaced — as if this press bubbled up to the shell and reopened the find bar"
        );
        assert!(
            cx.update(|_, app| find.read(app).query.composing()),
            "the composing state was reset"
        );
        assert_eq!(active(cx), Some(0), "it should not step while composing");

        let before = cx.update(|_, app| find.read(app).query().to_string());
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(
            cx.update(|_, app| find.read(app).query().to_string()),
            before,
            "an Enter during composition should be swallowed too"
        );
        assert_eq!(active(cx), Some(0));
    }

    #[gpui::test]
    fn the_query_field_gets_its_full_width(cx: &mut TestAppContext) {
        let (shell, cx) = shell_with(
            "x
", cx,
        );
        let (_find, bounds) = open_find(&shell, "hello", cx);
        assert_eq!(bounds.size.width, px(QUERY_W));
    }

    #[gpui::test]
    fn typing_four_letters_scans_once(cx: &mut TestAppContext) {
        let (shell, cx) = shell_with(
            "# Heading\n\ndeep water and deeper still\n\n- deep list item\n",
            cx,
        );
        let (find, _) = open_find(&shell, "", cx);
        let editor = cx.update(|_, app| shell.read(app).editor().clone());

        for _ in 0..4 {
            type_into_find(&find, "d", cx);
            cx.run_until_parked();
            assert_eq!(
                editor_query(&editor, cx),
                "",
                "no scan should happen between keystrokes — the debounce window has not burned out yet"
            );
        }
        type_into_find(&find, "x", cx);
        cx.run_until_parked();

        cx.executor().advance_clock(PAST_WINDOW);
        cx.run_until_parked();
        assert_eq!(
            editor_query(&editor, cx),
            "ddddx",
            "only after the window has elapsed since the last keystroke should it scan, and it scans the final query"
        );
    }

    #[gpui::test]
    fn matches_land_after_the_window(cx: &mut TestAppContext) {
        let (shell, cx) = shell_with(
            "# Heading\n\ndeep water and deeper still\n\n- deep list item\n",
            cx,
        );
        let (find, _) = open_find(&shell, "", cx);
        let editor = cx.update(|_, app| shell.read(app).editor().clone());

        type_into_find(&find, "deep", cx);
        cx.run_until_parked();
        assert_eq!(
            editor_matches(&editor, cx),
            0,
            "still inside the window; no matches yet"
        );

        cx.executor().advance_clock(PAST_WINDOW);
        cx.run_until_parked();
        assert_eq!(editor_query(&editor, cx), "deep");
        assert_eq!(
            editor_matches(&editor, cx),
            3,
            "\"deep\" appears three times in the fixture (deep water / deeper / deep list)"
        );
    }

    #[gpui::test]
    fn a_partial_wait_does_not_scan(cx: &mut TestAppContext) {
        let (shell, cx) = shell_with(
            "# Heading\n\ndeep water and deeper still\n\n- deep list item\n",
            cx,
        );
        let (find, _) = open_find(&shell, "", cx);
        let editor = cx.update(|_, app| shell.read(app).editor().clone());

        type_into_find(&find, "de", cx);
        cx.run_until_parked();
        cx.executor().advance_clock(WITHIN_WINDOW);
        cx.run_until_parked();
        assert_eq!(
            editor_query(&editor, cx),
            "",
            "it should not scan before the window has run out"
        );

        cx.executor().advance_clock(PAST_WINDOW);
        cx.run_until_parked();
        assert_eq!(
            editor_query(&editor, cx),
            "de",
            "it scans once the window has run out"
        );
    }

    #[gpui::test]
    fn clearing_the_query_takes_effect_at_once(cx: &mut TestAppContext) {
        let (shell, cx) = shell_with(
            "# Heading\n\ndeep water and deeper still\n\n- deep list item\n",
            cx,
        );
        let (find, _) = open_find(&shell, "", cx);
        let editor = cx.update(|_, app| shell.read(app).editor().clone());

        type_into_find(&find, "deep", cx);
        cx.executor().advance_clock(PAST_WINDOW);
        cx.run_until_parked();
        assert_eq!(editor_matches(&editor, cx), 3);

        clear_query(&find, cx);
        cx.run_until_parked();
        assert_eq!(
            editor_query(&editor, cx),
            "",
            "an empty query should take the direct path"
        );
        assert_eq!(
            editor_matches(&editor, cx),
            0,
            "the highlights should clear right away, not wait for the window"
        );
    }

    #[gpui::test]
    fn a_dismissed_bar_does_not_land_its_scan(cx: &mut TestAppContext) {
        let (shell, cx) = shell_with(
            "# Heading\n\ndeep water and deeper still\n\n- deep list item\n",
            cx,
        );
        let (find, _) = open_find(&shell, "", cx);
        let editor = cx.update(|_, app| shell.read(app).editor().clone());

        type_into_find(&find, "deep", cx);
        cx.run_until_parked();
        find.update(cx, |f, cx| f.dismiss(cx));

        cx.executor().advance_clock(PAST_WINDOW);
        cx.run_until_parked();
        assert_eq!(
            editor_query(&editor, cx),
            "",
            "after dismissing the bar, the pending scan should not run"
        );
        assert_eq!(editor_matches(&editor, cx), 0);
    }

    #[gpui::test]
    fn a_replaced_query_never_scans_the_abandoned_one(cx: &mut TestAppContext) {
        let (shell, cx) = shell_with(
            "# Heading\n\ndeep water and deeper still\n\n- deep list item\n",
            cx,
        );
        let (find, _) = open_find(&shell, "", cx);
        let editor = cx.update(|_, app| shell.read(app).editor().clone());

        type_into_find(&find, "wat", cx);
        cx.run_until_parked();
        assert_eq!(
            editor_query(&editor, cx),
            "",
            "an abandoned word should not be scanned"
        );

        clear_query(&find, cx);
        type_into_find(&find, "deep", cx);
        cx.run_until_parked();
        assert_eq!(
            editor_query(&editor, cx),
            "",
            "right after switching to the new word, it is still inside the window"
        );

        cx.executor().advance_clock(PAST_WINDOW);
        cx.run_until_parked();
        assert_eq!(
            editor_query(&editor, cx),
            "deep",
            "only the last query should be scanned"
        );
        assert_eq!(editor_matches(&editor, cx), 3);
    }
}
