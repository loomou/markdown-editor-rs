use md_core::document::WriteSnapshot;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write as IoWrite};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiskState {
    Missing,
    Present { modified: SystemTime, len: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckedWrite {
    Written(DiskState),
    Conflict(DiskState),
}

fn state_from_metadata(metadata: &fs::Metadata) -> io::Result<DiskState> {
    Ok(DiskState::Present {
        modified: metadata.modified()?,
        len: metadata.len(),
    })
}

pub fn disk_state(path: &Path) -> io::Result<DiskState> {
    match fs::metadata(path) {
        Ok(metadata) => state_from_metadata(&metadata),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(DiskState::Missing),
        Err(err) => Err(err),
    }
}

pub fn read_to_string_with_state(path: &Path) -> io::Result<(String, DiskState)> {
    let mut file = File::open(path)?;
    let before = state_from_metadata(&file.metadata()?)?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    let after = state_from_metadata(&file.metadata()?)?;
    if before != after {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "file changed while it was being read",
        ));
    }
    Ok((text, after))
}

struct IoFmt<W> {
    inner: W,
    err: Option<io::Error>,
}

impl<W: IoWrite> fmt::Write for IoFmt<W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.err.is_some() {
            return Err(fmt::Error);
        }
        if let Err(e) = self.inner.write_all(s.as_bytes()) {
            self.err = Some(e);
            return Err(fmt::Error);
        }
        Ok(())
    }
}

pub fn tmp_path(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_os_string();
    p.push(".tmp");
    PathBuf::from(p)
}

fn write_tmp(tmp: &Path, snap: &WriteSnapshot) -> io::Result<()> {
    let file = File::create(tmp)?;
    let mut buf = BufWriter::new(file);
    let mut w = IoFmt {
        inner: &mut buf,
        err: None,
    };
    if snap.write_markdown(&mut w).is_err() {
        return Err(w
            .err
            .take()
            .unwrap_or_else(|| io::Error::other("markdown write")));
    }
    buf.flush()?;
    buf.get_ref().sync_all()
}

#[cfg(windows)]
fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let from_w: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to_w: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    let ok = unsafe {
        MoveFileExW(
            from_w.as_ptr(),
            to_w.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent(_: &Path) -> io::Result<()> {
    Ok(())
}

pub(crate) fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = tmp_path(path);
    let write_result = (|| {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()
    })();
    if let Err(e) = write_result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    if let Err(e) = replace_file(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    sync_parent(path)
}

pub fn write_snapshot_atomic(snap: &WriteSnapshot, path: &Path) -> io::Result<()> {
    let tmp = tmp_path(path);
    if let Err(e) = write_tmp(&tmp, snap) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    if let Err(e) = replace_file(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    sync_parent(path)
}

pub fn write_snapshot_atomic_if_unchanged(
    snap: &WriteSnapshot,
    path: &Path,
    expected: DiskState,
) -> io::Result<CheckedWrite> {
    let tmp = tmp_path(path);
    if let Err(err) = write_tmp(&tmp, snap) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    let current = match disk_state(path) {
        Ok(current) => current,
        Err(err) => {
            let _ = fs::remove_file(&tmp);
            return Err(err);
        }
    };
    if current != expected {
        let _ = fs::remove_file(&tmp);
        return Ok(CheckedWrite::Conflict(current));
    }
    if let Err(err) = replace_file(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    sync_parent(path)?;
    disk_state(path).map(CheckedWrite::Written)
}

#[cfg(test)]
mod tests {
    use super::{
        CheckedWrite, disk_state, tmp_path, write_bytes_atomic, write_snapshot_atomic_if_unchanged,
    };
    use md_core::document::{editor_options, load_markdown};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_path() -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "md-test-checked-save-{}-{n}.md",
            std::process::id()
        ))
    }

    fn unique_atomic_path(tag: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir()
            .join(format!("md-test-atomic-{tag}-{}-{n}", std::process::id()))
            .join("café.md")
    }

    fn cleanup_atomic_path(path: &Path) {
        if let Some(root) = path.parent() {
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn checked_write_leaves_an_externally_changed_file_untouched() {
        let path = unique_path();
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(tmp_path(&path));
        fs::write(&path, "opened\n").expect("seed");
        let expected = disk_state(&path).expect("state");
        fs::write(&path, "changed elsewhere\n").expect("external edit");

        let doc = load_markdown("editor version\n", editor_options());
        let result = write_snapshot_atomic_if_unchanged(&doc.write_snapshot(), &path, expected)
            .expect("checked write");

        assert!(matches!(result, CheckedWrite::Conflict(_)));
        assert_eq!(
            fs::read_to_string(&path).expect("read"),
            "changed elsewhere\n"
        );
        assert!(!tmp_path(&path).exists());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn atomic_bytes_replace_an_existing_unicode_path_and_remove_the_tmp() {
        let path = unique_atomic_path("replace");
        cleanup_atomic_path(&path);
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(&path, b"old").expect("seed old file");

        write_bytes_atomic(&path, "new content\n".as_bytes()).expect("atomic replace");

        assert_eq!(
            fs::read_to_string(&path).expect("read result"),
            "new content\n"
        );
        assert!(
            !tmp_path(&path).exists(),
            "successful write left a tmp file"
        );
        cleanup_atomic_path(&path);
    }

    #[test]
    fn a_replace_failure_keeps_the_destination_and_removes_the_tmp() {
        let path = unique_atomic_path("directory-target");
        cleanup_atomic_path(&path);
        fs::create_dir_all(&path).expect("destination directory");

        assert!(write_bytes_atomic(&path, b"new").is_err());
        assert!(path.is_dir(), "the destination directory was damaged");
        assert!(!tmp_path(&path).exists(), "failed replace left a tmp file");
        cleanup_atomic_path(&path);
    }

    #[test]
    fn a_write_failure_does_not_leave_a_tmp_file() {
        let path = unique_atomic_path("missing-parent");
        cleanup_atomic_path(&path);

        assert!(write_bytes_atomic(&path, b"new").is_err());
        assert!(!path.exists());
        assert!(!tmp_path(&path).exists(), "failed write left a tmp file");
        cleanup_atomic_path(&path);
    }
}
