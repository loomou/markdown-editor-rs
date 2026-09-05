use std::ffi::OsStr;
#[cfg(any(test, all(not(windows), not(target_os = "macos"))))]
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn app_state_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        windows_state_dir(std::env::var_os("LOCALAPPDATA").as_deref())
    }
    #[cfg(target_os = "macos")]
    {
        macos_state_dir(std::env::var_os("HOME").as_deref())
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        xdg_state_dir(
            std::env::var_os("XDG_STATE_HOME").as_deref(),
            std::env::var_os("HOME").as_deref(),
        )
    }
}

#[cfg(any(test, windows))]
fn windows_state_dir(local_app_data: Option<&OsStr>) -> Option<PathBuf> {
    let local = local_app_data.filter(|path| !path.is_empty())?;
    Some(PathBuf::from(local).join("md-test"))
}

#[cfg(any(test, target_os = "macos"))]
fn macos_state_dir(home: Option<&OsStr>) -> Option<PathBuf> {
    let home = home.filter(|path| !path.is_empty())?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("md-test"),
    )
}

#[cfg(any(test, all(not(windows), not(target_os = "macos"))))]
fn xdg_state_dir(xdg_state_home: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf> {
    if let Some(state) = xdg_state_home.filter(|s| !s.is_empty()) {
        let path = Path::new(state);
        if path.is_absolute() {
            return Some(path.join("md-test"));
        }
        tracing::warn!(
            path = %path.display(),
            "XDG_STATE_HOME is not an absolute path; falling back to ~/.local/state"
        );
    }
    let Some(home) = home.filter(|s| !s.is_empty()) else {
        tracing::warn!("HOME is unset; not persisting app state");
        return None;
    };
    Some(
        PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("md-test"),
    )
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

#[cfg(test)]
mod tests {
    use super::{macos_state_dir, windows_state_dir, xdg_state_dir};
    use std::ffi::OsStr;
    use std::path::PathBuf;

    #[test]
    fn relative_xdg_state_home_falls_back_to_home_local_state() {
        let got = xdg_state_dir(Some(OsStr::new("rel-state")), Some(OsStr::new("home-root")));
        assert_eq!(
            got,
            Some(
                PathBuf::from("home-root")
                    .join(".local")
                    .join("state")
                    .join("md-test")
            )
        );
    }

    #[test]
    fn empty_xdg_state_home_falls_back_to_home_local_state() {
        let got = xdg_state_dir(Some(OsStr::new("")), Some(OsStr::new("home-root")));
        assert_eq!(
            got,
            Some(
                PathBuf::from("home-root")
                    .join(".local")
                    .join("state")
                    .join("md-test")
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
        let got = xdg_state_dir(Some(abs.as_os_str()), Some(OsStr::new("home-root")));
        assert_eq!(got, Some(abs.join("md-test")));
    }

    #[test]
    fn missing_home_without_usable_xdg_is_none() {
        assert_eq!(xdg_state_dir(None, None), None);
        assert_eq!(xdg_state_dir(Some(OsStr::new("rel")), None), None);
    }

    #[test]
    fn windows_state_uses_local_app_data_and_rejects_missing_roots() {
        assert_eq!(
            windows_state_dir(Some(OsStr::new(r"C:\Users\lo\AppData\Local"))),
            Some(PathBuf::from(r"C:\Users\lo\AppData\Local").join("md-test"))
        );
        assert_eq!(windows_state_dir(Some(OsStr::new(""))), None);
        assert_eq!(windows_state_dir(None), None);
    }

    #[test]
    fn macos_state_lives_under_application_support() {
        assert_eq!(
            macos_state_dir(Some(OsStr::new("/Users/lo"))),
            Some(
                PathBuf::from("/Users/lo")
                    .join("Library")
                    .join("Application Support")
                    .join("md-test")
            )
        );
        assert_eq!(macos_state_dir(Some(OsStr::new(""))), None);
        assert_eq!(macos_state_dir(None), None);
    }
}
