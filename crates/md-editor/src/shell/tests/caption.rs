use super::support::{stop_blink, test_doc};
use crate::shell::Shell;
use gpui::TestAppContext;
use gpui::{MouseButton, MouseDownEvent, MouseUpEvent};

fn grab_point(cx: &mut gpui::VisualTestContext) -> gpui::Point<gpui::Pixels> {
    let fill = cx
        .debug_bounds("drag-fill")
        .expect("the title bar has painted a frame, so the drag-fill hot zone should be present");
    assert!(
        f32::from(fill.size.width) > 0.0,
        "the hot zone measured zero width, so a press would register nowhere: {fill:?}"
    );
    fill.center()
}

fn press(cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>, clicks: usize) {
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: gpui::Modifiers::default(),
        click_count: clicks,
        first_mouse: false,
    });
    cx.run_until_parked();
}

fn armed(shell: &gpui::Entity<Shell>, cx: &mut gpui::VisualTestContext) -> bool {
    shell.read_with(cx, |s, _| s.caption_should_move)
}

#[gpui::test]
fn pressing_the_caption_arms_the_drag_and_releasing_disarms_it(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    let at = grab_point(cx);

    assert!(
        !armed(&shell, cx),
        "having done nothing, it should be in the undo state"
    );

    press(cx, at, 1);
    assert!(
        armed(&shell, cx),
        "after pressing on the hot zone the move loop should be armed"
    );

    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();
    assert!(
        !armed(&shell, cx),
        "nothing was done yet, so the drag should be disarmed"
    );
}

#[cfg(windows)]
#[gpui::test]
fn moving_after_a_press_consumes_the_armed_flag_once(cx: &mut TestAppContext) {
    use gpui::{MouseMoveEvent, point};

    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    let at = grab_point(cx);

    press(cx, at, 1);
    assert!(armed(&shell, cx));

    let drag_to = |cx: &mut gpui::VisualTestContext, dx: f32| {
        cx.simulate_event(MouseMoveEvent {
            position: point(at.x + gpui::px(dx), at.y),
            pressed_button: Some(MouseButton::Left),
            modifiers: gpui::Modifiers::default(),
        });
        cx.run_until_parked();
    };

    drag_to(cx, 8.0);
    assert!(!armed(&shell, cx), "the first move should consume the flag");

    drag_to(cx, 16.0);
    assert!(
        !armed(&shell, cx),
        "one press should enter the move loop only once"
    );
}

#[cfg(windows)]
#[gpui::test]
fn double_clicking_the_caption_does_not_arm_the_drag(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| Shell::new(test_doc(), cx));
    stop_blink(&shell, cx);
    let at = grab_point(cx);

    press(cx, at, 2);
    assert!(
        !armed(&shell, cx),
        "the release did not disarm, so any later mouse move would drag the window along"
    );
}
