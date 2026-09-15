use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;

const STATE_DIR_NAME: &str = "markdown-editor-rs";
const LEGACY_STATE_DIR_NAME: &str = "md-test";

pub(crate) fn app_state_dir() -> Option<PathBuf> {
    state_dir(STATE_DIR_NAME)
}

fn legacy_app_state_dir() -> Option<PathBuf> {
    state_dir(LEGACY_STATE_DIR_NAME)
}

fn state_dir(name: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        windows_state_dir(name, std::env::var_os("LOCALAPPDATA").as_deref())
    }
    #[cfg(target_os = "macos")]
    {
        macos_state_dir(name, std::env::var_os("HOME").as_deref())
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        xdg_state_dir(
            name,
            std::env::var_os("XDG_STATE_HOME").as_deref(),
            std::env::var_os("HOME").as_deref(),
        )
    }
}

pub fn migrate_legacy_state_dir() {
    if let (Some(legacy), Some(target)) = (legacy_app_state_dir(), app_state_dir()) {
        move_state_dir(&legacy, &target);
    }
}

fn move_state_dir(legacy: &Path, target: &Path) {
    if target.exists() || !legacy.exists() {
        return;
    }
    if let Err(err) = std::fs::rename(legacy, target) {
        eprintln!(
            "failed to move {} to {}: {err}",
            legacy.display(),
            target.display()
        );
    }
}

#[cfg(any(test, windows))]
fn windows_state_dir(name: &str, local_app_data: Option<&OsStr>) -> Option<PathBuf> {
    let local = local_app_data.filter(|path| !path.is_empty())?;
    Some(PathBuf::from(local).join(name))
}

#[cfg(any(test, target_os = "macos"))]
fn macos_state_dir(name: &str, home: Option<&OsStr>) -> Option<PathBuf> {
    let home = home.filter(|path| !path.is_empty())?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join(name),
    )
}

#[cfg(any(test, all(not(windows), not(target_os = "macos"))))]
fn xdg_state_dir(
    name: &str,
    xdg_state_home: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Option<PathBuf> {
    if let Some(state) = xdg_state_home.filter(|s| !s.is_empty()) {
        let path = Path::new(state);
        if path.is_absolute() {
            return Some(path.join(name));
        }
        tracing::warn!(
            path = %path.display(),
            "XDG_STATE_HOME is not an absolute path; falling back to ~/.local/state"
        );
    }
    let Some(home) = home.filter(|path| !path.is_empty()) else {
        tracing::warn!("HOME is unset; not persisting app state");
        return None;
    };
    Some(PathBuf::from(home).join(".local").join("state").join(name))
}

pub(crate) fn log_dir() -> Option<PathBuf> {
    Some(app_state_dir()?.join("logs"))
}

pub(crate) fn recovery_dir() -> Option<PathBuf> {
    Some(app_state_dir()?.join("recovery"))
}

pub(crate) fn settings_path() -> Option<PathBuf> {
    Some(app_state_dir()?.join("settings.json"))
}

pub(crate) fn recent_path() -> Option<PathBuf> {
    Some(app_state_dir()?.join("recent.json"))
}

#[cfg(test)]
mod tests {
    use super::{
        STATE_DIR_NAME, macos_state_dir, move_state_dir, windows_state_dir, xdg_state_dir,
    };
    use std::ffi::OsStr;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_dir(tag: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let mut p = std::env::temp_dir();
        p.push(format!(
            "markdown-editor-rs-paths-{tag}-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn relative_xdg_state_home_falls_back_to_home_local_state() {
        let got = xdg_state_dir(
            STATE_DIR_NAME,
            Some(OsStr::new("rel-state")),
            Some(OsStr::new("home-root")),
        );
        assert_eq!(
            got,
            Some(
                PathBuf::from("home-root")
                    .join(".local")
                    .join("state")
                    .join(STATE_DIR_NAME)
            )
        );
    }

    #[test]
    fn empty_xdg_state_home_falls_back_to_home_local_state() {
        let got = xdg_state_dir(
            STATE_DIR_NAME,
            Some(OsStr::new("")),
            Some(OsStr::new("home-root")),
        );
        assert_eq!(
            got,
            Some(
                PathBuf::from("home-root")
                    .join(".local")
                    .join("state")
                    .join(STATE_DIR_NAME)
            )
        );
    }

    #[test]
    fn absolute_xdg_state_home_is_used() {
        let abs = std::env::temp_dir();
        assert!(
            abs.is_absolute(),
            "temp_dir must be an absolute path, or this test cannot exercise the XDG branch"
        );
        let got = xdg_state_dir(
            STATE_DIR_NAME,
            Some(abs.as_os_str()),
            Some(OsStr::new("home-root")),
        );
        assert_eq!(got, Some(abs.join(STATE_DIR_NAME)));
    }

    #[test]
    fn missing_home_without_usable_xdg_is_none() {
        assert_eq!(xdg_state_dir(STATE_DIR_NAME, None, None), None);
        assert_eq!(
            xdg_state_dir(STATE_DIR_NAME, Some(OsStr::new("rel")), None),
            None
        );
    }

    #[test]
    fn windows_state_uses_local_app_data_and_rejects_missing_roots() {
        assert_eq!(
            windows_state_dir(
                STATE_DIR_NAME,
                Some(OsStr::new(r"C:\Users\lo\AppData\Local"))
            ),
            Some(PathBuf::from(r"C:\Users\lo\AppData\Local").join(STATE_DIR_NAME))
        );
        assert_eq!(
            windows_state_dir(STATE_DIR_NAME, Some(OsStr::new(""))),
            None
        );
        assert_eq!(windows_state_dir(STATE_DIR_NAME, None), None);
    }

    #[test]
    fn macos_state_lives_under_application_support() {
        assert_eq!(
            macos_state_dir(STATE_DIR_NAME, Some(OsStr::new("/Users/lo"))),
            Some(
                PathBuf::from("/Users/lo")
                    .join("Library")
                    .join("Application Support")
                    .join(STATE_DIR_NAME)
            )
        );
        assert_eq!(macos_state_dir(STATE_DIR_NAME, Some(OsStr::new(""))), None);
        assert_eq!(macos_state_dir(STATE_DIR_NAME, None), None);
    }

    #[test]
    fn a_legacy_state_dir_moves_when_the_target_is_missing() {
        let root = unique_dir("move");
        let legacy = root.join("legacy");
        let target = root.join("target");
        std::fs::create_dir_all(legacy.join("logs")).expect("seed");
        std::fs::write(legacy.join("settings.json"), "{}").expect("seed");
        move_state_dir(&legacy, &target);
        assert!(target.join("settings.json").is_file());
        assert!(target.join("logs").is_dir());
        assert!(!legacy.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_existing_target_wins_over_the_legacy_dir() {
        let root = unique_dir("keep");
        let legacy = root.join("legacy");
        let target = root.join("target");
        std::fs::create_dir_all(&legacy).expect("seed");
        std::fs::create_dir_all(&target).expect("seed");
        std::fs::write(target.join("marker"), "new").expect("seed");
        move_state_dir(&legacy, &target);
        assert!(target.join("marker").is_file());
        assert!(legacy.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_legacy_dir_is_a_no_op() {
        let root = unique_dir("noop");
        let legacy = root.join("legacy");
        let target = root.join("target");
        move_state_dir(&legacy, &target);
        assert!(!target.exists());
        assert!(!legacy.exists());
    }
}
