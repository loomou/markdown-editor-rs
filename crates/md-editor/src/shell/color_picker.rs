use super::{Shell, VIEW_MARGIN};
use crate::ui::text_input::{InputStyle, TextInput, TextInputElement};
use crate::ui::theme::{MONO_FONT, RADIUS, ShellTheme};
use gpui::{
    AnyElement, App, Background, Bounds, BoxShadow, ClickEvent, CursorStyle, DispatchPhase, Div,
    Element, Entity, FocusHandle, Font, FontFeatures, FontStyle, FontWeight, GlobalElementId,
    InspectorElementId, InteractiveElement, IntoElement, KeyDownEvent, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, Position, Size,
    Stateful, StatefulInteractiveElement, Style, Styled, Window, canvas, div, fill,
    linear_color_stop, linear_gradient, point, px, rgba,
};
use md_content::gpui_theme::ThemeColorExt;
use md_theme::{ColorSlot, ThemeColor};
use std::cell::Cell;
use std::rc::Rc;

pub(super) const PICKER_W: f32 = 232.0;
const PAD: f32 = 10.0;
const GAP: f32 = 8.0;
const FIELD_H: f32 = 148.0;
const HUE_H: f32 = 14.0;
const HEAD_H: f32 = 18.0;

const FOOT_H: f32 = 24.0;

const HEX_W: f32 = 46.0;

pub(super) const PICKER_H: f32 = PAD * 2.0 + 2.0 + HEAD_H + FIELD_H + HUE_H + FOOT_H + GAP * 3.0;

const BAND: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Drag {
    Field,

    Hue,
}

fn sl_at(pos: Point<Pixels>, field: Bounds<Pixels>) -> (f32, f32) {
    let w = f32::from(field.size.width).max(1.0);
    let h = f32::from(field.size.height).max(1.0);
    let s = ((f32::from(pos.x) - f32::from(field.origin.x)) / w).clamp(0.0, 1.0);
    let l = ((f32::from(pos.y) - f32::from(field.origin.y)) / h).clamp(0.0, 1.0);
    (s, 1.0 - l)
}

fn hue_at(x: Pixels, strip: Bounds<Pixels>) -> f32 {
    let w = f32::from(strip.size.width).max(1.0);
    ((f32::from(x) - f32::from(strip.origin.x)) / w).clamp(0.0, 1.0)
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
    Bounds {
        origin: point(px(x), px(y)),
        size: gpui::size(px(w), px(h)),
    }
}

const ANCHOR_GAP: f32 = 6.0;

fn hex_digits(color: ThemeColor) -> String {
    color.to_css_hex().trim_start_matches('#').to_string()
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Anchor {
    row: Bounds<Pixels>,

    clip: Bounds<Pixels>,
    viewport: Size<Pixels>,
}

impl Anchor {
    fn at_pointer(at: Point<Pixels>, viewport: Size<Pixels>) -> Self {
        let row = Bounds {
            origin: at,
            size: gpui::size(px(1.), px(1.)),
        };
        Self {
            row,
            clip: row,
            viewport,
        }
    }

    fn visible(&self) -> bool {
        self.row.intersects(&self.clip)
    }
}

fn place(a: Anchor) -> Option<Point<Pixels>> {
    if !a.visible() {
        return None;
    }
    let (vw, vh) = (f32::from(a.viewport.width), f32::from(a.viewport.height));
    let below = f32::from(a.row.bottom()) + ANCHOR_GAP;
    let above = f32::from(a.row.top()) - ANCHOR_GAP - PICKER_H;
    let y = if below + PICKER_H + VIEW_MARGIN <= vh {
        below
    } else if above >= VIEW_MARGIN {
        above
    } else {
        below
    };
    let x = f32::from(a.row.right()) - PICKER_W;

    Some(point(
        px(x.clamp(0.0, (vw - PICKER_W - VIEW_MARGIN).max(0.0))),
        px(y.clamp(0.0, (vh - PICKER_H - VIEW_MARGIN).max(0.0))),
    ))
}

pub(super) struct Placed {
    child: AnyElement,
    anchor: Rc<Cell<Anchor>>,
}

impl Element for Placed {
    type RequestLayoutState = ();

    type PrepaintState = bool;

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            position: Position::Absolute,
            ..Default::default()
        };
        let child = self.child.request_layout(window, cx);
        (window.request_layout(style, [child], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let Some(origin) = place(self.anchor.get()) else {
            return false;
        };

        let delta = origin - bounds.origin;
        let delta = point(delta.x.round(), delta.y.round());
        window.with_element_offset(delta, |window| self.child.prepaint(window, cx));
        true
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        showing: &mut bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        if *showing {
            self.child.paint(window, cx);
        }
    }
}

impl IntoElement for Placed {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

fn paint_field(window: &mut Window, bounds: Bounds<Pixels>, hue: f32) {
    let (x0, y0) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let bands = (w / BAND).ceil().max(1.0);
    let bw = w / bands;
    let half = h / 2.0;
    let white = gpui::hsla(0.0, 0.0, 1.0, 1.0);
    let black = gpui::hsla(0.0, 0.0, 0.0, 1.0);
    for i in 0..bands as usize {
        let s = (i as f32 + 0.5) / bands;
        let mid = ThemeColor::new(hue, s, 0.5, 1.0).hsla();
        let x = x0 + i as f32 * bw;

        window.paint_quad(fill(
            rect(x, y0, bw + 1.0, half),
            linear_gradient(
                180.0,
                linear_color_stop(white, 0.0),
                linear_color_stop(mid, 1.0),
            ),
        ));
        window.paint_quad(fill(
            rect(x, y0 + half, bw + 1.0, half),
            linear_gradient(
                180.0,
                linear_color_stop(mid, 0.0),
                linear_color_stop(black, 1.0),
            ),
        ));
    }
}

fn paint_hue(window: &mut Window, bounds: Bounds<Pixels>) {
    let (x0, y0) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let seg = w / 6.0;
    for i in 0..6 {
        let a = ThemeColor::new(i as f32 / 6.0, 1.0, 0.5, 1.0).hsla();
        let b = ThemeColor::new((i + 1) as f32 / 6.0, 1.0, 0.5, 1.0).hsla();
        window.paint_quad(fill(
            rect(x0 + i as f32 * seg, y0, seg + 1.0, h),
            linear_gradient(90.0, linear_color_stop(a, 0.0), linear_color_stop(b, 1.0)),
        ));
    }
}

fn paint_marker(window: &mut Window, cx: f32, cy: f32, color: Background) {
    for (d, bg) in [
        (12.0, gpui::hsla(0.0, 0.0, 0.0, 0.55).into()),
        (10.0, gpui::hsla(0.0, 0.0, 1.0, 1.0).into()),
        (6.0, color),
    ] {
        window
            .paint_quad(fill(rect(cx - d / 2.0, cy - d / 2.0, d, d), bg).corner_radii(px(d / 2.0)));
    }
}

fn drag_begin(shell: &mut Shell, which: Drag) {
    if let Some(p) = shell.color_picker.as_mut() {
        p.drag = Some(which);
    }
}

fn is_dragging(shell: &Shell, which: Drag) -> bool {
    shell
        .color_picker
        .as_ref()
        .is_some_and(|p| p.drag == Some(which))
}

fn drag_end(shell: &mut Shell, which: Drag) -> bool {
    match shell.color_picker.as_mut() {
        Some(p) if p.drag == Some(which) => {
            p.drag = None;
            true
        }
        _ => false,
    }
}

pub(super) struct ColorPicker {
    slot: ColorSlot,

    anchor: Rc<Cell<Anchor>>,
    drag: Option<Drag>,

    field: Rc<Cell<Option<Bounds<Pixels>>>>,
    hue: Rc<Cell<Option<Bounds<Pixels>>>>,

    pub(super) hex: TextInput,
}

impl ColorPicker {
    pub(super) fn open(
        slot: ColorSlot,
        at: Point<Pixels>,
        viewport: Size<Pixels>,
        color: ThemeColor,
    ) -> Self {
        let mut hex = TextInput::constrained(Self::filter_hex, 6);
        hex.set_text(hex_digits(color));
        Self {
            slot,
            anchor: Rc::new(Cell::new(Anchor::at_pointer(at, viewport))),
            drag: None,
            field: Rc::new(Cell::new(None)),
            hue: Rc::new(Cell::new(None)),
            hex,
        }
    }

    pub(super) fn slot(&self) -> ColorSlot {
        self.slot
    }

    pub(super) fn sync_hex(&mut self, color: ThemeColor, focused: bool) {
        if focused {
            return;
        }
        let want = hex_digits(color);
        if self.hex.text() != want {
            self.hex.set_text(want);
        }
    }

    pub(super) fn filter_hex(text: &str) -> String {
        text.chars()
            .filter(|c| c.is_ascii_hexdigit())
            .map(|c| c.to_ascii_lowercase())
            .collect()
    }

    pub(super) fn parsed_hex(&self) -> Option<ThemeColor> {
        ThemeColor::from_css_hex(&format!("#{}", self.hex.text()))
    }

    pub(super) fn showing(&self) -> bool {
        place(self.anchor.get()).is_some()
    }

    pub(super) fn anchor_probe(&self) -> impl IntoElement {
        let cell = Rc::clone(&self.anchor);
        div().absolute().inset_0().child(
            canvas(
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

    pub(super) fn cancel_drag(&mut self) -> bool {
        self.drag.take().is_some()
    }

    #[cfg(test)]
    pub(super) fn blocks(&self) -> (Option<Bounds<Pixels>>, Option<Bounds<Pixels>>) {
        (self.field.get(), self.hue.get())
    }

    #[cfg(test)]
    pub(super) fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub(super) fn overlay(
        &self,
        t: ShellTheme,
        color: ThemeColor,
        focus: &FocusHandle,
        host: Entity<Shell>,
    ) -> Placed {
        Placed {
            child: self.plate(t, color, focus, host).into_any_element(),
            anchor: Rc::clone(&self.anchor),
        }
    }

    fn plate(
        &self,
        t: ShellTheme,
        color: ThemeColor,
        focus: &FocusHandle,
        host: Entity<Shell>,
    ) -> Stateful<Div> {
        let slot = self.slot;
        let close = host.clone();
        let reset = host.clone();
        let hex_host = host.clone();
        div()
            .id("color-picker")
            .debug_selector(|| "color-picker".into())
            .w(px(PICKER_W))
            .flex()
            .flex_col()
            .gap(px(GAP))
            .p(px(PAD))
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
                    shell.close_color_picker();
                    cx.notify();
                });
            })
            .child(
                div().h(px(HEAD_H)).flex().items_center().child(
                    div()
                        .text_size(px(11.5))
                        .text_color(t.text_muted)
                        .child(md_i18n::t(slot.label())),
                ),
            )
            .child(
                self.block(&self.field, Drag::Field, FIELD_H, host.clone(), {
                    move |window: &mut Window, b: Bounds<Pixels>| {
                        paint_field(window, b, color.h);
                        let x = f32::from(b.origin.x) + color.s * f32::from(b.size.width);
                        let y = f32::from(b.origin.y) + (1.0 - color.l) * f32::from(b.size.height);
                        paint_marker(window, x, y, color.hsla().into());
                    }
                }),
            )
            .child(self.block(&self.hue, Drag::Hue, HUE_H, host, {
                move |window: &mut Window, b: Bounds<Pixels>| {
                    paint_hue(window, b);
                    let x = f32::from(b.origin.x) + color.h * f32::from(b.size.width);
                    let y = f32::from(b.origin.y) + f32::from(b.size.height) / 2.0;
                    let pure = ThemeColor::new(color.h, 1.0, 0.5, 1.0);
                    paint_marker(window, x, y, pure.hsla().into());
                }
            }))
            .child(
                div()
                    .h(px(FOOT_H))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(self.hex_field(t, focus, hex_host))
                    .child(
                        div()
                            .id("color-picker-reset")
                            .px(px(8.))
                            .py(px(3.))
                            .rounded(px(4.))
                            .border_1()
                            .border_color(t.border_variant)
                            .text_size(px(11.))
                            .text_color(t.text_muted)
                            .hover(move |s| s.bg(t.hover))
                            .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                                reset.update(cx, |shell, cx| {
                                    shell.reset_color(slot, cx);
                                    cx.notify();
                                });
                            })
                            .child(md_i18n::t(md_i18n::Key::PickerReset)),
                    ),
            )
    }

    fn hex_field(&self, t: ShellTheme, focus: &FocusHandle, host: Entity<Shell>) -> Stateful<Div> {
        let keys = host.clone();
        let clicks = host.clone();
        div()
            .id("color-picker-hex")
            .debug_selector(|| "color-picker-hex".into())
            .flex()
            .items_center()
            .gap(px(1.))
            .h(px(FOOT_H))
            .px(px(5.))
            .rounded(px(4.))
            .border_1()
            .border_color(t.border_variant)
            .font_family(MONO_FONT)
            .text_size(px(11.))
            .cursor(CursorStyle::IBeam)
            .track_focus(focus)
            .on_key_down(
                move |ev: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                    keys.update(cx, |shell, cx| shell.hex_key(ev, window, cx));
                },
            )
            .on_mouse_down(
                MouseButton::Left,
                move |ev: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                    let at = ev.position;
                    let count = ev.click_count;
                    let shift = ev.modifiers.shift;
                    clicks.update(cx, |shell, cx| {
                        shell.hex_mouse_down(at, count, shift, window, cx);
                    });
                },
            )
            .child(div().text_color(t.text_disabled).child("#"))
            .child(
                div()
                    .w(px(HEX_W))
                    .h(px(FOOT_H - 6.0))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .child(TextInputElement::new(
                        host,
                        InputStyle {
                            font: Font {
                                family: MONO_FONT.into(),
                                features: FontFeatures::default(),
                                fallbacks: None,
                                weight: FontWeight::NORMAL,
                                style: FontStyle::Normal,
                            },
                            font_size: px(11.),
                            line_height: px(14.),
                            text: t.text,
                            placeholder_color: t.text_disabled,

                            placeholder: "".into(),
                            selection: t.selected_bg,

                            ime: t.selected_bg,
                            caret: t.accent,
                            caret_width: 1.5,
                        },
                    )),
            )
    }

    fn block(
        &self,
        cell: &Rc<Cell<Option<Bounds<Pixels>>>>,
        which: Drag,
        h: f32,
        host: Entity<Shell>,
        paint: impl Fn(&mut Window, Bounds<Pixels>) + 'static,
    ) -> Div {
        let slot = self.slot;
        let cell = Rc::clone(cell);
        div().w_full().h(px(h)).overflow_hidden().child(
            canvas(
                |_, _, _| (),
                move |bounds: Bounds<Pixels>, _, window: &mut Window, _| {
                    cell.set(Some(bounds));
                    paint(window, bounds);
                    let apply = {
                        let host = host.clone();
                        move |pos: Point<Pixels>, cx: &mut App| {
                            host.update(cx, |shell, cx| {
                                shell.recolor(
                                    slot,
                                    |c| match which {
                                        Drag::Field => {
                                            let (s, l) = sl_at(pos, bounds);
                                            ThemeColor { s, l, ..c }
                                        }
                                        Drag::Hue => ThemeColor {
                                            h: hue_at(pos.x, bounds),
                                            ..c
                                        },
                                    },
                                    cx,
                                );
                            });
                        }
                    };
                    window.on_mouse_event({
                        let host = host.clone();
                        let apply = apply.clone();
                        move |ev: &MouseDownEvent, phase: DispatchPhase, _, cx: &mut App| {
                            if phase != DispatchPhase::Bubble
                                || ev.button != MouseButton::Left
                                || !bounds.contains(&ev.position)
                            {
                                return;
                            }
                            host.update(cx, |shell, _| drag_begin(shell, which));
                            apply(ev.position, cx);
                        }
                    });
                    window.on_mouse_event({
                        let host = host.clone();
                        let apply = apply.clone();
                        move |ev: &MouseMoveEvent, phase: DispatchPhase, _, cx: &mut App| {
                            if phase != DispatchPhase::Bubble
                                || !ev.dragging()
                                || !is_dragging(host.read(cx), which)
                            {
                                return;
                            }
                            apply(ev.position, cx);
                        }
                    });
                    window.on_mouse_event({
                        let host = host.clone();
                        move |_: &MouseUpEvent, phase: DispatchPhase, _, cx: &mut App| {
                            if phase != DispatchPhase::Bubble {
                                return;
                            }
                            host.update(cx, |shell, _| {
                                if drag_end(shell, which) {
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
}

#[cfg(test)]
mod tests {
    use super::{ANCHOR_GAP, Anchor, PICKER_H, PICKER_W, hue_at, place, rect, sl_at};
    use crate::shell::VIEW_MARGIN;
    use gpui::{point, px, size};

    fn anchor(y: f32) -> Anchor {
        Anchor {
            row: rect(300.0, y, 600.0, 34.0),
            clip: rect(0.0, 0.0, 900.0, 680.0),
            viewport: size(px(900.), px(680.)),
        }
    }

    #[test]
    fn the_square_maps_onto_saturation_and_lightness() {
        let field = rect(100.0, 200.0, 200.0, 100.0);
        assert_eq!(sl_at(point(px(100.), px(200.)), field), (0.0, 1.0));
        assert_eq!(sl_at(point(px(300.), px(300.)), field), (1.0, 0.0));
        assert_eq!(sl_at(point(px(200.), px(250.)), field), (0.5, 0.5));

        assert_eq!(sl_at(point(px(-999.), px(999.)), field), (0.0, 0.0));
        assert_eq!(sl_at(point(px(999.), px(-999.)), field), (1.0, 1.0));

        let strip = rect(100.0, 400.0, 240.0, 12.0);
        assert_eq!(hue_at(px(100.), strip), 0.0);
        assert_eq!(hue_at(px(340.), strip), 1.0);
        assert_eq!(hue_at(px(220.), strip), 0.5);
        assert_eq!(hue_at(px(-50.), strip), 0.0);
        assert_eq!(hue_at(px(9999.), strip), 1.0);

        let zero = rect(0.0, 0.0, 0.0, 0.0);
        assert_eq!(sl_at(point(px(5.), px(5.)), zero), (1.0, 0.0));
        assert_eq!(hue_at(px(5.), zero), 1.0);
    }

    #[test]
    fn the_popover_opens_below_a_row_unless_it_would_not_fit() {
        let below = |y: f32| px(y + 34.0 + ANCHOR_GAP);
        let at = |y: f32| place(anchor(y)).expect("the row must be visible");
        assert_eq!(at(100.0).y, below(100.0));
        let last_fit = 680.0 - PICKER_H - ANCHOR_GAP - VIEW_MARGIN - 34.0;
        assert_eq!(at(last_fit).y, below(last_fit));

        assert_eq!(at(390.0).y, px(390.0 - ANCHOR_GAP - PICKER_H));
        assert_eq!(at(600.0).y, px(600.0 - ANCHOR_GAP - PICKER_H));

        assert_eq!(at(100.0).x, px(900.0 - PICKER_W - VIEW_MARGIN));
    }

    #[test]
    fn a_window_too_short_for_the_plate_clamps_instead_of_panicking() {
        let squished = Anchor {
            row: rect(300.0, 40.0, 600.0, 34.0),
            clip: rect(0.0, 0.0, 900.0, 120.0),
            viewport: size(px(900.), px(120.)),
        };
        assert_eq!(
            place(squished).expect("must be visible"),
            point(px(660.), px(0.))
        );

        let top = anchor(2.0);
        assert_eq!(
            place(top).expect("must be visible").y,
            px(2.0 + 34.0 + ANCHOR_GAP)
        );
    }

    #[test]
    fn a_row_scrolled_out_of_the_list_stops_anchoring() {
        let clip = rect(0.0, 100.0, 900.0, 500.0);
        let seen = |y: f32| {
            place(Anchor {
                row: rect(300.0, y, 600.0, 34.0),
                clip,
                viewport: size(px(900.), px(680.)),
            })
            .is_some()
        };
        assert!(seen(300.0), "a row in the middle must obviously be visible");
        assert!(seen(80.0), "a half-revealed row still counts as visible");
        assert!(
            !seen(66.0),
            "just scrolled past the top of the clip: it must not paint"
        );
        assert!(!seen(-40.0), "scrolled far past, even less so");
        assert!(
            seen(595.0),
            "a sliver of the bottom edge still counts as visible"
        );
        assert!(
            !seen(600.0),
            "scrolled past the bottom of the clip: it must not paint"
        );
    }
}
