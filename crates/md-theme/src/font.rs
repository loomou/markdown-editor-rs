#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct FontWeight(pub f32);

impl FontWeight {
    pub const THIN: FontWeight = FontWeight(100.0);
    pub const EXTRA_LIGHT: FontWeight = FontWeight(200.0);
    pub const LIGHT: FontWeight = FontWeight(300.0);
    pub const NORMAL: FontWeight = FontWeight(400.0);
    pub const MEDIUM: FontWeight = FontWeight(500.0);
    pub const SEMIBOLD: FontWeight = FontWeight(600.0);
    pub const BOLD: FontWeight = FontWeight(700.0);
    pub const EXTRA_BOLD: FontWeight = FontWeight(800.0);
    pub const BLACK: FontWeight = FontWeight(900.0);
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::NORMAL
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}

pub const SYSTEM_UI: &str = ".SystemUIFont";

#[cfg(any(windows, target_os = "macos"))]
pub const SYSTEM_SERIF: &str = "Georgia";
#[cfg(all(not(windows), not(target_os = "macos")))]
pub const SYSTEM_SERIF: &str = "DejaVu Serif";

#[cfg(windows)]
pub const SYSTEM_MONO: &str = "Consolas";
#[cfg(target_os = "macos")]
pub const SYSTEM_MONO: &str = "Menlo";
#[cfg(all(not(windows), not(target_os = "macos")))]
pub const SYSTEM_MONO: &str = "DejaVu Sans Mono";
