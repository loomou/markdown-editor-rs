use super::Shell;
use super::settings_state::{font_size_at, font_size_fraction};
use crate::store::settings::LanguageChoice;
use crate::ui::theme::ShellTheme;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, BoxShadow, ClickEvent, Context, Div, Entity, FontWeight, InteractiveElement,
    IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels,
    Stateful, StatefulInteractiveElement, Styled, Window, canvas, div, point, px, rgba,
};
use md_i18n::{Key, t as t18};
use md_theme::{BodyFamily, Density, ThemeVariant};
use std::rc::Rc;

const SETTINGS_NAV: [Key; 5] = [
    Key::NavAppearance,
    Key::NavEditing,
    Key::NavFiles,
    Key::NavShortcuts,
    Key::NavAbout,
];

impl Shell {
    pub(super) fn settings_page(&self, t: ShellTheme, this: Entity<Self>) -> Div {
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .overflow_hidden()
            .bg(t.editor_bg)
            .child(self.settings_nav(t, this.clone()))
            .child(self.settings_main(t, this))
    }

    fn settings_nav_item(
        &self,
        t: ShellTheme,
        this: Entity<Self>,
        index: usize,
        label: Key,
    ) -> Stateful<Div> {
        let on = self.settings_nav == index;
        div()
            .id(label.debug_name())
            .h(px(28.))
            .flex_none()
            .flex()
            .items_center()
            .px(px(10.))
            .rounded(px(5.))
            .text_size(px(12.5))
            .text_color(if on { t.text } else { t.text_muted })
            .when(on, |d| d.bg(t.selected_bg))
            .when(!on, |d| d.hover(move |s| s.bg(t.hover)))
            .on_click(move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                this.update(cx, |shell, cx| {
                    shell.settings_nav = index;
                    shell.cancel_recording(window, cx);
                    cx.notify();
                });
            })
            .child(t18(label))
    }

    fn settings_nav(&self, t: ShellTheme, this: Entity<Self>) -> Div {
        div()
            .w(px(172.))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(2.))
            .py(px(10.))
            .px(px(8.))
            .bg(t.panel_bg)
            .border_r_1()
            .border_color(t.border)
            .children(
                SETTINGS_NAV
                    .iter()
                    .copied()
                    .enumerate()
                    .map(|(i, label)| self.settings_nav_item(t, this.clone(), i, label)),
            )
    }

    fn settings_main(&self, t: ShellTheme, this: Entity<Self>) -> Stateful<Div> {
        let title = SETTINGS_NAV[self.settings_nav];
        let theme = self.settings.appearance.document_theme();
        div()
            .id("settings-main")
            .flex_1()
            .overflow_y_scroll()
            .track_scroll(&self.settings_scroll)
            .px(px(34.))
            .py(px(24.))
            .flex()
            .flex_col()
            .items_center()
            .child(
                div()
                    .w(px(600.))
                    .max_w_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight(600.0))
                            .text_color(t.text)
                            .pb(px(6.))
                            .child(t18(title)),
                    )
                    .children(match self.settings_nav {
                        0 => vec![
                            self.set_row(
                                t,
                                Key::SetLanguage,
                                t18(Key::SetLanguageHint),
                                self.seg_control(
                                    t,
                                    &LanguageChoice::ALL.map(LanguageChoice::label),
                                    LanguageChoice::ALL
                                        .iter()
                                        .position(|c| *c == self.settings.language)
                                        .unwrap_or(0),
                                    |shell, i, _cx| {
                                        shell.settings.language = LanguageChoice::ALL
                                            [i.min(LanguageChoice::ALL.len() - 1)];
                                        shell.save_settings();
                                    },
                                    &this,
                                )
                                .into_any_element(),
                            ),
                            self.set_row(
                                t,
                                Key::Theme,
                                t18(Key::SetThemeHint),
                                self.seg_control(
                                    t,
                                    &[Key::SetThemeDark, Key::SetThemeLight],
                                    usize::from(!self.is_dark()),
                                    |shell, i, cx| {
                                        shell.settings.appearance.variant = if i == 0 {
                                            ThemeVariant::OneDark
                                        } else {
                                            ThemeVariant::OneLight
                                        };
                                        shell.appearance_changed(cx);
                                    },
                                    &this,
                                )
                                .into_any_element(),
                            ),
                            self.set_row(
                                t,
                                Key::SetBodyFont,
                                t18(Key::SetBodyFontHint),
                                self.seg_control(
                                    t,
                                    &[Key::SetFontSerif, Key::SetFontSans],
                                    usize::from(
                                        self.settings.appearance.body_family == BodyFamily::Sans,
                                    ),
                                    |shell, i, cx| {
                                        shell.settings.appearance.body_family = if i == 0 {
                                            BodyFamily::Serif
                                        } else {
                                            BodyFamily::Sans
                                        };
                                        shell.appearance_changed(cx);
                                    },
                                    &this,
                                )
                                .into_any_element(),
                            ),
                            self.set_row(
                                t,
                                Key::SetBodySize,
                                &md_i18n::fmt::body_size_hint(
                                    self.settings.appearance.body_size_px,
                                ),
                                self.font_size_slider(t, this.clone()).into_any_element(),
                            ),
                            self.set_row(
                                t,
                                Key::SetDensity,
                                t18(Key::SetDensityHint),
                                self.seg_control(
                                    t,
                                    &[
                                        Key::SetDensityCompact,
                                        Key::SetDensityNormal,
                                        Key::SetDensityRelaxed,
                                    ],
                                    Density::ALL
                                        .iter()
                                        .position(|d| *d == self.settings.appearance.density)
                                        .unwrap_or(1),
                                    |shell, i, cx| {
                                        shell.settings.appearance.density =
                                            Density::ALL[i.min(Density::ALL.len() - 1)];
                                        shell.appearance_changed(cx);
                                    },
                                    &this,
                                )
                                .into_any_element(),
                            ),
                            self.set_row(
                                t,
                                Key::SetAutosave,
                                t18(Key::SetAutosaveHint),
                                self.toggle_control(
                                    t,
                                    "tgl-autosave",
                                    self.settings.autosave,
                                    |shell, v, cx| {
                                        shell.settings.autosave = v;
                                        shell.editor.update(cx, |editor, _| {
                                            editor.set_autosave(v);
                                        });
                                        shell.save_settings();
                                    },
                                    this.clone(),
                                )
                                .into_any_element(),
                            ),
                            self.set_row(
                                t,
                                Key::SetRemoteImages,
                                t18(Key::SetRemoteImagesHint),
                                self.toggle_control(
                                    t,
                                    "tgl-remote-images",
                                    self.settings.remote_images,
                                    |shell, value, cx| {
                                        shell.settings.remote_images = value;
                                        shell.editor.update(cx, |editor, cx| {
                                            editor.set_remote_images(value, cx);
                                        });
                                        shell.save_settings();
                                    },
                                    this.clone(),
                                )
                                .into_any_element(),
                            ),
                            self.set_row(
                                t,
                                Key::SetStartup,
                                t18(Key::SetStartupHint),
                                self.seg_control(
                                    t,
                                    &[Key::SetStartupNew, Key::SetStartupLast],
                                    usize::from(self.open_last_on_start),
                                    |shell, i, _cx| shell.open_last_on_start = i == 1,
                                    &this,
                                )
                                .into_any_element(),
                            ),
                        ]
                        .into_iter()
                        .map(gpui::IntoElement::into_any_element)
                        .chain(self.color_sections(t, &this, &theme))
                        .collect(),
                        3 => self.shortcut_sections(t, &this),
                        4 => vec![self.set_about(t).into_any_element()],
                        _ => vec![
                            self.set_note(t, t18(Key::SetSectionEmpty))
                                .into_any_element(),
                        ],
                    }),
            )
    }

    pub(super) fn set_row(
        &self,
        t: ShellTheme,
        title: Key,
        desc: &str,
        control: AnyElement,
    ) -> Div {
        div()
            .debug_selector(move || format!("row:{}", title.debug_name()))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(px(13.))
            .border_b_1()
            .border_color(t.border_variant)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight(500.0))
                            .text_color(t.text)
                            .child(t18(title)),
                    )
                    .child(
                        div()
                            .mt(px(2.))
                            .text_size(px(11.5))
                            .text_color(t.text_disabled)
                            .child(desc.to_string()),
                    ),
            )
            .child(control)
    }

    fn seg_control(
        &self,
        t: ShellTheme,
        options: &[Key],
        selected: usize,
        on_pick: fn(&mut Shell, usize, &mut App),
        this: &Entity<Self>,
    ) -> Div {
        let this = this.clone();
        div()
            .flex_none()
            .flex()
            .overflow_hidden()
            .rounded(px(6.))
            .border_1()
            .border_color(t.border_variant)
            .children(options.iter().copied().enumerate().map(move |(i, opt)| {
                let on = i == selected;
                let cell = this.clone();
                div()
                    .id(opt.debug_name())
                    .debug_selector(move || format!("seg:{}", opt.debug_name()))
                    .px(px(12.))
                    .py(px(4.))
                    .text_size(px(12.))
                    .text_color(if on { t.text } else { t.text_muted })
                    .when(on, |d| d.bg(t.hover))
                    .when(!on, |d| d.hover(move |s| s.bg(t.hover)))
                    .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                        cell.update(cx, |shell, cx| {
                            on_pick(shell, i, cx);
                            cx.notify();
                        });
                    })
                    .child(t18(opt))
            }))
    }

    fn toggle_control(
        &self,
        t: ShellTheme,
        id: &'static str,
        on: bool,
        on_set: fn(&mut Shell, bool, &mut Context<'_, Shell>),
        this: Entity<Self>,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .debug_selector(move || format!("tgl:{id}"))
            .w(px(32.))
            .h(px(18.))
            .flex_none()
            .rounded_full()
            .p(px(2.))
            .bg(if on { t.accent } else { t.active })
            .flex()
            .when(on, |d| d.justify_end())
            .when(!on, |d| d.justify_start())
            .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                this.update(cx, |shell, cx| {
                    on_set(shell, !on, cx);
                    cx.notify();
                });
            })
            .child(
                div()
                    .size(px(14.))
                    .rounded_full()
                    .bg(rgba(0xffffffff))
                    .shadow(vec![BoxShadow {
                        color: rgba(0x00000059).into(),
                        offset: point(px(0.), px(1.)),
                        blur_radius: px(3.),
                        spread_radius: px(0.),
                    }]),
            )
    }

    fn font_size_slider(&self, t: ShellTheme, this: Entity<Self>) -> Div {
        const TRACK_W: f32 = 150.0;
        let frac = font_size_fraction(self.settings.appearance.body_size_px);
        let painted_track = Rc::clone(&self.font_track);
        div()
            .relative()
            .w(px(TRACK_W))
            .h(px(12.))
            .flex_none()
            .child(
                div()
                    .absolute()
                    .top(px(4.5))
                    .left_0()
                    .w_full()
                    .h(px(3.))
                    .rounded(px(2.))
                    .bg(t.active),
            )
            .child(
                div()
                    .absolute()
                    .top(px(4.5))
                    .left_0()
                    .h(px(3.))
                    .w(px(TRACK_W * frac))
                    .rounded(px(2.))
                    .bg(t.accent),
            )
            .child(
                div()
                    .absolute()
                    .size(px(12.))
                    .rounded_full()
                    .bg(t.text)
                    .top(px(0.))
                    .left(px(TRACK_W * frac - 6.)),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |track: gpui::Bounds<Pixels>, _, window, _| {
                        painted_track.set(Some(track));
                        let set = {
                            let this = this.clone();
                            move |x: Pixels, cx: &mut App| {
                                this.update(cx, |shell, cx| {
                                    let size = font_size_at(x, track);
                                    if shell.settings.appearance.body_size_px == size {
                                        return;
                                    }
                                    shell.settings.appearance =
                                        shell.settings.appearance.with_body_size_px(size);
                                    shell.sync_editor_theme(cx);
                                    cx.notify();
                                });
                            }
                        };
                        window.on_mouse_event({
                            let this = this.clone();
                            let set = set.clone();
                            move |ev: &MouseDownEvent, _, _, cx| {
                                if ev.button != MouseButton::Left || !track.contains(&ev.position) {
                                    return;
                                }
                                this.update(cx, |shell, _| shell.font_slider_drag = true);
                                set(ev.position.x, cx);
                            }
                        });
                        window.on_mouse_event({
                            let this = this.clone();
                            let set = set.clone();
                            move |ev: &MouseMoveEvent, _, _, cx| {
                                if !ev.dragging() || !this.read(cx).font_slider_drag {
                                    return;
                                }
                                set(ev.position.x, cx);
                            }
                        });
                        window.on_mouse_event({
                            move |_: &MouseUpEvent, _, _, cx| {
                                this.update(cx, |shell, _| {
                                    if shell.font_slider_drag {
                                        shell.font_slider_drag = false;
                                        shell.save_settings();
                                    }
                                });
                            }
                        });
                    },
                )
                .size_full(),
            )
    }

    pub(super) fn settings_file_button(&self, t: ShellTheme, this: &Entity<Self>) -> Div {
        let homeless = self.settings_store.is_none();
        let this = this.clone();
        div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(8.))
            .when(homeless, |d| {
                d.child(
                    div()
                        .debug_selector(|| "note:no-settings-home".into())
                        .flex_shrink()
                        .text_size(px(11.5))
                        .text_color(t.text_disabled)
                        .child(t18(Key::SetColorsNoHome)),
                )
            })
            .child(
                div()
                    .id("settings-open-file")
                    .debug_selector(|| "btn:open-settings".into())
                    .flex_none()
                    .px(px(10.))
                    .py(px(5.))
                    .rounded(px(5.))
                    .border_1()
                    .border_color(t.border)
                    .text_size(px(11.5))
                    .text_color(t.text)
                    .hover(move |s| s.bg(t.hover))
                    .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                        let path = this.update(cx, |shell, _| shell.settings_file_for_open());
                        if let Some(path) = path {
                            cx.open_with_system(&path);
                        }
                    })
                    .child(t18(Key::SetColorsEdit)),
            )
    }

    pub(super) fn set_note(&self, t: ShellTheme, text: &'static str) -> Div {
        div()
            .pt(px(8.))
            .text_size(px(12.5))
            .text_color(t.text_disabled)
            .child(text)
    }

    fn set_about(&self, t: ShellTheme) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .pt(px(8.))
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight(600.0))
                    .text_color(t.text)
                    .child("Markdown Editor RS"),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(t.text_muted)
                    .child(md_i18n::fmt::about_tagline(env!("CARGO_PKG_VERSION"))),
            )
    }
}
