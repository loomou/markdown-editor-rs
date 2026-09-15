use crate::platform::fs_atomic::write_bytes_atomic;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const RECENT_VERSION: u64 = 1;
pub const RECENT_LIMIT: usize = 8;

#[derive(Clone, Debug)]
pub struct RecentStore {
    path: PathBuf,
}

impl RecentStore {
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn discover() -> Option<Self> {
        Some(Self::at(crate::platform::paths::recent_path()?))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Vec<PathBuf> {
        let Ok(text) = fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        parse(&text).unwrap_or_default()
    }

    pub fn save(&self, files: &[PathBuf]) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        write_bytes_atomic(&self.path, encode(files).as_bytes())
    }
}

fn encode(files: &[PathBuf]) -> String {
    let mut root = serde_json::Map::new();
    root.insert("version".into(), RECENT_VERSION.into());
    root.insert(
        "files".into(),
        serde_json::Value::Array(
            files
                .iter()
                .filter_map(|p| p.to_str().map(serde_json::Value::from))
                .collect(),
        ),
    );
    let mut text = serde_json::to_string_pretty(&serde_json::Value::Object(root))
        .expect("a tree made of maps, strings, and numbers always serializes");
    text.push('\n');
    text
}

fn parse(text: &str) -> Option<Vec<PathBuf>> {
    let root = serde_json::from_str::<serde_json::Value>(text).ok()?;
    if root.get("version").and_then(serde_json::Value::as_u64) != Some(RECENT_VERSION) {
        return None;
    }
    Some(
        root.get("files")?
            .as_array()?
            .iter()
            .filter_map(|entry| entry.as_str())
            .map(PathBuf::from)
            .collect(),
    )
}

pub fn push(files: &mut Vec<PathBuf>, path: &Path) {
    files.retain(|p| p != path);
    files.insert(0, path.to_path_buf());
    files.truncate(RECENT_LIMIT);
}

#[cfg(test)]
mod tests {
    use super::{RECENT_LIMIT, RecentStore, encode, parse, push};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_path(tag: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let mut p = std::env::temp_dir();
        p.push(format!(
            "md-test-recent-{tag}-{}-{n}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn push_moves_to_front_dedups_and_truncates() {
        let mut files = Vec::new();
        for i in 0..RECENT_LIMIT {
            push(&mut files, &PathBuf::from(format!("/tmp/f{i}.md")));
        }
        push(&mut files, Path::new("/tmp/f0.md"));
        assert_eq!(files[0], Path::new("/tmp/f0.md"));
        assert_eq!(files.len(), RECENT_LIMIT);
        assert_eq!(files.iter().filter(|p| p.ends_with("f0.md")).count(), 1);
        push(&mut files, Path::new("/tmp/new.md"));
        assert_eq!(files.len(), RECENT_LIMIT);
        assert_eq!(files[0], Path::new("/tmp/new.md"));
        assert!(!files.iter().any(|p| p.ends_with("f1.md")));
    }

    #[test]
    fn a_store_round_trips_paths() {
        let path = unique_path("roundtrip");
        let store = RecentStore::at(path.clone());
        let files = vec![PathBuf::from("/tmp/a.md"), PathBuf::from("/tmp/b.md")];
        store.save(&files).expect("save");
        assert_eq!(store.load(), files);
        assert!(path.is_file());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_missing_or_corrupt_file_loads_empty() {
        let store = RecentStore::at(unique_path("missing"));
        assert!(store.load().is_empty());
        let path = unique_path("corrupt");
        std::fs::write(&path, "not json").expect("seed");
        assert!(RecentStore::at(path.clone()).load().is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_unknown_version_loads_empty() {
        assert!(parse(r#"{"version": 99, "files": ["/tmp/a.md"]}"#).is_none());
        assert_eq!(
            parse(&encode(&[PathBuf::from("/tmp/a.md")])),
            Some(vec![PathBuf::from("/tmp/a.md")])
        );
    }
}
