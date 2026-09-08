use super::{DocShapeMaps, EditorView, PaintFault};
use gpui::{AnyWindowHandle, AsyncApp, Context, WeakEntity, Window};
use md_core::doc::{Cursor, Doc};
use md_core::document::{editor_options, load_markdown};

struct OpenedDocument {
    doc: Doc,
    disk_state: crate::platform::fs_atomic::DiskState,
}

fn apply_open_result(
    this: WeakEntity<EditorView>,
    cx: &mut AsyncApp,
    window_handle: AnyWindowHandle,
    path: std::path::PathBuf,
    epoch: u64,
    edit_gen: u64,
    result: std::io::Result<OpenedDocument>,
) {
    let _ = window_handle.update(cx, |_, window, cx| {
        let _ = this.update(cx, |view, cx| {
            if view.open_epoch != epoch || view.state.doc.edit_gen() != edit_gen {
                return;
            }
            match result {
                Ok(opened) => {
                    view.replace_document(opened.doc, cx);
                    view.save.source_disk_state = Some(opened.disk_state);
                    view.sync_os_title(window);
                    cx.notify();
                }
                Err(source) => view.set_notice(crate::Error::Read { path, source }, cx),
            }
        });
    });
}

impl EditorView {
    pub(crate) fn replace_document(&mut self, mut doc: Doc, cx: &mut Context<'_, Self>) {
        doc.enable_trailing_blank();
        self.discard_recovery_files(cx);
        self.mermaid.clear(cx);
        self.drop_zoom_raster(cx);
        self.math.clear(cx);
        self.images.clear(cx);
        let first = doc.first_text_leaf().unwrap_or(0);
        self.state.doc = doc;
        self.state.cursor = Cursor {
            block: first,
            offset: 0,
        };
        self.state.selection = None;
        self.state.marked = None;
        self.state.scroll = 0.0;
        self.state.resolved_top = 0.0;
        self.state.shape_cache = md_content::shaper::ShapeCache::new();
        self.doc_maps = DocShapeMaps::empty();
        self.state.incremental = None;
        self.state.incremental_anchor_override = None;
        self.state.incremental_last_anchor = None;
        self.state.frame_times.clear();
        self.state.last_stable = None;
        self.state.paint_fault = PaintFault::None;
        self.pending_click = None;
        self.drag_pointer = None;
        self.pending_vertical = None;
        self.select_anchor = None;
        self.dragging = false;
        self.enter_block_edit_on_click = false;
        self.scrollbar_drag = None;
        self.follow_caret = true;
        self.park_caret_top = None;
        self.wake_caret();
        self.search = Default::default();
        self.well_scroll.clear();
        self.pending_open = None;
        self.open_epoch = self.open_epoch.wrapping_add(1);
        self.ime_stale = false;
        self.save = super::SaveState {
            source_disk_state: self
                .state
                .doc
                .source_path
                .as_deref()
                .and_then(|path| crate::platform::fs_atomic::disk_state(path).ok()),
            ..Default::default()
        };
        self.save_conflict = None;
        self.unsaved_nav = None;
        self.insert_table = None;
        self.os_title = None;
        self.notice = None;
        self.table_ui = Default::default();
        self.media_hover = None;
        self.media_chrome = None;
        self.media_chrome_hover = false;
        self.media_zoom = None;
    }

    pub(super) fn open_from_path(
        &mut self,
        path: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if !crate::platform::open_markdown::is_markdown_path(&path) {
            self.set_notice(crate::Error::NotMarkdown { path }, cx);
            return;
        }
        self.bind_window(window);
        self.open_epoch = self.open_epoch.wrapping_add(1);
        let epoch = self.open_epoch;
        let edit_gen = self.state.doc.edit_gen();
        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let read_path = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let (markdown, disk_state) =
                        crate::platform::fs_atomic::read_to_string_with_state(&read_path)?;
                    Ok(OpenedDocument {
                        doc: Doc::with_path(
                            load_markdown(&markdown, editor_options()),
                            Some(read_path),
                        ),
                        disk_state,
                    })
                })
                .await;
            apply_open_result(this, cx, window_handle, path, epoch, edit_gen, result);
        })
        .detach();
    }

    pub(crate) fn prompt_open_markdown(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let window_handle = window.window_handle();
        #[cfg(windows)]
        {
            let owner = crate::platform::open_markdown::gpui_hwnd() as isize;
            cx.spawn(async move |this, cx| {
                let path =
                    cx.background_executor()
                        .spawn(async move {
                            crate::platform::open_markdown::pick_markdown_file(owner as _)
                        })
                        .await;
                let Some(path) = path else {
                    return;
                };
                let _ = window_handle.update(cx, |_, window, cx| {
                    let _ = this.update(cx, |view, cx| view.open_from_path(path, window, cx));
                });
            })
            .detach();
        }
        #[cfg(not(windows))]
        {
            let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some(md_i18n::t(md_i18n::Key::DlgOpenMarkdown).into()),
            });
            cx.spawn(async move |this, cx| {
                let Ok(Ok(Some(paths))) = rx.await else {
                    return;
                };
                let Some(path) = paths.into_iter().next() else {
                    return;
                };
                let _ = window_handle.update(cx, |_, window, cx| {
                    let _ = this.update(cx, |view, cx| view.open_from_path(path, window, cx));
                });
            })
            .detach();
        }
    }
}
