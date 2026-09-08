use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

pub const MOON: &str = "icons/moon.svg";
pub const SLIDERS: &str = "icons/sliders.svg";
pub const OUTLINE: &str = "icons/outline.svg";
pub const OUTLINE_MINI: &str = "icons/outline-mini.svg";
pub const SEARCH: &str = "icons/search.svg";
pub const CHEV_UP: &str = "icons/chev-up.svg";
pub const CHEV_DOWN: &str = "icons/chev-down.svg";
pub const FIND_CLOSE: &str = "icons/find-close.svg";
pub const WIN_MIN: &str = "icons/win-min.svg";
pub const WIN_MAX: &str = "icons/win-max.svg";
pub const WIN_RESTORE: &str = "icons/win-restore.svg";
pub const WIN_CLOSE: &str = "icons/win-close.svg";
pub const TABLE_GRID: &str = "icons/table-grid.svg";
pub const ALIGN_START: &str = "icons/align-start.svg";
pub const ALIGN_CENTER: &str = "icons/align-center.svg";
pub const ALIGN_END: &str = "icons/align-end.svg";
pub const TABLE_MORE: &str = "icons/table-more.svg";
pub const TABLE_DELETE: &str = "icons/table-delete.svg";

pub struct ShellAssets;

impl ShellAssets {
    pub fn from_crate_assets() -> Self {
        Self
    }
}

fn embedded(path: &str) -> Option<&'static [u8]> {
    Some(match path {
        MOON => include_bytes!("../../assets/icons/moon.svg"),
        SLIDERS => include_bytes!("../../assets/icons/sliders.svg"),
        OUTLINE => include_bytes!("../../assets/icons/outline.svg"),
        OUTLINE_MINI => include_bytes!("../../assets/icons/outline-mini.svg"),
        SEARCH => include_bytes!("../../assets/icons/search.svg"),
        CHEV_UP => include_bytes!("../../assets/icons/chev-up.svg"),
        CHEV_DOWN => include_bytes!("../../assets/icons/chev-down.svg"),
        FIND_CLOSE => include_bytes!("../../assets/icons/find-close.svg"),
        WIN_MIN => include_bytes!("../../assets/icons/win-min.svg"),
        WIN_MAX => include_bytes!("../../assets/icons/win-max.svg"),
        WIN_RESTORE => include_bytes!("../../assets/icons/win-restore.svg"),
        WIN_CLOSE => include_bytes!("../../assets/icons/win-close.svg"),
        TABLE_GRID => include_bytes!("../../assets/icons/table-grid.svg"),
        ALIGN_START => include_bytes!("../../assets/icons/align-start.svg"),
        ALIGN_CENTER => include_bytes!("../../assets/icons/align-center.svg"),
        ALIGN_END => include_bytes!("../../assets/icons/align-end.svg"),
        TABLE_MORE => include_bytes!("../../assets/icons/table-more.svg"),
        TABLE_DELETE => include_bytes!("../../assets/icons/table-delete.svg"),
        _ => return None,
    })
}

impl AssetSource for ShellAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(embedded(path).map(Cow::Borrowed))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ALIGN_CENTER, ALIGN_END, ALIGN_START, CHEV_DOWN, CHEV_UP, FIND_CLOSE, MOON, OUTLINE,
        OUTLINE_MINI, SEARCH, SLIDERS, ShellAssets, TABLE_DELETE, TABLE_GRID, TABLE_MORE,
        WIN_CLOSE, WIN_MAX, WIN_MIN, WIN_RESTORE, embedded,
    };
    use gpui::AssetSource;

    const KNOWN: &[&str] = &[
        MOON,
        SLIDERS,
        OUTLINE,
        OUTLINE_MINI,
        SEARCH,
        CHEV_UP,
        CHEV_DOWN,
        FIND_CLOSE,
        WIN_MIN,
        WIN_MAX,
        WIN_RESTORE,
        WIN_CLOSE,
        TABLE_GRID,
        ALIGN_START,
        ALIGN_CENTER,
        ALIGN_END,
        TABLE_MORE,
        TABLE_DELETE,
    ];

    #[test]
    fn every_icon_constant_is_embedded() {
        for path in KNOWN {
            let bytes = embedded(path).unwrap_or_else(|| panic!("missing embed for {path}"));
            assert!(
                bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml"),
                "{path} is not an svg"
            );
        }
        assert!(embedded("icons/nope.svg").is_none());
    }

    #[test]
    fn load_returns_the_embedded_bytes() {
        let src = ShellAssets::from_crate_assets();
        for path in KNOWN {
            let bytes = src.load(path).expect("load").unwrap_or_else(|| {
                panic!("{path} is not embedded");
            });
            assert!(bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml"));
        }
        assert!(src.load("icons/nope.svg").expect("load").is_none());
    }
}
