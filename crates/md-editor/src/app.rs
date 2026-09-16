use crate::ShellAssets;
use crate::shell::{CloseFind, Shell};
use gpui::{
    App, AppContext, Bounds, KeyBinding, Pixels, SharedString, TitlebarOptions, WindowBounds,
    WindowOptions, px,
};
use md_core::doc::Doc;
use md_theme::DocumentTheme;

use crate::Error;

pub struct RunConfig {
    pub title: SharedString,
    pub notice: Option<Error>,
}

pub fn run(doc: Doc, config: RunConfig) {
    let title = config.title;
    let notice = config.notice;
    md_render::cold_trace::mark_gpui();
    gpui_platform::application()
        .with_http_client(md_content::images::http_client())
        .with_assets(ShellAssets::from_crate_assets())
        .run(move |cx: &mut App| {
            md_render::cold_trace::log(&format!(
                "cold gpui run-callback after {:.1}ms",
                md_render::cold_trace::gpui_ms()
            ));
            cx.bind_keys([KeyBinding::new("escape", CloseFind, None)]);

            let theme = DocumentTheme::one_dark();
            let centered = || {
                Bounds::centered(
                    None,
                    gpui::size(
                        px(theme.chrome.window_width),
                        px(theme.chrome.window_height),
                    ),
                    cx,
                )
            };
            let displays: Vec<Bounds<Pixels>> = cx
                .displays()
                .iter()
                .map(|display| display.bounds())
                .collect();
            let saved = crate::store::window::WindowStore::discover()
                .and_then(|store| store.load())
                .filter(|geometry| geometry.center_is_visible(&displays));
            let window_bounds = match saved {
                Some(geometry) if geometry.maximized => {
                    WindowBounds::Maximized(geometry.restore_bounds())
                }
                Some(geometry) => WindowBounds::Windowed(geometry.restore_bounds()),
                None => WindowBounds::Windowed(centered()),
            };
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(window_bounds),
                    window_min_size: Some(gpui::size(
                        px(theme.chrome.window_min_width),
                        px(theme.chrome.window_min_height),
                    )),
                    titlebar: Some(TitlebarOptions {
                        title: Some(title.clone()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    app_id: Some(crate::app_icon::APP_ID.into()),
                    ..Default::default()
                },
                {
                    let doc = doc;
                    move |window, cx| {
                        let shell = cx.new(|cx| Shell::new(doc, cx));
                        shell.update(cx, |s, cx| s.boot(notice, window, cx));
                        let handle = shell.clone();
                        window.on_window_should_close(cx, move |window, cx| {
                            handle.update(cx, |s, cx| s.on_window_should_close(window, cx))
                        });
                        let flush_handle = shell.clone();
                        cx.on_window_closed(move |cx, _window_id| {
                            flush_handle.update(cx, |s, _| s.flush_window_geometry());
                        })
                        .detach();
                        shell
                    }
                },
            )
            .expect("open_window");
            crate::app_icon::apply();
            md_render::cold_trace::log(&format!(
                "cold open_window after {:.1}ms",
                md_render::cold_trace::gpui_ms()
            ));

            cx.on_window_closed(|cx, _window_id| cx.quit()).detach();
            cx.activate(true);
        });
}
