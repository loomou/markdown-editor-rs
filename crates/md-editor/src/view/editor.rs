use super::jobs::{extend_link_dests, extra_snapshots};
use super::{
    CursorMotion, Diagnostics, DocShapeMaps, EditorElement, EditorState, EditorView, PaintFault,
};
use gpui::{
    Context, ExternalPaths, InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Render,
    Styled, Window, div,
};
use md_content::gpui_theme::ThemeColorExt;
use md_content::images::{self, ImageCache};
use md_content::math::MathCache;
use md_content::mermaid::MermaidCache;
use md_core::Px;
use md_core::block::{BlockId, BlockKind};
use md_core::doc::{Cursor, Doc};
use md_layout::style::BoxLayoutEnvironment;
use md_theme::DocumentTheme;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

impl DocShapeMaps {
    pub(super) fn empty() -> Self {
        DocShapeMaps {
            revision: u64::MAX,
            path: None,
            links_len: 0,
            link_dests: Rc::new(HashMap::new()),
            link_raw: Rc::new(HashMap::new()),
            data_source_links: Rc::new(HashMap::new()),
            block_image_dest: Rc::new(HashMap::new()),
            block_code_lang: Rc::new(HashMap::new()),
        }
    }

    pub(super) fn sync(&mut self, doc: &Doc) {
        let path_changed = self.path != doc.source_path;
        if self.revision == doc.document.revision() && !path_changed {
            return;
        }
        let text_only = !path_changed && doc.document.pending_changes().is_text_only();
        self.revision = doc.document.revision();
        if path_changed {
            self.path = doc.source_path.clone();
            self.links_len = 0;
            self.link_dests = Rc::new(HashMap::new());
            self.link_raw = Rc::new(HashMap::new());
            self.data_source_links = Rc::new(HashMap::new());
        }

        if self.links_len < doc.document.links.len() {
            let dests = Rc::make_mut(&mut self.link_dests);
            let raw = Rc::make_mut(&mut self.link_raw);
            let data_sources = Rc::make_mut(&mut self.data_source_links);
            self.links_len = extend_link_dests(doc, self.links_len, dests, raw, data_sources);
        }

        if text_only {
            return;
        }
        let (block_image_dest, block_code_lang) = extra_snapshots(doc);
        self.block_image_dest = Rc::new(block_image_dest);
        self.block_code_lang = Rc::new(block_code_lang);
    }
}

impl EditorView {
    pub fn new(mut doc: Doc, theme: DocumentTheme, cx: &mut Context<'_, Self>) -> Self {
        doc.enable_trailing_blank();
        let source_disk_state = doc
            .source_path
            .as_deref()
            .and_then(|path| crate::platform::fs_atomic::disk_state(path).ok());
        let first = doc.first_text_leaf().unwrap_or(0);
        EditorView {
            state: EditorState {
                doc,
                cursor: Cursor {
                    block: first,
                    offset: 0,
                },
                selection: None,
                marked: None,
                scroll: 0.0,
                resolved_top: 0.0,
                env: BoxLayoutEnvironment::default(),
                shape_cache: md_content::shaper::ShapeCache::new(),
                diag: Rc::new(RefCell::new(Diagnostics::default())),
                incremental: None,
                incremental_enabled: true,
                incremental_anchor_override: None,
                incremental_last_anchor: None,
                frame_times: std::collections::VecDeque::new(),
                show_fps: false,
                stress_redraw: false,
                last_stable: None,
                paint_fault: PaintFault::None,
                theme,
            },
            focus: cx.focus_handle(),
            pending_click: None,
            drag_pointer: None,
            pending_vertical: None,
            select_anchor: None,
            dragging: false,
            enter_block_edit_on_click: false,
            scrollbar_drag: None,
            follow_caret: false,
            park_caret_top: None,
            stale_paint: false,
            blink: crate::ui::blink::Blink::default(),
            keymap: crate::keymap::Keymap::default(),
            find_bar: None,
            search_open: false,
            search: Default::default(),
            mermaid: MermaidCache::new(),
            zoom_raster: None,
            zoom_raster_job: None,
            math: MathCache::new(),
            images: ImageCache::new(),
            remote_images: false,
            doc_maps: DocShapeMaps::empty(),
            well_scroll: HashMap::new(),
            well_bar_drag: None,
            pending_open: None,
            open_epoch: 0,
            ime_stale: false,
            save: super::SaveState {
                source_disk_state,
                ..Default::default()
            },
            save_conflict: None,
            save_conflict_focus: cx.focus_handle(),
            autosave: false,
            host_window: None,
            force_close: false,
            unsaved_nav: None,
            unsaved_focus: cx.focus_handle(),
            insert_table: None,
            insert_table_focus: cx.focus_handle(),
            os_title: None,
            notice: None,
            recovery: None,
            recovery_epoch: 0,
            table_ui: Default::default(),
            media_hover: None,
            media_chrome: None,
            media_chrome_hover: false,
            media_zoom: None,
        }
    }

    pub(crate) fn set_remote_images(&mut self, allow: bool, cx: &mut Context<'_, Self>) {
        if self.remote_images == allow {
            return;
        }
        self.remote_images = allow;
        self.images.clear(cx);
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn remote_images(&self) -> bool {
        self.remote_images
    }

    pub(crate) fn set_theme(&mut self, theme: DocumentTheme, cx: &mut Context<'_, Self>) {
        if self.state.theme == theme {
            return;
        }
        let moved = !self.state.theme.layout_metrics_eq(&theme);
        self.state.theme = theme;
        if moved {
            self.state.shape_cache = md_content::shaper::ShapeCache::new();
            self.state.incremental = None;
            self.state.incremental_anchor_override = None;
            self.state.incremental_last_anchor = None;
        }
        self.state.last_stable = None;
        cx.notify();
    }

    pub(crate) fn jump_to_block(&mut self, block: BlockId) {
        self.place_cursor(Cursor { block, offset: 0 }, CursorMotion::Move);
        self.follow_caret = false;
        self.search.reveal = None;
        self.park_caret_top = Some(0);
    }

    pub(crate) fn follow_line_slack(&self) -> Px {
        super::scrollbar::park_block_top_margin(
            self.state.theme.box_style(BlockKind::Paragraph).margin.top,
        )
    }

    pub(crate) fn start_blink(&mut self, cx: &mut Context<'_, Self>) {
        let period = Duration::from_millis(self.state.theme.paint.caret_blink_ms as u64);
        self.blink.start(period, cx, |v| Some(&mut v.blink));
    }

    #[cfg(test)]
    pub(crate) fn stop_blink(&mut self) {
        self.blink.stop();
    }

    pub(crate) fn wake_caret(&mut self) {
        self.blink.wake();
    }

    pub(crate) fn set_notice(&mut self, err: crate::Error, cx: &mut Context<'_, Self>) {
        tracing::error!(error = %err);
        self.notice = Some(err);
        cx.notify();
    }

    pub(crate) fn dismiss_notice(&mut self, cx: &mut Context<'_, Self>) {
        self.notice = None;
        cx.notify();
    }
}

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        self.bind_window(window);
        self.sync_os_title(window);
        let overlay = self
            .table_ui
            .chrome
            .filter(|_| self.media_zoom.is_none())
            .map(|chrome| {
                super::table_toolbar::overlay(
                    chrome,
                    self.state.scroll,
                    self.table_ui.more_open,
                    self.table_ui.picker_open,
                    self.table_ui.picker_hover,
                    self.state.theme,
                    self.keymap
                        .chord_for(crate::keymap::Cmd::TableRowBelow)
                        .map(crate::keymap::Chord::display),
                    cx.entity(),
                )
            });
        let media_btn = if self.media_zoom.is_none() {
            self.media_chrome.map(|chrome| {
                super::media_zoom::chrome_overlay(
                    chrome,
                    self.state.scroll,
                    self.state.theme,
                    cx.entity(),
                )
            })
        } else {
            None
        };
        let zoom_close = self
            .media_zoom
            .as_ref()
            .map(|_| super::media_zoom::close_overlay(self.state.theme, cx.entity()));
        div()
            .relative()
            .size_full()
            .bg(self.state.theme.paint.canvas.hsla())
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                if this.on_key(ev, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .can_drop(|v, _, _| {
                v.downcast_ref::<ExternalPaths>()
                    .is_some_and(|p| p.paths().iter().any(|path| images::is_image_path(path)))
            })
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.drop_images(paths.paths(), window, cx);
            }))
            .child(EditorElement { state: cx.entity() })
            .children(overlay)
            .children(media_btn)
            .children(zoom_close)
    }
}
