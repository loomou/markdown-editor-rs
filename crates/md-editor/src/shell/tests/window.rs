use super::support::{stop_blink, test_doc};
use crate::shell::Shell;
use crate::store::window::{WindowGeometry, WindowStore};
use gpui::{Bounds, TestAppContext, px};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn unique_path(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let mut p = std::env::temp_dir();
    p.push(format!(
        "markdown-editor-rs-shell-window-{tag}-{}-{n}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&p);
    p
}

#[gpui::test]
fn window_geometry_round_trips_through_the_shell(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.run_until_parked();

    let path = unique_path("geometry");
    let sample = WindowGeometry::from_window(
        Bounds {
            origin: gpui::point(px(10.0), px(20.0)),
            size: gpui::size(px(800.0), px(600.0)),
        },
        false,
    );
    cx.update(|_, app| {
        shell.update(app, |s, _| {
            s.window_store = Some(WindowStore::at(path.clone()));
            s.window_geometry = sample;
            s.flush_window_geometry();
        });
    });
    assert_eq!(WindowStore::at(path.clone()).load(), Some(sample));

    let live = cx.update(|window, _| window.bounds());
    cx.update(|window, app| {
        shell.update(app, |s, cx| s.note_window_bounds(window, cx));
    });
    shell.read_with(cx, |s, _| {
        assert_eq!(
            s.window_geometry,
            WindowGeometry::from_window(live, false),
            "note_window_bounds must track the live window"
        );
    });
    let _ = std::fs::remove_file(&path);
}
