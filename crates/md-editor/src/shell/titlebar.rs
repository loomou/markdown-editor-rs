use super::Shell;
use super::caption::{bind_caption_drag, toggle_zoom};
use super::menu::MenuId;
use crate::ui::icons;
use crate::ui::theme::{MONO_FONT, ShellTheme, TITLE_BAR_H, TITLE_LEAD_PAD, WINCTL_W};
use crate::view::PendingNav;
use gpui::prelude::FluentBuilder;
use gpui::{
    App, ClickEvent, Div, Entity, FontWeight, InteractiveElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, Window, WindowControlArea, div, px, rgba, svg,
};
use md_i18n::Key;

impl Shell {
    pub(super) fn title_bar(&self, t: ShellTheme, this: Entity<Self>, window: &Window) -> Div {
        div()
            .h(px(TITLE_BAR_H))
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .bg(t.bar_bg)
            .border_b_1()
            .border_color(t.border)
            .child(
                bind_caption_drag(
                    div()
                        .id("drag-lead")
                        .pl(px(TITLE_LEAD_PAD))
                        .h_full()
                        .flex()
                        .items_center(),
                    this.clone(),
                )
                .child(self.logo(t))
                .child(self.brand(t)),
            )
            .child(
                div().flex().flex_row().text_size(px(13.)).children(
                    [
                        (MenuId::File, Key::MenuBarFile),
                        (MenuId::Edit, Key::MenuBarEdit),
                        (MenuId::View, Key::MenuBarView),
                        (MenuId::Go, Key::MenuBarGo),
                        (MenuId::Help, Key::MenuBarHelp),
                    ]
                    .map(|(id, label)| self.menu_label(t, this.clone(), id, label)),
                ),
            )
            .child(bind_caption_drag(
                div()
                    .id("drag-fill")
                    .debug_selector(|| "drag-fill".into())
                    .flex_1()
                    .h_full(),
                this.clone(),
            ))
            .child(self.icon_button(t, "btn-theme", icons::MOON).on_click({
                let this = this.clone();
                move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                    this.update(cx, |shell, cx| {
                        shell.toggle_variant(cx);
                        cx.notify();
                    });
                }
            }))
            .child(
                self.icon_button(t, "btn-settings", icons::SLIDERS)
                    .on_click({
                        let this = this.clone();
                        move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                            this.update(cx, |shell, cx| {
                                shell.show_settings = !shell.show_settings;

                                shell.color_picker = None;

                                shell.cancel_recording(window, cx);
                                cx.notify();
                            });
                        }
                    }),
            )
            .when(cfg!(not(target_os = "macos")), |d| {
                d.child(self.winctl(t, this, window.is_maximized()))
            })
    }

    fn logo(&self, t: ShellTheme) -> Div {
        div()
            .size(px(16.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .bg(t.accent)
            .text_color(rgba(0x14181fff))
            .text_size(px(10.))
            .font_family(MONO_FONT)
            .font_weight(FontWeight(700.0))
            .child("M")
    }

    fn brand(&self, t: ShellTheme) -> Div {
        div()
            .px(px(8.))
            .flex()
            .items_center()
            .text_size(px(13.))
            .font_weight(FontWeight(600.0))
            .text_color(t.text)
            .child("md-test")
    }

    fn icon_button(&self, t: ShellTheme, id: &'static str, path: &'static str) -> Stateful<Div> {
        div()
            .id(id)
            .debug_selector(move || format!("btn:{id}"))
            .mr(px(4.))
            .w(px(28.))
            .h(px(26.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .hover(move |s| s.bg(t.hover))
            .child(
                svg()
                    .size(px(14.))
                    .path(path)
                    .text_color(t.text_muted)
                    .hover(move |s| s.text_color(t.text)),
            )
    }

    fn winctl(&self, t: ShellTheme, this: Entity<Self>, maximized: bool) -> Div {
        let (max_id, max_icon) = if maximized {
            ("win-restore", icons::WIN_RESTORE)
        } else {
            ("win-max", icons::WIN_MAX)
        };
        div()
            .flex_none()
            .h_full()
            .flex()
            .child(self.winctl_button(
                t,
                "win-min",
                icons::WIN_MIN,
                WindowControlArea::Min,
                false,
                this.clone(),
            ))
            .child(self.winctl_button(
                t,
                max_id,
                max_icon,
                WindowControlArea::Max,
                false,
                this.clone(),
            ))
            .child(self.winctl_button(
                t,
                "win-close",
                icons::WIN_CLOSE,
                WindowControlArea::Close,
                true,
                this,
            ))
    }

    fn winctl_button(
        &self,
        t: ShellTheme,
        id: &'static str,
        path: &'static str,
        area: WindowControlArea,
        danger: bool,
        this: Entity<Self>,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .w(px(WINCTL_W))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .window_control_area(area)
            .on_click(
                move |_: &ClickEvent, window: &mut Window, cx: &mut App| match area {
                    WindowControlArea::Min => window.minimize_window(),
                    WindowControlArea::Max => toggle_zoom(window),
                    WindowControlArea::Close => {
                        this.update(cx, |shell, cx| {
                            shell.editor.update(cx, |editor, cx| {
                                editor.request_nav(PendingNav::Close, window, cx);
                            });
                        });
                    }
                    _ => {}
                },
            )
            .when(!danger, |d| d.hover(move |s| s.bg(t.hover)))
            .when(danger, |d| d.hover(move |s| s.bg(t.close_hover)))
            .child(
                svg()
                    .size(px(10.))
                    .path(path)
                    .text_color(t.text_muted)
                    .when(!danger, |s| s.hover(move |st| st.text_color(t.text)))
                    .when(danger, |s| {
                        s.hover(move |st| st.text_color(rgba(0xffffffff)))
                    }),
            )
    }
}
