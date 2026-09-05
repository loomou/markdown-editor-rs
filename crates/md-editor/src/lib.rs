pub mod app;
pub mod app_icon;

pub mod shell;

pub mod view;

pub use md_editor_ui::{error, keymap, platform, store, ui};

#[deprecated(note = "the floating widgets were folded into view; use md_editor::view::find_bar")]
pub mod widgets {
    pub use crate::view::find_bar;
}

pub use error::Error;

pub use shell::{CloseFind, Shell};
pub use ui::icons::ShellAssets;
