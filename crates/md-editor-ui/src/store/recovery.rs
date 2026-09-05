use crate::platform::fs_atomic::{tmp_path, write_bytes_atomic};
use md_core::doc::Doc;
use md_core::document::{WriteSnapshot, editor_options, load_markdown};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const RECOVERY_NAME: &str = "draft.json";
const LEGACY_DRAFT_NAME: &str = "draft.md";
const LEGACY_META_NAME: &str = "draft.meta";
const RECOVERY_VERSION: u64 = 1;

#[derive(Clone, Debug)]
pub struct Recovery {
    dir: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingDraft {
    pub markdown: String,
    pub source_path: Option<PathBuf>,
}

impl PendingDraft {
    pub fn file_name(&self) -> &str {
        self.source_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("untitled")
    }

    pub fn into_doc(self) -> Doc {
        let mut doc = Doc::with_path(
            load_markdown(&self.markdown, editor_options()),
            self.source_path,
        );
        doc.mark_unsaved();
        doc
    }
}

impl Recovery {
    pub fn in_dir(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn discover() -> Option<Self> {
        Some(Self::in_dir(crate::platform::paths::recovery_dir()?))
    }

    pub fn load(&self) -> Option<PendingDraft> {
        let bytes = fs::read(self.recovery_path()).ok()?;
        let mut record: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        let record = record.as_object_mut()?;
        if record.get("version")?.as_u64()? != RECOVERY_VERSION {
            return None;
        }
        let markdown = record.remove("markdown")?.as_str()?.to_owned();
        let source_path = match record.remove("path") {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::String(path)) if path.is_empty() => None,
            Some(serde_json::Value::String(path)) => Some(PathBuf::from(path)),
            Some(_) => return None,
        };
        Some(PendingDraft {
            markdown,
            source_path,
        })
    }

    pub fn write(&self, snap: &WriteSnapshot, source_path: Option<&Path>) -> io::Result<()> {
        self.write_markdown(&snap.to_markdown(), source_path)
    }

    pub fn write_markdown(&self, markdown: &str, source_path: Option<&Path>) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let path = match source_path {
            Some(path) => match path.to_str() {
                Some(path) => Some(path.to_owned()),
                None => {
                    tracing::warn!(
                        path = %path.display(),
                        "recovery source path is not UTF-8; omitting it"
                    );
                    None
                }
            },
            None => None,
        };
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version": RECOVERY_VERSION,
            "markdown": markdown,
            "path": path,
        }))
        .map_err(io::Error::other)?;
        write_bytes_atomic(&self.recovery_path(), &bytes)
    }

    pub fn clear(&self) -> io::Result<()> {
        for path in [
            self.recovery_path(),
            self.dir.join(LEGACY_DRAFT_NAME),
            self.dir.join(LEGACY_META_NAME),
        ] {
            remove_quiet(&path)?;
            remove_quiet(&tmp_path(&path))?;
        }
        Ok(())
    }

    fn recovery_path(&self) -> PathBuf {
        self.dir.join(RECOVERY_NAME)
    }
}

pub fn load() -> Option<PendingDraft> {
    Recovery::discover()?.load()
}

pub fn startup() -> Option<(Doc, String)> {
    let draft = load()?;
    let name = draft.file_name().to_string();
    tracing::info!(path = %name, "restored crash recovery draft");
    let title = format!("md-test · {name} •");
    Some((draft.into_doc(), title))
}

fn remove_quiet(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::{PendingDraft, RECOVERY_NAME, Recovery};
    use md_core::document::{editor_options, load_markdown};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_dir(tag: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let mut p = std::env::temp_dir();
        p.push(format!("md-test-recovery-{tag}-{}-{n}", std::process::id()));
        p
    }

    fn cleanup(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn write_then_load_round_trips_newline_in_path() {
        let dir = unique_dir("nl-path");
        cleanup(&dir);
        let store = Recovery::in_dir(dir.clone());
        let doc = load_markdown("# hi\n", editor_options());
        let path = PathBuf::from("notes\nfile.md");
        store
            .write(&doc.write_snapshot(), Some(&path))
            .expect("write");
        let pending = store.load().expect("load");
        assert_eq!(pending.source_path.as_ref(), Some(&path));
        cleanup(&dir);
    }

    #[test]
    fn write_then_load_round_trips_markdown_and_path() {
        let dir = unique_dir("round");
        cleanup(&dir);
        let store = Recovery::in_dir(dir.clone());
        let doc = load_markdown("# hi\n\nbody\n", editor_options());
        let path = PathBuf::from("notes.md");
        store
            .write(&doc.write_snapshot(), Some(&path))
            .expect("write");
        let pending = store.load().expect("load");
        assert_eq!(pending.source_path.as_ref(), Some(&path));
        assert_eq!(pending.file_name(), "notes.md");
        let again = load_markdown(&pending.markdown, editor_options());
        assert_eq!(again.to_markdown(), doc.to_markdown());
        store.clear().expect("clear");
        assert!(store.load().is_none());
        cleanup(&dir);
    }

    #[test]
    fn untitled_draft_has_empty_source_path() {
        let dir = unique_dir("untitled");
        cleanup(&dir);
        let store = Recovery::in_dir(dir.clone());
        let doc = load_markdown("scratch\n", editor_options());
        store.write(&doc.write_snapshot(), None).expect("write");
        let pending = store.load().expect("load");
        assert!(pending.source_path.is_none());
        assert_eq!(pending.file_name(), "untitled");
        cleanup(&dir);
    }

    #[test]
    fn incomplete_or_legacy_pairs_are_ignored() {
        let dir = unique_dir("incomplete");
        cleanup(&dir);
        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join("draft.md"), "# new draft\n").expect("draft");
        fs::write(
            dir.join("draft.meta"),
            r#"{"version":2,"rev":1,"path":"old.md"}"#,
        )
        .expect("meta");
        let store = Recovery::in_dir(dir.clone());
        assert!(store.load().is_none());
        cleanup(&dir);
    }

    #[test]
    fn corrupt_record_is_ignored() {
        let dir = unique_dir("corrupt");
        cleanup(&dir);
        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join(RECOVERY_NAME), b"{not json").expect("record");
        let store = Recovery::in_dir(dir.clone());
        assert!(store.load().is_none());
        cleanup(&dir);
    }

    #[test]
    fn later_write_replaces_previous_draft() {
        let dir = unique_dir("replace");
        cleanup(&dir);
        let store = Recovery::in_dir(dir.clone());
        let first = load_markdown("one\n", editor_options());
        store.write(&first.write_snapshot(), None).expect("first");
        let second = load_markdown("two\n", editor_options());
        store.write(&second.write_snapshot(), None).expect("second");
        let pending = store.load().expect("load");
        assert!(pending.markdown.contains("two"), "{:?}", pending.markdown);
        assert!(!pending.markdown.contains("one"), "{:?}", pending.markdown);
        cleanup(&dir);
    }

    #[test]
    fn into_doc_keeps_path_and_stays_dirty() {
        let draft = PendingDraft {
            markdown: "# recovered\n".into(),
            source_path: Some(PathBuf::from("notes.md")),
        };
        let doc = draft.into_doc();
        assert!(doc.is_dirty());
        assert_eq!(doc.source_path.as_deref(), Some(Path::new("notes.md")));
        assert!(doc.document.to_markdown().contains("recovered"));
    }
}
