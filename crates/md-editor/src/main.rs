#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use md_core::doc::Doc;
use md_core::document::{dump_structure, editor_options, load_markdown};
use md_editor::APP_NAME;
use md_editor::Error;
use md_editor::app::{RunConfig, run};
use md_render::cold_trace;
use std::path::PathBuf;

fn main() {
    md_editor::platform::paths::migrate_legacy_state_dir();
    let _log = md_editor::platform::log::init();
    md_editor::store::settings::init_language();
    let t_boot = std::time::Instant::now();
    let startup_path = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .find(|path| md_editor::platform::open_markdown::is_markdown_path(path))
        .or_else(|| std::env::var_os("MARKDOWN_EDITOR_RS_PATH").map(PathBuf::from));
    let (doc, title, notice) = match startup_path {
        Some(path) => match std::fs::read_to_string(&path) {
            Ok(md) => {
                let t_parse = std::time::Instant::now();
                let loaded = load_markdown(&md, editor_options());
                let parse_ms = t_parse.elapsed();
                let skip_dump = cold_trace::enabled();
                let arena_nodes = if skip_dump {
                    0
                } else {
                    dump_structure(&loaded).lines().count()
                };
                let blocks = loaded.preorder().len();
                let leaves = loaded.text_leaves().len();
                tracing::info!(
                    path = %path.display(),
                    parse_ms = parse_ms.as_secs_f64() * 1000.0,
                    boot_ms = t_boot.elapsed().as_secs_f64() * 1000.0,
                    blocks,
                    leaves,
                    live = loaded.arena.live_count(),
                    tombstones = loaded.arena.tombstone_count(),
                    source = loaded.source.len(),
                    intern = loaded.intern_len(),
                    links = loaded.links.len(),
                    markdown = loaded.to_markdown().len(),
                    dump = arena_nodes,
                    replace = loaded.is_full_replace(),
                    "parsed markdown"
                );
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("markdown")
                    .to_string();
                (Doc::with_path(loaded, Some(path)), name, None)
            }
            Err(source) => {
                let err = Error::Read { path, source };
                tracing::error!(error = %err);
                (
                    Doc::new(load_markdown("", editor_options())),
                    APP_NAME.to_string(),
                    Some(err),
                )
            }
        },
        None => match md_editor::store::recovery::startup() {
            Some((doc, title)) => (doc, title, None),
            None => (
                Doc::new(load_markdown("", editor_options())),
                APP_NAME.to_string(),
                None,
            ),
        },
    };

    run(
        doc,
        RunConfig {
            title: title.into(),
            notice,
        },
    );
}
