use super::color_picker::{Anchor, Placed};
use super::{Shell, ShellTheme, VIEW_MARGIN};
use crate::ui::theme::RADIUS;
use gpui::prelude::FluentBuilder;
use gpui::{
    App, Bounds, BoxShadow, ClickEvent, Div, Entity, InteractiveElement, IntoElement,
    MouseDownEvent, ParentElement, Pixels, Point, Size, Stateful, StatefulInteractiveElement,
    Styled, Window, div, point, px, rgba, uniform_list,
};
use md_i18n::{Key, t as t18};
use md_theme::{SYSTEM_MONO, SYSTEM_UI};
use std::cell::Cell;
use std::rc::Rc;

const MENU_W: f32 = 288.0;
const ROW_H: f32 = 30.0;
const MAX_ROWS: usize = 8;
pub(super) const MENU_PAD: f32 = 4.0;
const ANCHOR_GAP: f32 = 6.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FontTarget {
    Body,
    Code,
}

impl FontTarget {
    fn default_family(self) -> &'static str {
        match self {
            Self::Body => SYSTEM_UI,
            Self::Code => SYSTEM_MONO,
        }
    }

    pub(super) fn current(self, appearance: &md_theme::Appearance) -> Option<&str> {
        match self {
            Self::Body => appearance.body_font.as_deref(),
            Self::Code => appearance.code_font.as_deref(),
        }
    }
}

pub(super) struct FontMenu {
    anchor: Rc<Cell<Anchor>>,
    names: Rc<Vec<String>>,
    target: FontTarget,
}

impl FontMenu {
    pub(super) fn target(&self) -> FontTarget {
        self.target
    }

    pub(super) fn open(
        at: Point<Pixels>,
        viewport: Size<Pixels>,
        window: &Window,
        target: FontTarget,
    ) -> Self {
        let default = target.default_family();
        let mut names = window.text_system().all_font_names();
        names.retain(|n| n != default);
        names.insert(0, default.to_owned());
        Self {
            anchor: Rc::new(Cell::new(Anchor::at_pointer(at, viewport))),
            names: Rc::new(names),
            target,
        }
    }

    fn measure(&self) -> Rc<dyn Fn(Anchor) -> Option<Point<Pixels>>> {
        let rows = self.names.len().min(MAX_ROWS) as f32;
        let h = rows * ROW_H + MENU_PAD * 2.0 + 2.0;
        Rc::new(move |a: Anchor| {
            if !a.visible() {
                return None;
            }
            let (vw, vh) = (f32::from(a.viewport.width), f32::from(a.viewport.height));
            let below = f32::from(a.row.bottom()) + ANCHOR_GAP;
            let above = f32::from(a.row.top()) - ANCHOR_GAP - h;
            let y = if below + h + VIEW_MARGIN <= vh {
                below
            } else if above >= VIEW_MARGIN {
                above
            } else {
                below
            };
            let x = f32::from(a.row.right()) - MENU_W;
            Some(point(
                px(x.clamp(0.0, (vw - MENU_W - VIEW_MARGIN).max(0.0))),
                px(y.clamp(0.0, (vh - h - VIEW_MARGIN).max(0.0))),
            ))
        })
    }

    pub(super) fn anchor_probe(&self) -> impl IntoElement {
        let cell = Rc::clone(&self.anchor);
        div().absolute().inset_0().child(
            gpui::canvas(
                move |bounds: Bounds<Pixels>, window: &mut Window, _: &mut App| {
                    cell.set(Anchor {
                        row: bounds,
                        clip: window.content_mask().bounds,
                        viewport: window.viewport_size(),
                    });
                },
                |_, _, _, _| (),
            )
            .size_full(),
        )
    }

    pub(super) fn overlay(
        &self,
        t: ShellTheme,
        current: Option<&str>,
        host: Entity<Shell>,
    ) -> Placed {
        Placed {
            child: self.plate(t, current, host).into_any_element(),
            anchor: Rc::clone(&self.anchor),
            measure: self.measure(),
        }
    }

    fn plate(&self, t: ShellTheme, current: Option<&str>, host: Entity<Shell>) -> Stateful<Div> {
        let names = Rc::clone(&self.names);
        let n = self.names.len();
        let close = host.clone();
        let current = current.map(str::to_owned);
        let target = self.target;
        let pick_host = host;
        div()
            .id("font-menu")
            .debug_selector(|| "font-menu".into())
            .w(px(MENU_W))
            .flex()
            .flex_col()
            .p(px(MENU_PAD))
            .bg(t.panel_bg)
            .border_1()
            .border_color(t.border)
            .rounded(px(RADIUS))
            .shadow(vec![BoxShadow {
                color: rgba(0x00000080).into(),
                offset: point(px(0.), px(14.)),
                blur_radius: px(40.),
                spread_radius: px(0.),
            }])
            .occlude()
            .on_mouse_down_out(move |_: &MouseDownEvent, _: &mut Window, cx: &mut App| {
                close.update(cx, |shell, cx| {
                    shell.font_menu = None;
                    cx.notify();
                });
            })
            .child(
                uniform_list("font-menu-rows", n, {
                    move |range, _window, _cx| {
                        range
                            .map(|i| {
                                font_row(
                                    t,
                                    i,
                                    names[i].clone(),
                                    current.as_deref(),
                                    target,
                                    pick_host.clone(),
                                )
                            })
                            .collect()
                    }
                })
                .h(px(self.names.len().min(MAX_ROWS) as f32 * ROW_H))
                .flex_none(),
            )
    }
}

fn font_row(
    t: ShellTheme,
    i: usize,
    name: String,
    current: Option<&str>,
    target: FontTarget,
    host: Entity<Shell>,
) -> Stateful<Div> {
    let default = target.default_family();
    let picked = current == Some(name.as_str());
    let is_default = name == default;
    let label = if is_default {
        t18(Key::SetFontSystemDefault).to_owned()
    } else {
        name.clone()
    };
    let pick = name.clone();
    let pick_name = name.clone();
    div()
        .id(("font-row", i))
        .debug_selector(move || format!("font-row:{pick_name}"))
        .w_full()
        .flex()
        .items_center()
        .px(px(10.))
        .rounded(px(5.))
        .when(picked, |d| d.bg(t.hover))
        .hover(move |s| s.bg(t.hover))
        .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            host.update(cx, |shell, cx| {
                let picked = if is_default { None } else { Some(pick.clone()) };
                match target {
                    FontTarget::Body => shell.settings.appearance.body_font = picked,
                    FontTarget::Code => shell.settings.appearance.code_font = picked,
                }
                shell.font_menu = None;
                shell.appearance_changed(cx);
                cx.notify();
            });
        })
        .child(
            div()
                .flex_1()
                .font_family(name)
                .text_size(px(12.5))
                .text_color(if picked { t.accent } else { t.text })
                .child(label),
        )
        .when(picked, |d| {
            d.child(div().text_size(px(11.)).text_color(t.accent).child("✓"))
        })
}

impl Shell {
    pub(super) fn font_menu_overlay(
        &self,
        t: ShellTheme,
        this: &Entity<Self>,
    ) -> Option<impl IntoElement> {
        let menu = self.font_menu.as_ref()?;
        let current = menu.target.current(&self.settings.appearance);
        Some(menu.overlay(t, current, this.clone()))
    }
}
