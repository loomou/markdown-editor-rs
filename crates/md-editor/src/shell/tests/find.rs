use super::support::stop_blink;
use crate::keymap::Cmd;
use crate::shell::Shell;
use gpui::{TestAppContext, VisualTestContext};
use md_core::doc::Doc;
use md_core::document::{editor_options, load_markdown};
use std::time::Duration;

fn settle_find(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
}

#[gpui::test]
fn find_next_in_the_shell_steps_the_open_bar(cx: &mut TestAppContext) {
    let (shell, cx) = cx.add_window_view(|_, cx| {
        Shell::new(
            Doc::new(load_markdown("needle needle needle\n", editor_options())),
            cx,
        )
    });
    stop_blink(&shell, cx);

    cx.update(|window, app| {
        shell.update(app, |s, cx| {
            s.run_command(Cmd::FindNext, window, cx);
        });
    });
    cx.run_until_parked();
    assert!(
        cx.update(|_, app| shell.read(app).find.read(app).open),
        "pressing next while the find bar is closed should open it first"
    );

    cx.simulate_input("needle");
    cx.run_until_parked();
    settle_find(cx);
    let active = |cx: &mut VisualTestContext| {
        cx.update(|_, app| shell.read(app).editor.read(app).search.active)
    };
    assert_eq!(
        active(cx),
        Some(0),
        "precondition: the seeded typing should stop at the first match"
    );

    cx.update(|window, app| {
        shell.update(app, |s, cx| {
            s.run_command(Cmd::FindNext, window, cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(
        active(cx),
        Some(1),
        "with the bar open, \"next\" should step to the second match"
    );
    let (query, selection_empty) = cx.update(|_, app| {
        let find = shell.read(app).find.read(app);
        (find.query().to_string(), find.selection().is_empty())
    });
    assert_eq!(
        query, "needle",
        "the query was replaced by the selection seed; this looks like the find bar was reopened"
    );
    assert!(
        selection_empty,
        "the query field got select-all — as if the find bar had been reopened"
    );

    cx.update(|window, app| {
        shell.update(app, |s, cx| {
            s.run_command(Cmd::FindPrev, window, cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(
        active(cx),
        Some(0),
        "\"previous\" should walk back to the first match"
    );
}
