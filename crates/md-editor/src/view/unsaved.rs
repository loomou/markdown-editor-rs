use super::EditorView;
use gpui::{Context, KeyDownEvent, Window};
use md_core::doc::Doc;
use md_core::document::{editor_options, load_markdown};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnsavedChoice {
    Save,
    Discard,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PendingNav {
    New,
    Open,
    Close,
}

pub(crate) fn doc_file_name(doc: &Doc) -> Option<&str> {
    doc.source_path
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
}

pub(crate) fn unsaved_file_name(doc: &Doc) -> &str {
    doc_file_name(doc).unwrap_or("untitled")
}

pub(crate) fn window_title(doc: &Doc) -> String {
    let name = doc_file_name(doc).unwrap_or(crate::APP_NAME);
    if doc.is_dirty() {
        format!("{name} •")
    } else {
        name.to_string()
    }
}

impl EditorView {
    pub(crate) fn bind_window(&mut self, window: &Window) {
        self.host_window = Some(window.window_handle());
    }

    pub(crate) fn sync_os_title(&mut self, window: &mut Window) {
        let title = window_title(&self.state.doc);
        if self.os_title.as_deref() != Some(title.as_str()) {
            self.os_title = Some(title.clone());
            window.set_window_title(&title);
        }
    }

    pub(crate) fn on_window_should_close(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        self.bind_window(window);
        if self.force_close {
            return true;
        }
        if self.save_conflict.is_some() {
            window.focus(&self.save_conflict_focus, cx);
            return false;
        }
        if !self.state.doc.is_dirty() {
            return true;
        }
        self.request_nav(PendingNav::Close, window, cx);
        false
    }

    pub(crate) fn request_nav(
        &mut self,
        nav: PendingNav,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.bind_window(window);
        if self.save_conflict.is_some() {
            window.focus(&self.save_conflict_focus, cx);
            return;
        }
        if !self.state.doc.is_dirty() {
            self.finish_nav(nav, window, cx);
            return;
        }
        self.unsaved_nav = Some(nav);
        window.focus(&self.unsaved_focus, cx);
        cx.notify();
    }

    pub(crate) fn apply_unsaved_choice(
        &mut self,
        choice: UnsavedChoice,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.bind_window(window);
        let Some(nav) = self.unsaved_nav.take() else {
            return;
        };
        match choice {
            UnsavedChoice::Cancel => {
                self.save.pending_after_save = None;
                self.pending_open_path = None;
                window.focus(&self.focus, cx);
                cx.notify();
            }
            UnsavedChoice::Discard => {
                self.finish_nav(nav, window, cx);
            }
            UnsavedChoice::Save => {
                if !self.state.doc.is_dirty() {
                    self.finish_nav(nav, window, cx);
                    return;
                }
                self.save.pending_after_save = Some(nav);
                self.save(window, cx);
                cx.notify();
            }
        }
    }

    pub(super) fn finish_nav(
        &mut self,
        nav: PendingNav,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.unsaved_nav = None;
        self.save.pending_after_save = None;
        match nav {
            PendingNav::New => self.new_untitled(window, cx),
            PendingNav::Open => {
                if let Some(path) = self.pending_open_path.take() {
                    self.open_from_path(path, window, cx);
                } else {
                    self.prompt_open_markdown(window, cx);
                }
            }
            PendingNav::Close => {
                self.discard_recovery_files(cx);
                self.force_close = true;
                window.remove_window();
            }
        }
    }

    pub(crate) fn new_untitled(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.replace_document(Doc::new(load_markdown("", editor_options())), cx);
        self.sync_os_title(window);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(crate) fn on_unsaved_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        if self.unsaved_nav.is_none() {
            return false;
        }
        if ev.is_held {
            return true;
        }
        let m = &ev.keystroke.modifiers;
        let key = ev.keystroke.key.as_str();
        if key == "escape" && !crate::ui::chord::has_chord(m) {
            self.apply_unsaved_choice(UnsavedChoice::Cancel, window, cx);
            return true;
        }
        if key == "enter" && !crate::ui::chord::has_chord(m) && !m.shift {
            self.apply_unsaved_choice(UnsavedChoice::Save, window, cx);
            return true;
        }
        if self
            .keymap
            .chord_for(crate::keymap::Cmd::Save)
            .is_some_and(|c| c.matches(&ev.keystroke))
        {
            self.apply_unsaved_choice(UnsavedChoice::Save, window, cx);
            return true;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::window_title;
    use md_core::doc::{Cursor, Doc};
    use md_core::document::{Command, Sel, editor_options, load_markdown};

    #[test]
    fn window_title_marks_dirty_and_untitled() {
        let mut doc = Doc::new(load_markdown("hi\n", editor_options()));
        assert_eq!(window_title(&doc), crate::APP_NAME);
        doc.source_path = Some(std::path::PathBuf::from("notes.md"));
        assert_eq!(window_title(&doc), "notes.md");
        let leaf = doc.text_leaves()[0];
        doc.apply(
            Sel::collapsed(Cursor {
                block: leaf,
                offset: 0,
            }),
            Command::Insert { text: "x".into() },
        );
        assert_eq!(window_title(&doc), "notes.md •");
    }
}
