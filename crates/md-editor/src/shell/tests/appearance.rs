use super::support::{stop_blink, test_doc};
use crate::shell::Shell;
use crate::shell::settings_state::{font_size_at, font_size_fraction};
use gpui::TestAppContext;
use gpui::{MouseButton, MouseDownEvent, Pixels, px};
use gpui::{MouseMoveEvent, MouseUpEvent, point};
use md_theme::DocumentTheme;
use md_theme::{Appearance, BodyFamily, Density};

#[gpui::test]
fn shell_starts_dark_with_outline_closed(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    let (dark, outline_open, settings_nav, settings, theme) = cx.update(|_, app| {
        let state = shell.read(app);
        (
            state.is_dark(),
            state.outline_open,
            state.settings_nav,
            state.settings.clone(),
            state.editor.read(app).state.theme,
        )
    });

    assert!(dark);
    assert!(!outline_open);
    assert_eq!(settings_nav, 0);
    assert_eq!(theme, DocumentTheme::one_dark());

    assert_eq!(settings, crate::store::settings::Settings::default());
    assert!(shell.read_with(cx, |s, _| s.settings_store.is_none()));
}

#[gpui::test]
fn the_appearance_rows_reach_the_document_theme(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);

    let theme = |cx: &mut gpui::VisualTestContext| {
        shell.read_with(cx, |s, app| s.editor.read(app).state.theme)
    };

    cx.update(|_, app| {
        shell.update(app, |s, cx| s.toggle_variant(cx));
    });
    assert_eq!(theme(cx), DocumentTheme::one_light());
    assert!(!shell.read_with(cx, |s, _| s.is_dark()));

    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.settings.appearance.body_family = BodyFamily::Serif;
            s.settings.appearance.density = Density::Compact;
            s.settings.appearance = s.settings.appearance.with_body_size_px(20.0);
            s.appearance_changed(cx);
        });
    });
    let t = theme(cx);
    assert_eq!(t.type_scale.body.family, md_theme::SYSTEM_SERIF);
    assert_eq!(t.type_scale.code.family, md_theme::SYSTEM_MONO);
    assert_eq!(t.type_scale.body.size_px, 20.0);
    assert_eq!(t.edge_scale, Density::Compact.edge_scale());
}

#[test]
fn the_font_slider_maps_the_track_onto_the_size_range() {
    let track = gpui::Bounds {
        origin: point(px(100.), px(0.)),
        size: gpui::size(px(150.), px(12.)),
    };
    assert_eq!(font_size_at(px(0.), track), Appearance::MIN_BODY_SIZE_PX);
    assert_eq!(font_size_at(px(9999.), track), Appearance::MAX_BODY_SIZE_PX);
    assert_eq!(font_size_at(px(100.), track), Appearance::MIN_BODY_SIZE_PX);
    assert_eq!(font_size_at(px(250.), track), Appearance::MAX_BODY_SIZE_PX);
    assert_eq!(font_size_at(px(175.), track), 18.0);

    assert_eq!(font_size_fraction(Appearance::MIN_BODY_SIZE_PX), 0.0);
    assert_eq!(font_size_fraction(Appearance::MAX_BODY_SIZE_PX), 1.0);
    assert_eq!(
        font_size_fraction(99.0),
        1.0,
        "out-of-range values should stay on the track"
    );

    assert!((font_size_fraction(18.0) - 0.5).abs() < f32::EPSILON);
}

#[test]
fn a_zero_width_track_does_not_divide_by_zero() {
    let track = gpui::Bounds {
        origin: point(px(0.), px(0.)),
        size: gpui::size(px(0.), px(0.)),
    };
    assert_eq!(font_size_at(px(40.), track), Appearance::MIN_BODY_SIZE_PX);
}

#[gpui::test]
fn dragging_the_font_slider_moves_the_body_size(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.update(|_, app| {
        shell.update(app, |s, cx| {
            s.show_settings = true;
            cx.notify();
        });
    });
    cx.run_until_parked();

    let track = shell
        .read_with(cx, |s, _| s.font_track.get())
        .expect("with the settings open, the font slider should have painted a frame");
    assert!(
        f32::from(track.size.width) > 0.0,
        "the track measured zero width: {track:?}"
    );

    let down = |cx: &mut gpui::VisualTestContext, x: Pixels| {
        cx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position: point(x, track.center().y),
            modifiers: gpui::Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        });
        cx.run_until_parked();
    };
    let size = |cx: &mut gpui::VisualTestContext| {
        shell.read_with(cx, |s, _| s.settings.appearance.body_size_px)
    };

    down(cx, track.right() - px(1.0));
    assert_eq!(size(cx), Appearance::MAX_BODY_SIZE_PX);
    assert!(
        shell.read_with(cx, |s, _| s.font_slider_drag),
        "a press should enter the drag state"
    );
    assert_eq!(
        shell.read_with(cx, |s, app| s
            .editor
            .read(app)
            .state
            .theme
            .type_scale
            .body
            .size_px),
        Appearance::MAX_BODY_SIZE_PX,
        "the font size should reach the body tokens"
    );

    cx.simulate_event(MouseMoveEvent {
        position: point(track.left(), track.center().y),
        pressed_button: Some(MouseButton::Left),
        modifiers: gpui::Modifiers::default(),
    });
    cx.run_until_parked();
    assert_eq!(size(cx), Appearance::MIN_BODY_SIZE_PX);

    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: point(track.left(), track.center().y),
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();
    assert!(
        !shell.read_with(cx, |s, _| s.font_slider_drag),
        "the release did not clear the drag state; any later mouse move would change the font size"
    );

    cx.simulate_event(MouseMoveEvent {
        position: point(track.right() - px(1.0), track.center().y),
        pressed_button: Some(MouseButton::Left),
        modifiers: gpui::Modifiers::default(),
    });
    cx.run_until_parked();
    assert_eq!(size(cx), Appearance::MIN_BODY_SIZE_PX);
}
