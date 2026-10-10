use super::support::{stop_blink, test_doc};
use crate::shell::Shell;
use gpui::{MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point, TestAppContext};

fn click(cx: &mut gpui::VisualTestContext, at: Point<Pixels>) {
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();
}

fn reading(shell: &gpui::Entity<Shell>, cx: &mut gpui::VisualTestContext) -> bool {
    shell.read_with(cx, |shell, _| shell.reading)
}

#[gpui::test]
fn the_reading_button_leaves_the_editor_focused(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.run_until_parked();

    let focus = cx.update(|_, app| shell.read(app).editor_focus().clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    assert!(
        cx.update(|window, _| focus.is_focused(window)),
        "the editor should hold the keyboard to begin with"
    );

    let at = cx
        .debug_bounds("btn-reading")
        .expect("the status bar has painted, so the reading button should be there")
        .center();
    click(cx, at);

    assert!(reading(&shell, cx), "the click should turn reading mode on");
    assert!(
        cx.update(|window, _| focus.is_focused(window)),
        "toggling reading mode should not take the keyboard away from the editor"
    );

    click(cx, at);

    assert!(
        !reading(&shell, cx),
        "the second click should turn it back off"
    );
    assert!(
        cx.update(|window, _| focus.is_focused(window)),
        "leaving reading mode should not take the keyboard away either"
    );
}

#[gpui::test]
fn the_outline_toggle_sits_before_the_reading_toggle(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    cx.run_until_parked();

    let outline = cx
        .debug_bounds("btn-outline")
        .expect("the status bar should paint the outline toggle")
        .center();
    let reading = cx
        .debug_bounds("btn-reading")
        .expect("the status bar should paint the reading toggle")
        .center();
    assert!(
        f32::from(outline.x) < f32::from(reading.x),
        "the outline toggle should come first: outline at {}, reading at {}",
        f32::from(outline.x),
        f32::from(reading.x)
    );
}
