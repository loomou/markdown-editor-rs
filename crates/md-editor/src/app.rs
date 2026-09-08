use crate::ShellAssets;
use crate::shell::{CloseFind, Shell};
use gpui::{
    App, AppContext, Application, Bounds, KeyBinding, SharedString, TitlebarOptions, WindowBounds,
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
    Application::new()
        .with_http_client(md_content::images::http_client())
        .with_assets(ShellAssets::from_crate_assets())
        .run(move |cx: &mut App| {
            md_render::cold_trace::log(&format!(
                "cold gpui run-callback after {:.1}ms",
                md_render::cold_trace::gpui_ms()
            ));
            cx.bind_keys([KeyBinding::new("escape", CloseFind, None)]);

            let theme = DocumentTheme::one_dark();
            let bounds = Bounds::centered(
                None,
                gpui::size(
                    px(theme.chrome.window_width),
                    px(theme.chrome.window_height),
                ),
                cx,
            );
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
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

            cx.on_window_closed(|cx| cx.quit()).detach();
            cx.activate(true);
        });
}
