mod caption;
mod color_picker;
mod colors;
mod dialogs;
mod font_menu;
mod menu;
mod outline;
mod settings;
mod settings_state;
mod shortcuts;
mod status;
mod titlebar;

use self::color_picker::ColorPicker;
use self::font_menu::FontMenu;
use self::menu::{MenuId, clamp_menu_pos};
use self::outline::{OUTLINE_MIN_VIEWPORT, OUTLINE_OVERDRAW, OutlineCache};
use self::status::StatusCache;

use crate::ui::theme::{OUTLINE_ROW_H, OUTLINE_W, ShellTheme, UI_FONT};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, AppContext, Context, Div, Entity, EntityInputHandler, FocusHandle, InteractiveElement,
    IntoElement, KeyDownEvent, ListAlignment, ListState, MouseButton, MouseDownEvent,
    ParentElement, Pixels, Point, Render, ScrollHandle, Styled, Window, actions, div, px,
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
use crate::view::{EditorView, SaveConflictChoice, UnsavedChoice, doc_file_name};

const VIEW_MARGIN: f32 = 8.0;

const GEOMETRY_SAVE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

actions!(md_editor, [CloseFind]);

pub struct Shell {
    settings: Settings,
    settings_store: Option<SettingsStore>,
    settings_mtime: Option<std::time::SystemTime>,
    _activation: Option<gpui::Subscription>,
    outline_open: bool,
    reading: bool,
    outline_width: f32,
    outline_resize: Option<(Pixels, f32)>,
    outline_scroll: ListState,
    outline_follow: Option<(usize, bool)>,
    outline_sb_drag: Option<Pixels>,
    open_menu: Option<(MenuId, Point<Pixels>)>,
    table_submenu_open: bool,
    menu_closed_at: Option<std::time::Instant>,
    recent_store: Option<crate::store::recent::RecentStore>,
    recent_files: Vec<std::path::PathBuf>,
    window_store: Option<crate::store::window::WindowStore>,
    window_geometry: crate::store::window::WindowGeometry,
    geometry_saved_at: Option<std::time::Instant>,
    geometry_generation: u64,
    _window_bounds: Option<gpui::Subscription>,
    find: Entity<FindBar>,
    show_settings: bool,
    settings_nav: usize,
    font_slider_drag: bool,
    font_track: Rc<Cell<Option<gpui::Bounds<Pixels>>>>,
    settings_scroll: ScrollHandle,
    color_groups_open: [bool; ColorGroup::COUNT],
    color_picker: Option<ColorPicker>,
    font_menu: Option<FontMenu>,
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
            shell.note_recent_file(editor.state.doc.source_path.as_deref());
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
            reading: false,
            outline_width: OUTLINE_W,
            outline_resize: None,
            outline_scroll: ListState::new(0, ListAlignment::Top, px(OUTLINE_OVERDRAW))
                .measure_all(),
            outline_follow: None,
            outline_sb_drag: None,
            open_menu: None,
            table_submenu_open: false,
            menu_closed_at: None,
            recent_store: None,
            recent_files: Vec::new(),
            window_store: None,
            window_geometry: Default::default(),
            geometry_saved_at: None,
            geometry_generation: 0,
            _window_bounds: None,
            find,
            show_settings: false,
            settings_nav: 0,
            font_slider_drag: false,
            font_track: Rc::new(Cell::new(None)),
            settings_scroll: ScrollHandle::new(),
            color_groups_open: [false; ColorGroup::COUNT],
            color_picker: None,
            font_menu: None,
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
        if let Some(store) = crate::store::recent::RecentStore::discover() {
            self.recent_files = store.load();
            self.recent_store = Some(store);
        }
        let startup_path = self.editor.read(cx).state.doc.source_path.clone();
        self.note_recent_file(startup_path.as_deref());
        let window_store = crate::store::window::WindowStore::discover();
        if let Some(geometry) = window_store.as_ref().and_then(|store| store.load()) {
            self.window_geometry = geometry;
        } else {
            self.window_geometry = crate::store::window::WindowGeometry::from_window(
                window.bounds(),
                window.is_maximized(),
            );
        }
        self.window_store = window_store;
        self._window_bounds = Some(cx.observe_window_bounds(window, |shell, window, cx| {
            shell.note_window_bounds(window, cx);
        }));
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
        window.focus(&self.editor_focus, cx);
    }

    fn theme(&self) -> ShellTheme {
        ShellTheme::from_app(&self.settings.appearance.document_theme().app)
    }

    fn note_window_bounds(&mut self, window: &Window, cx: &mut Context<'_, Self>) {
        if window.is_maximized() {
            self.window_geometry.maximized = true;
        } else {
            self.window_geometry =
                crate::store::window::WindowGeometry::from_window(window.bounds(), false);
        }
        self.geometry_generation += 1;
        if self
            .geometry_saved_at
            .is_none_or(|at| at.elapsed() >= GEOMETRY_SAVE_INTERVAL)
        {
            self.flush_window_geometry();
            return;
        }
        let generation = self.geometry_generation;
        cx.spawn(async move |shell, cx| {
            cx.background_executor().timer(GEOMETRY_SAVE_INTERVAL).await;
            let _ = shell.update(cx, |shell, _| {
                if shell.geometry_generation == generation {
                    shell.flush_window_geometry();
                }
            });
        })
        .detach();
    }

    pub(crate) fn flush_window_geometry(&mut self) {
        if !self.window_geometry.is_restorable() {
            return;
        }
        if let Some(store) = &self.window_store {
            let _ = store.save(self.window_geometry);
        }
        self.geometry_saved_at = Some(std::time::Instant::now());
    }

    fn note_recent_file(&mut self, path: Option<&std::path::Path>) {
        let Some(path) = path else {
            return;
        };
        if self.recent_files.first().is_some_and(|p| p == path) {
            return;
        }
        crate::store::recent::push(&mut self.recent_files, path);
        if let Some(store) = &self.recent_store {
            let _ = store.save(&self.recent_files);
        }
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
                            0,
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
        let file_name = doc_file_name(&editor.state.doc);
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
            let switched = self.outline_cache.get(&editor.state.doc);
            let count = self.outline_cache.rows.len();
            if switched {
                self.outline_current = None;
            }
            if switched || self.outline_scroll.item_count() != count {
                self.outline_scroll
                    .reset_with_uniform_height(count, px(OUTLINE_ROW_H));
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
            .child(self.title_bar(t, this.clone(), window, file_name))
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
            .children(self.menu_scroll_dismisser(this.clone()))
            .children(self.color_picker_overlay(t, &this))
            .children(self.font_menu_overlay(t, &this))
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
