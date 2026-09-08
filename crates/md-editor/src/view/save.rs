use super::EditorView;
use crate::platform::fs_atomic::{
    CheckedWrite, DiskState, disk_state, write_snapshot_atomic, write_snapshot_atomic_if_unchanged,
};
use gpui::{AnyWindowHandle, AsyncApp, Context, KeyDownEvent, WeakEntity, Window};
use md_core::document::WriteSnapshot;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

pub(super) enum SaveDest {
    Existing(PathBuf),
    Ask,
}

pub(super) fn save_destination(path: Option<PathBuf>) -> SaveDest {
    match path {
        Some(path) => SaveDest::Existing(path),
        None => SaveDest::Ask,
    }
}

const AUTOSAVE_IDLE: Duration = Duration::from_secs(2);

enum StartSave {
    Busy,
    Idle,
    Snap(SaveSnapshot),
}

struct SaveSnapshot {
    snap: WriteSnapshot,
    edit_gen: u64,
    epoch: u64,
}

pub(super) enum SaveWriteResult {
    Written(DiskState),
    Conflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SaveConflictChoice {
    Overwrite,
    Cancel,
}

fn apply_save_result(
    this: WeakEntity<EditorView>,
    cx: &mut AsyncApp,
    window_handle: AnyWindowHandle,
    path: PathBuf,
    edit_gen: u64,
    epoch: u64,
    result: io::Result<SaveWriteResult>,
) {
    let _ = window_handle.update(cx, |_, window, cx| {
        let _ = this.update(cx, |v, cx| {
            v.settle_save(path, edit_gen, epoch, result, window, cx)
        });
    });
}

async fn complete_save(
    this: WeakEntity<EditorView>,
    cx: &mut AsyncApp,
    window_handle: AnyWindowHandle,
    path: PathBuf,
    save: SaveSnapshot,
    expected: Option<DiskState>,
) {
    let write_path = path.clone();
    let SaveSnapshot {
        snap,
        edit_gen,
        epoch,
    } = save;
    let result = cx
        .background_executor()
        .spawn(async move {
            if let Some(expected) = expected {
                return match write_snapshot_atomic_if_unchanged(&snap, &write_path, expected)? {
                    CheckedWrite::Written(state) => Ok(SaveWriteResult::Written(state)),
                    CheckedWrite::Conflict(_) => Ok(SaveWriteResult::Conflict),
                };
            }
            write_snapshot_atomic(&snap, &write_path)?;
            disk_state(&write_path).map(SaveWriteResult::Written)
        })
        .await;
    apply_save_result(this, cx, window_handle, path, edit_gen, epoch, result);
}

async fn start_save_from_async(
    this: WeakEntity<EditorView>,
    cx: &mut AsyncApp,
    window_handle: AnyWindowHandle,
    path: PathBuf,
    force: bool,
) {
    let path = if force {
        crate::platform::open_markdown::with_markdown_extension(path)
    } else {
        path
    };
    let started = this.update(cx, |v, _| {
        if v.save.in_flight {
            return StartSave::Busy;
        }
        if !force && !v.state.doc.is_dirty() {
            return StartSave::Idle;
        }
        v.save.in_flight = true;
        v.save.epoch = v.save.epoch.wrapping_add(1);
        let epoch = v.save.epoch;
        let snap = v.state.doc.document.write_snapshot();
        let edit_gen = v.state.doc.edit_gen();
        StartSave::Snap(SaveSnapshot {
            snap,
            edit_gen,
            epoch,
        })
    });
    match started {
        Ok(StartSave::Snap(save)) => {
            complete_save(this, cx, window_handle, path, save, None).await;
        }
        Ok(StartSave::Idle) => {
            let _ = window_handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |v, cx| {
                    if let Some(nav) = v.save.pending_after_save.take() {
                        v.finish_nav(nav, window, cx);
                    }
                });
            });
        }
        Ok(StartSave::Busy) => {
            let _ = this.update(cx, |v, _| {
                v.save.save_as_retry = Some(path);
            });
        }
        Err(_) => {}
    }
}

impl EditorView {
    pub(super) fn settle_save(
        &mut self,
        path: PathBuf,
        edit_gen: u64,
        epoch: u64,
        result: io::Result<SaveWriteResult>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let stale = self.save.epoch != epoch;
        self.save.in_flight = false;
        if stale {
            return;
        }
        match result {
            Ok(SaveWriteResult::Written(disk_state)) => {
                self.state.doc.mark_saved(edit_gen);
                self.state.doc.source_path = Some(path);
                self.save.source_disk_state = Some(disk_state);
                self.save_conflict = None;
                if matches!(self.notice, Some(crate::Error::Save { .. })) {
                    self.notice = None;
                }
                if !self.state.doc.is_dirty() {
                    self.discard_recovery_files(cx);
                } else {
                    self.schedule_recovery(cx);
                }
                self.sync_os_title(window);
                cx.notify();
                self.continue_after_save(window, cx);
            }
            Ok(SaveWriteResult::Conflict) => {
                self.unsaved_nav = None;
                self.save_conflict = Some(path);
                window.focus(&self.save_conflict_focus);
                cx.notify();
            }
            Err(source) => {
                self.save.pending_after_save = None;
                self.unsaved_nav = None;
                window.focus(&self.focus);
                self.set_notice(crate::Error::Save { path, source }, cx);
            }
        }
    }

    pub(crate) fn set_autosave(&mut self, on: bool) {
        self.autosave = on;
        self.save.autosave_epoch = self.save.autosave_epoch.wrapping_add(1);
    }

    #[cfg(test)]
    pub(crate) fn autosave(&self) -> bool {
        self.autosave
    }

    pub(crate) fn note_edit(&mut self, cx: &mut Context<'_, Self>) {
        self.schedule_recovery(cx);
        if !self.autosave || self.state.doc.source_path.is_none() || !self.state.doc.is_dirty() {
            return;
        }
        self.save.autosave_epoch = self.save.autosave_epoch.wrapping_add(1);
        let epoch = self.save.autosave_epoch;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(AUTOSAVE_IDLE).await;
            let _ = this.update(cx, |v, cx| v.fire_autosave(epoch, cx));
        })
        .detach();
    }

    fn fire_autosave(&mut self, epoch: u64, cx: &mut Context<'_, Self>) {
        if self.save.autosave_epoch != epoch {
            return;
        }
        if !self.autosave || !self.state.doc.is_dirty() {
            return;
        }
        let Some(path) = self.state.doc.source_path.clone() else {
            return;
        };
        if self.save.in_flight {
            self.save.autosave_retry = true;
            return;
        }
        let Some(handle) = self.host_window else {
            return;
        };
        self.begin_save_with_handle(path, false, handle, cx);
    }

    fn continue_after_save(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if let Some(nav) = self.save.pending_after_save.take() {
            if let Some(path) = self.save.save_as_retry.take() {
                self.save.pending_after_save = Some(nav);
                self.begin_save_with_handle(path, true, window.window_handle(), cx);
                return;
            }
            if self.state.doc.is_dirty() {
                self.save.pending_after_save = Some(nav);
                match self.state.doc.source_path.clone() {
                    Some(path) => {
                        self.begin_save_with_handle(path, false, window.window_handle(), cx)
                    }
                    None => self.save_as(window, cx),
                }
            } else {
                self.finish_nav(nav, window, cx);
            }
            return;
        }
        if self.save.autosave_retry {
            self.save.autosave_retry = false;
            self.note_edit(cx);
        }
        if let Some(path) = self.save.save_as_retry.take() {
            self.begin_save_with_handle(path, true, window.window_handle(), cx);
        }
    }

    pub(crate) fn save(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.bind_window(window);
        match save_destination(self.state.doc.source_path.clone()) {
            SaveDest::Existing(path) => self.begin_save(path, false, window, cx),
            SaveDest::Ask => self.save_as(window, cx),
        }
    }

    pub(crate) fn apply_save_conflict_choice(
        &mut self,
        choice: SaveConflictChoice,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.bind_window(window);
        let Some(path) = self.save_conflict.take() else {
            return;
        };
        match choice {
            SaveConflictChoice::Cancel => {
                self.save.pending_after_save = None;
                window.focus(&self.focus);
                cx.notify();
            }
            SaveConflictChoice::Overwrite => {
                self.begin_save_with_handle(path, true, window.window_handle(), cx);
            }
        }
    }

    pub(crate) fn on_save_conflict_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        if self.save_conflict.is_none() {
            return false;
        }
        if ev.is_held {
            return true;
        }
        let modifiers = &ev.keystroke.modifiers;
        let key = ev.keystroke.key.as_str();
        if key == "escape" && !crate::ui::chord::has_chord(modifiers) {
            self.apply_save_conflict_choice(SaveConflictChoice::Cancel, window, cx);
            return true;
        }
        if key == "enter" && !crate::ui::chord::has_chord(modifiers) && !modifiers.shift {
            self.apply_save_conflict_choice(SaveConflictChoice::Overwrite, window, cx);
            return true;
        }
        true
    }

    pub(crate) fn save_as(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.bind_window(window);
        self.save.save_as_retry = None;
        let suggested = self.state.doc.source_path.clone();
        let window_handle = window.window_handle();
        #[cfg(windows)]
        {
            let owner = crate::platform::open_markdown::gpui_hwnd() as isize;
            cx.spawn(async move |this, cx| {
                let path = cx
                    .background_executor()
                    .spawn(async move {
                        crate::platform::open_markdown::pick_markdown_save_path(
                            owner as _,
                            suggested.as_deref(),
                        )
                    })
                    .await;
                let Some(path) = path else {
                    let _ = window_handle.update(cx, |_, _, cx| {
                        let _ = this.update(cx, |v, _| {
                            v.save.pending_after_save = None;
                            v.unsaved_nav = None;
                        });
                    });
                    return;
                };
                start_save_from_async(this, cx, window_handle, path, true).await;
            })
            .detach();
        }
        #[cfg(not(windows))]
        {
            let dir = suggested
                .as_ref()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
            let name = suggested
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("untitled.md");
            let rx = cx.prompt_for_new_path(&dir, Some(name));
            cx.spawn(async move |this, cx| {
                let Ok(Ok(Some(path))) = rx.await else {
                    let _ = window_handle.update(cx, |_, _, cx| {
                        let _ = this.update(cx, |v, _| {
                            v.save.pending_after_save = None;
                            v.unsaved_nav = None;
                        });
                    });
                    return;
                };
                start_save_from_async(this, cx, window_handle, path, true).await;
            })
            .detach();
        }
    }

    pub(super) fn begin_save(
        &mut self,
        path: PathBuf,
        force: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.bind_window(window);
        self.begin_save_with_handle(path, force, window.window_handle(), cx);
    }

    fn begin_save_with_handle(
        &mut self,
        path: PathBuf,
        force: bool,
        window_handle: AnyWindowHandle,
        cx: &mut Context<'_, Self>,
    ) {
        if self.save.in_flight || self.save_conflict.is_some() {
            return;
        }
        if !force && !self.state.doc.is_dirty() {
            return;
        }
        self.save.in_flight = true;
        self.save.epoch = self.save.epoch.wrapping_add(1);
        let epoch = self.save.epoch;
        let snap = self.state.doc.document.write_snapshot();
        let edit_gen = self.state.doc.edit_gen();
        let expected = (!force).then_some(self.save.source_disk_state).flatten();
        let save = SaveSnapshot {
            snap,
            edit_gen,
            epoch,
        };
        cx.spawn(async move |this, cx| {
            complete_save(this, cx, window_handle, path, save, expected).await;
        })
        .detach();
    }

    #[cfg(test)]
    pub(crate) fn save_as_confirmed_for_test(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            start_save_from_async(this, cx, window_handle, path, true).await;
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::{SaveDest, save_destination};
    use crate::platform::fs_atomic::{tmp_path, write_snapshot_atomic};
    use md_core::document::{editor_options, load_markdown};
    use md_core::inline::InlineMarks;
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_path(tag: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let mut p = std::env::temp_dir();
        p.push(format!("md-test-save-{tag}-{}-{n}.md", std::process::id()));
        p
    }

    fn cleanup(path: &Path) {
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(tmp_path(path));
    }

    #[test]
    fn save_without_path_asks() {
        assert!(matches!(save_destination(None), SaveDest::Ask));
        assert!(matches!(
            save_destination(Some(PathBuf::from("a.md"))),
            SaveDest::Existing(_)
        ));
    }

    #[test]
    fn atomic_write_round_trips_kind_and_marks() {
        let path = unique_path("round");
        cleanup(&path);
        let md = "# title\n\nhello `d` and *em*\n";
        let doc = load_markdown(md, editor_options());
        let snap = doc.write_snapshot();
        write_snapshot_atomic(&snap, &path).expect("write");
        let loaded = fs::read_to_string(&path).expect("read");
        let again = load_markdown(&loaded, editor_options());
        assert_eq!(again.to_markdown(), doc.to_markdown());
        let id = again.live_id(again.text_leaves()[1]).expect("para");
        assert!(
            again
                .runs(id)
                .iter()
                .any(|r| r.marks.contains(InlineMarks::CODE))
        );
        assert!(
            again
                .runs(id)
                .iter()
                .any(|r| r.marks.contains(InlineMarks::EM))
        );
        cleanup(&path);
    }

    #[test]
    fn atomic_write_replaces_existing_file() {
        let path = unique_path("replace");
        cleanup(&path);
        fs::write(&path, "old\n").expect("seed");
        let doc = load_markdown("# new\n", editor_options());
        write_snapshot_atomic(&doc.write_snapshot(), &path).expect("replace");
        let loaded = fs::read_to_string(&path).expect("read");
        assert!(loaded.contains("# new"), "{loaded:?}");
        assert!(!loaded.contains("old"), "{loaded:?}");
        cleanup(&path);
    }

    #[test]
    fn atomic_write_to_directory_fails() {
        let path = unique_path("dirfail");
        cleanup(&path);
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("dir");
        let doc = load_markdown("# new\n", editor_options());
        assert!(write_snapshot_atomic(&doc.write_snapshot(), &path).is_err());
        let _ = fs::remove_dir_all(&path);
        let _ = fs::remove_file(tmp_path(&path));
    }
}
