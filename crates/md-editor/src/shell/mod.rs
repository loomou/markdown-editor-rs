mod caption;
mod color_picker;
mod colors;
mod dialogs;
mod menu;
mod outline;
mod settings;
mod settings_state;
mod shortcuts;
mod status;
mod titlebar;

use self::color_picker::ColorPicker;
use self::menu::{MenuId, clamp_menu_pos};
use self::outline::{OUTLINE_MIN_VIEWPORT, OutlineCache};
use self::status::StatusCache;

use crate::ui::theme::{OUTLINE_W, ShellTheme, UI_FONT};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, AppContext, Context, Div, Entity, EntityInputHandler, FocusHandle, InteractiveElement,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, ParentElement, Pixels, Point, Render,
    ScrollHandle, Styled, UniformListScrollHandle, Window, actions, div, point, px,
};

use md_core::Px;
use md_core::doc::{Cursor, Doc};
use md_theme::{ColorGroup, DocumentTheme};
use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

use crate::keymap::Cmd;
use crate::store::settings::{Settings, SettingsStore};
use crate::ui::text_input::{TextInput, TextInputHost};
use crate::view::find_bar::FindBar;
use crate::view::{EditorView, SaveConflictChoice, UnsavedChoice};

const VIEW_MARGIN: f32 = 8.0;

actions!(md_editor, [CloseFind]);

pub struct Shell {
    settings: Settings,
    settings_store: Option<SettingsStore>,
    settings_mtime: Option<std::time::SystemTime>,
    _activation: Option<gpui::Subscription>,
    outline_open: bool,
    outline_width: f32,
    outline_resize: Option<(Pixels, f32)>,
    outline_scroll: UniformListScrollHandle,
    outline_sb_drag: Option<(bool, Pixels)>,
    open_menu: Option<(MenuId, Point<Pixels>)>,
    table_submenu_open: bool,
    menu_closed_at: Option<std::time::Instant>,
    find: Entity<FindBar>,
    show_settings: bool,
    settings_nav: usize,
    font_slider_drag: bool,
    font_track: Rc<Cell<Option<gpui::Bounds<Pixels>>>>,
    settings_scroll: ScrollHandle,
    color_groups_open: [bool; ColorGroup::COUNT],
    color_picker: Option<ColorPicker>,
    hex_focus: FocusHandle,
    recording: Option<Cmd>,
    record_note: Option<(Cmd, String)>,
    open_last_on_start: bool,
    focus: FocusHandle,
    editor: Entity<EditorView>,
    editor_focus: FocusHandle,
    status_cache: StatusCache,
    outline_cache: OutlineCache,
    outline_current: Option<u32>,
    editor_chrome: Option<EditorChromeKey>,
    caption_should_move: bool,
}

#[derive(Clone, Copy, PartialEq)]
struct EditorChromeKey {
    identity: u64,
    revision: u64,
    cursor: Cursor,
    scroll: Px,
    theme: DocumentTheme,
    unsaved: bool,
    save_conflict: bool,
    insert_table: bool,
    notice: bool,
}

impl Shell {
    pub fn new(doc: Doc, cx: &mut Context<'_, Self>) -> Self {
        let settings = Settings::default();
        let document_theme = settings.appearance.document_theme();
        let mut editor_focus = None;
        let editor = cx.new(|cx| {
            let mut v = EditorView::new(doc, document_theme, cx);
            v.start_blink(cx);
            editor_focus = Some(v.focus.clone());
            v
        });
        let find = cx.new(|cx| FindBar::new(editor.clone(), cx));
        editor.update(cx, |view, _| {
            view.find_bar = Some(find.downgrade());
        });
        cx.observe(&editor, |shell, editor, cx| {
            let editor = editor.read(cx);
            let key = EditorChromeKey {
                identity: editor.state.doc.identity(),
                revision: editor.state.doc.document.revision(),
                cursor: editor.state.cursor,
                scroll: editor.state.scroll,
                theme: editor.state.theme,
                unsaved: editor.unsaved_nav.is_some(),
                save_conflict: editor.save_conflict.is_some(),
                insert_table: editor.insert_table.is_some(),
                notice: editor.notice.is_some(),
            };
            if shell.editor_chrome == Some(key) {
                return;
            }
            shell.editor_chrome = Some(key);
            cx.notify();
        })
        .detach();
        cx.observe(&find, |_, _, cx| cx.notify()).detach();
        Self {
            settings,
            settings_store: None,
            settings_mtime: None,
            _activation: None,
            outline_open: false,
            outline_width: OUTLINE_W,
            outline_resize: None,
            outline_scroll: UniformListScrollHandle::new(),
            outline_sb_drag: None,
            open_menu: None,
            table_submenu_open: false,
            menu_closed_at: None,
            find,
            show_settings: false,
            settings_nav: 0,
            font_slider_drag: false,
            font_track: Rc::new(Cell::new(None)),
            settings_scroll: ScrollHandle::new(),
            color_groups_open: [false; ColorGroup::COUNT],
            color_picker: None,
            hex_focus: cx.focus_handle(),
            recording: None,
            record_note: None,
            open_last_on_start: false,
            focus: cx.focus_handle(),
            editor,
            editor_focus: editor_focus.expect("editor view holds a focus handle"),
            status_cache: StatusCache::default(),
            outline_cache: OutlineCache::default(),
            outline_current: None,
            editor_chrome: None,
            caption_should_move: false,
        }
    }

    pub fn editor_focus(&self) -> &FocusHandle {
        &self.editor_focus
    }

    #[cfg(test)]
    pub(crate) fn find_bar(&self) -> &Entity<FindBar> {
        &self.find
    }

    #[cfg(test)]
    pub(crate) fn editor(&self) -> &Entity<EditorView> {
        &self.editor
    }

    pub fn boot(
        &mut self,
        notice: Option<crate::Error>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.load_settings(cx);
        self._activation = Some(cx.observe_window_activation(window, |shell, window, cx| {
            if window.is_window_active() {
                shell.reload_settings_if_changed(cx);
            }
        }));
        if let Some(store) = crate::store::recovery::Recovery::discover() {
            self.editor
                .update(cx, |editor, _| editor.set_recovery(store));
        }
        if let Some(err) = notice {
            self.set_startup_notice(err, cx);
        }
        window.focus(&self.editor_focus);
    }

    fn theme(&self) -> ShellTheme {
        ShellTheme::from_app(&self.settings.appearance.document_theme().app)
    }

    fn body(&self, this: Entity<Self>) -> Div {
        div()
            .flex_1()
            .relative()
            .overflow_hidden()
            .on_mouse_down(MouseButton::Right, {
                move |ev: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                    this.update(cx, |shell, cx| {
                        let in_table = shell.editor.read(cx).caret_in_table();
                        let pos = clamp_menu_pos(
                            ev.position,
                            MenuId::Context,
                            window.viewport_size(),
                            in_table,
                        );
                        shell.clear_table_submenu_state(cx);
                        shell.open_menu = Some((MenuId::Context, pos));
                        cx.notify();
                    });
                }
            })
            .child(self.editor.clone())
            .child(self.find.clone())
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let t = self.theme();
        let this = cx.entity();
        let editor = self.editor.read(cx);
        let show_outline =
            self.outline_open && window.viewport_size().width >= px(OUTLINE_MIN_VIEWPORT);
        let status = if self.show_settings {
            None
        } else {
            Some(
                self.status_cache
                    .get(&editor.state.doc, editor.state.cursor),
            )
        };
        if show_outline {
            if self.outline_cache.get(window, &editor.state.doc) {
                self.outline_current = None;
                let handle = self.outline_scroll.0.borrow().base_handle.clone();
                let cur = handle.offset();
                handle.set_offset(point(cur.x, px(0.)));
            }
        }
        let picker_hidden = self.color_picker.as_ref().is_some_and(|p| !p.showing());
        if picker_hidden
            && self
                .color_picker
                .as_mut()
                .is_some_and(ColorPicker::cancel_drag)
        {
            self.save_settings();
        }
        if let Some(slot) = self.color_picker.as_ref().map(ColorPicker::slot) {
            let color = self.slot_color(slot, &self.settings.appearance.document_theme());
            let focused = self.hex_focused(window);
            if let Some(picker) = self.color_picker.as_mut() {
                picker.sync_hex(color, focused);
            }
        }

        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(t.editor_bg)
            .text_color(t.text)
            .font_family(UI_FONT)
            .text_size(px(13.))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                if this.record_key(ev, window, cx) {
                    cx.stop_propagation();
                    return;
                }
                if ev.is_held {
                    return;
                }
                let Some(cmd) = this.settings.keymap.lookup(&ev.keystroke) else {
                    return;
                };
                if this.run_command(cmd, window, cx) {
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &CloseFind, window, cx| {
                if this.cancel_recording(window, cx) {
                    return;
                }
                if this.hex_escape(window, cx) {
                    return;
                }
                if this.editor.read(cx).save_conflict.is_some() {
                    this.editor.update(cx, |editor, cx| {
                        editor.apply_save_conflict_choice(SaveConflictChoice::Cancel, window, cx);
                    });
                    cx.notify();
                    return;
                }
                if this.editor.read(cx).insert_table.is_some() {
                    this.editor.update(cx, |editor, cx| {
                        editor.close_insert_table(window, cx);
                    });
                    cx.notify();
                    return;
                }
                if this.editor.read(cx).unsaved_nav.is_some() {
                    this.editor.update(cx, |editor, cx| {
                        editor.apply_unsaved_choice(UnsavedChoice::Cancel, window, cx);
                    });
                    cx.notify();
                    return;
                }
                this.find.update(cx, |find, cx| find.close(window, cx));
                cx.notify();
            }))
            .child(self.title_bar(t, this.clone(), window))
            .children(self.notice_bar(
                t,
                this.clone(),
                editor.notice.as_ref().map(ToString::to_string),
            ))
            .children(if self.show_settings {
                Some(self.settings_page(t, this.clone()))
            } else {
                None
            })
            .when(!self.show_settings, |root| {
                root.child(
                    div()
                        .flex_1()
                        .flex()
                        .overflow_hidden()
                        .child(self.body(this.clone()))
                        .children(show_outline.then(|| {
                            self.outline_panel(t, this.clone(), window.viewport_size().width, cx)
                        })),
                )
            })
            .child(self.status_bar(t, this.clone(), status))
            .children(self.open_menu.map(|(id, pos)| {
                let in_table = id == MenuId::Context && editor.caret_in_table();
                self.menu_popup(t, id, pos, in_table, window.viewport_size(), this.clone())
            }))
            .children(self.color_picker_overlay(t, &this))
            .children(self.unsaved_overlay(t, this.clone(), editor))
            .children(self.insert_table_overlay(t, this.clone(), editor))
            .children(self.save_conflict_overlay(t, this, editor))
    }
}

impl EntityInputHandler for Shell {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<String> {
        let picker = self.color_picker.as_ref()?;
        Some(picker.hex.text_for_utf16(range_utf16, adjusted))
    }

    fn selected_text_range(
        &mut self,
        _ignore: bool,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<gpui::UTF16Selection> {
        Some(self.color_picker.as_ref()?.hex.selected_utf16())
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<Range<usize>> {
        self.color_picker.as_ref()?.hex.marked_utf16()
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<'_, Self>) {
        if let Some(picker) = self.color_picker.as_mut() {
            picker.hex.clear_mark();
        }
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(picker) = self.color_picker.as_ref() else {
            return;
        };
        let r = picker.hex.edit_range(range_utf16);
        self.hex_replace(r, text, cx);
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
        let Some(picker) = self.color_picker.as_ref() else {
            return;
        };
        let r = picker.hex.edit_range(range_utf16);
        self.hex_replace(r, new_text, cx);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: gpui::Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Bounds<Pixels>> {
        self.color_picker.as_ref()?.hex.ime_bounds(element_bounds)
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> Option<usize> {
        Some(
            self.color_picker
                .as_ref()?
                .hex
                .utf16_index_for_position(point),
        )
    }
}

impl TextInputHost for Shell {
    fn input(&self) -> Option<&TextInput> {
        Some(&self.color_picker.as_ref()?.hex)
    }

    fn input_mut(&mut self) -> Option<&mut TextInput> {
        Some(&mut self.color_picker.as_mut()?.hex)
    }

    fn input_focus(&self) -> FocusHandle {
        self.hex_focus.clone()
    }

    fn caret_live(&self, window: &Window) -> bool {
        self.hex_focused(window)
    }
}

#[cfg(test)]
mod tests;
