use super::support::{stop_blink, temp_settings_path, test_doc};
use crate::shell::CloseFind;
use crate::shell::Shell;
use crate::shell::color_picker::ColorPicker;
use crate::shell::color_picker::{PICKER_H, PICKER_W};
use gpui::{Entity, EntityInputHandler, MouseButton, MouseDownEvent, Pixels, Point, px};
use gpui::{MouseMoveEvent, MouseUpEvent, point};
use gpui::{TestAppContext, VisualTestContext, size};
use md_theme::ColorGroup;
use md_theme::{ColorSlot, ThemeColor, ThemeVariant};

fn open_picker(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    slot: ColorSlot,
) -> (gpui::Bounds<Pixels>, gpui::Bounds<Pixels>) {
    cx.update(|window, app| {
        let viewport = window.viewport_size();
        shell.update(app, |s, cx| {
            s.show_settings = true;
            s.toggle_color_group(slot.group());
            s.open_color_picker(slot, point(px(320.), px(120.)), viewport);
            cx.notify();
        });
    });
    cx.run_until_parked();
    let (field, hue) = shell
        .read_with(cx, |s, _| s.color_picker.as_ref().map(ColorPicker::blocks))
        .expect("the color picker overlay should be open");
    let field = field.expect("the overlay is open, so the S/L square should have painted a frame");
    let hue = hue.expect("the overlay is open, so the hue bar should have painted a frame");
    assert!(
        f32::from(field.size.width) > 1.0 && f32::from(field.size.height) > 1.0,
        "the square measured zero size:{field:?}"
    );
    assert!(
        f32::from(hue.size.width) > 1.0,
        "the hue bar measured zero width:{hue:?}"
    );

    let outer = cx
        .debug_bounds("color-picker")
        .expect("the overlay should have painted a frame")
        .size;
    assert!(
        (f32::from(outer.width) - PICKER_W).abs() <= 1.0
            && (f32::from(outer.height) - PICKER_H).abs() <= 1.0,
        "the overlay measured {outer:?}, yet the viewport clamp used {PICKER_W}x{PICKER_H}"
    );
    (field, hue)
}

fn press(cx: &mut VisualTestContext, at: Point<Pixels>) {
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.run_until_parked();
}

fn drag_to(cx: &mut VisualTestContext, at: Point<Pixels>) {
    cx.simulate_event(MouseMoveEvent {
        position: at,
        pressed_button: Some(MouseButton::Left),
        modifiers: gpui::Modifiers::default(),
    });
    cx.run_until_parked();
}

fn release(cx: &mut VisualTestContext, at: Point<Pixels>) {
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();
}

fn hover_to(cx: &mut VisualTestContext, at: Point<Pixels>) {
    cx.simulate_event(MouseMoveEvent {
        position: at,
        pressed_button: None,
        modifiers: gpui::Modifiers::default(),
    });
    cx.run_until_parked();
}

#[ignore = "the settings page no longer draws per-item swatches; wait for the built-in editor to reconnect the color picker"]
#[gpui::test]
fn a_swatch_toggles_its_own_popover_and_hands_over_in_one_click(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);

    open_picker(&shell, cx, ColorSlot::Canvas);
    let open_slot = |cx: &mut VisualTestContext| {
        shell.read_with(cx, |s, _| s.color_picker.as_ref().map(ColorPicker::slot))
    };
    let canvas_swatch = cx
        .debug_bounds("swatch:editor.canvas")
        .expect("the swatch in the canvas row should have painted a frame")
        .center();
    let rule_swatch = cx
        .debug_bounds("swatch:editor.rule")
        .expect("the swatch in the rule row should have painted a frame")
        .center();

    hover_to(cx, canvas_swatch);
    press(cx, canvas_swatch);
    assert_eq!(
        open_slot(cx),
        None,
        "clicking the same swatch should put the overlay away"
    );
    release(cx, canvas_swatch);

    press(cx, canvas_swatch);
    assert_eq!(
        open_slot(cx),
        Some(ColorSlot::Canvas),
        "a second click should bring it back"
    );
    release(cx, canvas_swatch);

    hover_to(cx, rule_swatch);
    press(cx, rule_swatch);
    assert_eq!(
        open_slot(cx),
        Some(ColorSlot::Rule),
        "clicking another swatch should switch over in one click"
    );
    release(cx, rule_swatch);
}

#[gpui::test]
fn dragging_the_color_square_recolors_just_that_slot(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    let (field, hue) = open_picker(&shell, cx, ColorSlot::Canvas);

    let canvas_now = |cx: &mut VisualTestContext| {
        shell.read_with(cx, |s, app| {
            (
                s.settings.appearance.colors().get(ColorSlot::Canvas),
                s.editor.read(app).state.theme.paint.canvas,
            )
        })
    };
    assert_eq!(
        canvas_now(cx).0,
        None,
        "nothing has been touched yet, so there should be no override"
    );

    press(cx, field.origin + point(px(1.), px(1.)));
    let (over, painted) = canvas_now(cx);
    let over = over.expect("a single press should record an override");
    assert!(
        over.l > 0.98 && over.s < 0.02,
        "the top-left corner is not white:{over:?}"
    );
    assert_eq!(
        painted, over,
        "the changed color should carry through to the editor's token"
    );
    assert!(
        shell.read_with(cx, |s, _| s
            .color_picker
            .as_ref()
            .is_some_and(ColorPicker::dragging)),
        "pressing should enter the dragging state"
    );
    assert!(
        shell.read_with(cx, |s, app| s.editor.read(app).state.incremental.is_some()),
        "the recolor dropped the incremental engine: without it, every drag across the plate would reflow the whole document"
    );

    drag_to(cx, field.bottom_right() - point(px(1.), px(1.)));
    let (over, painted) = canvas_now(cx);
    let over = over.expect("an override should exist mid-drag");
    assert!(
        over.l < 0.02,
        "the bottom-right corner is not black:{over:?}"
    );
    assert_eq!(painted, over);

    release(cx, field.bottom_right());
    assert!(
        !shell.read_with(cx, |s, _| s
            .color_picker
            .as_ref()
            .is_some_and(ColorPicker::dragging)),
        "the drag state was not cleared on release; any later mouse move would then recolor"
    );
    let after_release = canvas_now(cx).0;
    drag_to(cx, field.center());
    assert_eq!(
        canvas_now(cx).0,
        after_release,
        "moving after the release should not recolor"
    );

    press(cx, field.center());
    release(cx, field.center());
    let mid = canvas_now(cx)
        .0
        .expect("pressing the square's middle should record an override");
    press(cx, point(hue.left() + hue.size.width / 3., hue.center().y));
    let turned = canvas_now(cx)
        .0
        .expect("pressing the hue bar should record an override");
    assert!(
        (turned.h - 1.0 / 3.0).abs() < 0.02,
        "the hue did not land on 1/3:{turned:?}"
    );
    assert_eq!(
        (turned.s, turned.l),
        (mid.s, mid.l),
        "the hue-bar drag changed saturation or lightness as well"
    );
    release(cx, hue.center());

    assert_eq!(
        shell.read_with(cx, |s, _| s.settings.appearance.colors().len()),
        1
    );

    let base = md_theme::DocumentTheme::one_dark().paint.canvas;
    cx.update(|_, app| {
        shell.update(app, |s, cx| s.reset_color(ColorSlot::Canvas, cx));
    });
    let (over, painted) = canvas_now(cx);
    assert_eq!(over, None);
    assert_eq!(
        painted, base,
        "after removing the override, the canvas did not return to the stock theme's value"
    );
}

#[gpui::test]
fn collapsing_a_group_puts_away_the_picker_inside_it(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    open_picker(&shell, cx, ColorSlot::Canvas);
    let editing = |cx: &mut VisualTestContext| {
        shell.read_with(cx, |s, _| s.color_picker.as_ref().map(ColorPicker::slot))
    };
    assert_eq!(editing(cx), Some(ColorSlot::Canvas));

    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.toggle_color_group(ColorGroup::Syntax);
            cx.notify();
        });
    });
    assert_eq!(editing(cx), Some(ColorSlot::Canvas));

    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.toggle_color_group(ColorGroup::Surface);
            cx.notify();
        });
    });
    assert_eq!(
        editing(cx),
        None,
        "collapsing the group should put away the picker inside it"
    );

    cx.update(|window, app| {
        let viewport = window.viewport_size();
        shell.update(app, |s, _| {
            s.open_color_picker(ColorSlot::Body, point(px(320.), px(120.)), viewport);
        });
    });
    assert_eq!(editing(cx), Some(ColorSlot::Body));
    cx.update(|window, app| {
        let viewport = window.viewport_size();
        shell.update(app, |s, _| {
            s.open_color_picker(ColorSlot::Body, point(px(320.), px(120.)), viewport);
        });
    });
    assert_eq!(editing(cx), None);
}

#[gpui::test]
fn pressing_outside_the_popover_puts_it_away(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    let (field, _) = open_picker(&shell, cx, ColorSlot::Canvas);
    let open_slot = |cx: &mut VisualTestContext| {
        shell.read_with(cx, |s, _| s.color_picker.as_ref().map(ColorPicker::slot))
    };

    press(cx, field.center());
    release(cx, field.center());
    assert_eq!(
        open_slot(cx),
        Some(ColorSlot::Canvas),
        "pressing inside the overlay should not close it"
    );

    let y = cx.update(|window, _| window.viewport_size().height) / 2.;
    press(cx, point(px(40.), y));
    assert_eq!(
        open_slot(cx),
        None,
        "pressing outside the overlay should put it away"
    );
}

fn hex_state(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (String, Option<ThemeColor>) {
    shell.read_with(cx, |s, _| {
        (
            s.color_picker
                .as_ref()
                .map(|p| p.hex.text().to_string())
                .expect("the overlay should be open"),
            s.settings.appearance.colors().get(ColorSlot::Canvas),
        )
    })
}

fn focus_hex(shell: &Entity<Shell>, cx: &mut VisualTestContext) {
    let field = cx
        .debug_bounds("color-picker-hex")
        .expect("the overlay is open, so the hex field should have painted a frame");
    hover_to(cx, field.center());
    press(cx, field.center());
    release(cx, field.center());

    assert!(
        shell.read_with(cx, |s, _| s.color_picker.is_some()),
        "clicking the hex field closed the picker overlay"
    );
    assert!(
        cx.update(|window, app| shell.read(app).hex_focus.is_focused(window)),
        "clicking the hex field did not give it focus"
    );
}

#[gpui::test]
fn typing_six_hex_digits_recolors_and_a_partial_one_does_not(cx: &mut TestAppContext) {
    let path = temp_settings_path("hex-typing");
    let store = crate::store::settings::SettingsStore::at(path.clone());
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.adopt_settings_store(
                Some(crate::store::settings::SettingsStore::at(path.clone())),
                cx,
            );
        });
    });
    open_picker(&shell, cx, ColorSlot::Canvas);

    let base = md_theme::DocumentTheme::one_dark().paint.canvas;
    assert_eq!(
        hex_state(&shell, cx).0,
        base.to_css_hex().trim_start_matches('#'),
        "a freshly opened field should show this item's current hex"
    );
    focus_hex(&shell, cx);

    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.color_picker
                .as_mut()
                .expect("the picker should be open")
                .hex
                .select_all();
            cx.notify();
        });
    });
    cx.simulate_input("3a7");
    cx.run_until_parked();
    let (text, over) = hex_state(&shell, cx);
    assert_eq!(text, "3a7");
    assert_eq!(
        over, None,
        "typing just 3 digits recolored: a partial hex is not a color"
    );

    cx.simulate_input("f2c");
    cx.run_until_parked();
    let (text, over) = hex_state(&shell, cx);
    assert_eq!(text, "3a7f2c");
    let want = ThemeColor::from_css_hex("#3a7f2c").expect("the 6-digit hex should parse");
    let over = over.expect("six digits should apply the color");
    assert_eq!(over.to_css_hex(), want.to_css_hex());

    assert_eq!(
        shell
            .read_with(cx, |s, app| s.editor.read(app).state.theme.paint.canvas)
            .to_css_hex(),
        want.to_css_hex()
    );
    assert_eq!(
        store
            .load()
            .expect("once typed in full it should already be on disk")
            .appearance
            .colors()
            .get(ColorSlot::Canvas)
            .map(ThemeColor::to_css_hex),
        Some(want.to_css_hex()),
        "a completed hex should write the file on the spot — unlike dragging the square, which streams through over a hundred color values"
    );

    cx.simulate_input("9");
    cx.run_until_parked();
    assert_eq!(
        hex_state(&shell, cx).0,
        "3a7f2c",
        "the field should accept only 6 digits"
    );

    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[ignore = "the settings page no longer draws per-item swatches; wait for the built-in editor to reconnect the color picker"]
#[gpui::test]
fn pressing_inside_the_popover_never_reaches_the_rows_behind(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));

    cx.simulate_resize(size(px(900.), px(680.)));
    cx.run_until_parked();
    stop_blink(&shell, cx);
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.show_settings = true;
            for g in ColorGroup::ALL {
                if !s.color_groups_open[g.index()] {
                    s.toggle_color_group(g);
                }
            }
            cx.notify();
        });
    });
    cx.run_until_parked();

    let swatch = cx
        .debug_bounds("swatch:editor.body")
        .expect("the swatch should have painted a frame")
        .center();
    hover_to(cx, swatch);
    press(cx, swatch);
    release(cx, swatch);
    assert_eq!(
        shell.read_with(cx, |s, _| s.color_picker.as_ref().map(ColorPicker::slot)),
        Some(ColorSlot::Body),
        "precondition: clicking the swatch should open the overlay"
    );
    let plate = cx
        .debug_bounds("color-picker")
        .expect("the overlay should have painted a frame");
    let field = cx
        .debug_bounds("color-picker-hex")
        .expect("the hex field should have painted a frame");

    let groups_before = shell.read_with(cx, |s, _| s.color_groups_open);
    hover_to(cx, field.center());
    press(cx, field.center());
    release(cx, field.center());
    assert!(
        shell.read_with(cx, |s, _| s.color_picker.is_some()),
        "clicking the hex field put the picker overlay away: the plate did not block the row behind from the mouse"
    );
    assert_eq!(
        shell.read_with(cx, |s, _| s.color_groups_open),
        groups_before,
        "a click on the plate toggled the group behind it: the mouse-up leaked into the header's on_click"
    );

    for at in [
        plate.center(),
        plate.origin + point(px(4.), px(4.)),
        point(plate.center().x, plate.bottom() - px(4.)),
    ] {
        hover_to(cx, at);
        press(cx, at);
        release(cx, at);
        assert!(
            shell.read_with(cx, |s, _| s.color_picker.is_some()),
            "clicking inside the plate at {at:?} put the overlay away"
        );
        assert_eq!(
            shell.read_with(cx, |s, _| s.color_groups_open),
            groups_before,
            "clicking inside the plate at {at:?} toggled the group behind it"
        );
    }
}

#[gpui::test]
fn pasting_into_the_hex_field_still_filters(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    open_picker(&shell, cx, ColorSlot::Canvas);
    focus_hex(&shell, cx);

    cx.update(|_, app| {
        app.write_to_clipboard(gpui::ClipboardItem::new_string(
            "#28C2Ff not a colour".into(),
        ));
    });
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.color_picker
                .as_mut()
                .expect("the picker should be open")
                .hex
                .select_all();
            cx.notify();
        });
    });
    cx.simulate_keystrokes("secondary-v");
    cx.run_until_parked();
    let text = hex_state(&shell, cx).0;
    assert!(
        text.chars().all(|c| c.is_ascii_hexdigit()) && text.len() <= 6,
        "pasted content was not filtered:{text:?}"
    );
    assert_eq!(
        text, "28c2ff",
        "it should strip #, lowercase it, and clip to 6 digits"
    );
    assert_eq!(
        hex_state(&shell, cx).1.map(ThemeColor::to_css_hex),
        Some("#28c2ff".to_string()),
        "six digits should apply the color"
    );
}

#[gpui::test]
fn the_hex_field_only_takes_hex_digits(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    open_picker(&shell, cx, ColorSlot::Canvas);
    focus_hex(&shell, cx);
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.color_picker
                .as_mut()
                .expect("the picker should be open")
                .hex
                .select_all();
            cx.notify();
        });
    });

    cx.simulate_input("zz1");
    cx.run_until_parked();
    assert_eq!(hex_state(&shell, cx).0, "1", "z is not a hex digit");

    cx.update(|window, app| {
        shell.update(app, |s, cx| {
            s.color_picker
                .as_mut()
                .expect("the picker should be open")
                .hex
                .select_all();
            s.replace_text_in_range(None, "#28C2Ff00", window, cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(
        hex_state(&shell, cx).0,
        "28c2ff",
        "pasted text should strip #, lowercase it, and clip to 6 digits"
    );
}

#[gpui::test]
fn the_hex_text_follows_the_colour_only_while_unfocused(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    let (field, _) = open_picker(&shell, cx, ColorSlot::Canvas);
    let before = hex_state(&shell, cx).0;

    press(cx, field.origin + point(px(1.), px(1.)));
    release(cx, field.origin + point(px(1.), px(1.)));
    let (text, over) = hex_state(&shell, cx);
    let over = over.expect("a single drag should leave an override");
    assert_ne!(
        text, before,
        "the hex text should have followed the square drag"
    );
    assert_eq!(
        text,
        over.to_css_hex().trim_start_matches('#'),
        "the field should show exactly the current color"
    );

    focus_hex(&shell, cx);
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.color_picker
                .as_mut()
                .expect("the picker should be open")
                .hex
                .select_all();
            cx.notify();
        });
    });
    cx.simulate_input("ab");
    cx.run_until_parked();
    assert_eq!(hex_state(&shell, cx).0, "ab");

    cx.update(|_, app| shell.update(app, |_, cx| cx.notify()));
    cx.run_until_parked();
    assert_eq!(
        hex_state(&shell, cx).0,
        "ab",
        "the partial hex under focus was clobbered by the current-color writeback: the field would not accept typed input"
    );

    cx.update(|_, app| {
        app.bind_keys([gpui::KeyBinding::new("escape", CloseFind, None)]);
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(
        hex_state(&shell, cx).0,
        over.to_css_hex().trim_start_matches('#'),
        "Esc should restore the partial hex to this item's current color"
    );
    assert!(
        !cx.update(|window, app| shell.read(app).hex_focus.is_focused(window)),
        "after Esc, focus should return, or actions like Ctrl+F would not work"
    );
}

#[ignore = "the settings page no longer draws per-item swatches; wait for the built-in editor to reconnect the color picker"]
#[gpui::test]
fn the_popover_follows_its_swatch_as_the_list_scrolls(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    open_picker(&shell, cx, ColorSlot::Canvas);

    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            for g in ColorGroup::ALL {
                if !s.color_groups_open[g.index()] {
                    s.toggle_color_group(g);
                }
            }
            cx.notify();
        });
    });
    cx.run_until_parked();

    let measure = |cx: &mut VisualTestContext| {
        let swatch = cx
            .debug_bounds("swatch:editor.canvas")
            .expect("the swatch should have painted a frame");
        (swatch, cx.debug_bounds("color-picker"))
    };
    let scroll_by = |cx: &mut VisualTestContext, dy: f32| {
        cx.update(|_, app| {
            shell.update(app, |s, cx| {
                let at = s.settings_scroll.offset();
                s.settings_scroll.set_offset(point(at.x, at.y - px(dy)));
                cx.notify();
            });
        });
        cx.run_until_parked();
    };

    let band = shell.read_with(cx, |s, _| s.settings_scroll.bounds());
    let mid = f32::from(band.top()) + f32::from(band.size.height) / 2.0;
    let (before, _) = measure(cx);
    scroll_by(cx, f32::from(before.top()) - mid);

    let (swatch, popover) = measure(cx);
    let popover = popover.expect("scrolled to the column middle, the overlay should be painted");

    assert!(
        (f32::from(popover.right()) - f32::from(swatch.right())).abs() <= 1.0,
        "the overlay's right edge is not aligned with the swatch's:{popover:?} / {swatch:?}"
    );
    let gap = f32::from(popover.top()) - f32::from(swatch.bottom());
    assert!(
        (0.0..=20.0).contains(&gap),
        "the overlay did not open just below the row:{gap}"
    );

    scroll_by(cx, 80.0);
    let (rolled, popover2) = measure(cx);
    let popover2 =
        popover2.expect("the swatch is still visible, so the overlay should still be there");
    let moved = f32::from(swatch.top()) - f32::from(rolled.top());
    assert!(moved > 60.0, "the column did not scroll:{moved}");
    assert!(
        (f32::from(popover2.top()) - f32::from(rolled.bottom()) - gap).abs() <= 1.0,
        "the overlay did not follow the swatch: swatch {rolled:?}, overlay {popover2:?}"
    );

    let at = shell.read_with(cx, |s, _| s.settings_scroll.offset());
    scroll_by(cx, 10_000.0);
    let (gone, _) = measure(cx);
    assert!(
        f32::from(gone.bottom()) <= f32::from(band.top()),
        "content is not tall enough: the row has not scrolled out of the visible range:{gone:?} / {band:?}"
    );
    assert_eq!(
        shell.read_with(cx, |s, _| s.color_picker.as_ref().map(ColorPicker::showing)),
        Some(false),
        "the swatch scrolled out of view, yet the overlay is still painted"
    );

    assert_eq!(
        shell.read_with(cx, |s, _| s.color_picker.as_ref().map(ColorPicker::slot)),
        Some(ColorSlot::Canvas)
    );
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.settings_scroll.set_offset(at);
            cx.notify();
        });
    });
    cx.run_until_parked();
    let (back, popover5) = measure(cx);
    let popover5 =
        popover5.expect("after scrolling back, the overlay should still be on the swatch");
    assert_eq!(
        shell.read_with(cx, |s, _| s.color_picker.as_ref().map(ColorPicker::showing)),
        Some(true)
    );
    assert!(
        (f32::from(popover5.top()) - f32::from(back.bottom()) - gap).abs() <= 1.0,
        "after scrolling back the overlay did not stick to the row: swatch {back:?}, overlay {popover5:?}"
    );
}

#[gpui::test]
fn the_new_color_reaches_the_file_only_on_mouse_up(cx: &mut TestAppContext) {
    let path = temp_settings_path("color");
    let store = crate::store::settings::SettingsStore::at(path.clone());
    store
        .save(&crate::store::settings::Settings::default())
        .expect("write an initial settings file first");

    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.adopt_settings_store(
                Some(crate::store::settings::SettingsStore::at(path.clone())),
                cx,
            );
        });
    });
    let (field, _) = open_picker(&shell, cx, ColorSlot::Caret);

    press(cx, field.origin + point(px(1.), px(1.)));
    drag_to(cx, field.center());
    assert!(
        store
            .load()
            .expect("the settings file should still be there")
            .appearance
            .colors()
            .is_empty(),
        "it persisted mid-drag: one drag would write the file over a hundred times"
    );

    release(cx, field.center());
    let on_disk = store
        .load()
        .expect("after the release it should already be on disk");
    let want = shell.read_with(cx, |s, _| s.settings.clone());

    let hexes = |s: &crate::store::settings::Settings| {
        s.appearance
            .colors()
            .iter()
            .map(|(slot, c)| (slot.key(), c.to_css_hex()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        hexes(&on_disk),
        hexes(&want),
        "the colors on disk do not match the ones in memory"
    );
    assert!(on_disk.appearance.colors().get(ColorSlot::Caret).is_some());

    assert!(
        on_disk.appearance.colors[ThemeVariant::OneLight.index()].is_empty(),
        "the caret color edited under the dark theme leaked into the light one"
    );

    cx.update(|_, app| {
        shell.update(app, |s, cx| s.reset_all_colors(cx));
    });
    let on_disk = store
        .load()
        .expect("the revert should also be written to disk");
    assert!(on_disk.appearance.colors().is_empty());
    assert_eq!(
        shell.read_with(cx, |s, _| s.color_picker.as_ref().map(ColorPicker::slot)),
        None,
        "after restoring all defaults, the picker overlay should be put away"
    );

    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}
